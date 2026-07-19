//! Conformance replay for W3.2 (plan #5d, history row `987be2fd`): the disjunctive-allomorph /
//! free-fluctuation final re-check, `rust/conformance/allomorphy/disjunctive-recheck/`.
//! `expected.tsv` is C#-oracle-generated (parse-opt @ `ccf750e6`); see the fixture README for the
//! grammar design and the row-by-row rationale.
//!
//! Red-on-revert: remove the disjunctive candidate loop from
//! `pg-rules/src/validity.rs::allomorphs_valid_impl` (or stop populating
//! `MorphRecord::passed_over` in `morph.rs::synth_affix`) and `wakta` (root arm, `Range(0, Index)`
//! fallback) and/or `pakda` (affix arm, recorded passed-over indices) start parsing again —
//! exactly the two rows that diverged at fixture-freeze time.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::{Morpher, ParseOptions};
use pg_rules::trace::{FailureReason, TraceHandle, TreeTraceSink};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/allomorphy/disjunctive-recheck")
}

/// Self-skip guard: `rust/conformance/` isn't a submodule yet (module doc), so `--include-ignored`
/// runs (CI's release sweep included) must not panic on the missing directory.
fn have_fixture() -> bool {
    fixture_dir().join("grammar.xml").exists()
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
fn disjunctive_recheck_matches_oracle() {
    if !have_fixture() {
        eprintln!("skipping: rust/conformance/allomorphy/disjunctive-recheck not present on disk");
        return;
    }
    let dir = fixture_dir();
    let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("read grammar.xml");
    let grammar =
        load(&xml).unwrap_or_else(|e| panic!("disjunctive-recheck grammar failed to load: {e}"));
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
        assert_eq!(
            got, expected_sig,
            "disjunctive-recheck: word {word:?} signature mismatch vs C# oracle"
        );
        checked += 1;
    }
    assert_eq!(checked, 12, "expected.tsv should pin all 12 fixture words");
}

/// P12 chunk 3 acceptance: the fixture's own two named red-on-revert words ("wakta"/"pakda" --
/// module doc) must show `FailureReason::DisjunctiveAllomorph` fired against a REJECTED candidate
/// somewhere in the trace (the very gate this fixture pins), not just an unexplained-`Failed` or a
/// merely-correct final signature.
#[test]
#[ignore = "conformance/ not yet pulled into PanGloss as a submodule -- see docs/hermitcrab-rust-port-audit.md section 5; will start running again once it lands"]
fn disjunctive_recheck_traces_the_rejection_reason() {
    if !have_fixture() {
        eprintln!("skipping: rust/conformance/allomorphy/disjunctive-recheck not present on disk");
        return;
    }
    let dir = fixture_dir();
    let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("read grammar.xml");
    let grammar =
        load(&xml).unwrap_or_else(|e| panic!("disjunctive-recheck grammar failed to load: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    for word in ["wakta", "pakda"] {
        let sink = TreeTraceSink::new();
        let _outcome = morpher.parse_word_traced(word, &ParseOptions::default(), &sink);
        let root = sink.root().expect("analyze_word must mint a root");
        let mut reasons = Vec::new();
        collect_reasons(&sink, root, &mut reasons);
        assert!(
            reasons.contains(&FailureReason::DisjunctiveAllomorph),
            "{word:?}: expected a Failed(DisjunctiveAllomorph) node; got {reasons:?}"
        );
    }
}
