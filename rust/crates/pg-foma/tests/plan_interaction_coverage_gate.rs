//! Integration test for tree-structured node/subtree interaction coverage over the reified compilation plan (rather than pairwise covering arrays over raw grammar "knobs"); build-breaking if any required `AdjacencyTuple` is `Uncovered`.
//! See docs/research/pg-foma-plan-interaction-coverage-gate-notes.md for what this gate does and does not assert, and the separate fuzz-slice correctness check it also runs.

use pg_conformance_fixtures::discover;
use pg_foma::capability::CharacteristicsProfile;
use pg_foma::oracle::OracleResult;
use pg_foma::plan::Plan;
use pg_foma::plan_interaction_coverage::{
    compute_interaction_coverage, fuzz_gate_group_reordering_for_grammar, gate_group_count,
    plan_and_profile, TupleStatus,
};

/// The coverage-report half, build-breaking; see this file's own top-doc for exactly what it does and does not assert.
#[test]
fn plan_interaction_coverage_has_no_uncovered_required_tuples() {
    let mut owned: Vec<(String, Plan, CharacteristicsProfile)> = Vec::new();

    for f in discover() {
        let xml = f.load_grammar_xml();
        let Ok(g) = pg_grammar::load(&xml) else {
            // A fixture this preview can't even load contributes nothing either way; diagnosing a grammar-load failure is conformance_fixtures_gate.rs's job, not this test's.
            continue;
        };
        let (plan, profile) = plan_and_profile(&g);
        owned.push((f.label(), plan, profile));
    }

    assert!(
        !owned.is_empty(),
        "must discover and load at least one conformance fixture"
    );

    let refs: Vec<(&str, &Plan, &CharacteristicsProfile)> = owned
        .iter()
        .map(|(label, plan, profile)| (label.as_str(), plan, profile))
        .collect();
    let report = compute_interaction_coverage(&refs);

    // Non-vacuity first: a report that silently shrank/grew the tuple set would make the build-breaking assertion below pass trivially, so `7`/`2` here are pinned literals, never derived from the functions they check.
    assert_eq!(
        report.required.len(),
        7,
        "must report on all 7 documented legal adjacency tuples -- a shrunk or grown \
         legal_adjacency_tuples() set would otherwise make the assertion below vacuous or miss a \
         real gap"
    );
    assert!(
        report.unexpected_tuples.is_empty(),
        "an adjacency tuple outside pg_foma::plan_interaction_coverage::legal_adjacency_tuples()'s \
         documented closed set was observed against the real corpus -- a genuine finding (a second \
         plan shape enumerate_default can produce that this module's doc doesn't yet name), not \
         something to silently drop: {:?}",
        report.unexpected_tuples
    );
    assert_eq!(
        report.retired.len(),
        2,
        "the two cited orthogonality proofs"
    );

    let covered_n = report
        .required
        .iter()
        .filter(|r| r.status == TupleStatus::Covered)
        .count();
    let uncovered = report.uncovered();

    eprintln!(
        "=== plan-node/subtree interaction coverage (Stage 3, ADR 0001) BUILD-BREAKING ===\n\
         Required adjacency tuples: {} total | {covered_n} covered | {} uncovered",
        report.required.len(),
        uncovered.len(),
    );
    for row in &report.required {
        eprintln!(
            "  {:?}: {:?} (tags: {:?}, covering fixtures: {:?})",
            row.tuple, row.status, row.tags, row.covering_fixtures
        );
    }
    eprintln!("RETIRED (proven orthogonal, never fuzzed):");
    for r in &report.retired {
        eprintln!("  {}: {}", r.label, r.evidence);
    }

    assert!(
        uncovered.is_empty(),
        "COVERAGE REGRESSION: {} required adjacency tuple(s) have zero covering fixture in \
         the discovered corpus (machine/conformance/** + conformance-staging/**): {:?}\n\
         Either an existing fixture regressed (its plan no longer realizes this tuple shape), or \
         the corpus lost its only fixture exercising this shape. Author or restore a \
         conformance fixture whose grammar structurally realizes this tuple -- see \
         legal_adjacency_tuples()'s own doc (this module's top-doc \"The tuple model\" section) for \
         what each of the 7 shapes requires. Full report above.",
        uncovered.len(),
        uncovered.iter().map(|r| &r.tuple).collect::<Vec<_>>()
    );
}

/// The fuzz-slice half; see this file's own top-doc for why this is a hard assertion, unlike the coverage report above.
#[test]
fn gate_group_reordering_agrees_on_every_multi_group_corpus_fixture() {
    let mut checked = 0usize;
    let mut skipped_single_group = 0usize;
    let mut skipped_unloadable = 0usize;

    for f in discover() {
        let xml = f.load_grammar_xml();
        let Ok(g) = pg_grammar::load(&xml) else {
            skipped_unloadable += 1;
            continue;
        };
        let (plan, _profile) = plan_and_profile(&g);
        if gate_group_count(&plan) < 2 {
            // Reordering a single group (or no Gate node at all) is a no-op, so skip it rather than padding the count with vacuous Agrees.
            skipped_single_group += 1;
            continue;
        }

        let words_yaml = f.load_words_yaml();
        let words: Vec<&str> = words_yaml
            .words
            .iter()
            .map(|w| w.word.as_str())
            .take(25) // bound compile+apply cost per fixture; this is a targeted regression slice,
            // not an exhaustive replay (module top-doc's own "targeted, not general" framing).
            .collect();
        if words.is_empty() {
            continue;
        }

        let (groups, result) =
            fuzz_gate_group_reordering_for_grammar(&g, &words).unwrap_or_else(|e| {
                panic!(
                    "{}: both the default plan and its permuted twin must build under an unbounded \
                     budget: {e:?}",
                    f.label()
                )
            });
        assert!(groups >= 2, "{}: gate_group_count contract", f.label());

        match result {
            OracleResult::Agree => {}
            OracleResult::Disagree {
                word,
                only_in_a,
                only_in_b,
                ..
            } => panic!(
                "{}: gate-group reordering must Agree (retirement #2's own order-invariance claim) \
                 -- got a real divergence at {word:?}: only_in_a={only_in_a:?}, \
                 only_in_b={only_in_b:?}. This is a genuine regression, never something to paper \
                 over.",
                f.label()
            ),
        }
        checked += 1;
    }

    eprintln!(
        "=== Gate-group-reordering fuzz slice (deliverable 5) === checked {checked} multi-group \
         fixtures, skipped {skipped_single_group} single/no-group fixtures, {skipped_unloadable} \
         unloadable"
    );
    assert!(
        checked > 0,
        "at least one discovered fixture must have >=2 Gate partition groups to exercise this \
         slice -- if this ever fails, either the corpus lost its only gated-multi-group fixture or \
         fixture discovery itself broke; either way it's worth knowing, not silently skipping"
    );
}
