//! Conformance replay for the P3 compounding fixtures
//! (`rust/conformance/compounding/{prefix-commute,nonhead-not-root}/`): load each fixture's
//! `grammar.xml` exactly as authored (standalone, oracle-verified — no `csharp_port_common`
//! scaffolding), parse every word in `words.txt`, and check
//! `Morpher::parse_word(...).signature()` against the literal signature transcribed from that
//! fixture's oracle-generated `expected.tsv` (same convention as
//! `crates/hc-parse/tests/metathesis_conformance.rs`). Each fixture's README documents the
//! oracle-generating command and what the fixture pins:
//!
//! - `prefix-commute`: `CompoundingRuleTests.SimpleRules` reconfiguration 3's REAL grammar
//!   (nonHead+head output order carried over from reconfiguration 2) — a "di+" prefix commutes
//!   with compounding because the affixed span is the HEAD, which stays in the stratum cascade.
//! - `nonhead-not-root`: the same grammar with head+nonHead output order — the affixed span
//!   becomes the NON-HEAD, which `AnalysisCompoundingRule` requires to be a bare root, so BOTH
//!   engines return no analyses (parity pin for the shared "non-head must already be a root"
//!   design limit; this is the grammar the P3 plan item mistook for an engine gap). Its "pʰutdat"
//!   row (P4, 2026-07-09) additionally pins the homophone-disjunction fix: the dat-homophone pair
//!   (entries 8/9) resolves via the NON-HEAD under this grammar's word order, which is exactly the
//!   path `simple_rules_1_homophone_disjunction_finding` documents.

use std::path::{Path, PathBuf};

use hc_grammar::load;
use hc_parse::Morpher;

fn fixture_path(name: &str, file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/compounding")
        .join(name)
        .join(file)
}

fn load_fixture(name: &str) -> hc_grammar::model::Grammar {
    let xml = std::fs::read_to_string(fixture_path(name, "grammar.xml")).expect("read grammar.xml");
    load(&xml).unwrap_or_else(|e| panic!("{name}: grammar failed to load: {e}"))
}

/// `rust/conformance/compounding/prefix-commute/expected.tsv`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn prefix_commute_matches_oracle() {
    let g = load_fixture("prefix-commute");
    let m = Morpher::new(&g, usize::MAX);
    let cases = [
        ("pʰutdidat", "5+PAST+9|(pʰ)ut+?di+?dat"),
        ("pʰutdat", "5+8|(pʰ)ut+?dat;5+9|(pʰ)ut+?dat"),
        ("pʰutdas", "-"),
    ];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "prefix-commute word {word:?}"
        );
    }
}

/// `rust/conformance/compounding/nonhead-not-root/expected.tsv`.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn nonhead_not_root_matches_oracle() {
    let g = load_fixture("nonhead-not-root");
    let m = Morpher::new(&g, usize::MAX);
    // "pʰutdat" (P4, 2026-07-09): with head+nonHead order the dat-homophone pair (entries 8/9)
    // resolves via the NON-HEAD; the live oracle keeps both ("5+8|...;5+9|..."). Previously omitted
    // from this fixture because Rust's engine collapsed them to "5+8" only -- see
    // `csharp_port_compounding.rs::simple_rules_1_homophone_disjunction_finding` for the root cause
    // (fixed) and `nonhead-not-root/README.md` for the re-generation record.
    let cases = [
        ("pʰutdidat", "-"),
        ("pʰutdat", "5+8|(pʰ)ut+?dat;5+9|(pʰ)ut+?dat"),
        ("pʰutdas", "-"),
    ];
    for (word, expected) in cases {
        assert_eq!(
            m.parse_word(word).signature(),
            expected,
            "nonhead-not-root word {word:?}"
        );
    }
}
