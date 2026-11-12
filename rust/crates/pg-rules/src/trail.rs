//! A word's rule trail as a persistent list whose steps link to their parent.
//!
//! Every analysis step clones a word and appends one entry, so trails of sibling words share their
//! common prefix instead of each owning copies. A step is either a *slot* (one entry of the pair of
//! rule and allomorph sequences) or a *count-only* step, which raises a rule's unapplication count
//! without occupying a slot (generation records realizational rules that way).

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use pg_grammar_model::model::{AllomorphId, MRuleId};

/// The unapplication history of a word; cloning shares every step.
#[derive(Clone, Default)]
pub struct Trail(Option<Rc<Step>>);

struct Step {
    parent: Trail,
    rule: Option<MRuleId>,
    allomorph: Option<AllomorphId>,
    slot: bool,
    /// Slots up to and including this step.
    slots: u32,
}

/// The slot entries only, as the dedup key compares them; count-only steps are invisible here.
#[derive(Clone, Debug)]
pub struct SlotView(Trail);

type Node = Option<Rc<Step>>;

fn skip_count_only(mut node: &Node) -> &Node {
    while let Some(step) = node {
        if step.slot {
            break;
        }
        node = &step.parent.0;
    }
    node
}

impl Trail {
    /// A trail of slots, oldest first.
    pub fn from_slots(
        slots: impl IntoIterator<Item = (Option<MRuleId>, Option<AllomorphId>)>,
    ) -> Self {
        let mut trail = Trail::default();
        for (rule, allomorph) in slots {
            trail.push(rule, allomorph);
        }
        trail
    }

    /// The number of slots.
    pub fn len(&self) -> usize {
        self.0.as_ref().map_or(0, |step| step.slots as usize)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Appends one slot and counts `rule` as unapplied when it is known.
    pub fn push(&mut self, rule: Option<MRuleId>, allomorph: Option<AllomorphId>) {
        let slots = self.len() as u32 + 1;
        self.link(Step {
            parent: Trail::default(),
            rule,
            allomorph,
            slot: true,
            slots,
        });
    }

    /// Counts `rule` as unapplied without occupying a slot.
    pub fn push_count_only(&mut self, rule: MRuleId) {
        let slots = self.len() as u32;
        self.link(Step {
            parent: Trail::default(),
            rule: Some(rule),
            allomorph: None,
            slot: false,
            slots,
        });
    }

    fn link(&mut self, mut step: Step) {
        step.parent = std::mem::take(self);
        self.0 = Some(Rc::new(step));
    }

    /// How many times `rule` has been unapplied.
    pub fn count_of(&self, rule: MRuleId) -> u32 {
        let mut count = 0;
        let mut node = &self.0;
        while let Some(step) = node {
            count += u32::from(step.rule == Some(rule));
            node = &step.parent.0;
        }
        count
    }

    /// The known rule of every step, newest first, count-only steps included: each item is one unapplication.
    pub fn counted_rules(&self) -> impl Iterator<Item = MRuleId> + '_ {
        let mut node = &self.0;
        std::iter::from_fn(move || {
            while let Some(step) = node {
                node = &step.parent.0;
                if step.rule.is_some() {
                    return step.rule;
                }
            }
            None
        })
    }

    /// Every rule's unapplication count.
    pub fn rule_counts(&self) -> BTreeMap<MRuleId, u32> {
        let mut counts = BTreeMap::new();
        let mut node = &self.0;
        while let Some(step) = node {
            if let Some(rule) = step.rule {
                *counts.entry(rule).or_insert(0) += 1;
            }
            node = &step.parent.0;
        }
        counts
    }

    fn slot_step(&self, idx: usize) -> Option<&Step> {
        if idx >= self.len() {
            return None;
        }
        let want = idx as u32 + 1;
        let mut node = &self.0;
        while let Some(step) = node {
            if step.slot && step.slots == want {
                return Some(step);
            }
            node = &step.parent.0;
        }
        None
    }

    /// The rule of slot `idx`, oldest first; `Some(None)` is a slot with no known rule.
    pub fn rule(&self, idx: usize) -> Option<Option<MRuleId>> {
        self.slot_step(idx).map(|step| step.rule)
    }

