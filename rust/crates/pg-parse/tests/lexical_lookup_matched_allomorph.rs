//! machine#627: lexical lookup proposes only the matched root allomorph, and skips one whose entry's syntactic features do not unify with the input's; the sweep diffs signatures only.
//! Why a named pin: docs/design/fixture-pins.md

use pg_conformance_fixtures::require_fixture;
use pg_parse::{Morpher, ParseOptions};
use pg_rules::stats::{ObjectKind, ALLOMORPH_NONE};
use pg_rules::trace::{FailureReason, TraceHandle, TreeTraceSink};

const FIXTURE: &str = "lexical-lookup-sibling-allomorphs";

fn collect_reasons(sink: &TreeTraceSink, h: TraceHandle, out: &mut Vec<FailureReason>) {
    let n = sink.node(h);
    if let Some(r) = n.failure_reason {
        out.push(r);
    }
    for &c in &n.children {
        collect_reasons(sink, c, out);
    }
}

fn reasons_of(word: &str) -> (Vec<String>, Vec<FailureReason>) {
    let fixture = require_fixture("edge-cases", FIXTURE);
    let grammar = pg_grammar::load(&fixture.load_grammar_xml())
        .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", fixture.label()));
    let morpher = Morpher::new(&grammar, usize::MAX);
    let sink = TreeTraceSink::new();
    let outcome = morpher.parse_word_traced(word, &ParseOptions::default(), &sink);
    let root = sink.root().expect("analyze_word must mint a root");
    let mut reasons = Vec::new();
    collect_reasons(&sink, root, &mut reasons);
    let analyses = outcome.analyses.into_iter().map(|(m, _)| m).collect();
    (analyses, reasons)
}

/// Analysis matches only `bui`, whose environment fails; the sibling `buii` would synthesize to `bu`, and is not proposed.
#[test]
fn a_sibling_allomorph_the_search_did_not_match_is_not_synthesized() {
    let (analyses, _) = reasons_of("bu");
    assert!(
        analyses.is_empty(),
        "bu parsed as {analyses:?}: a sibling of the matched allomorph was proposed"
    );
}

/// `takt` asks for a noun stem, so the verb homograph `tak` is skipped by its syntactic features before synthesis: lookup proposes one allomorph, not two.
#[test]
fn a_matched_allomorph_with_non_unifying_syntactic_features_is_skipped_by_lookup() {
    let fixture = require_fixture("edge-cases", FIXTURE);
    let grammar = pg_grammar::load(&fixture.load_grammar_xml())
        .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", fixture.label()));
    let morpher = Morpher::new(&grammar, usize::MAX);
    let (outcome, rows) = morpher.parse_word_with_stats("takt", &ParseOptions::default());
    let analyses: Vec<_> = outcome.analyses.into_iter().map(|(m, _)| m).collect();
    assert_eq!(analyses, ["TAKN+NSUF"]);
    let proposed: u64 = rows
        .iter()
        .filter(|r| r.kind == ObjectKind::LexEntry && r.allomorph != ALLOMORPH_NONE)
        .map(|r| r.counters.attempts)
        .sum();
    assert_eq!(
        proposed, 1,
        "only the noun homograph may be proposed for synthesis; the verb is skipped in lookup"
    );
}

/// The bare word asks for no part of speech, so lookup skips nothing and both homographs parse.
#[test]
fn homographs_both_parse_when_no_part_of_speech_is_asked() {
    let (mut analyses, reasons) = reasons_of("tak");
    analyses.sort();
    assert_eq!(analyses, ["TAKN", "TAKV"]);
    assert!(
        !reasons.contains(&FailureReason::RequiredSyntacticFeatureStruct),
        "nothing should be skipped for the bare word, got {reasons:?}"
    );
}
