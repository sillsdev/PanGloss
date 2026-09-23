use super::*;
use pg_shape::ShapeBuilder;

fn w() -> Word {
    Word::new(ShapeBuilder::new().finish(), StratumId(0))
}

#[test]
fn noop_sink_is_not_tracing() {
    let sink = NoopSink;
    assert!(!sink.is_tracing());
}

#[test]
fn tree_sink_is_tracing() {
    let sink = TreeTraceSink::new();
    assert!(sink.is_tracing());
}

/// Applying a rule reassigns the cursor so a later event nests under the rule's own node.
#[test]
fn rule_applied_return_nests_next_event_under_it() {
    let sink = TreeTraceSink::new();
    let word = w();
    let root = sink.analyze_word(&word);

    // First rule application: appended as root's child.
    let h1 = sink.morphological_rule_applied(root, MRuleId(0), 0, &word);
    assert_eq!(sink.node(root).children, vec![h1]);

    // Caller passes h1 (not root) as parent for the next event -- it nests UNDER the rule.
    let h2 = sink.morphological_rule_applied(h1, MRuleId(1), 0, &word);
    assert_eq!(sink.node(h1).children, vec![h2]);
    assert!(
        sink.node(root).children == vec![h1],
        "h2 must not become a sibling of h1 under root"
    );
}

/// `SynthesizeWord` reaches two levels deep, descending into the just-appended `LexicalLookup` node's children.
#[test]
fn synthesize_word_nests_two_levels_deep_under_lexical_lookup() {
    let sink = TreeTraceSink::new();
    let word = w();
    let root = sink.analyze_word(&word);

    let lex = sink.lexical_lookup(root, StratumId(0), &word);
    assert_eq!(
        sink.node(root).children,
        vec![lex],
        "LexicalLookup is root's child, cursor unchanged"
    );

    // Call site passes `root` (the cursor, unchanged by LexicalLookup) -- not `lex`.
    let syn = sink.synthesize_word(root, &word);

    assert_eq!(
        sink.node(root).children,
        vec![lex],
        "no new direct child of root"
    );
    assert_eq!(
        sink.node(lex).children,
        vec![syn],
        "SynthesizeWord landed under lex's children, not root's"
    );
}

#[test]
fn failure_context_is_opt_in_and_attached_to_exact_event() {
    let plain = TreeTraceSink::new();
    assert!(!plain.captures_failure_context());
    let sink = TreeTraceSink::with_failure_context();
    assert!(sink.captures_failure_context());
    let word = w();
    let root = sink.analyze_word(&word);
    let failed = sink.failed(root, &word, FailureReason::SurfaceFormMismatch);
    let succeeded = sink.successful(root, &word);
    sink.set_failure_context(
        failed,
        FailureContext {
            required: Some("cats".into()),
            actual: Some("cat".into()),
            environment: None,
        },
    );
    assert_eq!(
        sink.node(failed)
            .failure_context
            .unwrap()
            .required
            .as_deref(),
        Some("cats")
    );
    assert!(sink.node(succeeded).failure_context.is_none());
    assert!(sink.node(root).failure_context.is_none());
}
#[test]
fn failed_and_successful_carry_reason_and_word() {
    let sink = TreeTraceSink::new();
    let word = w();
    let root = sink.analyze_word(&word);

    let f = sink.failed(root, &word, FailureReason::PartialParse);
    assert_eq!(sink.node(f).type_, TraceType::Failed);
    assert_eq!(
        sink.node(f).failure_reason,
        Some(FailureReason::PartialParse)
    );
    assert!(sink.node(f).output.is_some());

    let s = sink.successful(root, &word);
    assert_eq!(sink.node(s).type_, TraceType::Successful);
    assert_eq!(sink.node(s).failure_reason, None);
}

#[test]
fn stratum_bookends_carry_stratum_source() {
    let sink = TreeTraceSink::new();
    let word = w();
    let root = sink.analyze_word(&word);
    let begin = sink.begin_unapply_stratum(root, StratumId(2), &word);
    assert_eq!(sink.node(begin).source, TraceSource::Stratum(StratumId(2)));
    assert_eq!(sink.node(begin).type_, TraceType::StratumAnalysisInput);
    let end = sink.end_unapply_stratum(begin, StratumId(2), &word);
    assert_eq!(sink.node(end).type_, TraceType::StratumAnalysisOutput);
    assert_eq!(sink.node(begin).children, vec![end]);
}
