//! The speller's read-out of a trimmed graph (plan S.5, S.6): groups in `(weight, key)` order,
//! the variants of one group, and the features and weight of one exact string.
//!
//! The graph's arcs carry a [`Label`] made by the request's observer: the character written by
//! `Repr` (if the move wrote one), the folded base that passed `Fold` (if one did) and the case
//! mark. Nothing here knows which layer produced them, so it works for any layer stack whose model
//! can fill the three fields.
//!
//! Every function charges its work to the caller's meter before it allocates.

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap, HashSet};

use super::budget::CascadeStop;
use super::layer::{Tag, Token};
use super::trim::{distance_to_accept, TrimmedGraph};

const INF: u64 = u64::MAX;

/// What the observer says about one product move (S.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Label {
    /// The exact character `Repr` wrote in this move.
    pub written: Option<Token>,
    /// The folded base (stack removed) that left `Fold` in this move.
    pub folded: Option<Token>,
    /// The case mark of the move that flushes a unit through `Fold`.
    pub case: bool,
}

impl Label {
    pub fn pack(self) -> u64 {
        let w = self.written.map_or(0, |t| {
            assert!(t < 1 << 31, "written token {t} too large to label");
            t as u64 + 1
        });
        let f = self.folded.map_or(0, |t| {
            assert!(t < 1 << 31, "folded token {t} too large to label");
            t as u64 + 1
        });
        (w << 32) | (f << 1) | self.case as u64
    }

    pub fn unpack(x: u64) -> Label {
        let w = (x >> 32) as u32;
        let f = ((x >> 1) & 0x7fff_ffff) as u32;
        Label {
            written: w.checked_sub(1),
            folded: f.checked_sub(1),
            case: x & 1 == 1,
        }
    }
}

/// What the read-out needs to know about the character tape.
pub trait SpellModel {
    /// The label of one move.
    fn observe(&self, mv: &super::product::MoveView) -> Label;
    fn written_char(&self, t: Token) -> char;
    fn written_token(&self, c: char) -> Option<Token>;
    /// A combining mark: it joins the unit before it.
    fn is_mark(&self, t: Token) -> bool;
    /// The reference stack of each key position (S.2 `stack_distance`): the stack of the typed
    /// unit the key is aligned with at least base-level cost, or the empty stack.
    fn reference_stacks(&self, key: &[Token]) -> Vec<Vec<Token>>;
    /// The grammar can loop on a mark that no counter bounds (S.2 `stack_loop`).
    fn stack_loop(&self) -> bool {
        false
    }
}

/// One charge to the request's meter.
#[derive(Clone, Copy, Debug)]
pub enum Cost {
    Steps(u64),
    Bytes(u64),
}

pub type Charge<'c> = &'c mut dyn FnMut(Cost) -> Result<(), CascadeStop>;

/// The graph with adjacency and the exact remaining weight `h*` of the backward trim.
pub struct Reader<'g> {
    pub graph: &'g TrimmedGraph,
    /// CSR offsets and arc indexes, by source state.
    first: Vec<u32>,
    out: Vec<u32>,
    /// Least weight from each state to an accepting state; `u32::MAX` if none.
    rem: Vec<u32>,
    theta: Option<u64>,
}

impl<'g> Reader<'g> {
    pub fn new(graph: &'g TrimmedGraph, charge: Charge) -> Result<Self, CascadeStop> {
        let n = graph.state_count();
        let m = graph.arcs.len();
        charge(Cost::Bytes(n as u64 * 12 + m as u64 * 8))?;
        charge(Cost::Steps(m as u64 + n as u64))?;
        let mut first = vec![0u32; n + 1];
        for a in &graph.arcs {
            first[a.src as usize + 1] += 1;
        }
        for i in 0..n {
            first[i + 1] += first[i];
        }
        let mut fill = first.clone();
        let mut out = vec![0u32; m];
        for (i, a) in graph.arcs.iter().enumerate() {
            out[fill[a.src as usize] as usize] = i as u32;
            fill[a.src as usize] += 1;
        }
        let arcs: Vec<(u32, u32, u32)> = graph
            .arcs
            .iter()
            .map(|a| (a.src, a.dst, a.weight))
            .collect();
        let rem = distance_to_accept(n, &arcs, &graph.accepting);
        Ok(Reader {
            graph,
            first,
            out,
            rem,
            theta: graph.theta.map(u64::from),
        })
    }

