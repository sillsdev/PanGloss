//! A static proof that a template cannot produce a wanted set of affix morphemes.

use std::collections::BTreeSet;

use pg_grammar_model::model::{Grammar, MRuleId, MorphemeId, TemplateId};

/// The first required slot of `tid` none of whose rules belongs to `morphemes`; each applied rule adds its own morpheme, so an output through `tid` carries one not wanted.
pub fn unfillable_required_slot(
    g: &Grammar,
    tid: TemplateId,
    morphemes: &BTreeSet<MorphemeId>,
) -> Option<usize> {
    let template = g.templates.get(tid.0 as usize)?;
    template.slots.iter().position(|slot| {
        !slot.optional
            && !slot.rules.is_empty()
            && slot.rules.iter().all(|rule| {
                g.mrules
                    .get(rule.0 as usize)
                    .and_then(|def| def.morpheme())
                    .is_some_and(|morpheme| !morphemes.contains(&morpheme))
            })
    })
}

/// Every template holding a rule of `morphemes`, or `None` unless each has a rule and every rule sits in a template slot and in no stratum rule list.
pub fn hosting_templates(
    g: &Grammar,
    morphemes: &BTreeSet<MorphemeId>,
) -> Option<BTreeSet<TemplateId>> {
    let mut with_rule = BTreeSet::new();
    let mut hosts = BTreeSet::new();
    for (index, def) in g.mrules.iter().enumerate() {
        let Some(morpheme) = def.morpheme().filter(|m| morphemes.contains(m)) else {
            continue;
        };
        let rule = MRuleId(index as u32);
        if g.strata
            .iter()
            .any(|stratum| stratum.mrules.contains(&rule))
        {
            return None;
        }
        let mut placed = false;
        for (tid, template) in g.templates.iter().enumerate() {
            if template.slots.iter().any(|slot| slot.rules.contains(&rule)) {
                hosts.insert(TemplateId(tid as u32));
                placed = true;
            }
        }
        if !placed {
            return None;
        }
        with_rule.insert(morpheme);
    }
    (with_rule.len() == morphemes.len()).then_some(hosts)
}

/// True when the rules of `morphemes` apply only inside templates and every such template has a required slot no wanted morpheme fills.
pub fn target_unreachable(g: &Grammar, morphemes: &BTreeSet<MorphemeId>) -> bool {
    hosting_templates(g, morphemes).is_some_and(|hosts| {
        !hosts.is_empty()
            && hosts
                .iter()
                .all(|&host| unfillable_required_slot(g, host, morphemes).is_some())
    })
}

#[cfg(test)]
mod tests;
