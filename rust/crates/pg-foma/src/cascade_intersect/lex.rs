//! `Lex`: a trie over the token strings of every root allomorph (plan 2.2).
//!
//! Each entry ends in its own final epsilon arc tagged `root(entry)`; the trie arcs before it are
//! shared by every entry with that prefix, so the tag is never on a shared arc. An entry whose
//! allomorph has no segment token is just that arc from the start state. All allomorphs of all
//! entries of every stratum are in this one layer: a root of a later stratum reaches its stratum's
//! zone through the earlier zones by identity (every zone contains the identity from its start).
//! The token string is the shape synthesis seeds: the greedy segmentation of the text. A text that
//! does not segment (a root with a class node) is refused before this layer is built.

use std::collections::{BTreeSet, HashMap};

use pg_foma_runtime::cascade::{BuildMeter, Input, NfaBuilder, NfaLayer, Output, Tag};
use pg_grammar::chardef::CharDefTable;
use pg_grammar::model::Grammar;

use super::refusal::BuildError;

/// The token string of a root allomorph's text, exactly as synthesis seeds it; `None` when the text
/// does not segment (a root with a class node, which the refusal function names).
pub fn root_tokens(table: &CharDefTable, text: &str) -> Option<Vec<u32>> {
    let shape = pg_grammar::segment::segment(table, text).ok()?;
    Some(shape.interior().map(|(_, _, cd, _)| cd).collect())
}

pub struct LexBuilt {
    pub layer: NfaLayer,
    /// `(entry, allomorph index)` of every root allomorph in the trie.
    pub allomorphs: BTreeSet<(u32, usize)>,
}

pub fn build_lex(
    g: &Grammar,
    universe: u32,
    meter: &mut BuildMeter,
) -> Result<LexBuilt, BuildError> {
    let mut b = NfaBuilder::new("Lex", universe, meter);
    let start = b.add_state(false)?;
    b.set_start(start);
    let end = b.add_state(true)?;
    let mut children: HashMap<(u32, u32), u32> = HashMap::new();
    let mut allomorphs = BTreeSet::new();
    for (ei, entry) in g.entries.iter().enumerate() {
        let stratum = g.morphemes[entry.morpheme.0 as usize].stratum;
        let table = &g.char_tables[g.strata[stratum.0 as usize].table.0 as usize];
        for (ai, allo) in entry.allomorphs.iter().enumerate() {
            let mut at = start;
            let Some(tokens) = root_tokens(table, &allo.shape.text) else {
                return Err(BuildError::Accounting(format!(
                    "root allomorph {ai} of entry {ei} does not segment"
                )));
            };
            for t in tokens {
                at = match children.get(&(at, t)) {
                    Some(&next) => next,
                    None => {
                        let next = b.add_state(false)?;
                        b.add_arc(at, Input::Eps, Output::Tok(t), next, &[], 0, 0)?;
                        children.insert((at, t), next);
                        next
                    }
                };
            }
            b.add_arc(
                at,
                Input::Eps,
                Output::Eps,
                end,
                &[Tag::root(ei as u32)],
                0,
                0,
            )?;
            allomorphs.insert((ei as u32, ai));
        }
    }
    Ok(LexBuilt {
        layer: b.finish(),
        allomorphs,
    })
}
