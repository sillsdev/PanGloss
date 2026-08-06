//! `project` snapshot section — see `docs/snapshot-format.md` §2.

use pg_snapshot::Project;

use super::Ctx;
use crate::xml::Record;

/// A missing `LangProject` record warns rather than hard-errors: every downstream section already treats it as "resolve nothing, warn", so a warning plus an all-defaults `Snapshot` is more useful than a crash on truncated-but-parseable XML.
pub fn find_lang_project<'a>(ctx: &mut Ctx<'a>) -> Option<&'a Record> {
    let rec = ctx.graph.by_class("LangProject").next();
    if rec.is_none() {
        ctx.warn(
            super::codes::MISSING_LANG_PROJECT,
            "no <rt class=\"LangProject\"> record found in this .fwdata file".to_string(),
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
    }
}

/// `CurVernWss`/`CurAnalysisWss` are space-separated writing-system tags, default first.
fn ws_list(raw: Option<&str>) -> Vec<String> {
    raw.map(|s| s.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}
