//! `compiler.rs` (F7, HYBRID_FST_RUST_PLAN.md §8) — port of C# `RuleInverseCompiler.cs`: for each
//! `RewriteRuleDef`, build its OWN [`InversePhonology`] by probing that ONE rule's synthesis
//! behavior in isolation (`hc_rules::rewrite::synthesize`, the Rust `IPhonologicalRule.
//! CompileSynthesisRule` analog), reporting a per-rule [`RuleInverseTier`] + reason list.
//!
//! ## Scope for this milestone: `RewriteRuleDef` only, `MetathesisRuleDef` deferred
//! C#'s `RuleInverseCompiler` (I5) also compiles `MetathesisRule`s via the same probe technique
//! (`CompileMetathesisRule`). **This port defers that** — see [`compile_metathesis_stub`]'s doc.
//! None of the three reference grammars (Indonesian/Sena/Amharic) declares a `<MetathesisRule>`
//! (confirmed: all three tier-report goldens list only `RewriteRuleDef`-shaped rules, and Sena has
//! zero phonological rules of any kind), so this has **zero observable impact on any F7 gate**
//! (tier reports, chain-on corpus parity, the I4 marquee, the new toy-grammar gate — the toy
//! grammar itself is a plain substitution `RewriteRuleDef`, per this milestone's own design
//! rationale). Registering an encountered `MetathesisRuleDef` as `IdentitySkip` (identity-only
//! Pinv, `"metathesis-unported"` reason) rather than silently dropping it from the compiled list
//! keeps the invariant "an absent-from-the-chain rule behaves as identity" intact for the one
//! caller (`ChainPhonologyProposer`) that would otherwise need to know the difference.
//!
//! ## Probing technique
//! Mirrors `TryProbeCandidate`: build a tiny [`hc_shape::Shape`] directly from already-resolved
//! concrete lane rows (env representatives + one Lhs candidate combo), apply the WHOLE rule
//! forward via [`hc_rules::rewrite::synthesize`], and read back the observed effect. Building the
//! probe `Shape` node-by-node from known lane rows (never by rendering representation strings and
//! re-segmenting the concatenation) sidesteps the exact ambiguity `TryProbeCandidate`'s own doc
//! warns about (a table's maximal-munch segmentation merging two adjacent probe pieces into an
//! unintended grapheme at the join).
//!
//! ## Representation notes
//! - Every probe/env-representative segment's `char_def` is built as [`hc_shape::NO_CHAR_DEF`] —
//!   `synthesize`'s matching machinery (`hc_rules::bridge::PatternBridge`) reads only a node's
//!   `lanes`, never its `char_def`, for target/environment matching, so this is a safe, honest
//!   "this is a representative segment, not a literal lexical one" tag (the same convention
//!   `hc_rules::rewrite`'s own post-rewrite nodes use, per that module's `char_def reset` note).
//! - [`inverse::Arc`] surface/underlying lanes only (no char-def dimension) — see `inverse.rs`'s
//!   own PARITY note for why this is exact on every grammar this milestone gates (all three
//!   reference grammars plus the new toy grammar declare real phonological features).

use hc_featstruct::flat_unifiable;
use hc_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use hc_grammar::model::{Dir, Grammar, MetathesisRuleDef, MprSet, Pattern, PatternNode, PhonRuleDef, RewriteRuleDef};
use hc_shape::{NodeKind, ShapeBuilder, NO_CHAR_DEF};

use crate::env_nfa;
use crate::inverse::{InversePhonology, StateId};

/// Cap on Lhs segment count (C# `MaxLhsSegments`): probing enumerates `alphabet^N` candidates.
pub const MAX_LHS_SEGMENTS: usize = 3;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RuleInverseTier {
    Exact,
    Permissive,
    IdentitySkip,
}

