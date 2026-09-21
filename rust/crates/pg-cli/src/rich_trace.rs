//! Opt-in single-word JSON envelope around the existing trace tree and parser statistics.

use std::time::Duration;

use pg_grammar::model::Grammar;
use pg_parse::ParseOutcome;
use pg_rules::stats::{self, Counters, ObjectKind, StatsRow};
use pg_rules::trace::{TraceHandle, TreeTraceSink};
use serde_json::{json, Value};

/// Validate the explicit details flag without changing the ordinary trace mode.
pub fn validate_details(
    trace_requested: bool,
    trace_format: &str,
    details: bool,
    has_text_output_flags: bool,
) -> Result<(), String> {
    if details && !trace_requested {
        return Err("--trace-details requires --trace".into());
    }
    if details && trace_format != "json" {
        return Err("--trace-details requires --trace-format=json".into());
    }
    if details && has_text_output_flags {
        return Err("--trace-details cannot be combined with --gloss or --natural-gloss".into());
    }
    Ok(())
}

fn kind_name(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::MorphRule => "morphRule",
        ObjectKind::PhonRule => "phonRule",
        ObjectKind::LexEntry => "lexEntry",
        ObjectKind::RootIndex => "rootIndex",
        ObjectKind::Guesser => "guesser",
        ObjectKind::Overlay => "overlay",
    }
}

fn add_counters(total: &mut Counters, row: &StatsRow) {
    total.attempts += row.counters.attempts;
    total.work += row.counters.work;
    total.outputs += row.counters.outputs;
    total.not_applied += row.counters.not_applied;
    total.no_root += row.counters.no_root;
    total.surface_mismatch += row.counters.surface_mismatch;
    total.uses += row.counters.uses;
    total.self_time_ns += row.counters.self_time_ns;
}

fn stats_json(rows: &[StatsRow]) -> Value {
    let kinds = [
        ObjectKind::MorphRule,
        ObjectKind::PhonRule,
        ObjectKind::LexEntry,
        ObjectKind::RootIndex,
        ObjectKind::Guesser,
        ObjectKind::Overlay,
    ];
    let categories = kinds.into_iter().map(|kind| {
        let mut counters = Counters::default();
        for row in rows.iter().filter(|row| row.kind == kind) {
            add_counters(&mut counters, row);
        }
        let timing_available = stats::self_time_supported(kind);
        (
            kind_name(kind).to_owned(),
            json!({
                "attempts": counters.attempts,
                "work": counters.work,
                "outputs": counters.outputs,
                "notApplied": counters.not_applied,
                "noRoot": counters.no_root,
                "surfaceMismatch": counters.surface_mismatch,
                "uses": counters.uses,
                "timingAvailable": timing_available,
                "selfElapsedNs": timing_available.then_some(counters.self_time_ns),
            }),
        )
    });
    serde_json::Map::from_iter(categories).into()
}

fn render_envelope(
    trace: Value,
    word: &str,
    outcome: &ParseOutcome,
    rows: &[StatsRow],
    elapsed: Duration,
) -> Result<String, String> {
    let analyses: Vec<Value> = outcome
        .analyses
        .iter()
        .map(|(morphemes, surface)| json!({ "morphemes": morphemes, "surface": surface }))
        .collect();
    let elapsed_ns = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
    serde_json::to_string(&json!({
        "schemaVersion": "pangloss.trace-details.v1",
        "word": word,
        "search": {
            "completed": !outcome.capped && !outcome.timed_out && !outcome.invalid_shape,
            "capped": outcome.capped,
            "timedOut": outcome.timed_out,
            "invalidShape": outcome.invalid_shape,
            "steps": outcome.steps,
            "elapsedNs": elapsed_ns,
        },
        "result": {
            "signature": outcome.signature(),
            "guessed": outcome.guessed,
            "analyses": analyses,
        },
        "categories": stats_json(rows),
        "trace": trace,
    }))
    .map_err(|error| format!("serialize rich trace JSON: {error}"))
}