    fn arcs_of(&self, s: u32) -> impl Iterator<Item = (u32, &super::trim::GraphArc)> + '_ {
        let (a, b) = (
            self.first[s as usize] as usize,
            self.first[s as usize + 1] as usize,
        );
        self.out[a..b]
            .iter()
            .map(move |&i| (i, &self.graph.arcs[i as usize]))
    }

    fn rem_of(&self, s: u32) -> u64 {
        match self.rem[s as usize] {
            u32::MAX => INF,
            r => r as u64,
        }
    }

    /// `g + h*` within the ceiling (the creation test again, on the trimmed graph).
    fn within(&self, g: u64, s: u32) -> bool {
        let r = self.rem_of(s);
        r != INF && self.theta.is_none_or(|t| g + r <= t)
    }

    fn edits_of(&self, s: u32) -> u8 {
        self.graph.key(s)[1] as u8
    }
}

// ---------------------------------------------------------------- groups

/// A group as the prefix tree finds it: its key and least weight; its edits are those of a
/// least-weight path (fewest on a tie).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupCore {
    pub key: Vec<Token>,
    pub weight: u64,
    pub edits: u8,
}

struct Node {
    key: Vec<Token>,
    /// States reached by this key (closed under moves with no folded unit), with least weights.
    states: Vec<(u32, u64)>,
    ready: Option<(u64, u8)>,
}

/// Heap entry: weight, key, kind (0 node, 1 ready), node index.
type NodeItem = (u64, Vec<Token>, u8, usize);

/// Groups in `(weight, key)` order, lazily (S.6): best-first over the prefix tree of folded bases.
pub struct GroupIter<'r, 'g> {
    r: &'r Reader<'g>,
    nodes: Vec<Node>,
    /// `(f, key, kind, node)`: kind 0 is a node to expand, kind 1 a group ready to be reported.
    heap: BinaryHeap<Reverse<NodeItem>>,
}

impl<'r, 'g> GroupIter<'r, 'g> {
    pub fn new(r: &'r Reader<'g>) -> Self {
        let mut it = GroupIter {
            r,
            nodes: Vec::new(),
            heap: BinaryHeap::new(),
        };
        if let Some(start) = r.graph.start {
            if r.within(0, start) {
                it.nodes.push(Node {
                    key: Vec::new(),
                    states: vec![(start, 0)],
                    ready: None,
                });
                it.heap.push(Reverse((r.rem_of(start), Vec::new(), 0, 0)));
            }
        }
        it
    }

    pub fn next_group(&mut self, charge: Charge) -> Result<Option<GroupCore>, CascadeStop> {
        while let Some(Reverse((f, key, kind, ni))) = self.heap.pop() {
            if kind == 1 {
                let (weight, edits) = self.nodes[ni].ready.expect("ready entry has a weight");
                debug_assert_eq!(f, weight);
                return Ok(Some(GroupCore { key, weight, edits }));
            }
            self.expand(ni, charge)?;
        }
        Ok(None)
    }