pub struct CompiledRuleInverse {
    /// The rule's `<Name>` (empty string if unauthored — no reference grammar rule lacks one, but
    /// this never panics on a hand-built toy fixture that does).
    pub name: String,
    pub pinv: InversePhonology,
    pub tier: RuleInverseTier,
    /// Why this rule isn't Exact (empty iff Exact) — deduped, first-seen order (matches C#
    /// `AddReason`'s `if (!reasons.Contains(reason))`).
    pub reasons: Vec<String>,
}

fn add_reason(reasons: &mut Vec<String>, reason: &str) {
    if !reasons.iter().any(|r| r == reason) {
        reasons.push(reason.to_string());
    }
}

/// C# `Compile(Language, Morpher)`'s default: `Morpher.DeletionReapplications + 1`. **PARITY
/// note:** the Rust engine port (`hc-parse`) does not surface a `DeletionReapplications` knob at
/// all (grepped: no such field/config exists anywhere in `hc-parse`/`hc-grammar`), so this cannot
/// literally read it back the way C# does. C#'s own default value for that knob is `0` (the doc on
/// `RuleInverseCompiler.Compile`'s ctor explains the `+1` mirrors "deletion applies once
/// unconditionally, `DeletionReapplications` counts further reapplications" — with the default
/// `0`, exactly one round), so this hardcodes the RESULT of that default (`1`) rather than the
/// absent knob — behavior-identical to C#'s own default on every grammar in this port (none
/// exercises a non-default `DeletionReapplications`). [`compile`] takes an explicit
/// `restoration_cap` for a caller that needs a different value, mirroring C#'s second `Compile`
/// overload.
pub const DEFAULT_RESTORATION_CAP: i32 = 1;

/// C# `Compile(Language, Morpher, int)`: compile every stratum's `RewriteRuleDef`s (document
/// order — `g.strata`, then each stratum's `prules` in its own authored order) into their own
/// [`CompiledRuleInverse`]. A `MetathesisRuleDef` is registered as `IdentitySkip` (see this
/// module's doc for why full metathesis probing is deferred).
pub fn compile(g: &Grammar, restoration_cap: i32) -> Vec<CompiledRuleInverse> {
    let (table, _w) = crate::trie::surface_table(g);
    let segment_alphabet: Vec<CharDefId> =
        table.iter().filter(|(_, cd)| cd.kind() == CharDefKind::Segment).map(|(id, _)| id).collect();
    let probe_alphabet: Vec<CharDefId> = table.iter().map(|(id, _)| id).collect(); // Segment ∪ Boundary = every char def

    let mut results = Vec::new();
    for stratum in &g.strata {
        for &prule_id in &stratum.prules {
            match &g.prules[prule_id.0 as usize] {
                PhonRuleDef::Rewrite(rule) => {
                    results.push(compile_rewrite_rule(g, table, &segment_alphabet, &probe_alphabet, rule, restoration_cap));
                }
                PhonRuleDef::Metathesis(mrule) => {
                    results.push(compile_metathesis_stub(&probe_alphabet, table, mrule));
                }
            }
        }
    }
    results
}

/// [`compile`] with [`DEFAULT_RESTORATION_CAP`] (C# `Compile(Language, Morpher)`).
pub fn compile_default(g: &Grammar) -> Vec<CompiledRuleInverse> {
    compile(g, DEFAULT_RESTORATION_CAP)
}

/// Deferred `MetathesisRuleDef` handling (see module doc): identity-only Pinv (every probe
/// alphabet member self-loops at the one accepting state, exactly `RuleInverseTier::IdentitySkip`'s
/// documented contract), reason `"metathesis-unported"`.
fn compile_metathesis_stub(
    probe_alphabet: &[CharDefId],
    table: &CharDefTable,
    mrule: &MetathesisRuleDef,
) -> CompiledRuleInverse {
    let mut pinv = InversePhonology::new();
    pinv.start_state = 0;
    pinv.set_accepting(0);
    for &cd in probe_alphabet {
        let lanes = table.get(cd).feature_lanes().to_vec();
        pinv.add_arc(0, Some(lanes.clone()), Some(lanes), 0);
    }
    CompiledRuleInverse {
        name: mrule.name.clone().unwrap_or_default(),
        pinv,
        tier: RuleInverseTier::IdentitySkip,
        reasons: vec!["metathesis-unported".to_string()],
    }
}

