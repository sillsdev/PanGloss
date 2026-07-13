//! P12 chunk 6 acceptance test: phonological rule tracing (`hc_rules::rewrite`/`hc_rules::metathesis`
//! wired into `hc_rules::stratum::synthesize_stratum_traced`'s trailing prule application, plus the
//! live `stratum.rs` call-site swap that makes it observable through `Morpher::parse_word_traced`).
//!
//! Before this chunk, a `--trace` run's tree had zero `PhonologicalRuleSynthesis`/
//! `PhonologicalRuleAnalysis` nodes anywhere, no matter how many phonological rules actually fired --
//! chunk 6 closes that gap on the SYNTHESIS side (the analysis-side stratum caller
//! (`StratumAnalyzer::analyze`) is itself untraced, a separate, pre-existing, documented P12 gap; see
//! `rust-optimizations-phase2.md`'s P12 section).

mod csharp_port_common;
use csharp_port_common::build_grammar;
use hc_parse::{Morpher, ParseOptions};
use hc_rules::trace::{FailureReason, TraceHandle, TraceType, TreeTraceSink};

/// Every `PhonologicalRuleSynthesis` node anywhere in the tree, as `(subrule_index, failure_reason)`.
fn phon_synth_nodes(
    sink: &TreeTraceSink,
    h: TraceHandle,
    out: &mut Vec<(Option<i32>, Option<FailureReason>)>,
) {
    let n = sink.node(h);
    if n.type_ == TraceType::PhonologicalRuleSynthesis {
        out.push((n.subrule_index, n.failure_reason));
    }
    for &c in &n.children {
        phon_synth_nodes(sink, c, out);
    }
}

/// A single feature-change rewrite rule (final devoicing: C -> VlUnasp / _ #), reusing the shared
/// grammar's `ncC`/`ncVlUnasp` natural classes and `posV` roots "11"/"12" (underlying "gab") --
/// `csharp_port_rewrite.rs::anchor_rules` case (2)'s exact rule/grammar shape, minus the case (1)
/// root ("10") that test's own doc flags as separately broken (unrelated to this rule -- a
/// cross-table `StrRep` gap in root lookup, not the rewrite rule itself).
fn devoicing_grammar() -> hc_grammar::model::Grammar {
    build_grammar(
        r#"<PhonologicalRule id="pr3"><Name>rule3</Name>
             <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncC" /></PhoneticSequence></PhoneticInput>
             <PhonologicalSubrules><PhonologicalSubrule>
               <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVlUnasp" /></PhoneticSequence></PhoneticOutput>
               <Environment><RightEnvironment><PhoneticTemplate finalBoundaryCondition="true" /></RightEnvironment></Environment>
             </PhonologicalSubrule></PhonologicalSubrules>
           </PhonologicalRule>"#,
        "pr3",
        "",
        "",
        "",
    )
}

#[test]
fn live_trace_shows_phonological_rule_applied_for_a_word_the_rule_fires_on() {
    let g = devoicing_grammar();
    let m = Morpher::new(&g, usize::MAX);
    // "gap": root "11"/"12" (underlying "gab") word-final-devoiced to "gap" by rule3 during
    // synthesis's trailing-prule pass -- confirming the parse succeeds at all already exercises the
    // rule (analysis un-devoices "p" back to an optional "b"/"p", then synthesis re-devoices it).
    let word = "gap";
    let plain = m.parse_word(word);
    let sink = TreeTraceSink::new();
    let outcome = m.parse_word_traced(word, &ParseOptions::default(), &sink);
    assert_eq!(
        plain.signature(),
        outcome.signature(),
        "tracing must not change the parse result -- the traced synthesis body is a hand-copied \
         re-implementation of the untraced subrule loop (design doc's own flagged hazard: tracing \
         must never change control flow/results), so this is the one check that turns the \
         equivalence argument from \"by inspection\" into \"by test\""
    );
    assert!(
        !outcome.analyses.is_empty(),
        "sanity: {word:?} must still parse with tracing on"
    );

    let root = sink.root().expect("analyze_word must mint a root");
    let mut nodes = Vec::new();
    phon_synth_nodes(&sink, root, &mut nodes);
    assert!(
        !nodes.is_empty(),
        "expected at least one PhonologicalRuleSynthesis node anywhere in the trace for {word:?} -- \
         before P12 chunk 6 this list was ALWAYS empty regardless of grammar, since rewrite.rs/\
         metathesis.rs fired no trace events at all"
    );
    assert!(
        nodes
            .iter()
            .any(|(idx, reason)| *idx == Some(0) && reason.is_none()),
        "expected subrule 0 to report Applied (no FailureReason) for at least one derivation of \
         {word:?}; got {nodes:?}"
    );
}

