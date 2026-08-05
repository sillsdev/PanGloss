//! THE conformance-coverage cross-check (ADR 0001, honest capability boundary):
//! **BUILD-BREAKING**.
//!
//! This test computes, for EVERY `CharacteristicKind` (`pg_foma::capability`) — not just the
//! `Proven` ("supported") subset — whether a covering, PASSING conformance fixture exists. It
//! prints the full report on every run **and fails the build** if any row lacks that evidence.
//!
//! A green build-breaking gate that can silently start lying is worse than an advisory report,
//! because the green light is what gets cited. See
//! `supported_construct_conformance_coverage_has_no_gaps`'s own doc for what had to be true before
//! the flip, for what this gate still does not assert, and for why row-level coverage is not the
//! same claim as configuration-level completeness. `pg_foma::conformance_coverage`'s module doc has
//! the mapping contract and what "passing" means here.
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
//! # What this file does NOT do
//! - Does not modify `machine/conformance/` fixtures or the conformance runner.
//! - Does not touch `conformance_fixtures_gate.rs` (`pg-parse`'s own full-suite oracle-replay
//!   gate) — this file re-derives its own "passing" replay independently (same oracle, same
//!   `pg_conformance_fixtures::discover`), rather than depending on that test's internals.

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

/// The ledger-wide cross-check, now **BUILD-BREAKING** -- this is the finish line, not a
/// follow-on cleanup step. Widened ledger-wide by G8, re-mapped by G9, flipped here.
///
/// # What flipping asserts, and what had to be true first
/// It asserts **zero `Uncovered` and zero `Unmappable` rows** across all 20 `CharacteristicKind`s,
/// each graded against a covering, passing conformance fixture.
///
/// The flip waited on three things, because a green build-breaking gate that can silently start
/// lying is worse than an advisory report — the green light is what gets cited:
/// 1. **G9** — `Unmappable` had to reach zero; 4 `constructs.txt` rows were missing entirely
///    (sillsdev/machine#465).
/// 2. **`tests/exercises_tag_liveness.rs`** — three fixtures tagged CHARACTERISTIC NAMES where a
///    row id is required, so their evidence silently counted for nothing. An unknown tag is now a
///    hard error rather than `constructs.txt`'s own documented "soft warning".
/// 3. **`tests/structural_witness_gate.rs`** — four row ids are each mapped by two characteristics,
///    so the finer one could report `Covered` on the coarser sibling's evidence. Each now has a
///    mechanized grammar-shape witness. Full reasoning:
///    `docs/conformance/shared-construct-id-analysis.md`.
///
/// Plus `tests/coverage_citation_liveness.rs`, which keeps the curated containment citations from
/// becoming dangling pointers.
///
/// # What it still does NOT assert
/// - That a fixture tags the RIGHT construct. A tag claiming something the fixture does not
///   exercise stays a human-authoring risk; no string or shape check closes it in general.
/// - That `Covered` means `Admit`. Ten rows are `ConfigPredicate` and three `ConfirmOnly`;
///   `Covered` means "evidenced at its own disposition" (§D1: `ConfirmOnly → Admit` is a separate,
///   optional track).
/// - That every CONFIGURATION inside a covered row is closed. Row-level coverage and
///   configuration-level completeness are different questions, and §D7 requires both — several
///   rows still have open configuration splits tracked elsewhere in this crate's conformance docs.
///
/// The full report still prints on every run: a failure must say WHICH row regressed and how, not
/// merely that a count moved.
#[test]
fn supported_construct_conformance_coverage_has_no_gaps() {
    let covered = passing_covered_constructs();
    let covered_refs: HashSet<&str> = covered.iter().map(String::as_str).collect();
    let report = supported_coverage_report(&covered_refs);

    // Non-vacuity first: a report that enumerated nothing would make every assertion below pass
    // trivially, which is the failure mode this whole subsystem's gates are written against.
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
    // Staged-flip preview -- split the non-Covered set by disposition, since a real flip should
    // gate Proven (hard error) ahead of ConfigPredicate/ConfirmOnly (also required, but stageable).
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
    // The gate. Reported by disposition rather than as one undifferentiated count, so a failure
    // says what KIND of evidence is missing and therefore what would fix it.
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
