//! `--word-timeout-ms` end-to-end regression guard, at the `Morpher::parse_word` level
//! (`docs/budget-model.md`'s addendum; `hc-cli`'s `batch` subcommand is the CLI-facing consumer,
//! covered separately by `hc-cli/src/main.rs`'s own `#[cfg(test)]` module, which exercises the
//! flag-parsing + TSV-row-shape plumbing this file does not touch).
//!
//! Neither `Morpher::with_word_timeout` nor `ParseOutcome::timed_out` existed before this change,
//! so this whole file is "red" (does not compile) against the pre-change engine — the regression
//! guard the task asked for.

mod csharp_port_common;
use csharp_port_common::build_grammar;
use hc_parse::Morpher;
use std::time::{Duration, Instant};

/// The shared simple V-requiring `ed_suffix` grammar every other `csharp_port_morpher.rs` test in
/// this crate reuses (entry "32" = "sag", `MorpherTests.AnalyzeWord_CanAnalyzeUnordered_...`'s
/// shape) — chosen here purely because it is already known-good and cheap to build, not because it
/// is itself pathological; the deadline in each test below is what forces the timeout, not the
/// grammar's own difficulty (see `hc-rules/src/stratum.rs`'s `step_budget_timeout_tests` for the
/// dedicated "actually slow" stress case).
fn simple_grammar() -> hc_grammar::model::Grammar {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    build_grammar("", "", mrules, "mrEd", "")
}

/// `--word-timeout-ms` omitted (the default, `with_word_timeout` never called, mirroring every
/// pre-existing `Morpher::new(...)` call site in this crate) must behave exactly as before this
/// flag existed: full analyses, `timed_out` false, `capped` false (this grammar never approaches
/// `usize::MAX` steps).
#[test]
fn no_word_timeout_is_unaffected() {
    let g = simple_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let outcome = m.parse_word("sagd");
    assert!(!outcome.timed_out);
    assert!(!outcome.capped);
    assert!(
        !outcome.analyses.is_empty(),
        "sanity: the grammar still parses \"sagd\""
    );
}

/// `with_word_timeout(Some(0ms))` deterministically expires before construction even finishes
/// (`StepBudget::with_timeout` resolves the deadline to `Instant::now() + 0`), so the very first
/// `over_budget()` check inside `parse_word`'s analysis loop must catch it — independent of
/// `--step-cap`, which stays at `usize::MAX` (uncapped) here specifically to prove the timeout
/// does not depend on the step cap ever being reached. Deterministic (no timing race): any
/// `Instant::now()` sampled even a few nanoseconds after construction is `>=` the already-elapsed
/// deadline.
#[test]
fn zero_ms_word_timeout_fires_and_reports_no_analyses() {
    let g = simple_grammar();
    let m = Morpher::new(&g, usize::MAX).with_word_timeout(Some(Duration::from_millis(0)));

    let start = Instant::now();
    let outcome = m.parse_word("sagd");
    let elapsed = start.elapsed();

    assert!(
        outcome.timed_out,
        "a 0ms deadline must fire on the very first budget check"
    );
    assert!(
        !outcome.capped,
        "--step-cap was usize::MAX -- the cap itself must never fire"
    );
    assert!(
        outcome.analyses.is_empty(),
        "an immediately-expired budget must abort before finding any analyses"
    );
    // A generous bound: a real analysis of this grammar/word takes microseconds, so even with
    // scheduling noise this must stay far below "ran to completion normally".
    assert!(
        elapsed < Duration::from_secs(1),
        "elapsed {elapsed:?} should be near-instant"
    );
}

/// A `--word-timeout-ms` long enough to never fire is a no-op vs. omitting the flag entirely --
/// the two code paths (`deadline: None` vs. `deadline: Some(far_future)`) must agree exactly on
/// this word.
#[test]
fn generous_word_timeout_matches_no_timeout() {
    let g = simple_grammar();
    let plain = Morpher::new(&g, usize::MAX);
    let generous = Morpher::new(&g, usize::MAX).with_word_timeout(Some(Duration::from_secs(60)));

    let a = plain.parse_word("sagd");
    let b = generous.parse_word("sagd");
    assert_eq!(a.signature(), b.signature());
    assert!(!a.timed_out && !b.timed_out);
}
