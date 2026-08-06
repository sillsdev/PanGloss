//! `--word-timeout-ms` end-to-end regression guard, at the `Morpher::parse_word` level; `pg-cli`'s `batch` flag-parsing/TSV plumbing is covered separately by `pg-cli/src/main.rs`'s own tests.

mod csharp_port_common;
use csharp_port_common::build_grammar;
use pg_parse::Morpher;
use std::time::{Duration, Instant};

/// A known-good, cheap-to-build grammar; the deadline in each test below is what forces the timeout, not the grammar's own difficulty.
fn simple_grammar() -> pg_grammar::model::Grammar {
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

/// `--word-timeout-ms` omitted must behave exactly as if the flag never existed: full analyses, `timed_out` false, `capped` false.
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

/// `with_word_timeout(Some(0ms))` deterministically expires before construction finishes, so the first `over_budget()` check must catch it, independent of `--step-cap` (left uncapped here).
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
    // A generous bound: a real analysis takes microseconds, so this must stay far below "ran to completion normally" even with scheduling noise.
    assert!(
        elapsed < Duration::from_secs(1),
        "elapsed {elapsed:?} should be near-instant"
    );
}

/// A `--word-timeout-ms` long enough to never fire must be a no-op vs. omitting the flag entirely: the two code paths must agree exactly on this word.
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
