//! The lexicon of a grammar as one acceptor over its root allomorph shapes.

use super::TileCounts;
use crate::admission::build;
use crate::admission::meter::Meter;
use crate::replace::SegAlphabet;
use foma::options::FomaOptions;
use foma::structures::{fsm_empty_set, fsm_empty_string};
use foma::trie::{fsm_trie_add_word, fsm_trie_done, fsm_trie_init_sized};
use foma::types::Fsm;
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::Grammar;
use pg_shape::NodeKind;
use std::collections::BTreeSet;

pub(super) fn trie_net(meter: &Meter, opts: &FomaOptions, words: &BTreeSet<String>) -> Fsm {
    if words.is_empty() || !meter.allows(0) {
        return fsm_empty_set();
    }
    let mut trie = fsm_trie_init_sized((words.len() as u32).saturating_mul(2).max(1031));
    for word in words {
        fsm_trie_add_word(&mut trie, word);
    }
    meter.minimize(opts, fsm_trie_done(trie))
}

enum RootForm {
    Plain(String),
    Pattern(Vec<(Option<char>, bool, bool)>),
}

fn root_form(alphabet: &SegAlphabet, shape: &pg_shape::Shape) -> RootForm {
    let nodes: Vec<(Option<char>, bool, bool, NodeKind)> = shape
        .interior()
        .map(|(_, kind, cd, flags)| {
            let ch = (cd != pg_shape::NO_CHAR_DEF).then(|| alphabet.token(CharDefId(cd)));
            (ch, flags.is_optional(), flags.is_iterative(), kind)
        })
        .collect();
    if nodes
        .iter()
        .all(|(ch, optional, iterative, _)| ch.is_some() && !optional && !iterative)
    {
        return RootForm::Plain(nodes.iter().filter_map(|(ch, ..)| *ch).collect());
    }
    RootForm::Pattern(nodes.into_iter().map(|(c, o, i, _)| (c, o, i)).collect())
}

fn pattern_net(
    meter: &Meter,
    opts: &FomaOptions,
    form: &[(Option<char>, bool, bool)],
    alphabet: &[char],
) -> Fsm {
    let mut net = fsm_empty_string();
    for (ch, optional, iterative) in form {
        let base = match ch {
            Some(c) => meter.symbol(&c.to_string()),
            None => build::symbols_union(meter, opts, alphabet.iter().map(|c| c.to_string()))
                .unwrap_or_else(fsm_empty_string),
        };
        let node = match (optional, iterative) {
            (true, true) => meter.star(opts, base),
            (false, true) => meter.plus(opts, base),
            (true, false) => meter.optional(opts, base),
            (false, false) => base,
        };
        net = meter.concat(opts, net, node);
    }
    net
}

/// Units the trie of the plain root spellings weighs at least: a state for every distinct prefix and an arc into it.
pub(super) fn trie_least(g: &Grammar, alphabet: &SegAlphabet) -> u64 {
    let mut words = BTreeSet::new();
    for allo in g.entries.iter().flat_map(|entry| &entry.allomorphs) {
        if let RootForm::Plain(text) = root_form(alphabet, &allo.shape.shape) {
            words.insert(text);
        }
    }
    let mut previous: Vec<char> = Vec::new();
    let mut prefixes = 0u64;
    for word in &words {
        let chars: Vec<char> = word.chars().collect();
        let shared = previous
            .iter()
            .zip(&chars)
            .take_while(|(a, b)| a == b)
            .count();
        prefixes += (chars.len() - shared) as u64;
        previous = chars;
    }
    match prefixes {
        0 => 0,
        n => 2 * n + 1,
    }
}

/// Every allomorph shape as one acceptor, and the deep-stratum subset.
pub(super) struct Roots {
    pub(super) all: Fsm,
    pub(super) deep: Option<Fsm>,
}

pub(super) fn root_nets(
    meter: &Meter,
    opts: &FomaOptions,
    g: &Grammar,
    alphabet: &SegAlphabet,
    tokens_all: &[char],
    at: usize,
    counts: &mut TileCounts,
) -> Roots {
    let (mut plain, mut plain_deep) = (BTreeSet::new(), BTreeSet::new());
    let (mut patterns, mut patterns_deep): (Vec<Fsm>, Vec<Fsm>) = (Vec::new(), Vec::new());
    counts.entries = g.entries.len();
    for entry in &g.entries {
        let deep = g.morphemes[entry.morpheme.0 as usize].stratum.0 as usize > at;
        counts.deep_roots += usize::from(deep);
        for allo in &entry.allomorphs {
            counts.root_allomorphs += 1;
            match root_form(alphabet, &allo.shape.shape) {
                RootForm::Plain(text) => {
                    counts.root_plain += 1;
                    plain.insert(text.clone());
                    if deep {
                        plain_deep.insert(text);
                    }
                }
                RootForm::Pattern(form) => {
                    counts.root_patterned += 1;
                    patterns.push(pattern_net(meter, opts, &form, tokens_all));
                    if deep {
                        patterns_deep.push(pattern_net(meter, opts, &form, tokens_all));
                    }
                }
            }
        }
    }
    let join = |words: &BTreeSet<String>, extra: Vec<Fsm>| -> Option<Fsm> {
        let mut nets = extra;
        if !words.is_empty() {
            nets.push(trie_net(meter, opts, words));
        }
        build::union_all(meter, opts, nets)
    };
    Roots {
        all: join(&plain, patterns).unwrap_or_else(fsm_empty_set),
        deep: join(&plain_deep, patterns_deep),
    }
}
