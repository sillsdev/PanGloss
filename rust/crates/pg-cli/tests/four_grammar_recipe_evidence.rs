//! Portable evidence gate for the four promoted synthetic conformance grammars.
//! It invokes the production CLI and derives word lists from each checked-in words.yaml.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join(relative)
}

fn word_file(fixture: &str, root: &Path) -> PathBuf {
    let yaml = fs::read_to_string(repo_file(&format!(
        "machine/conformance/{fixture}/words.yaml"
    )))
    .expect("read fixture words.yaml");
    let words = yaml
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- word:"))
        .map(|word| word.trim().trim_matches(['\"', '\'']))
        .collect::<Vec<_>>();
    assert!(!words.is_empty(), "fixture must contain words: {fixture}");
    let path = root.join(fixture.replace('/', "-") + ".txt");
    fs::write(&path, words.join("\n") + "\n").expect("write derived word list");
    path
}

fn run_fixture(fixture: &str, root: &Path) -> Value {
    let out = root.join(fixture.replace('/', "-"));
    let words = word_file(fixture, root);
    let grammar = repo_file(&format!("machine/conformance/{fixture}/grammar.xml"));
    let status = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "recipe-optimize",
            grammar.to_str().unwrap(),
            words.to_str().unwrap(),
            out.to_str().unwrap(),
            "--seed",
            "17",
            "--candidates",
            "8",
            "--evaluations",
            "8",
        ])
        .status()
        .expect("production recipe-optimize must start");
    assert!(status.success(), "recipe-optimize failed for {fixture}");
    serde_json::from_str(&fs::read_to_string(out.join("report.json")).unwrap()).unwrap()
}

#[test]
fn four_promoted_grammars_have_truthful_recipe_evidence() {
    let root = std::env::temp_dir().join(format!(
        "pangloss-four-grammar-recipe-evidence-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let mpr = run_fixture("edge-cases/mpr-gated-exception", &root);
    assert_eq!(mpr["termination"], "complete");
    assert_eq!(mpr["counts"]["feasible"]["kind"], "exact");
    assert_eq!(mpr["counts"]["feasible"]["value"], 3);
    assert_eq!(mpr["pruning"]["confirmed"], 3);
    assert!(mpr["usage"]["memory_peak"].as_u64().unwrap() > 0);
    assert_eq!(mpr["replay_parameters"]["beam_width"], "16");
    assert_eq!(mpr["replay_parameters"]["pilot_candidate_cap"], "8");
    let pruning = &mpr["pruning"];
    let accounted = [
        "inapplicable",
        "duplicates",
        "materialization_rejects",
        "capability_rejected",
        "build_failures",
        "evaluated",
        "unvisited",
        "budget_pruned",
    ]
    .into_iter()
    .map(|key| pruning[key].as_u64().unwrap())
    .sum::<u64>();
    assert_eq!(pruning["generated"].as_u64().unwrap(), accounted);
    assert_eq!(mpr["candidates"].as_array().unwrap().len(), 3);
    assert!(mpr["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| {
            candidate["recipe_id"] == "class-exception-cascade|topology=gate-permutation"
        }));
    let mpr_out = root.join("edge-cases-mpr-gated-exception");
    for artifact in [
        "report.json",
        "report.md",
        "baseline.plan.json",
        "baseline.plan.mmd",
        "winner.plan.json",
        "winner.plan.mmd",
    ] {
        assert!(mpr_out.join(artifact).is_file(), "missing {artifact}");
    }
    let baseline_plan: Value =
        serde_json::from_str(&fs::read_to_string(mpr_out.join("baseline.plan.json")).unwrap())
            .unwrap();
    let winner_plan: Value =
        serde_json::from_str(&fs::read_to_string(mpr_out.join("winner.plan.json")).unwrap())
            .unwrap();
    assert_eq!(mpr["baseline"], baseline_plan["root"]);
    assert_eq!(mpr["winner"], winner_plan["root"]);
    assert!(mpr["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| candidate["id"] == winner_plan["root"]));

    let meta = run_fixture("languages/metathesis-phase-isolation", &root);
    assert!(meta["winner"].is_null());
    assert!(meta["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|candidate| {
            candidate["certification"]["status"] == "multiplicity-mismatch"
                && candidate["certification"]["word"] == "pur"
        }));

    let strata = run_fixture("languages/polysynthetic-stratal-derivation-chain", &root);
    assert!(strata["winner"].is_null());
    assert!(strata["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|candidate| {
            candidate["certification"]["status"] == "multiplicity-mismatch"
                && candidate["certification"]["word"] == "akutat"
        }));

    let deep_fixture = "edge-cases/deep-optional-affix-nesting";
    let deep_out = root.join("deep-timeout");
    let deep_words = word_file(deep_fixture, &root);
    let deep_grammar = repo_file(&format!("machine/conformance/{deep_fixture}/grammar.xml"));
    let status = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args([
            "recipe-optimize",
            deep_grammar.to_str().unwrap(),
            deep_words.to_str().unwrap(),
            deep_out.to_str().unwrap(),
            "--elapsed-ns",
            "100000000",
        ])
        .status()
        .expect("supervised deep optimizer must start");
    assert!(
        !status.success(),
        "pathological run must exhaust its budget"
    );
    let timeout: Value =
        serde_json::from_str(&fs::read_to_string(deep_out.join("status.json")).unwrap()).unwrap();
    assert_eq!(timeout["status"], "budget-exhausted");
    assert_eq!(timeout["certifying"], false);

    let _ = fs::remove_dir_all(root);
}
