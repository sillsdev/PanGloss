//! Integration test for tree-structured node/subtree interaction coverage over the reified compilation plan (rather than pairwise covering arrays over raw grammar "knobs"); build-breaking if any required `AdjacencyTuple` is `Uncovered`.
//! See docs/research/pg-foma-plan-interaction-coverage-gate-notes.md for what this gate does and does not assert.

use pg_conformance_fixtures::discover;
use pg_foma::capability::CharacteristicsProfile;
use pg_foma::plan::Plan;
use pg_foma_backend::plan_interaction_coverage::{
    compute_interaction_coverage, plan_and_profile, TupleStatus,
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

    // Non-vacuity first: a report that silently shrank/grew the tuple set would make the build-breaking assertion below pass trivially, so `7`/`1` here are pinned literals, never derived from the functions they check.
    assert_eq!(
        report.required.len(),
        7,
        "must report on all 7 documented legal adjacency tuples -- a shrunk or grown \
         legal_adjacency_tuples() set would otherwise make the assertion below vacuous or miss a \
         real gap"
    );
    assert!(
        report.unexpected_tuples.is_empty(),
        "an adjacency tuple outside pg_foma_backend::plan_interaction_coverage::legal_adjacency_tuples()'s \
         documented closed set was observed against the real corpus -- a genuine finding (a second \
         plan shape enumerate_default can produce that this module's doc doesn't yet name), not \
         something to silently drop: {:?}",
        report.unexpected_tuples
    );
    assert_eq!(report.retired.len(), 1, "the one cited orthogonality proof");

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
    eprintln!("RETIRED (proven orthogonal):");
    for r in &report.retired {
        eprintln!("  {}: {}", r.label, r.evidence);
    }

    assert!(
        uncovered.is_empty(),
        "COVERAGE REGRESSION: {} required adjacency tuple(s) have zero covering fixture in \
         the discovered corpus (conformance/** + conformance-staging/**): {:?}\n\
         Either an existing fixture regressed (its plan no longer realizes this tuple shape), or \
         the corpus lost its only fixture exercising this shape. Author or restore a \
         conformance fixture whose grammar structurally realizes this tuple -- see \
         legal_adjacency_tuples()'s own doc (this module's top-doc \"The tuple model\" section) for \
         what each of the 7 shapes requires. Full report above.",
        uncovered.len(),
        uncovered.iter().map(|r| &r.tuple).collect::<Vec<_>>()
    );
}
