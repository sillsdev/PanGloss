//! Pin-group read-out of the product's [`TrimmedGraph`] (plan S.6, section 4; W4).
//!
//! Only [`ReadoutMode::Single`] is built (plan revision 7; `Split`, `Auto` and the bypass rule are
//! demoted to Appendix A).
//!
//! # Input graph
//!
//! [`read_out`] consumes the product graph directly; there is no second graph type. The product
//! is built for one word, so the word position is part of every state key and the read-out never
//! looks at characters:
//!
//! - the position of a state is `key[0]`; an arc whose destination position is one more than its
//!   source consumes one word token, an arc that keeps the position writes nothing. Every product
//!   move consumes at most one token, so there is nothing to split; an arc that goes back or jumps
//!   more than one position is a graph the product cannot produce, and [`ReadoutIndex::new`]
//!   panics naming it (a W5 layer that wrote several tokens in one move would have to split it);
//! - a state is accepting when `accepting[s]`; `start` is `None` for a word proven out of the
//!   language, which reads out as zero groups;
//! - parallel arcs (same endpoints, different weight or edits) have their tags UNIONED: the
//!   read-out takes the union over every arc that lies on an accepting walk, never one arc's;
//! - tags are packed [`Tag`](super::layer::Tag) words on arcs only (plan 2.0), decoded by `Tag::kind`: head roots
//!   and rules are pinned, non-head entries are explicitly not (root tags are head root entries
//!   only, plan 4.1), and a word that is none of them makes the read-out return
//!   [`Readout::BadTag`]; nothing is ever treated as "other";
//! - with a weight ceiling the product keeps every arc that passes its own creation test, so some
//!   arcs lie only on accepting walks above it. The graph records that ceiling
//!   ([`TrimmedGraph::theta`]) and the read-out keeps an arc only when the least weight of an
//!   accepting walk through it is at most the ceiling. A weight clamped by the product only
//!   underestimates, so the result can only be wider than exact, never narrower.
//!
//! # What is computed
//!
//! A forward sweep from the start and a backward sweep from the accepting states, both
//! least-weight (Dijkstra) over SPARSE maps of the states they reach. `roots` and `rules` are the
//! tags on arcs whose source the forward sweep reached and whose destination the backward sweep
//! reached, self-loops included, within `theta`. That is the union over **all** accepting walks,
//! never one walk (mutation C95). The result is zero groups when the word is not accepted and
//! exactly one group when it is (a group with empty sets is a real answer: accepted, nothing to
//! pin).
//!
//! # Budget
//!
//! Every set entry, queue entry, arc visit, collected id and the [`ReadoutIndex`] itself is charged
//! to a [`ReadoutBudget`] before it is stored. An overrun returns [`Readout::Incomplete`] naming
//! the dimension and no groups; it never returns a partial group.
//!
//! The read-out has its own budget, not the word's product budget, because it runs afterwards:
//! once per string, on the finished graph, when the product's meter and tables are gone. The
//! graph it reads was already paid for by the product (states, arcs, tag sets and the trim), so
//! this budget bounds only what the read-out adds, and a caller that wants one figure per word
//! adds the two `used.bytes`.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use pg_grammar::model::{LexEntryId, MRuleId};

use super::layer::{BadTag, TagKind};
use super::trim::{TrimmedGraph, KEY_PREFIX};

/// Read-out mode. Only `Single` exists; the other variants are demoted (Appendix A).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReadoutMode {
    /// One group: all root tags and all rule tags of the accepting paths of the word.
    #[default]
    Single,
}

/// Local mirror of `confirm::PinGroup` (PR #419, not on main yet): swap it when that lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PinGroup {
    /// Head root entries only.
    pub roots: HashSet<LexEntryId>,
    pub rules: HashSet<MRuleId>,
}

/// The dimension that ended a read-out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReadoutStop {
    /// States reached by the two sweeps.
    ReadoutNodes,
    /// Arc visits.
    ReadoutSteps,
    /// Nominal bytes of the sets, queue entries and collected ids.
    ReadoutBytes,
}

/// Per-read-out limits. Nominal bytes (fixed figures), so a figure is the same on any machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadoutBudget {
    pub nodes: u64,
    pub steps: u64,
    pub bytes: u64,
}

impl ReadoutBudget {
    pub const UNLIMITED: ReadoutBudget = ReadoutBudget {
        nodes: u64::MAX,
        steps: u64::MAX,
        bytes: u64::MAX,
    };
}

impl Default for ReadoutBudget {
    /// Twice the largest product word budget (4,000,000 states, 32,000,000 arcs, 1 GiB).
    fn default() -> Self {
        ReadoutBudget {
            nodes: 8_000_000,
            steps: 128_000_000,
            bytes: 1 << 30,
        }
    }
}

/// What a read-out used, for the budget tests and reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadoutUsed {
    pub nodes: u64,
    pub steps: u64,
    pub bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Readout {
    /// A tag word that no tag was built as: the graph is corrupt, so there is no answer.
    BadTag(BadTag),
    /// Zero groups (not accepted) or exactly one.
    Complete {
        groups: Vec<PinGroup>,
        used: ReadoutUsed,
    },
    Incomplete {
        stop: ReadoutStop,
        used: ReadoutUsed,
    },
}

