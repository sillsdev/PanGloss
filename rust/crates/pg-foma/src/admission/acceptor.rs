//! One small deterministic acceptor per rule tag, read out of the final admission net as a plain table.

use super::meter::Meter;
use super::AdmissionRefusal;
use foma::types::{Fsm, EPSILON, IDENTITY};
use rustc_hash::FxHashMap;

/// The token of a character no net symbol matches, and of a symbol followed by combining marks.
pub(super) const OTHER: u16 = IDENTITY as u16;
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

/// A minimal deterministic acceptor over symbol classes; a missing transition rejects.
#[derive(Debug, PartialEq)]
pub(super) struct RuleTable {
    class_of: Box<[u16]>,
    classes: usize,
    next: Box<[u32]>,
    accepting: Box<[bool]>,
    start: u32,
}

impl RuleTable {
    /// Whether the table accepts the tokens, and how many transitions the walk took, the one that fails included.
    pub(super) fn walk(&self, tokens: &[u16]) -> (bool, usize) {
        let mut state = self.start;
        for (taken, &token) in tokens.iter().enumerate() {
            let class = self.class_of[token as usize] as usize;
            state = self.next[state as usize * self.classes + class];
            if state == DEAD {
                return (false, taken + 1);
            }
        }
        (self.accepting[state as usize], tokens.len())
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
#[derive(Debug, PartialEq)]
pub(super) enum RuleOutcome {
    Table(RuleTable),
    /// The language of the tag is empty: no word admits the rule.
    Never,
    /// The language holds every text: the rule is admitted for every word.
    Always,
    /// Determinizing needed more than the bound's states; carries the count reached.
    TooBig(usize),
    /// The meter's construction budget ran out while the table was read out.
    OverBudget,
}

/// The arcs of every state in one array: row `s` is `items[at[s]..at[s + 1]]`.
struct Rows<T> {
    at: Vec<u32>,
    items: Vec<T>,
}

impl<T> Rows<T> {
    fn new(lists: Vec<Vec<T>>) -> Self {
        let mut at = Vec::with_capacity(lists.len() + 1);
        let mut items = Vec::new();
        at.push(0);
        for list in lists {
            items.extend(list);
            at.push(items.len() as u32);
        }
        Self { at, items }
    }

    fn row(&self, state: usize) -> &[T] {
        &self.items[self.at[state] as usize..self.at[state + 1] as usize]
    }
}

/// The arcs, starts and finals of a transducer net, read once.
pub(super) struct NetView {
    /// Arcs with no lower symbol: upper symbol and target.
    silent: Rows<(i32, u32)>,
    /// Arcs that read a lower symbol: upper symbol, lower token and target.
    reading: Rows<(i32, u16, u32)>,
    is_final: Vec<bool>,
    starts: Vec<usize>,
    /// Symbols that occur on a lower side, as token ids; `OTHER` is not among them.
    lower_alphabet: Vec<u16>,
    max_symbol: usize,
}

/// Buffers that every table build leaves clean for the next, so a net's tables share one allocation.
pub(super) struct Scratch {
    /// Nodes are `state * 2 + seen`, where `seen` says the tag has been read on the upper side.
    mark: Vec<bool>,
    stack: Vec<usize>,
    by_symbol: Vec<Vec<usize>>,
    touched: Vec<u16>,
}

impl Scratch {
    /// Empties every bucket the last row filled and every mark the last closure set.
    fn reset(&mut self) {
        for &symbol in &self.touched {
            self.by_symbol[symbol as usize].clear();
        }
        self.touched.clear();
        for node in self.stack.drain(..) {
            self.mark[node] = false;
        }
    }

    fn push(&mut self, node: usize) {
        if !std::mem::replace(&mut self.mark[node], true) {
            self.stack.push(node);
        }
    }
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
        let mut silent: Vec<Vec<(i32, u32)>> = (0..count).map(|_| Vec::new()).collect();
        let mut reading: Vec<Vec<(i32, u16, u32)>> = (0..count).map(|_| Vec::new()).collect();
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
                let (upper, target) = (i32::from(arc.r#in), arc.target as u32);
                if i32::from(arc.out) == EPSILON {
                    silent[state].push((upper, target));
                    continue;
                }
                if i32::from(arc.out) > IDENTITY {
                    lower.insert(arc.out as u16);
                }
                reading[state].push((upper, lower_token(i32::from(arc.out)), target));
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
            silent: Rows::new(silent),
            reading: Rows::new(reading),
            is_final,
            starts,
            lower_alphabet: lower.into_iter().collect(),
            max_symbol,
        }
    }

    pub(super) fn scratch(&self) -> Scratch {
        Scratch {
            mark: vec![false; self.is_final.len() * 2],
            stack: Vec::new(),
            by_symbol: (0..=self.max_symbol).map(|_| Vec::new()).collect(),
            touched: Vec::new(),
        }
    }

    /// The nodes reachable from the seeds on the scratch stack without reading a lower symbol, sorted.
    fn close(&self, tag: Option<i32>, scratch: &mut Scratch) -> Vec<usize> {
        let mut set = Vec::new();
        while let Some(node) = scratch.stack.pop() {
            set.push(node);
            let seen = node % 2;
            for &(upper, target) in self.silent.row(node / 2) {
                if let Some(now) = upper_step(upper, tag, seen) {
                    scratch.push(target as usize * 2 + now);
                }
            }
        }
        for &node in &set {
            scratch.mark[node] = false;
        }
        set.sort_unstable();
        set
    }

    /// The deterministic acceptor of the lower side of the paths whose upper side is exactly `tag`.
    pub(super) fn rule(
        &self,
        tag: i32,
        scratch: &mut Scratch,
        limits: (usize, &Meter),
    ) -> RuleOutcome {
        self.table(Some(tag), scratch, limits)
    }

    /// The deterministic acceptor of the lower side of every path of an acceptor net, whose arcs carry no tag.
    pub(super) fn language(&self, scratch: &mut Scratch, limits: (usize, &Meter)) -> RuleOutcome {
        self.table(None, scratch, limits)
    }

    /// Reads one table and leaves the scratch clean whatever the outcome, so no return path inside can leave a bucket behind.
    fn table(
        &self,
        tag: Option<i32>,
        scratch: &mut Scratch,
        limits: (usize, &Meter),
    ) -> RuleOutcome {
        let outcome = self.determinize(tag, scratch, limits);
        scratch.reset();
        outcome
    }

    fn determinize(
        &self,
        tag: Option<i32>,
        scratch: &mut Scratch,
        (bound, meter): (usize, &Meter),
    ) -> RuleOutcome {
        let seen_at_start = usize::from(tag.is_none());
        for start in &self.starts {
            scratch.push(start * 2 + seen_at_start);
        }
        let start = self.close(tag, scratch);
        if !meter.allows(start.len() as u64) {
            return RuleOutcome::OverBudget;
        }
        meter.spend(start.len() as u64);
        let mut ids: FxHashMap<Vec<usize>, u32> = FxHashMap::default();
        let mut sets: Vec<Vec<usize>> = Vec::new();
        let mut moves: Vec<Vec<(u16, u32)>> = Vec::new();
        ids.insert(start.clone(), 0);
        sets.push(start);
        let mut at = 0;
        while at < sets.len() {
            scratch.touched.clear();
            for &node in &sets[at] {
                let seen = node % 2;
                for &(upper, lower, target) in self.reading.row(node / 2) {
                    let Some(now) = upper_step(upper, tag, seen) else {
                        continue;
                    };
                    let bucket = &mut scratch.by_symbol[lower as usize];
                    if bucket.is_empty() {
                        scratch.touched.push(lower);
                    }
                    bucket.push(target as usize * 2 + now);
                }
            }
            scratch.touched.sort_unstable();
            let mut row = Vec::with_capacity(scratch.touched.len());
            for index in 0..scratch.touched.len() {
                let symbol = scratch.touched[index];
                let mut seed = std::mem::take(&mut scratch.by_symbol[symbol as usize]);
                for &node in &seed {
                    scratch.push(node);
                }
                seed.clear();
                scratch.by_symbol[symbol as usize] = seed;
                let target = self.close(tag, scratch);
                let id = match ids.get(target.as_slice()) {
                    Some(id) => *id,
                    None => {
                        if sets.len() >= bound {
                            return RuleOutcome::TooBig(sets.len());
                        }
                        let id = sets.len() as u32;
                        if !meter.allows(target.len() as u64) {
                            return RuleOutcome::OverBudget;
                        }
                        meter.spend(target.len() as u64);
                        ids.insert(target.clone(), id);
                        sets.push(target);
                        id
                    }
                };
                row.push((symbol, id));
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
        self.minimize(&moves, &accepting, meter)
    }

    /// Merges equivalent states of the determinized acceptor, drops the dead ones and compresses the symbols into classes.
    fn minimize(
        &self,
        moves: &[Vec<(u16, u32)>],
        accepting: &[bool],
        meter: &Meter,
    ) -> RuleOutcome {
        let count = moves.len();
        let sink = count;
        let mut symbols: Vec<u16> = moves
            .iter()
            .flat_map(|row| row.iter().map(|(symbol, _)| *symbol))
            .collect();
        symbols.sort_unstable();
        symbols.dedup();
        let step = |state: usize, symbol: u16| -> usize {
            if state == sink {
                return sink;
            }
            let row = &moves[state];
            match row.binary_search_by_key(&symbol, |(s, _)| *s) {
                Ok(i) => row[i].1 as usize,
                Err(_) => sink,
            }
        };
        let mut block: Vec<u32> = (0..=count)
            .map(|s| u32::from(s != sink && accepting[s]))
            .collect();
        let mut blocks = 2;
        let mut key: Vec<u32> = Vec::with_capacity(symbols.len() + 1);
        let round = ((count + 1) * (symbols.len() + 1)) as u64;
        loop {
            if !meter.allows(round) {
                return RuleOutcome::OverBudget;
            }
            let mut ids: FxHashMap<Vec<u32>, u32> = FxHashMap::default();
            let mut next_block = vec![0u32; count + 1];
            for state in 0..=count {
                key.clear();
                key.push(block[state]);
                if state == sink {
                    key.resize(symbols.len() + 1, block[sink]);
                } else {
                    let mut row = moves[state].iter().peekable();
                    for &symbol in &symbols {
                        let target = match row.next_if(|(s, _)| *s == symbol) {
                            Some((_, target)) => *target as usize,
                            None => sink,
                        };
                        key.push(block[target]);
                    }
                }
                next_block[state] = match ids.get(key.as_slice()) {
                    Some(id) => *id,
                    None => {
                        let fresh = ids.len() as u32;
                        ids.insert(key.clone(), fresh);
                        fresh
                    }
                };
            }
            meter.spend(round);
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
        let mut class_ids: FxHashMap<Vec<u32>, u16> = FxHashMap::default();
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
        let tail = (next.len() + class_of.len() + accepts.len()) as u64;
        if !meter.allows(tail) {
            return RuleOutcome::OverBudget;
        }
        meter.spend(tail);
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
