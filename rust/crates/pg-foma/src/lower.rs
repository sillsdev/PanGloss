//! The shared pattern/environment → FST lowering seam: `lower_span` lowers one subrule's
//! `left_env · lhs_focus · right_env` triple into foma acceptors, and `spans_overlap` tests two
//! such spans for a non-empty intersection at the shared focus position — the real
//! automaton-intersection test behind `crate::capability::SimultaneousSubruleOverlapPredicate`,
//! replacing a conservative unconditional-`Refuse` fallback.
//!
//! This module owns the pattern-lowering vocabulary (`Slot`, `pattern_slots`,
//! `slots_from_nodes`, `resolve_alpha_tuples`, `render_slots`, `AlphaAssignment`,
//! `TupleReport`, `class_members`, `slots_contain_alpha`, `MAX_QUANTIFIER_BOUND`) that
//! `replace.rs`'s rewrite-rule/metathesis compilation also uses; `replace.rs` re-exports every one
//! at its own path so existing callers are unaffected by where the logic actually lives.
//!
//! `crate::replace::SegAlphabet` (the char-def <-> PUA-token codec) and
//! `crate::replace::owning_table`/`crate::replace::owning_table_for_metathesis` (rule ->
//! owning-stratum -> `CharDefTable` resolution) stay in `replace.rs`: the former is general
//! token-alphabet infrastructure several other modules depend on directly, not something
//! pattern/environment lowering owns; the latter is rule/stratum bookkeeping that `lower_span`'s
//! own callers already resolve before calling in, so this module never needs to call it itself.
//!
//! `UnsupportedPatternNode` is the typed disposition for a pattern node kind `lower_span`
//! cannot yet represent — always returned explicitly rather than silently omitting or weakening the
//! node. Quantifier metadata is partially covered, transparently: `pattern_slots` accepts both a
//! finitely bounded and a genuinely unbounded (`max: None`) alpha-free `PatternNode::Quantifier`
//! natively (`Slot::Repeat`), and since `lower_span` calls `pattern_slots` directly rather than
//! re-deriving pattern coverage, either shape anywhere in `left_env`/`focus`/`right_env` lowers for
//! free. An inverted-bound (`min > max`), over-budget-finite (past `MAX_QUANTIFIER_BOUND`), or
//! alpha-nested quantifier is not representable: `pattern_slots` returns `None` for it, and
//! `UnsupportedPatternNode::Quantifier` is the typed reason `diagnose_unsupported` reports.

use std::collections::HashSet;

use foma::constructions::{fsm_concat, fsm_intersect, fsm_union, fsm_universal};
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::structures::{fsm_empty_set, fsm_empty_string, fsm_isempty};
use foma::types::Fsm;

use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{
    AnchorSide, Grammar, NaturalClassKind, Pattern, PatternNode, TableId, VarId,
};

use crate::replace::SegAlphabet;

// Natural-class member resolution, exact, from the model's own NaturalClassKind — never re-derived through a matcher-oriented helper tuned for a different job.

/// One class's members, resolved from `NaturalClassKind` with a set of alpha-bound feature lanes excluded from the `Feature`-kind pin test, since an alpha-bound feature is resolved per tuple, not a fixed pin.
fn class_members(
    g: &Grammar,
    table: &CharDefTable,
    nat_class: pg_grammar::model::NatClassId,
    exclude_lanes: &HashSet<usize>,
) -> Vec<CharDefId> {
    match &g.natural_classes[nat_class.0 as usize].kind {
        // Explicit segment list: verbatim, exact, never re-derived via a feature reconstruction that could silently diverge from the authored list.
        NaturalClassKind::Segments(ids) => ids.clone(),
        NaturalClassKind::Feature(pairs) => table
            .iter()
            .filter(|(_, cd)| cd.kind() == CharDefKind::Segment)
            .filter(|(_, cd)| {
                pairs.iter().all(|(f, bits)| {
                    exclude_lanes.contains(&(f.0 as usize))
                        || (cd.feature_lanes()[f.0 as usize] & bits.0 != 0)
                })
            })
            .map(|(id, _)| id)
            .collect(),
    }
}

// Pattern -> slot list (one slot per PatternNode, in document order); `None` on any construct this prototype doesn't render.

/// Which additional pattern-node shapes a particular `pattern_slots`/`slots_from_nodes` CALLER
/// may accept, beyond the floor every caller has always shared (`Context`/`CharDef`/a well-formed
/// `Quantifier`). `pattern_slots` is a single shared lowering seam deliberately reused by THREE
/// independent consumers with DIFFERENT verification obligations (module top doc's own "reuse, not
/// re-derive" discipline: `lower_span` for `crate::capability::SimultaneousSubruleOverlapPredicate`
/// (an `hc.dll`-oracle-verified span-intersection test), `crate::replace::compile_rewrite_rule_
/// subset`/`crate::replace::compile_metathesis_rule` for the real rewrite-rule/metathesis compile) --
/// widening what ONE consumer accepts must never silently widen what an UNRELATED consumer accepts
/// too, since each consumer's own soundness argument is independently made and independently
/// verified. This enum makes that boundary an explicit, typed parameter rather than a single shared
/// default a later change could accidentally loosen for everyone at once.
///
/// This split exists because `PatternNode::Segments`/`PatternNode::Anchor` were once an
/// unconditional `None` for every caller ALIKE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternLowerScope {
    /// The floor: `Segments`/`Anchor` still refuse unconditionally — `lower_span`'s callers and `compile_metathesis_rule` stay on this tier permanently, since widening either's own admitted set is a separately closed question this scope doesn't get to reopen.
    Baseline,
    /// The widening for the rewrite-rule compile path: additionally accepts a same- or cross-table `Segments` (lowering to `Slot::Fixed`/`Slot::ForeignFixed`) and any `Anchor` (to `Slot::Anchor`); a disagree-polarity alpha var or malformed `Quantifier` is unaffected and stays unsupported — strictly additive, never a blanket accept-everything switch.
    RewriteRuleCompile,
}

/// One position in a rendered pattern.
///
/// `pub(crate)`: this is the canonical definition -- `crate::replace` re-exports it at its OLD path
/// (`pub(crate) use crate::lower::Slot;`) so `capability.rs`'s `crate::replace::Slot::Alpha`/
/// `crate::replace::Slot::Repeat` pattern matches and `replace.rs`'s own `slot_candidates`/
/// `reversed_slots`/`compile_rtl_branch_net` keep compiling unmodified.
///
/// `Clone`: `replace.rs`'s RTL reversal
/// construction needs a REVERSED copy of a subrule's own slot lists (`reversed_slots`, that
/// file) alongside the original document-order lists it builds the safety-net `LeftToRight`-style
/// branch from -- see that file's `compile_rtl_branch_net` doc.
#[derive(Debug, Clone)]
pub(crate) enum Slot {
    /// A single fixed char-def, from a `CharDef` node or a singleton-class `Context` with no alpha vars.
    Fixed(CharDefId),
    /// One literal segment from a `Segments` node explicitly segmented against a table other than the rewrite rule's owning table; render-time lowering resolves it via a cross-table feature constraint.
    ForeignFixed { table: TableId, cd: CharDefId },
    /// A natural class with no alpha binding at this occurrence: renders as a `[c1|c2|...]` union.
    Union(Vec<CharDefId>),
    /// A natural class occurrence bound to one or more alpha variables, resolved per-tuple by `resolve_alpha_tuples`; `occurrence` is this slot instance's own id (unique per occurrence, not per variable, since two occurrences of the same `VarId` can draw from different classes that must only agree on feature value).
    Alpha {
        vars: Vec<(VarId, pg_grammar::featsys::FlatIndex)>,
        occurrence: usize,
        base_members: Vec<CharDefId>,
    },
    /// An alpha-free repetition of `children`'s own rendered slots, finitely or genuinely unboundedly. Renders as foma's native repetition operator, linear in `min` and, for the unbounded case, independent of any repetition count.
    /// See `docs/research/pg-foma-lower-design-notes.md` for why unbounded is a native construction rather than a scope limit, and why no `Slot::Alpha` may appear inside `children`.
    Repeat {
        min: u32,
        max: Option<u32>,
        children: Vec<Slot>,
    },
    /// A word-boundary condition, accepted only under `PatternLowerScope::RewriteRuleCompile`; renders identically as foma's `.#.` atom regardless of `AnchorSide`, since the compiled meaning comes from which side of the rule's focus marker the text sits on.
    /// See `docs/research/pg-foma-lower-design-notes.md` for the RTL-reversal argument and why the unread `AnchorSide` field is kept anyway.
    #[allow(dead_code)]
    Anchor(AnchorSide),
}

