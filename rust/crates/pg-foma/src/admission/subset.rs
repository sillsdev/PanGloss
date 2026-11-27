//! Minimizing a non-deterministic net in compact tables: foma's determinized net, often a hundred times the net it reads and the minimal net it ends in, is never made; the reversed net gets a bounded try first, since determinizing a determinized reverse backwards gives the minimal net directly (Brzozowski) and cannot grow past it.

mod nets;
mod refine;
#[cfg(test)]
mod tests;

use super::meter::Meter;
use foma::constructions::{fsm_concat, fsm_count, fsm_merge_sigma};
use foma::options::FomaOptions;
use foma::sigma::{sigma_add_special, sigma_find_number};
use foma::types::{Fsm, EPSILON};
use nets::{Dfa, Nfa, Rows};
use rustc_hash::FxHasher;
use std::hash::Hasher;

/// Nets up to this many states plus arcs go to foma unchanged.
const SMALL: usize = 2_000;
/// Units the forward construction makes, as a multiple of the net it reads, before the reverse one gets its try.
const HEAD_START: u64 = 4;
/// Arcs read and subset members written that the reverse construction may spend, as a multiple of the net it reads.
const REVERSE_TRY: u64 = 16;
/// Units the forward construction makes between two looks at the meter.
const TURN: u64 = 1 << 16;
const NONE: u32 = u32::MAX;

/// A subset construction that can stop between states and go on later.
struct Subsets<'a> {
    nfa: &'a Nfa,
    /// The members of every subset, one after another; subset `s` is `members[member_at[s]..member_at[s + 1]]`.
    members: Vec<u32>,
    member_at: Vec<usize>,
    /// Open-addressed index from a subset's members to its number.
    index: Vec<u32>,
    rows: Rows,
    by_label: Vec<Vec<u32>>,
    touched: Vec<u32>,
    seen: Vec<u32>,
    stamp: u32,
    set: Vec<u32>,
    stack: Vec<u32>,
    row: Vec<(u32, u32)>,
    /// Arcs read so far.
    read: u64,
}

fn hash(set: &[u32]) -> usize {
    let mut hasher = FxHasher::default();
    for member in set {
        hasher.write_u32(*member);
    }
    hasher.finish() as usize
}

impl<'a> Subsets<'a> {
    fn new(nfa: &'a Nfa) -> Self {
        let mut subsets = Self {
            nfa,
            members: Vec::new(),
            member_at: vec![0],
            index: vec![NONE; 1024],
            rows: Rows::new(nfa.labels),
            by_label: (0..nfa.labels).map(|_| Vec::new()).collect(),
            touched: Vec::new(),
            seen: vec![0; nfa.accepting.len()],
            stamp: 0,
            set: Vec::new(),
            stack: Vec::new(),
            row: Vec::new(),
            read: 0,
        };
        let starts = nfa.starts.clone();
        subsets.close(&starts);
        subsets.number();
        subsets
    }

    fn done(&self) -> bool {
        self.rows.states() + 1 == self.member_at.len()
    }

    /// States plus arcs made so far.
    fn made(&self) -> u64 {
        (self.member_at.len() - 1 + self.rows.arcs()) as u64
    }

    /// Arcs read plus subset members written so far, which is what the construction costs to run.
    fn work(&self) -> u64 {
        self.read + self.members.len() as u64
    }

    fn subset(&self, number: usize) -> &[u32] {
        &self.members[self.member_at[number]..self.member_at[number + 1]]
    }

    /// Leaves in `set` the seeds and everything they reach without reading a symbol, sorted.
    fn close(&mut self, seeds: &[u32]) {
        if self.stamp == u32::MAX {
            self.seen.fill(0);
            self.stamp = 0;
        }
        self.stamp += 1;
        self.set.clear();
        let nfa = self.nfa;
        for seed in seeds {
            if std::mem::replace(&mut self.seen[*seed as usize], self.stamp) != self.stamp {
                self.set.push(*seed);
                self.stack.push(*seed);
            }
        }
        while let Some(state) = self.stack.pop() {
            for target in nfa.silent(state as usize) {
                if std::mem::replace(&mut self.seen[*target as usize], self.stamp) != self.stamp {
                    self.set.push(*target);
                    self.stack.push(*target);
                }
            }
        }
        self.set.sort_unstable();
    }

    /// The number of the subset in `set`, added when it is new.
    fn number(&mut self) -> u32 {
        let mask = self.index.len() - 1;
        let mut at = hash(&self.set) & mask;
        loop {
            match self.index[at] {
                NONE => break,
                number if self.subset(number as usize) == self.set.as_slice() => return number,
                _ => at = (at + 1) & mask,
            }
        }
        let number = (self.member_at.len() - 1) as u32;
        self.members.extend_from_slice(&self.set);
        self.member_at.push(self.members.len());
        self.index[at] = number;
        if self.member_at.len() * 2 > self.index.len() {
            self.grow();
        }
        number
    }

    fn grow(&mut self) {
        let mask = self.index.len() * 2 - 1;
        let mut index = vec![NONE; mask + 1];
        for number in 0..self.member_at.len() - 1 {
            let mut at = hash(self.subset(number)) & mask;
            while index[at] != NONE {
                at = (at + 1) & mask;
            }
            index[at] = number as u32;
        }
        self.index = index;
    }

