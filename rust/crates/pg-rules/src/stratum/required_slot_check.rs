//! The required-slot look-ahead as the analyzer applies it (see `crate::required_slots`).

use super::*;
use crate::required_slots::{self, SlotWalk};
use pg_fst::Segment;

impl StratumAnalyzer<'_, '_, '_, '_, '_, '_> {
    pub(super) fn slot_material(&self, tid: TemplateId) -> &RequiredSlotMaterial {
        match self.cache {
            Some(cache) => cache.template_material(tid),
            None => &self.fallback_material.get_or_init(|| {
                (0..self.g.templates.len())
                    .map(|t| RequiredSlotMaterial::build(self.g, TemplateId(t as u32), None))
                    .collect()
            })[tid.0 as usize],
        }
    }

    /// True if `w` can no longer complete template `tid`: some required slot among `last_slot` and the slots before it cannot find its inserted material. Testing runs from the next slot to unapply and stops after an impure slot, which makes the test for any slot behind it unsound. Shadow mode prunes nothing: it counts and records the fired slots in `fired`.
    pub(super) fn required_slot_cannot_unapply(
        &self,
        tid: TemplateId,
        w: &Word,
        last_slot: isize,
        fired: &mut Vec<usize>,
        segs: &mut Option<Vec<Segment>>,
    ) -> bool {
        let mode = self.cfg.required_slot_lookahead;
        if mode == RequiredSlotLookahead::Off {
            return false;
        }
        let material = self.slot_material(tid);
        let tmpl = &self.g.templates[tid.0 as usize];
        let mut walk = SlotWalk::start();
        for j in (0..=last_slot).rev() {
            let j = j as usize;
            if !slot_optional(&tmpl.slots[j]) && !fired.contains(&j) {
                let rows = segs.get_or_insert_with(|| {
                    crate::morph::segs_of(self.g, self.stratum.table, &w.shape, false).0
                });
                if material.cannot_unapply(j, rows, &mut walk) {
                    if mode == RequiredSlotLookahead::On {
                        return true;
                    }
                    fired.push(j);
                    required_slots::shadow_fired();
                }
            }
            material.advance(j, &mut walk);
            if !material.is_pure(j) {
                break;
            }
        }
        false
    }

    /// Shadow: a rule of slot `slot` unapplied under a partial for which that slot had fired.
    pub(super) fn shadow_violated(&self, fired: &[usize], slot: usize) {
        if fired.contains(&slot) {
            required_slots::shadow_violated();
        }
    }

    /// Shadow: a template walk completed with `fired` slots on its path.
    pub(super) fn shadow_completed(&self, fired: &[usize]) {
        for _ in fired {
            required_slots::shadow_completed_under_fired();
        }
    }

    /// The look-ahead for every live template passing through `node`, on the word `w` that reached it: a failing template is marked dead (returned, to be undone on leaving the node), and in shadow mode a template's newly fired slots are returned as the fired sets below this node.
    pub(super) fn shared_look_ahead(
        &self,
        node: &SlotTrieNode,
        w: &Word,
        walk: &mut TrieWalk,
        fired: &[Vec<usize>],
    ) -> (Vec<usize>, Option<Vec<Vec<usize>>>) {
        let mut marked = Vec::new();
        if self.cfg.required_slot_lookahead == RequiredSlotLookahead::Off {
            return (marked, None);
        }
        let mut segs = None;
        let mut fired_here: Option<Vec<Vec<usize>>> = None;
        for &pos in &node.below {
            if !walk.live(pos) {
                continue;
            }
            let tid = self.stratum.templates[pos];
            let last_slot =
                self.g.templates[tid.0 as usize].slots.len() as isize - 1 - node.depth as isize;
            let mut template_fired = fired_of(fired, pos).to_vec();
            if self.required_slot_cannot_unapply(tid, w, last_slot, &mut template_fired, &mut segs)
            {
                walk.dead[pos] = true;
                marked.push(pos);
            } else if template_fired.len() != fired_of(fired, pos).len() {
                let all = fired_here.get_or_insert_with(|| {
                    let mut all = fired.to_vec();
                    all.resize(walk.admitted.len(), Vec::new());
                    all
                });
                all[pos] = template_fired;
            }
        }
        (marked, fired_here)
    }
}