/// One reached state: its set entry and its least-weight map entry.
pub const NODE_BYTES: u64 = 32;
pub const QUEUE_ENTRY_BYTES: u64 = 24;
/// One collected root or rule id.
pub const ID_BYTES: u64 = 16;

/// Running counters against a [`ReadoutBudget`].
pub struct ReadoutMeter {
    limits: ReadoutBudget,
    used: ReadoutUsed,
}

impl ReadoutMeter {
    pub fn new(limits: ReadoutBudget) -> Self {
        ReadoutMeter {
            limits,
            used: ReadoutUsed::default(),
        }
    }

    pub fn used(&self) -> ReadoutUsed {
        self.used
    }

    fn charge_node(&mut self) -> Result<(), ReadoutStop> {
        if self.used.nodes >= self.limits.nodes {
            return Err(ReadoutStop::ReadoutNodes);
        }
        self.charge_bytes(NODE_BYTES)?;
        self.used.nodes += 1;
        Ok(())
    }

    fn charge_step(&mut self) -> Result<(), ReadoutStop> {
        if self.used.steps >= self.limits.steps {
            return Err(ReadoutStop::ReadoutSteps);
        }
        self.used.steps += 1;
        Ok(())
    }

    fn charge_bytes(&mut self, n: u64) -> Result<(), ReadoutStop> {
        if self.used.bytes.saturating_add(n) > self.limits.bytes {
            return Err(ReadoutStop::ReadoutBytes);
        }
        self.used.bytes += n;
        Ok(())
    }
}

/// Nominal bytes of the index of a graph with `states` states and `arcs` arcs: two list headers
/// per state and two arc ids per arc.
pub const fn index_bytes(states: usize, arcs: usize) -> u64 {
    states as u64 * 48 + arcs as u64 * 8
}

/// Adjacency lists of a graph, built once and reused for every read-out of it.
pub struct ReadoutIndex {
    states: usize,
    arcs: usize,
    out_arcs: Vec<Vec<u32>>,
    in_arcs: Vec<Vec<u32>>,
}

impl ReadoutIndex {
    /// Charges [`index_bytes`] before building. Panics with a message naming the offending part
    /// when the graph is not one the product can produce: the start or an arc end outside the
    /// states, a key table that does not match the states, a tag set id outside the table, or an
    /// arc that does not move the position by 0 or 1.
    pub fn new(graph: &TrimmedGraph, meter: &mut ReadoutMeter) -> Result<Self, ReadoutStop> {
        let n = graph.state_count();
        let width = KEY_PREFIX + graph.layers;
        assert_eq!(
            graph.keys.len(),
            n * width,
            "TrimmedGraph has {n} states but {} key words (width {width})",
            graph.keys.len()
        );
        if let Some(s) = graph.start {
            assert!((s as usize) < n, "TrimmedGraph start {s} is outside 0..{n}");
        }
        meter.charge_bytes(index_bytes(n, graph.arcs.len()))?;
        let mut out_arcs: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut in_arcs: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (k, a) in graph.arcs.iter().enumerate() {
            assert!(
                (a.src as usize) < n && (a.dst as usize) < n,
                "TrimmedGraph arc {k} ({} -> {}) names a state outside 0..{n}",
                a.src,
                a.dst
            );
            assert!(
                (a.tags as usize) < graph.tag_sets.len(),
                "TrimmedGraph arc {k} names tag set {} of {}",
                a.tags,
                graph.tag_sets.len()
            );
            let (p, q) = (graph.key(a.src)[0], graph.key(a.dst)[0]);
            assert!(
                q == p || q == p + 1,
                "TrimmedGraph arc {k} ({} -> {}) moves the position {p} -> {q}: split moves that consume several tokens",
                a.src,
                a.dst
            );
            out_arcs[a.src as usize].push(k as u32);
            in_arcs[a.dst as usize].push(k as u32);
        }
        Ok(Self {
            states: n,
            arcs: graph.arcs.len(),
            out_arcs,
            in_arcs,
        })
    }
}

/// Pin groups of the word `graph` was built for, under the weight ceiling it records. Builds the
/// index (charged to `budget`); use [`read_out_indexed`] to reuse one.
pub fn read_out(graph: &TrimmedGraph, budget: ReadoutBudget, mode: ReadoutMode) -> Readout {
    let mut meter = ReadoutMeter::new(budget);
    match ReadoutIndex::new(graph, &mut meter) {
        Ok(index) => run(graph, &index, &mut meter, mode),
        Err(stop) => Readout::Incomplete {
            stop,
            used: meter.used,
        },
    }
}

/// As [`read_out`], with a prebuilt [`ReadoutIndex`] (whose cost its builder charged); panics when
/// the index is of another graph.
pub fn read_out_indexed(
    graph: &TrimmedGraph,
    index: &ReadoutIndex,
    budget: ReadoutBudget,
    mode: ReadoutMode,
) -> Readout {
    run(graph, index, &mut ReadoutMeter::new(budget), mode)
}

enum Fail {
    Stop(ReadoutStop),
    Bad(BadTag),
}