/// `true` iff `slots`, at any nesting depth through a `Slot::Repeat`'s `children`, contains a `Slot::Alpha` occurrence — checked at every depth so a nested quantifier can never smuggle one past a shallow check.
fn slots_contain_alpha(slots: &[Slot]) -> bool {
    slots.iter().any(|s| match s {
        Slot::Alpha { .. } => true,
        Slot::Repeat { children, .. } => slots_contain_alpha(children),
        Slot::Fixed(_) | Slot::ForeignFixed { .. } | Slot::Union(_) | Slot::Anchor(_) => false,
    })
}

/// Preflight ceiling on a `Quantifier`'s own finite `max` bound, checked before any xre text is rendered; a finite `max` above this ceiling is honestly reported unsupported, never silently clamped (which would round an honest refusal toward false acceptance). Never applied to a genuinely unbounded quantifier — `max: None` is a different, always-native-size construction, not a bound above this ceiling.
const MAX_QUANTIFIER_BOUND: u32 = 512;

/// Walk `pattern`'s nodes into `Slot`s, numbering each `Alpha` occurrence sequentially from
/// `*next_occurrence` (shared across LHS/RHS/left-env/right-env for one subrule — see
/// `replace.rs`'s `compile_rewrite_rule`, or this module's own `lower_span`, which resets its own
/// FRESH counter per span). Returns `None` (uncovered) on a disagree-polarity `Context`; an
/// out-of-scope `Quantifier` (inverted/over-budget-finite/alpha-nested/empty-children — see
/// `Slot::Repeat`'s own doc; a genuinely UNBOUNDED quantifier is not, by itself, out of
/// scope); or, when `scope` is
/// `PatternLowerScope::Baseline`, any `Segments`/`Anchor` node at all (when `scope` is
/// `PatternLowerScope::RewriteRuleCompile`, both same-table and table-qualified cross-table
/// `Segments` plus any `Anchor` lower successfully -- see `PatternLowerScope`'s own doc).
///
/// `table`: every `Context` node's `NatClassId` is resolved against THIS table
/// (`class_members`), never an implicit grammar-wide default
/// ("table zero is never an
/// implicit default"). The caller is responsible for choosing the RIGHT table — see
/// `crate::replace::owning_table`'s own doc for how `replace.rs`'s `compile_rewrite_rule_subset`
/// picks it (the rule's own stratum's `StratumDef::table`), and `lower_span`'s own call sites for
/// how THIS module picks it (`alphabet.table()`, already the correct per-caller table by that
/// function's own contract). A `PatternNode::Segments`' OWN declared table is compared against THIS
/// SAME `table` by pointer identity (`std::ptr::eq`, both being borrowed from the same `g.char_tables`
/// vec this pattern's own grammar owns) -- cheap, exact, and needs no new `TableId`-threading
/// through this function's signature.
///
/// `pub(crate)`: canonical definition -- `replace.rs`
/// re-exports it at its OLD path so `capability.rs`'s structural probes and every existing
/// `crate::replace::pattern_slots`/`pg_foma::replace::pattern_slots` caller keep compiling
/// unmodified.
pub(crate) fn pattern_slots(
    g: &Grammar,
    table: &CharDefTable,
    pattern: &Pattern,
    next_occurrence: &mut usize,
    scope: PatternLowerScope,
) -> Option<Vec<Slot>> {
    slots_from_nodes(g, table, &pattern.nodes, next_occurrence, scope)
}

/// `pattern_slots`'s own per-node walk, factored over a bare node slice so a `Quantifier`'s own `children` recurse through the identical per-node semantics, with `next_occurrence`/`scope` both threaded through unchanged.
fn slots_from_nodes(
    g: &Grammar,
    table: &CharDefTable,
    nodes: &[PatternNode],
    next_occurrence: &mut usize,
    scope: PatternLowerScope,
) -> Option<Vec<Slot>> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            PatternNode::CharDef(id) => out.push(Slot::Fixed(*id)),
            PatternNode::Context(sc) => {
                if sc.vars.is_empty() {
                    let members = class_members(g, table, sc.nat_class, &HashSet::new());
                    out.push(Slot::Union(members));
                } else {
                    if sc.vars.iter().any(|v| !v.plus) {
                        // "disagree" polarity — documented gap, never seen in the reference grammars.
                        return None;
                    }
                    let excl: HashSet<usize> =
                        sc.vars.iter().map(|v| v.feature.0 as usize).collect();
                    let base = class_members(g, table, sc.nat_class, &excl);
                    let occurrence = *next_occurrence;
                    *next_occurrence += 1;
                    let vars = sc.vars.iter().map(|v| (v.var, v.feature)).collect();
                    out.push(Slot::Alpha {
                        vars,
                        occurrence,
                        base_members: base,
                    });
                }
            }
            PatternNode::Quantifier { min, max, children } => {
                // A genuinely unbounded quantifier is accepted here: it has its own native, finite-size foma construction, so refusing it would be a scope line, not a feasibility finding. The inverted-bound/MAX_QUANTIFIER_BOUND checks below apply only to a finite bound and are skipped for `None`.
                if let Some(max_v) = max {
                    // Inverted bound: no sound finite construction exists for it; honest-unsupported rather than silently swapping/clamping min/max.
                    if min > max_v {
                        return None;
                    }
                    // Checked before recursing into children or rendering any xre text — the cheapest possible predictor.
                    if *max_v > MAX_QUANTIFIER_BOUND {
                        return None;
                    }
                }
                let child_slots = slots_from_nodes(g, table, children, next_occurrence, scope)?;
                if child_slots.is_empty() {
                    // No renderable child at all — nothing to bound-repeat, so honest-unsupported rather than rendering a vacuous group.
                    return None;
                }
                if slots_contain_alpha(&child_slots) {
                    // Alpha-bound occurrence nested inside a quantifier group is out of scope, since resolve_alpha_tuples does not recurse into a Slot::Repeat's own children.
                    return None;
                }
                out.push(Slot::Repeat {
                    min: *min,
                    max: *max,
                    children: child_slots,
                });
            }
            PatternNode::Segments {
                table: seg_table_id,
                shape,
            } => {
                if scope != PatternLowerScope::RewriteRuleCompile {
                    return None;
                }
                // Preserve a foreign (TableId, CharDefId) through lowering rather than reinterpreting its dense id in the owning table; same-table Segments keep the existing Fixed path.
                let seg_table = &g.char_tables[seg_table_id.0 as usize];
                for (_, _kind, char_def, _flags) in shape.shape.interior() {
                    let cd = CharDefId(char_def);
                    if std::ptr::eq(seg_table, table) {
                        out.push(Slot::Fixed(cd));
                    } else {
                        out.push(Slot::ForeignFixed {
                            table: *seg_table_id,
                            cd,
                        });
                    }
                }
            }
            PatternNode::Anchor(side) => {
                if scope != PatternLowerScope::RewriteRuleCompile {
                    return None;
                }
                out.push(Slot::Anchor(*side));
            }
        }
    }
    Some(out)
}

