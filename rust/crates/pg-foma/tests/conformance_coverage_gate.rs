//! THE conformance-coverage cross-check (ADR 0001, honest capability boundary), BUILD-BREAKING: for every `CharacteristicKind` (not just the `Proven` subset), asserts a covering, PASSING conformance fixture exists, re-deriving its own oracle replay independently of `conformance_fixtures_gate.rs` rather than depending on that test's internals.

use std::collections::HashSet;

use pg_conformance_fixtures::discover;
use pg_foma::conformance_coverage::{supported_coverage_report, CoverageStatus};
use pg_parse::Morpher;

/// Replays every discovered fixture against `pg_parse::Morpher` and collects `exercises:` construct identifiers only from words whose engine output CURRENTLY MATCHES the fixture's ground-truth signature -- the "passing" qualifier ADR 0001's cross-check requires.
fn passing_covered_constructs() -> HashSet<String> {
    let mut covered = HashSet::new();

    for f in discover() {
        let words_yaml = f.load_words_yaml();
        if words_yaml.skip_in_generic_replay().is_some() {
            continue; // expect_crash / budget_ms fixtures: no signature ground truth to replay
        }

        let xml = f.load_grammar_xml();
        let Ok(grammar) = pg_grammar::load(&xml) else {
            // A fixture this preview can't even load contributes no coverage either way -- `conformance_fixtures_gate.rs` already gates load failures for real.
            continue;
        };
        let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

        for w in &words_yaml.words {
            if !w.adapter_visible() {
                continue; // self-check-only (guess:true parse), PROTOCOL.md section 3
            }
            let outcome = morpher.parse_word(&w.word);
            if w.expect_skip {
                continue; // SKIPPED words carry no meaningful "matched ground truth" signal here
            }
            if outcome.invalid_shape {
                continue; // unexpectedly SKIPPED -> not passing
            }
            if outcome.signature() != w.expected_signature() {
                continue; // mismatch -> not passing; this word's exercises: tags don't count
            }
            for c in &w.exercises {
                covered.insert(c.clone());
            }
            for p in &w.parses {
                for c in &p.exercises {
                    covered.insert(c.clone());
                }
            }
        }
    }

    covered
}

/// The ledger-wide cross-check: zero `Uncovered` and zero `Unmappable` rows across all `CharacteristicKind`s, each graded against a covering, passing conformance fixture -- but row-level coverage is not configuration-level completeness, and `Covered` is not `Admit`.
/// See `docs/research/pg-foma-conformance-coverage-gate-notes.md` for what had to be true before this gate could be build-breaking and for what it still does not assert.
#[test]
fn supported_construct_conformance_coverage_has_no_gaps() {
    let covered = passing_covered_constructs();
    let covered_refs: HashSet<&str> = covered.iter().map(String::as_str).collect();
    let report = supported_coverage_report(&covered_refs);

    // Non-vacuity first: a report that enumerated nothing would make every assertion below pass trivially.
    assert_eq!(
        report.len(),
        pg_foma::capability::CharacteristicKind::ALL.len(),
        "the coverage report must enumerate EVERY CharacteristicKind ({} expected, {} reported) -- \
         a short report would make the build-breaking assertions below vacuous",
        pg_foma::capability::CharacteristicKind::ALL.len(),
        report.len()
    );

    let mut covered_n = 0usize;
    let mut uncovered = Vec::new();
    let mut unmappable = Vec::new();
    // Split the non-Covered set by disposition: Proven is a hard error, ConfigPredicate/ConfirmOnly are also required but reported separately.
    let mut proven_gaps = Vec::new();
    let mut config_or_confirm_gaps = Vec::new();
    for row in &report {
        match row.status {
            CoverageStatus::Covered => covered_n += 1,
            CoverageStatus::Uncovered => uncovered.push(row.kind),
            CoverageStatus::Unmappable => unmappable.push(row.kind),
        }
        if row.status != CoverageStatus::Covered {
            match row.disposition {
                pg_foma::capability::Disposition::Proven => proven_gaps.push(row.kind),
                pg_foma::capability::Disposition::ConfigPredicate
                | pg_foma::capability::Disposition::ConfirmOnly => {
                    config_or_confirm_gaps.push(row.kind)
                }
            }
        }
    }

    eprintln!(
        "=== conformance-coverage cross-check (ADR 0001; ledger-wide per G8, remapped per G9) \
         BUILD-BREAKING ===\n\
         CharacteristicKinds: {} total | {covered_n} covered | {} uncovered | {} unmappable",
        report.len(),
        uncovered.len(),
        unmappable.len(),
    );
    for row in &report {
        eprintln!(
            "  {:?}: disposition={:?} status={:?} (mapped construct ids: {:?})",
            row.kind, row.disposition, row.status, row.construct_ids
        );
    }
    // The gate. Reported by disposition, not one undifferentiated count, so a failure says what KIND of evidence is missing.
    assert!(
        unmappable.is_empty(),
        "MAPPING-CONTRACT REGRESSION: {} CharacteristicKind(s) have no constructs.txt row at all: \
         {unmappable:?}\n\
         This is a vocabulary gap, not a fixture gap -- a row must be added upstream (see G9 / \
         sillsdev/machine#465 for the precedent) and mapped in \
         `conformance_coverage::construct_ids_for`. Full report above.",
        unmappable.len()
    );
    assert!(
        proven_gaps.is_empty(),
        "COVERAGE REGRESSION (Proven): {proven_gaps:?} are admission-filtered unconditionally yet \
         have no passing conformance fixture tagging their construct id.\n\
         A Proven construct with no covering fixture is the strongest form of this gap: the \
         compiler admits it with no evidence. Either a fixture regressed (check whether its words \
         still pass -- a FAILING word's exercises: tags do not count), or a tag is not a literal \
         constructs.txt row id (tests/exercises_tag_liveness.rs catches that specifically). Full \
         report above."
    );
    assert!(
        config_or_confirm_gaps.is_empty(),
        "COVERAGE REGRESSION (ConfigPredicate/ConfirmOnly): {config_or_confirm_gaps:?} are \
         compiled and relied upon, and ADR 0001 requires them evidenced too -- ConfirmOnly means \
         'the oracle prunes over-generation', never 'no fixture needed'. Same two likely causes as \
         the Proven case above. Full report above."
    );
    assert_eq!(
        covered_n,
        report.len(),
        "internal inconsistency: {covered_n} of {} rows are Covered yet every gap list above is \
         empty -- the status/disposition split in this test has drifted from CoverageStatus",
        report.len()
    );
}
