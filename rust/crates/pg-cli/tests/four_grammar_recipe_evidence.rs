//! Portable evidence gate for the four promoted synthetic conformance grammars: invokes the production CLI and derives word lists from each checked-in words.yaml.

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

/// Asserts a report's pruning ledger accounts for every generated candidate, so a tightened applicability predicate moves an instance between buckets rather than making it vanish.
fn accounted_pruning(report: &Value, label: &str) -> Value {
    let pruning = &report["pruning"];
    let accounted = [
        "declared_not_searched",
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
    .map(|key| {
        pruning[key]
            .as_u64()
            .unwrap_or_else(|| panic!("{label}: pruning ledger is missing {key}"))
    })
    .sum::<u64>();
    assert_eq!(
        pruning["generated"].as_u64().unwrap(),
        accounted,
        "{label}: pruning ledger does not account for every generated candidate: {pruning:?}"
    );
    pruning.clone()
}

/// `topology=` values naming a whole-grammar `EmissionStrategy`: these build their own composite/structural material, so they have no marker gap and the plan-composed attribution rule does not apply.
const WHOLE_GRAMMAR_TOPOLOGIES: [&str; 2] = ["templated-underlying-tokens", "tuned-surface-probed"];

fn is_whole_grammar_candidate(candidate: &Value) -> bool {
    let backend_id = candidate["backend_id"].as_str().unwrap_or_default();
    WHOLE_GRAMMAR_TOPOLOGIES
        .iter()
        .any(|topology| backend_id.ends_with(&format!("topology={topology}")))
}

fn word_file(fixture: &str, root: &Path) -> PathBuf {
    word_file_from(
        &format!("machine/conformance/{fixture}/words.yaml"),
        fixture,
        root,
        |words| words,
    )
}

fn word_file_from(
    relative: &str,
    fixture: &str,
    root: &Path,
    select: impl FnOnce(Vec<String>) -> Vec<String>,
) -> PathBuf {
    let yaml = fs::read_to_string(repo_file(relative)).expect("read fixture words.yaml");
    let words = yaml
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- word:"))
        .map(|word| word.trim().trim_matches(['\"', '\'']).to_owned())
        .collect::<Vec<_>>();
    let words = select(words);
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
            // Sized to the registry, not a historical count: at 8 evaluations this fixture reported budget-exhausted once the registry grew a second whole-grammar compiler.
            "--candidates",
            "32",
            "--evaluations",
            "64",
        ])
        .status()
        .expect("production recipe-optimize must start");
    assert!(status.success(), "recipe-optimize failed for {fixture}");
    serde_json::from_str(&fs::read_to_string(out.join("report.json")).unwrap()).unwrap()
}