// Alpha-tuple resolution: cartesian product per variable, filtered by joint agreement, generic over N variables / N slots-per-variable.

/// One assignment of every alpha slot OCCURRENCE (module doc on `Slot::Alpha` — keyed by
/// occurrence id, NOT by `VarId`: two occurrences of the same variable generally resolve to
/// two DIFFERENT concrete segments, e.g. prule4's nasal-output segment and its
/// following-obstruent segment, which merely need to AGREE on the variable's feature value, not
/// be the same segment) to a concrete `CharDefId`, surviving the joint agreement filter.
pub struct AlphaAssignment {
    pub values: std::collections::HashMap<usize, CharDefId>,
}

/// Report for one alpha-bearing subrule: the naive per-slot product size (what a per-variable-name
/// expander would enumerate before any filtering) vs. the number of tuples surviving the joint
/// agreement constraint.
#[derive(Debug, Clone, Copy)]
pub struct TupleReport {
    pub raw_product: usize,
    pub surviving: usize,
}

/// Locate every `Slot::Alpha` occurrence across `slot_lists` (one `Vec<Slot>` per pattern zone:
/// LHS, RHS, left-env, right-env — in that order, any of which may be empty), and enumerate the
/// surviving tuple-indexed cross product: the FULL product of every occurrence's OWN candidate
/// set (never a same-var intersection — see `AlphaAssignment`'s doc for why that shortcut is
/// wrong), filtered to combinations where every pair of occurrences sharing a `VarId` AGREES —
/// unify (bitwise-overlap, matching this codebase's own natural-class-membership idiom, not
/// strict equality, since an underspecified segment's lane can carry more than one live bit) — at
/// that variable's feature lane. This bounds the count of segment tuples satisfying
/// the joint constraint (Amharic's 20-var CV-merger: nc15=59 × nc16=6 ⇒ ≤354, never v^20),
/// implemented generically over N variables and N occurrences per variable. Returns
/// `(assignments, report)`; a rule with zero alpha slots returns one trivial
/// `AlphaAssignment { values: {} }` and a `raw_product`/`surviving` of 1 (nothing to expand).
///
/// `table`: every alpha occurrence's feature-lane agreement test (`lane_value`, below) resolves
/// against THIS table, never an implicit `g.char_tables[0]` default
/// (the second of two former hardcoded-table sites, alongside `pattern_slots`'s own former
/// `table_of` call). The
/// `members: Vec<CharDefId>` each `Slot::Alpha` already carries were themselves resolved against
/// this SAME table by `pattern_slots` (the caller's job: pass ONE consistent table to both), so
/// this function's own `table` parameter must be the identical table `pattern_slots` used to
/// build `slot_lists` in the first place — never a second, independently-chosen one.
///
/// `pub(crate)`: canonical definition -- `replace.rs`
/// re-exports it at its OLD path (`pub(crate) use crate::lower::resolve_alpha_tuples;`) so its own
/// `compile_rewrite_rule_subset` and every other existing caller keep compiling unmodified.
pub(crate) fn resolve_alpha_tuples(
    table: &CharDefTable,
    slot_lists: &[&[Slot]],
) -> (Vec<AlphaAssignment>, TupleReport) {
    // Flatten to (occurrence, vars, members) in document order, plus the var-group membership needed for the filter step; one occurrence may carry many (var, feature) pairs, all constraining the same concrete segment.
    struct Occ {
        id: usize,
        vars: Vec<(VarId, pg_grammar::featsys::FlatIndex)>,
        members: Vec<CharDefId>,
    }
    let mut occs: Vec<Occ> = Vec::new();
    for slots in slot_lists {
        for slot in slots.iter() {
            if let Slot::Alpha {
                vars,
                occurrence,
                base_members,
            } = slot
            {
                occs.push(Occ {
                    id: *occurrence,
                    vars: vars.clone(),
                    members: base_members.clone(),
                });
            }
        }
    }
    if occs.is_empty() {
        return (
            vec![AlphaAssignment {
                values: std::collections::HashMap::new(),
            }],
            TupleReport {
                raw_product: 1,
                surviving: 1,
            },
        );
    }
    occs.sort_by_key(|o| o.id);

    let raw_product: usize = occs.iter().map(|o| o.members.len().max(1)).product();

    // Cross product across all occurrences: each ranges independently over its own candidate set.
    let mut assignments: Vec<std::collections::HashMap<usize, CharDefId>> =
        vec![std::collections::HashMap::new()];
    for occ in &occs {
        let mut next = Vec::with_capacity(assignments.len() * occ.members.len().max(1));
        for asg in &assignments {
            for &cd in &occ.members {
                let mut a = asg.clone();
                a.insert(occ.id, cd);
                next.push(a);
            }
        }
        assignments = next;
    }

    // Joint-agreement filter: for every pair of occurrences sharing a VarId, the two chosen segments must unify (bitwise overlap) at that variable's feature lane.
    let mut var_pairs: std::collections::HashMap<
        VarId,
        Vec<(usize, pg_grammar::featsys::FlatIndex)>,
    > = std::collections::HashMap::new();
    for occ in &occs {
        for &(var, feature) in &occ.vars {
            var_pairs.entry(var).or_default().push((occ.id, feature));
        }
    }
    let lane_value = |cd: CharDefId, feature: pg_grammar::featsys::FlatIndex| -> u64 {
        table.get(cd).feature_lanes()[feature.0 as usize]
    };
    let survivors: Vec<AlphaAssignment> = assignments
        .into_iter()
        .filter(|asg| {
            var_pairs.values().all(|occs_for_var| {
                occs_for_var.iter().all(|&(id_a, feat)| {
                    occs_for_var.iter().all(|&(id_b, _)| {
                        let a = lane_value(asg[&id_a], feat);
                        let b = lane_value(asg[&id_b], feat);
                        a & b != 0
                    })
                })
            })
        })
        .map(|values| AlphaAssignment { values })
        .collect();

    let surviving = survivors.len();
    (
        survivors,
        TupleReport {
            raw_product,
            surviving,
        },
    )
}

// Slot -> regex text (given a concrete alpha assignment).

/// Renders an already-deduplicated token set as one atom: a bare char for a singleton, or foma's own `[a | b | ...]` union syntax for two-or-more, so an aliased `Slot::Fixed` atom and an aliased-and-unioned `Slot::Union` atom look identical to a caller that never observes aliasing.
fn format_union_tokens(chars: &[char]) -> String {
    if chars.len() == 1 {
        chars[0].to_string()
    } else {
        let inner: Vec<String> = chars.iter().map(|c| c.to_string()).collect();
        format!("[{}]", inner.join(" | "))
    }
}

