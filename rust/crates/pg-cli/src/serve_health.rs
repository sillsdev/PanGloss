//! `pangloss serve` op `health`: the `grammar-health` report for the grammar this process already loaded, so one process answers health and parse requests without a second load. Wire shape: `docs/formats/serve-protocol.md`.

use pg_grammar::model::Grammar;
use serde_json::{json, Map, Value};

use crate::grammar_health::{
    fieldworks_project_for_path, render_report, run_checks, Checks, FstSection,
};
use crate::stored_analysis_health::{Coverage, Limits};

const VERSION: u32 = 1;
const KNOWN_FIELDS: [&str; 5] = [
    "id",
    "op",
    "shadowed",
    "includeSelectable",
    "storedAnalyses",
];
const ERROR_CODES: [&str; 2] = ["bad_request", "internal"];

/// What the loaded project keeps for the health checks.
pub(crate) struct Input<'a> {
    pub(crate) project: &'a str,
    pub(crate) warnings: &'a [pg_snapshot::Warning],
    pub(crate) stored_analyses: &'a [pg_fwdata::StoredAnalysis],
}

/// The `interfaces.serveOps.health` capability object.
pub(crate) fn describe() -> Value {
    json!({
        "version": VERSION,
        "readOnly": true,
        "request": &KNOWN_FIELDS[2..],
        "errors": ERROR_CODES,
        "defaults": {"shadowed": false, "includeSelectable": false, "storedAnalyses": "bounded"},
        "storedAnalyses": ["bounded", "all", "off"],
        "notRun": ["fst", "snapshotOut", "factsOut"],
        "contract": "result.report is the document `pangloss grammar-health <project>` prints for the same project and options (same code), without the stderr log. Options are --shadowed [--include-selectable] and --stored-analyses bounded|all|off as request fields; includeSelectable requires shadowed. Not offered, because serve has no input for them or they write files: --fst, --fw-project (the project name is taken from the --project path as the command takes it), --snapshot-out, --facts-out, --log-guids. Stored analyses exist only for a .fwdata or .fwbackup project; a snapshot or grammar XML has none, so storedAnalyses changes nothing there. The report is computed from the grammar loaded at startup, never reloaded. Unlike the command, an error-level finding does not make the response an error. The report changes no parse",
    })
}

fn error(id: &Value, message: String) -> Value {
    json!({"id": id, "type": "error", "code": "bad_request", "message": message})
}

fn flag(object: &Map<String, Value>, name: &str) -> Result<bool, String> {
    match object.get(name) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(format!("\"{name}\" must be a boolean")),
    }
}

fn checks(object: &Map<String, Value>) -> Result<Checks, String> {
    if let Some(unknown) = object.keys().find(|k| !KNOWN_FIELDS.contains(&k.as_str())) {
        return Err(format!("unknown health field: {unknown}"));
    }
    let shadowed = flag(object, "shadowed")?;
    let include_selectable = flag(object, "includeSelectable")?;
    if include_selectable && !shadowed {
        return Err("\"includeSelectable\" requires \"shadowed\"".to_string());
    }
    let limits = Limits::DEFAULT;
    let coverage = match object.get("storedAnalyses") {
        None => Coverage::Budget(limits.work_budget),
        Some(Value::String(s)) => Coverage::parse(s, limits)
            .map_err(|_| format!("\"storedAnalyses\" must be bounded, all or off, not {s:?}"))?,
        Some(_) => return Err("\"storedAnalyses\" must be a string".to_string()),
    };
    Ok(Checks {
        shadowed,
        include_selectable,
        coverage,
        work_cap: limits.work_cap,
    })
}

/// Answers one `health` request.
pub(crate) fn handle(
    grammar: &Grammar,
    input: &Input,
    id: &Value,
    object: &Map<String, Value>,
) -> Value {
    let checks = match checks(object) {
        Ok(c) => c,
        Err(message) => return error(id, message),
    };
    let built = run_checks(
        grammar,
        input.stored_analyses,
        input.warnings,
        checks,
        |_| {},
    )
    .and_then(|run| {
        render_report(
            run.diagnostics,
            run.shadowed_report,
            run.stored_words,
            fieldworks_project_for_path(input.project, None),
            FstSection {
                requested: false,
                report: None,
                check_error: None,
            },
        )
    })
    .and_then(|(_, json)| serde_json::from_str::<Value>(&json).map_err(|e| e.to_string()));
    match built {
        Ok(report) => json!({"id": id, "type": "result", "op": "health", "report": report}),
        Err(message) => {
            json!({"id": id, "type": "error", "code": "internal", "message": message})
        }
    }
}
