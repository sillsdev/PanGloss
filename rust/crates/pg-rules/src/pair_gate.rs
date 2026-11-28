//! Adjacent affix rules whose required-feature gate can never pass against what the inner rule leaves behind (issue #584).
//!
//! Analysis unapplies the outer rule first, so a rule `O` already on a trail is followed in synthesis by the next rule `I` unapplied. Synthesis applies `I`, which writes its allomorph's `out_mpr` through an overwrite group (`mpr_add_output`), and then tests `O`'s required features against that word. For a group `I` always overwrites, the word's members of the group after `I` are exactly the members `I`'s allomorph outputs, whatever the root is. If `O`'s required members in that group cannot be met by anything `I` outputs, `O` fails its gate in every synthesis of every candidate that has `I` directly inside `O`. The pair is dead before any root is known.
//!
//! Only the failing direction is proved: a pair is dead when some single group fails for every allomorph of `O`. Anything this cannot decide (an ungrouped feature, a group `I` does not always overwrite, a group a realizational rule also writes, a template rule) leaves the pair alive. The final lookup check (`pg_parse` `lookup_replay`) and synthesis still run on every surviving candidate, so this only removes candidates they would reject.

use pg_grammar_model::model::{
    mpr_required_ok, AffixProcessRuleDef, Grammar, MRuleId, MorphRuleDef, MprGroupOutput, MprSet,
};

/// Dead `(outer, inner)` pairs, by outer rule id; built once from the grammar.
#[derive(Default)]
pub struct MprDeadPairs {
    /// Indexed by `MRuleId.0`; `Some` for an outer rule with at least one dead inner rule, holding one bit per inner rule id.
    rows: Vec<Option<Vec<u64>>>,
    /// Number of dead pairs, for diagnostics and tests.
    pairs: usize,
}

impl MprDeadPairs {
    pub fn build(g: &Grammar) -> Self {
        let n = g.mrules.len();
        let mut rows: Vec<Option<Vec<u64>>> = vec![None; n];
        let mut pairs = 0;
        // A group whose members no other group shares: its bucket in a required set is exactly the required members in it.
        let groups: Vec<usize> = (0..g.mpr_groups.len())
            .filter(|&i| {
                g.mpr_groups[i].output == MprGroupOutput::Overwrite
                    && g.mpr_groups.iter().enumerate().all(|(j, other)| {
                        i == j || !other.members.overlaps(&g.mpr_groups[i].members)
                    })
            })
            .collect();
        if groups.is_empty() {
            return Self { rows, pairs };
        }
        // Rules whose output a realizational rule or compounding rule might also write are not trusted for those members.
        let mut elsewhere = MprSet::EMPTY;
        for rule in &g.mrules {
            match rule {
                MorphRuleDef::Realizational(r) => {
                    for a in &r.allomorphs {
                        elsewhere = elsewhere.union(&a.out_mpr);
                    }
                }
                MorphRuleDef::Compounding(c) => {
                    elsewhere = elsewhere.union(&c.output_prod_restrictions_mpr);
                    for s in &c.subrules {
                        elsewhere = elsewhere.union(&s.out_mpr);
                    }
                }
                MorphRuleDef::AffixProcess(_) => {}
            }
        }
        let groups: Vec<usize> = groups
            .into_iter()
            .filter(|&i| !g.mpr_groups[i].members.overlaps(&elsewhere))
            .collect();
        for (o, outer) in g.mrules.iter().enumerate() {
            let Some(outer) = eligible(g, outer) else {
                continue;
            };
            if outer.allomorphs.iter().any(|a| a.required_mpr.is_empty()) {
                continue;
            }
            for (i, inner) in g.mrules.iter().enumerate() {
                let Some(inner) = eligible(g, inner) else {
                    continue;
                };
                let dead = groups.iter().any(|&gi| {
                    let members = &g.mpr_groups[gi].members;
                    // The group is certainly overwritten by whichever allomorph of the inner rule applies.
                    if inner
                        .allomorphs
                        .iter()
                        .any(|a| !a.out_mpr.overlaps(members))
                    {
                        return false;
                    }
                    let left = inner
                        .allomorphs
                        .iter()
                        .fold(MprSet::EMPTY, |acc, a| acc.union(&a.out_mpr))
                        .intersection(members);
                    outer.allomorphs.iter().all(|a| {
                        let need = a.required_mpr.intersection(members);
                        !need.is_empty() && !mpr_required_ok(&g.mpr_groups, &need, &left)
                    })
                });
                if dead {
                    let row = rows[o].get_or_insert_with(|| vec![0; n.div_ceil(64)]);
                    row[i / 64] |= 1 << (i % 64);
                    pairs += 1;
                }
            }
        }
        Self { rows, pairs }
    }

    /// Number of dead pairs.
    pub fn len(&self) -> usize {
        self.pairs
    }

    pub fn is_empty(&self) -> bool {
        self.pairs == 0
    }

    /// Whether `inner` unapplied directly after `outer` can never synthesise.
    #[inline]
    pub fn is_dead(&self, outer: MRuleId, inner: MRuleId) -> bool {
        self.rows
            .get(outer.0 as usize)
            .and_then(Option::as_ref)
            .is_some_and(|row| row[inner.0 as usize / 64] & (1 << (inner.0 % 64)) != 0)
    }
}

/// An ordinary affix rule: template-slot and clitic rules interleave with other rules in ways the trail does not show.
fn eligible<'a>(g: &Grammar, rule: &'a MorphRuleDef) -> Option<&'a AffixProcessRuleDef> {
    match rule {
        MorphRuleDef::AffixProcess(r)
            if !r.is_template_rule && !r.allomorphs.is_empty() && !g.affix_rule_is_clitic(r) =>
        {
            Some(r)
        }
        _ => None,
    }
}
