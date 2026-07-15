//! Sub-project 2 (`PanGloss-demo` repo, `docs/superpowers/specs/2026-07-14-add-to-dictionary-
//! and-realize-inference-design.md`, "Sub-project 2: RealizeMap inference"): built-in English
//! gloss-alias inference, so a grammar with no `realize.toml` sidecar (`crate::map`'s
//! `RealizeMap::empty()` path) still gets natural-ish phrases out of the box instead of every
//! affix falling straight through to [`crate::ir::GlossIr::extras`].
//!
//! [`infer_english`] builds a [`RealizeMap`] directly from a grammar's affix-morpheme gloss
//! strings — no sidecar file, no grammar-specific tuning. Token normalization: lowercase, then
//! split on `.`, `-`, `_`, `:`; the resulting token set is matched against a small built-in
//! alias table (Num/Poss/Case, below). Only glosses actually present in the input iterator get
//! an entry (mirrors `RealizeMap::parse`'s "only what's in the file" shape) — an unmatched gloss
//! is simply absent, never guessed. **A wrong phrase is worse than an honest residue**: the
//! design's guiding rule for this whole sub-project, and the reason this table stays narrow and
//! declines to match ambiguous or partial tokens rather than reaching for a "close enough" guess.
//!
//! Alias table:
//! - **Num**: `pl`/`plur`/`plural` -> `Num::Pl`; `sg`/`sing`/`singular` -> `Num::Sg`.
//! - **Poss**: only recognized when a `poss`/`gen` token co-occurs (in the same normalized
//!   token set) with a person+number token: `1sg`->`P1Sg`, `1pl`->`P1Pl`, `2sg`->`P2SgM`,
//!   `2pl`->`P2Pl`, `3sg`->`P3SgM`, `3pl`->`P3Pl`. A bare `1sg`/`3` with no `poss`/`gen` token
//!   alongside it is NOT possessive marking as far as this table is concerned (could just as
//!   well be subject agreement, an object index, etc.) — no entry.
//!
//!   **Known limitation** (call this out to callers/grammar authors): the alias table carries
//!   no gender information, so 2nd/3rd person singular possessives default to the masculine
//!   variant (`2sg`->`P2SgM`, `3sg`->`P3SgM`) — there's no generic `2sg`/`3sg`-only signal to
//!   split masculine from feminine (contrast a grammar with its own gendered gloss inventory,
//!   e.g. amharic's `poss.2m`/`poss.2f`, which a sidecar `realize.toml` can still map precisely).
//!   A grammar that needs the feminine variant corrects this per gloss key via a sidecar and
//!   [`RealizeMap::extend_overriding`] — sidecar wins, this inferred base is just a starting
//!   point.
//! - **Case**: `loc`/`locative` -> `Case::Loc`; `abl`/`ablative` -> `Case::Abl`; `all`/
//!   `allative` -> `Case::All`.
//!
//! Anything unmatched (`appl`, `caus`, a bare `1sg` with no `poss`/`gen` token, a bare `3`, ...)
//! gets no entry at all — it falls through to `extras` at `crate::ir::to_ir` time, same as any
//! other unmapped gloss.
#![forbid(unsafe_code)]

use crate::ir::{CaseRole, Num, Poss};
use crate::map::{FeatureAssignment, RealizeMap};

/// Build a [`RealizeMap`] by matching each gloss in `glosses` against the built-in English
/// alias table (module docs above). `glosses` is expected to be a grammar's affix-morpheme
/// gloss strings (e.g. iterated from `Grammar::morphemes`); only glosses that actually match an
/// alias produce an entry, keyed by the exact (un-normalized) string as given by the iterator —
/// the same key shape [`crate::map::RealizeMap::lookup`] is called with from
/// [`crate::ir::to_ir`] (a token's raw `gloss` string, verbatim).
pub fn infer_english<'a>(glosses: impl Iterator<Item = &'a str>) -> RealizeMap {
    let mut map = RealizeMap::empty();
    for gloss in glosses {
        if let Some(assignment) = infer_one(gloss) {
            map.insert(gloss.to_string(), assignment);
        }
    }
    map
}

/// Normalize one raw gloss string into its token set: lowercase, then split on `.`, `-`, `_`,
/// `:`, dropping empty pieces (consecutive delimiters, leading/trailing delimiters).
fn normalize_tokens(gloss: &str) -> Vec<String> {
    gloss
        .to_lowercase()
        .split(['.', '-', '_', ':'])
        .filter(|piece| !piece.is_empty())
        .map(|piece| piece.to_string())
        .collect()
}

/// Person+number tokens the Poss branch recognizes, paired with the (gender-defaulted-masculine
/// for 2sg/3sg, per the module docs' known limitation) `Poss` variant each maps to.
const POSS_PERSON_NUMBER: &[(&str, Poss)] = &[
    ("1sg", Poss::P1Sg),
    ("1pl", Poss::P1Pl),
    ("2sg", Poss::P2SgM),
    ("2pl", Poss::P2Pl),
    ("3sg", Poss::P3SgM),
    ("3pl", Poss::P3Pl),
];