fn alloc(next_state: &mut u32) -> StateId {
    let s = *next_state;
    *next_state += 1;
    s
}

/// A flat sequence of plain `Context`/`CharDef` nodes — the bounded shape this compiler's Lhs/Rhs
/// windows support (C# `FlattenFlatConstraints`). `None` if any node isn't one of those two kinds
/// (a quantifier/anchor/segments directly in the target window — not attempted). `pub(crate)`:
/// `compiler_v1.rs` (F7) reuses this exact shape check for its OWN Lhs/Rhs/environment windows (C#
/// `PhonologyRuleCompiler.TryGetConstraints` requires precisely the same flat plain-constraint
/// shape, for both the target window AND the environments — v1 has no `EnvNfaCompiler`-equivalent
/// at all).
pub(crate) fn flatten_flat(nodes: &[PatternNode]) -> Option<&[PatternNode]> {
    if nodes.iter().all(|n| matches!(n, PatternNode::Context(_) | PatternNode::CharDef(_))) {
        Some(nodes)
    } else {
        None
    }
}

fn node_has_vars(node: &PatternNode) -> bool {
    matches!(node, PatternNode::Context(sc) if !sc.vars.is_empty())
}

fn pattern_has_vars(p: Option<&Pattern>) -> bool {
    p.is_some_and(|p| p.nodes.iter().any(node_has_vars))
}

/// One probed target-window arc (C# `ArcSpec`): `surface = None` is a deletion-inverse restoration
/// (ε-input); `underlying = None` is an epenthesis-inverse consume (ε-output).
struct ArcSpec {
    surface: Option<Vec<u64>>,
    underlying: Option<Vec<u64>>,
}

/// One subrule's probed compile result (C# `SubruleSpec`).
struct SubruleSpec<'a> {
    left_env: Option<&'a Pattern>,
    right_env: Option<&'a Pattern>,
    is_restoration_event: bool,
    candidates: Vec<Vec<ArcSpec>>,
}

/// One environment/target representative segment: its lane row plus whether it names a Segment or
/// Boundary char def (so a probe `Shape` pushes it via the matching `ShapeBuilder` method).
#[derive(Clone)]
struct ProbeSeg {
    lanes: Vec<u64>,
    kind: CharDefKind,
}

/// C# `CompileRule`: probe phase (classify + probe each subrule) then emission phase (lay the
/// automaton down once per restoration "floor").
fn compile_rewrite_rule(
    g: &Grammar,
    table: &CharDefTable,
    segment_alphabet: &[CharDefId],
    probe_alphabet: &[CharDefId],
    rule: &RewriteRuleDef,
    restoration_cap: i32,
) -> CompiledRuleInverse {
    let mut next_state = 1u32; // state 0 reserved for floor 0's base
    let mut pinv = InversePhonology::new();
    pinv.start_state = 0;

    let mut reasons: Vec<String> = Vec::new();
    let mut specs: Vec<SubruleSpec> = Vec::new();

    match flatten_flat(&rule.lhs.nodes) {
        None => add_reason(&mut reasons, "lhs-not-flat"),
        Some(lhs) if lhs.len() > MAX_LHS_SEGMENTS => add_reason(&mut reasons, "lhs-too-long"),
        Some(lhs) => {
            for subrule in &rule.subrules {
                if let Some(spec) =
                    try_build_subrule_spec(g, table, segment_alphabet, probe_alphabet, rule, subrule, lhs, &mut reasons)
                {
                    specs.push(spec);
                }
            }
        }
    }

    if rule.dir == Dir::RightToLeft {
        add_reason(&mut reasons, "direction");
    }

    let mut any_restoration = specs.iter().any(|s| s.is_restoration_event);
    if any_restoration && restoration_cap <= 0 {
        add_reason(&mut reasons, "restoration-cap");
        specs.retain(|s| !s.is_restoration_event);
        any_restoration = false;
    }
    let floors: usize = if any_restoration { restoration_cap as usize + 1 } else { 1 };
    let mut floor_base = vec![0u32; floors];
    for f in floor_base.iter_mut().enumerate().skip(1).map(|(_, fb)| fb) {
        *f = alloc(&mut next_state);
    }

    let mut any_compiled = false;
    for f in 0..floors {
        pinv.set_accepting(floor_base[f]);
        for &cd in probe_alphabet {
            let lanes = table.get(cd).feature_lanes().to_vec();
            pinv.add_arc(floor_base[f], Some(lanes.clone()), Some(lanes), floor_base[f]);
        }
        for spec in &specs {
            if spec.is_restoration_event && f == floors - 1 {
                continue; // top floor: restoration budget spent -- no further deletion branches
            }
            let rejoin = if spec.is_restoration_event { floor_base[f + 1] } else { floor_base[f] };
            emit_subrule(g, table, &mut pinv, &mut next_state, spec, floor_base[f], rejoin, &mut reasons);
            any_compiled = true;
        }
    }

    let tier = if !any_compiled {
        RuleInverseTier::IdentitySkip
    } else if !reasons.is_empty() {
        RuleInverseTier::Permissive
    } else {
        RuleInverseTier::Exact
    };
    CompiledRuleInverse { name: rule.name.clone().unwrap_or_default(), pinv, tier, reasons }
}

