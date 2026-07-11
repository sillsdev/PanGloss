//! Conformance replay for W3.3 (environment spans on discontinuous morphs, history row
//! `97fa7721`): `rust/conformance/allomorphy/discontinuous-env/`. `expected.tsv` is
//! C#-oracle-generated (parse-opt @ `ccf750e6`); see the fixture README for the two discontinuity
//! flavors (circumfix-with-env, env-bearing root split by an infix).
//!
//! Red-on-revert: collapse `attribute_morphs` (`hc-rules/src/morph.rs`) back to one record per
//! morph (drop the contiguous-run split) and `xpitz` + `muat` start parsing again — the
//! environment is then only anchored at the morph's FIRST piece instead of every piece.

use std::path::{Path, PathBuf};

use hc_grammar::load;
use hc_parse::{Morpher, ParseOptions};
use hc_rules::trace::{FailureReason, TraceHandle, TreeTraceSink};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/allomorphy/discontinuous-env")
}

/// Collect every `FailureReason` reported anywhere in the tree (P12 chunk 3's own acceptance
/// criterion: extend this fixture with a same-data assertion on *why*, not just the outcome).
fn collect_reasons(sink: &TreeTraceSink, h: TraceHandle, out: &mut Vec<FailureReason>) {
    let n = sink.node(h);
    if let Some(r) = n.failure_reason {
        out.push(r);
    }
    for &c in &n.children {
        collect_reasons(sink, c, out);
    }
}

#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn discontinuous_env_matches_oracle() {
    let dir = fixture_dir();
    let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("read grammar.xml");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("discontinuous-env grammar failed to load: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    let text = std::fs::read_to_string(dir.join("expected.tsv")).expect("read expected.tsv");
    let mut checked = 0;
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 5 {
            continue; // interleaved STARTED sentinel rows
        }
        let (word, expected_sig) = (cols[1], cols[4]);
        let got = morpher.parse_word(word).signature();
        assert_eq!(got, expected_sig, "discontinuous-env: word {word:?} signature mismatch vs C# oracle");
        checked += 1;
    }
    assert_eq!(checked, 7, "expected.tsv should pin all 7 fixture words");
}

/// P12 chunk 3 acceptance: the fixture's own two named red-on-revert words ("xpitz"/"muat" -- module
/// doc) must show `FailureReason::Environments` fired against a rejected candidate somewhere in the
/// trace -- the per-piece environment anchoring this fixture pins, not just a correct final
/// signature with no explanation.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn discontinuous_env_traces_the_rejection_reason() {
    let dir = fixture_dir();
    let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("read grammar.xml");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("discontinuous-env grammar failed to load: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    for word in ["xpitz", "muat"] {
        let sink = TreeTraceSink::new();
        let _outcome = morpher.parse_word_traced(word, &ParseOptions::default(), &sink);
        let root = sink.root().expect("analyze_word must mint a root");
        let mut reasons = Vec::new();
        collect_reasons(&sink, root, &mut reasons);
        assert!(
            reasons.contains(&FailureReason::Environments),
            "{word:?}: expected a Failed(Environments) node; got {reasons:?}"
        );
    }
}
