//! `env_nfa.rs` (F7, HYBRID_FST_RUST_PLAN.md §8) — port of C# `EnvNfaCompiler.cs`: compiles an
//! environment [`Pattern`] (a `RewriteSubruleDef`'s `left_env`/`right_env`, or a rule's `lhs`) into
//! an identity pass-through NFA fragment inside an [`InversePhonology`].
//!
//! ## Node inventory: narrower than C#'s, by construction, not by omission
//! C#'s `SIL.Machine.Matching` pattern tree has FOUR node kinds environments can use: `Constraint`,
//! `Group`, `Quantifier`, `Alternation` (`EnvNfaCompiler.cs`'s own module doc confirms this is the
//! COMPLETE inventory for that library). `hc_grammar::model::PatternNode` — the loader's OWN
//! already-flattened representation (`LoadPatternNodes`/`LoadPhoneticTemplate`) — has no `Group` or
//! `Alternation` variant at all: `Group` was a pure C#-side naming wrapper (used for named LHS
//! captures elsewhere, not authored in `<Environment>` XML), and grepping every model/loader site
//! confirms no `<Alternation>` element is modeled anywhere in this port's grammar loader — the DTD
//! subset `<Environment>`/`<PhoneticInput>`/`<PhoneticOutput>` patterns actually use is
//! `SimpleContext` (natural-class `Constraint`) / `CharDef` (concrete-segment `Constraint`) /
//! `Quantifier` (`<OptionalSegmentSequence>`, itself flattening `Group(children)` into a plain
//! `children: Vec<PatternNode>`) / `Anchor` (word-edge). This module therefore compiles exactly
//! that four-case inventory — narrower than C#'s dispatch not because a case was skipped, but
//! because the cases that don't exist in this model's `PatternNode` enum were already resolved (or
//! never representable) at grammar-load time. `PatternNode::Segments` (a pre-segmented literal
//! shape, used by morphological-rule LHS parts, not phonological environments in any reference
//! grammar) is handled defensively like C#'s `default:` case — a reason is recorded and the
//! fragment passes through unchanged (a safe superset, matching the plan's "superset, never silent
//! skip" principle), never a hard error.
//!
//! ## Anchors → Permissive (unchanged from C#)
//! A word-edge [`PatternNode::Anchor`] is a zero-width position assertion this window-local
//! automaton has no way to check (states carry no word-position information) — dropped (the
//! fragment continues from the same state) and recorded in [`EnvCompileResult::reasons`] so the
//! caller tiers the rule Permissive rather than silently over-claim Exact.
//!
//! ## Boundary markers need no special handling here
//! A `PatternNode::CharDef` naming a `Boundary`-kind char def compiles as an ordinary identity arc,
//! exactly like a phonological segment — this is `RuleInverseCompiler`'s boundary-representative
//! fix (v1's `PhonologyRuleCompiler` excludes boundaries from its alphabet entirely; this compiler
//! doesn't, `F1_QUIRK_AUDIT.md` quirk 2).

use hc_grammar::chardef::CharDefTable;
use hc_grammar::model::{Grammar, Pattern, PatternNode};

use crate::inverse::{InversePhonology, StateId};

/// Result of compiling one environment pattern: the state reached once the whole pattern has
/// matched, plus any precision dropped along the way (e.g. `"anchor"`) — the caller tiers Exact iff
/// this is empty after compiling every fragment a subrule needs.
pub struct EnvCompileResult {
    pub end_state: StateId,
    pub reasons: Vec<String>,
}

/// Compile `pattern`'s children in sequence from `start`, returning the state reached after the
/// whole pattern matches (C# `EnvNfaCompiler.Compile`). `pattern = None` is a no-op — matches
/// `RewriteSubruleDef`'s `None` `left_env`/`right_env` convention (unconditioned side).
/// `next_state` mints fresh state ids (shared with the caller's own counter, e.g.
/// `RuleInverseCompiler`'s `_next_state`).
pub fn compile_env(
    g: &Grammar,
    table: &CharDefTable,
    pattern: Option<&Pattern>,
    pinv: &mut InversePhonology,
    next_state: &mut u32,
    start: StateId,
) -> EnvCompileResult {
    let mut reasons = Vec::new();
    let end_state = match pattern {
        None => start,
        Some(p) => compile_sequence(g, table, &p.nodes, pinv, next_state, start, &mut reasons),
    };
    EnvCompileResult { end_state, reasons }
}

fn alloc(next_state: &mut u32) -> StateId {
    let s = *next_state;
    *next_state += 1;
    s
}

