//! Conformance replay for environment spans on discontinuous morphs: collapsing `attribute_morphs`'s
//! contiguous-run split back to one record per morph makes `xpitz`/`muat` wrongly parse again.
//!
//! THIS PIN IS DEAD AND HAS NEVER RUN IN THIS TREE. It reads a fixture at
//! `rust/conformance/allomorphy/discontinuous-env/`; neither that directory nor `rust/conformance/`
//! has ever existed here, so both tests skip twice over -- once on `#[ignore]`, and again on
//! `have_fixture()` even under `--include-ignored`. CLAUDE.md's "oracle hierarchy" section cites
//! this very fixture as the worked example of why oracle-diffed fixtures matter, which makes a
//! silently skipping pin the exact "a control that cannot act must say so" defect that file names.
//!
//! To revive: author `conformance-staging/edge-cases/discontinuous-env/` (grammar.xml + words.yaml)
//! per `.claude/skills/conformance-grammars/SKILL.md`, generate its expectations from the C#
//! founding oracle, rewrite the body below to use `pg_conformance_fixtures` discovery instead of the
//! hand-rolled path and `expected.tsv` reader, drop `have_fixture()` so an absent fixture FAILS
//! rather than skips, and delete both `#[ignore]` attributes. The grammar needs a discontinuous
//! morph whose allomorph environment holds at its first piece and is violated at a later one.

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::{Morpher, ParseOptions};
use pg_rules::trace::{FailureReason, TraceHandle, TreeTraceSink};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/allomorphy/discontinuous-env")
}

/// Self-skip guard, so an `--include-ignored` run does not panic when the fixture directory is absent.
fn have_fixture() -> bool {
    fixture_dir().join("grammar.xml").exists()
}

/// Collects every `FailureReason` reported anywhere in the trace tree, so this fixture can assert on *why*, not just the outcome.
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
#[ignore = "DEAD PIN: its fixture has never existed at rust/conformance/allomorphy/discontinuous-env/ in this tree. Rebuild it under conformance-staging/edge-cases/ and delete this attribute -- see this file's header."]
fn discontinuous_env_matches_oracle() {
    if !have_fixture() {
        eprintln!("skipping: rust/conformance/allomorphy/discontinuous-env not present on disk");
        return;
    }
    let dir = fixture_dir();
    let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("read grammar.xml");
    let grammar =
        load(&xml).unwrap_or_else(|e| panic!("discontinuous-env grammar failed to load: {e}"));
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
            "discontinuous-env: word {word:?} signature mismatch vs C# oracle"
        );
        checked += 1;
    }
    assert_eq!(checked, 7, "expected.tsv should pin all 7 fixture words");
}

/// The fixture's two red-on-revert words ("xpitz"/"muat") must show `FailureReason::Environments` fired against a rejected candidate somewhere in the trace, not just a correct final signature with no explanation.
#[test]
#[ignore = "DEAD PIN: its fixture has never existed at rust/conformance/allomorphy/discontinuous-env/ in this tree. Rebuild it under conformance-staging/edge-cases/ and delete this attribute -- see this file's header."]
fn discontinuous_env_traces_the_rejection_reason() {
    if !have_fixture() {
        eprintln!("skipping: rust/conformance/allomorphy/discontinuous-env not present on disk");
        return;
    }
    let dir = fixture_dir();
    let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("read grammar.xml");
    let grammar =
        load(&xml).unwrap_or_else(|e| panic!("discontinuous-env grammar failed to load: {e}"));
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