/// Match one normalized gloss's token set against the alias table, in Num -> Poss -> Case
/// priority order (the categories are disjoint in every realistic gloss inventory; the order
/// only matters for a defensive, never-expected overlap). `None` means no alias matched -- the
/// caller leaves this gloss with no entry at all.
fn infer_one(gloss: &str) -> Option<FeatureAssignment> {
    let tokens = normalize_tokens(gloss);
    let has_token = |candidates: &[&str]| tokens.iter().any(|t| candidates.contains(&t.as_str()));

    if has_token(&["pl", "plur", "plural"]) {
        return Some(FeatureAssignment::Num(Num::Pl));
    }
    if has_token(&["sg", "sing", "singular"]) {
        return Some(FeatureAssignment::Num(Num::Sg));
    }

    if has_token(&["poss", "gen"]) {
        if let Some(&(_, variant)) = POSS_PERSON_NUMBER
            .iter()
            .find(|(person_number, _)| tokens.iter().any(|t| t == person_number))
        {
            return Some(FeatureAssignment::Poss(variant));
        }
    }

    if has_token(&["loc", "locative"]) {
        return Some(FeatureAssignment::Case(CaseRole::Loc));
    }
    if has_token(&["abl", "ablative"]) {
        return Some(FeatureAssignment::Case(CaseRole::Abl));
    }
    if has_token(&["all", "allative"]) {
        return Some(FeatureAssignment::Case(CaseRole::All));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Table-driven per the design doc's testing section: one row per gloss -> expected
    /// assignment (or `None` for "no entry at all").
    #[test]
    fn alias_table_matches_the_design_docs_examples() {
        let cases: &[(&str, Option<FeatureAssignment>)] = &[
            ("pl", Some(FeatureAssignment::Num(Num::Pl))),
            ("PL", Some(FeatureAssignment::Num(Num::Pl))),
            ("plural", Some(FeatureAssignment::Num(Num::Pl))),
            ("sg", Some(FeatureAssignment::Num(Num::Sg))),
            ("1sg.poss", Some(FeatureAssignment::Poss(Poss::P1Sg))),
            ("POSS.3PL", Some(FeatureAssignment::Poss(Poss::P3Pl))),
            ("poss-2pl", Some(FeatureAssignment::Poss(Poss::P2Pl))),
            ("loc", Some(FeatureAssignment::Case(CaseRole::Loc))),
            ("ablative", Some(FeatureAssignment::Case(CaseRole::Abl))),
            // Negatives: never guess -- no entry at all.
            ("appl", None),
            ("caus", None),
            ("1sg", None), // no poss/gen token alongside it
            ("3", None),
        ];

        for (gloss, expected) in cases {
            assert_eq!(infer_one(gloss), *expected, "gloss {gloss:?}");
        }
    }

    #[test]
    fn infer_english_only_produces_entries_for_glosses_present_in_the_input() {
        let glosses = ["pl", "appl", "loc", "1sg", "caus"];
        let map = infer_english(glosses.into_iter());

        assert_eq!(map.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
        assert_eq!(
            map.lookup("loc"),
            Some(FeatureAssignment::Case(CaseRole::Loc))
        );
        assert_eq!(map.lookup("appl"), None);
        assert_eq!(map.lookup("1sg"), None);
        assert_eq!(map.lookup("caus"), None);
        // Never asked about at all -- also absent, same as any other unmapped gloss.
        assert_eq!(map.lookup("nonexistent"), None);
    }

    #[test]
    fn gender_limitation_defaults_2sg_and_3sg_possessives_to_masculine() {
        let map = infer_english(["2sg.poss", "3sg.poss"].into_iter());
        assert_eq!(
            map.lookup("2sg.poss"),
            Some(FeatureAssignment::Poss(Poss::P2SgM))
        );
        assert_eq!(
            map.lookup("3sg.poss"),
            Some(FeatureAssignment::Poss(Poss::P3SgM))
        );
    }

    #[test]
    fn merge_precedence_sidecar_overrides_inferred_base_per_gloss_key() {
        // Base: purely inferred from grammar glosses, no sidecar.
        let mut base = infer_english(["pl", "loc", "appl"].into_iter());
        assert_eq!(base.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
        assert_eq!(base.lookup("appl"), None, "inference never guesses appl");

        // Sidecar: overrides "loc" to a different case, and adds a mapping for "appl" that
        // inference alone could never produce (design doc: "a wrong phrase is worse than an
        // honest residue" -- but a grammar author supplying an explicit sidecar mapping is not
        // guessing, it's curated data, so it's allowed to fill in what inference leaves absent).
        let sidecar =
            RealizeMap::parse("[features]\n\"loc\" = \"Case:Abl\"\n\"appl\" = \"Ignore\"\n")
                .expect("valid sidecar");

        base.extend_overriding(sidecar);

        // Untouched key survives from the inferred base.
        assert_eq!(base.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
        // Sidecar wins on the shared key.
        assert_eq!(
            base.lookup("loc"),
            Some(FeatureAssignment::Case(CaseRole::Abl))
        );
        // Sidecar adds a mapping inference alone left absent.
        assert_eq!(base.lookup("appl"), Some(FeatureAssignment::Ignore));
    }
}
