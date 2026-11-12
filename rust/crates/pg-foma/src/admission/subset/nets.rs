//! The compact nets the subset construction reads and makes, and their reading from and writing to foma nets.

use foma::constructions::{fsm_count, fsm_update_flags};
use foma::dynarray::{
    fsm_state_add_arc, fsm_state_close, fsm_state_end_state, fsm_state_init,
    fsm_state_set_current_state,
};
use foma::sigma::sigma_max;
use foma::types::{Fsm, EPSILON, UNK, YES};
use rustc_hash::FxHashMap;

/// Most arcs one block of [`Rows`] holds; a growing table adds blocks and never copies what it has.
const BLOCK: usize = 1 << 18;
const FIRST_BLOCK: usize = 1 << 10;

/// The labels of the arcs in one block of [`Rows`], two bytes each when the net has few enough labels.
enum Labels {
    Narrow(Vec<u16>),
    Wide(Vec<u32>),
}

impl Labels {
    fn len(&self) -> usize {
        match self {
            Self::Narrow(labels) => labels.len(),
            Self::Wide(labels) => labels.len(),
        }
    }

    fn room(&self) -> usize {
        match self {
            Self::Narrow(labels) => labels.capacity(),
            Self::Wide(labels) => labels.capacity(),
        }
    }

    fn at(&self, index: usize) -> u32 {
        match self {
            Self::Narrow(labels) => u32::from(labels[index]),
            Self::Wide(labels) => labels[index],
        }
    }
}

/// The arcs of states added one after another: a label and a target per arc, in blocks no row crosses.
pub(super) struct Rows {
    wide: bool,
    labels: Vec<Labels>,
    targets: Vec<Vec<u32>>,
    /// Block, offset in the block and length of every row.
    at: Vec<(u32, u32, u32)>,
    arcs: usize,
}

impl Rows {
    /// An empty table for a net with `labels` labels.
    pub(super) fn new(labels: usize) -> Self {
        Self {
            wide: labels > usize::from(u16::MAX) + 1,
            labels: Vec::new(),
            targets: Vec::new(),
            at: Vec::new(),
            arcs: 0,
        }
    }

    pub(super) fn states(&self) -> usize {
        self.at.len()
    }

    pub(super) fn arcs(&self) -> usize {
        self.arcs
    }

    pub(super) fn push(&mut self, row: &[(u32, u32)]) {
        let fits = self
            .labels
            .last()
            .is_some_and(|block| block.len() + row.len() <= block.room());
        if !fits {
            let last = self.labels.last().map_or(0, Labels::room);
            let room = (last * 2).clamp(FIRST_BLOCK, BLOCK).max(row.len());
            self.labels.push(match self.wide {
                true => Labels::Wide(Vec::with_capacity(room)),
                false => Labels::Narrow(Vec::with_capacity(room)),
            });
            self.targets.push(Vec::with_capacity(room));
        }
        let block = self.labels.len() - 1;
        let offset = self.targets[block].len();
        match &mut self.labels[block] {
            Labels::Narrow(labels) => labels.extend(row.iter().map(|(label, _)| *label as u16)),
            Labels::Wide(labels) => labels.extend(row.iter().map(|(label, _)| *label)),
        }
        self.targets[block].extend(row.iter().map(|(_, target)| *target));
        self.at
            .push((block as u32, offset as u32, row.len() as u32));
        self.arcs += row.len();
    }

    /// The label and target of every arc of a state, in label order.
    pub(super) fn row(&self, state: usize) -> impl Iterator<Item = (u32, u32)> + '_ {
        let (block, offset, len) = self.at[state];
        let labels = &self.labels[block as usize];
        let targets = &self.targets[block as usize];
        (offset as usize..(offset + len) as usize).map(move |at| (labels.at(at), targets[at]))
    }
}

/// A net with arcs grouped by source state; a label is the index of an upper and lower symbol pair.
pub(super) struct Nfa {
    arc_at: Vec<usize>,
    arc_labels: Vec<u32>,
    arc_targets: Vec<u32>,
    silent_at: Vec<usize>,
    silent: Vec<u32>,
    pub(super) starts: Vec<u32>,
    pub(super) accepting: Vec<bool>,
    pub(super) labels: usize,
}