    fn expand(&mut self, ni: usize, charge: Charge) -> Result<(), CascadeStop> {
        let r = self.r;
        // closure over moves that pass no folded unit
        let mut best: HashMap<u32, u64> = HashMap::new();
        let mut q: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();
        for &(s, g) in &self.nodes[ni].states {
            best.insert(s, g);
            q.push(Reverse((g, s)));
        }
        while let Some(Reverse((g, s))) = q.pop() {
            if best[&s] != g {
                continue;
            }
            let mut n = 0u64;
            for (_, a) in r.arcs_of(s) {
                n += 1;
                if Label::unpack(a.label).folded.is_some() {
                    continue;
                }
                let ng = g + a.weight as u64;
                if r.within(ng, a.dst) && best.get(&a.dst).is_none_or(|&b| ng < b) {
                    charge(Cost::Bytes(24))?;
                    best.insert(a.dst, ng);
                    q.push(Reverse((ng, a.dst)));
                }
            }
            charge(Cost::Steps(n.max(1)))?;
        }
        let mut states: Vec<(u32, u64)> = best.into_iter().collect();
        states.sort();
        // a group is ready here if some accepting state is in the closure
        let acc = states
            .iter()
            .filter(|&&(s, _)| r.graph.accepting[s as usize])
            .map(|&(s, g)| (g, r.edits_of(s)))
            .min();
        if let Some((w, e)) = acc {
            self.nodes[ni].ready = Some((w, e));
            let key = self.nodes[ni].key.clone();
            charge(Cost::Bytes(32 + 4 * key.len() as u64))?;
            self.heap.push(Reverse((w, key, 1, ni)));
        }
        // children by the folded base passed
        let mut kids: HashMap<Token, HashMap<u32, u64>> = HashMap::new();
        for &(s, g) in &states {
            let mut n = 0u64;
            for (_, a) in r.arcs_of(s) {
                n += 1;
                let Some(b) = Label::unpack(a.label).folded else {
                    continue;
                };
                let ng = g + a.weight as u64;
                if r.within(ng, a.dst) {
                    let e = kids.entry(b).or_default().entry(a.dst).or_insert(u64::MAX);
                    *e = (*e).min(ng);
                }
            }
            charge(Cost::Steps(n.max(1)))?;
        }
        let mut kids: Vec<_> = kids.into_iter().collect();
        kids.sort_by_key(|(b, _)| *b);
        for (b, m) in kids {
            let mut key = self.nodes[ni].key.clone();
            key.push(b);
            let mut st: Vec<(u32, u64)> = m.into_iter().collect();
            st.sort();
            let f = st
                .iter()
                .map(|&(s, g)| g + r.rem_of(s))
                .min()
                .expect("a child has a state");
            charge(Cost::Bytes(
                64 + 4 * key.len() as u64 + 16 * st.len() as u64,
            ))?;
            let id = self.nodes.len();
            self.heap.push(Reverse((f, key.clone(), 0, id)));
            self.nodes.push(Node {
                key,
                states: st,
                ready: None,
            });
        }
        // the expanded node's own state list is no longer needed
        self.nodes[ni].states = Vec::new();
        Ok(())
    }
}

// ---------------------------------------------------------------- one exact string

/// What a graph says about one exact string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringInfo {
    pub weight: u64,
    pub edits: u8,
    /// Least case count among the least-weight, fewest-edit paths.
    pub case_distance: u32,
    /// Tags on arcs of least-weight accepting paths that spell the string, self-loops included.
    pub tags: BTreeSet<Tag>,
}

type Cost3 = (u64, u32, u32);

fn add3(a: Cost3, w: u64, e: u32, c: u32) -> Cost3 {
    (a.0 + w, a.1 + e, a.2 + c)
}

