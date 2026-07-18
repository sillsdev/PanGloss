//! P12 chunks 4/5 acceptance test (design doc §6's bar: "the trace needs to show a rule sequence a
//! human could actually follow", not just a bare Successful/Failed leaf under the root).
//!
//! `pg_rules::stratum::guided_synth` (the synthesis-confirmation gate) now fires
//! `MorphologicalRuleApplied` on every successful rule confirmation and reassigns each output word's
//! trace cursor (`Word::trace`), so a real multi-rule derivation renders as a chain of nested
//! `MorphologicalRuleSynthesis` nodes, each one the parent of the next -- exactly the "rule sequence"
//! a person (or a future Rust/C# divergence hunt) needs to follow. This is real-corpus, not a hand-
//! built fixture: self-skips if the untracked sample corpus isn't present (matching
//! `indonesian_redup_gate.rs`'s existing convention).

use std::path::{Path, PathBuf};

use pg_grammar::load;
use pg_parse::{Morpher, ParseOptions};
use pg_rules::trace::{TraceHandle, TraceType, TreeTraceSink};

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

/// Every chain of `MorphologicalRuleSynthesis` nodes that ends in a `Successful` leaf, found
/// anywhere in the tree -- the "rule sequence" a human would read off a successful derivation.
/// Collects all of them (a word can have several winning derivations); the caller picks the longest.
fn successful_rule_chains(
    sink: &TreeTraceSink,
    h: TraceHandle,
    chain: &mut Vec<TraceHandle>,
    out: &mut Vec<Vec<TraceHandle>>,
) {
    let n = sink.node(h);
    if n.type_ == TraceType::Successful {
        out.push(chain.clone());
    }
    for &c in &n.children {
        let pushed = sink.node(c).type_ == TraceType::MorphologicalRuleSynthesis;
        if pushed {
            chain.push(c);
        }
        successful_rule_chains(sink, c, chain, out);
        if pushed {
            chain.pop();
        }
    }
}

#[test]
fn real_indonesian_word_renders_a_followable_multi_rule_sequence() {
    let Some(grammar_path) = sample_path("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let grammar = load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let morpher = Morpher::new(&grammar, usize::MAX);

    // "menziarahi": men- (nasal prefix) + ziarah (root) + -i (suffix) -- a genuine 2-rule synthesis
    // derivation (confirmed via this same tracing machinery during this chunk's development).
    let word = "menziarahi";
    let sink = TreeTraceSink::new();
    let outcome = morpher.parse_word_traced(word, &ParseOptions::default(), &sink);
    assert!(
        !outcome.analyses.is_empty(),
        "sanity: {word:?} must still parse"
    );

    let root = sink.root().expect("analyze_word must mint a root");
    let mut chain = Vec::new();
    let mut chains = Vec::new();
    successful_rule_chains(&sink, root, &mut chain, &mut chains);
    let found = chains
        .into_iter()
        .max_by_key(|c| c.len())
        .unwrap_or_else(|| panic!("expected at least one Successful derivation for {word:?}"));

    assert!(
        found.len() >= 2,
        "expected a multi-rule (>= 2 MorphologicalRuleSynthesis) chain leading to Successful for \
         {word:?}; got a chain of length {} -- the whole point of chunks 4/5's applied-event spine \
         is that a real multi-morph word shows its rule-by-rule derivation, not a flat leaf",
        found.len()
    );

    // Each node in the chain must be a DESCENDANT of the previous one (a real nested sequence, not
    // siblings coincidentally collected) -- re-walk from root confirming ancestry.
    //
    // 2026-07-17 update (dead-end-attribution census, `docs/superpowers/specs/
    // 2026-07-17-better-proposing-fst-plan.md` Phase 0): this used to require `step` be a DIRECT
    // child of `cursor` -- true before the census wired the analysis (unapply) cascade into this
    // same trace tree (see `pg-rules/src/stratum.rs`/`morph.rs`'s 2026-07-17 doc comments). Now a
    // real multi-rule word's successful synthesis chain is typically nested a few levels under the
    // analysis-side `MorphologicalRuleAnalysis`/`PhonologicalRuleAnalysis` nodes that produced its
    // input (correct and intended: the tree now shows the WHOLE derivation, analysis included, not
    // just the synthesis half) -- so "direct child" is too strict. `successful_rule_chains`'s own
    // DFS already guarantees `found` is a single root-to-leaf path in visitation order (it only
    // ever grows one `Vec` along the current recursion stack), so this loop's job is just to
    // confirm each step is reachable strictly below the previous one, not a sibling.
    fn is_descendant(sink: &TreeTraceSink, ancestor: TraceHandle, target: TraceHandle) -> bool {
        let mut stack = sink.node(ancestor).children;
        while let Some(h) = stack.pop() {
            if h == target {
                return true;
            }
            stack.extend(sink.node(h).children);
        }
        false
    }
    let mut cursor = root;
    for &step in &found {
        assert!(
            is_descendant(&sink, cursor, step),
            "chain node must be a descendant of the previous step -- got a non-nested collection"
        );
        cursor = step;
    }
}
