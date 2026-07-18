//! Conformance replay for the oracle-verified rewrite-rule fixtures under
//! `rust/conformance/rewrite/` that have an in-tree replay (same convention as
//! `metathesis_conformance.rs`): load each fixture's `grammar.xml` exactly as authored (no
//! `csharp_port_common` scaffolding), parse every word in `words.txt`, and check
//! `Morpher::parse_word(...).signature()` against the literal signature transcribed from that
//! fixture's oracle-generated `expected.tsv`. Each fixture's README documents the
//! oracle-generating command and the derivation of every expected value.

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

/// `rust/conformance/rewrite/word-initial-epenthesis/expected.tsv` — the P1 fixture (bare-root
/// word-initial epenthesis: `syn_epenthesis`'s site-0 gap + `compile_lane_fst`'s multi-node
/// RtL analysis-target ordering; see the fixture README for the full dual-bug derivation).
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn word_initial_epenthesis_matches_oracle() {
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

/// `rust/conformance/rewrite/deletion-reinsertion/expected.tsv` — the P2 fixture (multi-site
/// deletion unapplication: ONE analysis pass inserts the deleted segment at ALL matching sites as
/// OPTIONAL nodes, and root lookup's consume-or-skip branching over those optional nodes reaches
/// every per-subset lexical entry; `Morpher.DeletionReapplications` defaults to 0, so the
/// insert-after-an-insert entry `buiibuii` must stay unreachable from `bubu`). See the fixture
/// README for the full derivation and `csharp_port_rewrite.rs`'s
/// `deletion_rules_multi_position_reinsertion` for the C# source anatomy.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn deletion_reinsertion_matches_oracle() {
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

/// `rust/conformance/rewrite/multiplesegment-deletion-composition/expected.tsv` — the P6 fixture
/// (a 2-segment feature-change target composed, in the SAME stratum, with a later-listed
/// pure-deletion rule that never fires on any word here: C#'s reverse-listed-order analysis
/// convention still unapplies the deletion rule FIRST, and its legitimate multi-site OPTIONAL
/// reinsertion interposes an Optional segment between the two real segments of the 2-segment
/// rule's own target pairs). See the fixture README for the full root-cause derivation
/// (`pg_rules::rewrite::ana_feature`'s group-capture fix, `compile_lane_fst_grouped`) and
/// `csharp_port_rewrite.rs`'s `multiple_segment_rules_deletion_composition_finding` for the C#
/// source anatomy.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn multiplesegment_deletion_composition_matches_oracle() {
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

/// `rust/conformance/rewrite/merge/expected.tsv` — history-matrix row 1 (`812aa48e`/#403/
/// LT-22480, the C# merge-rule stale-index deletion bug). At fixture-freeze time this fixture's
/// README recorded a DIVERGENCE ("Rust fails to find the valid parse" for `butbut`). Re-verified
/// 2026-07-10 (P9/W12 closeout): Rust now MATCHES the oracle on all 3 words, including `butbut`.
/// The grammar has exactly one lexical entry and zero morphological rules (pure phonology), so a
/// surface accept has no alternate root/analysis it could be spuriously matching — this is a real
/// positive demonstration that Rust's analysis-side now reverses the 2-segment-to-1 merge rule
/// correctly. Root cause of the flip not chased to a single commit, but P10's
/// `GetSkippedOptionalNodes` fold (`63b0a89f`) is circumstantially the same family of segment-count
/// bookkeeping fix and is the leading candidate. See the fixture README for the full history.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn merge_matches_oracle() {
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

/// `rust/conformance/rewrite/multiplemerge/expected.tsv` — same history-matrix row 1 family
/// (3-segment-to-2 merge). Re-verified 2026-07-10 alongside `merge_matches_oracle`: now MATCHES
/// the oracle on all 3 words (previously diverged on `bttbtt` at freeze time). See that test's doc
/// comment and the fixture README for the full history.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn multiplemerge_matches_oracle() {
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
