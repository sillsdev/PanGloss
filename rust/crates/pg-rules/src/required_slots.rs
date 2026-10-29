//! The required-slot look-ahead of the analysis affix-template walk (C# `RequiredSlotMaterial`, machine #575).
//!
//! `embed` adds the order, edge and window tests of machine #576.
//!
//! A necessary condition for a required template slot to unapply later in a walk: some allomorph of
//! some rule in the slot must have every inserted segment unify with a segment of the shape.
//!
//! Soundness. The analysis pattern of an allomorph holds one top-level constraint per inserted
//! segment, and the rule can only unapply if each constraint unifies with a segment node of the
//! shape. A rule is pure when every right-hand action copies an input part or inserts material and
//! every left-hand part holding a segment is copied and must capture at least one. Then
//! `generate_shape` only copies input nodes and never untruncates a segment. A constraint that
//! unifies with no node of the shape unifies with none after any number of pure unapplications.
//! Inside one template walk only that template's slot rules run, and the stratum's phonological
//! rules ran before the walk. The test for slot j is therefore used only when every slot still to be
//! walked after j is pure. Optional nodes count as present because the matcher can match them. A
//! null affix, a rule that is not an affix process and an allomorph whose analysis pattern did not
//! build leave the slot unprunable.

mod embed;

use std::cell::Cell;

use pg_fst::Segment;
use pg_grammar_model::chardef::{CharDefId, CharDefKind};
use pg_grammar_model::model::{
    AffixAllomorphDef, Grammar, MRuleId, MorphRuleDef, OutputAction, PartRef, Pattern, PatternNode,
    TableId, TemplateId,
};
use pg_shape::NodeKind;

pub(crate) use self::embed::SlotWalk;
use self::embed::{all_present, embeds, AllomorphSummary, UNBOUNDED};
use crate::cache::{owning_table_for_morpheme, RuleCache};
use crate::morph::{build_analysis_lhs_for, InsertedMaterial};

/// How the analysis template walk uses the look-ahead (C# `RequiredSlotLookaheadMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RequiredSlotLookahead {
    /// The look-ahead is not evaluated.
    Off,
    /// It is evaluated and counted (`take_shadow`), never pruning.
    Shadow,
    /// It prunes template partials.
    #[default]
    On,
}

/// Counts of the shadow mode; any `violated` or `completed_under_fired` is a soundness failure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LookaheadShadow {
    /// The test was true for a required slot.
    pub fired: u64,
    /// A rule of a fired slot then unapplied under that partial.
    pub violated: u64,
    /// A template walk completed with a fired slot on its path.
    pub completed_under_fired: u64,
}

thread_local! {
    static SHADOW: Cell<LookaheadShadow> = const { Cell::new(LookaheadShadow { fired: 0, violated: 0, completed_under_fired: 0 }) };
}

/// Returns and clears this thread's shadow counts.
pub fn take_shadow() -> LookaheadShadow {
    SHADOW.with(|c| c.replace(LookaheadShadow::default()))
}

pub(crate) fn shadow_fired() {
    SHADOW.with(|c| {
        let mut s = c.get();
        s.fired += 1;
        c.set(s);
    });
}

pub(crate) fn shadow_violated() {
    SHADOW.with(|c| {
        let mut s = c.get();
        s.violated += 1;
        c.set(s);
    });
}

pub(crate) fn shadow_completed_under_fired() {
    SHADOW.with(|c| {
        let mut s = c.get();
        s.completed_under_fired += 1;
        c.set(s);
    });
}

/// The look-ahead tables of one template, built once from the grammar.
pub struct RequiredSlotMaterial {
    slots: Vec<SlotSummary>,
}

#[derive(Default)]
struct SlotSummary {
    impure: bool,
    /// False when some rule can reorder or repeat the nodes it copies.
    unordered: bool,
    may_always_unapply: bool,
    reach_left: usize,
    reach_right: usize,
    allomorphs: Vec<AllomorphSummary>,
}

impl RequiredSlotMaterial {
    pub(crate) fn build(g: &Grammar, tid: TemplateId, cache: Option<&RuleCache>) -> Self {
        let slots = g.templates[tid.0 as usize]
            .slots
            .iter()
            .map(|slot| summarize(g, &slot.rules, cache))
            .collect();
        RequiredSlotMaterial { slots }
    }