/// Renders `slots` to xre source text, one space between consecutive slots (never omitted) and between union members inside one `[...]` group.
///
/// **Load-bearing finding:** this vendored foma-rs's xre lexer does not reliably treat two adjacent
/// non-ASCII (here: Private-Use-Area) codepoints written back-to-back with no separator as two
/// independent single-symbol atoms — confirmed by direct bisection: a PUA-token rule with
/// space-separated tokens correctly matches in context, while the byte-identical rule with tokens
/// concatenated with no space silently fails to match, with no parse error and no panic. ASCII
/// letters tolerate bare concatenation fine; the gap is specific to non-ASCII/high-codepoint
/// symbols, which is exactly what a char-def-identity token alphabet is built from. This is a hard
/// rule for any xre string this compiler emits.
pub(crate) fn render_slots(
    alphabet: &SegAlphabet,
    slots: &[Slot],
    assignment: &AlphaAssignment,
) -> String {
    let mut pieces: Vec<String> = Vec::with_capacity(slots.len());
    for slot in slots {
        let piece = match slot {
            // Slot::Fixed/Slot::Union: render-time cross-table alias expansion happens here, not in class_members. Slot::Alpha deliberately does not alias here, since its resolved segment already came from class_members' own single-table resolution.
            Slot::Fixed(cd) => format_union_tokens(&alphabet.render_tokens(*cd)),
            Slot::ForeignFixed { table, cd } => {
                format_union_tokens(&alphabet.render_foreign_constraint_tokens(*table, *cd))
            }
            Slot::Union(members) => {
                let mut chars: Vec<char> = Vec::with_capacity(members.len());
                for m in members {
                    for c in alphabet.render_tokens(*m) {
                        if !chars.contains(&c) {
                            chars.push(c);
                        }
                    }
                }
                format_union_tokens(&chars)
            }
            Slot::Alpha { occurrence, .. } => {
                let cd = assignment.values.get(occurrence).expect(
                    "every alpha slot's occurrence has a resolved assignment by render time",
                );
                alphabet.token(*cd).to_string()
            }
            Slot::Repeat { min, max, children } => {
                // Recurses into render_slots for children: same rendering, same PUA-token space, same space-separation rule; no second text-rendering path.
                let inner = render_slots(alphabet, children, assignment);
                match max {
                    // Foma's own native bounded-repetition xre operator, `^{min,max}`.
                    Some(max_v) => format!("[{inner}]^{{{min},{max_v}}}"),
                    // Load-bearing off-by-one: foma's `^>N` means "more than N", not "N or more", so rendering min "or more" requires `^>(min-1)`, never `^>min`.
                    // See `docs/research/pg-foma-lower-design-notes.md` for the construction this depends on and the test that pins it.
                    None if *min == 0 => format!("[{inner}]*"),
                    None => format!("[{inner}]^>{}", min - 1),
                }
            }
            // Foma's own `.#.` word-boundary xre atom, identical regardless of `AnchorSide`: the rendered position, not the tag, conveys word-initial vs word-final.
            Slot::Anchor(_) => ".#.".to_string(),
        };
        pieces.push(piece);
    }
    pieces.join(" ")
}

/// A pattern node kind `lower_span` cannot yet represent — a typed unsupported disposition that
/// does not omit or weaken the node. Named after the `model.rs` `PatternNode` variant (or, for the
/// one non-node case, the `pg_grammar::model::AlphaVar` shape) it names, so a caller's diagnostic
/// can cite the exact construct rather than a generic "pattern too complex" message, carried
/// through as a typed value instead of a silent `None`.
///
/// Under `PatternLowerScope::Baseline`, any `Segments` or `Anchor` node triggers `Segments`/`Anchor`
/// respectively; under `PatternLowerScope::RewriteRuleCompile`, both lower successfully instead
/// (`Segments` preserves table semantics same- or cross-table; `Anchor` always lowers to
/// `Slot::Anchor`), so those two variants become baseline-scope-only refusals. `Quantifier` covers
/// an inverted, over-budget-finite, alpha-nested, or empty-children quantifier — a finitely bounded
/// or genuinely unbounded, alpha-free quantifier never reaches this variant, since `pattern_slots`
/// accepts it directly as a `Slot::Repeat`. `AlphaDisagreePolarity` is not a distinct node kind but
/// the same "cannot lower faithfully" outcome for a disagree-polarity alpha variable, refused under
/// every `PatternLowerScope` tier — an orthogonal, pre-existing gap unrelated to direction/reversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedPatternNode {
    /// An inverted, over-budget-finite, alpha-nested, or empty-children `Quantifier`; pinned by `inverted_finite_quantifier_still_unsupported`.
    Quantifier,
    /// An inline pre-segmented literal `Segments` shape group, under `PatternLowerScope::Baseline` only.
    Segments,
    /// A word-boundary `Anchor` condition, under `PatternLowerScope::Baseline` only.
    Anchor,
    /// A disagree-polarity `AlphaVar` occurrence, refused under every scope.
    AlphaDisagreePolarity,
}

impl std::fmt::Display for UnsupportedPatternNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            UnsupportedPatternNode::Quantifier => "Quantifier (OptionalSegmentSequence)",
            UnsupportedPatternNode::Segments => "Segments (inline PhoneticShape group)",
            UnsupportedPatternNode::Anchor => "Anchor (word-boundary condition)",
            UnsupportedPatternNode::AlphaDisagreePolarity => {
                "Context with a disagree-polarity AlphaVariable"
            }
        };
        f.write_str(label)
    }
}

/// Scans `pattern` for the FIRST node `pattern_slots` (called with this SAME `g`/`table`/`scope`)
/// cannot lower, to recover a typed reason after `pattern_slots` has already returned `None` for it.
/// `pub(crate)`: exposed so
/// `capability.rs`'s `RightToLeftRewriteFaithfulReversalPredicate` can name the EXACT failing shape
/// in its own `Refuse` witness, rather than a laundry-list "could be any of these" message.
///
/// Recurses into a `Quantifier`'s own `children` (`diagnose_unsupported_nodes`) rather than
/// assuming the FIRST `Quantifier` node encountered is automatically the culprit: a well-formed
/// quantifier earlier in document order than the REAL failing node would otherwise be mis-blamed
/// (this function's own precision bar, matching `slots_from_nodes`'s actual accept/reject order
/// exactly — a diagnostic that mis-names its cause is worse than no diagnostic).
pub(crate) fn diagnose_unsupported(
    g: &Grammar,
    table: &CharDefTable,
    pattern: &Pattern,
    scope: PatternLowerScope,
) -> UnsupportedPatternNode {
    diagnose_unsupported_nodes(g, table, &pattern.nodes, scope).unwrap_or_else(|| {
        unreachable!(
            "pg_foma::lower::diagnose_unsupported called on a pattern pattern_slots did not \
             actually reject under scope {scope:?}: {pattern:?} (a caller bug, not a \
             grammar-authoring one)"
        )
    })
}

/// `true` iff `nodes`, at any nesting depth through a `Quantifier`'s `children`, contains a `Context` carrying an `AlphaVar` — the pre-lowering version of `slots_contain_alpha`, letting `diagnose_unsupported_nodes`'s `Quantifier` arm check without first building `Slot`s for a subtree it may reject for a different reason.
fn nodes_contain_alpha_context(nodes: &[PatternNode]) -> bool {
    nodes.iter().any(|n| match n {
        PatternNode::Context(sc) => !sc.vars.is_empty(),
        PatternNode::Quantifier { children, .. } => nodes_contain_alpha_context(children),
        PatternNode::CharDef(_) | PatternNode::Segments { .. } | PatternNode::Anchor(_) => false,
    })
}

