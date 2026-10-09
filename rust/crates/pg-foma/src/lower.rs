//! Owns pattern slots, alpha assignments, and their rendering for rewrite compilation and
//! span intersection. Caller scopes separate exact overlap checks from confirmed proposals.
//! Repeated environment variables widen to independent class membership; confirmation restores
//! agreement, and optional rewriting preserves analyses where the widened environment overmatches.

use std::collections::HashSet;

use foma::constructions::{fsm_concat, fsm_intersect, fsm_union, fsm_universal};
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::structures::{fsm_empty_set, fsm_empty_string, fsm_isempty};
use foma::types::Fsm;

use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{Grammar, NaturalClassKind, Pattern, PatternNode, TableId, VarId};

use crate::replace::SegAlphabet;

/// Resolves exact class membership while leaving alpha-bound feature lanes for tuple binding.
fn class_members(
    g: &Grammar,
    table: &CharDefTable,
    nat_class: pg_grammar::model::NatClassId,
    exclude_lanes: &HashSet<usize>,
) -> Vec<CharDefId> {
    let class = &g.natural_classes[nat_class.0 as usize];
    let pairs = match &class.kind {
        NaturalClassKind::Feature(pairs) => pairs
            .iter()
            .copied()
            .filter(|(feature, _)| !exclude_lanes.contains(&(feature.0 as usize)))
            .collect(),
        NaturalClassKind::Segments(_) => Vec::new(),
    };
    let members = pg_grammar::segment::nat_class_cd_set_with_constraints(table, class, &pairs);
    table
        .iter()
        .filter(|(id, cd)| match &members {
            pg_shape::CdSet::Unrestricted => cd.kind() == CharDefKind::Segment,
            pg_shape::CdSet::Members(bits) => bits.contains(id.0),
        })
        .map(|(id, _)| id)
        .collect()
}

// Pattern -> slot list (one slot per PatternNode, in document order); `None` on any construct this prototype doesn't render.

/// Caller scope keeps exact span intersection separate from confirmed rewrite proposals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternLowerScope {
    /// Exact span intersection excludes literal segments and anchors.
    Baseline,
    /// Rewrite targets preserve literal table identity and boundaries.
    RewriteRuleCompile,
    /// Repeated environment variables widen independently and require confirmation.
    RewriteEnvironment,
}

/// A rendered position; occurrence identity keeps separate classes bound by feature value.
#[derive(Debug, Clone)]
pub(crate) enum Slot {
    /// A single fixed char-def, from a `CharDef` node or a singleton-class `Context` with no alpha vars.
    Fixed(CharDefId),
    /// One literal segment from a `Segments` node explicitly segmented against a table other than the rewrite rule's owning table; render-time lowering resolves it via a cross-table feature constraint.
    ForeignFixed { table: TableId, cd: CharDefId },
    /// A natural class with no alpha binding at this occurrence: renders as a `[c1|c2|...]` union.
    Union(Vec<CharDefId>),
    /// Alpha membership, with agreement deferred to confirmation.
    DeferredAlpha {
        members: Vec<CharDefId>,
        governed_features: Vec<pg_grammar::featsys::FlatIndex>,
        explicit_segment_class: bool,
        ambiguous_disagree: bool,
    },
    /// A natural class occurrence bound to one or more alpha variables, resolved per-tuple by `resolve_alpha_tuples`; `occurrence` is this slot instance's own id (unique per occurrence, not per variable, since two occurrences of the same `VarId` can draw from different classes that must only agree on feature value). The `bool` is `AlphaVar::plus` (`true` == agree/`+`, `false` == disagree/`-`).
    Alpha {
        vars: Vec<(VarId, pg_grammar::featsys::FlatIndex, bool)>,
        occurrence: usize,
        base_members: Vec<CharDefId>,
        explicit_segment_class: bool,
        ambiguous_disagree: bool,
    },
    /// Repetition renders natively; environment agreement is deferred via `DeferredAlpha`.
    Repeat {
        min: u32,
        max: Option<u32>,
        children: Vec<Slot>,
    },
    /// A word-boundary condition, accepted only under `PatternLowerScope::RewriteRuleCompile`; renders as foma's `.#.` atom, with its meaning determined by its position relative to the rule's focus marker.
    Anchor,
}

/// Exact consumers must preserve alpha constraints at every repetition depth.
fn slots_contain_alpha(slots: &[Slot]) -> bool {
    slots.iter().any(|s| match s {
        Slot::Alpha { .. } | Slot::DeferredAlpha { .. } => true,
        Slot::Repeat { children, .. } => slots_contain_alpha(children),
        Slot::Fixed(_) | Slot::ForeignFixed { .. } | Slot::Union(_) | Slot::Anchor => false,
    })
}