/// C# `TryBuildSubruleSpec`.
#[allow(clippy::too_many_arguments)]
fn try_build_subrule_spec<'a>(
    g: &Grammar,
    table: &CharDefTable,
    segment_alphabet: &[CharDefId],
    probe_alphabet: &[CharDefId],
    rule: &RewriteRuleDef,
    subrule: &'a hc_grammar::model::RewriteSubruleDef,
    lhs: &[PatternNode],
    reasons: &mut Vec<String>,
) -> Option<SubruleSpec<'a>> {
    if subrule.required_pos.is_some() || subrule.required_mpr != MprSet::EMPTY || subrule.excluded_mpr != MprSet::EMPTY {
        // I1's call, unchanged: the gate is DROPPED (a sound superset), reported so the tier
        // report shows the cost.
        add_reason(reasons, "mpr-or-syntactic-gate");
    }

    let rhs = match flatten_flat(&subrule.rhs.nodes) {
        None => {
            add_reason(reasons, "rhs-not-flat");
            return None;
        }
        Some(r) => r,
    };
    if lhs.is_empty() && rhs.is_empty() {
        add_reason(reasons, "empty-subrule");
        return None;
    }

    if lhs.iter().any(node_has_vars)
        || rhs.iter().any(node_has_vars)
        || pattern_has_vars(subrule.left_env.as_ref())
        || pattern_has_vars(subrule.right_env.as_ref())
    {
        add_reason(reasons, "alpha-variable");
    }

    let left_probe = build_probe_representative(g, table, probe_alphabet, subrule.left_env.as_ref());
    let right_probe = build_probe_representative(g, table, probe_alphabet, subrule.right_env.as_ref());
    let (left_probe, right_probe) = match (left_probe, right_probe) {
        (Some(l), Some(r)) => (l, r),
        _ => {
            add_reason(reasons, "env-representative-not-found");
            return None;
        }
    };
    if lhs.is_empty() && left_probe.is_empty() && right_probe.is_empty() {
        add_reason(reasons, "epenthesis-unprobeable");
        return None;
    }

    let mut spec = SubruleSpec {
        left_env: subrule.left_env.as_ref(),
        right_env: subrule.right_env.as_ref(),
        is_restoration_event: rhs.len() < lhs.len(),
        candidates: Vec::new(),
    };
    for combo in enumerate_lhs_candidates(g, table, segment_alphabet, lhs) {
        if let Some(arcs) = try_probe_candidate(g, table, rule, &combo, rhs.len(), &left_probe, &right_probe) {
            spec.candidates.push(arcs);
        }
    }
    if spec.candidates.is_empty() {
        add_reason(reasons, "no-effect");
        return None;
    }
    Some(spec)
}

