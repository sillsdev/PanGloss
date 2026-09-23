//! Built-in English gloss-alias inference for the `PanGloss-demo` consumer, so a grammar with no
//! `realize.toml` sidecar (`crate::map`'s `RealizeMap::empty()` path) still gets natural-ish
//! phrases out of the box instead of every affix falling straight through to
//! `crate::ir::GlossIr::extras`.
//!
//! `infer_english` builds a `RealizeMap` directly from a grammar's affix-morpheme gloss
//! strings — no sidecar file, no grammar-specific tuning. Token normalization: lowercase, then
//! split on `.`, `-`, `_`, `:`; the resulting token set is matched against a small built-in
//! alias table (Num/Poss/Case, below). Only glosses actually present in the input iterator get
//! an entry (mirrors `RealizeMap::parse`'s "only what's in the file" shape) — an unmatched gloss
//! is simply absent, never guessed. **A wrong phrase is worse than an honest residue**: this
//! module's guiding rule, and the reason this table stays narrow and declines to match ambiguous
//! or partial tokens rather than reaching for a "close enough" guess.
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
//!   `RealizeMap::extend_overriding` — sidecar wins, this inferred base is just a starting
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

/// Build a `RealizeMap` by matching each gloss in `glosses` against the built-in English
/// alias table (module docs above). `glosses` is expected to be a grammar's affix-morpheme
/// gloss strings (e.g. iterated from `Grammar::morphemes`); only glosses that actually match an
/// alias produce an entry, keyed by the exact (un-normalized) string as given by the iterator —
/// the same key shape `crate::map::RealizeMap::lookup` is called with from
/// `crate::ir::to_ir` (a token's raw `gloss` string, verbatim).
pub fn infer_english<'a>(glosses: impl Iterator<Item = &'a str>) -> RealizeMap {
    let mut map = RealizeMap::empty();
    for gloss in glosses {
        if let Some(assignment) = infer_one(gloss) {
            map.insert(gloss.to_string(), assignment);
        }
    }
    map
}

/// Normalizes one raw gloss string into its token set: lowercase, split on `.`/`-`/`_`/`:`, dropping empty pieces.
fn normalize_tokens(gloss: &str) -> Vec<String> {
    gloss
        .to_lowercase()
        .split(['.', '-', '_', ':'])
        .filter(|piece| !piece.is_empty())
        .map(|piece| piece.to_string())
        .collect()
}

/// Person+number tokens the Poss branch recognizes, paired with the `Poss` variant each maps to (2sg/3sg default to masculine gender — a known limitation).
const POSS_PERSON_NUMBER: &[(&str, Poss)] = &[
    ("1sg", Poss::P1Sg),
    ("1pl", Poss::P1Pl),
    ("2sg", Poss::P2SgM),
    ("2pl", Poss::P2Pl),
    ("3sg", Poss::P3SgM),
    ("3pl", Poss::P3Pl),
];

/// Matches one normalized gloss's tokens against the alias table, in Num -> Poss -> Case priority order; `None` means no alias matched, and the caller leaves this gloss with no entry.
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
mod tests;
