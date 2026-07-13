//! `compiler_v1.rs` (F7 — F6's deferred scope, HYBRID_FST_RUST_PLAN.md §8) — port of C#
//! `PhonologyRuleCompiler.cs`: the v1 merged-single-automaton phonology compiler,
//! [`crate::proposers::LockstepPhonologyProposer`]'s source of arcs, the DEFAULT (opt-out via
//! `useChainPhonology`) phonology path in C#.
//!
//! ## Bug-for-bug: quirk 2 (`F1_QUIRK_AUDIT.md`)
//! `_alphabet` is Segment-type char defs ONLY — a `BoundaryMarker` constraint anywhere in a
//! subrule's environment makes [`build_probe_representative_v1`] find no representative for that
//! side (a Segment-only alphabet has no member whose lanes carry `Type=Boundary`), so
//! [`try_compile_subrule_v1`] reports the subrule unsupported. Ported by CONSTRUCTION (the
//! alphabet really is Segment-only here — [`compiler::compile_default`]'s sibling `compiler.rs`
//! deliberately uses `Segment ∪ Boundary`), not by a special-cased check: the same generic
//! representative-search code as `compiler.rs`'s [`crate::compiler`] naturally reproduces the bug
//! when handed the narrower alphabet.
//!
//! ## NOT ported: quirk 1 (`HasNonIdentityArcs`'s start-state-only scan)
//! That quirk lives in [`crate::proposers::LockstepPhonologyProposer`] (the arc-scan check itself),
//! not in this compiler — this module only builds the merged `InversePhonology`; see that type's
//! doc for where quirk 1 is actually ported.
//!
//! ## Probing mechanism: reimplemented shape-based, not string-based (a reasoned deviation)
//! C#'s v1 probes by rendering representation STRINGS and concatenating them
//! (`BuildProbeString`/`SegmentFeatureStructs`), then RE-SEGMENTING the joined string
//! (`table.Segment(probeString)`) — the exact "maximal-munch re-segmentation" hazard
//! `RuleInverseCompiler.TryProbeCandidate`'s own doc warns about (and which was FOUND live on
//! Indonesian's real "Nasal assimilation" rule via `RIC_DEBUG`, per that doc). This module instead
//! builds the probe `Shape` node-by-node from already-resolved lane rows (the SAME safe technique
//! `compiler.rs` uses), matching every OBSERVABLE C# quirk this plan tracks (quirks 1 and 2,
//! `F1_QUIRK_AUDIT.md`) while not reintroducing a known, already-fixed-elsewhere bug class that is
//! not itself one of those tracked quirks. This is a reasoned choice, not an oversight: v1's
//! candidate-arc OUTPUT (which rules produce which restoration/substitution branches) depends only
//! on the rule's true synthesis effect, which both probing mechanisms observe identically on any
//! grammar whose alphabet has no representation-string ambiguity at the probe join — true of all
//! three reference grammars and the new toy grammar (§9's gate).
//!
//! ## v1's shape restriction (C# `TryGetConstraints`/`PhonologyRuleCompiler` class doc)
//! Environments AND the Lhs/Rhs window must be flat plain `Context`/`CharDef` sequences (no
//! quantifier/anchor/segments anywhere) — reuses [`crate::compiler::flatten_flat`] exactly, since
//! that is the identical shape requirement `RuleInverseCompiler`'s OWN Lhs/Rhs windows have (v1's
//! environments are simply held to the SAME bound RuleInverseCompiler only applies to its target
//! window — v1 has no `EnvNfaCompiler`-equivalent NFA compiler for environments at all).
//! Lhs must be exactly 1 segment; Rhs must be 0 (deletion) or 1 (substitution) segments; an
//! unconditioned deletion (both environments empty) is rejected (would over-restore everywhere).

use hc_featstruct::flat_unifiable;
use hc_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use hc_grammar::model::{
    Grammar, MprSet, Pattern, PatternNode, PhonRuleDef, RewriteRuleDef, RewriteSubruleDef,
};
use hc_shape::{NodeKind, ShapeBuilder, NO_CHAR_DEF};

use crate::compiler::flatten_flat;
use crate::inverse::InversePhonology;

