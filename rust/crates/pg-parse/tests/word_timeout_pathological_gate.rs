//! Regression guard for `--word-timeout-ms`: a genuinely slow, real `parse_word` call (not a synthetic loop, not a 0ms-deadline vacuous case) times out promptly on a fixture of `k` homophonous one-shot suffix rules whose unwind is genuinely combinatorial.

#[path = "csharp_port_common/mod.rs"]
mod csharp_port_common;
use csharp_port_common::build_grammar_custom_lexicon;
use pg_parse::Morpher;
use std::time::{Duration, Instant};

/// A grammar with `k` distinct one-shot rules that all unapply the same literal suffix "n" from any word, one root entry, no phonological rules or templates, so the only work is the morphological cascade.
fn homophonous_suffix_grammar(k: usize) -> pg_grammar_model::model::Grammar {
    let mut mrule_defs = String::new();
    let mut ids = Vec::with_capacity(k);
    for i in 0..k {
        mrule_defs.push_str(&format!(
            r#"<MorphologicalRule id="mrN{i}"><Name>n_suffix_{i}</Name><MorphemeId>N{i}</MorphemeId>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subN{i}">
                  <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                  <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+n</PhoneticShape></InsertSegments></MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>"#
        ));
        ids.push(format!("mrN{i}"));
    }
    let lexicon = r#"<LexicalEntry id="eroot" partOfSpeech="posV"><MorphemeId>ROOT</MorphemeId>
      <Allomorphs><Allomorph id="aroot"><PhoneticShape>sag</PhoneticShape></Allomorph></Allomorphs>
    </LexicalEntry>"#;
    build_grammar_custom_lexicon(&mrule_defs, &ids.join(" "), lexicon)
}

/// The work an unbounded run of the fixture word takes, measured once with `--work-cap unbounded`.
const UNBOUNDED_WORK_STEPS: usize = 3_032_258;

/// Control (no `--word-timeout-ms`): a small bounded `--work-cap` alone must fire, proving this fixture is genuinely heavy without ever running the full unbounded case in the test suite.
#[test]
fn homophonous_suffix_fixture_is_genuinely_heavy_under_a_small_work_cap() {
    let g = homophonous_suffix_grammar(7);
    let word = format!("sag{}", "n".repeat(7));
    let m = Morpher::new(&g, 50_000);
    let outcome = m.parse_word(&word);
    assert!(
        outcome.capped,
        "a work cap of 50000 (vs. the work an unbounded run takes) must fire"
    );
    assert!(!outcome.timed_out, "no --word-timeout-ms was configured");
}

/// The actual regression guard: `--work-cap` uncapped (must never fire, proving the timeout is independent of it), `--word-timeout-ms` armed at 50ms, on the real pathological `parse_word` call from the control test above.
#[test]
fn fifty_ms_word_timeout_fires_promptly_on_a_genuinely_slow_parse() {
    let g = homophonous_suffix_grammar(7);
    let word = format!("sag{}", "n".repeat(7));
    let m = Morpher::new(&g, usize::MAX).with_word_timeout(Some(Duration::from_millis(50)));

    let start = Instant::now();
    let outcome = m.parse_word(&word);
    let elapsed = start.elapsed();

    assert!(
        outcome.timed_out,
        "the 50ms deadline must fire during this slow parse"
    );
    assert!(
        !outcome.capped,
        "the work cap was usize::MAX -- it must never fire"
    );
    // Machine-speed-independent proxy for "aborted early": the deadline fires well short of the full unbounded unwind's work.
    assert!(
        outcome.work_steps < UNBOUNDED_WORK_STEPS * 2 / 3,
        "aborted at {} work steps, which should be far short of the {UNBOUNDED_WORK_STEPS} an unbounded unwind takes",
        outcome.work_steps
    );
    // A generous upper bound, well under the unbounded run's time but tolerant of a slow CI machine.
    assert!(
        elapsed < Duration::from_secs(2),
        "elapsed {elapsed:?} should stay a small fraction of the ~3.6s unbounded run, not balloon \
         toward it"
    );
}

/// Slow (about 4 s): the work an unbounded run of the fixture word takes, which UNBOUNDED_WORK_STEPS must equal. Run it by hand after changing the grammar or the charging.
#[test]
#[ignore = "slow: runs the pathological word to completion"]
fn the_unbounded_work_constant_is_what_an_unbounded_run_takes() {
    let g = homophonous_suffix_grammar(7);
    let word = format!("sag{}", "n".repeat(7));
    let outcome = Morpher::new(&g, usize::MAX).parse_word(&word);
    assert_eq!(outcome.work_steps, UNBOUNDED_WORK_STEPS);
}
