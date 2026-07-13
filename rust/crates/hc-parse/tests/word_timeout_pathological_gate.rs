//! The task's actual regression-guard shape for `--word-timeout-ms`: a genuinely slow, real
//! `parse_word` call (not a synthetic loop, not a 0ms-deadline vacuous case) times out promptly.
//!
//! `hc-rules/src/stratum.rs`'s `step_budget_timeout_tests` already prove the deadline mechanism
//! itself fires mid-run on a tight synthetic loop, and `word_timeout_gate.rs` already proves the
//! `Morpher`/`ParseOutcome` plumbing with a deterministic 0ms deadline. Neither of those exercises
//! whether `over_budget()` is actually reached *often enough inside a real, expensive analysis
//! cascade* for the wall-clock bound to be effective — the exact failure mode `--word-timeout-ms`
//! exists to avoid (per-step cost varies wildly; a hot inner stretch between checks could overshoot
//! a short deadline badly). This file is that missing case.
//!
//! ## The fixture
//! `k` distinct one-shot morphological rules (`multipleApplication` left at its DTD default of 1,
//! so each rule id is usable at most once per candidate's whole history), all on one `Unordered`
//! stratum, all matching the identical literal suffix "n" (homophonous, same mechanism as
//! `hc-rules/tests/stratum_gate.rs`'s `merge_equivalent_analyses_folds_homophonous_suffixes...`
//! test, just without the merge collapsing the *interior* search — `MergeEquivalentAnalyses` only
//! dedupes the stratum's final candidate set by shape, not the unmemoized cascade's per-step work).
//! Unapplying `d` stacked "n" suffixes from the root gives up to `k!/(k-d)!` distinct rule-id
//! orderings (`docs/phase2-completed/narrowing-budget-w8.md`'s "k!-Unordered stratum blow up"),
//! with `--memo=off` (the fair unmemoized baseline, `Morpher::with_memo(false)`) so the M6 memo
//! cannot prune the redundant work away.
//!
//! Measured once (debug profile, `cargo test`, matching this project's actual test runs) at
//! `k = d = 7`: unbounded (`--step-cap` effectively `usize::MAX`, no timeout) takes ~3.6s and
//! 13699 steps to fully enumerate all 5040 analyses. That number is not asserted directly here
//! (asserting it would make every test run pay the full 3.6s); the assertions below instead pin
//! the two behaviors that matter: (a) a small bounded step-cap alone proves the fixture doesn't
//! cheaply self-terminate, and (b) a 50ms wall-clock deadline aborts within a small fraction of
//! that ~3.6s, independent of the step cap (`--step-cap` stays `usize::MAX` for the timeout case).

mod csharp_port_common;
use csharp_port_common::build_grammar_custom_lexicon;
use hc_parse::Morpher;
use std::time::{Duration, Instant};

/// Build a grammar with `k` distinct rules `mrN0..mrN{k-1}`, each unapplying from any word ending
/// in the literal suffix "n" (the `MorphologicalInput`'s `OptionalSegmentSequence` matches any
/// nonempty prefix; the `MorphologicalOutput` appends `+n`, so unapplication finds and strips a
/// trailing "n" regardless of which rule id does it) — one root entry, no phonological rules, no
/// affix templates, so the only work is the morphological cascade itself.
fn homophonous_suffix_grammar(k: usize) -> hc_grammar::model::Grammar {
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

/// Control (no `--word-timeout-ms` at all): a small bounded `--step-cap` alone must fire
/// (`capped == true`) well before the full combinatorial unwind — proving this fixture is
/// genuinely heavy, not a fixture that happens to finish instantly regardless of any bound, without
/// ever running the ~3.6s unbounded case inside the test suite.
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

/// The actual regression guard: `--step-cap` uncapped (`usize::MAX` — must never fire, proving the
/// timeout is independent of it), `--word-timeout-ms` armed at 50ms, on the real pathological
/// `parse_word` call from the control test above. `over_budget()` is reached constantly inside
/// this cascade (a fresh check on every one of the ~13699+ (un)application attempts an unbounded
/// run would make), and — since the O1b fix (`hc-rules/src/stratum.rs`'s `StepBudget::over_budget`
/// doc) — the wall clock is sampled on *every* such check once a deadline is armed, so this is not
/// the "fires on the very first check" vacuous case `word_timeout_gate.rs`'s 0ms test covers. Note
/// this fixture's ~13699-step total (crossing many of the OLD 1024-tick cadence's boundaries)
/// happened to pass even under the pre-O1b cadence — it did not catch the O1b bug; the dedicated
/// regression guard for that is `hc-rules/src/stratum.rs`'s
/// `wall_clock_deadline_fires_even_when_total_ticks_never_reach_the_old_check_interval`, whose
/// fixture deliberately stays under 1024 total ticks.
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
    // Machine-speed-independent proxy for "aborted early": since the O1b fix, over_budget() samples
    // the wall clock on every call, so the deadline fires within about one (un)application attempt's
    // cost of the 50ms mark (~266µs/step in debug for this fixture) -- nowhere near the ~13699 steps
    // an unbounded unwind takes. The bound below stays generous (not tightened to the new, much
    // smaller expected overshoot) so this test doesn't become a timing-precision assertion.
    assert!(
        outcome.steps < 5_000,
        "aborted at {} steps, which should be far short of the ~13699 an unbounded unwind takes",
        outcome.steps
    );
    // Wall-clock assertion too (the task explicitly wants one). Measured baseline (see module doc):
    // the unbounded run takes ~3.6s. A generous upper bound -- comfortably above the ~250-350ms
    // this actually took when measured pre-O1b-fix (50ms deadline + up to one WALL_CLOCK_CHECK_
    // INTERVAL-sized overshoot at this fixture's per-step cost; post-fix the overshoot is much
    // smaller, bounded by a single tick's cost instead), but a small fraction of the ~3.6s an
    // unbounded run takes, and tolerant of a slow CI machine.
    assert!(
        elapsed < Duration::from_secs(2),
        "elapsed {elapsed:?} should stay a small fraction of the ~3.6s unbounded run, not balloon \
         toward it"
    );
}
