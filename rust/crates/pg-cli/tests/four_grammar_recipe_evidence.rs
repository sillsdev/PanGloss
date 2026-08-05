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

/// Asserts a report's pruning ledger accounts for every candidate it generated, and returns the
/// ledger for further inspection. Applied to EVERY report this test reads, not just one: the ledger
/// is what makes a shrinking bucket readable. When an applicability predicate tightens, an instance
/// should move between buckets (`duplicates` -> `inapplicable`), never vanish -- and only this
/// identity can tell those two apart.
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

/// `topology=` values that name a whole-grammar `EmissionStrategy` rather than a plan rewrite.
///
/// A candidate carrying one of these was compiled by its own compiler, which builds the
/// composite/structural material `build_controllable` skips. It therefore has NO marker gap, and the
/// attribution rule that applies to plan-composed candidates -- "a failure must be reported as the
/// builder limitation that caused it, not as a word-level grammar fault" -- does not apply to it.
/// Its verdict, pass or fail, is a real measurement of a real network and must be read as one.
const WHOLE_GRAMMAR_TOPOLOGIES: [&str; 2] = ["templated-underlying-tokens", "tuned-surface-probed"];

fn is_whole_grammar_candidate(candidate: &Value) -> bool {
    let recipe_id = candidate["recipe_id"].as_str().unwrap_or_default();
    WHOLE_GRAMMAR_TOPOLOGIES
        .iter()
        .any(|topology| recipe_id.ends_with(&format!("topology={topology}")))
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
            // These budgets have to scale with the registry, and they are not decoration: at
            // `--evaluations 8` this fixture reported `budget-exhausted` the moment the registry grew
            // a second whole-grammar compiler, because the pilot consumes evaluations too (measured:
            // `usage.evaluations` = 10 for 5 candidates). An exhausted run cannot assert `complete`/
            // `exact`, so the whole point of this gate would quietly go untested.
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
    let fixture = "edge-cases/recipe-template-generic";
    let out = root.join(fixture.replace('/', "-"));
    let words = word_file_from(
        "conformance-staging/edge-cases/recipe-template-generic/words.yaml",
        fixture,
        root,
        |words| {
            // Characterization deliberately uses the C(12, 0)=1 boundary
            // observation. The C(12, 6)=924 midpoint and C(12, 12) endpoint
            // remain in the promoted full-HC oracle, outside this bounded pilot.
            vec![words.first().unwrap().clone()]
        },
    );
    let grammar = repo_file("conformance-staging/edge-cases/recipe-template-generic/grammar.xml");
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
    // 4, not 3, on BOTH counts, and this is the first real recipe result this project has produced.
    // The registry now offers a candidate that varies the COMPILER rather than the plan shape
    // (`token-cascade-morphology` = `EmissionStrategy::TemplatedUnderlyingTokens`), and on this
    // grammar it compiles a genuinely different network -- 25 states / 32 arcs against the baseline's
    // 27/38 -- AND confirms against full HC on every corpus word with non-zero proposals. So it is
    // selected over the baseline on a deterministic size difference rather than a build-time
    // tie-break. Every plan-shape candidate, by contrast, still ties the baseline exactly at 27/38.
    assert_eq!(mpr["counts"]["feasible"]["value"], 5);
    assert_eq!(mpr["pruning"]["confirmed"], 5);
    assert!(mpr["usage"]["memory_peak"].as_u64().unwrap() > 0);
    assert_eq!(mpr["replay_parameters"]["beam_width"], "16");
    assert_eq!(mpr["replay_parameters"]["pilot_candidate_cap"], "8");
    assert_eq!(mpr["replay_parameters"]["search_all_families"], "false");
    accounted_pruning(&mpr, "mpr-gated-exception");
    assert_eq!(mpr["candidates"].as_array().unwrap().len(), 5);
    // Pin the new axis by id, not just by count: a count alone would still pass if the
    // token-cascade candidate were replaced by yet another plan permutation, which is the
    // degeneracy this axis exists to escape.
    assert!(
        mpr["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|candidate| {
                candidate["recipe_id"]
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
    // The winner's id is NO LONGER its plan root, and asserting that it is would be asserting
    // something false. This winner is a whole-grammar strategy: its compiler derives its own topology
    // and never interprets the plan it carries, so it shares the BASELINE plan and its id is
    // suffixed to stay distinct. `winner.plan.*` therefore renders a plan the winner did not compile
    // -- which is why the report now states `winner_strategy`, so a reader of that diagram is told
    // rather than misled.
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

    // These two fixtures now CONFIRM, where they previously produced no winner at all (their
    // candidates failed with a `multiplicity-mismatch` on one word -- `pur` and `akutat`
    // respectively). Both plans need composite/structural marker subtrees `build_controllable`
    // cannot build, so an earlier version of this test expected a non-certifying result attributed
    // to that limitation.
    //
    // HONEST CAVEAT: the flip to confirming was NOT isolated to a specific change. The controllable
    // build path is byte-for-byte unchanged across it, and the only edits in the window were
    // strictly-stricter ones (a guard rejecting an all-empty corpus comparison, and a fallback that
    // fires only after a failure). So the improvement is real and reproducible on demand, but its
    // cause is unexplained and worth pinning down before anyone leans on it.
    //
    // The assertions below are therefore written so they cannot pass vacuously: a confirmation must
    // come with real proposals and a real corpus hash over every fixture word. An all-empty
    // comparison -- which `certify_word` would quite correctly call equal, and which certified three
    // Amharic candidates with zero proposals before it was guarded -- fails these.
    for (fixture, words_expected) in [
        ("languages/metathesis-phase-isolation", 19usize),
        ("languages/polysynthetic-stratal-derivation-chain", 0usize),
    ] {
        let report = run_fixture(fixture, &root);
        let candidates = report["candidates"].as_array().unwrap();
        assert!(!candidates.is_empty(), "{fixture}: no candidates evaluated");
        // IN-BAND ELIGIBILITY DERIVATION, at the artifact boundary. The run must publish the ledger
        // it derived from the RAW words file -- counts that reconcile, and the oracle configuration
        // it was derived under. Without this an artifact reporting "zero exclusions" is
        // indistinguishable from one handed a pre-filtered word list, which is precisely why the
        // Amharic certification is labelled provisional.
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
                // A whole-grammar strategy is exempt, and not as a convenience: it has no marker gap,
                // so there is no builder limitation to attribute, and relabelling its verdict
                // `unsupported` would hide a real measurement behind a notice that does not apply.
                // Measured: `templated-underlying-tokens` reports `multiplicity-mismatch` with
                // non-zero proposals on these two fixtures, while CONFIRMING on
                // `mpr-gated-exception` above -- the same strategy, two honest and different results.
                if is_whole_grammar_candidate(candidate) {
                    assert_ne!(
                        status, "unsupported",
                        "{fixture}: a whole-grammar strategy builds the marker material, so its                          verdict must be the real measurement rather than a limitation notice: {:?}",
                        candidate["certification"]
                    );
                    continue;
                }
                // A non-confirming PLAN-COMPOSED candidate is legitimate, but it must be attributed
                // rather than reporting a bare word-level symptom for what is really a builder
                // limitation.
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
    // 9 seeded families: the seven original plan-rewrite families plus the two whole-grammar
    // compilers (`token-cascade-morphology`, `surface-probe-morphology`). These two counts are of the
    // registry's DECLARED families, independent of which apply to this grammar.
    assert_eq!(template["counts"]["syntactic"], 9);
    assert_eq!(template["counts"]["attested"], 9);
    // 3, not 2: `token-cascade-morphology` used to gate on `HasPhonology` alone, so a
    // phonology-free grammar like this one never got it -- only the plan-composed baseline and the
    // always-applicable surface-probed compiler (`Applicability::Always`) were distinct candidates.
    // Widened to `HasPhonologyOrTemplates`, this grammar's `<AffixTemplate>` alone now qualifies it
    // too, and it is a genuinely distinct candidate rather than a relabelled baseline -- it
    // compiles 14 states / 91 arcs, same as the surface-probed one, where the plan-composed
    // baseline compiles 2/13. All three confirm; all three do 1 confirmation call, so the
    // work-first key ties them and the smaller network wins, which is why `winner_strategy` below
    // is still the plan-composed one.
    assert_eq!(template["counts"]["static_count"], 3);
    assert_eq!(template["counts"]["feasible"]["kind"], "exact");
    assert_eq!(template["counts"]["feasible"]["value"], 3);
    assert_eq!(template["pilot"]["sample_size"], 3);
    assert_eq!(template["winner_strategy"], "plan-composed");
    // One syntactic duplicate remains, while one applicable plan-rewrite instance is declared
    // not searched by the recorded compositional-topology policy. The single-entry applicability
    // predicate still accounts for `specialized-branch` separately as `inapplicable`.
    let template_pruning = accounted_pruning(&template, "recipe-template-generic");
    assert_eq!(template_pruning["duplicates"], 1);
    assert_eq!(template_pruning["declared_not_searched"], 1);
    assert!(
        template_pruning["inapplicable"].as_u64().unwrap() >= 4,
        "a one-entry, single-stratum, metathesis-free grammar admits only a minority of the seven \
         families; an `inapplicable` bucket this small means the report is counting applicability \
         rejections as something else: {template_pruning:?}"
    );
    // 3, not 2, mirroring `static_count`/`feasible` above: `token-cascade-morphology` is now a third
    // surviving, evaluated, confirming candidate for this phonology-free templated grammar.
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
