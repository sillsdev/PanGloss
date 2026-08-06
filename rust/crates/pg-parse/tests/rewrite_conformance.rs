//! Conformance replay for the oracle-verified rewrite-rule fixtures under `rust/conformance/rewrite/`: load each fixture's `grammar.xml` as authored, parse every word, and check `Morpher::parse_word(...).signature()` against that fixture's oracle-generated `expected.tsv`.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::Morpher;

fn fixture_path(name: &str, file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/rewrite")
        .join(name)
        .join(file)
}

fn load_fixture(name: &str) -> pg_grammar::model::Grammar {
    let xml = std::fs::read_to_string(fixture_path(name, "grammar.xml")).expect("read grammar.xml");
    load(&xml).unwrap_or_else(|e| panic!("{name}: grammar failed to load: {e}"))
}

/// Self-skip guard: `rust/conformance/` isn't a submodule yet, so `--include-ignored` runs must not panic on the missing directory.
fn have_fixture(name: &str) -> bool {
    fixture_path(name, "grammar.xml").exists()
}

/// `rust/conformance/rewrite/word-initial-epenthesis/expected.tsv`: bare-root word-initial epenthesis, pinning `syn_epenthesis`'s site-0 gap plus `compile_lane_fst`'s multi-node RtL analysis-target ordering.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn word_initial_epenthesis_matches_oracle() {
    if !have_fixture("word-initial-epenthesis") {
        eprintln!("skipping: rust/conformance/rewrite/word-initial-epenthesis not present on disk");
        return;
    }
    let g = load_fixture("word-initial-epenthesis");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [
        ("taba", "|taba"), // strip word-initial ta on analysis, re-epenthesize on confirm
        ("ba", "-"),       // obligatory epenthesis: the bare surface can never round-trip
        ("bata", "|bata"), // no-fire control: right env needs exactly `C V #`
        ("tabata", "-"),   // env-respecting negative: `ta` before `C V C V` must not strip
    ];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "word-initial-epenthesis word {word:?}"
        );
    }
}

/// `rust/conformance/rewrite/deletion-reinsertion/expected.tsv`: multi-site deletion unapplication, where one analysis pass inserts the deleted segment at every matching site as optional nodes, so root lookup's consume-or-skip branching reaches every per-subset lexical entry.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn deletion_reinsertion_matches_oracle() {
    if !have_fixture("deletion-reinsertion") {
        eprintln!("skipping: rust/conformance/rewrite/deletion-reinsertion not present on disk");
        return;
    }
    let g = load_fixture("deletion-reinsertion");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [
        // b u (i) b u (i): skip/skip=19, skip/consume=24, consume/skip=25, consume/consume=26
        ("bubu", "19|bubu;24|bubu;25|bubu;26|bubu"),
        ("buibu", "-"), // surface i after a high V can never survive obligatory synthesis
        ("bibu", "-"),  // analysis candidates (bibu, biibu, ...) match no lexical entry
        ("buiibuii", "-"), // reachable only as its own analysis candidate; synthesis yields bubu
    ];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "deletion-reinsertion word {word:?}"
        );
    }
}

/// `rust/conformance/rewrite/multiplesegment-deletion-composition/expected.tsv`: a 2-segment feature-change target composed in the same stratum with a later-listed pure-deletion rule that never fires, since C#'s reverse-listed-order analysis convention unapplies the deletion rule first regardless.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn multiplesegment_deletion_composition_matches_oracle() {
    if !have_fixture("multiplesegment-deletion-composition") {
        eprintln!(
            "skipping: rust/conformance/rewrite/multiplesegment-deletion-composition not present on disk"
        );
        return;
    }
    let g = load_fixture("multiplesegment-deletion-composition");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [
        ("buuubuuu", "|buuubuuu"), // both HighVowel pairs preceded by a BackRndVowel -> root 27
        ("buiibuii", "-"),         // surface can't round-trip (rule1 always narrows ii after u)
        ("buiibuuu", "-"),         // mixed surface: same reason, no valid analysis
    ];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "multiplesegment-deletion-composition word {word:?}"
        );
    }
}

/// `rust/conformance/rewrite/merge/expected.tsv`: Rust now matches the oracle on all 3 words, including a 2-segment-to-1 merge rule reversal on analysis (`butbut`) that once diverged from it.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn merge_matches_oracle() {
    if !have_fixture("merge") {
        eprintln!("skipping: rust/conformance/rewrite/merge not present on disk");
        return;
    }
    let g = load_fixture("merge");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [
        ("butbut", "|butbut"), // 2-segment `ii` -> 1-segment `t` merge, reversed on analysis
        ("buiibuii", "-"),     // un-rewritten surface: no valid analysis (rule is obligatory)
        ("buiibut", "-"),      // mixed surface: same reason
    ];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "merge word {word:?}"
        );
    }
}

/// `rust/conformance/rewrite/multiplemerge/expected.tsv`: same merge-rule family (3-segment-to-2), now matching the oracle on all 3 words including the once-divergent `bttbtt`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn multiplemerge_matches_oracle() {
    if !have_fixture("multiplemerge") {
        eprintln!("skipping: rust/conformance/rewrite/multiplemerge not present on disk");
        return;
    }
    let g = load_fixture("multiplemerge");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [
        ("bttbtt", "|bttbtt"), // 3-segment `u-i-i` -> 2-segment `t-t` merge, reversed
        ("buiibuii", "-"),     // un-rewritten surface: no valid analysis
        ("bttbuii", "-"),      // mixed surface: same reason
    ];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "multiplemerge word {word:?}"
        );
    }
}
