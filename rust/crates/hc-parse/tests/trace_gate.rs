//! P12 chunk 2 acceptance test (design doc §5, chunk 2): the smallest end-to-end tracing slice --
//! `Morpher::parse_word_traced` mints the root `WordAnalysis` node and wires the three morpher-level
//! `Failed(...)` reasons (`PartialParse`/`ObligatorySyntacticFeatures` in `is_word_valid_traced`,
//! `SurfaceFormMismatch` in `is_match_traced`) plus `Successful`. This file proves the handle threads
//! correctly from `parse_word`'s entry through to its exit without touching `hc_rules` internals.
//!
//! `ObligatorySyntacticFeatures` and `SurfaceFormMismatch` are exercised with real, deterministic
//! fixtures below (a rule that declares an obligatory feature it never actually contributes; a real
//! Indonesian word empirically confirmed, via this same tracing machinery, to produce a
//! `SurfaceFormMismatch`-rejected candidate on the normal parse path). `PartialParse` needs a
//! multi-stratum/template scenario this crate's shared test grammar helper does not cheaply support
//! (see `hc-parse/src/morpher.rs`'s own `#[cfg(test)]` module for a direct, hand-built-`Word` unit
//! test of that gate instead, since it is private-method-level and does not need a full grammar).

mod csharp_port_common;
use csharp_port_common::build_grammar;
use hc_parse::{Morpher, ParseOptions};
use hc_rules::trace::{FailureReason, TraceType, TreeTraceSink};
use std::path::{Path, PathBuf};

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

/// The shared `ed_suffix`-shaped grammar (same shape as `word_timeout_gate.rs`'s `simple_grammar`,
/// duplicated here to keep this file self-contained): `posV` entry "32" = "sag", rule `ed_suffix`
/// appends "+d". A trivially-valid word.
fn valid_grammar() -> hc_grammar::model::Grammar {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    build_grammar("", "", mrules, "mrEd", "")
}

/// A rule that declares `outputObligatoryFeatures="featEvid"` (the shared `featEvid`/`symWit`
/// evidential feature, never otherwise referenced by any fixture in this crate's shared grammar
/// helper — grepped clean) but whose own `<MorphologicalOutput>` never sets it (no
/// `<OutputHeadFeatures>` at all) and which no lexical entry provides either. Confirming this rule
/// during synthesis unconditionally pushes `featEvid`'s `FeatId` onto `Word::obligatory`
/// (`hc_rules::morph::synth_process_allomorph`, unconditional `w.obligatory.extend_from_slice`) while
/// `Word::syn_fs` never actually contains it -- a guaranteed, deterministic
/// `FailureReason::ObligatorySyntacticFeatures` rejection at `Morpher::is_word_valid_traced`'s second
/// clause.
fn obligatory_feature_never_satisfied_grammar() -> hc_grammar::model::Grammar {
    let mrules = r#"
      <MorphologicalRule id="mrObl" requiredPartsOfSpeech="posV" outputObligatoryFeatures="featEvid">
        <Name>obl_suffix</Name><MorphemeId>OBL</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subObl">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+z</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    build_grammar("", "", mrules, "mrObl", "")
}

/// Walk the whole tree collecting `(TraceType, FailureReason)` for every node that carries a
/// `failure_reason`, plus a flag for whether any `Successful` node exists.
fn scan(sink: &TreeTraceSink, h: hc_rules::trace::TraceHandle, reasons: &mut Vec<(TraceType, FailureReason)>, has_success: &mut bool) {
    let n = sink.node(h);
    if n.type_ == TraceType::Successful {
        *has_success = true;
    }
    if let Some(r) = n.failure_reason {
        reasons.push((n.type_, r));
    }
    for &c in &n.children {
        scan(sink, c, reasons, has_success);
    }
}

#[test]
fn traced_and_untraced_parse_agree_on_the_signature() {
    let g = valid_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let plain = m.parse_word("sagd");
    let sink = TreeTraceSink::new();
    let traced = m.parse_word_traced("sagd", &ParseOptions::default(), &sink);
    assert_eq!(plain.signature(), traced.signature(), "tracing must not change parse behavior");
}

#[test]
fn trivially_valid_word_produces_a_successful_node_under_the_root() {
    let g = valid_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let sink = TreeTraceSink::new();
    let outcome = m.parse_word_traced("sagd", &ParseOptions::default(), &sink);
    assert!(!outcome.analyses.is_empty(), "sanity: the grammar still parses \"sagd\"");

    let root = sink.root().expect("analyze_word must mint a root");
    assert_eq!(sink.node(root).type_, TraceType::WordAnalysis);

    let mut reasons = Vec::new();
    let mut has_success = false;
    scan(&sink, root, &mut reasons, &mut has_success);
    assert!(has_success, "a valid parse must produce a Successful node somewhere in the tree");
}

#[test]
fn obligatory_syntactic_feature_never_satisfied_is_reported() {
    let g = obligatory_feature_never_satisfied_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let sink = TreeTraceSink::new();
    let outcome = m.parse_word_traced("sagz", &ParseOptions::default(), &sink);
    // The rule's own obligatory feature is never satisfiable, so this word must never validate.
    assert!(outcome.analyses.is_empty(), "sanity: \"sagz\" must NOT validate (its obligatory feature is unsatisfiable)");

    let root = sink.root().expect("analyze_word must mint a root");
    let mut reasons = Vec::new();
    let mut has_success = false;
    scan(&sink, root, &mut reasons, &mut has_success);
    assert!(!has_success);
    assert!(
        reasons.iter().any(|&(t, r)| t == TraceType::Failed && r == FailureReason::ObligatorySyntacticFeatures),
        "expected a Failed(ObligatorySyntacticFeatures) node; got {reasons:?}"
    );
}

/// Real-corpus fixture (self-skips if the untracked sample corpus isn't present on disk, matching
/// `indonesian_redup_gate.rs`'s existing convention): empirically confirmed (via this same tracing
/// machinery, during this chunk's development) that Indonesian's "memaca" produces at least one
/// synthesis candidate that passes `is_word_valid_traced` but is rejected by `is_match_traced` with
/// `SurfaceFormMismatch` -- the real grammar naturally exercises the third wired reason, not just a
/// hand-built one.
#[test]
fn real_indonesian_word_exercises_surface_form_mismatch() {
    let Some(grammar_path) = sample_path("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let grammar = hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let m = Morpher::new(&grammar, usize::MAX);

    let sink = TreeTraceSink::new();
    let _outcome = m.parse_word_traced("memaca", &ParseOptions::default(), &sink);
    let root = sink.root().expect("analyze_word must mint a root");
    let mut reasons = Vec::new();
    let mut has_success = false;
    scan(&sink, root, &mut reasons, &mut has_success);
    assert!(
        reasons.iter().any(|&(t, r)| t == TraceType::Failed && r == FailureReason::SurfaceFormMismatch),
        "expected at least one Failed(SurfaceFormMismatch) node for \"memaca\"; got {reasons:?}"
    );
}
