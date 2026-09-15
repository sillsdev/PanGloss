//! W3.2's rejection REASON, which the generic replay cannot see: it diffs signatures only.
//! Why this file survived the v1 cull and its siblings did not: docs/design/fixture-pins.md

use pg_conformance_fixtures::require_fixture;
use pg_parse::{Morpher, ParseOptions};
use pg_rules::trace::{FailureReason, TraceHandle, TreeTraceSink};

/// Every `FailureReason` anywhere in the trace tree, so this asserts on *why*, not just the outcome.
fn collect_reasons(sink: &TreeTraceSink, h: TraceHandle, out: &mut Vec<FailureReason>) {
    let n = sink.node(h);
    if let Some(r) = n.failure_reason {
        out.push(r);
    }
    for &c in &n.children {
        collect_reasons(sink, c, out);
    }
}

/// `wakta`/`pakda` must be rejected BY the disjunctive re-check, not merely rejected.
/// Why a signature alone cannot show that: docs/design/fixture-pins.md
#[test]
fn disjunctive_recheck_rejects_for_the_disjunctive_reason() {
    let fixture = require_fixture("edge-cases", "disjunctive-recheck");
    let grammar = pg_grammar::load(&fixture.load_grammar_xml())
        .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", fixture.label()));
    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);

    for word in ["wakta", "pakda"] {
        let sink = TreeTraceSink::new();
        let _ = morpher.parse_word_traced(word, &ParseOptions::default(), &sink);
        let root = sink.root().expect("analyze_word must mint a root");
        let mut reasons = Vec::new();
        collect_reasons(&sink, root, &mut reasons);
        assert!(
            reasons.contains(&FailureReason::DisjunctiveAllomorph),
            "{word:?}: expected a Failed(DisjunctiveAllomorph) node in {}; got {reasons:?}",
            fixture.label()
        );
    }
}