/// `diagnose_unsupported`'s recursive walk, mirroring `slots_from_nodes`'s exact accept/reject decisions node-by-node so the reason it reports is always the real one; `None` means fully lowerable, which the caller treats as a caller-bug panic.
fn diagnose_unsupported_nodes(
    _g: &Grammar,
    _table: &CharDefTable,
    nodes: &[PatternNode],
    scope: PatternLowerScope,
) -> Option<UnsupportedPatternNode> {
    // `_g`/`_table` thread through only to match this recursion's caller signature; never inspected here.
    for node in nodes {
        match node {
            PatternNode::CharDef(_) => {}
            PatternNode::Context(sc) => {
                if sc.vars.iter().any(|v| !v.plus) {
                    return Some(UnsupportedPatternNode::AlphaDisagreePolarity);
                }
            }
            PatternNode::Quantifier { min, max, children } => {
                if let Some(max_v) = max {
                    if min > max_v || *max_v > MAX_QUANTIFIER_BOUND {
                        return Some(UnsupportedPatternNode::Quantifier);
                    }
                }
                if children.is_empty() {
                    return Some(UnsupportedPatternNode::Quantifier);
                }
                // A well-formed quantifier's own children might still hide the true failing node, so recurse rather than assume this quantifier is the culprit just because it is the first one seen.
                if let Some(reason) = diagnose_unsupported_nodes(_g, _table, children, scope) {
                    return Some(reason);
                }
                // Children lower cleanly, but an alpha-bound occurrence anywhere inside them still makes the outer Slot::Repeat unbuildable, so the true reason here is this quantifier.
                if nodes_contain_alpha_context(children) {
                    return Some(UnsupportedPatternNode::Quantifier);
                }
            }
            PatternNode::Segments { .. } => {
                if scope != PatternLowerScope::RewriteRuleCompile {
                    return Some(UnsupportedPatternNode::Segments);
                }
            }
            PatternNode::Anchor(_) => {
                if scope != PatternLowerScope::RewriteRuleCompile {
                    return Some(UnsupportedPatternNode::Anchor);
                }
            }
        }
    }
    None
}

/// Compiles `text` to an `Fsm` acceptor, treating an empty rendered string as the empty-string language rather than an invalid regex, since `render_slots` legitimately returns `""` for an absent/empty pattern.
fn parse_template(opts: &FomaOptions, text: &str) -> Fsm {
    if text.is_empty() {
        fsm_empty_string()
    } else {
        fsm_parse_regex(opts, text, None, None).unwrap_or_else(|| {
            panic!("pg_foma::lower: foma rejected a lowered span template regex {text:?}")
        })
    }
}

/// Lowers one subrule's `left_env · lhs_focus · right_env` triple (the `span(s)`
/// formula) into a pair of foma acceptors over `alphabet`'s token space, for `spans_overlap`'s
/// intersection test. `focus` is `RewriteRuleDef.lhs` — shared verbatim across every subrule of
/// one rule (`RewriteSubruleDef` only supplies its own `rhs`/`left_env`/`right_env`, model.rs
/// `RewriteSubruleDef` doc).
///
/// # Why a `(left_language, focus_right_language)` PAIR, not one combined `Fsm`
/// `span(s) = left_env · lhs_focus · right_env`, and the goal is to intersect two subrules'
/// spans. Read as a literal concatenation of the three patterns' own node sequences and compared
/// as ONE automaton, that is only sound when both subrules' `left_env`/`right_env` describe the
/// SAME fixed length: `left_env`/`right_env` are boundary-anchored templates (they constrain the
/// segments immediately adjacent to the shared focus, not "some point in the word"), so two
/// subrules whose environments describe DIFFERENT lengths (whether because they have different
/// node counts, or because one or both
/// contain a bounded `Quantifier` whose own `min..max` range makes even ONE subrule's own template
/// match more than one length) describe overlapping-but-different-length windows around the SAME
/// anchor point. A literal fixed-length concatenation, intersected whole, would (wrongly) report
/// them as non-overlapping merely because the two automata accept different string lengths — an
/// UNSOUND under-refusal (rounding toward `Admit` when a real overlap is missed is exactly
/// backwards from this crate's required direction: `Refuse` must round toward "never", not toward
/// "always"). The `Σ*`-padding fix below does not
/// depend on either side being fixed-length in the first place — a bounded quantifier's own
/// template is still a plain REGULAR language (a finite union of finite lengths, exactly what
/// `Slot::Repeat`'s `^{min,max}` compiles to), which `fsm_parse_regex` compiles the same as any
/// other template, and a genuinely unbounded quantifier's own native construction is unaffected by
/// this padding either.
///
/// The fix: represent `left_env` as the SUFFIX language `Σ* · left_env` (any prefix, ending in the
/// template) and fold `lhs_focus`/`right_env` into the PREFIX language `lhs_focus · right_env ·
/// Σ*` (starting with the shared focus then the template, any suffix) — each half anchored at the
/// boundary it actually describes, `Σ*` absorbing any length mismatch between the two subrules'
/// own templates. `spans_overlap` then intersects the two subrules' LEFT halves and FOCUS+RIGHT
/// halves SEPARATELY (not concatenated into one "contains the whole span somewhere in the word"
/// automaton) — see that function's own doc for why checking them separately is the CORRECT
/// decomposition of "at a shared focus position", not merely a convenient
/// approximation of one (a single combined `Σ* · L · F · R · Σ*` "contains" automaton would
/// actually be WRONG here: it would accept a witness word where subrule i's context holds at one
/// position and subrule j's holds at an unrelated OTHER position, which is not the same-position
/// overlap this is meant to catch).
///
/// # Alpha variables
/// `left_env`/`focus`/`right_env` are lowered with a FRESH, shared occurrence counter local to
/// this call — exactly mirroring how `replace.rs::compile_rewrite_rule_subset` resets
/// `next_occurrence` to `0` per subrule — and jointly resolved via the REUSED
/// `resolve_alpha_tuples`, so an `AlphaVariable` shared between (say) `left_env` and `focus` is
/// resolved with the SAME joint-agreement semantics real rewrite-rule compilation already uses,
/// not a re-derived one. The subrule's OWN `rhs` is deliberately NOT included in this joint
/// resolution (unlike `replace.rs`'s per-subrule fold, which joins LHS+RHS+left+right together):
/// whether this SPAN can match does not depend on the subrule's RHS at all, and omitting it can
/// only ever ADD spurious alpha tuples relative to the true RHS-constrained set (never remove real
/// ones, since the RHS's own occurrences could only additionally NARROW the joint-agreement
/// filter) — a strictly SAFE, over-permissive simplification that rounds toward more overlap being
/// detected (i.e. toward `Refuse` in `spans_overlap`), never an unsound one.
///
/// Each resolved tuple's rendered text is `fsm_parse_regex`-compiled (via `parse_template`)
/// and the per-tuple automata are `fsm_union`-folded per half (a subrule's span matches under ANY
/// of its own valid alpha assignments, not just one) — contrast
/// `crate::replace::compile_rewrite_rule_subset`'s per-tuple fold, which is a SEQUENTIAL
/// composition because there each tuple's compiled net is a full elsewhere-preserving REPLACE
/// transducer (that module's own doc: union would reintroduce a spurious "did nothing" path).
/// Here each tuple's compiled net is a plain ACCEPTOR with no "elsewhere" case, so union is exactly
/// the right combinator, not a divergence from that module's reasoning.
///
/// # Returns
/// `Err` names the FIRST unsupported node encountered (checked in `left_env`, `focus`, `right_env`
/// order) via `UnsupportedPatternNode` — the caller (`capability.rs`) rounds this to a
/// conservative `Refuse` naming the kind: any approximation here rounds toward `Refuse`, never
/// toward `Admit`.
pub fn lower_span(
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    left_env: Option<&Pattern>,
    focus: &Pattern,
    right_env: Option<&Pattern>,
) -> Result<(Fsm, Fsm), UnsupportedPatternNode> {
    let mut next_occurrence = 0usize;
    // pattern_slots/resolve_alpha_tuples take an explicit table, never an implicit g.char_tables[0]; alphabet.table() is already the correct one by this function's own caller contract.
    let table = alphabet.table();

    // lower_span is SimultaneousSubruleOverlapPredicate's own machinery and must stay on PatternLowerScope::Baseline permanently, unaffected by RewriteRuleCompile widening elsewhere in this module.
    let scope = PatternLowerScope::Baseline;
    let left_slots = match left_env {
        Some(p) => pattern_slots(g, table, p, &mut next_occurrence, scope)
            .ok_or_else(|| diagnose_unsupported(g, table, p, scope))?,
        None => Vec::new(),
    };
    let focus_slots = pattern_slots(g, table, focus, &mut next_occurrence, scope)
        .ok_or_else(|| diagnose_unsupported(g, table, focus, scope))?;
    let right_slots = match right_env {
        Some(p) => pattern_slots(g, table, p, &mut next_occurrence, scope)
            .ok_or_else(|| diagnose_unsupported(g, table, p, scope))?,
        None => Vec::new(),
    };

    let (assignments, _report) = resolve_alpha_tuples(
        table,
        &[
            left_slots.as_slice(),
            focus_slots.as_slice(),
            right_slots.as_slice(),
        ],
    );

    let mut left_lang: Option<Fsm> = None;
    let mut focus_right_lang: Option<Fsm> = None;
    for asg in &assignments {
        let left_text = render_slots(alphabet, &left_slots, asg);
        let focus_text = render_slots(alphabet, &focus_slots, asg);
        let right_text = render_slots(alphabet, &right_slots, asg);

        let left_tpl = parse_template(opts, &left_text);
        let focus_tpl = parse_template(opts, &focus_text);
        let right_tpl = parse_template(opts, &right_text);

        // Sigma* . left_template  (suffix language: any prefix, ending in the left template).
        let this_left = fsm_concat(opts, fsm_universal(), left_tpl);
        // focus_template . right_template . Sigma* (prefix language: starts with the shared focus then the right template, any suffix).
        let this_focus_right = fsm_concat(
            opts,
            fsm_concat(opts, focus_tpl, right_tpl),
            fsm_universal(),
        );

        left_lang = Some(match left_lang {
            None => this_left,
            Some(prev) => fsm_union(opts, prev, this_left),
        });
        focus_right_lang = Some(match focus_right_lang {
            None => this_focus_right,
            Some(prev) => fsm_union(opts, prev, this_focus_right),
        });
    }

    // `assignments` is empty only when no valid alpha tuple exists at all; the empty language is exactly correct for a subrule that can never match anything.
    Ok((
        left_lang.unwrap_or_else(fsm_empty_set),
        focus_right_lang.unwrap_or_else(fsm_empty_set),
    ))
}