    /// Whether every rule of the slot is pure, so unapplying it leaves the segments needed by earlier slots no easier to find.
    pub(crate) fn is_pure(&self, slot: usize) -> bool {
        !self.slots[slot].impure
    }

    /// Records that the slot is the next one to unapply, so the slots before it are tested against what it can leave of the shape.
    pub(crate) fn advance(&self, slot: usize, walk: &mut SlotWalk) {
        let summary = &self.slots[slot];
        walk.advance(!summary.unordered, summary.reach_left, summary.reach_right);
    }

    /// True if no allomorph of the slot can unapply to a word with these segments, provided every slot walked between the word and this one is pure. When the slot can unapply and is ordered, narrows the window of `walk` for the slots before it. Each shape node tested costs one work unit; on exhaustion the answer is `false`.
    pub(crate) fn cannot_unapply(
        &self,
        slot: usize,
        segs: &[Segment],
        walk: &mut SlotWalk,
    ) -> bool {
        let summary = &self.slots[slot];
        if summary.may_always_unapply {
            return false;
        }
        // Narrowing the window needs every passing allomorph, not the first, and only helps a slot that has slots after it in the walk.
        let narrow = walk.is_ordered() && !summary.unordered && slot > 0;
        let (mut new_lo, mut new_hi) = (isize::MAX, -1);
        let mut any = false;
        for allomorph in &summary.allomorphs {
            let found = if walk.is_ordered() {
                match embeds(allomorph, segs, walk) {
                    None => return false,
                    Some(embedding) => embedding,
                }
            } else {
                match all_present(allomorph, segs) {
                    None => return false,
                    Some(true) => Some(embed::Embedding {
                        prefix_end: -1,
                        suffix_start: isize::MAX,
                    }),
                    Some(false) => None,
                }
            };
            let Some(embedding) = found else {
                continue;
            };
            if !narrow {
                return false;
            }
            any = true;
            new_lo = new_lo.min(if allomorph.pre > 0 {
                embedding.prefix_end
            } else {
                walk.lo()
            });
            new_hi = new_hi.max(if allomorph.post > 0 {
                embedding.suffix_start
            } else {
                walk.hi()
            });
        }
        if !any {
            return true;
        }
        walk.narrow(new_lo, new_hi);
        false
    }
}

fn summarize(g: &Grammar, rules: &[MRuleId], cache: Option<&RuleCache>) -> SlotSummary {
    let mut summary = SlotSummary::default();
    for &rid in rules {
        let (morpheme, allos) = match &g.mrules[rid.0 as usize] {
            MorphRuleDef::AffixProcess(def) => (def.morpheme, &def.allomorphs),
            MorphRuleDef::Realizational(def) => (def.morpheme, &def.allomorphs),
            MorphRuleDef::Compounding(_) => {
                summary.may_always_unapply = true;
                summary.impure = true;
                summary.unordered = true;
                summary.reach_left = UNBOUNDED;
                summary.reach_right = UNBOUNDED;
                continue;
            }
        };
        let table = owning_table_for_morpheme(g, morpheme).unwrap_or(TableId(0));
        for allo in allos {
            let pure = is_pure_allomorph(g, table, allo);
            summary.impure |= !pure;
            summary.unordered |= !pure || !is_ordered_allomorph(allo);
            let Some(inserted) = inserted_material(g, table, allo, cache) else {
                // The analysis pattern did not build, so the rule is never applied; stay conservative.
                summary.may_always_unapply = true;
                summary.impure = true;
                summary.unordered = true;
                summary.reach_left = UNBOUNDED;
                summary.reach_right = UNBOUNDED;
                continue;
            };
            let (reach_left, reach_right) = if inserted.infix {
                (UNBOUNDED, UNBOUNDED)
            } else {
                (inserted.pre, inserted.post)
            };
            summary.reach_left = summary.reach_left.max(reach_left);
            summary.reach_right = summary.reach_right.max(reach_right);
            if inserted.sequence.is_empty() {
                // A null affix inserts nothing to look for.
                summary.may_always_unapply = true;
            } else {
                summary.allomorphs.push(AllomorphSummary {
                    sequence: inserted.sequence,
                    pre: if inserted.infix { 0 } else { inserted.pre },
                    post: if inserted.infix { 0 } else { inserted.post },
                });
            }
        }
    }
    summary
}

