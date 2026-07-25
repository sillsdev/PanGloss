//! NON-BLOCKING PREVIEW of task 5.1 (`openspec/changes/add-capability-characteristics-check`, ADR
//! 0001, `docs/adr/0001-honest-capability-boundary.md`): the conformance-coverage cross-check.
//!
//! This test computes which `Proven` ("supported") `CharacteristicKind`s (`pg_foma::capability`)
//! lack a covering, PASSING conformance fixture, and PRINTS the full advisory report — it asserts
//! only that the check RUNS and produces output, **never that the gap set is empty**. Turning this
//! into a hard gate is task 5.1's own later, deliberate step (mechanically: replace this file's
//! non-assertion with `assert!(gaps.is_empty(), ...)`) — see `pg_foma::conformance_coverage`'s own
//! module doc for the mapping contract, why this is deferred, and what "passing" means here.
//!
//! # Home
//! `pg-foma` (this crate) because `capability.rs`'s disposition table — the registry side of the
//! cross-check — lives here. The fixture-loading + oracle-replay glue below is a dev-dependency-
//! only concern (`pg-conformance-fixtures` + `pg-parse`, already ordinary dependencies of this
//! crate's own ecosystem), kept OUT of `src/conformance_coverage.rs` itself so that module stays a
//! pure, cheaply unit-testable function over a caller-supplied covered-construct set (see this
//! crate's own `capability.rs`/`capability_entry.rs` split for the same "pure core, wired-up test/
//! entry-point glue lives at the edge" pattern).
//!
//! # Non-blocking, additive: what this file does NOT do
//! - Does not modify `machine/conformance/` fixtures or the conformance runner.
//! - Does not touch `conformance_fixtures_gate.rs` (`pg-parse`'s own full-suite oracle-replay
//!   gate) — this file re-derives its own "passing" replay independently (same oracle, same
//!   `pg_conformance_fixtures::discover`), rather than depending on that test's internals.
//! - Does not fail if `supported_uncovered` is non-empty — see the module doc above.

use std::collections::HashSet;

use pg_conformance_fixtures::discover;
use pg_foma::conformance_coverage::{supported_coverage_report, CoverageStatus};
use pg_parse::Morpher;

/// Replays every discovered fixture (`machine/conformance/**` + `conformance-staging/**`) against
/// `pg_parse::Morpher` — the same oracle `pg-parse`'s own `conformance_fixtures_gate.rs` runs the
/// full suite against — and collects the `exercises:` construct identifiers named by every
/// word/parse whose engine output CURRENTLY MATCHES the fixture's declared ground-truth signature.
/// A currently-FAILING word's `exercises:` tags do not count toward coverage — exactly the
/// "passing" qualifier ADR 0001's cross-check requires (see `pg_foma::conformance_coverage`'s own
/// "What 'passing' means here" section).
fn passing_covered_constructs() -> HashSet<String> {
    let mut covered = HashSet::new();

    for f in discover() {
        let words_yaml = f.load_words_yaml();
        if words_yaml.skip_in_generic_replay().is_some() {
            continue; // expect_crash / budget_ms fixtures: no signature ground truth to replay
        }

        let xml = f.load_grammar_xml();
        let Ok(grammar) = pg_grammar::load(&xml) else {
            // A fixture this preview can't even load contributes no coverage either way -- not
            // this test's job to diagnose a grammar-load failure (conformance_fixtures_gate.rs
            // already gates that for real).
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

/// The advisory cross-check itself. See this file's own top-doc for exactly what it does and does
/// not assert.
#[test]
fn supported_construct_conformance_coverage_report_advisory() {
    let covered = passing_covered_constructs();
    let covered_refs: HashSet<&str> = covered.iter().map(String::as_str).collect();
    let report = supported_coverage_report(&covered_refs);

    // The only real assertion this test makes: the mechanism runs and reports something for every
    // Proven CharacteristicKind. It must NEVER assert gaps == 0 -- see the module doc.
    assert!(
        !report.is_empty(),
        "the coverage report must enumerate at least one Proven (\"supported\") \
         CharacteristicKind"
    );

    let mut covered_n = 0usize;
    let mut uncovered = Vec::new();
    let mut unmappable = Vec::new();
    for row in &report {
        match row.status {
            CoverageStatus::Covered => covered_n += 1,
            CoverageStatus::Uncovered => uncovered.push(row.kind),
            CoverageStatus::Unmappable => unmappable.push(row.kind),
        }
    }

    eprintln!(
        "=== ADVISORY conformance-coverage cross-check (task 5.1 preview, ADR 0001) ===\n\
         supported-but-uncovered will become a hard CI gate later -- this run does NOT fail the \
         build on gaps.\n\
         Proven (\"supported\") CharacteristicKinds: {} total | {covered_n} covered | \
         {} uncovered | {} unmappable (no constructs.txt id exists for this kind at all)",
        report.len(),
        uncovered.len(),
        unmappable.len(),
    );
    for row in &report {
        eprintln!(
            "  {:?}: {:?} (mapped construct ids: {:?})",
            row.kind, row.status, row.construct_ids
        );
    }
    if !uncovered.is_empty() {
        eprintln!(
            "SUPPORTED-BUT-UNCOVERED (advisory today; task 5.1 proper will hard-fail CI on this): \
             {uncovered:?}"
        );
    }
    if !unmappable.is_empty() {
        eprintln!(
            "UNMAPPABLE (mapping-contract gap: no constructs.txt row corresponds to this \
             CharacteristicKind at all): {unmappable:?}"
        );
    }
}