/// Build the versioned envelope while delegating tree rendering to the existing serializer.
pub fn render(
    grammar: &Grammar,
    sink: &TreeTraceSink,
    root: Option<TraceHandle>,
    word: &str,
    outcome: &ParseOutcome,
    rows: &[StatsRow],
    elapsed: Duration,
) -> Result<String, String> {
    let trace = match root {
        Some(root) => {
            serde_json::from_str::<Value>(&crate::trace_render::render_json(grammar, sink, root))
                .map_err(|error| format!("serialize trace JSON: {error}"))?
        }
        None => Value::Null,
    };
    render_envelope(trace, word, outcome, rows, elapsed)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{render_envelope, validate_details};
    use pg_grammar::model::StratumId;
    use pg_parse::ParseOutcome;
    use pg_rules::stats::{Counters, Direction, ObjectKind, StatsRow};
    use serde_json::json;

    #[test]
    fn details_requires_trace_and_json() {
        assert!(validate_details(true, "json", true, false).is_ok());
        assert!(validate_details(false, "json", true, false).is_err());
        assert!(validate_details(true, "text", true, false).is_err());
        assert!(validate_details(true, "json", true, true).is_err());
    }

    #[test]
    fn envelope_keeps_tree_result_and_unmeasured_timing_explicit() {
        let outcome = ParseOutcome {
            analyses: vec![("root+past".into(), "sagd".into())],
            structured: Vec::new(),
            capped: false,
            invalid_shape: false,
            steps: 17,
            timed_out: false,
            guessed: false,
            candidates_generated: 1,
        };
        let rows = vec![
            StatsRow {
                kind: ObjectKind::MorphRule,
                object_index: 0,
                stratum: StratumId(0),
                allomorph: 0,
                direction: Direction::Synthesis,
                counters: Counters {
                    attempts: 2,
                    self_time_ns: 11,
                    ..Counters::default()
                },
            },
            StatsRow {
                kind: ObjectKind::Overlay,
                object_index: 0,
                stratum: StratumId(0),
                allomorph: 0,
                direction: Direction::Analysis,
                counters: Counters {
                    attempts: 1,
                    self_time_ns: 99,
                    ..Counters::default()
                },
            },
            StatsRow {
                kind: ObjectKind::PhonRule,
                object_index: 0,
                stratum: StratumId(0),
                allomorph: 0,
                direction: Direction::Analysis,
                counters: Counters {
                    attempts: 3,
                    self_time_ns: 77,
                    ..Counters::default()
                },
            },
        ];
        let tree = json!({
            "type": "WordAnalysis",
            "children": [{"type": "Successful", "children": []}]
        });
        let value: serde_json::Value = serde_json::from_str(
            &render_envelope(
                tree.clone(),
                "sagd",
                &outcome,
                &rows,
                Duration::from_nanos(7),
            )
            .expect("rich envelope serializes"),
        )
        .expect("rich envelope is JSON");
        assert_eq!(value["schemaVersion"], "pangloss.trace-details.v1");
        assert_eq!(value["trace"], tree);
        assert_eq!(value["search"]["completed"], true);
        assert_eq!(value["result"]["signature"], "root+past|sagd");
        assert_eq!(value["categories"]["morphRule"]["selfElapsedNs"], 11);
        assert_eq!(value["categories"]["morphRule"]["attempts"], 2);
        assert_eq!(value["categories"]["phonRule"]["attempts"], 3);
        assert_eq!(
            value["categories"]["phonRule"]["selfElapsedNs"],
            serde_json::Value::Null
        );
        assert_eq!(value["categories"]["phonRule"]["timingAvailable"], false);
        assert_eq!(
            value["categories"]["overlay"]["selfElapsedNs"],
            serde_json::Value::Null
        );
        assert_eq!(value["categories"]["overlay"]["timingAvailable"], false);
    }
}