/// The arcs of one or more nets read into one numbering of states and labels.
#[derive(Default)]
struct Parts {
    pairs: Vec<(i16, i16)>,
    label_of: FxHashMap<(i16, i16), u32>,
    arcs: Vec<(u32, u32, u32)>,
    silent: Vec<(u32, u32)>,
    states: u32,
}

impl Parts {
    /// Adds a counted net after the states already read; its start states and which of its states accept.
    fn add(&mut self, net: &Fsm) -> (Vec<u32>, Vec<bool>) {
        let first = self.states;
        let states = net.statecount.max(0) as usize;
        let mut starts = Vec::new();
        let mut accepting = vec![false; states];
        self.arcs.reserve(net.arccount.max(0) as usize);
        for (block, block_arcs) in net.states.iter_blocks() {
            let state = block.state_no as u32;
            accepting[state as usize] = block.final_state != 0;
            if block.start_state != 0 {
                starts.push(first + state);
            }
            for arc in block_arcs.iter().filter(|arc| arc.target >= 0) {
                let target = first + arc.target as u32;
                if i32::from(arc.r#in) == EPSILON && i32::from(arc.out) == EPSILON {
                    self.silent.push((first + state, target));
                    continue;
                }
                let label = match self.label_of.get(&(arc.r#in, arc.out)) {
                    Some(label) => *label,
                    None => {
                        let label = self.pairs.len() as u32;
                        self.pairs.push((arc.r#in, arc.out));
                        self.label_of.insert((arc.r#in, arc.out), label);
                        label
                    }
                };
                self.arcs.push((first + state, label, target));
            }
        }
        self.states += states as u32;
        (starts, accepting)
    }
}

/// Groups `(source, item)` pairs by source: an offset per source and the sources' order of the pairs.
fn group<T>(states: usize, pairs: &[T], source: impl Fn(&T) -> u32) -> (Vec<usize>, Vec<u32>) {
    let mut at = vec![0usize; states + 1];
    for pair in pairs {
        at[source(pair) as usize + 1] += 1;
    }
    for state in 0..states {
        at[state + 1] += at[state];
    }
    let mut next = at.clone();
    let mut order = vec![0u32; pairs.len()];
    for (index, pair) in pairs.iter().enumerate() {
        let slot = &mut next[source(pair) as usize];
        order[*slot] = index as u32;
        *slot += 1;
    }
    (at, order)
}

impl Nfa {
    /// Arcs with a source, a label and a target, and arcs that read nothing, into a net.
    pub(super) fn new(
        accepting: Vec<bool>,
        starts: Vec<u32>,
        labels: usize,
        arcs: &[(u32, u32, u32)],
        silent: &[(u32, u32)],
    ) -> Self {
        let states = accepting.len();
        let (arc_at, order) = group(states, arcs, |arc| arc.0);
        let (silent_at, silent_order) = group(states, silent, |arc| arc.0);
        Self {
            arc_at,
            arc_labels: order.iter().map(|i| arcs[*i as usize].1).collect(),
            arc_targets: order.iter().map(|i| arcs[*i as usize].2).collect(),
            silent_at,
            silent: silent_order.iter().map(|i| silent[*i as usize].1).collect(),
            starts,
            accepting,
            labels,
        }
    }

    /// States plus arcs.
    pub(super) fn size(&self) -> u64 {
        (self.accepting.len() + self.arc_targets.len() + self.silent.len()) as u64
    }

    pub(super) fn arcs(&self, state: usize) -> (&[u32], &[u32]) {
        let range = self.arc_at[state]..self.arc_at[state + 1];
        (&self.arc_labels[range.clone()], &self.arc_targets[range])
    }

    pub(super) fn silent(&self, state: usize) -> &[u32] {
        &self.silent[self.silent_at[state]..self.silent_at[state + 1]]
    }

    /// Reads a counted net, with its symbol pairs in label order.
    pub(super) fn read(net: &Fsm) -> (Self, Vec<(i16, i16)>) {
        let mut parts = Parts::default();
        let (starts, accepting) = parts.add(net);
        let nfa = Self::new(
            accepting,
            starts,
            parts.pairs.len(),
            &parts.arcs,
            &parts.silent,
        );
        (nfa, parts.pairs)
    }

    /// Reads two counted nets over one alphabet as the first followed by the second.
    pub(super) fn chain(first: &Fsm, second: &Fsm) -> (Self, Vec<(i16, i16)>) {
        let mut parts = Parts::default();
        let (starts, ends) = parts.add(first);
        let (continues, accepts) = parts.add(second);
        for (state, _) in ends.iter().enumerate().filter(|(_, ends)| **ends) {
            for target in &continues {
                parts.silent.push((state as u32, *target));
            }
        }
        let mut accepting = vec![false; ends.len()];
        accepting.extend(accepts);
        let nfa = Self::new(
            accepting,
            starts,
            parts.pairs.len(),
            &parts.arcs,
            &parts.silent,
        );
        (nfa, parts.pairs)
    }

    /// The net read backwards: it starts where this one accepts and accepts where this one starts.
    pub(super) fn reversed(&self) -> Self {
        let states = self.accepting.len();
        let mut arcs: Vec<(u32, u32, u32)> = Vec::with_capacity(self.arc_targets.len());
        let mut silent: Vec<(u32, u32)> = Vec::with_capacity(self.silent.len());
        for state in 0..states {
            let (labels, targets) = self.arcs(state);
            for (label, target) in labels.iter().zip(targets) {
                arcs.push((*target, *label, state as u32));
            }
            for target in self.silent(state) {
                silent.push((*target, state as u32));
            }
        }
        let mut accepting = vec![false; states];
        for start in &self.starts {
            accepting[*start as usize] = true;
        }
        let starts = (0..states as u32)
            .filter(|s| self.accepting[*s as usize])
            .collect();
        Self::new(accepting, starts, self.labels, &arcs, &silent)
    }
}

/// A deterministic net: state 0 is the start, and every state is reachable.
pub(super) struct Dfa {
    /// Labels the net it was made from has.
    pub(super) labels: usize,
    pub(super) rows: Rows,
    pub(super) accepting: Vec<bool>,
}

impl Dfa {
    /// States plus arcs.
    pub(super) fn size(&self) -> u64 {
        (self.accepting.len() + self.rows.arcs()) as u64
    }

    /// The reverse as a net: it starts in the accepting states and accepts in the start.
    pub(super) fn reversed(&self) -> Nfa {
        let states = self.accepting.len();
        let mut arcs: Vec<(u32, u32, u32)> = Vec::with_capacity(self.rows.arcs());
        for state in 0..states {
            for (label, target) in self.rows.row(state) {
                arcs.push((target, label, state as u32));
            }
        }
        let mut accepting = vec![false; states];
        accepting[0] = true;
        let starts = (0..states as u32)
            .filter(|s| self.accepting[*s as usize])
            .collect();
        Nfa::new(accepting, starts, self.labels, &arcs, &[])
    }

    /// Writes the states into `net` in place of its own, keeping its alphabet, as the minimal net foma would mark.
    pub(super) fn write(&self, mut net: Fsm, pairs: &[(i16, i16)]) -> Fsm {
        let mut builder = fsm_state_init(sigma_max(&net.sigma));
        net.states = Vec::new().into();
        for state in 0..self.accepting.len() {
            let number = state as i32;
            let (accepts, starts) = (i32::from(self.accepting[state]), i32::from(state == 0));
            fsm_state_set_current_state(&mut builder, number, accepts, starts);
            for (label, target) in self.rows.row(state) {
                let (upper, lower) = pairs[label as usize];
                let (upper, lower, target) = (i32::from(upper), i32::from(lower), target as i32);
                fsm_state_add_arc(&mut builder, number, upper, lower, target, accepts, starts);
            }
            fsm_state_end_state(&mut builder);
        }
        fsm_state_close(&mut builder, &mut net);
        fsm_count(&mut net);
        fsm_update_flags(&mut net, YES, YES, YES, YES, UNK, UNK);
        net
    }
}