pub(crate) fn slots_have_ambiguous_disagree(slots: &[Slot]) -> bool {
    slots.iter().any(|slot| match slot {
        Slot::Alpha { ambiguous_disagree, .. }
        | Slot::DeferredAlpha { ambiguous_disagree, .. } => *ambiguous_disagree,
        Slot::Repeat { children, .. } => slots_have_ambiguous_disagree(children),
        _ => false,
    })
}

pub(crate) fn slots_require_optional_rewrite(slots: &[Slot]) -> bool {
    slots.iter().any(|slot| match slot {
        Slot::DeferredAlpha { .. } => true,
        Slot::Alpha { ambiguous_disagree, .. } => *ambiguous_disagree,
        Slot::Repeat { children, .. } => slots_require_optional_rewrite(children),
        _ => false,
    })
}

fn defer_alpha_agreement(slots: &mut [Slot]) {
    for slot in slots {
        match slot {
            Slot::Alpha {
                vars,
                base_members,
                explicit_segment_class,
                ambiguous_disagree,
                ..
            } => {
                *slot = Slot::DeferredAlpha {
                    members: base_members.clone(),
                    governed_features: vars
                        .iter()
                        .map(|(_, feature, _)| *feature)
                        .collect(),
                    explicit_segment_class: *explicit_segment_class,
                    ambiguous_disagree: *ambiguous_disagree,
                };
            }
            Slot::Repeat { children, .. } => defer_alpha_agreement(children),
            _ => {}
        }
    }
}

/// Projects an authored class's lane unions without imposing correlations between member lanes.
fn project_explicit_members(
    table: &CharDefTable,
    authored_members: &[CharDefId],
    governed_features: &[pg_grammar::featsys::FlatIndex],
) -> Vec<CharDefId> {
    if authored_members.is_empty() {
        return Vec::new();
    }
    let governed: HashSet<usize> = governed_features
        .iter()
        .map(|feature| feature.0 as usize)
        .collect();
    let authored_lanes: Vec<_> = authored_members
        .iter()
        .map(|member| table.get(*member).feature_lanes())
        .collect();
    let feature_count = authored_lanes[0].len();
    let lane_unions: Vec<Option<u64>> = (0..feature_count)
        .map(|feature| {
            if governed.contains(&feature)
                || authored_lanes.iter().any(|lanes| lanes[feature] == 0)
            {
                None
            } else {
                Some(authored_lanes.iter().fold(0, |union, lanes| union | lanes[feature]))
            }
        })
        .collect();
    table
        .iter()
        .filter(|(_, candidate)| candidate.kind() == CharDefKind::Segment)
        .filter(|(_, candidate)| {
            candidate
                .feature_lanes()
                .iter()
                .zip(&lane_unions)
                .all(|(candidate_value, authored_union)| {
                    authored_union.is_none_or(|union| {
                        *candidate_value == 0 || *candidate_value & union != 0
                    })
                })
        })
        .map(|(id, _)| id)
        .collect()
}

/// Defer agreement to HC, using a projected explicit-class set or a table-wide proposal set.
pub(crate) fn defer_ambiguous_alpha_agreement(slots: &mut [Slot], table: &CharDefTable) {
    for slot in slots {
        match slot {
            Slot::Alpha {
                vars,
                base_members,
                explicit_segment_class,
                ..
            } => {
                let governed_features: Vec<_> = vars
                    .iter()
                    .map(|(_, feature, _)| *feature)
                    .collect();
                let members = if *explicit_segment_class {
                    project_explicit_members(table, base_members, &governed_features)
                } else {
                    table
                        .iter()
                        .filter(|(_, definition)| definition.kind() == CharDefKind::Segment)
                        .map(|(id, _)| id)
                        .collect()
                };
                *slot = Slot::DeferredAlpha {
                    members,
                    governed_features,
                    explicit_segment_class: *explicit_segment_class,
                    ambiguous_disagree: true,
                };
            }
            Slot::DeferredAlpha {
                members,
                governed_features,
                explicit_segment_class,
                ambiguous_disagree,
            } => {
                if *explicit_segment_class {
                    *members = project_explicit_members(table, members, governed_features);
                } else {
                    *members = table
                        .iter()
                        .filter(|(_, definition)| definition.kind() == CharDefKind::Segment)
                        .map(|(id, _)| id)
                        .collect();
                }
                *ambiguous_disagree = true;
            }
            Slot::Repeat { children, .. } => defer_ambiguous_alpha_agreement(children, table),
            _ => {}
        }
    }
}