    /// Expands the next state: one arc for every label its members read.
    fn expand(&mut self) {
        let state = self.rows.states();
        let nfa = self.nfa;
        for at in self.member_at[state]..self.member_at[state + 1] {
            let (labels, targets) = nfa.arcs(self.members[at] as usize);
            self.read += labels.len() as u64;
            for (label, target) in labels.iter().zip(targets) {
                let bucket = &mut self.by_label[*label as usize];
                if bucket.is_empty() {
                    self.touched.push(*label);
                }
                bucket.push(*target);
            }
        }
        self.touched.sort_unstable();
        self.row.clear();
        for at in 0..self.touched.len() {
            let label = self.touched[at];
            let mut seeds = std::mem::take(&mut self.by_label[label as usize]);
            self.close(&seeds);
            seeds.clear();
            self.by_label[label as usize] = seeds;
            let target = self.number();
            self.row.push((label, target));
        }
        self.touched.clear();
        self.rows.push(&self.row);
    }

    /// Expands states until `units` more are made or none is left; true when none is left.
    fn run(&mut self, units: u64) -> bool {
        let stop = self.made().saturating_add(units);
        while !self.done() && self.made() < stop {
            self.expand();
        }
        self.done()
    }

    /// Expands states until the construction has cost `work` or none is left; true when none is left.
    fn run_within(&mut self, work: u64) -> bool {
        while !self.done() && self.work() < work {
            self.expand();
        }
        self.done()
    }

    fn finish(self) -> Dfa {
        let nfa = self.nfa;
        let accepting = (0..self.member_at.len() - 1)
            .map(|number| {
                self.subset(number)
                    .iter()
                    .any(|s| nfa.accepting[*s as usize])
            })
            .collect();
        Dfa {
            labels: nfa.labels,
            rows: self.rows,
            accepting,
        }
    }
}

/// How the subset constructions ended.
enum Ended {
    /// The net determinized.
    Forward(Dfa),
    /// The reversed net determinized within its bound.
    Reverse(Dfa),
    Refused,
}

/// Charges what a construction made since `before`; false when that is past the budget.
fn charge(meter: &Meter, subsets: &Subsets, before: u64) -> bool {
    let made = subsets.made() - before;
    let fits = meter.allows(made);
    if fits {
        meter.spend(made);
    }
    fits
}

fn construct(meter: &Meter, nfa: &Nfa) -> Ended {
    let mut forward = Subsets::new(nfa);
    let done = forward.run(nfa.size().saturating_mul(HEAD_START));
    if !charge(meter, &forward, 0) {
        return Ended::Refused;
    }
    if !done {
        let reversed = nfa.reversed();
        let mut reverse = Subsets::new(&reversed);
        let ended = reverse.run_within(nfa.size().saturating_mul(REVERSE_TRY));
        // The try is charged whether it ends or not: the reversed net and what its construction made are work done.
        let made = nfa.size() + reverse.made();
        if !meter.allows(made) {
            return Ended::Refused;
        }
        meter.spend(made);
        if ended {
            return Ended::Reverse(reverse.finish());
        }
    }
    while !forward.done() {
        let before = forward.made();
        forward.run(TURN);
        if !charge(meter, &forward, before) {
            return Ended::Refused;
        }
    }
    Ended::Forward(forward.finish())
}

/// The minimal deterministic net of `nfa`; `None` when its language is empty, `Err` past the budget.
fn minimal(meter: &Meter, nfa: Nfa) -> Result<Option<Dfa>, ()> {
    match construct(meter, &nfa) {
        Ended::Refused => Err(()),
        Ended::Forward(dfa) => {
            drop(nfa);
            Ok(refine::refine(&dfa))
        }
        Ended::Reverse(dfa) => {
            drop(nfa);
            if !dfa.accepting.iter().any(|accepts| *accepts) {
                return Ok(None);
            }
            if !meter.allows(dfa.size()) {
                return Err(());
            }
            meter.spend(dfa.size());
            let back = dfa.reversed();
            drop(dfa);
            let mut subsets = Subsets::new(&back);
            subsets.run(u64::MAX);
            match charge(meter, &subsets, 0) {
                true => Ok(Some(subsets.finish())),
                false => Err(()),
            }
        }
    }
}

/// The minimal net of a counted non-deterministic `net`, charging what the subset constructions made, or an empty net past the budget; small nets and nets with an empty language go to `plain`, foma's own minimizing.
pub(super) fn minimize(meter: &Meter, net: Fsm, plain: impl FnOnce(Fsm) -> Fsm) -> Fsm {
    let small = net.statecount.max(0) as usize + net.arccount.max(0) as usize <= SMALL;
    if small || net.finalcount == 0 {
        return plain(net);
    }
    let (nfa, pairs) = Nfa::read(&net);
    match minimal(meter, nfa) {
        Err(()) => foma::structures::fsm_empty_set(),
        // foma decides what the net of an empty language looks like.
        Ok(None) => plain(net),
        Ok(Some(dfa)) => dfa.write(net, &pairs),
    }
}

/// The minimal net of `left` followed by `right`, charging what the subset constructions made, or an empty net past the budget.
pub(super) fn concat(meter: &Meter, opts: &FomaOptions, mut left: Fsm, mut right: Fsm) -> Fsm {
    fsm_count(&mut left);
    fsm_count(&mut right);
    let size = |net: &Fsm| net.statecount.max(0) as usize + net.arccount.max(0) as usize;
    let small = size(&left) + size(&right) <= SMALL;
    if small || !opts.minimal || left.finalcount == 0 || right.finalcount == 0 {
        return fsm_concat(opts, left, right);
    }
    fsm_merge_sigma(opts, &mut left, &mut right);
    let (nfa, pairs) = Nfa::chain(&left, &right);
    match minimal(meter, nfa) {
        Err(()) => foma::structures::fsm_empty_set(),
        Ok(None) => fsm_concat(opts, left, right),
        Ok(Some(dfa)) => {
            drop(right);
            if sigma_find_number(EPSILON, &left.sigma).is_none() {
                sigma_add_special(EPSILON, &mut left.sigma);
            }
            dfa.write(left, &pairs)
        }
    }
}
