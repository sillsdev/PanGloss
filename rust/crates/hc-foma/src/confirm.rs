//! Fresh port of `hc-hybrid/src/replay.rs`'s confirm half (module doc there, `replay.rs:1-49`,
//! esp. the quirk-8 `RuleRef` mapping) — attribution comments throughout, NO dependency on
//! `hc-hybrid` (that crate is being sunset, plan D8). Adapted to this crate's own
//! [`crate::tags::Candidate`] (the FST-tag-decoded candidate shape, plan D2 — `MorphemeId`
//! sequence + `root_index`, not `hc-hybrid/src/walk.rs`'s own candidate type) and to plan D4's
//! multiplicity recovery: [`confirm_all`] collects EVERY matching analysis the pinned
//! `parse_word_selected` outcome contains, not just the first the way the original's `confirm`
//! (`replay.rs:118-192`, `.find()`) did — the engine returns a genuine multiset (Sena `mbali`: 8),
//! and D4 requires restoring it rather than silently collapsing to one hit per candidate.
//!
//! Each collected match is paired with its own `(morpheme-join, surface)` display-string pair —
//! `hc_parse::ParseOutcome::analyses[i]` is, by that struct's own doc (`hc-parse/src/morpher.rs:
//! 79-120`), built from the exact same traversal as `structured[i]` and shares its index, so
//! zipping the two `Vec`s together before filtering (rather than re-deriving the strings some other
//! way afterward) is what keeps a matched analysis's numeric ids and display strings describing the
//! same thing.

use rustc_hash::FxHashSet as HashSet;

use hc_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId};
use hc_parse::{Morpher, ParseOptions, WordAnalysis as EngineAnalysis};
use hc_rules::stratum::RuleRef;

use crate::tags::Candidate;

/// Which grammar object owns a given [`MorphemeId`] — ported from `hc-hybrid/src/replay.rs`'s
/// `MorphemeOwner` (`replay.rs:70-74`) verbatim. See that module's doc for the full quirk-8
/// rationale (why a `CompoundingRule` never owns a morpheme and so is never this enum's `MRule`
/// variant).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MorphemeOwner {
    LexEntry(LexEntryId),
    MRule(MRuleId),
}

/// `replay.rs::build_morpheme_owners` (`replay.rs:82-98`), ported verbatim onto this crate's own
/// `Grammar`/`MorphemeId` types (same crate, `hc-grammar`, as the original — no adaptation needed
/// beyond the module it lives in).
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

/// `replay.rs::analyses_match` (`replay.rs:200-208`): positional identity comparison, ported
/// verbatim except for the candidate type (this crate's [`Candidate`] instead of `hc-hybrid`'s).
/// Plan §2's "positional match trap" — element-wise, not set-wise; morphemes in the wrong order or
/// wrong `root_index` is a silent loss, never a false negative match.
fn analyses_match(wa: &EngineAnalysis, candidate: &Candidate) -> bool {
    wa.root_morpheme_index == candidate.root_index
        && wa.morpheme_ids.len() == candidate.morphemes.len()
        && wa
            .morpheme_ids
            .iter()
            .zip(candidate.morphemes.iter())
            .all(|(&a, &b)| a == b.0)
}

