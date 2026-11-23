//! `Fold`: full Unicode case folding of a unit's base, stateless (plan S.2, S.3).
//!
//! The folding table is the generated [`casefold_table`](super::casefold_table) (statuses C and F of
//! CaseFolding.txt as Python's `str.casefold` implements them; Rust's std has no case folding, and
//! `to_lowercase` differs from it, e.g. on sharp s, final sigma and the Cherokee small letters).
//! A unit stays ONE unit when its folded base is several characters, and its marks are never
//! touched, so the layer is a single-state `Image` map over unit tokens (mutations C139 to C141).

use unicode_normalization::UnicodeNormalization;

use super::budget::{BuildMeter, BuildRefused};
use super::casefold_table::CASEFOLD;
use super::layer::{Input, NfaBuilder, NfaLayer, Output, TokenSet};
use super::units::UnitModel;

/// Full case folding of one character: the folded string, or `None` when the character folds to itself.
pub fn casefold_char(c: char) -> Option<&'static str> {
    CASEFOLD
        .binary_search_by_key(&(c as u32), |&(cp, _)| cp)
        .ok()
        .map(|i| CASEFOLD[i].1)
}

/// The folded form of a base (a string of non-mark characters), in NFD.
pub fn fold_base(base: &str) -> String {
    let mut out = String::new();
    for c in base.chars() {
        match casefold_char(c) {
            Some(f) => out.push_str(f),
            None => out.push(c),
        }
    }
    out.nfd().collect()
}

/// The `Fold` layer: one final state, one class arc whose image sends every unit token to the
/// token of its folded base with the same stack (typed prefix node or `other`). Tokens already
/// folded map to themselves.
pub fn build_fold_layer(
    model: &UnitModel,
    meter: &mut BuildMeter,
) -> Result<NfaLayer, BuildRefused> {
    let n = model.token_count();
    let mut b = NfaBuilder::new("Fold", n, meter);
    let s = b.add_state(true)?;
    let all = b.add_set(TokenSet::full(n))?;
    let image: Vec<u32> = (0..n).map(|t| model.fold_token(t)).collect();
    let img = b.add_image(image)?;
    b.add_arc(s, Input::Class(all), Output::Image(img), s, &[], 0, 0)?;
    Ok(b.finish())
}
