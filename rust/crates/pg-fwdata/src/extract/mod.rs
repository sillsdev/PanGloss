//! Object-graph → [`pg_snapshot::Snapshot`] extraction.
//!
//! Split by snapshot section (mirrors `docs/snapshot-format.md`'s own section numbering):
//! [`project`], [`features`], [`phonology`], [`morphology`], [`lexicon`]. All share the same
//! [`Ctx`] (the raw object graph, an accumulating warnings list, and small caches like the
//! project's writing-system priority lists used for "best analysis/vernacular alternative"
//! resolution).
//!
//! ## Guid fields: pass-through vs. dereference
//!
//! Most guid-valued fields in the snapshot (`Msa::Stem.part_of_speech`, `Allomorph.environments`,
//! `AdhocProhibition::Morpheme.primary`, ...) are **pass-through**: `pg-fwdata` copies the
//! `objsur` guid into the snapshot unresolved, and [`pg_snapshot::Snapshot::validate`] is the
//! place dangling references in that category get flagged later. This crate only needs to
//! actually *look up* (dereference) a guid where the snapshot embeds the target's own data
//! inline — phoneme/natural-class/feature-structure/POS-tree/rule-context construction, and the
//! handful of "wrapper" indirections FieldWorks uses (`PhPhonRuleFeat.Item`, the phoneme/boundary
//! representations `MoInsertPhones` concatenates). Every dereference point warns and skips
//! (never panics) when the target is missing — this is where this crate's log-and-skip
//! robustness requirement (`docs/fwdata-import-plan.md` §1) actually bites.

mod features;
mod lexicon;
mod morphology;
mod phonology;
mod project;

use pg_snapshot::Snapshot;

use crate::xml::{RawGraph, Record};

/// Shared extraction context: the raw object graph, the accumulating import-report warnings, and
/// the writing-system priority lists ("best analysis/vernacular alternative" resolution) that
/// only become known once the `project` section has been read.
pub struct Ctx<'a> {
    pub graph: &'a RawGraph,
    pub warnings: Vec<String>,
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

    pub fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    pub fn get(&self, guid: &str) -> Option<&'a Record> {
        self.graph.get(guid)
    }

    /// Resolve `guid` expecting a specific class; warns (once, with `context`) and returns
    /// `None` if the guid is dangling or resolves to a surprising class.
    pub fn require(&mut self, guid: &str, want_class: &str, context: &str) -> Option<&'a Record> {
        match self.get(guid) {
            Some(r) if r.class == want_class => Some(r),
            Some(r) => {
                self.warn(format!(
                    "{context}: expected {want_class} but {guid} is {}",
                    r.class
                ));
                None
            }
            None => {
                self.warn(format!("{context}: dangling reference to {want_class} {guid}"));
                None
            }
        }
    }

    /// The best-analysis-alternative string out of a multilingual field's forms: the form whose
    /// writing system is earliest in [`Ctx::analysis_ws`], falling back to the first form present
    /// (or `""` if the field was empty) — mirrors `BestAnalysisAlternative` as used throughout
    /// `HCLoader.cs` for `Name`/`Abbreviation`.
    pub fn best_analysis(&self, forms: &[pg_snapshot::WsForm]) -> String {
        best_alt(forms, &self.analysis_ws)
    }

    /// As [`Ctx::best_analysis`] but preferring [`Ctx::vernacular_ws`] — used for boundary-marker
    /// representations (`BestVernacularAlternative`, HCLoader.cs:2700-2702).
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

/// Extract a whole [`Snapshot`] from a parsed object graph. `filename_stem` is the `.fwdata`
/// file's stem (e.g. `"Sena 3"`), used as `project.name` — FieldWorks derives
/// `LcmCache.ProjectId.Name` from the project folder/file name, not from anything stored inside
/// the XML (`HCLoader.LoadLanguage`, HCLoader.cs:166).
pub fn extract(graph: &RawGraph, filename_stem: &str) -> (Snapshot, Vec<String>) {
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
