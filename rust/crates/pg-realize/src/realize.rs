//! Natural-phrases N2 (`docs/natural-phrases-plan.md` N2): the realizer trait boundary and its
//! result type. [`crate::table::TableRealizer`] is the one implementation this milestone ships;
//! this module only defines the shape every realizer produces and the trait a caller (`pg-cli`)
//! programs against, deliberately independent of *how* a `Realization` gets built.
#![forbid(unsafe_code)]

use crate::ir::GlossIr;

/// One [`GlossIr`]'s realized natural-language phrase, plus enough bookkeeping for a caller to
/// judge how much to trust it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Realization {
    /// The rendered phrase, e.g. `"in my houses"`. Always non-empty for a non-empty `GlossIr`
    /// (worst case, a realizer falls back to the concept's bare citation form) — no realizer
    /// implementation may return an empty string outside of a genuinely empty input.
    pub text: String,
    /// `true` only when `residue` is empty AND `text` was produced by filling a real, matched
    /// construction template (not a fallback path) AND every noun-form slot that template needed
    /// was cleanly derivable (no missing plural, no guessed/uninflectable root). `false` is not
    /// an error — it just means the caller should treat `text` as a best-effort rendering and may
    /// want to fall back to (or append) the Leipzig gloss line (`crate::leipzig`, N0).
    pub complete: bool,
    /// Display strings for material the `GlossIr` carried but this realization couldn't place —
    /// mirrors [`GlossIr::extras`] verbatim (a realizer never invents its own residue beyond what
    /// the IR already flagged as unrealized).
    pub residue: Vec<String>,
}

/// Turns a [`GlossIr`] into a [`Realization`]. Implemented today by
/// [`crate::table::TableRealizer`] (compile-time-authored English construction tables,
/// Architecture B — `docs/natural-glosses-plan.md` §8). The trait boundary exists specifically so
/// a future Architecture A implementation (an embedded PGF runtime linearizing against the real
/// `Gloss.gf`/`GlossEng.gf` sources N3 lays down, instead of a hand-authored table) can slot in
/// here as a second `Realizer` impl with no change to any caller — `pg-cli`'s `--natural-gloss`
/// wiring, and any future caller, programs against this trait, never against `TableRealizer`
/// directly, so that swap is the whole migration (`docs/natural-glosses-plan.md` §8's
/// Architecture A/B comparison; §8's "Recommendation" is what motivates shipping B first with
/// this seam already in place).
pub trait Realizer {
    fn realize(&self, ir: &GlossIr) -> Realization;
}