/// C# `BuildProbeRepresentative`/`AppendNodeRepresentative`: one alphabet representative's lane row
/// per constraint node (never round-tripped through a representation string), `MinOccur` copies of
/// a quantifier's body, nothing for an anchor. `None` if some constraint has no unifiable `table`
/// member in `probe_alphabet`.
fn build_probe_representative(
    g: &Grammar,
    table: &CharDefTable,
    probe_alphabet: &[CharDefId],
    pattern: Option<&Pattern>,
) -> Option<Vec<ProbeSeg>> {
    let mut result = Vec::new();
    if let Some(p) = pattern {
        if !append_representative(g, table, probe_alphabet, &p.nodes, &mut result) {
            return None;
        }
    }
    Some(result)
}

fn append_representative(
    g: &Grammar,
    table: &CharDefTable,
    probe_alphabet: &[CharDefId],
    nodes: &[PatternNode],
    result: &mut Vec<ProbeSeg>,
) -> bool {
    for node in nodes {
        if !append_node_representative(g, table, probe_alphabet, node, result) {
            return false;
        }
    }
    true
}

fn append_node_representative(
    g: &Grammar,
    table: &CharDefTable,
    probe_alphabet: &[CharDefId],
    node: &PatternNode,
    result: &mut Vec<ProbeSeg>,
) -> bool {
    match node {
        PatternNode::Anchor(_) => true, // zero-width; the probe word's own edges satisfy it
        PatternNode::Context(_) | PatternNode::CharDef(_) => {
            let lanes = hc_rules::rewrite::node_full_lanes(g, table, node);
            match probe_alphabet.iter().find(|&&cd| flat_unifiable(table.get(cd).feature_lanes(), &lanes)) {
                Some(&cd) => {
                    result.push(ProbeSeg { lanes: table.get(cd).feature_lanes().to_vec(), kind: table.get(cd).kind() });
                    true
                }
                None => false,
            }
        }
        PatternNode::Quantifier { min, children, .. } => {
            for _ in 0..*min {
                if !append_representative(g, table, probe_alphabet, children, result) {
                    return false;
                }
            }
            true
        }
        PatternNode::Segments { .. } => false, // not authored in any reference grammar's rule patterns
    }
}

/// C# `EnumerateLhsCandidates`/`CartesianProduct`: every combination of `segment_alphabet` members
/// unifiable with each Lhs position, in POSITION-0-SLOWEST, LAST-POSITION-FASTEST order (matching
/// C#'s recursive `CartesianProduct(pools, combo, index)`, which iterates `pools[index]` in an
/// OUTER loop around the recursive call for `index+1`) -- this order becomes candidate ARC
/// enumeration order at walk time, so it is preserved exactly even though the tier-report gate
/// itself doesn't depend on it.
fn enumerate_lhs_candidates(
    g: &Grammar,
    table: &CharDefTable,
    segment_alphabet: &[CharDefId],
    lhs: &[PatternNode],
) -> Vec<Vec<CharDefId>> {
    let pools: Vec<Vec<CharDefId>> = lhs
        .iter()
        .map(|node| {
            let lanes = hc_rules::rewrite::node_full_lanes(g, table, node);
            segment_alphabet.iter().copied().filter(|&cd| flat_unifiable(table.get(cd).feature_lanes(), &lanes)).collect()
        })
        .collect();
    if pools.iter().any(|p| p.is_empty()) {
        return Vec::new();
    }
    let mut combos: Vec<Vec<CharDefId>> = vec![Vec::new()];
    for pool in &pools {
        let mut next = Vec::with_capacity(combos.len() * pool.len());
        for combo in &combos {
            for &cd in pool {
                let mut c = combo.clone();
                c.push(cd);
                next.push(c);
            }
        }
        combos = next;
    }
    combos
}