#[test]
fn live_trace_reports_pattern_fallback_for_a_word_the_rule_never_matches() {
    let g = devoicing_grammar();
    let m = Morpher::new(&g, usize::MAX);
    // Root "44" (posV, underlying "gigigi") from the shared lexicon: every segment is a vowel, so
    // rule3's LHS target (`ncC`, a consonant natural class) never finds a single match position
    // anywhere in the word -- `syn_feature`'s scan comes back empty, so subrule 0 must report
    // `NotApplied(Pattern)` on this derivation (there is no more specific gate to have failed: no
    // MPR/POS restriction is declared on this subrule at all).
    let word = "gigigi";
    let plain = m.parse_word(word);
    let sink = TreeTraceSink::new();
    let outcome = m.parse_word_traced(word, &ParseOptions::default(), &sink);
    assert_eq!(
        plain.signature(),
        outcome.signature(),
        "tracing must not change the parse result"
    );
    assert!(
        !outcome.analyses.is_empty(),
        "sanity: {word:?} must still parse with tracing on"
    );

    let root = sink.root().expect("analyze_word must mint a root");
    let mut nodes = Vec::new();
    phon_synth_nodes(&sink, root, &mut nodes);
    assert!(
        nodes
            .iter()
            .any(|(idx, reason)| *idx == Some(0) && *reason == Some(FailureReason::Pattern)),
        "expected subrule 0 to report NotApplied(Pattern) for at least one derivation of {word:?} \
         (rule3's LHS never matches a vowel-final root); got {nodes:?}"
    );
}

/// `hc_rules::metathesis::synthesize_cached_traced`'s live-wiring, via
/// `csharp_port_metathesis.rs::simple_rule`'s exact grammar (adjacent i/u swap). Metathesis has no
/// subrules (`MetathesisRuleDef` carries ONE pattern, no gate) -- `subrule_index` is always `-1`
/// (`SynthesisMetathesisRule.cs:47,52`), unlike every rewrite-rule node above.
#[test]
fn live_trace_shows_metathesis_rule_applied() {
    let g = build_grammar(
        r#"<MetathesisRule id="mr1" leftSwitch="segU" rightSwitch="segI">
             <Name>metathesis1</Name>
             <StructuralDescription><PhoneticTemplate><PhoneticSequence>
               <SimpleContext id="segI" naturalClass="ncISeg" />
               <SimpleContext id="segU" naturalClass="ncUSeg" />
             </PhoneticSequence></PhoneticTemplate></StructuralDescription>
           </MetathesisRule>"#,
        "mr1",
        "",
        "",
        "",
    );
    let m = Morpher::new(&g, usize::MAX);
    let word = "mui";
    let plain = m.parse_word(word);
    let sink = TreeTraceSink::new();
    let outcome = m.parse_word_traced(word, &ParseOptions::default(), &sink);
    assert_eq!(
        plain.signature(),
        outcome.signature(),
        "tracing must not change the parse result"
    );
    assert!(
        !outcome.analyses.is_empty(),
        "sanity: {word:?} must still parse with tracing on"
    );

    let root = sink.root().expect("analyze_word must mint a root");
    let mut nodes = Vec::new();
    phon_synth_nodes(&sink, root, &mut nodes);
    assert!(
        !nodes.is_empty(),
        "expected at least one PhonologicalRuleSynthesis node for {word:?} -- metathesis's own \
         `synthesize_cached_traced` must fire live through `synthesize_stratum_traced`'s trailing \
         prule loop exactly like the rewrite-rule case above"
    );
    assert!(
        nodes
            .iter()
            .any(|(idx, reason)| *idx == Some(-1) && reason.is_none()),
        "expected subrule index -1, Applied (no reason), for at least one derivation of {word:?}; \
         got {nodes:?}"
    );
}
