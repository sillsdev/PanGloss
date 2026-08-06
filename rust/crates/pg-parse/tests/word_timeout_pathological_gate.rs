//! Regression guard for `--word-timeout-ms`: a genuinely slow, real `parse_word` call (not a synthetic loop, not a 0ms-deadline vacuous case) times out promptly, unmemoized, on a fixture of `k` homophonous one-shot suffix rules whose unmemoized unwind is genuinely combinatorial.

mod csharp_port_common;
use csharp_port_common::build_grammar_custom_lexicon;
use pg_parse::Morpher;
use std::time::{Duration, Instant};

/// A grammar with `k` distinct one-shot rules that all unapply the same literal suffix "n" from any word, one root entry, no phonological rules or templates, so the only work is the morphological cascade.
fn homophonous_suffix_grammar(k: usize) -> pg_grammar::model::Grammar {
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

/// Control (no `--word-timeout-ms`): a small bounded `--step-cap` alone must fire, proving this fixture is genuinely heavy without ever running the full unbounded case in the test suite.
#[test]
fn homophonous_suffix_fixture_is_genuinely_heavy_under_a_small_step_cap() {
    let g = homophonous_suffix_grammar(7);
    let word = format!("sag{}", "n".repeat(7));
    let m = Morpher::new(&g, 500).with_memo(false);
    let outcome = m.parse_word(&word);
    assert!(
        outcome.capped,
        "a step-cap of 500 (vs. the ~13699 steps an unbounded unmemoized run takes) must fire"
    );
    assert!(!outcome.timed_out, "no --word-timeout-ms was configured");
}

/// The actual regression guard: `--step-cap` uncapped (must never fire, proving the timeout is independent of it), `--word-timeout-ms` armed at 50ms, on the real pathological `parse_word` call from the control test above.
#[test]
fn fifty_ms_word_timeout_fires_promptly_on_a_genuinely_slow_parse() {
    let g = homophonous_suffix_grammar(7);
    let word = format!("sag{}", "n".repeat(7));
    let m = Morpher::new(&g, usize::MAX)
        .with_memo(false)
        .with_word_timeout(Some(Duration::from_millis(50)));

    let start = Instant::now();
    let outcome = m.parse_word(&word);
    let elapsed = start.elapsed();

    assert!(
        outcome.timed_out,
        "the 50ms deadline must fire during this slow parse"
    );
    assert!(
        !outcome.capped,
        "--step-cap was usize::MAX -- the step cap itself must never fire"
    );
    // Machine-speed-independent proxy for "aborted early": the deadline fires well short of the full unbounded unwind's step count.
    assert!(
        outcome.steps < 9_000,
        "aborted at {} steps, which should be far short of the ~13699 an unbounded unwind takes",
        outcome.steps
    );
    // A generous upper bound, well under the unbounded run's time but tolerant of a slow CI machine.
    assert!(
        elapsed < Duration::from_secs(2),
        "elapsed {elapsed:?} should stay a small fraction of the ~3.6s unbounded run, not balloon \
         toward it"
    );
}
