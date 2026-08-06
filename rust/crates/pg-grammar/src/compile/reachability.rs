//! Post-hoc reachability compaction for morphological rules and morpheme co-occurrence rules, the mrule/morpheme-scoped sibling of `super::natclass::compact_to_referenced`, extended to cascade through the allomorph-owner registry's back-references.
//! See docs/research/pg-grammar-reachability-compaction-design-notes.md for why `mrules` needs this, why the cascade is required unlike `natclass`, and how morpheme co-occurrence rules are handled separately.

use std::collections::HashMap as StdHashMap;

use hashbrown::HashSet;

use crate::model::{
    AffixAllomorphDef, AllomorphCoOccurrenceRuleDef, AllomorphId, AllomorphOwner, Grammar, MRuleId,
    MorphRuleDef,
};

/// Compact `grammar.mrules` to exactly the set HCLoader's own exporter would ever visit, remapping every surviving `MRuleId` to a dense index, then cascade the same treatment to `grammar.allomorph_owners` and every surviving allomorph's own `id`/`co_occurrence` (see module doc for why the cascade is required).
pub(crate) fn compact_mrules(grammar: &mut Grammar, warnings: &mut Vec<String>) {
    // --- 1. Every mrule a stratum or an (enabled) template slot actually names. ---
    let mut used_mrules: HashSet<u32> = HashSet::new();
    for s in &grammar.strata {
        for r in &s.mrules {
            used_mrules.insert(r.0);
        }
    }
    for t in &grammar.templates {
        for slot in &t.slots {
            for r in &slot.rules {
                used_mrules.insert(r.0);
            }
        }
    }

    // --- 2. Compact grammar.mrules; remap the two structural reference sites above. ---
    let old_mrules = std::mem::take(&mut grammar.mrules);
    let mut old_to_new_mrule: StdHashMap<u32, u32> = StdHashMap::with_capacity(used_mrules.len());
    let mut new_mrules = Vec::with_capacity(used_mrules.len());
    for (old_id, def) in old_mrules.into_iter().enumerate() {
        if used_mrules.contains(&(old_id as u32)) {
            old_to_new_mrule.insert(old_id as u32, new_mrules.len() as u32);
            new_mrules.push(def);
        }
    }
    grammar.mrules = new_mrules;

    for s in &mut grammar.strata {
        for r in &mut s.mrules {
            r.0 = *old_to_new_mrule
                .get(&r.0)
                .expect("stratum mrule referenced but not marked used -- compaction sweep bug");
        }
    }
    for t in &mut grammar.templates {
        for slot in &mut t.slots {
            for r in &mut slot.rules {
                r.0 = *old_to_new_mrule.get(&r.0).expect(
                    "template slot mrule referenced but not marked used -- compaction sweep bug",
                );
            }
        }
    }

    // 3. Cascade to the allomorph-owner registry: a `Root`-owned allomorph always survives; an `Affix`-owned one survives iff its mrule did, remapped to that mrule's new dense id.
    let old_owners = std::mem::take(&mut grammar.allomorph_owners);
    let mut old_to_new_allo: StdHashMap<u32, u32> = StdHashMap::with_capacity(old_owners.len());
    let mut new_owners = Vec::with_capacity(old_owners.len());
    for (old_id, owner) in old_owners.into_iter().enumerate() {
        let kept = match owner {
            AllomorphOwner::Root(le, k) => Some(AllomorphOwner::Root(le, k)),
            AllomorphOwner::Affix(mr, k) => old_to_new_mrule
                .get(&mr.0)
                .map(|&new_mr| AllomorphOwner::Affix(MRuleId(new_mr), k)),
        };
        if let Some(new_owner) = kept {
            old_to_new_allo.insert(old_id as u32, new_owners.len() as u32);
            new_owners.push(new_owner);
        }
    }
    grammar.allomorph_owners = new_owners;

    // 4. Fix up every surviving allomorph's own self-tagging `id` and remap/drop any `others` reference through the same table; a dropped mrule's own allomorphs vanished along with it in step 2.
    for e in &mut grammar.entries {
        for a in &mut e.allomorphs {
            remap_allomorph_id_and_coocc(
                &mut a.id,
                &mut a.co_occurrence,
                &old_to_new_allo,
                warnings,
            );
        }
    }
    for r in &mut grammar.mrules {
        let allos: &mut Vec<AffixAllomorphDef> = match r {
            MorphRuleDef::AffixProcess(d) => &mut d.allomorphs,
            MorphRuleDef::Realizational(d) => &mut d.allomorphs,
            MorphRuleDef::Compounding(_) => continue,
        };
        for a in allos {
            remap_allomorph_id_and_coocc(
                &mut a.id,
                &mut a.co_occurrence,
                &old_to_new_allo,
                warnings,
            );
        }
    }
}

fn remap_allomorph_id_and_coocc(
    id: &mut AllomorphId,
    coocc: &mut Vec<AllomorphCoOccurrenceRuleDef>,
    old_to_new: &StdHashMap<u32, u32>,
    warnings: &mut Vec<String>,
) {
    id.0 = *old_to_new
        .get(&id.0)
        .expect("surviving allomorph id not marked used -- compaction sweep bug");
    coocc.retain_mut(|rule| {
        let before = rule.others.len();
        rule.others = rule
            .others
            .iter()
            .filter_map(|o| old_to_new.get(&o.0).map(|&nid| AllomorphId(nid)))
            .collect();
        if rule.others.len() < before {
            warnings.push(
                "allomorph co-occurrence rule: an 'others' target was dropped by mrule \
                 reachability compaction; reference removed"
                    .to_string(),
            );
        }
        !rule.others.is_empty()
    });
}

/// Drops a `MorphemeCoOccurrenceRuleDef` whose primary morpheme or any `others` target is no longer reachable after `compact_mrules`; must run after it, since "reachable" is defined in terms of the already-compacted grammar.
pub(crate) fn trim_unreachable_morpheme_coocurrence(grammar: &mut Grammar) {
    let mut reachable: HashSet<u32> = HashSet::new();
    for r in &grammar.mrules {
        match r {
            MorphRuleDef::AffixProcess(d) => {
                reachable.insert(d.morpheme.0);
            }
            MorphRuleDef::Realizational(d) => {
                reachable.insert(d.morpheme.0);
            }
            MorphRuleDef::Compounding(_) => {}
        }
    }
    for e in &grammar.entries {
        reachable.insert(e.morpheme.0);
    }

    for (i, m) in grammar.morphemes.iter_mut().enumerate() {
        if !reachable.contains(&(i as u32)) {
            m.co_occurrence.clear();
        } else {
            m.co_occurrence
                .retain(|rule| rule.others.iter().all(|o| reachable.contains(&o.0)));
        }
    }
}
