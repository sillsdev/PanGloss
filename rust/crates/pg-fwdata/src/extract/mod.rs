//! Object-graph → `pg_snapshot::Snapshot` extraction, split by snapshot section (`project`, `features`, `phonology`, `morphology`, `lexicon`), sharing one `Ctx`; most guid fields pass through unresolved (validated later), and this crate only dereferences a guid where the snapshot embeds the target's own data inline, warning and skipping rather than panicking when the target is missing.

mod features;
mod lexicon;
mod morphology;
mod phonology;
mod project;

pub(crate) mod codes;

use pg_snapshot::{Snapshot, Warning};

use crate::xml::{RawGraph, Record};

/// Shared extraction context: the raw object graph, accumulating warnings, and writing-system priority lists that only become known once the `project` section has been read.
pub struct Ctx<'a> {
    pub graph: &'a RawGraph,
    pub warnings: Vec<Warning>,
    /// Analysis writing systems, default first — `project.analysisWritingSystems`.
    pub analysis_ws: Vec<String>,
    /// Vernacular writing systems, default first — `project.vernacularWritingSystems`.
    pub vernacular_ws: Vec<String>,
}

impl<'a> Ctx<'a> {
    fn new(graph: &'a RawGraph) -> Self {
        Ctx {
            graph,
            warnings: Vec::new(),
            analysis_ws: Vec::new(),
            vernacular_ws: Vec::new(),
        }
    }

    /// Record a warning: `code` is a stable short identifier naming the situation (see the `codes` module); `msg` is the human-readable prose.
    pub fn warn(&mut self, code: &'static str, msg: impl Into<String>) {
        self.warnings.push(Warning::new(code, msg));
    }

    pub fn get(&self, guid: &str) -> Option<&'a Record> {
        self.graph.get(guid)
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
pub fn extract(graph: &RawGraph, filename_stem: &str) -> (Snapshot, Vec<Warning>) {
    let mut ctx = Ctx::new(graph);

    let lang_project = project::find_lang_project(&mut ctx);
    let project = project::extract_project(&mut ctx, lang_project, filename_stem);
    ctx.analysis_ws = project.analysis_writing_systems.clone();
    ctx.vernacular_ws = project.vernacular_writing_systems.clone();

    let feature_systems = features::extract_feature_systems(&mut ctx, lang_project);
    let phonology = phonology::extract_phonology(&mut ctx, lang_project, &feature_systems);
    let morphology = morphology::extract_morphology(&mut ctx, lang_project, &feature_systems);
    let lexicon = lexicon::extract_lexicon(&mut ctx, &feature_systems, &morphology);

    morphology::check_stale_adhoc_morpheme_rules(&mut ctx, &morphology, &lexicon);

    let snapshot = Snapshot::new(project, feature_systems, phonology, morphology, lexicon);
    (snapshot, ctx.warnings)
}
