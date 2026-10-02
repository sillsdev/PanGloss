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
    ConversionProvenance, FwClass, FwObjectRef, ImportWarningCode, InventoryKey, IssueClass,
    SelectionRecorder, Snapshot, SourceInventoryStatus, SourceRef, Warning,
    CONVERSION_PROVENANCE_SCHEMA_VERSION,
};

use crate::{
    xml::{RawGraph, Record},
    ImportError,
};

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

#[derive(Clone, Copy)]
pub(crate) enum ReferenceResolution<'a> {
    Found(&'a Record),
    WrongClass(&'a Record),
    Missing,
}

impl<'a> Ctx<'a> {
    fn new(graph: &'a RawGraph) -> Self {
        let mut recorder = SelectionRecorder::default();
        inventory::seed_authored_from_graph(&mut recorder, graph);
        Ctx {
            graph,
            warnings: Vec::new(),
            analysis_ws: Vec::new(),
            vernacular_ws: Vec::new(),
            recorder,
        }
    }

    pub(crate) fn warn_with_subjects(
        &mut self,
        code: ImportWarningCode,
        msg: impl Into<String>,
        subjects: impl IntoIterator<Item = FwObjectRef>,
    ) {
        let mut warning = Warning::new(code, msg);
        warning.subjects.extend(subjects);
        self.warnings.push(warning);
    }

    pub(crate) fn subject_for_record(&self, record: &Record, field: Option<&str>) -> FwObjectRef {
        let name = record_name(record, self);
        let mut subject = FwObjectRef::new(FwClass::from_wire(&record.class))
            .guid(record.guid.clone())
            .source_class(record.class.clone());
        if let Some(name) = name.filter(|name| !name.trim().is_empty()) {
            subject = subject.name(name);
        }
        if let Some(field) = field {
            subject = subject.field(field);
        }
        subject
    }

    pub(crate) fn unresolved_subject(
        &self,
        guid: &str,
        want_class: &str,
        field: Option<&str>,
    ) -> FwObjectRef {
        let mut subject = FwObjectRef::new(FwClass::from_wire(want_class))
            .guid(guid)
            .unresolved_reference()
            .source_class(want_class);
        if let Some(field) = field {
            subject = subject.field(field);
        }
        subject
    }

    pub(crate) fn reject_with_warning(
        &mut self,
        key: InventoryKey,
        class: IssueClass,
        fatal: bool,
        source: Option<SourceRef>,
        warning: Warning,
    ) {
        self.warnings.push(warning.clone());
        self.recorder.rejected(
            key,
            pg_snapshot::ConversionIssue {
                code: ImportWarningCode::from_wire_or_unregistered(&warning.code),
                class,
                source,
                fatal,
                message: warning.message,
            },
        );
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

    /// Records `key` rejected with no new warning, for a failure a caller has already warned about through another path (e.g. `Ctx::require` or `parser_params`'s own issues).
    pub(crate) fn record_rejected(
        &mut self,
        key: InventoryKey,
        issue: pg_snapshot::ConversionIssue,
    ) {
        self.recorder.rejected(key, issue);
    }

    pub(crate) fn require_from(
        &mut self,
        guid: &str,
        want_class: &str,
        context: &str,
        owner: &Record,
        field: &str,
    ) -> Option<&'a Record> {
        let resolution = self.resolve_reference(guid, want_class);
        self.require_resolution(guid, want_class, context, resolution, (owner, field))
    }

    /// Shares one missing/wrong-class decision between diagnostics and extraction.
    pub(crate) fn resolve_reference(
        &self,
        guid: &str,
        want_class: &str,
    ) -> ReferenceResolution<'a> {
        match self.get(guid) {
            Some(record) if record.class == want_class => ReferenceResolution::Found(record),
            Some(record) => ReferenceResolution::WrongClass(record),
            None => ReferenceResolution::Missing,
        }
    }

    pub(crate) fn subjects_for_reference(
        &self,
        guid: &str,
        want_class: &str,
        resolution: ReferenceResolution<'a>,
        owner_field: (&Record, &str),
    ) -> Vec<FwObjectRef> {
        let (owner, field) = owner_field;
        let mut subjects = vec![self.subject_for_record(owner, Some(field))];
        match resolution {
            ReferenceResolution::Found(record) => {
                subjects.push(self.subject_for_record(record, Some(field)));
            }
            ReferenceResolution::WrongClass(record) => {
                subjects.push(self.unresolved_subject(guid, want_class, Some(field)));
                subjects.push(self.subject_for_record(record, Some(field)));
            }
            ReferenceResolution::Missing => {
                subjects.push(self.unresolved_subject(guid, want_class, Some(field)));
            }
        }
        subjects
    }

    pub(crate) fn require_resolution(
        &mut self,
        guid: &str,
        want_class: &str,
        context: &str,
        resolution: ReferenceResolution<'a>,
        owner_field: (&Record, &str),
    ) -> Option<&'a Record> {
        match resolution {
            ReferenceResolution::Found(record) => Some(record),
            ReferenceResolution::WrongClass(record) => {
                let subjects =
                    self.subjects_for_reference(guid, want_class, resolution, owner_field);
                self.warn_with_subjects(
                    codes::UNEXPECTED_CLASS,
                    format!(
                        "{context}: expected {want_class} but {guid} is {}",
                        record.class
                    ),
                    subjects,
                );
                None
            }
            ReferenceResolution::Missing => {
                let subjects =
                    self.subjects_for_reference(guid, want_class, resolution, owner_field);
                self.warn_with_subjects(
                    codes::DANGLING_REFERENCE,
                    format!("{context}: dangling reference to {want_class} {guid}"),
                    subjects,
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

fn record_name(record: &Record, ctx: &Ctx<'_>) -> Option<String> {
    if record.class == "LexEntry" {
        let citation = ctx.best_analysis(&record.node.ws_forms("CitationForm"));
        if !citation.trim().is_empty() {
            return Some(citation);
        }
        if let Some(name) = record
            .node
            .objsur_one("LexemeForm")
            .and_then(|guid| ctx.get(&guid))
            .map(|lexeme_form| ctx.best_vernacular(&lexeme_form.node.ws_forms("Form")))
            .filter(|name| !name.trim().is_empty())
        {
            return Some(name);
        }
    }
    let fields = match record.class.as_str() {
        "LexEntry" => ["CitationForm", "Name", "Abbreviation"],
        "MoStemAllomorph" | "MoAffixAllomorph" | "MoAffixProcess" | "MoForm" => {
            ["Form", "Name", "CitationForm"]
        }
        _ => ["Name", "Abbreviation", "Form"],
    };
    for field in fields {
        let forms = record.node.ws_forms(field);
        let name = ctx.best_analysis(&forms);
        if !name.trim().is_empty() {
            return Some(name);
        }
    }
    None
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
    let (snapshot, warnings, _recorder) = extract_recording(graph, filename_stem)?;
    Ok((snapshot, warnings))
}

/// As [`extract`], but also returns the unfinished [`SelectionRecorder`] `import_file_measured` reads into an `InventoryDelta`; `conversion_provenance` itself comes from an independently finished clone, so this changes nothing about `extract`'s own behaviour.
pub(crate) fn extract_recording(
    graph: &RawGraph,
    filename_stem: &str,
) -> Result<(Snapshot, Vec<Warning>, SelectionRecorder), ImportError> {
    let mut ctx = Ctx::new(graph);

    let lang_project = project::find_lang_project(&mut ctx, filename_stem);
    let project = project::extract_project(&mut ctx, lang_project, filename_stem);
    ctx.analysis_ws = project.analysis_writing_systems.clone();
    ctx.vernacular_ws = project.vernacular_writing_systems.clone();

    let feature_systems = features::extract_feature_systems(&mut ctx, lang_project);
    let phonology =
        phonology::extract_phonology(&mut ctx, lang_project, &feature_systems, &project.name);
    let morphology =
        morphology::extract_morphology(&mut ctx, lang_project, &feature_systems, &project.name)?;
    let lexicon = lexicon::extract_lexicon(&mut ctx, &feature_systems, &morphology);

    morphology::check_stale_adhoc_morpheme_rules(&mut ctx, &morphology, &lexicon);

    let (graph_to_snapshot, recorder_issues) = ctx.recorder.clone().finish();
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
    Ok((snapshot, ctx.warnings, ctx.recorder))
}
