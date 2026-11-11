//! One small deterministic acceptor per rule tag, read out of the final admission net as a plain table.

use super::AdmissionRefusal;
use foma::types::{Fsm, EPSILON, IDENTITY};
use foma::utf8::is_combining;
use std::collections::{BTreeMap, HashMap};

/// The token of a character no net symbol matches, and of a symbol followed by combining marks.
const OTHER: u16 = IDENTITY as u16;
const DEAD: u32 = u32::MAX;

/// Refuses a net whose symbol numbers do not fit the 16-bit tokens the tables and the tokenizer read.
pub(super) fn check_token_range(net: &Fsm) -> Result<(), AdmissionRefusal> {
    let widest = net.sigma.iter().map(|s| s.number).max().unwrap_or(0);
    match usize::try_from(widest) {
        Ok(number) if number <= usize::from(u16::MAX) => Ok(()),
        _ => Err(AdmissionRefusal::Construction(format!(
            "symbol number {widest} does not fit a 16-bit token"
        ))),
    }
}

struct TrieNode {
    symbol: u16,
    next: Vec<(char, u32)>,
}

/// Splits text into net symbols the way foma's apply does: longest symbol first, and a combining mark turns the token into `OTHER`.
pub(super) struct Tokenizer {
    nodes: Vec<TrieNode>,
}

impl Tokenizer {
    pub(super) fn new(net: &Fsm) -> Self {
        let mut nodes = vec![TrieNode {
            symbol: 0,
            next: Vec::new(),
        }];
        for entry in net.sigma.iter().filter(|s| s.number > IDENTITY) {
            let mut at = 0usize;
            for c in entry.symbol.chars() {
                at = match nodes[at].next.iter().find(|(k, _)| *k == c) {
                    Some((_, child)) => *child as usize,
                    None => {
                        nodes.push(TrieNode {
                            symbol: 0,
                            next: Vec::new(),
                        });
                        let child = nodes.len() - 1;
                        nodes[at].next.push((c, child as u32));
                        child
                    }
                };
            }
            nodes[at].symbol = entry.number as u16;
        }
        Self { nodes }
    }

    /// The longest symbol at the start of `text` and its length in bytes; symbol 0 when none matches.
    fn longest(&self, text: &str) -> (u16, usize) {
        let mut best = (0u16, 0usize);
        let mut at = 0usize;
        for (index, c) in text.char_indices() {
            let Some((_, child)) = self.nodes[at].next.iter().find(|(k, _)| *k == c) else {
                break;
            };
            at = *child as usize;
            if self.nodes[at].symbol != 0 {
                best = (self.nodes[at].symbol, index + c.len_utf8());
            }
        }
        best
    }

    pub(super) fn tokens(&self, text: &str) -> Vec<u16> {
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(text.len());
        let mut at = 0usize;
        while at < text.len() {
            let (mut symbol, mut used) = self.longest(&text[at..]);
            if symbol == 0 {
                symbol = OTHER;
                used = text[at..].chars().next().map_or(1, char::len_utf8);
            }
            loop {
                let mark = is_combining(&bytes[at + used..]);
                if mark == 0 {
                    break;
                }
                symbol = OTHER;
                used += mark as usize;
            }
            out.push(symbol);
            at += used;
        }
        out
    }
}

/// A minimal deterministic acceptor over symbol classes; a missing transition rejects.
pub(super) struct RuleTable {
    class_of: Box<[u16]>,
    classes: usize,
    next: Box<[u32]>,
    accepting: Box<[bool]>,
    start: u32,
}

impl RuleTable {
    pub(super) fn accepts(&self, tokens: &[u16]) -> bool {
        let mut state = self.start;
        for &token in tokens {
            let class = self.class_of[token as usize] as usize;
            state = self.next[state as usize * self.classes + class];
            if state == DEAD {
                return false;
            }
        }
        self.accepting[state as usize]
    }