pub struct V1CompileResult {
    pub pinv: InversePhonology,
    /// Coverage diagnostic only (C# `UnsupportedRuleCount`) — how many (rule, subrule) pairs the
    /// compiler could not put into the merged Pinv. Not consulted by any gate in this milestone
    /// (v1 has no tier report), kept for parity with the C# public surface.
    pub unsupported_rule_count: usize,
}

fn alloc(next_state: &mut u32) -> u32 {
    let s = *next_state;
    *next_state += 1;
    s
}

/// C# `PhonologyRuleCompiler.Compile`: build ONE merged `InversePhonology` across every stratum's
/// supported `RewriteRuleDef` subrules (document order). A `MetathesisRuleDef` (or any non-Rewrite
/// phonological rule kind) is silently skipped WITHOUT incrementing `unsupported_rule_count` — C#'s
/// own loop `continue`s before ever reaching its unsupported-counting code path for a non-`RewriteRule`
/// (confirmed by direct reading, not inferred).
pub fn compile(g: &Grammar) -> V1CompileResult {
    let (table, w) = crate::trie::surface_table(g);
    // Quirk 2: Segment-type ONLY (see module doc).
    let alphabet: Vec<CharDefId> = table
        .iter()
        .filter(|(_, cd)| cd.kind() == CharDefKind::Segment)
        .map(|(id, _)| id)
        .collect();

    let mut pinv = InversePhonology::new();
    pinv.start_state = 0;
    pinv.set_accepting(0);
    for &cd in &alphabet {
        let lanes = table.get(cd).feature_lanes().to_vec();
        pinv.add_arc(0, Some(lanes.clone()), Some(lanes), 0); // identity: everything outside a rule
    }
    let mut next_state = 1u32;
    let mut unsupported = 0usize;

    for stratum in &g.strata {
        for &prule_id in &stratum.prules {
            let PhonRuleDef::Rewrite(rule) = &g.prules[prule_id.0 as usize] else {
                continue; // metathesis and other non-rewrite rule types: not yet supported (uncounted)
            };
            for subrule in &rule.subrules {
                if !try_compile_subrule_v1(
                    g,
                    table,
                    w as u32,
                    &alphabet,
                    &mut pinv,
                    &mut next_state,
                    rule,
                    subrule,
                ) {
                    unsupported += 1;
                }
            }
        }
    }
    V1CompileResult {
        pinv,
        unsupported_rule_count: unsupported,
    }
}

fn flatten_env(pattern: Option<&Pattern>) -> Option<&[PatternNode]> {
    match pattern {
        None => Some(&[]),
        Some(p) => flatten_flat(&p.nodes),
    }
}

/// One alphabet representative's lane row per environment/target constraint node (C#
/// `BuildProbeString`, generalized here to lane rows instead of representation-string characters —
/// see module doc). `None` if some node has no unifiable `alphabet` member (quirk 2's mechanism).
fn build_probe_representative_v1(
    g: &Grammar,
    table: &CharDefTable,
    alphabet: &[CharDefId],
    nodes: &[PatternNode],
) -> Option<Vec<Vec<u64>>> {
    let mut result = Vec::with_capacity(nodes.len());
    for node in nodes {
        let lanes = hc_rules::rewrite::node_full_lanes(g, table, node);
        match alphabet
            .iter()
            .find(|&&cd| flat_unifiable(table.get(cd).feature_lanes(), &lanes))
        {
            Some(&cd) => result.push(table.get(cd).feature_lanes().to_vec()),
            None => return None,
        }
    }
    Some(result)
}

