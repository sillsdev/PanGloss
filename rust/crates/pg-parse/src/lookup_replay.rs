//! Rejects a lexical-lookup alternative before synthesis when a rule on its trail can never pass its stem-name or MPR gate.

use std::sync::atomic::{AtomicU64, Ordering};

use pg_grammar_model::model::{
    mpr_excluded_ok, mpr_required_ok, AffixProcessRuleDef, AllomorphId, AllomorphOwner, Grammar,
    MorphRuleDef, MprGroupOutput, MprSet,
};
use pg_rules::word::Word;

/// What the grammar's rules can add to a word's MPR set beyond the root's, computed once from the grammar and never from a word.
pub(crate) struct LookupReplay {
    /// False when no rule has a stem-name or MPR gate, so no alternative can be rejected.
    checks_mpr: bool,
    /// Outputs of realizational rules, which are not on the trail and may fire anywhere.
    realizational_out: MprSet,
    /// Outputs of every compounding rule.
    compounding_out: MprSet,
    /// Whether some output can remove another feature through an overwrite group.
    overwrite_reachable: bool,
    /// One unit per feature-set match, union or copy; diagnostic only.
    work: AtomicU64,
}

impl LookupReplay {
    pub(crate) fn build(g: &Grammar) -> Self {
        let mut checks_mpr = false;
        let mut all_out = MprSet::EMPTY;
        let mut realizational_out = MprSet::EMPTY;
        let mut compounding_out = MprSet::EMPTY;
        for rule in &g.mrules {
            match rule {
                MorphRuleDef::AffixProcess(rule) => {
                    checks_mpr |= rule.required_stem_name.is_some();
                    for allo in &rule.allomorphs {
                        checks_mpr |=
                            !allo.required_mpr.is_empty() || !allo.excluded_mpr.is_empty();
                        all_out = all_out.union(allo.out_mpr);
                    }
                }
                MorphRuleDef::Realizational(rule) => {
                    for allo in &rule.allomorphs {
                        realizational_out = realizational_out.union(allo.out_mpr);
                        all_out = all_out.union(allo.out_mpr);
                    }
                }
                MorphRuleDef::Compounding(rule) => {
                    compounding_out = compounding_out.union(rule.output_prod_restrictions_mpr);
                    for sub in &rule.subrules {
                        compounding_out = compounding_out.union(sub.out_mpr);
                    }
                    all_out = all_out.union(compounding_out);
                }
            }
        }
        let overwrite_reachable = g.mpr_groups.iter().any(|group| {
            group.output == MprGroupOutput::Overwrite && group.members.overlaps(all_out)
        });
        LookupReplay {
            checks_mpr,
            realizational_out,
            compounding_out,
            overwrite_reachable,
            work: AtomicU64::new(0),
        }
    }

    pub(crate) fn work(&self) -> u64 {
        self.work.load(Ordering::Relaxed)
    }

    /// False only when no synthesis of `alt` can apply every trail rule. A root in a family is never rejected, because stem choice and blocking can replace it.
    pub(crate) fn is_feasible(&self, g: &Grammar, alt: &Word) -> bool {
        if !self.checks_mpr {
            return true;
        }
        let Some(root_id) = alt.root_allomorph else {
            return true;
        };
        if root_id == AllomorphId::GUESSED || alt.root_runtime_id.is_some() {
            return true;
        }
        let Some(&AllomorphOwner::Root(le, idx)) = g.allomorph_owners.get(root_id.0 as usize)
        else {
            return true;
        };
        let entry = &g.entries[le.0 as usize];
        if entry.family.is_some() {
            return true;
        }
        let mut work = 0;
        let feasible = self.trail_is_feasible(
            g,
            alt,
            entry.mpr,
            entry.allomorphs[idx as usize].stem_name,
            &mut work,
        );
        if work > 0 {
            self.work.fetch_add(work, Ordering::Relaxed);
        }
        feasible
    }

    /// Required gates are checked against a superset of the real MPR set and excluded gates against a subset, so a failure is a failure of the real set.
    fn trail_is_feasible(
        &self,
        g: &Grammar,
        alt: &Word,
        root_mpr: MprSet,
        root_stem: Option<pg_grammar_model::model::StemNameId>,
        work: &mut u64,
    ) -> bool {
        let mut extra = self.realizational_out;
        let mut excluded_ok = !self.overwrite_reachable;
        let mut stem_ok = true;
        for entry in alt.mrule_apps.iter().rev() {
            let rule = entry.and_then(|id| match &g.mrules[id.0 as usize] {
                MorphRuleDef::AffixProcess(rule) => Some(rule),
                _ => None,
            });
            let Some(rule) = rule else {
                if !self.compounding_out.is_empty() {
                    *work += 1;
                    extra = extra.union(self.compounding_out);
                }
                excluded_ok = false;
                stem_ok = false;
                continue;
            };
            if stem_ok && rule.required_stem_name.is_some() && rule.required_stem_name != root_stem
            {
                return false;
            }
            if !any_allomorph_possible(g, rule, root_mpr, extra, excluded_ok, work) {
                return false;
            }
            for allo in &rule.allomorphs {
                if !allo.out_mpr.is_empty() {
                    *work += 1;
                    extra = extra.union(allo.out_mpr);
                }
            }
        }
        true
    }
}

fn any_allomorph_possible(
    g: &Grammar,
    rule: &AffixProcessRuleDef,
    root_mpr: MprSet,
    extra: MprSet,
    excluded_ok: bool,
    work: &mut u64,
) -> bool {
    let upper = root_mpr.union(extra);
    rule.allomorphs.iter().any(|allo| {
        if !allo.required_mpr.is_empty() {
            *work += 1;
            if !mpr_required_ok(&g.mpr_groups, allo.required_mpr, upper) {
                return false;
            }
        }
        if !excluded_ok || allo.excluded_mpr.is_empty() {
            return true;
        }
        *work += 1;
        mpr_excluded_ok(&g.mpr_groups, allo.excluded_mpr, root_mpr)
    })
}
