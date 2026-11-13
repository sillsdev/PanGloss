//! A word's rule trail as a persistent list whose steps link to their parent.
//!
//! Every analysis step clones a word and appends one entry, so trails of sibling words share their
//! common prefix instead of each owning copies. A step is either a *slot* (one entry of the pair of
//! rule and allomorph sequences) or a *count-only* step, which raises a rule's unapplication count
//! without occupying a slot (generation records realizational rules that way).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::rc::Rc;

use pg_grammar_model::model::{AllomorphId, MRuleId};

/// The unapplication history; cloning shares every step. A handle into this thread's step slab, 0 for the empty trail.
#[derive(Default)]
pub struct Trail(u32, PhantomData<Rc<()>>);

/// One slab entry: a step and its reference count, 20 bytes. A free entry keeps the next free handle in `parent`.
#[derive(Clone, Copy)]
struct Step {
    parent: u32,
    /// Meaningful only with `HAS_RULE`.
    rule: u32,
    /// Meaningful only with `HAS_ALLOMORPH`.
    allomorph: u32,
    /// The slots up to and including this step, with the three flags above them.
    packed: u32,
    refs: u32,
}

const CHUNK_BITS: u32 = 16;
const CHUNK: usize = 1 << CHUNK_BITS;
/// What a step cost the meter as a reference-counted allocation.
const METERED_STEP_BYTES: usize = 40;

/// Fixed-size chunks (never copied on growth) and a free list, so a dropped word's steps are reused.
struct Slab {
    chunks: Vec<Vec<Step>>,
    free: u32,
    live: usize,
}

thread_local! {
    static SLAB: RefCell<Slab> = const { RefCell::new(Slab { chunks: Vec::new(), free: 0, live: 0 }) };
}

impl Slab {
    fn get(&self, handle: u32) -> &Step {
        let at = handle as usize - 1;
        &self.chunks[at >> CHUNK_BITS][at & (CHUNK - 1)]
    }

    fn get_mut(&mut self, handle: u32) -> &mut Step {
        let at = handle as usize - 1;
        &mut self.chunks[at >> CHUNK_BITS][at & (CHUNK - 1)]
    }

    fn alloc(&mut self, step: Step) -> u32 {
        self.live += 1;
        // The cost meter charges a step as the 40-byte allocation it replaces; the slab's own growth is unmetered, so cost does not depend on which thread grew it.
        pg_cost::charge_bytes(METERED_STEP_BYTES);
        if self.free != 0 {
            let handle = self.free;
            self.free = self.get(handle).parent;
            *self.get_mut(handle) = step;
            return handle;
        }
        if self.chunks.last().is_none_or(|chunk| chunk.len() == CHUNK) {
            pg_cost::unmetered(|| self.chunks.push(Vec::with_capacity(CHUNK)));
        }
        let last = self.chunks.len() - 1;
        self.chunks[last].push(step);
        let at = last * CHUNK + self.chunks[last].len();
        u32::try_from(at).expect("fewer than 2^32 live trail steps")
    }

    fn release(&mut self, handle: u32) {
        let free = self.free;
        let slot = self.get_mut(handle);
        slot.parent = free;
        slot.refs = 0;
        self.free = handle;
        self.live -= 1;
        if self.live == 0 {
            self.chunks.truncate(1);
            if let Some(first) = self.chunks.first_mut() {
                first.clear();
            }
            self.free = 0;
        }
    }
}

fn step(handle: u32) -> Step {
    SLAB.with(|slab| *slab.borrow().get(handle))
}

const IS_SLOT: u32 = 1 << 31;
const HAS_RULE: u32 = 1 << 30;
const HAS_ALLOMORPH: u32 = 1 << 29;
const SLOTS: u32 = HAS_ALLOMORPH - 1;

