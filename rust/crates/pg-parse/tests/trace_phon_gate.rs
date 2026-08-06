//! Phonological rule tracing: `pg_rules::rewrite`/`pg_rules::metathesis` wired into `synthesize_stratum_traced`'s trailing prule application, observable through `Morpher::parse_word_traced` (the analysis-side stratum caller remains untraced).

mod csharp_port_common;
use csharp_port_common::build_grammar;
use pg_parse::{Morpher, ParseOptions};
use pg_rules::trace::{FailureReason, TraceHandle, TraceType, TreeTraceSink};

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

/// A single feature-change rewrite rule (final devoicing: C -> VlUnasp / _ #), reusing the shared grammar's `ncC`/`ncVlUnasp` natural classes.
fn devoicing_grammar() -> pg_grammar::model::Grammar {
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
    // "gap": word-final-devoiced from underlying "gab" by rule3 during synthesis's trailing-prule pass; a successful parse already exercises the rule.
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
    // Every segment of "gigigi" is a vowel, so rule3's consonant-only LHS never finds a match position, and subrule 0 must report `NotApplied(Pattern)`.
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

/// `pg_rules::metathesis::synthesize_cached_traced`'s live-wiring; metathesis has no subrules, so `subrule_index` is always `-1`, unlike every rewrite-rule node above.
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
