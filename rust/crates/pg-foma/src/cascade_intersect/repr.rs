//! `Repr`: each token to each of its written forms, one character per arc (plan 2.6).
//!
//! The word automaton reads NFD characters, so a digraph or a second representation is an
//! ordinary multi-arc path, an empty representation is a token read that writes nothing, and every
//! segmentation of the word is explored (no greedy encoder). The start state is the only final
//! state: a token is consumed whole before the next one is read.

use std::collections::BTreeMap;

use pg_foma_runtime::cascade::{BuildMeter, BuildRefused, Input, NfaBuilder, NfaLayer, Output};

use super::spell::Spelling;

pub fn build_repr(
    spelling: &Spelling,
    char_ids: &BTreeMap<char, u32>,
    universe: u32,
    meter: &mut BuildMeter,
) -> Result<NfaLayer, BuildRefused> {
    let mut b = NfaBuilder::new("Repr", universe, meter);
    let start = b.add_state(true)?;
    b.set_start(start);
    for (token, forms) in spelling.forms.iter().enumerate() {
        let token = token as u32;
        for form in forms {
            if form.is_empty() {
                b.add_arc(start, Input::Tok(token), Output::Eps, start, &[], 0, 0)?;
                continue;
            }
            let mut from = start;
            for (k, c) in form.iter().enumerate() {
                let last = k + 1 == form.len();
                let to = if last { start } else { b.add_state(false)? };
                let input = if k == 0 {
                    Input::Tok(token)
                } else {
                    Input::Eps
                };
                b.add_arc(from, input, Output::Tok(char_ids[c]), to, &[], 0, 0)?;
                from = to;
            }
        }
    }
    Ok(b.finish())
}