fn compile_sequence(
    g: &Grammar,
    table: &CharDefTable,
    nodes: &[PatternNode],
    pinv: &mut InversePhonology,
    next_state: &mut u32,
    start: StateId,
    reasons: &mut Vec<String>,
) -> StateId {
    let mut state = start;
    for node in nodes {
        state = compile_node(g, table, node, pinv, next_state, state, reasons);
    }
    state
}

fn compile_node(
    g: &Grammar,
    table: &CharDefTable,
    node: &PatternNode,
    pinv: &mut InversePhonology,
    next_state: &mut u32,
    start: StateId,
    reasons: &mut Vec<String>,
) -> StateId {
    match node {
        PatternNode::Context(_) | PatternNode::CharDef(_) => {
            let lanes = hc_rules::rewrite::node_full_lanes(g, table, node);
            let next = alloc(next_state);
            pinv.add_arc(start, Some(lanes.clone()), Some(lanes), next); // identity pass-through
            next
        }
        PatternNode::Quantifier { min, max, children } => compile_quantifier(
            g, table, *min, *max, children, pinv, next_state, start, reasons,
        ),
        PatternNode::Anchor(_) => {
            add_reason(reasons, "anchor");
            start
        }
        PatternNode::Segments { .. } => {
            // Not authored in any reference grammar's phonological environments/LHS/RHS (see
            // module doc); handled defensively like C#'s `default:` case — record and pass
            // through rather than silently drop or panic.
            add_reason(reasons, "unsupported-node:Segments");
            start
        }
    }
}

/// C# `CompileQuantifier`: `min` mandatory copies chained directly (no epsilon — each copy's end
/// state IS the next copy's start). Unbounded (`max.is_none()`, C#'s `Infinite`): one more copy
/// becomes the loopable unit, with a structural epsilon from its end back to its own start —
/// `start`-after-`min` is returned as the exit (0 further iterations needs no arc; any count ≥ 0
/// further iterations is reachable via the loop-back cycle). Bounded: unroll the remaining
/// `max - min` copies, each optionally skippable via a structural epsilon to one shared
/// final-exit state.
#[allow(clippy::too_many_arguments)]
fn compile_quantifier(
    g: &Grammar,
    table: &CharDefTable,
    min: u32,
    max: Option<u32>,
    children: &[PatternNode],
    pinv: &mut InversePhonology,
    next_state: &mut u32,
    start: StateId,
    reasons: &mut Vec<String>,
) -> StateId {
    if children.is_empty() {
        return start; // an empty quantifier body contributes nothing
    }

    let mut state = start;
    for _ in 0..min {
        state = compile_sequence(g, table, children, pinv, next_state, state, reasons);
    }

    match max {
        None => {
            let loop_exit = compile_sequence(g, table, children, pinv, next_state, state, reasons);
            pinv.add_epsilon(loop_exit, state); // repeat: loop back
            state // stop: 0+ further iterations, no arc needed to "exit" the loop
        }
        Some(max) if max == min => state, // exact count -- no optional copies, no extra state needed
        Some(max) => {
            let final_exit = alloc(next_state);
            pinv.add_epsilon(state, final_exit); // 0 of the optional copies
            let mut cur = state;
            for _ in min..max {
                let next = compile_sequence(g, table, children, pinv, next_state, cur, reasons);
                pinv.add_epsilon(next, final_exit); // stop after this optional copy
                cur = next;
            }
            final_exit
        }
    }
}