/// Weight, edits, case count and features of the exact string `toks` (S.6, "Features of an exact
/// string"): forward over the string, backward filter from acceptance, tags of the arcs that
/// survive and lie on a least-weight path. `None` if the graph has no path spelling it within the
/// ceiling.
pub fn string_info(
    r: &Reader,
    toks: &[Token],
    charge: Charge,
) -> Result<Option<StringInfo>, CascadeStop> {
    let g = r.graph;
    let Some(start) = g.start else {
        return Ok(None);
    };
    let len = toks.len();
    let mut idx: HashMap<(usize, u32), u32> = HashMap::new();
    let mut nodes: Vec<(usize, u32)> = Vec::new();
    // edges: (from node, to node, arc index)
    let mut edges: Vec<(u32, u32, u32)> = Vec::new();
    idx.insert((0, start), 0);
    nodes.push((0, start));
    let mut next = 0;
    while next < nodes.len() {
        let (k, s) = nodes[next];
        let from = next as u32;
        next += 1;
        let mut n = 0u64;
        for (ai, a) in r.arcs_of(s) {
            n += 1;
            let nk = match Label::unpack(a.label).written {
                None => k,
                Some(t) if k < len && toks[k] == t => k + 1,
                Some(_) => continue,
            };
            charge(Cost::Bytes(40))?;
            let to = *idx.entry((nk, a.dst)).or_insert_with(|| {
                nodes.push((nk, a.dst));
                nodes.len() as u32 - 1
            });
            edges.push((from, to, ai));
        }
        charge(Cost::Steps(n.max(1)))?;
    }
    let count = nodes.len();
    charge(Cost::Bytes(count as u64 * 56 + edges.len() as u64 * 24))?;
    charge(Cost::Steps(edges.len() as u64 * 2 + count as u64))?;
    let mut out_of: Vec<Vec<u32>> = vec![Vec::new(); count];
    let mut in_of: Vec<Vec<u32>> = vec![Vec::new(); count];
    for (i, &(f, t, _)) in edges.iter().enumerate() {
        out_of[f as usize].push(i as u32);
        in_of[t as usize].push(i as u32);
    }
    let cost = |ai: u32| {
        let a = &g.arcs[ai as usize];
        (
            a.weight as u64,
            a.edits as u32,
            Label::unpack(a.label).case as u32,
        )
    };
    // forward least (weight, edits, case)
    let mut fwd: Vec<Option<Cost3>> = vec![None; count];
    let mut q: BinaryHeap<Reverse<(Cost3, u32)>> = BinaryHeap::new();
    fwd[0] = Some((0, 0, 0));
    q.push(Reverse(((0, 0, 0), 0)));
    while let Some(Reverse((c, n))) = q.pop() {
        if fwd[n as usize] != Some(c) {
            continue;
        }
        for &ei in &out_of[n as usize] {
            let (_, to, ai) = edges[ei as usize];
            let (w, e, cs) = cost(ai);
            let nc = add3(c, w, e, cs);
            if fwd[to as usize].is_none_or(|b| nc < b) {
                fwd[to as usize] = Some(nc);
                q.push(Reverse((nc, to)));
            }
        }
    }
    let accepting = |n: usize| nodes[n].0 == len && g.accepting[nodes[n].1 as usize];
    let best = (0..count)
        .filter(|&n| accepting(n))
        .filter_map(|n| fwd[n])
        .min();
    let Some(best) = best else {
        return Ok(None);
    };
    if r.theta.is_some_and(|t| best.0 > t) {
        return Ok(None);
    }
    // backward least weight to an accepting node
    let mut bwd: Vec<u64> = vec![INF; count];
    let mut q: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();
    for (n, slot) in bwd.iter_mut().enumerate() {
        if accepting(n) {
            *slot = 0;
            q.push(Reverse((0, n as u32)));
        }
    }
    while let Some(Reverse((d, n))) = q.pop() {
        if bwd[n as usize] != d {
            continue;
        }
        for &ei in &in_of[n as usize] {
            let (from, _, ai) = edges[ei as usize];
            let nd = d + g.arcs[ai as usize].weight as u64;
            if nd < bwd[from as usize] {
                bwd[from as usize] = nd;
                q.push(Reverse((nd, from)));
            }
        }
    }
    let mut tags = BTreeSet::new();
    for &(f, t, ai) in &edges {
        let (Some(fc), true) = (fwd[f as usize], bwd[t as usize] != INF) else {
            continue;
        };
        let a = &g.arcs[ai as usize];
        if fc.0 + a.weight as u64 + bwd[t as usize] == best.0 {
            tags.extend(g.arc_tags(a).iter().copied());
        }
    }
    Ok(Some(StringInfo {
        weight: best.0,
        edits: u8::try_from(best.1).unwrap_or(u8::MAX),
        case_distance: best.2,
        tags,
    }))
}

// ---------------------------------------------------------------- variants of one group

/// One exact string of a group with the ordering key of S.6.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantCore {
    pub tokens: Vec<Token>,
    pub chars: Vec<char>,
    pub weight: u64,
    pub edits: u8,
    pub stack_distance: u32,
    pub case_distance: u32,
    pub tags: BTreeSet<Tag>,
    /// The marks of each unit, in order: the "stack assignment" the cap counts.
    pub stacks: Vec<Vec<Token>>,
}

/// Levenshtein row of a growing mark stack against its reference.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Unit {
    /// Index of the current unit, or `None` before the first character.
    idx: Option<usize>,
    /// Sum of the final distances of the finished units.
    done: u32,
    row: Vec<u32>,
}

fn lev_start(refstack: &[Token]) -> Vec<u32> {
    (0..=refstack.len() as u32).collect()
}