/// The intersection test: `true` iff subrules `a` and `b`'s spans (each a
/// `(left_language, focus_right_language)` pair from `lower_span`) can hold AT THE SAME shared
/// focus position — i.e. genuinely overlap.
///
/// # Why two independent intersections, not one combined automaton
/// The real actual-word content immediately LEFT of the shared focus position is ONE concrete
/// (finite) string; it satisfies subrule `a`'s left environment iff it is a member of `a`'s
/// `left_language`, and independently satisfies `b`'s iff it is a member of `b`'s `left_language`
/// — both languages describe THE SAME region of the SAME word, so the question "can some real
/// left-context simultaneously satisfy both" is exactly `intersect(left_a, left_b)` non-empty, no
/// further alignment machinery needed (the `Σ*` prefix in each already anchors the comparison at
/// the shared right edge — see `lower_span`'s own doc). The symmetric argument holds for the
/// content AT/RIGHT of the position via `focus_right_language`. Because the left region and the
/// focus+right region of a word are DISJOINT and freely composable (any accepted left-string
/// concatenated with any accepted focus+right-string is a valid witness word — nothing else
/// constrains them jointly once each subrule's OWN internal alpha agreement has already been
/// resolved inside `lower_span`), `a` and `b` can co-fire at the same position iff BOTH
/// intersections are non-empty; checking them as one combined `Σ* · L · F · R · Σ*` "contains
/// somewhere" automaton instead would be WRONG (see `lower_span`'s own doc for the false-overlap
/// case that construction admits).
///
/// Any imprecision `lower_span`'s per-subrule marginalization introduces (projecting each of a
/// subrule's OWN internally-consistent alpha tuples down to a left-only / focus+right-only piece
/// before unioning across tuples) can only ever make a language LARGER than the true "matches
/// under some single self-consistent assignment" set — i.e. can only report MORE overlap than
/// truly exists, never less — which rounds toward `Refuse`, the safe direction.
pub fn spans_overlap(opts: &FomaOptions, a: &(Fsm, Fsm), b: &(Fsm, Fsm)) -> bool {
    let (left_a, focus_right_a) = a;
    let (left_b, focus_right_b) = b;

    let mut left_intersection = fsm_intersect(opts, left_a.clone(), left_b.clone());
    if fsm_isempty(opts, &mut left_intersection) {
        return false;
    }
    let mut focus_right_intersection =
        fsm_intersect(opts, focus_right_a.clone(), focus_right_b.clone());
    !fsm_isempty(opts, &mut focus_right_intersection)
}

#[cfg(test)]
mod tests {
    //! Synthetic, delanguaged fixtures only (no natural-language names), mirroring
    //! `capability.rs`'s own test-module convention.

    use pg_grammar::model::{PhonRuleDef, RewriteMode};

    use super::*;

