//! Prints the construction units a refused build spends at fractions of the full build, for the admission tables and the tiled acceptor; with `--marks`, only at 5% and 25%, each build between two calls of `stage_mark` for a profiler to dump at.
use foma::options::FomaOptions;
use pg_foma::admission::{
    build_rule_admission, build_rule_admission_within, build_tiled_within, class_of,
    plan_admission, tiled_prospect, AdmissionRefusal,
};
use pg_grammar::model::Grammar;
use std::fs;
use std::path::Path;

const PERCENTS: [u64; 4] = [1, 5, 25, 50];

fn load(path: &str) -> Result<Grammar, String> {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
    {
        "fwdata" | "fwbackup" => {
            let (snapshot, report) =
                pg_fwdata::import_file(Path::new(path)).map_err(|error| error.to_string())?;
            let mut warnings = report.warnings;
            warnings.extend(snapshot.validate());
            pg_grammar::compile_project_with_options_and_import_warnings(
                &snapshot,
                Default::default(),
                warnings,
            )
            .map(|out| out.grammar)
            .map_err(|error| format!("{error:?}"))
        }
        _ => pg_grammar::load(&fs::read_to_string(path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("{error:?}")),
    }
}

#[inline(never)]
fn stage_mark() {
    std::hint::black_box(());
}

fn main() -> Result<(), String> {
    let marks = std::env::args().any(|argument| argument == "--marks");
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: construction_budget <grammar>")?;
    let g = load(&path)?;
    let opts = FomaOptions::default();
    let admission = build_rule_admission(&opts, &g).map_err(|e| e.reason())?;
    let full_admission = admission.construction_units();
    let tiled_full = pg_foma::admission::build_tiled_within(&opts, &g, &admission, u64::MAX);
    let full_tiled = tiled_full.construction_units();
    let floor_admission = plan_admission(&g, &class_of(&g))
        .map_err(|e| e.reason())?
        .construction_floor(&g);
    let floor_tiled = tiled_prospect(&g, &admission).construction_floor();
    println!(
        "{}",
        serde_json::json!({"grammar": path, "admission_units": full_admission, "admission_floor": floor_admission, "tiled_units": full_tiled, "tiled_floor": floor_tiled, "tiled_active": tiled_full.active(), "tiled_off": tiled_full.inactive_reason()})
    );
    let percents: &[u64] = if marks { &[5, 25] } else { &PERCENTS };
    for &percent in percents {
        let budget = (full_admission * percent / 100).max(1);
        if marks {
            stage_mark();
        }
        let refused = match build_rule_admission_within(&opts, &g, budget) {
            Ok(done) => serde_json::json!({"done": done.construction_units()}),
            Err(AdmissionRefusal::ConstructionBudget { spent, .. }) => {
                serde_json::json!({"spent": spent})
            }
            Err(other) => serde_json::json!({"other": other.reason()}),
        };
        let budget_t = (full_tiled * percent / 100).max(1);
        if marks {
            stage_mark();
        }
        let tiled = build_tiled_within(&opts, &g, &admission, budget_t);
        println!(
            "{}",
            serde_json::json!({"percent": percent, "admission_budget": budget, "admission": refused, "tiled_budget": budget_t, "tiled_spent": tiled.construction_units(), "tiled_exhausted": tiled.evidence().budget_exhausted})
        );
    }
    if marks {
        stage_mark();
    }
    Ok(())
}