fn lev_push(row: &[u32], refstack: &[Token], mark: Token) -> Vec<u32> {
    let mut next = vec![row[0] + 1];
    for j in 1..row.len() {
        let sub = row[j - 1] + (refstack[j - 1] != mark) as u32;
        next.push(sub.min(row[j] + 1).min(next[j - 1] + 1));
    }
    next
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Prio {
    weight: u64,
    stack_lb: u32,
    case: u32,
    nchars: usize,
    chars: Vec<char>,
}

#[derive(Clone, Debug)]
struct PathEntry {
    prio: Prio,
    state: u32,
    j: usize,
    toks: Vec<Token>,
    stacks: Vec<Vec<Token>>,
    unit: Unit,
}

#[derive(PartialEq, Eq)]
struct HeapItem(Prio, u64);

impl Ord for HeapItem {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        (&self.0, self.1).cmp(&(&o.0, o.1))
    }
}
impl PartialOrd for HeapItem {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

/// The variants of one group, nearest first (S.6): weight, `stack_distance`, `case_distance`,
/// number of characters, string.
///
/// The search runs on prefix lower bounds of that key. A string is scored exactly, once, when its
/// first complete path pops (its true weight, fewest edits among least-weight paths, and least
/// case count among those, by [`string_info`]); it is released only when no unseen string can
/// precede it.
pub struct VariantIter<'r, 'g, 'm> {
    r: &'r Reader<'g>,
    model: &'m dyn SpellModel,
    key: Vec<Token>,
    refs: Vec<Vec<Token>>,
    paths: Vec<PathEntry>,
    heap: BinaryHeap<Reverse<HeapItem>>,
    closed: HashSet<(u32, usize, Vec<char>)>,
    seen: HashSet<Vec<char>>,
    buffer: BinaryHeap<Reverse<(Prio, u8, u32)>>,
    scored: Vec<VariantCore>,
    /// Variants of weight at or above this are not listed (the stop weight of `Best(n)`).
    limit: Option<u64>,
    /// Something was held back by `limit`.
    pub beyond_limit: bool,
}

impl<'r, 'g, 'm> VariantIter<'r, 'g, 'm> {
    pub fn new(
        r: &'r Reader<'g>,
        model: &'m dyn SpellModel,
        key: &[Token],
        limit: Option<u64>,
    ) -> Self {
        let mut it = VariantIter {
            r,
            model,
            key: key.to_vec(),
            refs: model.reference_stacks(key),
            paths: Vec::new(),
            heap: BinaryHeap::new(),
            closed: HashSet::new(),
            seen: HashSet::new(),
            buffer: BinaryHeap::new(),
            scored: Vec::new(),
            limit,
            beyond_limit: false,
        };
        if let Some(start) = r.graph.start {
            let e = PathEntry {
                prio: Prio {
                    weight: 0,
                    stack_lb: 0,
                    case: 0,
                    nchars: 0,
                    chars: Vec::new(),
                },
                state: start,
                j: 0,
                toks: Vec::new(),
                stacks: Vec::new(),
                unit: Unit {
                    idx: None,
                    done: 0,
                    row: Vec::new(),
                },
            };
            it.push(e);
        }
        it
    }

    fn push(&mut self, e: PathEntry) {
        let id = self.paths.len() as u64;
        self.heap.push(Reverse(HeapItem(e.prio.clone(), id)));
        self.paths.push(e);
    }

    fn ref_of(&self, unit: usize) -> &[Token] {
        self.refs.get(unit).map_or(&[], |v| v.as_slice())
    }

    /// The next variant in order, or `None` when the group has no more (below the limit).
    pub fn next_variant(&mut self, charge: Charge) -> Result<Option<VariantCore>, CascadeStop> {
        loop {
            let top = self.heap.peek().map(|Reverse(h)| &h.0);
            if let Some(Reverse((prio, _, si))) = self.buffer.peek() {
                if top.is_none_or(|t| prio <= t) {
                    let si = *si;
                    self.buffer.pop();
                    return Ok(Some(self.scored[si as usize].clone()));
                }
            }
            let Some(Reverse(HeapItem(prio, id))) = self.heap.pop() else {
                return Ok(None);
            };
            if self.limit.is_some_and(|l| prio.weight >= l) {
                // everything left is at least this heavy
                self.beyond_limit = true;
                self.heap.clear();
                continue;
            }
            let e = self.paths[id as usize].clone();
            self.paths[id as usize].toks = Vec::new();
            self.paths[id as usize].stacks = Vec::new();
            if !self.closed.insert((e.state, e.j, e.prio.chars.clone())) {
                continue;
            }
            self.extend(&e, charge)?;
        }
    }