    fn load(xml: &str) -> pg_grammar::model::Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
    }

    const OVERLAP_LOWER_PROBE_XML: &str = r#"<HermitCrabInput><Language><Name>OverlapLowerProbe</Name>
      <PhonologicalFeatureSystem>
        <SymbolicFeature id="featPlace"><Name>place</Name>
          <Symbols>
            <Symbol id="symNeutral">neutral</Symbol>
            <Symbol id="symFront">front</Symbol>
            <Symbol id="symBack">back</Symbol>
          </Symbols>
        </SymbolicFeature>
      </PhonologicalFeatureSystem>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="cStop"><Representations><Representation>p</Representation></Representations>
            <FeatureValue feature="featPlace" symbolValues="symNeutral" />
          </SegmentDefinition>
          <SegmentDefinition id="cFront"><Representations><Representation>i</Representation></Representations>
            <FeatureValue feature="featPlace" symbolValues="symFront" />
          </SegmentDefinition>
          <SegmentDefinition id="cBack"><Representations><Representation>u</Representation></Representations>
            <FeatureValue feature="featPlace" symbolValues="symBack" />
          </SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses>
        <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cStop" /></SegmentNaturalClass>
        <FeatureNaturalClass id="ncFront"><Name>Front</Name>
          <FeatureValue feature="featPlace" symbolValues="symFront" />
        </FeatureNaturalClass>
        <FeatureNaturalClass id="ncBack"><Name>Back</Name>
          <FeatureValue feature="featPlace" symbolValues="symBack" />
        </FeatureNaturalClass>
      </NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prNoOverlap" multipleApplicationOrder="simultaneous"><Name>noOverlap</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncFront" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
              <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prOverlap" multipleApplicationOrder="simultaneous"><Name>overlap</Name>
          <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
    </Language></HermitCrabInput>"#;

    fn rewrite_rule<'g>(
        g: &'g pg_grammar::model::Grammar,
        xml_id: &str,
    ) -> &'g pg_grammar::model::RewriteRuleDef {
        for pr in &g.prules {
            if let PhonRuleDef::Rewrite(r) = pr {
                if r.xml_id == xml_id {
                    return r;
                }
            }
        }
        panic!("rewrite rule {xml_id:?} not found");
    }

    /// Two subrules whose right environments are mutually exclusive natural classes must lower to spans whose focus+right intersection is empty — they cannot both hold at the same position.
    #[test]
    fn lower_span_disjoint_right_environments_do_not_overlap() {
        let g = load(OVERLAP_LOWER_PROBE_XML);
        let r = rewrite_rule(&g, "prNoOverlap");
        assert_eq!(r.mode, RewriteMode::Simultaneous);
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();

        let span_a = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[0].left_env.as_ref(),
            &r.lhs,
            r.subrules[0].right_env.as_ref(),
        )
        .expect("prNoOverlap subrule 0 must lower (no unsupported nodes)");
        let span_b = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[1].left_env.as_ref(),
            &r.lhs,
            r.subrules[1].right_env.as_ref(),
        )
        .expect("prNoOverlap subrule 1 must lower (no unsupported nodes)");

        assert!(
            !spans_overlap(&opts, &span_a, &span_b),
            "Front/Back-flanked subrules must NOT overlap"
        );
    }

    /// Two subrules with identical (unconstrained) focus/environment lower to the same span, so their intersection is trivially non-empty.
    #[test]
    fn lower_span_identical_unconstrained_subrules_overlap() {
        let g = load(OVERLAP_LOWER_PROBE_XML);
        let r = rewrite_rule(&g, "prOverlap");
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();

        let span_a = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[0].left_env.as_ref(),
            &r.lhs,
            r.subrules[0].right_env.as_ref(),
        )
        .expect("prOverlap subrule 0 must lower");
        let span_b = lower_span(
            &opts,
            &g,
            &alphabet,
            r.subrules[1].left_env.as_ref(),
            &r.lhs,
            r.subrules[1].right_env.as_ref(),
        )
        .expect("prOverlap subrule 1 must lower");

        assert!(
            spans_overlap(&opts, &span_a, &span_b),
            "two unconstrained same-focus subrules must overlap"
        );
    }

    // A genuinely unbounded (max: None) quantifier compiles via foma's native E*/E^>N operator; MAX_QUANTIFIER_BOUND and the inverted-bound check apply only to a finite max.

    use foma::apply::{apply_init, apply_up};

    /// One `CharacterDefinitionTable` and five `PhonologicalRule`s, each a bare Segment-focused LHS with one quantifier-bearing probe fed straight to `pattern_slots`, never compiled/composed.
    const QUANTIFIER_SCOPE_PROBE_XML: &str = r#"<HermitCrabInput><Language><Name>QuantifierScopeProbe</Name>
      <PhonologicalFeatureSystem>
        <SymbolicFeature id="featA"><Name>a</Name>
          <Symbols><Symbol id="symX">x</Symbol><Symbol id="symY">y</Symbol></Symbols>
        </SymbolicFeature>
      </PhonologicalFeatureSystem>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="c1"><Representations><Representation>a</Representation></Representations>
            <FeatureValue feature="featA" symbolValues="symX" />
          </SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses>
        <SegmentNaturalClass id="ncC1"><Name>C1</Name><Segment segment="c1" /></SegmentNaturalClass>
      </NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prUnboundedMinZero"><Name>demo0</Name>
          <PhoneticInput><PhoneticSequence>
            <OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prUnboundedLargeMin"><Name>demo1</Name>
          <PhoneticInput><PhoneticSequence>
            <OptionalSegmentSequence min="1000" max="-1"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prInvertedFinite"><Name>demo2</Name>
          <PhoneticInput><PhoneticSequence>
            <OptionalSegmentSequence min="5" max="2"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prOverBudgetFinite"><Name>demo3</Name>
          <PhoneticInput><PhoneticSequence>
            <OptionalSegmentSequence min="1" max="600"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="prAlphaNestedUnbounded"><Name>demo4</Name>
          <VariableFeatures><VariableFeature id="var1" name="a" phonologicalFeature="featA" /></VariableFeatures>
          <PhoneticInput><PhoneticSequence>
            <OptionalSegmentSequence min="1" max="-1">
              <SimpleContext naturalClass="ncC1"><AlphaVariables><AlphaVariable variableFeature="var1" /></AlphaVariables></SimpleContext>
            </OptionalSegmentSequence>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
    </Language></HermitCrabInput>"#;

    fn quantifier_probe_rule<'g>(
        g: &'g pg_grammar::model::Grammar,
        xml_id: &str,
    ) -> &'g pg_grammar::model::RewriteRuleDef {
        for pr in &g.prules {
            if let PhonRuleDef::Rewrite(r) = pr {
                if r.xml_id == xml_id {
                    return r;
                }
            }
        }
        panic!("rule {xml_id:?} not found");
    }

    /// Positive witness: a genuinely unbounded (`max="-1"`), `min="0"` quantifier lowers to `Some(Slot::Repeat { min: 0, max: None, .. })`.
    #[test]
    fn unbounded_quantifier_min_zero_is_accepted_not_refused() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let rule = quantifier_probe_rule(&g, "prUnboundedMinZero");
        let mut next_occurrence = 0usize;
        let slots = pattern_slots(
            &g,
            table,
            &rule.lhs,
            &mut next_occurrence,
            PatternLowerScope::Baseline,
        )
        .expect("a well-formed unbounded (min=0), alpha-free quantifier must now lower");
        assert_eq!(slots.len(), 1);
        match &slots[0] {
            Slot::Repeat { min, max, .. } => {
                assert_eq!(*min, 0);
                assert_eq!(*max, None);
            }
            _ => panic!("expected a Slot::Repeat"),
        }
    }

    /// Positive witness: `MAX_QUANTIFIER_BOUND` is never checked against an unbounded quantifier's `min` — a `min` well past the bound must still lower to `Some(_)`.
    #[test]
    fn unbounded_quantifier_large_min_is_never_checked_against_max_quantifier_bound() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let rule = quantifier_probe_rule(&g, "prUnboundedLargeMin");
        let mut next_occurrence = 0usize;
        let slots = pattern_slots(&g, table, &rule.lhs, &mut next_occurrence, PatternLowerScope::Baseline).expect(
            "min=1000 (> MAX_QUANTIFIER_BOUND=512) must NOT be refused for an unbounded (max=None) \
             quantifier -- that ceiling only bounds a FINITE max, never `None`",
        );
        match &slots[0] {
            Slot::Repeat { min, max, .. } => {
                assert_eq!(*min, 1000);
                assert_eq!(*max, None);
            }
            _ => panic!("expected a Slot::Repeat"),
        }
    }

    /// Negative witness: an inverted finite bound (`min=5 > max=2`) has no sound finite construction and must stay refused.
    #[test]
    fn inverted_finite_quantifier_still_unsupported() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let rule = quantifier_probe_rule(&g, "prInvertedFinite");
        let mut next_occurrence = 0usize;
        assert!(
            pattern_slots(
                &g,
                table,
                &rule.lhs,
                &mut next_occurrence,
                PatternLowerScope::Baseline
            )
            .is_none(),
            "min=5 > max=2 (both concrete) must stay refused"
        );
    }

    /// Negative witness: a finite `max` past `MAX_QUANTIFIER_BOUND` must stay refused, never silently clamped down to the ceiling.
    #[test]
    fn over_budget_finite_quantifier_still_unsupported() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let rule = quantifier_probe_rule(&g, "prOverBudgetFinite");
        let mut next_occurrence = 0usize;
        assert!(
            pattern_slots(
                &g,
                table,
                &rule.lhs,
                &mut next_occurrence,
                PatternLowerScope::Baseline
            )
            .is_none(),
            "max=600 exceeds MAX_QUANTIFIER_BOUND=512 -- must stay refused, never silently clamped"
        );
    }

    /// Negative witness: an `AlphaVariable` occurrence inside a quantifier's own children is out of scope regardless of whether the quantifier itself is bounded or unbounded.
    #[test]
    fn alpha_nested_unbounded_quantifier_still_unsupported() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let rule = quantifier_probe_rule(&g, "prAlphaNestedUnbounded");
        let mut next_occurrence = 0usize;
        assert!(
            pattern_slots(
                &g,
                table,
                &rule.lhs,
                &mut next_occurrence,
                PatternLowerScope::Baseline
            )
            .is_none(),
            "an AlphaVariable occurrence inside a quantifier's own children is out of scope \
             regardless of whether the quantifier itself is bounded or unbounded"
        );
    }

    /// Load-bearing off-by-one, pinned at the compiled FST level, not just the rendered text: `min=2` must render `^>1` and accept exactly 2 (and 3+) occurrences while rejecting 1.
    /// See `docs/research/pg-foma-lower-design-notes.md` for why `^>min` (rather than `^>(min-1)`) would be wrong.
    #[test]
    fn render_slots_unbounded_min_off_by_one_boundary() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let (cd, _) = table
            .iter()
            .next()
            .expect("QUANTIFIER_SCOPE_PROBE_XML's table must have exactly 1 segment");
        let tok = alphabet.token(cd).to_string();

        let slots = vec![Slot::Repeat {
            min: 2,
            max: None,
            children: vec![Slot::Fixed(cd)],
        }];
        let asg = AlphaAssignment {
            values: std::collections::HashMap::new(),
        };
        let text = render_slots(&alphabet, &slots, &asg);
        assert_eq!(
            text,
            format!("[{tok}]^>1"),
            "min=2 (\"2 or more\") must render as ^>1 (min-1), never ^>2 (min)"
        );

        let net = fsm_parse_regex(&opts, &text, None, None)
            .expect("rendered unbounded-quantifier text must compile");
        let one = tok.clone();
        let two = format!("{tok}{tok}");
        let three = format!("{tok}{tok}{tok}");

        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&one)),
            None,
            "1 occurrence (below min=2) must NOT match"
        );
        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&two)),
            Some(two.clone()),
            "exactly min=2 occurrences must match -- the off-by-one this test pins"
        );
        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&three)),
            Some(three.clone()),
            "MORE than min (3) must ALSO match -- genuinely unbounded, not a min..min+1 accident"
        );
    }

    /// `min == 0` renders as plain `*` (foma's native Kleene star), a distinct code path from the `min >= 1` `^>` case above.
    #[test]
    fn render_slots_unbounded_min_zero_is_kleene_star() {
        let g = load(QUANTIFIER_SCOPE_PROBE_XML);
        let table = &g.char_tables[0];
        let alphabet = SegAlphabet::new(table);
        let opts = FomaOptions::default();
        let (cd, _) = table
            .iter()
            .next()
            .expect("QUANTIFIER_SCOPE_PROBE_XML's table must have exactly 1 segment");
        let tok = alphabet.token(cd).to_string();

        let slots = vec![Slot::Repeat {
            min: 0,
            max: None,
            children: vec![Slot::Fixed(cd)],
        }];
        let asg = AlphaAssignment {
            values: std::collections::HashMap::new(),
        };
        let text = render_slots(&alphabet, &slots, &asg);
        assert_eq!(
            text,
            format!("[{tok}]*"),
            "min=0 or more must render as a plain Kleene star"
        );

        let net = fsm_parse_regex(&opts, &text, None, None)
            .expect("rendered unbounded-quantifier text must compile");
        let zero = String::new();
        let one = tok.clone();
        let two = format!("{tok}{tok}");

        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&zero)),
            Some(zero.clone()),
            "0 occurrences must match"
        );
        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&one)),
            Some(one.clone()),
            "1 occurrence must match"
        );
        let mut h = apply_init(&net);
        assert_eq!(
            apply_up(&mut h, Some(&two)),
            Some(two.clone()),
            "2 occurrences must match"
        );
    }

    /// Cross-table representation aliasing happens in render_slots's Fixed/Union arms, not class_members: an alphabet built with a table identity must render a shared atom as a bracketed union of both tables' tokens, while one without renders the same slot bare.
    #[test]
    fn render_slots_aliases_fixed_and_union_atoms_across_tables() {
        const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>RenderSlotsAliasProbe</Name>
  <CharacterDefinitionTable id="t0"><Name>TableA</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="c0x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <CharacterDefinitionTable id="t1"><Name>TableB</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="c1z"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="c1x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="c1y"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
</Language></HermitCrabInput>"#;
        let g = load(XML);
        let table_a = &g.char_tables[0];
        let table_b = &g.char_tables[1];
        let cd_a_x = table_a.lookup_nfd("x").unwrap();
        let cd_b_x = table_b.lookup_nfd("x").unwrap();
        let cd_b_y = table_b.lookup_nfd("y").unwrap();
        assert_ne!(
            cd_a_x.0, cd_b_x.0,
            "the fixture's own misalignment must hold"
        );

        let alias_map = crate::replace::RepresentationAliasMap::build(&g);
        let aliased =
            SegAlphabet::with_table_id(table_b, pg_grammar::model::TableId(1), &alias_map);
        let bare = SegAlphabet::new(table_b);
        let asg = AlphaAssignment {
            values: std::collections::HashMap::new(),
        };

        // Slot::Fixed: the shared "x" atom aliases under `aliased`, stays bare under `bare`.
        let fixed_slots = vec![Slot::Fixed(cd_b_x)];
        let aliased_text = render_slots(&aliased, &fixed_slots, &asg);
        let bare_text = render_slots(&bare, &fixed_slots, &asg);
        assert_eq!(
            bare_text,
            bare.token(cd_b_x).to_string(),
            "unaliased rendering must stay exactly the single bare token"
        );
        assert_ne!(
            aliased_text, bare_text,
            "aliased rendering of a shared atom must differ from the unaliased rendering"
        );
        assert!(
            aliased_text.contains(&bare.token(cd_b_x).to_string())
                && aliased_text.contains(&SegAlphabet::new(table_a).token(cd_a_x).to_string()),
            "aliased rendering must contain BOTH tables' own tokens for the shared spelling: \
             {aliased_text:?}"
        );

        // Slot::Union: an unshared atom degenerates to the same bare rendering whether aliased or not, since aliasing only ever adds, never touches a spelling unique to its table.
        let union_slots = vec![Slot::Union(vec![cd_b_y])];
        assert_eq!(
            render_slots(&aliased, &union_slots, &asg),
            render_slots(&bare, &union_slots, &asg),
            "an unshared atom's rendering must be unaffected by aliasing"
        );
    }
}
