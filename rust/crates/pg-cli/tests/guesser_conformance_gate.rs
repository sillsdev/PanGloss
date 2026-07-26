//! HC-rust port gap G3 closure gate (`docs/hermitcrab-rust-port-audit.md` sec 2/3 item 1;
//! `docs/p11-guesser-api-design.md`): drives the staged
//! `conformance-staging/edge-cases/guesser-pattern-root-fallback/` fixture directly, through the
//! same `pg_parse::Morpher`/`ParseOptions` surface `pg-cli`'s own `--guess` flag uses, proving:
//!
//! 1. guesser OFF reproduces the pre-existing (empty) result for an out-of-lexicon word whose
//!    only "lexical entry" is a guess-only pattern;
//! 2. guesser ON analyzes that same word and marks every resulting analysis guessed; and
//! 3. the ordinary control root ("kad") never gets guessed, on or off -- the guesser only ever
//!    fires on a genuine total miss.
//!
//! This is the "guesser exercised" half of the fixture's own gate; the generic, always-on
//! `pg-parse/tests/conformance_fixtures_gate.rs::all_discovered_fixtures_match_oracle` replay
//! covers the guess-OFF/adapter-visible half (it discovers this fixture too, replays "kad"
//! normally, and correctly skips "gag"/"gagd" via `WordEntry::adapter_visible()` since every one
//! of their parses carries `guess: true` -- see `STAGING.md`).

use pg_conformance_fixtures::{discover, Root};
use pg_parse::{AnalysisProvenance, Morpher, ParseOptions};

fn fixture_grammar() -> pg_grammar::model::Grammar {
    let fixtures = discover();
    let f = fixtures
        .iter()
        .find(|f| {
            f.root == Root::Staging
                && f.category == "edge-cases"
                && f.name == "guesser-pattern-root-fallback"
        })
        .expect(
            "conformance-staging/edge-cases/guesser-pattern-root-fallback must be discoverable",
        );
    pg_grammar::load(&f.load_grammar_xml())
        .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", f.label()))
}

/// Gate 1 + gate 4: guesser OFF (the default -- `ParseOptions::default()`, and equally
/// `Morpher::parse_word`'s pre-existing entry point) must reproduce the pre-existing empty result
/// for both out-of-lexicon words. Also the "flag default is OFF" pin: `ParseOptions::default()` is
/// exactly what `pangloss batch`/`parse` build when `--guess` is never passed.
#[test]
fn guess_off_is_the_default_and_reproduces_the_pre_existing_empty_result() {
    let g = fixture_grammar();
    let m = Morpher::new(&g, usize::MAX);

    for word in ["gag", "gagd"] {
        let via_default_opts = m.parse_word_opts(word, &ParseOptions::default());
        let via_parse_word = m.parse_word(word);
        assert_eq!(
            via_default_opts.signature(),
            "-",
            "guess off must find nothing for out-of-lexicon word {word:?}"
        );
        assert!(
            !via_default_opts.guessed,
            "guess off must never set the outcome-level guessed flag for {word:?}"
        );
        assert!(
            via_default_opts.structured.is_empty(),
            "guess off must produce zero structured analyses for {word:?}"
        );
        // `ParseOptions::default()` and the plain `parse_word` entry point must be byte-identical
        // (P11 sec 4.1) -- proving `--guess` omitted is truly a no-op, not just "usually agrees".
        assert_eq!(via_default_opts.signature(), via_parse_word.signature());
        assert_eq!(via_default_opts.guessed, via_parse_word.guessed);
    }
}

/// Gate 2: guesser ON analyzes both out-of-lexicon words and marks every resulting analysis
/// guessed, with the exact signatures this fixture's `words.yaml`/`STAGING.md` pin (transcribed
/// directly from a live engine run, per `STAGING.md`'s Verification section).
#[test]
fn guess_on_analyzes_the_out_of_lexicon_words_and_marks_them_guessed() {
    let g = fixture_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default().with_guess_root(true);

    let gag = m.parse_word_opts("gag", &opts);
    assert!(gag.guessed, "\"gag\" must be marked guessed with guessing on");
    assert_eq!(gag.signature(), "gag|gag");
    assert_eq!(gag.structured.len(), 1);
    assert!(gag.structured[0].guessed);
    assert_eq!(gag.structured[0].provenance, AnalysisProvenance::Guessed);

    let gagd = m.parse_word_opts("gagd", &opts);
    assert!(
        gagd.guessed,
        "\"gagd\" must be marked guessed with guessing on"
    );
    assert_eq!(gagd.signature(), "gag+PAST|gag+?d;gagd|gagd");
    assert_eq!(gagd.structured.len(), 2);
    assert!(gagd.structured.iter().all(|s| s.guessed));
    assert!(gagd
        .structured
        .iter()
        .all(|s| s.provenance == AnalysisProvenance::Guessed));
}

/// Gate 3 (the negative control this fixture exists to pin, per `STAGING.md`): the ordinary
/// lexical root "kad" analyzes identically with guessing on or off, and is NEVER marked guessed --
/// proving the guesser only fires on a genuine total miss, never overriding or duplicating a real
/// lexical hit.
#[test]
fn control_root_never_guessed_on_or_off() {
    let g = fixture_grammar();
    let m = Morpher::new(&g, usize::MAX);

    let off = m.parse_word_opts("kad", &ParseOptions::default());
    let on = m.parse_word_opts("kad", &ParseOptions::default().with_guess_root(true));

    for outcome in [&off, &on] {
        assert_eq!(outcome.signature(), "KAD|kad");
        assert!(!outcome.guessed);
        assert_eq!(outcome.structured.len(), 1);
        assert!(!outcome.structured[0].guessed);
        assert_eq!(outcome.structured[0].provenance, AnalysisProvenance::Grammar);
    }
    assert_eq!(off.signature(), on.signature());
}