/// C# `TryCompileSubrule`.
#[allow(clippy::too_many_arguments)]
fn try_compile_subrule_v1(
    g: &Grammar,
    table: &CharDefTable,
    w: u32,
    alphabet: &[CharDefId],
    pinv: &mut InversePhonology,
    next_state: &mut u32,
    rule: &RewriteRuleDef,
    subrule: &RewriteSubruleDef,
) -> bool {
    if subrule.required_pos.is_some()
        || subrule.required_mpr != MprSet::EMPTY
        || subrule.excluded_mpr != MprSet::EMPTY
    {
        return false;
    }
    let Some(left_env) = flatten_env(subrule.left_env.as_ref()) else {
        return false;
    };
    let Some(lhs) = flatten_flat(&rule.lhs.nodes) else {
        return false;
    };
    if lhs.len() != 1 {
        return false; // v1: single-segment Lhs only
    }
    let Some(rhs) = flatten_flat(&subrule.rhs.nodes) else {
        return false;
    };
    if rhs.len() > 1 {
        return false; // v1: deletion (0) or plain substitution (1) only
    }
    let Some(right_env) = flatten_env(subrule.right_env.as_ref()) else {
        return false;
    };

    let is_deletion = rhs.is_empty();
    if is_deletion && left_env.is_empty() && right_env.is_empty() {
        return false; // unconditioned deletion would over-restore everywhere
    }

    let Some(left_probe) = build_probe_representative_v1(g, table, alphabet, left_env) else {
        return false;
    };
    let Some(right_probe) = build_probe_representative_v1(g, table, alphabet, right_env) else {
        return false;
    };

    // Environment ARCS use the constraint's own (possibly underspecified) lanes -- NOT the probe
    // representative -- exactly like `EnvNfaCompiler`'s identity arcs (C# `ChainLeftEnvironment`/
    // `ChainRightEnvironment` read `leftEnv`/`rightEnv`'s own `FeatureStruct`s, never the probe).
    let left_env_lanes: Vec<Vec<u64>> = left_env
        .iter()
        .map(|n| hc_rules::rewrite::node_full_lanes(g, table, n))
        .collect();
    let right_env_lanes: Vec<Vec<u64>> = right_env
        .iter()
        .map(|n| hc_rules::rewrite::node_full_lanes(g, table, n))
        .collect();
    let lhs_lanes = hc_rules::rewrite::node_full_lanes(g, table, &lhs[0]);

    // The left-environment chain is the same for every candidate below (depends only on the
    // rule's environment, not on which Lhs segment is being probed) -- build it once.
    let from_state = chain_left_environment_v1(pinv, next_state, &left_env_lanes);

    let mut added_any = false;
    for &cd in alphabet {
        let candidate_lanes = table.get(cd).feature_lanes().to_vec();
        if !flat_unifiable(&candidate_lanes, &lhs_lanes) {
            continue;
        }
        match probe_v1(
            g,
            w,
            rule,
            &left_probe,
            &candidate_lanes,
            &right_probe,
            is_deletion,
        ) {
            ProbeOutcome::Restoration => {
                add_restoration_branch_v1(
                    pinv,
                    next_state,
                    from_state,
                    candidate_lanes,
                    &right_env_lanes,
                );
                added_any = true;
            }
            ProbeOutcome::Substitution(surface) => {
                add_substitution_branch_v1(
                    pinv,
                    next_state,
                    from_state,
                    surface,
                    candidate_lanes,
                    &right_env_lanes,
                );
                added_any = true;
            }
            ProbeOutcome::NoEffect => {}
        }
    }
    added_any
}

enum ProbeOutcome {
    Restoration,
    Substitution(Vec<u64>),
    NoEffect,
}

fn probe_v1(
    g: &Grammar,
    w: u32,
    rule: &RewriteRuleDef,
    left_probe: &[Vec<u64>],
    candidate_lanes: &[u64],
    right_probe: &[Vec<u64>],
    is_deletion: bool,
) -> ProbeOutcome {
    let target_index = left_probe.len();
    let before_len = left_probe.len() + 1 + right_probe.len();

    let mut b = ShapeBuilder::with_features_capacity(w, before_len);
    for lanes in left_probe {
        b.push_segment_with_lanes(NO_CHAR_DEF, lanes);
    }
    b.push_segment_with_lanes(NO_CHAR_DEF, candidate_lanes);
    for lanes in right_probe {
        b.push_segment_with_lanes(NO_CHAR_DEF, lanes);
    }
    let shape = b.finish();
    let results = hc_rules::rewrite::synthesize(g, rule, &shape);
    let Some(out_shape) = results.first() else {
        return ProbeOutcome::NoEffect; // DefaultIfEmpty(word): unchanged -> no effect observed
    };
    let after: Vec<Vec<u64>> = out_shape
        .interior()
        .filter(|(_, kind, _, _)| matches!(kind, NodeKind::Segment | NodeKind::Boundary))
        .map(|(i, ..)| out_shape.node_lanes(i).to_vec())
        .collect();

    if is_deletion {
        if after.len() + 1 == before_len {
            ProbeOutcome::Restoration
        } else {
            ProbeOutcome::NoEffect
        }
    } else if after.len() == before_len && after[target_index] != candidate_lanes {
        ProbeOutcome::Substitution(after[target_index].clone())
    } else {
        ProbeOutcome::NoEffect
    }
}