impl Step {
    fn new(
        rule: Option<MRuleId>,
        allomorph: Option<AllomorphId>,
        is_slot: bool,
        slots: usize,
    ) -> Self {
        let slots = u32::try_from(slots)
            .ok()
            .filter(|&slots| slots <= SLOTS)
            .expect("a trail holds fewer than 2^29 slots");
        let flag = |on: bool, bit: u32| if on { bit } else { 0 };
        Step {
            parent: 0,
            refs: 1,
            rule: rule.map_or(0, |id| id.0),
            allomorph: allomorph.map_or(0, |id| id.0),
            packed: slots
                | flag(is_slot, IS_SLOT)
                | flag(rule.is_some(), HAS_RULE)
                | flag(allomorph.is_some(), HAS_ALLOMORPH),
        }
    }

    fn rule(&self) -> Option<MRuleId> {
        (self.packed & HAS_RULE != 0).then_some(MRuleId(self.rule))
    }

    fn allomorph(&self) -> Option<AllomorphId> {
        (self.packed & HAS_ALLOMORPH != 0).then_some(AllomorphId(self.allomorph))
    }

    fn is_slot(&self) -> bool {
        self.packed & IS_SLOT != 0
    }

    fn slots(&self) -> u32 {
        self.packed & SLOTS
    }
}

/// The slot entries only, as the dedup key compares them; count-only steps are invisible here.
#[derive(Clone, Debug)]
pub struct SlotView(Trail);

fn skip_count_only(slab: &Slab, mut node: u32) -> u32 {
    while node != 0 {
        let step = slab.get(node);
        if step.is_slot() {
            break;
        }
        node = step.parent;
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
        if self.0 == 0 {
            0
        } else {
            step(self.0).slots() as usize
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Appends one slot and counts `rule` as unapplied when it is known.
    pub fn push(&mut self, rule: Option<MRuleId>, allomorph: Option<AllomorphId>) {
        self.link(Step::new(rule, allomorph, true, self.len() + 1));
    }

    /// Counts `rule` as unapplied without occupying a slot.
    pub fn push_count_only(&mut self, rule: MRuleId) {
        self.link(Step::new(Some(rule), None, false, self.len()));
    }

    fn link(&mut self, mut step: Step) {
        step.parent = self.0;
        self.0 = SLAB.with(|slab| slab.borrow_mut().alloc(step));
    }

    /// How many times `rule` has been unapplied.
    pub fn count_of(&self, rule: MRuleId) -> u32 {
        SLAB.with(|slab| {
            let slab = slab.borrow();
            let mut count = 0;
            let mut node = self.0;
            while node != 0 {
                let step = slab.get(node);
                count += u32::from(step.rule() == Some(rule));
                node = step.parent;
            }
            count
        })
    }

    /// The known rule of every step, newest first, count-only steps included: each item is one unapplication.
    pub fn counted_rules(&self) -> impl Iterator<Item = MRuleId> + '_ {
        let mut node = self.0;
        std::iter::from_fn(move || {
            while node != 0 {
                let step = step(node);
                node = step.parent;
                if step.rule().is_some() {
                    return step.rule();
                }
            }
            None
        })
    }

    /// Every rule's unapplication count.
    pub fn rule_counts(&self) -> BTreeMap<MRuleId, u32> {
        let mut counts = BTreeMap::new();
        SLAB.with(|slab| {
            let slab = slab.borrow();
            let mut node = self.0;
            while node != 0 {
                let step = slab.get(node);
                if let Some(rule) = step.rule() {
                    *counts.entry(rule).or_insert(0) += 1;
                }
                node = step.parent;
            }
        });
        counts
    }

    fn slot_step(&self, idx: usize) -> Option<Step> {
        if idx >= self.len() {
            return None;
        }
        let want = idx as u32 + 1;
        SLAB.with(|slab| {
            let slab = slab.borrow();
            let mut node = self.0;
            while node != 0 {
                let step = slab.get(node);
                if step.is_slot() && step.slots() == want {
                    return Some(*step);
                }
                node = step.parent;
            }
            None
        })
    }

    /// The rule of slot `idx`, oldest first; `Some(None)` is a slot with no known rule.
    pub fn rule(&self, idx: usize) -> Option<Option<MRuleId>> {
        self.slot_step(idx).map(|step| step.rule())
    }

    /// The allomorph of slot `idx`, oldest first.
    pub fn allomorph(&self, idx: usize) -> Option<Option<AllomorphId>> {
        self.slot_step(idx).map(|step| step.allomorph())
    }

    /// The rule of the newest slot.
    pub fn last_rule(&self) -> Option<Option<MRuleId>> {
        self.slots_rev().next().map(|(rule, _)| rule)
    }

    /// Slots newest first.
    pub fn slots_rev(&self) -> SlotsRev<'_> {
        SlotsRev(
            SLAB.with(|slab| skip_count_only(&slab.borrow(), self.0)),
            PhantomData,
        )
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

    /// Whether both trails are the same step.
    pub fn same_allocation(&self, other: &Trail) -> bool {
        self.0 == other.0
    }

    /// This trail seen as its slots only.
    pub fn slot_view(&self) -> SlotView {
        SlotView(self.clone())
    }

    /// Whether both trails hold the same slots, ignoring count-only steps.
    pub fn same_slots(&self, other: &Trail) -> bool {
        SLAB.with(|slab| {
            let slab = slab.borrow();
            let (mut a, mut b) = (
                skip_count_only(&slab, self.0),
                skip_count_only(&slab, other.0),
            );
            loop {
                if a == b {
                    return true;
                }
                if a == 0 || b == 0 {
                    return false;
                }
                let (x, y) = (slab.get(a), slab.get(b));
                if x.rule() != y.rule() || x.allomorph() != y.allomorph() {
                    return false;
                }
                a = skip_count_only(&slab, x.parent);
                b = skip_count_only(&slab, y.parent);
            }
        })
    }
}

/// Slots newest first.
pub struct SlotsRev<'a>(u32, PhantomData<&'a Trail>);

