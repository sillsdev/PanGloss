//! ADVISORY-FIRST integration test for Stage 3 of `openspec/changes/
//! add-pairwise-grammar-interaction-coverage` (the REFRAMED design: tree-structured node/subtree
//! interaction coverage over the reified compilation plan, not pairwise covering arrays over raw
//! grammar "knobs" -- see that change's `design.md`/`proposal.md`/`specs/grammar-interactions/
//! spec.md`, and `docs/adr/0001-honest-capability-boundary.md`).
//!
//! Computes [`pg_foma::plan_interaction_coverage::compute_interaction_coverage`]'s report over every
//! discoverable conformance fixture (`pg_conformance_fixtures::discover()` -- `machine/conformance/
//! **` + `conformance-staging/**`), and PRINTS it -- mirroring `conformance_coverage_gate.rs`'s own
//! non-blocking discipline exactly: this test asserts only that the mechanism RUNS and reports
//! something non-empty, **never that `uncovered().is_empty()`**. See
//! `pg_foma::plan_interaction_coverage`'s own module doc for the tuple model, the orthogonality-
//! retirement evidence, and why the build-breaking flip is deferred (the corpus does not cover
//! everything yet -- e.g. no discovered fixture declares a circumfix/dropped-material construct
//! today, so the `StructuralCompositeMarker` tuple is expected `Uncovered`).
//!
//! # The fuzz slice (deliverable 5) -- a HARD assertion, not advisory
//! For every discovered fixture whose plan's `Gate` node has >=2 partition groups,
//! [`pg_foma::plan_interaction_coverage::fuzz_gate_group_reordering_for_grammar`] builds the
//! grammar's default plan and its `permute_gate_groups` twin and asserts `differential_oracle`
//! reports `Agree`. Unlike the coverage report above, THIS assertion is NOT advisory: it re-confirms
//! a mechanized correctness property (Gate-group order-invariance, `crate::gate`'s own "why the
//! union is safe here" argument plus union commutativity) on every REAL corpus grammar, not a
//! coverage-completeness claim that is expected to have gaps today. A real disagreement here would
//! be a genuine regression, never something to paper over.
//!
//! # Non-blocking, additive: what this file does NOT do
//! - Does not modify `machine/conformance/` fixtures, `conformance-staging/`, or any production
//!   compile path (`plan.rs`/`enumerate.rs`/`build.rs`/`oracle.rs`/`capability.rs` are read/reused
//!   only, per this task's own hard rule).
//! - Does not touch `conformance_coverage_gate.rs`/`conformance_fixtures_gate.rs` -- a separate,
//!   independent cross-check over a different axis (conformance-construct coverage vs. plan-node-
//!   interaction coverage).

use pg_conformance_fixtures::discover;
use pg_foma::capability::CharacteristicsProfile;
use pg_foma::oracle::OracleResult;
use pg_foma::plan::Plan;
use pg_foma::plan_interaction_coverage::{
    compute_interaction_coverage, fuzz_gate_group_reordering_for_grammar, gate_group_count,
    plan_and_profile, TupleStatus,
};

/// The advisory coverage-report half. See this file's own top-doc for exactly what it does and does
/// not assert.
#[test]
fn plan_interaction_coverage_report_advisory() {
    let mut owned: Vec<(String, Plan, CharacteristicsProfile)> = Vec::new();

    for f in discover() {
        let xml = f.load_grammar_xml();
        let Ok(g) = pg_grammar::load(&xml) else {
            // A fixture this preview can't even load contributes nothing either way -- not this
            // test's job to diagnose a grammar-load failure (conformance_fixtures_gate.rs already
            // gates that for real).
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

    // The only real assertions this test makes: the mechanism runs, reports something for every
    // documented legal tuple, and never observes a tuple outside that documented set. It must NEVER
    // assert uncovered() is empty -- see the module doc.
    assert_eq!(
        report.required.len(),
        7,
        "must report on all 7 documented legal adjacency tuples"
    );
    assert!(
        report.unexpected_tuples.is_empty(),
        "an adjacency tuple outside pg_foma::plan_interaction_coverage::legal_adjacency_tuples()'s \
         documented closed set was observed against the real corpus -- a genuine finding (a second \
         plan shape enumerate_default can produce that this module's doc doesn't yet name), not \
         something to silently drop: {:?}",
        report.unexpected_tuples
    );
    assert_eq!(report.retired.len(), 2, "the two cited orthogonality proofs");

    let covered_n = report
        .required
        .iter()
        .filter(|r| r.status == TupleStatus::Covered)
        .count();
    let uncovered = report.uncovered();
    let unsupported_n = report
        .required
        .iter()
        .filter(|r| r.status == TupleStatus::ContainsUnsupported)
        .count();

    eprintln!(
        "=== ADVISORY plan-node/subtree interaction coverage (Stage 3 preview, ADR 0001) ===\n\
         uncovered-required will NOT become a hard CI gate in this step -- this run does NOT fail \
         the build on gaps.\n\
         Required adjacency tuples: {} total | {covered_n} covered | {} uncovered | \
         {unsupported_n} contains-unsupported",
        report.required.len(),
        uncovered.len(),
    );
    for row in &report.required {
        eprintln!(
            "  {:?}: {:?} (tags: {:?}, covering fixtures: {:?}, unsupported-instance fixtures: {:?})",
            row.tuple, row.status, row.tags, row.covering_fixtures, row.unsupported_fixtures
        );
    }
    if !uncovered.is_empty() {
        eprintln!(
            "UNCOVERED REQUIRED TUPLES (advisory today; a later step may hard-fail CI on this): {:?}",
            uncovered.iter().map(|r| &r.tuple).collect::<Vec<_>>()
        );
    }
    eprintln!("RETIRED (proven orthogonal, never fuzzed):");
    for r in &report.retired {
        eprintln!("  {}: {}", r.label, r.evidence);
    }
}

/// The fuzz-slice half (deliverable 5). See this file's own top-doc for why this IS a hard
/// assertion, unlike the coverage report above.
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
            // Reordering a single group (or a plan with no Gate node at all) is a no-op -- not a
            // real exercise of retirement #2's own claim (module top-doc), so skip it rather than
            // padding the count with vacuous Agrees.
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

        let (groups, result) = fuzz_gate_group_reordering_for_grammar(&g, &words)
            .unwrap_or_else(|e| {
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