/// C# `TryProbeCandidate`: probe one concrete Lhs candidate through the rule's own compiled
/// synthesis rule.
fn try_probe_candidate(
    g: &Grammar,
    table: &CharDefTable,
    rule: &RewriteRuleDef,
    combo: &[CharDefId],
    rhs_count: usize,
    left_probe: &[ProbeSeg],
    right_probe: &[ProbeSeg],
) -> Option<Vec<ArcSpec>> {
    let w = g.phon_features.len() as u32;
    let mut before: Vec<(Vec<u64>, CharDefKind)> = Vec::with_capacity(left_probe.len() + combo.len() + right_probe.len());
    before.extend(left_probe.iter().map(|p| (p.lanes.clone(), p.kind)));
    for &cd in combo {
        before.push((table.get(cd).feature_lanes().to_vec(), CharDefKind::Segment));
    }
    before.extend(right_probe.iter().map(|p| (p.lanes.clone(), p.kind)));

    let mut b = ShapeBuilder::with_features_capacity(w, before.len());
    for (lanes, kind) in &before {
        match kind {
            CharDefKind::Segment => b.push_segment_with_lanes(NO_CHAR_DEF, lanes),
            CharDefKind::Boundary => b.push_boundary_with_lanes(NO_CHAR_DEF, lanes),
        }
    }
    let shape = b.finish();
    let results = hc_rules::rewrite::synthesize(g, rule, &shape);
    let out_shape = results.first();

    let after: Vec<Vec<u64>> = match out_shape {
        Some(s) => s
            .interior()
            .filter(|(_, kind, _, _)| matches!(kind, NodeKind::Segment | NodeKind::Boundary))
            .map(|(i, ..)| s.node_lanes(i).to_vec())
            .collect(),
        None => before.iter().map(|(l, _)| l.clone()).collect(), // DefaultIfEmpty(word): unchanged
    };

    let delta = rhs_count as i64 - combo.len() as i64;
    if after.len() as i64 != before.len() as i64 + delta {
        return None; // did not fire as this subrule's declared shape
    }

    let target_start = left_probe.len();
    if delta == 0 {
        let changed = (0..combo.len()).any(|i| after[target_start + i] != before[target_start + i].0);
        if !changed {
            return None; // unaffected by the rule in this context -- no arc
        }
    }

    let mut arcs = Vec::new();
    let shared = combo.len().min(rhs_count);
    for i in 0..shared {
        arcs.push(ArcSpec {
            surface: Some(after[target_start + i].clone()),
            underlying: Some(before[target_start + i].0.clone()),
        });
    }
    for i in shared..combo.len() {
        // Deletion-inverse: this underlying segment left no surface trace -- ε-input restoration.
        arcs.push(ArcSpec { surface: None, underlying: Some(before[target_start + i].0.clone()) });
    }
    for i in shared..rhs_count {
        // Epenthesis-inverse: this surface segment was inserted by the rule -- consume it, emit
        // nothing downstream (ε-output).
        arcs.push(ArcSpec { surface: Some(after[target_start + i].clone()), underlying: None });
    }
    Some(arcs)
}

/// C# `EmitSubrule`: identity pass-through env fragments (via [`env_nfa::compile_env`]) bracketing
/// each probed candidate's arc chain.
#[allow(clippy::too_many_arguments)]
fn emit_subrule(
    g: &Grammar,
    table: &CharDefTable,
    pinv: &mut InversePhonology,
    next_state: &mut u32,
    spec: &SubruleSpec,
    entry: StateId,
    rejoin: StateId,
    reasons: &mut Vec<String>,
) {
    let left = env_nfa::compile_env(g, table, spec.left_env, pinv, next_state, entry);
    for r in &left.reasons {
        add_reason(reasons, r);
    }
    let left_end = left.end_state;

    let right_entry = alloc(next_state);
    let right = env_nfa::compile_env(g, table, spec.right_env, pinv, next_state, right_entry);
    pinv.add_epsilon(right.end_state, rejoin);
    for r in &right.reasons {
        add_reason(reasons, r);
    }

    for candidate in &spec.candidates {
        let mut state = left_end;
        for arc in candidate {
            let next = alloc(next_state);
            pinv.add_arc(state, arc.surface.clone(), arc.underlying.clone(), next);
            state = next;
        }
        pinv.add_epsilon(state, right_entry);
    }
}