    /// The allomorph of slot `idx`, oldest first.
    pub fn allomorph(&self, idx: usize) -> Option<Option<AllomorphId>> {
        self.slot_step(idx).map(|step| step.allomorph)
    }

    /// The rule of the newest slot.
    pub fn last_rule(&self) -> Option<Option<MRuleId>> {
        self.slots_rev().next().map(|(rule, _)| rule)
    }

    /// Slots newest first.
    pub fn slots_rev(&self) -> SlotsRev<'_> {
        SlotsRev(skip_count_only(&self.0))
    }

    /// The slot rules, oldest first.
    pub fn rules(&self) -> Vec<Option<MRuleId>> {
        let mut rules: Vec<_> = self.slots_rev().map(|(rule, _)| rule).collect();
        rules.reverse();
        rules
    }

    /// The slot allomorphs, oldest first.
    pub fn allomorphs(&self) -> Vec<Option<AllomorphId>> {
        let mut allomorphs: Vec<_> = self.slots_rev().map(|(_, allomorph)| allomorph).collect();
        allomorphs.reverse();
        allomorphs
    }

    /// Whether both trails are the same allocation.
    pub fn same_allocation(&self, other: &Trail) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }

    /// This trail seen as its slots only.
    pub fn slot_view(&self) -> SlotView {
        SlotView(self.clone())
    }

    /// Whether both trails hold the same slots, ignoring count-only steps.
    pub fn same_slots(&self, other: &Trail) -> bool {
        let (mut a, mut b) = (skip_count_only(&self.0), skip_count_only(&other.0));
        loop {
            match (a, b) {
                (None, None) => return true,
                (Some(x), Some(y)) => {
                    if Rc::ptr_eq(x, y) {
                        return true;
                    }
                    if x.rule != y.rule || x.allomorph != y.allomorph {
                        return false;
                    }
                    a = skip_count_only(&x.parent.0);
                    b = skip_count_only(&y.parent.0);
                }
                _ => return false,
            }
        }
    }
}

/// Slots newest first.
pub struct SlotsRev<'a>(&'a Node);

impl Iterator for SlotsRev<'_> {
    type Item = (Option<MRuleId>, Option<AllomorphId>);

    fn next(&mut self) -> Option<Self::Item> {
        let step = self.0.as_ref()?;
        self.0 = skip_count_only(&step.parent.0);
        Some((step.rule, step.allomorph))
    }
}

impl PartialEq for Trail {
    fn eq(&self, other: &Trail) -> bool {
        let (mut a, mut b) = (&self.0, &other.0);
        loop {
            match (a, b) {
                (None, None) => return true,
                (Some(x), Some(y)) => {
                    if Rc::ptr_eq(x, y) {
                        return true;
                    }
                    if x.rule != y.rule || x.allomorph != y.allomorph || x.slot != y.slot {
                        return false;
                    }
                    a = &x.parent.0;
                    b = &y.parent.0;
                }
                _ => return false,
            }
        }
    }
}

impl Eq for Trail {}

impl std::fmt::Debug for Trail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trail")
            .field("rules", &self.rules())
            .field("allomorphs", &self.allomorphs())
            .field("counts", &self.rule_counts())
            .finish()
    }
}

// Iterative so a trail of any depth drops without recursing through its parents.
impl Drop for Trail {
    fn drop(&mut self) {
        let mut next = self.0.take();
        while let Some(rc) = next {
            match Rc::try_unwrap(rc) {
                Ok(mut step) => next = step.parent.0.take(),
                Err(_) => break,
            }
        }
    }
}

impl SlotView {
    /// Whether `trail` holds the slots this view holds.
    pub fn matches(&self, trail: &Trail) -> bool {
        self.0.same_slots(trail)
    }
}

impl PartialEq for SlotView {
    fn eq(&self, other: &SlotView) -> bool {
        self.0.same_slots(&other.0)
    }
}

impl Eq for SlotView {}

impl Hash for SlotView {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.0.len());
        for slot in self.0.slots_rev() {
            slot.hash(state);
        }
    }
}

#[cfg(test)]
mod tests;