fn inserted_material(
    g: &Grammar,
    table: TableId,
    allo: &AffixAllomorphDef,
    cache: Option<&RuleCache>,
) -> Option<InsertedMaterial> {
    match cache {
        Some(c) => c
            .allomorph(allo.id)
            .ana_lhs
            .as_ref()
            .map(|(_, lhs)| lhs.inserted_material()),
        None => build_analysis_lhs_for(g, table, allo)
            .ok()
            .map(|lhs| lhs.inserted_material()),
    }
}

/// `generate_shape` writes parts in left-hand order, so copying them in another order, or one twice, can reorder or repeat nodes.
fn is_ordered_allomorph(allo: &AffixAllomorphDef) -> bool {
    let mut previous = None;
    for action in &allo.rhs {
        if let OutputAction::Copy(PartRef::Input(i)) = action {
            if previous.is_some_and(|p| *i <= p) {
                return false;
            }
            previous = Some(*i);
        }
    }
    true
}

/// Every right-hand action copies an input part or inserts material, and every left-hand part holding a segment is copied and captures at least one.
fn is_pure_allomorph(g: &Grammar, table: TableId, allo: &AffixAllomorphDef) -> bool {
    let mut copied: Vec<u16> = Vec::new();
    for action in &allo.rhs {
        match action {
            OutputAction::Copy(PartRef::Input(i)) => copied.push(*i),
            OutputAction::InsertSegments { .. } | OutputAction::InsertContext(_) => {}
            _ => return false,
        }
    }
    allo.lhs.iter().enumerate().all(|(i, part)| {
        !has_segment(g, table, &part.nodes)
            || restored_segments(g, table, &part.nodes) == 0
            || (copied.contains(&(i as u16)) && min_segments(g, table, part) > 0)
    })
}

fn is_segment_chardef(g: &Grammar, table: TableId, cd: CharDefId) -> bool {
    let t = &g.char_tables[table.0 as usize];
    (cd.0 as usize) < t.len() && t.get(cd).kind() == CharDefKind::Segment
}

fn has_segment(g: &Grammar, table: TableId, nodes: &[PatternNode]) -> bool {
    nodes.iter().any(|n| match n {
        PatternNode::Context(_) => true,
        PatternNode::CharDef(cd) => is_segment_chardef(g, table, *cd),
        PatternNode::Quantifier { children, .. } => has_segment(g, table, children),
        PatternNode::Segments { shape, .. } => shape
            .shape
            .interior()
            .any(|(_, kind, _, _)| kind == NodeKind::Segment),
        PatternNode::Anchor(_) => false,
    })
}

/// The segment nodes `untruncate` writes for a part that was not captured; an unbounded quantifier, a segment string and an anchor write none.
fn restored_segments(g: &Grammar, table: TableId, nodes: &[PatternNode]) -> u64 {
    nodes
        .iter()
        .map(|n| match n {
            PatternNode::Context(_) => 1,
            PatternNode::CharDef(cd) => u64::from(is_segment_chardef(g, table, *cd)),
            PatternNode::Quantifier {
                max: Some(max),
                children,
                ..
            } => u64::from(*max).saturating_mul(restored_segments(g, table, children)),
            PatternNode::Quantifier { max: None, .. }
            | PatternNode::Segments { .. }
            | PatternNode::Anchor(_) => 0,
        })
        .fold(0, u64::saturating_add)
}

/// The fewest segments a successful match of the part must consume. A part matching none is captured as a null range, and `generate_shape` then untruncates it, which adds segments that are not copies.
fn min_segments(g: &Grammar, table: TableId, part: &Pattern) -> u64 {
    fn nodes_min(g: &Grammar, table: TableId, nodes: &[PatternNode]) -> u64 {
        nodes
            .iter()
            .map(|n| match n {
                PatternNode::Context(_) => 1,
                PatternNode::CharDef(cd) => u64::from(is_segment_chardef(g, table, *cd)),
                PatternNode::Quantifier { min, children, .. } => {
                    u64::from(*min) * nodes_min(g, table, children)
                }
                PatternNode::Segments { shape, .. } => shape
                    .shape
                    .interior()
                    .filter(|(_, kind, _, _)| *kind == NodeKind::Segment)
                    .count() as u64,
                PatternNode::Anchor(_) => 0,
            })
            .sum()
    }
    nodes_min(g, table, &part.nodes)
}