    fn extend(&mut self, e: &PathEntry, charge: Charge) -> Result<(), CascadeStop> {
        let r = self.r;
        if e.j == self.key.len()
            && r.graph.accepting[e.state as usize]
            && self.seen.insert(e.prio.chars.clone())
        {
            if let Some(info) = string_info(r, &e.toks, charge)? {
                let (stacks, stack_distance) = self.final_stacks(e);
                let v = VariantCore {
                    tokens: e.toks.clone(),
                    chars: e.prio.chars.clone(),
                    weight: info.weight,
                    edits: info.edits,
                    stack_distance,
                    case_distance: info.case_distance,
                    tags: info.tags,
                    stacks,
                };
                let prio = Prio {
                    weight: v.weight,
                    stack_lb: v.stack_distance,
                    case: v.case_distance,
                    nchars: v.chars.len(),
                    chars: v.chars.clone(),
                };
                charge(Cost::Bytes(64 + 8 * v.chars.len() as u64))?;
                let si = self.scored.len() as u32;
                self.scored.push(v);
                self.buffer.push(Reverse((prio, 0, si)));
            }
        }
        let mut n = 0u64;
        let arcs: Vec<(u32, super::trim::GraphArc)> =
            r.arcs_of(e.state).map(|(i, a)| (i, *a)).collect();
        for (_, a) in arcs {
            n += 1;
            let lab = Label::unpack(a.label);
            let mut j = e.j;
            if let Some(b) = lab.folded {
                if j >= self.key.len() || self.key[j] != b {
                    continue;
                }
                j += 1;
            }
            let g = e.prio.weight + a.weight as u64;
            if !r.within(g, a.dst) {
                continue;
            }
            let mut ne = PathEntry {
                prio: Prio {
                    weight: g,
                    stack_lb: e.prio.stack_lb,
                    case: e.prio.case + lab.case as u32,
                    nchars: e.prio.nchars,
                    chars: e.prio.chars.clone(),
                },
                state: a.dst,
                j,
                toks: e.toks.clone(),
                stacks: e.stacks.clone(),
                unit: e.unit.clone(),
            };
            if let Some(t) = lab.written {
                self.write(&mut ne, t);
            }
            charge(Cost::Bytes(
                96 + 8 * ne.toks.len() as u64 + 8 * ne.stacks.len() as u64,
            ))?;
            self.push(ne);
        }
        charge(Cost::Steps(n.max(1)))
    }

    /// Append `t` to a path: cut units, keep each unit's stack and the `stack_distance` bound.
    fn write(&self, e: &mut PathEntry, t: Token) {
        e.toks.push(t);
        e.prio.chars.push(self.model.written_char(t));
        e.prio.nchars += 1;
        let mark = self.model.is_mark(t);
        let u = &mut e.unit;
        match (mark, u.idx) {
            (false, _) => {
                if let Some(i) = u.idx {
                    u.done += *u.row.last().expect("a row");
                    let _ = i;
                }
                let i = u.idx.map_or(0, |i| i + 1);
                u.idx = Some(i);
                u.row = lev_start(self.ref_of(i));
                e.stacks.push(Vec::new());
            }
            (true, None) => {
                u.idx = Some(0);
                u.row = lev_start(self.ref_of(0));
                e.stacks.push(Vec::new());
            }
            (true, Some(_)) => {}
        }
        if mark {
            let i = u.idx.expect("a unit");
            u.row = lev_push(&u.row, self.refs.get(i).map_or(&[], |v| v.as_slice()), t);
            e.stacks.last_mut().expect("a stack").push(t);
        }
        e.prio.stack_lb = u.done + u.row.iter().copied().min().unwrap_or(0);
    }

    fn final_stacks(&self, e: &PathEntry) -> (Vec<Vec<Token>>, u32) {
        let d = e.unit.done + e.unit.row.last().copied().unwrap_or(0);
        (e.stacks.clone(), d)
    }
}