impl From<ReadoutStop> for Fail {
    fn from(s: ReadoutStop) -> Self {
        Fail::Stop(s)
    }
}

fn run(
    graph: &TrimmedGraph,
    index: &ReadoutIndex,
    meter: &mut ReadoutMeter,
    mode: ReadoutMode,
) -> Readout {
    assert!(
        index.states == graph.state_count() && index.arcs == graph.arcs.len(),
        "ReadoutIndex is of another graph ({} states, {} arcs; this graph has {}, {})",
        index.states,
        index.arcs,
        graph.state_count(),
        graph.arcs.len()
    );
    let r = match mode {
        ReadoutMode::Single => read_out_single(graph, index, meter),
    };
    match r {
        Ok(groups) => Readout::Complete {
            groups,
            used: meter.used,
        },
        Err(Fail::Stop(stop)) => Readout::Incomplete {
            stop,
            used: meter.used,
        },
        Err(Fail::Bad(b)) => Readout::BadTag(b),
    }
}

/// Least weight from the seeds, along the arcs or against them, over the states reached only.
fn sweep(
    graph: &TrimmedGraph,
    index: &ReadoutIndex,
    forward: bool,
    seeds: impl Iterator<Item = u32>,
    meter: &mut ReadoutMeter,
) -> Result<HashMap<u32, u64>, ReadoutStop> {
    let mut dist: HashMap<u32, u64> = HashMap::new();
    let mut heap: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();
    for s in seeds {
        if let std::collections::hash_map::Entry::Vacant(e) = dist.entry(s) {
            meter.charge_node()?;
            meter.charge_bytes(QUEUE_ENTRY_BYTES)?;
            e.insert(0);
            heap.push(Reverse((0, s)));
        }
    }
    let adj = if forward {
        &index.out_arcs
    } else {
        &index.in_arcs
    };
    while let Some(Reverse((d, s))) = heap.pop() {
        if dist.get(&s).is_some_and(|&best| d > best) {
            continue;
        }
        for &k in &adj[s as usize] {
            meter.charge_step()?;
            let a = &graph.arcs[k as usize];
            let next = if forward { a.dst } else { a.src };
            let nd = d + a.weight as u64;
            match dist.get(&next) {
                Some(&old) if old <= nd => {}
                Some(_) => {
                    meter.charge_bytes(QUEUE_ENTRY_BYTES)?;
                    dist.insert(next, nd);
                    heap.push(Reverse((nd, next)));
                }
                None => {
                    meter.charge_node()?;
                    meter.charge_bytes(QUEUE_ENTRY_BYTES)?;
                    dist.insert(next, nd);
                    heap.push(Reverse((nd, next)));
                }
            }
        }
    }
    Ok(dist)
}