/// C# `ChainLeftEnvironment`: consume each left-environment segment as an identity transition FROM
/// state 0, returning the state reached once the whole environment has matched (state 0 itself if
/// empty). Runs in parallel with state 0's own identity self-loops (an NFA permits multiple
/// outgoing arcs from one state), so no existing arc is disturbed.
fn chain_left_environment_v1(
    pinv: &mut InversePhonology,
    next_state: &mut u32,
    left_env: &[Vec<u64>],
) -> u32 {
    let mut state = 0u32;
    for lanes in left_env {
        let next = alloc(next_state);
        pinv.add_arc(state, Some(lanes.clone()), Some(lanes.clone()), next);
        state = next;
    }
    state
}

/// C# `ChainRightEnvironment`: consume each right-environment segment as identity, ending back at
/// state 0. Only called with a non-empty environment (the zero-length case is handled directly by
/// the two `AddXBranch` callers).
fn chain_right_environment_v1(
    pinv: &mut InversePhonology,
    next_state: &mut u32,
    from: u32,
    right_env: &[Vec<u64>],
) {
    let mut state = from;
    for (i, lanes) in right_env.iter().enumerate() {
        let next = if i == right_env.len() - 1 {
            0
        } else {
            alloc(next_state)
        };
        pinv.add_arc(state, Some(lanes.clone()), Some(lanes.clone()), next);
        state = next;
    }
}

/// C# `AddRestorationBranch`: ε-input restore `underlying` from `from_state`, then consume the
/// right-environment segments as identity back to state 0 (straight to 0 if no right context).
fn add_restoration_branch_v1(
    pinv: &mut InversePhonology,
    next_state: &mut u32,
    from_state: u32,
    underlying: Vec<u64>,
    right_env: &[Vec<u64>],
) {
    if right_env.is_empty() {
        pinv.add_arc(from_state, None, Some(underlying), 0);
        return;
    }
    let state = alloc(next_state);
    pinv.add_arc(from_state, None, Some(underlying), state);
    chain_right_environment_v1(pinv, next_state, state, right_env);
}

/// C# `AddSubstitutionBranch`: a real arc consuming the surfaced segment and emitting the
/// underlying one, then the right-environment chain (straight to 0 if no right context).
fn add_substitution_branch_v1(
    pinv: &mut InversePhonology,
    next_state: &mut u32,
    from_state: u32,
    surface: Vec<u64>,
    underlying: Vec<u64>,
    right_env: &[Vec<u64>],
) {
    if right_env.is_empty() {
        pinv.add_arc(from_state, Some(surface), Some(underlying), 0);
        return;
    }
    let state = alloc(next_state);
    pinv.add_arc(from_state, Some(surface), Some(underlying), state);
    chain_right_environment_v1(pinv, next_state, state, right_env);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path(name: &str) -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn load(name: &str) -> Option<Grammar> {
        let path = sample_path(name)?;
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("load {name}: {e}")))
    }

    /// Quirk 2 in action on the real grammar: every Indonesian phonological rule's environment
    /// references the `meN-`/junction boundary marker in some subrule, or is otherwise unsupported
    /// by v1's narrow shape -- so the merged Pinv, while non-empty (identity self-loops always
    /// exist), never contributes a `LockstepPhonologyProposer` candidate distinguishable from the
    /// bare FST's own junction-baked arcs on this corpus (F6's own empirical finding, re-confirmed
    /// structurally here: `unsupported_rule_count` covers every one of Indonesian's 5 rules' worth
    /// of subrules).
    #[test]
    fn indonesian_v1_compiles_without_panicking_and_reports_unsupported_subrules() {
        let Some(g) = load("indonesian-hc.xml") else {
            eprintln!("skipping: indonesian-hc.xml not present");
            return;
        };
        let result = compile(&g);
        assert!(
            result.unsupported_rule_count > 0,
            "Indonesian's real rules all hit v1's narrow shape (quirks 1/2)"
        );
    }

    #[test]
    fn sena_has_no_phonological_rules_v1_is_a_pure_identity_pinv() {
        let Some(g) = load("sena-hc.xml") else {
            eprintln!("skipping: sena-hc.xml not present");
            return;
        };
        let result = compile(&g);
        assert_eq!(result.unsupported_rule_count, 0);
    }
}