/// Uses the caller-selected table and scope, with shared occurrence numbering per subrule.
pub(crate) fn pattern_slots(
    g: &Grammar,
    table: &CharDefTable,
    pattern: &Pattern,
    next_occurrence: &mut usize,
    scope: PatternLowerScope,
) -> Option<Vec<Slot>> {
    slots_from_nodes(g, table, &pattern.nodes, next_occurrence, scope)
}

/// Exact tuple expansion requires each governed value to identify one class member.
fn class_feature_partition_is_unambiguous(
    g: &Grammar,
    table: &CharDefTable,
    nat_class: pg_grammar::model::NatClassId,
    feature: pg_grammar::featsys::FlatIndex,
) -> bool {
    let members = class_members(g, table, nat_class, &HashSet::new());
    let mut seen_values: Vec<u64> = Vec::with_capacity(members.len());
    for cd in members {
        let value = table.get(cd).feature_lanes()[feature.0 as usize];
        if seen_values.contains(&value) {
            return false;
        }
        seen_values.push(value);
    }
    true
}

fn alpha_members(
    g: &Grammar,
    table: &CharDefTable,
    context: &pg_grammar::model::SimpleContext,
    scope: PatternLowerScope,
) -> Option<(Vec<CharDefId>, bool)> {
    let ambiguous = context.vars.iter().any(|var| {
        !var.plus
            && !class_feature_partition_is_unambiguous(g, table, context.nat_class, var.feature)
    });
    let excluded = context.vars.iter().map(|var| var.feature.0 as usize).collect();
    let members = class_members(g, table, context.nat_class, &excluded);
    if ambiguous
        && (scope == PatternLowerScope::Baseline
            || !matches!(g.natural_classes[context.nat_class.0 as usize].kind, NaturalClassKind::Segments(_))
            || members.is_empty()
            || !context.vars.iter().all(|var| {
                g.phon_features.symbol_count(var.feature) == 2
                    && members.iter().all(|member| {
                        table.get(*member).feature_lanes()[var.feature.0 as usize].count_ones() == 1
                    })
            }))
    {
        return None;
    }
    Some((members, ambiguous))
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
                    let (base, ambiguous_disagree) = alpha_members(g, table, sc, scope)?;
                    let occurrence = *next_occurrence;
                    *next_occurrence += 1;
                    let vars = sc.vars.iter().map(|v| (v.var, v.feature, v.plus)).collect();
                    let explicit_segment_class = matches!(
                        g.natural_classes[sc.nat_class.0 as usize].kind,
                        NaturalClassKind::Segments(_)
                    );
                    out.push(Slot::Alpha {
                        vars,
                        occurrence,
                        base_members: base,
                        explicit_segment_class,
                        ambiguous_disagree,
                    });
                }
            }
            PatternNode::Quantifier { min, max, children } => {
                // A genuinely unbounded quantifier is accepted here: it has its own native, finite-size foma construction, so refusing it would be a scope line, not a feasibility finding. The inverted-bound check applies only to a finite bound and is skipped for `None`.
                if let Some(max_v) = max {
                    // Inverted bounds are malformed authoring data, so their interpretation is not guessed.
                    if min > max_v {
                        return None;
                    }
                }
                let mut child_slots = slots_from_nodes(g, table, children, next_occurrence, scope)?;
                if child_slots.is_empty() {
                    // No renderable child at all — nothing to bound-repeat, so honest-unsupported rather than rendering a vacuous group.
                    return None;
                }
                if slots_contain_alpha(&child_slots) {
                    if scope != PatternLowerScope::RewriteEnvironment {
                        return None;
                    }
                    defer_alpha_agreement(&mut child_slots);
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
                if scope == PatternLowerScope::Baseline {
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
            PatternNode::Anchor(_) => {
                if scope == PatternLowerScope::Baseline {
                    return None;
                }
                out.push(Slot::Anchor);
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

/// Occurrence tuples preserve each class while agreeing by feature value, using the same table as lowering.
pub(crate) fn resolve_alpha_tuples(
    table: &CharDefTable,
    slot_lists: &[&[Slot]],
) -> (Vec<AlphaAssignment>, TupleReport) {
    // Flatten to (occurrence, vars, members) in document order, plus the var-group membership needed for the filter step; one occurrence may carry many (var, feature, polarity) triples, all constraining the same concrete segment.
    struct Occ {
        id: usize,
        vars: Vec<(VarId, pg_grammar::featsys::FlatIndex, bool)>,
        members: Vec<CharDefId>,
    }
    let mut occs: Vec<Occ> = Vec::new();
    for slots in slot_lists {
        for slot in slots.iter() {
            if let Slot::Alpha {
                vars,
                occurrence,
                base_members,
                ..
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

    // Joint-polarity filter: same polarity requires the pair to unify (overlap); opposite requires disjoint.
    let mut var_pairs: std::collections::HashMap<
        VarId,
        Vec<(usize, pg_grammar::featsys::FlatIndex, bool)>,
    > = std::collections::HashMap::new();
    for occ in &occs {
        for &(var, feature, plus) in &occ.vars {
            var_pairs
                .entry(var)
                .or_default()
                .push((occ.id, feature, plus));
        }
    }
    let lane_value = |cd: CharDefId, feature: pg_grammar::featsys::FlatIndex| -> u64 {
        table.get(cd).feature_lanes()[feature.0 as usize]
    };
    let survivors: Vec<AlphaAssignment> = assignments
        .into_iter()
        .filter(|asg| {
            var_pairs.values().all(|occs_for_var| {
                occs_for_var.iter().all(|&(id_a, feat, plus_a)| {
                    occs_for_var.iter().all(|&(id_b, _, plus_b)| {
                        let a = lane_value(asg[&id_a], feat);
                        let b = lane_value(asg[&id_b], feat);
                        if plus_a == plus_b {
                            a & b != 0
                        } else {
                            a & b == 0
                        }
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
            Slot::Union(members) | Slot::DeferredAlpha { members, .. } => {
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
            // Foma's own `.#.` word-boundary xre atom: the rendered position, not the source-side tag, conveys word-initial vs word-final.
            Slot::Anchor => ".#.".to_string(),
        };
        pieces.push(piece);
    }
    pieces.join(" ")
}

/// A typed refusal from the pattern owner.
///
/// Exact overlap checks reject literal segments and anchors; confirmed rewrite environments
/// can widen repeated alpha agreement. Malformed repetition and ambiguous disagreement refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedPatternNode {
    /// Malformed repetition or alpha repetition outside the confirmed environment scope.
    Quantifier,
    /// An inline pre-segmented literal `Segments` shape group, under `PatternLowerScope::Baseline` only.
    Segments,
    /// A word-boundary `Anchor` condition, under `PatternLowerScope::Baseline` only.
    Anchor,
    /// A disagree-polarity `AlphaVar` whose own natural class has two members sharing that feature's value -- disagreement is genuinely one-to-many, which `resolve_alpha_tuples`' branch-union construction does not represent faithfully.
    AlphaAmbiguousDisagree,
}

impl std::fmt::Display for UnsupportedPatternNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            UnsupportedPatternNode::Quantifier => "Quantifier (OptionalSegmentSequence)",
            UnsupportedPatternNode::Segments => "Segments (inline PhoneticShape group)",
            UnsupportedPatternNode::Anchor => "Anchor (word-boundary condition)",
            UnsupportedPatternNode::AlphaAmbiguousDisagree => {
                "Context with a disagree-polarity AlphaVariable whose feature does not uniquely determine a class member"
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
    g: &Grammar,
    table: &CharDefTable,
    nodes: &[PatternNode],
    scope: PatternLowerScope,
) -> Option<UnsupportedPatternNode> {
    for node in nodes {
        match node {
            PatternNode::CharDef(_) => {}
            PatternNode::Context(sc) => {
                if !sc.vars.is_empty() && alpha_members(g, table, sc, scope).is_none() {
                    return Some(UnsupportedPatternNode::AlphaAmbiguousDisagree);
                }
            }
            PatternNode::Quantifier { min, max, children } => {
                if let Some(max_v) = max {
                    if min > max_v {
                        return Some(UnsupportedPatternNode::Quantifier);
                    }
                }
                if children.is_empty() {
                    return Some(UnsupportedPatternNode::Quantifier);
                }
                // A well-formed quantifier's own children might still hide the true failing node, so recurse rather than assume this quantifier is the culprit just because it is the first one seen.
                if let Some(reason) = diagnose_unsupported_nodes(g, table, children, scope) {
                    return Some(reason);
                }
                // Exact consumers cannot erase agreement between repeated occurrences.
                if scope != PatternLowerScope::RewriteEnvironment && nodes_contain_alpha_context(children) {
                    return Some(UnsupportedPatternNode::Quantifier);
                }
            }
            PatternNode::Segments { .. } => {
                if scope == PatternLowerScope::Baseline {
                    return Some(UnsupportedPatternNode::Segments);
                }
            }
            PatternNode::Anchor(_) => {
                if scope == PatternLowerScope::Baseline {
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
mod tests;
