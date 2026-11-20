//! Pinning a morpheme sequence for a restricted reparse: which entries and rules a candidate
//! names, and the parse that admits only those. Confirmation (`pg-foma-runtime`) and why-not both
//! read this one resolution, so they cannot disagree about what a pin means.

use pg_grammar_model::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId};
use pg_rules::stratum::RuleRef;
use pg_rules::trace::TraceSink;
use rustc_hash::FxHashSet as HashSet;

use crate::{Morpher, ParseOptions, ParseOutcome};

/// Which grammar object owns a given `MorphemeId`. A compounding rule never owns a morpheme, so it
/// is never the `MRule` variant.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MorphemeOwner {
    LexEntry(LexEntryId),
    MRule(MRuleId),
}

/// The owner of every morpheme, indexed by `MorphemeId`.
pub fn build_morpheme_owners(g: &Grammar) -> Vec<Option<MorphemeOwner>> {
    let mut owners = vec![None; g.morphemes.len()];
    for (i, e) in g.entries.iter().enumerate() {
        owners[e.morpheme.0 as usize] = Some(MorphemeOwner::LexEntry(LexEntryId(i as u32)));
    }
    for (i, r) in g.mrules.iter().enumerate() {
        let morpheme = match r {
            MorphRuleDef::AffixProcess(def) => Some(def.morpheme),
            MorphRuleDef::Realizational(def) => Some(def.morpheme),
            MorphRuleDef::Compounding(_) => None,
        };
        if let Some(m) = morpheme {
            owners[m.0 as usize] = Some(MorphemeOwner::MRule(MRuleId(i as u32)));
        }
    }
    owners
}

fn owner_of(owners: &[Option<MorphemeOwner>], m: MorphemeId) -> Option<MorphemeOwner> {
    owners.get(m.0 as usize).copied().flatten()
}

/// One candidate's resolved pins: root entry, non-root rule set, extra compound roots.
pub struct CandidatePins {
    pub root_entry: LexEntryId,
    pub rules: HashSet<MRuleId>,
    pub extra_roots: HashSet<LexEntryId>,
}

/// `None` when `root_index` is out of range, the root is not a lexical entry, or a non-root
/// morpheme is unowned.
pub fn resolve_pins(
    owners: &[Option<MorphemeOwner>],
    morphemes: &[MorphemeId],
    root_index: i32,
) -> Option<CandidatePins> {
    if root_index < 0 || root_index as usize >= morphemes.len() {
        return None;
    }
    let root_index = root_index as usize;
    let root_entry = match owner_of(owners, morphemes[root_index]) {
        Some(MorphemeOwner::LexEntry(le)) => le,
        _ => return None,
    };
    let mut rules: HashSet<MRuleId> = HashSet::default();
    let mut extra_roots: HashSet<LexEntryId> = HashSet::default();
    for (i, &m) in morphemes.iter().enumerate() {
        if i == root_index {
            continue;
        }
        match owner_of(owners, m) {
            Some(MorphemeOwner::LexEntry(le)) => {
                extra_roots.insert(le);
            }
            Some(MorphemeOwner::MRule(mid)) => {
                rules.insert(mid);
            }
            None => return None,
        }
    }
    Some(CandidatePins {
        root_entry,
        rules,
        extra_roots,
    })
}

/// Strata and templates always run; a compounding rule is always admitted; an affix rule only when pinned.
pub fn rule_is_allowed(g: &Grammar, rules: &HashSet<MRuleId>, rule: RuleRef) -> bool {
    match rule {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => {
            rules.contains(&id) || matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_))
        }
    }
}

/// The restricted reparse for exactly one candidate's pins (root set plus exact rule set, no
/// slack), traced so a caller can classify why it produced no match.
pub fn parse_word_pinned_traced(
    g: &Grammar,
    morpher: &Morpher,
    pins: &CandidatePins,
    word: &str,
    trace: &dyn TraceSink,
) -> ParseOutcome {
    let lex_entry_filter = |le: LexEntryId| le == pins.root_entry || pins.extra_roots.contains(&le);
    let rule_filter = |r: RuleRef| rule_is_allowed(g, &pins.rules, r);
    morpher.parse_word_selected_traced(
        word,
        &ParseOptions::default(),
        trace,
        Some(&lex_entry_filter),
        Some(&rule_filter),
    )
}
