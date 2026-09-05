//! Object-graph → `pg_snapshot::Snapshot` extraction, split by snapshot section (`project`, `features`, `phonology`, `morphology`, `lexicon`), sharing one `Ctx`; most guid fields pass through unresolved (validated later), and this crate only dereferences a guid where the snapshot embeds the target's own data inline, warning and skipping rather than panicking when the target is missing.

mod features;
mod inventory;
mod lexicon;
mod morphology;
mod phonology;
mod project;

pub(crate) mod codes;

pub(crate) use inventory::tracked_kind;

use pg_snapshot::{
    ConversionProvenance, InventoryKey, IssueClass, Snapshot, SourceInventoryStatus, SourceRef,
    Warning, CONVERSION_PROVENANCE_SCHEMA_VERSION,
};

use crate::{
    xml::{RawGraph, Record},
    ImportError,
};

use inventory::SelectionRecorder;

/// Shared extraction context: the raw object graph, accumulating warnings, and writing-system priority lists that only become known once the `project` section has been read.
pub struct Ctx<'a> {
    pub graph: &'a RawGraph,
    /// Semantic, never-fatal tolerances the extractor noticed; see `RawGraph.issues` for structural, fatal-capable graph problems.
    pub warnings: Vec<Warning>,
    /// Analysis writing systems, default first — `project.analysisWritingSystems`.
    pub analysis_ws: Vec<String>,
    /// Vernacular writing systems, default first — `project.vernacularWritingSystems`.
    pub vernacular_ws: Vec<String>,
    pub(crate) recorder: SelectionRecorder,
}

impl<'a> Ctx<'a> {
    fn new(graph: &'a RawGraph) -> Self {
        let mut recorder = SelectionRecorder::default();
        recorder.seed_authored_from_graph(graph);
        Ctx {
            graph,
            warnings: Vec::new(),
            analysis_ws: Vec::new(),
            vernacular_ws: Vec::new(),
            recorder,
        }
    }

    /// Record a warning: `code` is a stable short identifier naming the situation (see the `codes` module); `msg` is the human-readable prose.
    pub fn warn(&mut self, code: &'static str, msg: impl Into<String>) {
        self.warnings.push(Warning::new(code, msg));
    }

    pub fn get(&self, guid: &str) -> Option<&'a Record> {
        self.graph.get(guid)
    }

    /// Records an attachment/expansion/setting `key` as sourced (object identities are seeded once in `Ctx::new` instead).
    pub(crate) fn authored(&mut self, key: InventoryKey) {
        self.recorder.authored(key);
    }

    pub(crate) fn considered(&mut self, key: InventoryKey) {
        self.recorder.considered(key);
    }

    pub(crate) fn selected(&mut self, key: InventoryKey) {
        self.recorder.selected(key);
    }

    pub(crate) fn represented(&mut self, key: InventoryKey) {
        self.recorder.represented(key);
    }

    pub(crate) fn synthesized(&mut self, key: InventoryKey) {
        self.recorder.synthesized(key);
    }

    pub(crate) fn is_represented(&self, key: &InventoryKey) -> bool {
        self.recorder.is_represented(key)
    }

    /// Records `key` rejected and emits the SAME warning a caller would otherwise have emitted alone, so converting a warn-only site to also reject never changes warning prose or counts.
    pub(crate) fn reject(
        &mut self,
        key: InventoryKey,
        code: &'static str,
        class: IssueClass,
        fatal: bool,
        source: Option<SourceRef>,
        msg: impl Into<String>,
    ) {
        let msg = msg.into();
        self.warn(code, msg.clone());
        self.recorder.rejected(
            key,
            pg_snapshot::ConversionIssue {
                code: code.to_string(),
                class,
                source,
                fatal,
                message: msg,
            },
        );
    }

    /// Records `key` rejected with no new warning, for a failure a caller has already warned about through another path (e.g. `Ctx::require` or `parser_params`'s own issues).
    pub(crate) fn record_rejected(&mut self, key: InventoryKey, issue: pg_snapshot::ConversionIssue) {
        self.recorder.rejected(key, issue);
    }

    /// Resolve `guid` expecting a specific class; warns and returns `None` if it is dangling or resolves to a surprising class.
    pub fn require(&mut self, guid: &str, want_class: &str, context: &str) -> Option<&'a Record> {
        match self.get(guid) {
            Some(r) if r.class == want_class => Some(r),
            Some(r) => {
                self.warn(
                    codes::UNEXPECTED_CLASS,
                    format!("{context}: expected {want_class} but {guid} is {}", r.class),
                );
                None
            }
            None => {
                self.warn(
                    codes::DANGLING_REFERENCE,
                    format!("{context}: dangling reference to {want_class} {guid}"),
                );
                None
            }
        }
    }

    /// The best-analysis-alternative string out of a multilingual field's forms: earliest in `Ctx::analysis_ws`, falling back to the first form present (or `""`).
    pub fn best_analysis(&self, forms: &[pg_snapshot::WsForm]) -> String {
        best_alt(forms, &self.analysis_ws)
    }

    /// As `Ctx::best_analysis` but preferring `Ctx::vernacular_ws`, used for boundary-marker representations.
    pub fn best_vernacular(&self, forms: &[pg_snapshot::WsForm]) -> String {
        best_alt(forms, &self.vernacular_ws)
    }
}

fn best_alt(forms: &[pg_snapshot::WsForm], priority: &[String]) -> String {
    for ws in priority {
        if let Some(f) = forms.iter().find(|f| &f.ws == ws) {
            return f.form.clone();
        }
    }
    forms.first().map(|f| f.form.clone()).unwrap_or_default()
}

/// Extract a whole `Snapshot` from a parsed object graph. `filename_stem` is the `.fwdata` file's stem, used as `project.name` since FieldWorks derives the project name from the file, never from the XML.
pub fn extract(
    graph: &RawGraph,
    filename_stem: &str,
) -> Result<(Snapshot, Vec<Warning>), ImportError> {
    let mut ctx = Ctx::new(graph);

    let lang_project = project::find_lang_project(&mut ctx);
    let project = project::extract_project(&mut ctx, lang_project, filename_stem);
    ctx.analysis_ws = project.analysis_writing_systems.clone();
    ctx.vernacular_ws = project.vernacular_writing_systems.clone();

    let feature_systems = features::extract_feature_systems(&mut ctx, lang_project);
    let phonology = phonology::extract_phonology(&mut ctx, lang_project, &feature_systems);
    let morphology = morphology::extract_morphology(&mut ctx, lang_project, &feature_systems)?;
    let lexicon = lexicon::extract_lexicon(&mut ctx, &feature_systems, &morphology);

    morphology::check_stale_adhoc_morpheme_rules(&mut ctx, &morphology, &lexicon);

    let (graph_to_snapshot, recorder_issues) = std::mem::take(&mut ctx.recorder).finish();
    let mut snapshot = Snapshot::new(project, feature_systems, phonology, morphology, lexicon);
    let mut import_issues = graph.issues.clone();
    import_issues.extend(recorder_issues);
    let source_inventory_status = if import_issues.iter().any(|issue| issue.fatal) {
        SourceInventoryStatus::ImportedWithFatalIssues
    } else {
        SourceInventoryStatus::ImportedComplete
    };
    snapshot.conversion_provenance = ConversionProvenance {
        schema_version: CONVERSION_PROVENANCE_SCHEMA_VERSION,
        source_inventory_status,
        source_census: graph.census(),
        graph_to_snapshot,
        import_issues,
    };
    Ok((snapshot, ctx.warnings))
}