fn run_template_characterization(root: &Path, search_all_families: bool) -> Value {
    let fixture = "edge-cases/backend-template-generic";
    let out = root.join(fixture.replace('/', "-"));
    let words = word_file_from(
        "conformance-staging/edge-cases/backend-template-generic/words.yaml",
        fixture,
        root,
        |words| {
            // Deliberately uses the C(12, 0)=1 boundary; the C(12, 6)/C(12, 12) cases stay in the promoted full-HC oracle, outside this bounded pilot.
            vec![words.first().unwrap().clone()]
        },
    );
    let grammar = repo_file("conformance-staging/edge-cases/backend-template-generic/grammar.xml");
    let mut command = Command::new(env!("CARGO_BIN_EXE_pangloss"));
    command.args([
        "recipe-optimize",
        grammar.to_str().unwrap(),
        words.to_str().unwrap(),
        out.to_str().unwrap(),
        "--seed",
        "17",
        // Same reason as `run_fixture`: sized to the registry, not to a historical count.
        "--candidates",
        "32",
        "--evaluations",
        "64",
        "--elapsed-ns",
        "5000000000",
    ]);
    if search_all_families {
        command.arg("--search-all-families");
    }
    let status = command
        .status()
        .expect("bounded template characterization must start");
    assert!(
        status.success(),
        "bounded template characterization must complete"
    );
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
    // 4, not 3: the compiler-varying token-cascade-morphology candidate compiles a genuinely smaller network (25/32 states/arcs vs. baseline 27/38) and confirms, so it wins on size, not a tie-break.
    assert_eq!(mpr["counts"]["feasible"]["value"], 5);
    assert_eq!(mpr["pruning"]["confirmed"], 5);
    assert!(mpr["usage"]["memory_peak"].as_u64().unwrap() > 0);
    assert_eq!(mpr["replay_parameters"]["beam_width"], "16");
    assert_eq!(mpr["replay_parameters"]["pilot_candidate_cap"], "8");
    assert_eq!(mpr["replay_parameters"]["search_all_families"], "false");
    accounted_pruning(&mpr, "mpr-gated-exception");
    assert_eq!(mpr["candidates"].as_array().unwrap().len(), 5);
    // Pinned by id, not just count, so a swap to another plan permutation cannot pass vacuously.
    assert!(
        mpr["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|candidate| {
                candidate["backend_id"]
                    == "token-cascade-morphology|topology=templated-underlying-tokens"
            }),
        "the compiler-varying candidate must appear in the evidence: {:?}",
        mpr["candidates"]
    );
    assert!(mpr["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| {
            candidate["backend_id"] == "class-exception-cascade|topology=gate-permutation"
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
    // A whole-grammar winner shares the baseline's plan (its compiler never interprets it), so its id is suffixed to stay distinct; `winner.plan.*` renders a plan it did not itself compile, hence `winner_strategy`.
    assert_eq!(mpr["winner_strategy"], "templated-underlying-tokens");
    let winner_id = mpr["winner"].as_str().expect("a winner id");
    let winner_root = winner_plan["root"].as_str().expect("a plan root");
    assert_eq!(
        winner_id,
        format!("{winner_root}@templated-underlying-tokens"),
        "a whole-grammar winner's id must be its plan root plus its strategy, so it stays distinct          from the baseline candidate that shares that very plan"
    );
    assert_eq!(
        winner_root,
        baseline_plan["root"].as_str().expect("a baseline root"),
        "this winner is expected to carry the baseline plan verbatim; if it stops doing so, the          reasoning above about the diagram no longer applies and this block needs revisiting"
    );
    assert!(mpr["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| candidate["id"] == winner_id));

    // These two fixtures now confirm (previously `multiplicity-mismatch`); the flip is real and reproducible but its cause is unexplained, so assertions below require real proposals and a real hash, never an all-empty vacuous pass.
    for (fixture, words_expected) in [
        ("languages/metathesis-phase-isolation", 19usize),
        ("languages/polysynthetic-stratal-derivation-chain", 0usize),
    ] {
        let report = run_fixture(fixture, &root);
        let candidates = report["candidates"].as_array().unwrap();
        assert!(!candidates.is_empty(), "{fixture}: no candidates evaluated");
        // The run must publish the corpus ledger it derived from the raw words file, or "zero exclusions" would be indistinguishable from a pre-filtered word list.
        let corpus = &report["corpus"];
        assert!(
            corpus.is_object(),
            "{fixture}: the report must carry its corpus eligibility ledger, got {corpus:?}"
        );
        let requested = corpus["requested"].as_u64().unwrap();
        let raw_lines = fs::read_to_string(word_file(fixture, &root))
            .unwrap()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count() as u64;
        assert_eq!(
            requested, raw_lines,
            "{fixture}: the ledger must account for every RAW corpus line"
        );
        assert_eq!(
            requested,
            corpus["included"].as_u64().unwrap() + corpus["excluded"].as_u64().unwrap(),
            "{fixture}: requested must equal included + excluded"
        );
        assert!(
            corpus["oracle_step_cap"].as_u64().unwrap() > 0,
            "{fixture}: the ledger must state the step cap it classified under"
        );
        assert!(corpus["oracle_memory_ceiling_bytes"].as_u64().is_some());
        assert!(corpus["oracle_liveness_net_ns"].as_u64().is_some());
        assert!(
            !corpus["exclusions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|exclusion| exclusion["reason"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("timeout")),
            "{fixture}: a wall-clock outcome must be unrepresentable as an exclusion: {:?}",
            corpus["exclusions"]
        );
        for candidate in candidates {
            let status = candidate["certification"]["status"].as_str().unwrap_or("");
            if status != "full-hc-confirmed" {
                // A whole-grammar strategy has no marker gap, so nothing to attribute; relabelling it `unsupported` would hide a real measurement.
                if is_whole_grammar_candidate(candidate) {
                    assert_ne!(
                        status, "unsupported",
                        "{fixture}: a whole-grammar strategy builds the marker material, so its                          verdict must be the real measurement rather than a limitation notice: {:?}",
                        candidate["certification"]
                    );
                    continue;
                }
                // A non-confirming plan-composed candidate is legitimate but must be attributed to the builder limitation, not reported as a bare word-level fault.
                assert_eq!(
                    status, "unsupported",
                    "{fixture}: a non-confirming candidate must be attributed, got {:?}",
                    candidate["certification"]
                );
                continue;
            }
            assert!(
                candidate["score"]["proposals"].as_u64().unwrap_or(0) > 0,
                "{fixture}: confirmed with zero proposals is a vacuous pass: {:?}",
                candidate["score"]
            );
            let hash = candidate["certification"]["corpus_hash"]
                .as_str()
                .unwrap_or_default();
            assert_eq!(
                hash.len(),
                64,
                "{fixture}: confirmation must carry a corpus hash"
            );
            if words_expected > 0 {
                assert_eq!(
                    candidate["certification"]["words"].as_u64().unwrap_or(0),
                    words_expected as u64,
                    "{fixture}: confirmation must cover every fixture word"
                );
            }
        }
    }

    let template = run_template_characterization(&root, false);
    assert_eq!(template["termination"], "complete");
    assert_eq!(template["quality"], "exact");
    // 9 seeded families: seven plan-rewrite plus two whole-grammar compilers, counted independent of which apply to this grammar.
    assert_eq!(template["counts"]["syntactic"], 9);
    assert_eq!(template["counts"]["attested"], 9);
    // 3, not 2: widening to HasPhonologyOrTemplates makes this template-bearing, phonology-free grammar qualify for token-cascade-morphology too; all three confirm and tie on work, so the smaller plan-composed network still wins.
    assert_eq!(template["counts"]["static_count"], 3);
    assert_eq!(template["counts"]["feasible"]["kind"], "exact");
    assert_eq!(template["counts"]["feasible"]["value"], 3);
    assert_eq!(template["pilot"]["sample_size"], 3);
    assert_eq!(template["winner_strategy"], "plan-composed");
    // One syntactic duplicate remains; the single-entry applicability predicate still separately declares `specialized-branch` inapplicable.
    let template_pruning = accounted_pruning(&template, "backend-template-generic");
    assert_eq!(template_pruning["duplicates"], 1);
    assert_eq!(template_pruning["declared_not_searched"], 1);
    assert!(
        template_pruning["inapplicable"].as_u64().unwrap() >= 4,
        "a one-entry, single-stratum, metathesis-free grammar admits only a minority of the seven \
         families; an `inapplicable` bucket this small means the report is counting applicability \
         rejections as something else: {template_pruning:?}"
    );
    // 3, not 2, mirroring static_count/feasible: token-cascade-morphology is now a third surviving, confirming candidate.
    assert_eq!(template["pruning"]["evaluated"], 3);
    assert_eq!(template["pruning"]["confirmed"], 3);
    assert_eq!(template["strategy"], "exhaustive");
    let template_opt_in = run_template_characterization(&root, true);
    assert_eq!(
        template_opt_in["replay_parameters"]["search_all_families"],
        "true"
    );
    assert_eq!(template_opt_in["pruning"]["declared_not_searched"], 0);
    assert_eq!(template_opt_in["pruning"]["duplicates"], 2);
    assert_eq!(template_opt_in["pruning"]["evaluated"], 3);

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