/// The `== RuleInverseCompiler tier report ==` section body (F0/F7 golden format): summary line
/// (`Exact=N, Permissive=N, IdentitySkip=N`) then one `{name}\t{tier}\t{reasons}` line per rule,
/// SORTED ORDINAL (byte-wise) by name (plan §4.2/§6.1's golden-line convention) -- `reasons` is
/// comma-joined, `-` if empty.
pub fn format_tier_report(compiled: &[CompiledRuleInverse]) -> String {
    let exact = compiled.iter().filter(|c| c.tier == RuleInverseTier::Exact).count();
    let permissive = compiled.iter().filter(|c| c.tier == RuleInverseTier::Permissive).count();
    let identity_skip = compiled.iter().filter(|c| c.tier == RuleInverseTier::IdentitySkip).count();

    let mut lines: Vec<String> = compiled
        .iter()
        .map(|c| {
            let tier = match c.tier {
                RuleInverseTier::Exact => "Exact",
                RuleInverseTier::Permissive => "Permissive",
                RuleInverseTier::IdentitySkip => "IdentitySkip",
            };
            let reasons = if c.reasons.is_empty() { "-".to_string() } else { c.reasons.join(",") };
            format!("{}\t{}\t{}", c.name, tier, reasons)
        })
        .collect();
    lines.sort();

    let mut out = format!("Exact={exact}, Permissive={permissive}, IdentitySkip={identity_skip}\n");
    out.push_str(&lines.join("\n"));
    out
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

    /// The F7 primary gate: the tier-report SECTION (not the whole `fst-stats` dump, which is F8's
    /// job) byte-identical to the golden's own section, on all three reference grammars.
    #[test]
    fn indonesian_tier_report_matches_golden() {
        let Some(g) = load("indonesian-hc.xml") else {
            eprintln!("skipping: indonesian-hc.xml not present");
            return;
        };
        let compiled = compile_default(&g);
        let got = format_tier_report(&compiled);
        let expected = "Exact=2, Permissive=3, IdentitySkip=0\n\
Nasal assimilation\tPermissive\talpha-variable\n\
Nasal deletion\tExact\t-\n\
Nasalization in reduplication\tPermissive\talpha-variable\n\
Unspecified nasal default\tExact\t-\n\
Voiceless obstruent deletion\tPermissive\tmpr-or-syntactic-gate";
        assert_eq!(got, expected);
    }

    #[test]
    fn amharic_tier_report_matches_golden() {
        let Some(g) = load("amharic-hc.xml") else {
            eprintln!("skipping: amharic-hc.xml not present");
            return;
        };
        let compiled = compile_default(&g);
        let got = format_tier_report(&compiled);
        let expected = "Exact=2, Permissive=4, IdentitySkip=1\n\
Consonant-Vowel merger at morpheme boundaries\tIdentitySkip\talpha-variable,no-effect\n\
Consonant-Vowel merger inside\tPermissive\talpha-variable\n\
a deletion before a\tExact\t-\n\
e-creation merging \u{e4} and y in imperfective and converb stems\tPermissive\tmpr-or-syntactic-gate\n\
e-creation merging \u{e4} and y in perfective stems\tPermissive\tmpr-or-syntactic-gate\n\
o-creation merging \u{e4} and w in perfective stems\tPermissive\tmpr-or-syntactic-gate\n\
remove consonant length from lexical forms\tExact\t-";
        assert_eq!(got, expected);
    }

    #[test]
    fn sena_has_zero_phonological_rules() {
        let Some(g) = load("sena-hc.xml") else {
            eprintln!("skipping: sena-hc.xml not present");
            return;
        };
        let compiled = compile_default(&g);
        assert!(compiled.is_empty(), "Sena has zero phonological rules, confirmed no-op");
        assert_eq!(format_tier_report(&compiled), "Exact=0, Permissive=0, IdentitySkip=0\n");
    }
}