fn add_reason(reasons: &mut Vec<String>, reason: &str) {
    if !reasons.iter().any(|r| r == reason) {
        reasons.push(reason.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_grammar::model::AnchorSide;

    fn tiny_grammar_with_feature() -> Grammar {
        // The smallest grammar exercising a real phonological feature + a segment/boundary char
        // def, built via the XML loader (no hand-built `Grammar` constructor exists/should exist --
        // the loader is the only supported entry point). Schema mirrors `hc-rules`'s own
        // `tests/common/mod.rs::GRAMMAR_XML` (verified-working probe grammar), reduced further.
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>EnvNfaProbe</Name>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voi">
        <Name>voi</Name>
        <Symbols>
          <Symbol id="sym_vp">+</Symbol>
          <Symbol id="sym_vm">-</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a">
          <Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_p">
          <Representations><Representation>p</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vm" />
        </SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="char_bnd">
          <Representations><Representation>+</Representation></Representations>
        </BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;
        hc_grammar::load(xml).expect("tiny env_nfa fixture grammar loads")
    }

    #[test]
    fn empty_pattern_is_a_no_op() {
        let g = tiny_grammar_with_feature();
        let table = &g.char_tables[0];
        let mut pinv = InversePhonology::new();
        let mut next_state = 1u32;
        let result = compile_env(&g, table, None, &mut pinv, &mut next_state, 0);
        assert_eq!(result.end_state, 0);
        assert!(result.reasons.is_empty());
        assert!(pinv.arcs_from(0).is_empty());
    }

    #[test]
    fn chardef_constraint_compiles_one_identity_arc() {
        let g = tiny_grammar_with_feature();
        let table = &g.char_tables[0];
        let cd_p = table
            .iter()
            .find(|(_, cd)| cd.representations() == ["p"])
            .unwrap()
            .0;
        let pattern = Pattern {
            nodes: vec![PatternNode::CharDef(cd_p)],
        };
        let mut pinv = InversePhonology::new();
        let mut next_state = 1u32;
        let result = compile_env(&g, table, Some(&pattern), &mut pinv, &mut next_state, 0);
        assert!(result.reasons.is_empty());
        assert_ne!(result.end_state, 0);
        let arcs = pinv.arcs_from(0);
        assert_eq!(arcs.len(), 1);
        assert!(!arcs[0].is_epsilon_input() && !arcs[0].is_epsilon_output());
        assert_eq!(arcs[0].target, result.end_state);
    }

    #[test]
    fn anchor_drops_and_records_reason_without_advancing_state() {
        let g = tiny_grammar_with_feature();
        let table = &g.char_tables[0];
        let pattern = Pattern {
            nodes: vec![PatternNode::Anchor(AnchorSide::Left)],
        };
        let mut pinv = InversePhonology::new();
        let mut next_state = 1u32;
        let result = compile_env(&g, table, Some(&pattern), &mut pinv, &mut next_state, 0);
        assert_eq!(result.end_state, 0, "anchor is a zero-width no-op fragment");
        assert_eq!(result.reasons, vec!["anchor".to_string()]);
        assert!(pinv.arcs_from(0).is_empty());
    }

    #[test]
    fn unbounded_quantifier_loops_back_to_its_own_start() {
        let g = tiny_grammar_with_feature();
        let table = &g.char_tables[0];
        let cd_p = table
            .iter()
            .find(|(_, cd)| cd.representations() == ["p"])
            .unwrap()
            .0;
        let pattern = Pattern {
            nodes: vec![PatternNode::Quantifier {
                min: 0,
                max: None,
                children: vec![PatternNode::CharDef(cd_p)],
            }],
        };
        let mut pinv = InversePhonology::new();
        let mut next_state = 1u32;
        let result = compile_env(&g, table, Some(&pattern), &mut pinv, &mut next_state, 0);
        assert_eq!(
            result.end_state, 0,
            "0+ further iterations exit at the loop's own start"
        );
        // One identity arc out of state 0 (into the loop body), and a loop-back epsilon from the
        // body's end back to state 0.
        let arcs = pinv.arcs_from(0);
        assert_eq!(arcs.len(), 1);
        let body_end = arcs[0].target;
        let body_arcs = pinv.arcs_from(body_end);
        assert_eq!(body_arcs.len(), 1);
        assert!(body_arcs[0].is_structural_epsilon());
        assert_eq!(body_arcs[0].target, 0);
    }

    #[test]
    fn bounded_quantifier_offers_a_skip_epsilon_to_a_shared_exit() {
        let g = tiny_grammar_with_feature();
        let table = &g.char_tables[0];
        let cd_p = table
            .iter()
            .find(|(_, cd)| cd.representations() == ["p"])
            .unwrap()
            .0;
        let pattern = Pattern {
            nodes: vec![PatternNode::Quantifier {
                min: 0,
                max: Some(2),
                children: vec![PatternNode::CharDef(cd_p)],
            }],
        };
        let mut pinv = InversePhonology::new();
        let mut next_state = 1u32;
        let result = compile_env(&g, table, Some(&pattern), &mut pinv, &mut next_state, 0);
        // state 0: one skip-epsilon to final_exit, one real arc into the first optional copy.
        let arcs0 = pinv.arcs_from(0);
        assert_eq!(arcs0.len(), 2);
        assert!(arcs0
            .iter()
            .any(|a| a.is_structural_epsilon() && a.target == result.end_state));
        assert!(arcs0.iter().any(|a| !a.is_structural_epsilon()));
    }
}