fn read_out_single(
    graph: &TrimmedGraph,
    index: &ReadoutIndex,
    meter: &mut ReadoutMeter,
) -> Result<Vec<PinGroup>, Fail> {
    // every tag set is decoded up front: one bad word anywhere is a corrupt graph, not an answer
    let mut decoded: Vec<Vec<TagKind>> = Vec::with_capacity(graph.tag_sets.len());
    for set in &graph.tag_sets {
        meter.charge_bytes(ID_BYTES * set.len() as u64)?;
        decoded.push(
            set.iter()
                .map(|t| t.kind())
                .collect::<Result<_, _>>()
                .map_err(Fail::Bad)?,
        );
    }
    let theta = graph.theta;
    let Some(start) = graph.start else {
        return Ok(Vec::new());
    };
    let fwd = sweep(graph, index, true, std::iter::once(start), meter)?;
    let accepting = (0..graph.state_count() as u32).filter(|&s| graph.accepting[s as usize]);
    let bwd = sweep(graph, index, false, accepting, meter)?;
    let within = |w: u64| theta.is_none_or(|t| w <= t as u64);
    match bwd.get(&start) {
        Some(&best) if fwd.contains_key(&start) && within(best) => {}
        _ => return Ok(Vec::new()),
    }
    let mut group = PinGroup::default();
    for a in &graph.arcs {
        let (Some(&g), Some(&h)) = (fwd.get(&a.src), bwd.get(&a.dst)) else {
            continue;
        };
        if !within(g + a.weight as u64 + h) {
            continue;
        }
        for &k in &decoded[a.tags as usize] {
            match k {
                TagKind::Root(e) if !group.roots.contains(&LexEntryId(e)) => {
                    meter.charge_bytes(ID_BYTES)?;
                    group.roots.insert(LexEntryId(e));
                }
                TagKind::Rule(m) if !group.rules.contains(&MRuleId(m)) => {
                    meter.charge_bytes(ID_BYTES)?;
                    group.rules.insert(MRuleId(m));
                }
                TagKind::Root(_) | TagKind::Rule(_) | TagKind::NonHead(_) => {}
            }
        }
    }
    Ok(vec![group])
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::super::budget::BuildMeter;
    use super::super::layer::{Input, NfaBuilder, Output, Tag};
    use super::super::product::{Outcome, Product, ProductScratch, Request};
    use super::super::trim::{trim, GraphArc, RawGraph};
    use super::*;

    /// A state machine with written characters: the shape the product graph is unrolled from.
    #[derive(Clone, Debug, Default)]
    struct Auto {
        accepting: Vec<u32>,
        arcs: Vec<AArc>,
    }

    #[derive(Clone, Debug)]
    struct AArc {
        src: u32,
        dst: u32,
        written: Option<char>,
        weight: u32,
        roots: Vec<u32>,
        rules: Vec<u32>,
        other: bool,
    }

    fn meter() -> ReadoutMeter {
        ReadoutMeter::new(ReadoutBudget::UNLIMITED)
    }

    fn arc(src: u32, dst: u32, written: Option<char>, roots: &[u32], rules: &[u32]) -> AArc {
        AArc {
            src,
            dst,
            written,
            weight: 0,
            roots: roots.to_vec(),
            rules: rules.to_vec(),
            other: false,
        }
    }

    fn auto(_states: u32, accepting: &[u32], arcs: Vec<AArc>) -> Auto {
        Auto {
            accepting: accepting.to_vec(),
            arcs,
        }
    }

    fn group(roots: &[u32], rules: &[u32]) -> PinGroup {
        PinGroup {
            roots: roots.iter().map(|&r| LexEntryId(r)).collect(),
            rules: rules.iter().map(|&r| MRuleId(r)).collect(),
        }
    }

    fn tag_ids(a: &AArc) -> BTreeSet<u32> {
        let mut t: BTreeSet<u32> = a.roots.iter().map(|&r| Tag::root(r).0).collect();
        t.extend(a.rules.iter().map(|&r| Tag::rule(r).0));
        if a.other {
            t.insert(Tag::nonhead(9).0);
        }
        t
    }

    /// The product graph of `a` over `s`: keys `[position, 0, state]`, arcs merged per `(src, dst, weight)`.
    fn unroll(a: &Auto, s: &str, theta: Option<u32>) -> RawGraph {
        let chars: Vec<char> = s.chars().collect();
        let mut ids: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        let mut order: Vec<(u32, u32)> = vec![(0, 0)];
        ids.insert((0, 0), 0);
        let mut merged: BTreeMap<(u32, u32, u32), BTreeSet<u32>> = BTreeMap::new();
        let mut next = 0;
        while next < order.len() {
            let (i, q) = order[next];
            let src = next as u32;
            next += 1;
            for e in a.arcs.iter().filter(|e| e.src == q) {
                let j = match e.written {
                    None => i,
                    Some(c) if (i as usize) < chars.len() && chars[i as usize] == c => i + 1,
                    Some(_) => continue,
                };
                let dst = *ids.entry((j, e.dst)).or_insert_with(|| {
                    order.push((j, e.dst));
                    order.len() as u32 - 1
                });
                merged
                    .entry((src, dst, e.weight))
                    .or_default()
                    .extend(tag_ids(e));
            }
        }
        let mut tag_sets: Vec<Box<[Tag]>> = Vec::new();
        let mut arcs = Vec::new();
        for ((src, dst, weight), t) in merged {
            let set: Box<[Tag]> = t.into_iter().map(Tag).collect();
            let id = match tag_sets.iter().position(|x| *x == set) {
                Some(p) => p,
                None => {
                    tag_sets.push(set);
                    tag_sets.len() - 1
                }
            } as u32;
            arcs.push(GraphArc {
                src,
                dst,
                weight,
                edits: 0,
                tags: id,
            });
        }
        RawGraph {
            layers: 1,
            word_len: chars.len(),
            theta,
            keys: order.iter().flat_map(|&(i, q)| [i, 0, q]).collect(),
            accepting: order
                .iter()
                .map(|&(i, q)| i as usize == chars.len() && a.accepting.contains(&q))
                .collect(),
            arcs,
            tag_sets,
        }
    }

    /// The same graph without the trim, as a consumer that does not trust its input would see it.
    fn untrimmed(raw: RawGraph) -> TrimmedGraph {
        TrimmedGraph {
            layers: raw.layers,
            word_len: raw.word_len,
            theta: raw.theta,
            start: Some(0),
            keys: raw.keys,
            accepting: raw.accepting,
            arcs: raw.arcs,
            tag_sets: raw.tag_sets,
        }
    }

    fn clone_raw(r: &RawGraph) -> RawGraph {
        RawGraph {
            layers: r.layers,
            word_len: r.word_len,
            theta: r.theta,
            keys: r.keys.clone(),
            accepting: r.accepting.clone(),
            arcs: r.arcs.clone(),
            tag_sets: r.tag_sets.clone(),
        }
    }

    fn groups(g: &TrimmedGraph) -> Vec<PinGroup> {
        match read_out(g, ReadoutBudget::UNLIMITED, ReadoutMode::Single) {
            Readout::Complete { groups, .. } => groups,
            o => panic!("{o:?}"),
        }
    }

    /// Read out `a` over `s` from the trimmed graph and from the untrimmed one; they must agree.
    fn ro(a: &Auto, s: &str, theta: Option<u32>) -> Vec<PinGroup> {
        let raw = unroll(a, s, theta);
        let loose = groups(&untrimmed(clone_raw(&raw)));
        let tight = groups(&trim(raw));
        assert_eq!(loose, tight, "trimmed and untrimmed graphs disagree");
        tight
    }

    /// Every walk spelling `s` within `theta`, each node at most twice (covers the cheapest walk through every arc).
    fn oracle(a: &Auto, s: &str, theta: Option<u32>) -> Vec<PinGroup> {
        let chars: Vec<char> = s.chars().collect();
        let mut visits: BTreeMap<(usize, u32), u8> = BTreeMap::new();
        let mut path: Vec<usize> = Vec::new();
        let mut walks: Vec<Vec<usize>> = Vec::new();
        #[allow(clippy::too_many_arguments)]
        fn go(
            a: &Auto,
            chars: &[char],
            theta: Option<u32>,
            i: usize,
            u: u32,
            weight: u64,
            visits: &mut BTreeMap<(usize, u32), u8>,
            path: &mut Vec<usize>,
            walks: &mut Vec<Vec<usize>>,
        ) {
            if theta.is_some_and(|t| weight > t as u64) {
                return;
            }
            let seen = visits.entry((i, u)).or_insert(0);
            if *seen >= 2 {
                return;
            }
            *seen += 1;
            if i == chars.len() && a.accepting.contains(&u) {
                walks.push(path.clone());
            }
            for (k, e) in a.arcs.iter().enumerate() {
                if e.src != u {
                    continue;
                }
                let j = match e.written {
                    None => i,
                    Some(c) if i < chars.len() && chars[i] == c => i + 1,
                    Some(_) => continue,
                };
                path.push(k);
                go(
                    a,
                    chars,
                    theta,
                    j,
                    e.dst,
                    weight + e.weight as u64,
                    visits,
                    path,
                    walks,
                );
                path.pop();
            }
            *visits.get_mut(&(i, u)).expect("entered above") -= 1;
        }
        go(
            a,
            &chars,
            theta,
            0,
            0,
            0,
            &mut visits,
            &mut path,
            &mut walks,
        );
        if walks.is_empty() {
            return Vec::new();
        }
        let mut pg = PinGroup::default();
        for w in &walks {
            for &k in w {
                pg.roots
                    .extend(a.arcs[k].roots.iter().map(|&r| LexEntryId(r)));
                pg.rules.extend(a.arcs[k].rules.iter().map(|&r| MRuleId(r)));
            }
        }
        vec![pg]
    }

    #[test]
    #[should_panic(expected = "start 5 is outside 0..2")]
    fn start_outside_the_states_panics_clearly() {
        let mut g = trim(unroll(
            &auto(2, &[1], vec![arc(0, 1, Some('a'), &[], &[])]),
            "a",
            None,
        ));
        g.start = Some(5);
        let _ = ReadoutIndex::new(&g, &mut meter());
    }

    #[test]
    #[should_panic(expected = "outside 0..2")]
    fn arc_outside_the_states_panics_clearly() {
        let mut g = trim(unroll(
            &auto(2, &[1], vec![arc(0, 1, Some('a'), &[], &[])]),
            "a",
            None,
        ));
        g.arcs[0].dst = 7;
        let _ = ReadoutIndex::new(&g, &mut meter());
    }

    #[test]
    #[should_panic(expected = "key words")]
    fn key_table_that_does_not_match_the_states_panics_clearly() {
        let mut g = trim(unroll(
            &auto(2, &[1], vec![arc(0, 1, Some('a'), &[], &[])]),
            "a",
            None,
        ));
        g.accepting.push(true);
        let _ = ReadoutIndex::new(&g, &mut meter());
    }

    #[test]
    #[should_panic(expected = "split moves")]
    fn arc_that_consumes_two_tokens_panics_clearly() {
        let mut g = trim(unroll(
            &auto(
                3,
                &[2],
                vec![
                    arc(0, 1, Some('a'), &[], &[]),
                    arc(1, 2, Some('b'), &[], &[]),
                ],
            ),
            "ab",
            None,
        ));
        g.arcs[0].dst = 2;
        let _ = ReadoutIndex::new(&g, &mut meter());
    }

    #[test]
    #[should_panic(expected = "tag set")]
    fn tag_set_outside_the_table_panics_clearly() {
        let mut g = trim(unroll(
            &auto(2, &[1], vec![arc(0, 1, Some('a'), &[], &[])]),
            "a",
            None,
        ));
        g.arcs[0].tags = 9;
        let _ = ReadoutIndex::new(&g, &mut meter());
    }

    #[test]
    #[should_panic(expected = "another graph")]
    fn index_of_another_graph_is_refused() {
        let a = trim(unroll(
            &auto(2, &[1], vec![arc(0, 1, Some('a'), &[1], &[])]),
            "a",
            None,
        ));
        let b = trim(unroll(
            &auto(
                3,
                &[2],
                vec![
                    arc(0, 1, Some('a'), &[], &[]),
                    arc(1, 2, Some('b'), &[], &[]),
                ],
            ),
            "ab",
            None,
        ));
        let _ = read_out_indexed(
            &b,
            &ReadoutIndex::new(&a, &mut meter()).unwrap(),
            ReadoutBudget::UNLIMITED,
            ReadoutMode::Single,
        );
    }

    #[test]
    fn empty_graph_is_the_one_legitimate_empty_case() {
        let g = trim(unroll(
            &auto(2, &[1], vec![arc(0, 1, Some('a'), &[1], &[])]),
            "b",
            None,
        ));
        assert_eq!(g.start, None);
        assert_eq!(groups(&g), Vec::<PinGroup>::new());
    }

    #[test]
    fn unaccepted_string_has_no_group() {
        let a = auto(2, &[1], vec![arc(0, 1, Some('a'), &[1], &[])]);
        assert!(ro(&a, "b", None).is_empty());
        assert!(ro(&a, "", None).is_empty());
        assert!(ro(&a, "aa", None).is_empty());
    }

    #[test]
    fn accepted_string_without_tags_is_one_empty_group() {
        let a = auto(2, &[1], vec![arc(0, 1, Some('a'), &[], &[])]);
        assert_eq!(ro(&a, "a", None), vec![group(&[], &[])]);
    }

    #[test]
    fn arc_with_root_and_rule_tag_gives_both() {
        // One arc carries root 7 and rule 3; a second path to a dead end carries rule 9.
        let a = auto(
            3,
            &[1],
            vec![
                arc(0, 1, Some('a'), &[7], &[3]),
                arc(0, 2, Some('a'), &[8], &[9]),
            ],
        );
        assert_eq!(ro(&a, "a", None), vec![group(&[7], &[3])]);
    }

    #[test]
    fn non_head_tags_are_not_pinned() {
        let mut other = arc(0, 1, Some('a'), &[4], &[]);
        other.other = true;
        let a = auto(2, &[1], vec![other]);
        assert_eq!(ro(&a, "a", None), vec![group(&[4], &[])]);
    }

    #[test]
    fn rules_after_the_root_on_the_path_are_kept() {
        let a = auto(
            4,
            &[3],
            vec![
                arc(0, 1, Some('a'), &[1], &[]),
                arc(1, 2, Some('b'), &[], &[10]),
                arc(2, 3, Some('c'), &[], &[11]),
            ],
        );
        assert_eq!(ro(&a, "abc", None), vec![group(&[1], &[10, 11])]);
    }

    #[test]
    fn tags_of_all_paths_are_unioned_not_first_path() {
        // C95: two accepting paths for "a" with different roots and rules.
        let a = auto(
            4,
            &[3],
            vec![
                arc(0, 1, Some('a'), &[1], &[]),
                arc(1, 3, None, &[], &[10]),
                arc(0, 2, Some('a'), &[2], &[]),
                arc(2, 3, None, &[], &[20]),
            ],
        );
        assert_eq!(ro(&a, "a", None), vec![group(&[1, 2], &[10, 20])]);
    }

    #[test]
    fn parallel_arcs_union_their_tags_once() {
        let mut heavy = arc(0, 1, Some('a'), &[5, 6], &[2]);
        heavy.weight = 3;
        let a = auto(
            2,
            &[1],
            vec![
                arc(0, 1, Some('a'), &[5], &[1]),
                arc(0, 1, Some('a'), &[5], &[1]),
                heavy,
            ],
        );
        assert_eq!(ro(&a, "a", None), vec![group(&[5, 6], &[1, 2])]);
    }

    #[test]
    fn self_loop_tags_count_only_when_on_a_surviving_path() {
        let a = auto(
            4,
            &[1],
            vec![
                arc(0, 1, Some('a'), &[1], &[]),
                arc(1, 1, Some('b'), &[], &[4]),
                arc(0, 2, Some('a'), &[2], &[]),
                arc(2, 2, Some('b'), &[], &[5]),
                arc(0, 3, Some('a'), &[], &[]),
            ],
        );
        assert_eq!(ro(&a, "abb", None), vec![group(&[1], &[4])]);
        assert_eq!(ro(&a, "a", None), vec![group(&[1], &[])]);
    }

    #[test]
    fn epsilon_cycle_is_closed_and_terminates() {
        let a = auto(
            3,
            &[2],
            vec![
                arc(0, 1, Some('a'), &[1], &[]),
                arc(1, 2, None, &[], &[1]),
                arc(2, 1, None, &[], &[2]),
            ],
        );
        assert_eq!(ro(&a, "a", None), vec![group(&[1], &[1, 2])]);
    }

    #[test]
    fn root_tag_position_on_the_path_does_not_matter() {
        // C71: moving the root tag from the first arc of the entry to its last arc changes nothing.
        let early = auto(
            3,
            &[2],
            vec![
                arc(0, 1, Some('a'), &[1], &[]),
                arc(1, 2, Some('b'), &[], &[3]),
            ],
        );
        let late = auto(
            3,
            &[2],
            vec![
                arc(0, 1, Some('a'), &[], &[]),
                arc(1, 2, Some('b'), &[1], &[3]),
            ],
        );
        let e = ro(&early, "ab", None);
        assert_eq!(e, ro(&late, "ab", None));
        assert_eq!(e, vec![group(&[1], &[3])]);
    }

    #[test]
    fn same_state_at_two_positions_is_two_nodes() {
        let a = auto(
            3,
            &[2],
            vec![
                arc(0, 1, Some('a'), &[], &[]),
                arc(1, 1, Some('a'), &[], &[7]),
                arc(1, 2, Some('b'), &[], &[]),
            ],
        );
        assert_eq!(ro(&a, "ab", None), vec![group(&[], &[])]);
        assert_eq!(ro(&a, "aab", None), vec![group(&[], &[7])]);
    }

    #[test]
    fn theta_drops_tags_that_lie_only_on_heavier_walks() {
        // Cheap walk root 1 / rule 5 at weight 1; dear walk root 2 / rule 6 at weight 9.
        let mut dear1 = arc(0, 2, Some('a'), &[2], &[]);
        dear1.weight = 4;
        let mut dear2 = arc(2, 3, Some('b'), &[], &[6]);
        dear2.weight = 5;
        let mut cheap2 = arc(1, 3, Some('b'), &[], &[5]);
        cheap2.weight = 1;
        let a = auto(
            4,
            &[3],
            vec![arc(0, 1, Some('a'), &[1], &[]), cheap2, dear1, dear2],
        );
        assert_eq!(ro(&a, "ab", None), vec![group(&[1, 2], &[5, 6])]);
        assert_eq!(ro(&a, "ab", Some(9)), vec![group(&[1, 2], &[5, 6])]);
        assert_eq!(ro(&a, "ab", Some(8)), vec![group(&[1], &[5])]);
        assert_eq!(ro(&a, "ab", Some(1)), vec![group(&[1], &[5])]);
        assert!(ro(&a, "ab", Some(0)).is_empty());
    }

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, m: u32) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as u32) % m
        }
    }

    fn random_auto(rng: &mut Lcg) -> Auto {
        let ns = 2 + rng.next(4);
        let mut arcs = Vec::new();
        for _ in 0..2 + rng.next(9) {
            let written = match rng.next(5) {
                0 => None,
                1 | 2 => Some('a'),
                _ => Some('b'),
            };
            let mut e = arc(rng.next(ns), rng.next(ns), written, &[], &[]);
            e.weight = [0, 0, 1, 2, 3][rng.next(5) as usize];
            if rng.next(3) == 0 {
                e.roots.push(rng.next(4));
            }
            if rng.next(2) == 0 {
                e.rules.push(rng.next(5));
            }
            if rng.next(6) == 0 {
                e.rules.push(rng.next(5));
            }
            e.other = rng.next(8) == 0;
            arcs.push(e);
        }
        Auto {
            accepting: (0..ns).filter(|_| rng.next(3) == 0).collect(),
            arcs,
        }
    }

    #[test]
    fn random_small_graphs_match_explicit_enumeration() {
        let mut rng = Lcg(0x5eed);
        let strings = ["", "a", "b", "ab", "ba", "aa", "aab", "abb", "abab"];
        let (mut accepted, mut cut) = (0, 0);
        for _ in 0..400 {
            let a = random_auto(&mut rng);
            for s in strings {
                for theta in [None, Some(rng.next(7))] {
                    let got = ro(&a, s, theta);
                    let want = oracle(&a, s, theta);
                    assert_eq!(got, want, "auto {a:?} string {s:?} theta {theta:?}");
                    accepted += got.len();
                    if theta.is_some() && oracle(&a, s, None) != want {
                        cut += 1;
                    }
                }
            }
        }
        assert!(accepted > 300, "generator too sparse: {accepted} accepted");
        assert!(cut > 20, "theta never changed an answer: {cut}");
    }

    /// Each budget dimension ends the read-out at its own figure, never with a partial group.
    #[test]
    fn each_readout_budget_is_incomplete_naming_its_dimension() {
        let a = auto(
            4,
            &[3],
            vec![
                arc(0, 1, Some('a'), &[1], &[]),
                arc(1, 1, Some('b'), &[], &[4]),
                arc(1, 2, Some('b'), &[2], &[5]),
                arc(2, 3, None, &[3], &[6]),
            ],
        );
        let g = trim(unroll(&a, "abb", None));
        let run = |b: ReadoutBudget| read_out(&g, b, ReadoutMode::Single);
        let Readout::Complete { groups, used } = run(ReadoutBudget::UNLIMITED) else {
            panic!()
        };
        assert_eq!(groups.len(), 1);
        for (stop, n) in [
            (ReadoutStop::ReadoutNodes, used.nodes),
            (ReadoutStop::ReadoutSteps, used.steps),
            (ReadoutStop::ReadoutBytes, used.bytes),
        ] {
            assert!(n > 4, "{stop:?} too small to test");
            let with = |v: u64| {
                let mut b = ReadoutBudget::UNLIMITED;
                match stop {
                    ReadoutStop::ReadoutNodes => b.nodes = v,
                    ReadoutStop::ReadoutSteps => b.steps = v,
                    ReadoutStop::ReadoutBytes => b.bytes = v,
                }
                b
            };
            match run(with(n - 1)) {
                Readout::Incomplete { stop: s, used: u } => {
                    assert_eq!(s, stop);
                    assert!(u.nodes < used.nodes || u.steps < used.steps || u.bytes < used.bytes);
                }
                o => panic!("{stop:?}: overrun did not stop: {o:?}"),
            }
            match run(with(n)) {
                Readout::Complete { groups: g2, .. } => assert_eq!(g2, groups),
                o => panic!("{stop:?}: exact limit must complete: {o:?}"),
            }
        }
    }

    /// The sweeps are sparse: unreachable states add nothing to the nodes they reach.
    #[test]
    fn unreached_states_cost_nothing() {
        let a = auto(2, &[1], vec![arc(0, 1, Some('a'), &[1], &[])]);
        let mut g = untrimmed(unroll(&a, "a", None));
        let w = KEY_PREFIX + g.layers;
        for k in 0..10_000u32 {
            g.keys.extend_from_slice(&[0, 0, 100 + k]);
            g.accepting.push(false);
        }
        assert_eq!(g.keys.len(), g.state_count() * w);
        let Readout::Complete { used, .. } =
            read_out(&g, ReadoutBudget::UNLIMITED, ReadoutMode::Single)
        else {
            panic!()
        };
        assert_eq!(used.nodes, 4);
    }

    /// The real product: a graph whose arcs lie only on walks above theta, read out under theta.
    #[test]
    fn product_graph_under_theta_is_filtered_by_path_weight() {
        let mut meter = BuildMeter::default_limits();
        let mut b = NfaBuilder::new("t", 1, &mut meter);
        for fin in [false, false, false, true] {
            b.add_state(fin).unwrap();
        }
        let root = |r: u32| [Tag::root(r)];
        let rule = |r: u32| [Tag::rule(r)];
        let eps = |b: &mut NfaBuilder, from, to, tags: &[Tag], w| {
            b.add_arc(from, Input::Eps, Output::Eps, to, tags, w, 0)
                .unwrap()
        };
        eps(&mut b, 0, 1, &root(2), 0);
        eps(&mut b, 1, 2, &[], 600);
        eps(&mut b, 0, 2, &root(1), 0);
        eps(&mut b, 2, 3, &rule(7), 600);
        let layer = b.finish();
        let refs: Vec<&dyn super::super::layer::Layer> = vec![&layer];
        let prod = Product::new(&refs).unwrap();
        let word: [u32; 0] = [];
        let mut req = Request::new(&word, 0);
        req.theta = Some(1000);
        let Outcome::Complete { mut graph, .. } = prod.run(&mut ProductScratch::new(1), &req)
        else {
            panic!()
        };
        assert_eq!(
            graph.arcs.len(),
            4,
            "every arc passes its own creation test"
        );
        assert_eq!(
            groups(&graph),
            vec![group(&[1], &[7])],
            "root 2 lies only on a walk of weight 1200"
        );
        graph.theta = None;
        assert_eq!(groups(&graph), vec![group(&[1, 2], &[7])]);
    }

    /// A tag word that no tag was built as is an error, never an ignored tag.
    #[test]
    fn unknown_tag_word_is_an_error_not_other() {
        let a = auto(2, &[1], vec![arc(0, 1, Some('a'), &[1], &[])]);
        let mut raw = unroll(&a, "a", None);
        raw.tag_sets.push(Box::new([Tag(3 << Tag::ID_BITS)]));
        raw.arcs[0].tags = raw.tag_sets.len() as u32 - 1;
        let g = trim(raw);
        assert_eq!(
            read_out(&g, ReadoutBudget::UNLIMITED, ReadoutMode::Single),
            Readout::BadTag(BadTag(Tag(3 << Tag::ID_BITS)))
        );
    }

    /// The index is charged to the read-out budget before it is built.
    #[test]
    fn index_bytes_are_charged_before_the_index_is_built() {
        let a = auto(2, &[1], vec![arc(0, 1, Some('a'), &[1], &[])]);
        let g = trim(unroll(&a, "a", None));
        let need = index_bytes(g.state_count(), g.arcs.len());
        let mut tight = ReadoutMeter::new(ReadoutBudget {
            bytes: need - 1,
            ..ReadoutBudget::UNLIMITED
        });
        assert!(matches!(
            ReadoutIndex::new(&g, &mut tight),
            Err(ReadoutStop::ReadoutBytes)
        ));
        assert_eq!(tight.used().bytes, 0);
        let b = ReadoutBudget {
            bytes: need - 1,
            ..ReadoutBudget::UNLIMITED
        };
        assert!(matches!(
            read_out(&g, b, ReadoutMode::Single),
            Readout::Incomplete {
                stop: ReadoutStop::ReadoutBytes,
                ..
            }
        ));
    }

    /// The ceiling is the graph's own: the same arcs read out differently under a different one.
    #[test]
    fn read_out_uses_the_ceiling_stored_in_the_graph() {
        let mut dear = arc(0, 1, Some('a'), &[2], &[]);
        dear.weight = 5;
        let a = auto(2, &[1], vec![arc(0, 1, Some('a'), &[1], &[]), dear]);
        let mut g = untrimmed(unroll(&a, "a", Some(1)));
        assert_eq!(groups(&g), vec![group(&[1], &[])]);
        g.theta = None;
        assert_eq!(groups(&g), vec![group(&[1, 2], &[])]);
    }
}