/// D4 multiplicity recovery over `replay.rs::confirm`'s lex_entry_filter/rule_filter construction
/// (`replay.rs:118-192`; quirk-8 mapping in that module's own doc — `Stratum`/`Template` always
/// admitted, an `MRule` admitted iff it's one of the candidate's own rules or a `Compounding` rule
/// with extra roots present). `morpher` MUST be built uncapped (`Morpher::new(g, usize::MAX)`,
/// `replay.rs:106-110`'s rationale carries over unchanged: a Rust-side cap here could silently drop
/// a result the full engine would find, which would look like a parity bug rather than the
/// deliberate absence of a work budget it actually is).
///
/// Returns every matching `(engine analysis, morpheme-join string, surface string)` triple in the
/// pinned outcome's own order — empty (never panics) when the candidate's root position isn't a
/// `LexEntry`, when any non-root morpheme resolves to neither a `LexEntry` nor an `MRule`, or when
/// the restricted re-analysis simply confirms nothing.
pub fn confirm_all(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidate: &Candidate,
    word: &str,
) -> Vec<(EngineAnalysis, String, String)> {
    if candidate.root_index < 0 || candidate.root_index as usize >= candidate.morphemes.len() {
        return Vec::new();
    }
    let root_index = candidate.root_index as usize;
    let root_entry = match owner_of(owners, candidate.morphemes[root_index]) {
        Some(MorphemeOwner::LexEntry(le)) => le,
        _ => return Vec::new(), // replay.rs:38-41 — the designated root must be a LexEntry.
    };

    let mut rules: HashSet<MRuleId> = HashSet::default();
    let mut extra_roots: HashSet<LexEntryId> = HashSet::default();
    for (i, &m) in candidate.morphemes.iter().enumerate() {
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
            None => return Vec::new(), // replay.rs:56-59 — neither a LexEntry nor a rule -> None.
        }
    }

    let lex_entry_filter = |le: LexEntryId| le == root_entry || extra_roots.contains(&le);
    let rule_filter = |r: RuleRef| match r {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => {
            rules.contains(&id)
                || (!extra_roots.is_empty()
                    && matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_)))
        }
    };

    let outcome = morpher.parse_word_selected(
        word,
        &ParseOptions::default(),
        Some(&lex_entry_filter),
        Some(&rule_filter),
    );

    // `outcome.analyses[i]` and `outcome.structured[i]` describe the SAME analysis (ParseOutcome's
    // own doc, `hc-parse/src/morpher.rs:79-120`) — zip before filtering so a match keeps both.
    outcome
        .structured
        .into_iter()
        .zip(outcome.analyses)
        .filter(|(wa, _)| analyses_match(wa, candidate))
        .map(|(wa, (join, surface))| (wa, join, surface))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_grammar::model::MorphemeId as Mid;

    fn sample_path(name: &str) -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn load_indonesian() -> Option<Grammar> {
        let path = sample_path("indonesian-hc.xml")?;
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
    }

    /// A bare-root word ("ajar", per `replay.rs`'s own equivalent test) confirms to a non-empty set
    /// of matches, all sharing the expected root entry.
    #[test]
    fn confirm_bare_root_word_verifies() {
        let Some(g) = load_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let morpher = Morpher::new(&g, usize::MAX);
        let owners = build_morpheme_owners(&g);
        // "ajar" is a bare root (entry25/entry26 homograph, per replay.rs's own fixture comment) —
        // find its morpheme id from the grammar itself rather than hard-coding one that might drift.
        let entry = g
            .entries
            .iter()
            .enumerate()
            .find(|(_, e)| {
                g.morphemes[e.morpheme.0 as usize].xml_key == "entry25"
                    || g.morphemes[e.morpheme.0 as usize].xml_key == "entry26"
            })
            .map(|(i, e)| (i, e.morpheme));
        let Some((_idx, morpheme)) = entry else {
            eprintln!("skipping: entry25/entry26 not found in indonesian-hc.xml");
            return;
        };
        let candidate = Candidate {
            morphemes: vec![morpheme],
            root_index: 0,
        };
        let matches = confirm_all(&g, &owners, &morpher, &candidate, "ajar");
        assert!(!matches.is_empty(), "\"ajar\" must confirm to at least one analysis");
        for (wa, _, _) in &matches {
            assert_eq!(wa.root_morpheme_index, 0);
            assert_eq!(wa.morpheme_ids, vec![morpheme.0]);
        }
    }

    /// A candidate whose designated "root" position is out of range (or empty) must confirm to
    /// nothing, never panic.
    #[test]
    fn confirm_rejects_out_of_range_root_index() {
        let Some(g) = load_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let morpher = Morpher::new(&g, usize::MAX);
        let owners = build_morpheme_owners(&g);
        let bogus = Candidate {
            morphemes: vec![],
            root_index: 0,
        };
        assert!(confirm_all(&g, &owners, &morpher, &bogus, "ajar").is_empty());
    }

    /// A non-root morpheme id that resolves to neither a `LexEntry` nor an `MRule` (e.g. a
    /// `MorphemeId` that doesn't exist in this grammar at all) must confirm to nothing.
    #[test]
    fn confirm_rejects_unowned_non_root_morpheme() {
        let Some(g) = load_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let morpher = Morpher::new(&g, usize::MAX);
        let owners = build_morpheme_owners(&g);
        let root = g.entries[0].morpheme;
        let candidate = Candidate {
            morphemes: vec![root, Mid(u32::MAX - 5)],
            root_index: 0,
        };
        assert!(confirm_all(&g, &owners, &morpher, &candidate, "ajar").is_empty());
    }
}