impl Iterator for SlotsRev<'_> {
    type Item = (Option<MRuleId>, Option<AllomorphId>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 {
            return None;
        }
        SLAB.with(|slab| {
            let slab = slab.borrow();
            let step = slab.get(self.0);
            self.0 = skip_count_only(&slab, step.parent);
            Some((step.rule(), step.allomorph()))
        })
    }
}

impl PartialEq for Trail {
    fn eq(&self, other: &Trail) -> bool {
        SLAB.with(|slab| {
            let slab = slab.borrow();
            let (mut a, mut b) = (self.0, other.0);
            loop {
                if a == b {
                    return true;
                }
                if a == 0 || b == 0 {
                    return false;
                }
                let (x, y) = (slab.get(a), slab.get(b));
                if x.rule() != y.rule()
                    || x.allomorph() != y.allomorph()
                    || x.is_slot() != y.is_slot()
                {
                    return false;
                }
                a = x.parent;
                b = y.parent;
            }
        })
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

impl Clone for Trail {
    fn clone(&self) -> Self {
        if self.0 != 0 {
            SLAB.with(|slab| slab.borrow_mut().get_mut(self.0).refs += 1);
        }
        Trail(self.0, PhantomData)
    }
}

// Iterative so a trail of any depth drops without recursing through its parents.
impl Drop for Trail {
    fn drop(&mut self) {
        if self.0 == 0 {
            return;
        }
        let handle = self.0;
        // A drop during thread teardown has no slab left to update.
        let _ = SLAB.try_with(|slab| {
            let mut slab = slab.borrow_mut();
            let mut next = handle;
            while next != 0 {
                let step = slab.get_mut(next);
                step.refs -= 1;
                if step.refs > 0 {
                    break;
                }
                let parent = step.parent;
                slab.release(next);
                next = parent;
            }
        });
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
