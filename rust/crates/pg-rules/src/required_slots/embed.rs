//! Order, edge and window tests of the required-slot look-ahead (machine #576); the argument is in docs/divergences/111.

use pg_fst::Segment;

/// Stands for "any number" in the reach of a slot.
pub(super) const UNBOUNDED: usize = usize::MAX / 4;

/// What the slots walked since the shape was taken have established about it.
pub(crate) struct SlotWalk {
    /// Every node the slots still to unapply can match lies after this ordinal.
    lo: isize,
    /// Every node they can match lies before this ordinal.
    hi: isize,
    /// The most nodes the slots walked so far can have removed from the left edge.
    reach_left: usize,
    reach_right: usize,
    /// False once a slot walked so far can reorder the nodes it copies.
    ordered: bool,
}

impl SlotWalk {
    pub(crate) fn start() -> Self {
        SlotWalk {
            lo: -1,
            hi: isize::MAX,
            reach_left: 0,
            reach_right: 0,
            ordered: true,
        }
    }

    pub(super) fn is_ordered(&self) -> bool {
        self.ordered
    }

    /// Records that a slot with this reach is the next to unapply, so the slots before it are tested against what it can leave of the shape.
    pub(super) fn advance(&mut self, ordered: bool, reach_left: usize, reach_right: usize) {
        if !self.ordered || !ordered {
            *self = SlotWalk {
                lo: -1,
                hi: isize::MAX,
                reach_left: UNBOUNDED,
                reach_right: UNBOUNDED,
                ordered: false,
            };
            return;
        }
        self.reach_left = UNBOUNDED.min(self.reach_left + reach_left);
        self.reach_right = UNBOUNDED.min(self.reach_right + reach_right);
    }

    /// The window the slots before a passing slot are left with.
    pub(super) fn narrow(&mut self, lo: isize, hi: isize) {
        self.lo = lo;
        self.hi = hi;
    }

    pub(super) fn lo(&self) -> isize {
        self.lo
    }

    pub(super) fn hi(&self) -> isize {
        self.hi
    }
}

/// One allomorph's inserted segments and where they sit among its copied parts.
pub(super) struct AllomorphSummary {
    pub(super) sequence: Vec<Vec<u64>>,
    pub(super) pre: usize,
    pub(super) post: usize,
}

/// `None` when the work budget ran out mid-test.
fn unifiable(seg: &Segment, required: &[u64]) -> Option<bool> {
    pg_fst::work::consume().then(|| pg_featstruct::flat_unifiable(&seg.lanes, required))
}

/// Where an embedding put its prefix and suffix, as ordinals.
pub(super) struct Embedding {
    pub(super) prefix_end: isize,
    pub(super) suffix_start: isize,
}

/// The earliest embedding of the inserted segments, in order, into distinct segment nodes inside the window with the prefix and suffix within reach of their edge; `Some(None)` if there is none, `None` if the budget ran out. A node counts against the reach only when it cannot be skipped, because the matcher may skip an optional node.
pub(super) fn embeds(
    allomorph: &AllomorphSummary,
    segs: &[Segment],
    walk: &SlotWalk,
) -> Option<Option<Embedding>> {
    let sequence = &allomorph.sequence;
    let front = sequence.len() - allomorph.post;
    let mut prefix_end = -1;
    let mut forward_end = -1;
    let mut next = 0;
    let mut before = 0;
    for (ordinal, seg) in segs.iter().enumerate() {
        if next == front || ordinal as isize >= walk.hi {
            break;
        }
        if ordinal as isize > walk.lo {
            if next < allomorph.pre && before > UNBOUNDED.min(walk.reach_left + next) {
                return Some(None);
            }
            if unifiable(seg, &sequence[next])? {
                if next + 1 == allomorph.pre {
                    prefix_end = ordinal as isize;
                }
                forward_end = ordinal as isize;
                next += 1;
            }
        }
        if !seg.optional {
            before += 1;
        }
    }
    if next < front {
        return Some(None);
    }
    let mut suffix_start = isize::MAX;
    let mut taken = 0;
    let mut after = 0;
    for ordinal in (0..segs.len()).rev() {
        if taken >= allomorph.post || ordinal as isize <= walk.lo || ordinal as isize <= forward_end
        {
            break;
        }
        let seg = &segs[ordinal];
        if (ordinal as isize) < walk.hi {
            if after > UNBOUNDED.min(walk.reach_right + taken) {
                return Some(None);
            }
            if unifiable(seg, &sequence[sequence.len() - 1 - taken])? {
                suffix_start = ordinal as isize;
                taken += 1;
            }
        }
        if !seg.optional {
            after += 1;
        }
    }
    Some((taken == allomorph.post).then_some(Embedding {
        prefix_end,
        suffix_start,
    }))
}

/// Each inserted segment found somewhere among the segment nodes, in no order: all that holds once a slot that can reorder has been walked.
pub(super) fn all_present(allomorph: &AllomorphSummary, segs: &[Segment]) -> Option<bool> {
    for required in &allomorph.sequence {
        let mut found = false;
        for seg in segs {
            if unifiable(seg, required)? {
                found = true;
                break;
            }
        }
        if !found {
            return Some(false);
        }
    }
    Some(true)
}
