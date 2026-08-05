//! Post-hoc reachability compaction for morphological rules and morpheme co-occurrence rules —
//! the mrule/morpheme-scoped sibling of `super::natclass::compact_to_referenced` (read that
//! function's doc first; this module mirrors its used-set + remap-with-expect pattern exactly,
//! extended to cover the extra wrinkle `MRuleId` has that `NatClassId` doesn't: an owner-registry
//! back-reference, not just read-only structural edges).
//!
//! # Why `mrules` needs this at all
//! `compile_project` already filters *templates*: a disabled `MoInflAffixTemplate` never becomes
//! an `AffixTemplateDef` (`templates::build_pos`'s `if tmpl.disabled { continue; }`). That
//! filtering does not propagate to the affix-process rule(s) whose *only* slot lives in that
//! disabled template. `lexicon::build`'s own doc explains why: an `Msa::Inflectional` MSA with at
//! least one slot is `template_only` (HCLoader's `LoadMorphologicalRule`, HCLoader.cs:887-892: `if
//! (inflMsa.SlotsRC.Count > 0) s = null` — the rule is reachable *only* through
//! `AddMorphologicalRule`'s per-slot fallout, `stratum.MorphologicalRules` never sees it). Such a
//! rule is still built unconditionally (`LoadMorphologicalRules` is called per-MSA regardless of
//! template membership) and still lands in `acc.mrules`/`acc.slot_rules[slot_guid]` — but if every
//! slot referencing it belongs to a *disabled* template, nothing in the final `Grammar` ever
//! records that `MRuleId` anywhere HCLoader's own exporter would visit (neither a stratum's own
//! `mrules` list nor an enabled template's slot content). HCLoader's XML export walks exactly
//! those two places, so this reference set is precisely "every place the legacy exporter looks."
//!
//! # Why the allomorph-owner registry needs a cascade, unlike `natclass`
//! `super::natclass::compact_to_referenced` only ever *reads* a `NatClassId` from structural
//! sites (patterns/environments/rules) — dropping an unreferenced class breaks nothing else,
//! because nothing else's own identity depends on that class's position in the `Vec`.
//! `Grammar::allomorph_owners` is different: it's an owner *registry*, indexed by `AllomorphId`,
//! and every `RootAllomorphDef`/`AffixAllomorphDef` in the grammar carries its *own* `id` field
//! that must round-trip back to its position in that registry
//! (`compile::tests::assert_grammar_ids_are_internally_consistent` checks exactly this, and
//! several `pg-rules`/`hc-hybrid` consumers enumerate `allomorph_owners` in registry order and
//! dereference every entry unconditionally — a stale `AllomorphOwner::Affix` pointing at a
//! `MRuleId` outside the now-shorter `mrules` `Vec`, or silently aliasing an unrelated surviving
//! rule after the shift, is a live bug, not a merely-unobserved one). Dropping an `MRuleId` that
//! owns allomorphs therefore requires: dropping its `AllomorphOwner::Affix` rows from
//! `allomorph_owners` too, remapping every surviving `AllomorphId` to a dense index, and fixing up
//! every surviving allomorph's own `id` field plus any `AllomorphCoOccurrenceRuleDef.others`
//! reference through the same table (steps 3-4 below).
//!
//! # Morpheme co-occurrence rules
//! Separately, `MorphemeCoOccurrenceRuleDef`s live on `Grammar::morphemes[i].co_occurrence`
//! (`strata_assign_co_occurrence` in `compile/mod.rs` populates them from the snapshot's
//! `MoMorphAdhocProhib` ad-hoc rules, keyed by MSA guid via `acc.msa_guid_index`) — entirely
//! independent of the `mrules`/`allomorph_owners` cascade above. `Grammar::morphemes` is never
//! compacted (nothing else needs `MorphemeId` to stay dense — the gate's own multiset comparisons
//! resolve `MorphemeId` by content, never by raw index), so no id remap is needed there. But a
//! morpheme whose *sole* mrule was just dropped above is exactly as unreachable as that mrule was,
//! and HCLoader's own XML export skips such a morpheme's `MorphemeCoOccurrenceRules` too
//! (same "walks only what's stratum/template-reachable" principle) — so any co-occurrence
//! rule keyed to (or targeting, via `others`) such a morpheme must be dropped too.

use std::collections::HashMap as StdHashMap;

use hashbrown::HashSet;

use crate::model::{
    AffixAllomorphDef, AllomorphCoOccurrenceRuleDef, AllomorphId, AllomorphOwner, Grammar, MRuleId,
    MorphRuleDef,
};

/// Step 1-2: compact `grammar.mrules` to exactly the set HCLoader's own exporter would ever visit
/// (every stratum's own `mrules` list, plus every already-enabled-template-filtered template
/// slot's `rules`), remapping every surviving `MRuleId` to a dense index in both of those places.
/// Step 3-4: cascade the same treatment to `grammar.allomorph_owners` and every surviving
/// allomorph's own `id`/`co_occurrence` (see this module's top doc for why that cascade is
/// required, unlike `super::natclass::compact_to_referenced`'s simpler read-only-edge case).
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

    // --- 3. Cascade to the allomorph-owner registry: a `Root`-owned allomorph (lex entries are
    // untouched by this pass) always survives; an `Affix`-owned one survives iff its mrule did,
    // remapped to that mrule's new dense id.
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

    // --- 4. Fix up every surviving allomorph's own self-tagging `id` (the round-trip invariant
    // `compile::tests::assert_grammar_ids_are_internally_consistent` checks) and remap/drop any
    // `AllomorphCoOccurrenceRuleDef.others` reference through the same table. A dropped mrule's
    // own allomorphs vanished along with it in step 2, so only surviving allomorphs need visiting.
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

/// Drops a `crate::model::MorphemeCoOccurrenceRuleDef` whose primary morpheme (the one whose
/// `co_occurrence` list holds it) or any `others` target is no longer reachable after
/// `compact_mrules` has run — see this module's top doc for why `Grammar::morphemes` itself is
/// never compacted (only its `co_occurrence` contents are filtered here). Must run *after*
/// `compact_mrules`, since "reachable" is defined in terms of the already-compacted
/// `grammar.mrules`/`grammar.entries`.
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
