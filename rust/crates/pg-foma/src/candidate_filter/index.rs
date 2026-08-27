//! The immutable grammar-derived facts the structural passes decide against.
//!
//! Built once per grammar and self-contained afterwards: it copies what it needs out of the
//! grammar and out of [`MorphotacticIndex`](crate::morphotactics::MorphotacticIndex) at build
//! time and borrows neither. That is what lets one index be shared by every pass, cached beside a
//! compiled grammar, and eventually serialized — none of which a structure holding a reference
//! into a `Grammar` can do.
//!
//! It also owns the provenance mapping for the contract's slot and stratum identities. Those are
//! filter-contract identities, not grammar table indices, so this is the one place that says which
//! `(template, slot)` site a [`TraceSlotId`] means; a producer that wants to emit one asks here
//! rather than reusing an ordinal that happens to line up.
//!
//! Nothing here decides anything. Every query answers with what the grammar establishes or with an
//! explicit "not known here", and turning that into a decision is a pass's job.

use std::collections::BTreeMap;

use pg_grammar::model::{Grammar, MRuleId, MorphRuleDef, MorphemeId};

use crate::candidate_filter::model::{TraceSlotId, TraceStratumId};
use crate::confirm::{build_morpheme_owners, resolve_pins, MorphemeOwner};
use crate::morphotactics::MorphotacticIndex;
use crate::tags::Candidate;

/// What kind of rule owns a morpheme, enumerated rather than matched with a wildcard so that a new
/// `MorphRuleDef` variant is a compile error here instead of a silent classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleShape {
    /// An affix process rule, which owns the morpheme it introduces.
    AffixProcess,
    /// A realizational rule, which likewise owns its morpheme.
    Realizational,
    /// A compounding rule, which owns no morpheme of its own.
    Compounding,
}

impl RuleShape {
    fn of(rule: &MorphRuleDef) -> Self {
        match rule {
            MorphRuleDef::AffixProcess(_) => Self::AffixProcess,
            MorphRuleDef::Realizational(_) => Self::Realizational,
            MorphRuleDef::Compounding(_) => Self::Compounding,
        }
    }
}

/// Whether a slot is one a rule may fire at.
///
/// The third answer is load-bearing: an established grammar fact and the absence of one are
/// different things, and only the first may ever contribute to a rejection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SiteVerdict {
    Admits,
    Refuses,
    Unknown,
}

/// What the index knows about one `(template, slot)` site.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SlotFacts {
    /// `None` when no stratum, or more than one, declares the owning template.
    stratum: Option<TraceStratumId>,
    /// The rules the site lists, sorted so a lookup is a binary search and a report is stable.
    rules: Vec<MRuleId>,
}

/// Immutable grammar facts, owned outright.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterIndex {
    owners: Vec<Option<MorphemeOwner>>,
    rule_shapes: Vec<RuleShape>,
    slots: Vec<SlotFacts>,
    sites: BTreeMap<(u16, u8), TraceSlotId>,
    strata: u8,
}

