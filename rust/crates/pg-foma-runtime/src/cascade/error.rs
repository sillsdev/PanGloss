//! `Err1` and `Err2`: the edit operation `E1` of plan S.2, one layer per application (S.3).
//!
//! A layer reads a candidate unit token from above and writes a typed-side token below. Tokens are
//! FOLDED unit tokens and are compared by exact token equality, so no case reaches these layers.
//! States: `clean`, `used` (both final), and for a swap `P(u1)` (first unit read, nothing written)
//! then `Q(u1)` (second unit written, the first still to write); neither of those is final, so a
//! swap is never left pending. The number of layers not in `clean` is the edit counter.
//!
//! | operation | arc | weight |
//! |---|---|---|
//! | match | `u:u`, in `clean` and in `used` | 0 |
//! | `sub`, `kb`, `dia` | `u:v` for a typed unit `v`, `u != v`; `dia` when the bases are equal | table |
//! | `del` (a typed unit missing from the candidate) | `eps:v` | `ins`/`del`, or `dia` for an empty base |
//! | `ins` (an extra candidate unit) | `u:eps` | same |
//! | `trans` | `u1:eps`, `u2:u2` (`u2 != u1`), `eps:u1` | `trans` on the first arc |
//!
//! The written side of a substitution or deletion is restricted to the units of the typed word:
//! an intermediate unit that is neither typed nor read from above can be removed from any path
//! without raising its weight or edit count, so nothing is lost (S.2, "Why this is the ball").
//! The layers are built per typed word.

use std::collections::BTreeMap;

use super::budget::{BuildMeter, BuildRefused};
use super::layer::{Input, NfaBuilder, NfaLayer, Output, Token, TokenSet};
use super::units::UnitModel;

/// Edit weights in thousandths (S.2 table; placeholders until an error corpus exists).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ErrWeights {
    pub sub: u32,
    pub kb: u32,
    pub dia: u32,
    pub del: u32,
    pub ins: u32,
    pub trans: u32,
}

impl Default for ErrWeights {
    fn default() -> Self {
        ErrWeights {
            sub: 1000,
            kb: 800,
            dia: 500,
            del: 1000,
            ins: 1000,
            trans: 900,
        }
    }
}

/// A weight table that failed validation at load (`Refused`): which entry and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightRefused {
    pub entry: String,
    pub reason: String,
}

fn refuse(entry: &str, reason: &str) -> WeightRefused {
    WeightRefused {
        entry: entry.to_string(),
        reason: reason.to_string(),
    }
}

/// A plain non-negative decimal as whole thousandths; any other text is an error.
pub fn parse_thousandths(entry: &str, text: &str) -> Result<u32, WeightRefused> {
    let t = text.trim();
    let (int, frac) = match t.split_once('.') {
        Some((i, f)) => (i, f),
        None => (t, ""),
    };
    if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) {
        return Err(refuse(entry, "not a plain non-negative decimal"));
    }
    if !frac.bytes().all(|b| b.is_ascii_digit()) || (t.contains('.') && frac.is_empty()) {
        return Err(refuse(entry, "not a plain non-negative decimal"));
    }
    let (keep, rest) = if frac.len() > 3 {
        frac.split_at(3)
    } else {
        (frac, "")
    };
    if rest.bytes().any(|b| b != b'0') {
        return Err(refuse(entry, "not a whole number of thousandths"));
    }
    let mut digits = int.to_string();
    digits.push_str(keep);
    for _ in keep.len()..3 {
        digits.push('0');
    }
    digits
        .parse::<u32>()
        .map_err(|_| refuse(entry, "out of range"))
}

impl ErrWeights {
    /// Load `name = value` lines (`#` comments and blank lines allowed). All six names are required,
    /// each once; every value must be a finite, non-negative whole number of thousandths.
    pub fn from_table(text: &str) -> Result<ErrWeights, WeightRefused> {
        let mut seen: BTreeMap<String, u32> = BTreeMap::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                return Err(refuse(line, "expected `name = value`"));
            };
            let k = k.trim();
            if !["sub", "kb", "dia", "del", "ins", "trans"].contains(&k) {
                return Err(refuse(k, "unknown weight name"));
            }
            if seen.contains_key(k) {
                return Err(refuse(k, "given twice"));
            }
            seen.insert(k.to_string(), parse_thousandths(k, v)?);
        }
        let get = |k: &str| {
            seen.get(k)
                .copied()
                .ok_or_else(|| refuse(k, "missing weight"))
        };
        Ok(ErrWeights {
            sub: get("sub")?,
            kb: get("kb")?,
            dia: get("dia")?,
            del: get("del")?,
            ins: get("ins")?,
            trans: get("trans")?,
        })
    }
}

