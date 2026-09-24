//! `project` snapshot section — see `docs/snapshot-format.md` §2.

use pg_snapshot::{FwClass, FwObjectRef, Project, Warning};

use super::Ctx;
use crate::xml::Record;

/// A missing `LangProject` record warns rather than hard-errors: every downstream section already treats it as "resolve nothing, warn", so a warning plus an all-defaults `Snapshot` is more useful than a crash on truncated-but-parseable XML.
pub fn find_lang_project<'a>(ctx: &mut Ctx<'a>, project_name: &str) -> Option<&'a Record> {
    let rec = ctx.graph.by_class("LangProject").next();
    if rec.is_none() {
        ctx.warnings.push(
            Warning::new(
                super::codes::MISSING_LANG_PROJECT,
                format!("FieldWorks project '{project_name}' has no language project data."),
            )
            .with_subject(FwObjectRef::new(FwClass::Project).name(project_name)),
        );
    }
    rec
}

pub fn extract_project(
    _ctx: &mut Ctx,
    lang_project: Option<&Record>,
    filename_stem: &str,
) -> Project {
    let (vernacular, analysis) = match lang_project {
        Some(rec) => (
            ws_list(rec.node.uni_text("CurVernWss").as_deref()),
            ws_list(rec.node.uni_text("CurAnalysisWss").as_deref()),
        ),
        None => (Vec::new(), Vec::new()),
    };
    Project {
        name: filename_stem.to_string(),
        vernacular_writing_systems: vernacular,
        analysis_writing_systems: analysis,
        exemplar_characters: Vec::new(),
    }
}

/// `CurVernWss`/`CurAnalysisWss` are space-separated writing-system tags, default first.
fn ws_list(raw: Option<&str>) -> Vec<String> {
    raw.map(|s| s.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}