    pub(super) fn size(&self) -> TableSize {
        let arcs = self.next.iter().filter(|t| **t != DEAD).count();
        let bytes = self.class_of.len() * 2 + self.next.len() * 4 + self.accepting.len();
        TableSize {
            states: self.accepting.len(),
            arcs,
            bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct TableSize {
    pub states: usize,
    pub arcs: usize,
    pub bytes: usize,
}

/// What one rule tag became.
pub(super) enum RuleOutcome {
    Table(RuleTable),
    /// The language of the tag is empty: no word admits the rule.
    Never,
    /// The language holds every text: the rule is admitted for every word.
    Always,
    /// Determinizing needed more than the bound's states; carries the count reached.
    TooBig(usize),
}

struct NetArc {
    upper: i32,
    lower: i32,
    target: usize,
}

/// The arcs, starts and finals of a transducer net, read once.
pub(super) struct NetView {
    arcs: Vec<Vec<NetArc>>,
    is_final: Vec<bool>,
    starts: Vec<usize>,
    /// Symbols that occur on a lower side, as token ids; `OTHER` is not among them.
    lower_alphabet: Vec<u16>,
    max_symbol: usize,
}

fn lower_token(symbol: i32) -> u16 {
    if symbol <= IDENTITY {
        OTHER
    } else {
        symbol as u16
    }
}

impl NetView {
    pub(super) fn new(net: &Fsm) -> Self {
        let count = net.statecount.max(0) as usize;
        let mut arcs: Vec<Vec<NetArc>> = (0..count).map(|_| Vec::new()).collect();
        let mut is_final = vec![false; count];
        let mut starts = Vec::new();
        let mut lower = std::collections::BTreeSet::new();
        for (block, block_arcs) in net.states.iter_blocks() {
            let state = block.state_no as usize;
            is_final[state] = block.final_state != 0;
            if block.start_state != 0 {
                starts.push(state);
            }
            for arc in block_arcs {
                if arc.target < 0 {
                    continue;
                }
                if i32::from(arc.out) > IDENTITY {
                    lower.insert(arc.out as u16);
                }
                arcs[state].push(NetArc {
                    upper: i32::from(arc.r#in),
                    lower: i32::from(arc.out),
                    target: arc.target as usize,
                });
            }
        }
        let max_symbol = net
            .sigma
            .iter()
            .map(|s| s.number)
            .max()
            .unwrap_or(0)
            .max(IDENTITY) as usize;
        Self {
            arcs,
            is_final,
            starts,
            lower_alphabet: lower.into_iter().collect(),
            max_symbol,
        }
    }

    /// Nodes are `state * 2 + seen`, where `seen` says the tag has been read on the upper side.
    fn close(&self, tag: Option<i32>, seed: Vec<usize>, mark: &mut [bool]) -> Vec<usize> {
        let mut set = Vec::new();
        let mut stack = seed;
        while let Some(node) = stack.pop() {
            if std::mem::replace(&mut mark[node], true) {
                continue;
            }
            set.push(node);
            let seen = node % 2;
            for arc in &self.arcs[node / 2] {
                let Some(now) = upper_step(arc.upper, tag, seen) else {
                    continue;
                };
                if arc.lower == EPSILON {
                    stack.push(arc.target * 2 + now);
                }
            }
        }
        for &node in &set {
            mark[node] = false;
        }
        set.sort_unstable();
        set
    }

    /// The deterministic acceptor of the lower side of the paths whose upper side is exactly `tag`.
    pub(super) fn rule(&self, tag: i32, bound: usize) -> RuleOutcome {
        self.table(Some(tag), bound)
    }

    /// The deterministic acceptor of the lower side of every path of an acceptor net, whose arcs carry no tag.
    pub(super) fn language(&self, bound: usize) -> RuleOutcome {
        self.table(None, bound)
    }

    fn table(&self, tag: Option<i32>, bound: usize) -> RuleOutcome {
        let mut mark = vec![false; self.arcs.len() * 2];
        let seen_at_start = usize::from(tag.is_none());
        let start_seed = self.starts.iter().map(|s| s * 2 + seen_at_start).collect();
        let start = self.close(tag, start_seed, &mut mark);
        let mut ids: HashMap<Vec<usize>, u32> = HashMap::new();
        let mut sets: Vec<Vec<usize>> = Vec::new();
        let mut moves: Vec<BTreeMap<u16, u32>> = Vec::new();
        ids.insert(start.clone(), 0);
        sets.push(start);
        let mut at = 0;
        while at < sets.len() {
            let mut by_symbol: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
            for &node in &sets[at] {
                let seen = node % 2;
                for arc in &self.arcs[node / 2] {
                    if arc.lower == EPSILON {
                        continue;
                    }
                    if let Some(now) = upper_step(arc.upper, tag, seen) {
                        by_symbol
                            .entry(lower_token(arc.lower))
                            .or_default()
                            .push(arc.target * 2 + now);
                    }
                }
            }
            let mut row = BTreeMap::new();
            for (symbol, seed) in by_symbol {
                let target = self.close(tag, seed, &mut mark);
                let id = match ids.get(&target) {
                    Some(id) => *id,
                    None => {
                        if sets.len() >= bound {
                            return RuleOutcome::TooBig(sets.len());
                        }
                        let id = sets.len() as u32;
                        ids.insert(target.clone(), id);
                        sets.push(target);
                        id
                    }
                };
                row.insert(symbol, id);
            }
            moves.push(row);
            at += 1;
        }
        let accepting: Vec<bool> = sets
            .iter()
            .map(|set| {
                set.iter()
                    .any(|node| node % 2 == 1 && self.is_final[node / 2])
            })
            .collect();
        self.minimize(&moves, &accepting)
    }

    /// Merges equivalent states of the determinized acceptor, drops the dead ones and compresses the symbols into classes.
    fn minimize(&self, moves: &[BTreeMap<u16, u32>], accepting: &[bool]) -> RuleOutcome {
        let count = moves.len();
        let sink = count;
        let mut symbols: Vec<u16> = moves.iter().flat_map(|row| row.keys().copied()).collect();
        symbols.sort_unstable();
        symbols.dedup();
        let step = |state: usize, symbol: u16| -> usize {
            if state == sink {
                return sink;
            }
            moves[state]
                .get(&symbol)
                .map_or(sink, |target| *target as usize)
        };
        let mut block: Vec<u32> = (0..=count)
            .map(|s| u32::from(s != sink && accepting[s]))
            .collect();
        let mut blocks = 2;
        loop {
            let mut ids: HashMap<Vec<u32>, u32> = HashMap::new();
            let mut next_block = vec![0u32; count + 1];
            for state in 0..=count {
                let mut key = Vec::with_capacity(symbols.len() + 1);
                key.push(block[state]);
                key.extend(symbols.iter().map(|&s| block[step(state, s)]));
                let fresh = ids.len() as u32;
                next_block[state] = *ids.entry(key).or_insert(fresh);
            }
            let settled = ids.len() == blocks;
            blocks = ids.len();
            block = next_block;
            if settled {
                break;
            }
        }
        let dead = block[sink];
        if block[0] == dead {
            return RuleOutcome::Never;
        }
        let mut live: Vec<u32> = (0..=count)
            .map(|s| block[s])
            .filter(|b| *b != dead)
            .collect();
        live.sort_unstable();
        live.dedup();
        let index = |b: u32| live.binary_search(&b).ok().map(|i| i as u32);
        let mut representative = vec![0usize; live.len()];
        for state in (0..count).rev() {
            if let Some(i) = index(block[state]) {
                representative[i as usize] = state;
            }
        }
        let column = |symbol: u16| -> Vec<u32> {
            representative
                .iter()
                .map(|&s| index(block[step(s, symbol)]).unwrap_or(DEAD))
                .collect()
        };
        let mut class_ids: HashMap<Vec<u32>, u16> = HashMap::new();
        let mut columns: Vec<Vec<u32>> = vec![vec![DEAD; live.len()]];
        class_ids.insert(columns[0].clone(), 0);
        let mut class_of = vec![0u16; self.max_symbol + 1];
        for &symbol in &symbols {
            let col = column(symbol);
            let id = *class_ids.entry(col.clone()).or_insert_with(|| {
                columns.push(col);
                (columns.len() - 1) as u16
            });
            class_of[symbol as usize] = id;
        }
        let classes = columns.len();
        let mut next = vec![DEAD; live.len() * classes];
        for (class, col) in columns.iter().enumerate() {
            for (state, target) in col.iter().enumerate() {
                next[state * classes + class] = *target;
            }
        }
        let accepts: Vec<bool> = representative.iter().map(|&s| accepting[s]).collect();
        let start = index(block[0]).unwrap_or(DEAD);
        let loops = |&sym: &u16| next[class_of[sym as usize] as usize] == start;
        let every_text = || self.lower_alphabet.iter().chain(&[OTHER]).all(loops);
        if live.len() == 1 && accepts[0] && every_text() {
            return RuleOutcome::Always;
        }
        RuleOutcome::Table(RuleTable {
            class_of: class_of.into(),
            classes,
            next: next.into(),
            accepting: accepts.into(),
            start,
        })
    }
}

/// The `seen` flag after an arc's upper symbol, or `None` when the arc leaves the paths whose upper side is exactly the tag.
fn upper_step(upper: i32, tag: Option<i32>, seen: usize) -> Option<usize> {
    let Some(tag) = tag else {
        return Some(seen);
    };
    if upper == EPSILON {
        return Some(seen);
    }
    (upper == tag && seen == 0).then_some(1)
}