const QWERTY: [&str; 3] = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];

/// Keyboard neighbours on folded single-character bases: adjacent rows, columns within one key
/// (row offsets of a quarter and three quarters of a key), as in the reference generator.
pub fn kb_adjacent(a: &str, b: &str) -> bool {
    fn pos(s: &str) -> Option<(i32, i32)> {
        let mut it = s.chars();
        let c = it.next()?;
        if it.next().is_some() {
            return None;
        }
        for (r, row) in QWERTY.iter().enumerate() {
            if let Some(i) = row.chars().position(|x| x == c) {
                let off = [0, 1, 3][r];
                return Some((r as i32, 4 * i as i32 + off));
            }
        }
        None
    }
    match (pos(a), pos(b)) {
        (Some((ra, xa)), Some((rb, xb))) => a != b && (ra - rb).abs() <= 1 && (xa - xb).abs() <= 4,
        _ => false,
    }
}

/// One error layer for the typed word of `model`. `Err1` and `Err2` are the same table.
pub fn build_err_layer(
    name: &str,
    model: &UnitModel,
    w: &ErrWeights,
    meter: &mut BuildMeter,
) -> Result<NfaLayer, BuildRefused> {
    let n = model.token_count();
    let folded: Vec<Token> = (0..n).filter(|&t| model.is_folded_token(t)).collect();
    let mut typed: Vec<Token> = model.typed_tokens().to_vec();
    typed.sort();
    typed.dedup();
    let empty = model.empty_base();
    let u = model.universe();
    let mut b = NfaBuilder::new(name, u, meter);
    let clean = b.add_state(true)?;
    let used = b.add_state(true)?;
    b.set_start(clean);
    let all = b.add_set(TokenSet::from_tokens(u, &folded))?;
    b.add_arc(clean, Input::Class(all), Output::Same, clean, &[], 0, 0)?;
    b.add_arc(used, Input::Class(all), Output::Same, used, &[], 0, 0)?;

    // Substitutions into a typed unit v, by weight class.
    for &v in &typed {
        let vb = model.unit_tok(v).base;
        let vs = model.base_str(vb).to_string();
        let mut dia = Vec::new();
        let mut kb = Vec::new();
        let mut sub = Vec::new();
        for &u in &folded {
            if u == v {
                continue;
            }
            let ub = model.unit_tok(u).base;
            if ub == vb {
                dia.push(u);
            } else if kb_adjacent(model.base_str(ub), &vs) {
                kb.push(u);
            } else {
                sub.push(u);
            }
        }
        for (set, weight) in [(dia, w.dia), (kb, w.kb), (sub, w.sub)] {
            if set.is_empty() {
                continue;
            }
            let s = b.add_set(TokenSet::from_tokens(u, &set))?;
            b.add_arc(clean, Input::Class(s), Output::Tok(v), used, &[], weight, 1)?;
        }
        // A typed unit missing from the candidate.
        let wt = if vb == empty { w.dia } else { w.del };
        b.add_arc(clean, Input::Eps, Output::Tok(v), used, &[], wt, 1)?;
    }
    // An extra candidate unit.
    let (empties, others): (Vec<Token>, Vec<Token>) = folded
        .iter()
        .partition(|&&u| model.unit_tok(u).base == empty);
    for (set, weight) in [(empties, w.dia), (others, w.ins)] {
        if set.is_empty() {
            continue;
        }
        let s = b.add_set(TokenSet::from_tokens(u, &set))?;
        b.add_arc(clean, Input::Class(s), Output::Eps, used, &[], weight, 1)?;
    }
    // Swap of two adjacent unequal units.
    for &u1 in &folded {
        let p = b.add_state(false)?;
        let q = b.add_state(false)?;
        b.add_arc(clean, Input::Tok(u1), Output::Eps, p, &[], w.trans, 1)?;
        let rest: Vec<Token> = folded.iter().copied().filter(|&x| x != u1).collect();
        if !rest.is_empty() {
            let s = b.add_set(TokenSet::from_tokens(u, &rest))?;
            b.add_arc(p, Input::Class(s), Output::Same, q, &[], 0, 0)?;
        }
        b.add_arc(q, Input::Eps, Output::Tok(u1), used, &[], 0, 0)?;
    }
    Ok(b.finish())
}