impl FilterIndex {
    /// Derives every fact the structural passes need, once, from one grammar.
    pub fn build(grammar: &Grammar) -> Self {
        let morphotactics = MorphotacticIndex::build(grammar);
        let owners = build_morpheme_owners(grammar);
        let rule_shapes: Vec<RuleShape> = grammar.mrules.iter().map(RuleShape::of).collect();

        let mut declaring: Vec<Vec<u8>> = vec![Vec::new(); grammar.templates.len()];
        for (stratum, def) in grammar.strata.iter().enumerate() {
            let Ok(stratum) = u8::try_from(stratum) else {
                continue;
            };
            for template in &def.templates {
                if let Some(entry) = declaring.get_mut(template.0 as usize) {
                    entry.push(stratum);
                }
            }
        }

        let mut rules_at: BTreeMap<(u16, u8), Vec<MRuleId>> = BTreeMap::new();
        for rule in 0..grammar.mrules.len() {
            let rule = MRuleId(rule as u32);
            for &site in morphotactics.slot_sites_of(rule) {
                rules_at.entry(site).or_default().push(rule);
            }
        }

        let mut slots: Vec<SlotFacts> = Vec::new();
        let mut sites: BTreeMap<(u16, u8), TraceSlotId> = BTreeMap::new();
        for (template, def) in grammar.templates.iter().enumerate() {
            let Ok(template) = u16::try_from(template) else {
                continue;
            };
            let stratum = single(declaring.get(usize::from(template)).map(Vec::as_slice))
                .and_then(|stratum| stratum_id_of(&morphotactics, template, stratum));
            for slot in 0..def.slots.len() {
                let Ok(slot) = u8::try_from(slot) else {
                    continue;
                };
                let site = (template, slot);
                let mut rules = rules_at.remove(&site).unwrap_or_default();
                rules.sort_unstable();
                rules.dedup();
                sites.insert(site, TraceSlotId(slots.len() as u32));
                slots.push(SlotFacts {
                    stratum,
                    rules,
                });
            }
        }

        Self {
            owners,
            rule_shapes,
            slots,
            sites,
            strata: u8::try_from(grammar.strata.len()).unwrap_or(u8::MAX),
        }
    }

    /// The grammar object that owns `morpheme`, or `None` when nothing does.
    pub fn morpheme_owner(&self, morpheme: MorphemeId) -> Option<MorphemeOwner> {
        self.owners.get(morpheme.0 as usize).copied().flatten()
    }

    /// Whether HC's own pin resolution accepts this identity.
    ///
    /// A candidate it refuses never reaches a restricted reparse, so nothing HC would have
    /// confirmed is lost by rejecting one.
    pub fn pins_resolve(&self, identity: &Candidate) -> bool {
        resolve_pins(&self.owners, identity).is_some()
    }

    /// The contract slot identity of one grammar site, which is how a producer names it.
    pub fn slot_id(&self, template: u16, slot: u8) -> Option<TraceSlotId> {
        self.sites.get(&(template, slot)).copied()
    }

    /// The contract stratum identity of one grammar stratum.
    pub fn stratum_id(&self, stratum: u8) -> Option<TraceStratumId> {
        (stratum < self.strata).then_some(TraceStratumId(u32::from(stratum)))
    }

    /// The stratum whose own templates include this slot's, when exactly one does.
    pub fn slot_stratum(&self, slot: TraceSlotId) -> Option<TraceStratumId> {
        self.facts(slot).and_then(|facts| facts.stratum)
    }

    /// Whether the slot's own rule list names `rule`.
    pub fn slot_admits(&self, slot: TraceSlotId, rule: MRuleId) -> SiteVerdict {
        match self.facts(slot) {
            None => SiteVerdict::Unknown,
            Some(facts) if facts.rules.binary_search(&rule).is_ok() => SiteVerdict::Admits,
            Some(_) => SiteVerdict::Refuses,
        }
    }

    /// What kind of rule this is, or `None` for a rule the grammar does not define.
    pub fn rule_shape(&self, rule: MRuleId) -> Option<RuleShape> {
        self.rule_shapes.get(rule.0 as usize).copied()
    }

    fn facts(&self, slot: TraceSlotId) -> Option<&SlotFacts> {
        self.slots.get(slot.0 as usize)
    }
}

/// The one element of a slice, or `None` for none and for several.
fn single(values: Option<&[u8]>) -> Option<u8> {
    match values {
        Some([only]) => Some(*only),
        _ => None,
    }
}

/// The template's stratum, taken from the morphotactic authority and cross-checked against it.
fn stratum_id_of(
    morphotactics: &MorphotacticIndex,
    template: u16,
    declaring: u8,
) -> Option<TraceStratumId> {
    let owning = morphotactics.template_stratum(template)?;
    (owning == declaring).then_some(TraceStratumId(u32::from(declaring)))
}
