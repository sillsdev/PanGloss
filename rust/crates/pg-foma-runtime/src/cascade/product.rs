//! The lazy product (plan section 3, S.3, S.4, S.5): best-first expansion over interned
//! `(i, u, q_1 .. q_L)` states, a pre-pass over the eager bottom suffix, and the backward trim.
//!
//! Decisions that are easy to break and are pinned by the tests:
//! - **No epsilon-filter.** Every interleaving is explored; states are interned on the key and
//!   product arcs are stored per `(src, dst, weight, edits)` with the UNION of their tag sets.
//! - **The top layer's input tape is free**: its token and class arcs are moves too (the main pass
//!   takes `arcs_free` at layer 0, as the pre-pass does at its cut), so a top layer with input
//!   arcs never silently loses a word.
//! - **Every layer's epsilon-input arcs are moves at every state**, not only when a lower layer
//!   asks for a token (mutation C63).
//! - **The pre-pass has its own budgets and never makes a word `Incomplete`**: an overrun frees it,
//!   shortens the suffix, and finally means `h = 0` (C51, C65).
//! - **Budgets end the word.** An overrun returns `Incomplete` naming the dimension; nothing
//!   truncates the worklist and carries on (C20, C36). Bytes are charged before allocation.
//! - **Creation test** (S.5): a move to a state with `u` edits used and path weight `g` is taken
//!   iff `u <= E`, `h_{E-u} < inf` and `g + h <= theta`. `h` is exact on the suffix product, so
//!   best-first order on `f = g + h` pops with non-decreasing `f`.
//! - **Per-word scratch** lives in [`ProductScratch`] and is reset at the start of every run (C77).
//!
//! Edit counts: a path's edits `u` are part of the state key. The pre-pass table is keyed by the
//! suffix tuple and `u`, which equals the edits used so far only if every layer that carries an
//! edit is inside the suffix; the effective cut is therefore pulled up to the first such layer.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use super::budget::{
    state_bytes, tagset_bytes, CascadeStop, PrepassBudget, WordBudget, WordMeter, ID_LIMIT,
    QUEUE_ENTRY_BYTES,
};
use super::layer::{Arc, Layer, LayerScratch, StateId, Tag, Token};
use super::trim::{
    distance_to_accept, trim, trim_bytes, GraphArc, RawGraph, TrimmedGraph, KEY_PREFIX,
};

const INF: u32 = u32::MAX;

/// Which suffixes to try for the pre-pass, first to last, each under a fresh `budget`.
/// A cut is the index of the first suffix layer. After the last overrun there is no pre-pass.
#[derive(Clone, Debug)]
pub struct PrepassPlan {
    pub cuts: Vec<usize>,
    pub budget: PrepassBudget,
}

impl PrepassPlan {
    pub fn none() -> Self {
        PrepassPlan {
            cuts: Vec::new(),
            budget: PrepassBudget::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Request<'w> {
    pub word: &'w [Token],
    pub max_edits: u8,
    /// Weight ceiling in thousandths.
    pub theta: Option<u32>,
    pub budget: WordBudget,
    pub prepass: PrepassPlan,
}

impl<'w> Request<'w> {
    pub fn new(word: &'w [Token], max_edits: u8) -> Self {
        Request {
            word,
            max_edits,
            theta: None,
            budget: WordBudget::for_edits(max_edits),
            prepass: PrepassPlan::none(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrepassUsed {
    /// A pre-pass over the suffix starting at this layer index (the effective cut).
    Suffix(usize),
    None,
}

/// A requested pre-pass cut that was changed or skipped, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrepassNote {
    /// The cut was moved up to the first edit-carrying layer (the table's `u` must equal the word's).
    PulledUpToFirstEdit { requested: usize, cut: usize },
    /// The suffix from `cut` would hold a lazy layer: no pre-pass for this request.
    LazyLayerInSuffix { requested: usize, cut: usize },
    /// The cut leaves no suffix layer.
    EmptySuffix { requested: usize },
    /// Two requested cuts became the same suffix; the second is not a new rung.
    DuplicateCut { requested: usize, cut: usize },
}

#[derive(Clone, Debug)]
pub struct Report {
    pub states: u64,
    pub arcs: u64,
    pub steps: u64,
    pub bytes: u64,
    pub peak_queue: usize,
    pub prepass: PrepassUsed,
    pub prepass_attempts_overrun: u32,
    /// Why requested cuts did not become attempts (the ladder collapsing is visible here).
    pub prepass_notes: Vec<PrepassNote>,
    pub prepass_states: u64,
    pub prepass_steps: u64,
    pub prepass_bytes: u64,
    /// True when `f` never decreased from one expansion to the next (consistency of `h`).
    pub f_monotone: bool,
    /// The least weight of an accepting path, over all accepting states created.
    pub best_weight: Option<u64>,
}

#[derive(Debug)]
pub enum Outcome {
    Complete { graph: TrimmedGraph, report: Report },
    Incomplete { stop: CascadeStop, report: Report },
}

impl Outcome {
    pub fn report(&self) -> &Report {
        match self {
            Outcome::Complete { report, .. } | Outcome::Incomplete { report, .. } => report,
        }
    }
}

#[derive(Default)]
struct Tables {
    keys: Vec<u32>,
    index: HashMap<Box<[u32]>, u32>,
    g: Vec<u64>,
    closed: Vec<bool>,
    accepting: Vec<bool>,
    arcs: Vec<GraphArc>,
    arc_index: HashMap<(u32, u32, u32, u8), u32>,
    tag_sets: Vec<Box<[Tag]>>,
    tag_index: HashMap<Box<[Tag]>, u32>,
}

/// Everything a word changes. The layers stay shared and immutable; this is reset per word.
pub struct ProductScratch {
    layer: Vec<LayerScratch>,
    tables: Tables,
}

impl ProductScratch {
    pub fn new(layer_count: usize) -> Self {
        ProductScratch {
            layer: (0..layer_count).map(|_| LayerScratch::default()).collect(),
            tables: Tables::default(),
        }
    }

    pub fn reset(&mut self) {
        for l in &mut self.layer {
            l.reset();
        }
        self.tables = Tables::default();
    }
}

/// A move: the layers whose state changed, the position, and the summed weight, edits and tags.
struct Acc {
    changed: Vec<(usize, StateId)>,
    weight: u64,
    edits: u32,
    tags: Vec<Tag>,
    i: usize,
}

/// Charges `(steps, bytes)` for one layer call; the bytes are what a lazy layer interned.
type Steps<'c> = &'c mut dyn FnMut(u64, u64) -> Result<(), CascadeStop>;
/// The product bytes still available, handed to a layer's scratch before each call.
type Left<'c> = &'c dyn Fn() -> u64;
type Emit<'c> = &'c mut dyn FnMut(&Acc) -> Result<(), CascadeStop>;

struct Walk<'c, 'a> {
    layers: &'a [&'a dyn Layer],
    lscr: &'c mut [LayerScratch],
    q: &'c [u32],
    word: &'c [Token],
    steps: Steps<'c>,
    left: Left<'c>,
    emit: Emit<'c>,
}

impl<'a> Walk<'_, 'a> {
    fn begin(&mut self, l: usize) {
        let left = (self.left)();
        self.lscr[l].begin(left);
    }

    /// Charge a layer call's steps and interned bytes; a refused intern ends the word on bytes.
    fn end(&mut self, l: usize, st: u64) -> Result<(), CascadeStop> {
        if self.lscr[l].refused() {
            return Err(CascadeStop::ProductBytes);
        }
        let bytes = self.lscr[l].take_charged();
        (self.steps)(st, bytes)
    }

    fn follow(&mut self, l: usize, arc: Arc<'a>, acc: &mut Acc) -> Result<(), CascadeStop> {
        let saved = (
            acc.changed.len(),
            acc.weight,
            acc.edits,
            acc.tags.len(),
            acc.i,
        );
        acc.changed.push((l, arc.to));
        acc.weight += arc.weight as u64;
        acc.edits += arc.edits as u32;
        acc.tags.extend_from_slice(arc.tags);
        let r = match arc.output {
            None => (self.emit)(acc),
            Some(t) if l + 1 == self.layers.len() => {
                if acc.i < self.word.len() && self.word[acc.i] == t {
                    acc.i += 1;
                    (self.emit)(acc)
                } else {
                    Ok(())
                }
            }
            Some(t) => {
                let layer: &'a dyn Layer = self.layers[l + 1];
                let mut next = Vec::new();
                self.begin(l + 1);
                let st = layer.arcs_from(&mut self.lscr[l + 1], self.q[l + 1], Some(t), &mut |a| {
                    next.push(a)
                });
                self.end(l + 1, st).and_then(|()| {
                    next.into_iter()
                        .try_for_each(|a| self.follow(l + 1, a, acc))
                })
            }
        };
        acc.changed.truncate(saved.0);
        acc.weight = saved.1;
        acc.edits = saved.2;
        acc.tags.truncate(saved.3);
        acc.i = saved.4;
        r
    }

    /// Every move from the tuple: a chain from any layer `>= lo`, down while a token is written, to an epsilon output or the word.
    fn all_moves(&mut self, lo: usize, free_top: bool, i: usize) -> Result<(), CascadeStop> {
        for l in lo..self.layers.len() {
            let layer: &'a dyn Layer = self.layers[l];
            let mut first = Vec::new();
            self.begin(l);
            let st = if l == lo && free_top {
                layer.arcs_free(&mut self.lscr[l], self.q[l], &mut |a| first.push(a))
            } else {
                layer.arcs_from(&mut self.lscr[l], self.q[l], None, &mut |a| first.push(a))
            };
            self.end(l, st)?;
            for a in first {
                let mut acc = Acc {
                    changed: Vec::new(),
                    weight: 0,
                    edits: 0,
                    tags: Vec::new(),
                    i,
                };
                self.follow(l, a, &mut acc)?;
            }
        }
        Ok(())
    }
}

struct PrepassTable {
    cut: usize,
    dist: HashMap<Box<[u32]>, u32>,
}

enum PrepassResult {
    Table(PrepassTable),
    Overrun,
}

#[derive(Default)]
struct PrepassStats {
    states: u64,
    steps: u64,
    bytes: u64,
}

pub struct Product<'a> {
    layers: &'a [&'a dyn Layer],
}

fn sorted_union(a: &[Tag], b: &[Tag]) -> Vec<Tag> {
    let mut m = a.to_vec();
    m.extend_from_slice(b);
    m.sort();
    m.dedup();
    m
}

impl Tables {
    /// Intern a tag set, charging the bytes before it is stored.
    fn intern_tags(&mut self, meter: &mut WordMeter, tags: &[Tag]) -> Result<u32, CascadeStop> {
        if let Some(&id) = self.tag_index.get(tags) {
            return Ok(id);
        }
        if self.tag_sets.len() as u64 >= ID_LIMIT {
            return Err(CascadeStop::ProductBytes);
        }
        meter.charge_bytes(tagset_bytes(tags.len()))?;
        let id = self.tag_sets.len() as u32;
        self.tag_sets.push(tags.into());
        self.tag_index.insert(tags.into(), id);
        Ok(id)
    }
}

/// Why a set of layers is not a product.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductError {
    NoLayers,
    /// A layer's alphabet differs from layer 0's, so a token written above would silently match nothing.
    UniverseMismatch {
        layer: usize,
        expected: u32,
        found: u32,
    },
}

impl<'a> Product<'a> {
    pub fn new(layers: &'a [&'a dyn Layer]) -> Result<Self, ProductError> {
        let first = layers.first().ok_or(ProductError::NoLayers)?.universe();
        for (layer, l) in layers.iter().enumerate() {
            if l.universe() != first {
                return Err(ProductError::UniverseMismatch {
                    layer,
                    expected: first,
                    found: l.universe(),
                });
            }
        }
        Ok(Product { layers })
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// The suffix start used for a requested cut (pulled up over edit layers), or why there is none.
    fn effective_cut(&self, requested: usize) -> Result<usize, PrepassNote> {
        let l = self.layers.len();
        let first_edit = self.layers.iter().position(|x| x.has_edits()).unwrap_or(l);
        let cut = requested.min(first_edit);
        if cut >= l {
            return Err(PrepassNote::EmptySuffix { requested });
        }
        if self.layers[cut..].iter().any(|x| !x.is_eager()) {
            return Err(PrepassNote::LazyLayerInSuffix { requested, cut });
        }
        Ok(cut)
    }

    fn prepass(&self, cut: usize, req: &Request) -> (PrepassResult, PrepassStats) {
        let mut stats = PrepassStats::default();
        let mut meter = WordMeter::for_prepass(req.prepass.budget);
        let r = self.prepass_inner(cut, req, &mut meter);
        stats.states = meter.states;
        stats.steps = meter.steps;
        stats.bytes = meter.bytes;
        match r {
            Ok(t) => (PrepassResult::Table(t), stats),
            Err(_) => (PrepassResult::Overrun, stats),
        }
    }

    fn prepass_inner(
        &self,
        cut: usize,
        req: &Request,
        meter: &mut WordMeter,
    ) -> Result<PrepassTable, CascadeStop> {
        let l = self.layers.len();
        let n = req.word.len();
        let e_max = req.max_edits as u32;
        let width = KEY_PREFIX + (l - cut);
        let mut lscr: Vec<LayerScratch> = (0..l).map(|_| LayerScratch::default()).collect();
        let mut keys: Vec<u32> = Vec::new();
        let mut index: HashMap<Box<[u32]>, u32> = HashMap::new();
        let mut arcs: Vec<(u32, u32, u32)> = Vec::new();
        let mut start_key = vec![0u32; width];
        for k in cut..l {
            start_key[KEY_PREFIX + k - cut] = self.layers[k].start(&mut lscr[k]);
        }
        meter.charge_state(state_bytes(width))?;
        keys.extend_from_slice(&start_key);
        index.insert(start_key.into(), 0);
        let mut next = 0usize;
        while next < index.len() {
            let cur: Vec<u32> = keys[next * width..(next + 1) * width].to_vec();
            let cur_id = next as u32;
            next += 1;
            let mut q = vec![0u32; l];
            q[cut..].copy_from_slice(&cur[KEY_PREFIX..]);
            let meter_cell = std::cell::RefCell::new(&mut *meter);
            let mut steps = |s: u64, b: u64| {
                let mut m = meter_cell.borrow_mut();
                m.charge_steps(s)?;
                m.charge_bytes(b)
            };
            let left = || {
                let m = meter_cell.borrow();
                m.limits().bytes.saturating_sub(m.bytes)
            };
            let mut pending: Vec<(Vec<u32>, u32)> = Vec::new();
            let mut emit = |acc: &Acc| -> Result<(), CascadeStop> {
                let u2 = cur[1] + acc.edits;
                if u2 > e_max {
                    return Ok(());
                }
                let mut key = cur.clone();
                key[0] = acc.i as u32;
                key[1] = u2;
                for &(ly, s) in &acc.changed {
                    key[KEY_PREFIX + ly - cut] = s;
                }
                pending.push((key, acc.weight.min(INF as u64 - 1) as u32));
                Ok(())
            };
            let mut walk = Walk {
                layers: self.layers,
                lscr: &mut lscr,
                q: &q,
                word: req.word,
                steps: &mut steps,
                left: &left,
                emit: &mut emit,
            };
            walk.all_moves(cut, true, cur[0] as usize)?;
            // transient: the moves of this one state, charged to the pre-pass meter
            meter.charge_bytes(pending.len() as u64 * (4 * width as u64 + 8))?;
            for (key, w) in pending {
                let id = match index.get(key.as_slice()) {
                    Some(&id) => id,
                    None => {
                        meter.charge_state(state_bytes(width))?;
                        let id = index.len() as u32;
                        keys.extend_from_slice(&key);
                        index.insert(key.into_boxed_slice(), id);
                        id
                    }
                };
                meter.charge_bytes(super::budget::ARC_BYTES)?;
                arcs.push((cur_id, id, w));
            }
        }
        let count = index.len();
        let accepting: Vec<bool> = (0..count)
            .map(|s| {
                let key = &keys[s * width..(s + 1) * width];
                key[0] as usize == n
                    && (cut..l)
                        .all(|k| self.layers[k].is_final(&lscr[k], key[KEY_PREFIX + k - cut]))
            })
            .collect();
        // transient: reverse adjacency, distances and the table, charged before they are built
        meter.charge_bytes(count as u64 * 48 + arcs.len() as u64 * 24)?;
        let dist = distance_to_accept(count, &arcs, &accepting);
        let mut table = HashMap::new();
        for s in 0..count {
            if dist[s] != INF {
                table.insert(keys[s * width..(s + 1) * width].into(), dist[s]);
            }
        }
        Ok(PrepassTable { cut, dist: table })
    }

    /// Run one request. `scratch` is reset first, so the same scratch serves any number of words.
    pub fn run(&self, scratch: &mut ProductScratch, req: &Request) -> Outcome {
        scratch.reset();
        let l = self.layers.len();
        assert_eq!(scratch.layer.len(), l);
        let n = req.word.len();
        let e_max = req.max_edits as u32;
        let width = KEY_PREFIX + l;

        // Pre-pass: first attempt, fallbacks, then none (h = 0). Never `Incomplete`.
        let mut report = Report {
            states: 0,
            arcs: 0,
            steps: 0,
            bytes: 0,
            peak_queue: 0,
            prepass: PrepassUsed::None,
            prepass_attempts_overrun: 0,
            prepass_notes: Vec::new(),
            prepass_states: 0,
            prepass_steps: 0,
            prepass_bytes: 0,
            f_monotone: true,
            best_weight: None,
        };
        let mut table: Option<PrepassTable> = None;
        let mut tried: Vec<usize> = Vec::new();
        for &requested in &req.prepass.cuts {
            let cut = match self.effective_cut(requested) {
                Ok(c) => c,
                Err(note) => {
                    report.prepass_notes.push(note);
                    continue;
                }
            };
            if cut < requested {
                report
                    .prepass_notes
                    .push(PrepassNote::PulledUpToFirstEdit { requested, cut });
            }
            if tried.contains(&cut) {
                report
                    .prepass_notes
                    .push(PrepassNote::DuplicateCut { requested, cut });
                continue;
            }
            tried.push(cut);
            let (res, st) = self.prepass(cut, req);
            report.prepass_states += st.states;
            report.prepass_steps += st.steps;
            report.prepass_bytes += st.bytes;
            match res {
                PrepassResult::Table(t) => {
                    report.prepass = PrepassUsed::Suffix(t.cut);
                    table = Some(t);
                    break;
                }
                PrepassResult::Overrun => report.prepass_attempts_overrun += 1,
            }
        }
        let h_of = |key: &[u32]| -> u32 {
            match &table {
                None => 0,
                Some(t) => {
                    let mut k = Vec::with_capacity(KEY_PREFIX + l - t.cut);
                    k.extend_from_slice(&key[..KEY_PREFIX]);
                    k.extend_from_slice(&key[KEY_PREFIX + t.cut..]);
                    t.dist.get(k.as_slice()).copied().unwrap_or(INF)
                }
            }
        };

        let mut meter = WordMeter::new(req.budget);
        let ProductScratch { layer, tables } = scratch;
        let mut heap: BinaryHeap<Reverse<(u64, u32, u64)>> = BinaryHeap::new();

        macro_rules! finish_incomplete {
            ($stop:expr) => {{
                report.states = meter.states;
                report.arcs = meter.arcs;
                report.steps = meter.steps;
                report.bytes = meter.bytes;
                return Outcome::Incomplete {
                    stop: $stop,
                    report,
                };
            }};
        }

        let mut start_key = vec![0u32; width];
        for k in 0..l {
            layer[k].begin(meter.limits().bytes.saturating_sub(meter.bytes));
            start_key[KEY_PREFIX + k] = self.layers[k].start(&mut layer[k]);
            if layer[k].refused() {
                finish_incomplete!(CascadeStop::ProductBytes)
            }
            if let Err(s) = meter.charge_bytes(layer[k].take_charged()) {
                finish_incomplete!(s)
            }
        }
        let h0 = h_of(&start_key);
        let theta_ok = |g: u64, h: u32| req.theta.is_none_or(|t| g + h as u64 <= t as u64);
        if h0 == INF || !theta_ok(0, h0) {
            // Proven non-word: no accepting path can exist.
            report.states = 0;
            return Outcome::Complete {
                graph: TrimmedGraph {
                    layers: l,
                    word_len: n,
                    theta: req.theta,
                    ..TrimmedGraph::default()
                },
                report,
            };
        }
        if let Err(s) = meter.charge_state(state_bytes(width)) {
            finish_incomplete!(s)
        }
        if let Err(s) = meter.charge_bytes(QUEUE_ENTRY_BYTES) {
            finish_incomplete!(s)
        }
        tables.keys.extend_from_slice(&start_key);
        tables.index.insert(start_key.into(), 0);
        tables.g.push(0);
        tables.closed.push(false);
        tables.accepting.push(false);
        heap.push(Reverse((h0 as u64, 0, 0)));

        let mut last_f = 0u64;
        while let Some(Reverse((f, id, g))) = heap.pop() {
            report.peak_queue = report.peak_queue.max(heap.len() + 1);
            let idu = id as usize;
            if tables.closed[idu] || tables.g[idu] != g {
                continue;
            }
            tables.closed[idu] = true;
            debug_assert!(f >= last_f, "f decreased: h is not consistent");
            if f < last_f {
                report.f_monotone = false;
            }
            last_f = f;
            let cur: Vec<u32> = tables.keys[idu * width..(idu + 1) * width].to_vec();
            let accepting = cur[0] as usize == n
                && (0..l).all(|k| self.layers[k].is_final(&layer[k], cur[KEY_PREFIX + k]));
            if accepting {
                tables.accepting[idu] = true;
                report.best_weight = Some(report.best_weight.map_or(g, |b| b.min(g)));
            }
            let q: Vec<u32> = cur[KEY_PREFIX..].to_vec();
            let result = {
                let tables = &mut *tables;
                let meter_cell = std::cell::RefCell::new(&mut meter);
                let heap_cell = std::cell::RefCell::new(&mut heap);
                let mut steps = |s: u64, b: u64| {
                    let mut m = meter_cell.borrow_mut();
                    m.charge_steps(s)?;
                    m.charge_bytes(b)
                };
                let left = || {
                    let m = meter_cell.borrow();
                    m.limits().bytes.saturating_sub(m.bytes)
                };
                let mut emit = |acc: &Acc| -> Result<(), CascadeStop> {
                    let u2 = cur[1] + acc.edits;
                    if u2 > e_max {
                        return Ok(());
                    }
                    let mut key = cur.clone();
                    key[0] = acc.i as u32;
                    key[1] = u2;
                    for &(ly, s) in &acc.changed {
                        key[KEY_PREFIX + ly] = s;
                    }
                    let h = h_of(&key);
                    if h == INF {
                        return Ok(());
                    }
                    let g2 = g + acc.weight;
                    if !theta_ok(g2, h) {
                        return Ok(());
                    }
                    let mut meter = meter_cell.borrow_mut();
                    let dst = match tables.index.get(key.as_slice()) {
                        Some(&d) => d,
                        None => {
                            meter.charge_state(state_bytes(width))?;
                            let d = tables.g.len() as u32;
                            tables.keys.extend_from_slice(&key);
                            tables.index.insert(key.into_boxed_slice(), d);
                            tables.g.push(u64::MAX);
                            tables.closed.push(false);
                            tables.accepting.push(false);
                            d
                        }
                    };
                    if g2 < tables.g[dst as usize] && !tables.closed[dst as usize] {
                        meter.charge_bytes(QUEUE_ENTRY_BYTES)?;
                        tables.g[dst as usize] = g2;
                        let kd = &tables.keys[dst as usize * width..(dst as usize + 1) * width];
                        let hd = h_of(kd);
                        heap_cell
                            .borrow_mut()
                            .push(Reverse((g2 + hd as u64, dst, g2)));
                    }
                    let mut tags = acc.tags.clone();
                    tags.sort();
                    tags.dedup();
                    let w32 = acc.weight.min(INF as u64 - 1) as u32;
                    let edits8 =
                        u8::try_from(acc.edits).expect("edits fit u8: u2 <= e_max was checked");
                    let akey = (id, dst, w32, edits8);
                    match tables.arc_index.get(&akey).copied() {
                        None => {
                            meter.charge_arc()?;
                            let tid = tables.intern_tags(&mut meter, &tags)?;
                            tables.arc_index.insert(akey, tables.arcs.len() as u32);
                            tables.arcs.push(GraphArc {
                                src: id,
                                dst,
                                weight: w32,
                                edits: edits8,
                                tags: tid,
                            });
                        }
                        Some(ai) => {
                            let old = tables.arcs[ai as usize].tags;
                            let merged = sorted_union(&tables.tag_sets[old as usize], &tags);
                            let tid = tables.intern_tags(&mut meter, &merged)?;
                            tables.arcs[ai as usize].tags = tid;
                        }
                    }
                    Ok(())
                };
                let mut walk = Walk {
                    layers: self.layers,
                    lscr: layer,
                    q: &q,
                    word: req.word,
                    steps: &mut steps,
                    left: &left,
                    emit: &mut emit,
                };
                walk.all_moves(0, true, cur[0] as usize)
            };
            if let Err(stop) = result {
                finish_incomplete!(stop)
            }
        }

        // the trim's adjacency, flags and copy of the graph, charged before they are built
        if let Err(s) =
            meter.charge_bytes(trim_bytes(tables.accepting.len(), tables.arcs.len(), width))
        {
            finish_incomplete!(s)
        }
        report.states = meter.states;
        report.arcs = meter.arcs;
        report.steps = meter.steps;
        report.bytes = meter.bytes;
        let t = std::mem::take(tables);
        let graph = trim(RawGraph {
            layers: l,
            word_len: n,
            theta: req.theta,
            keys: t.keys,
            accepting: t.accepting,
            arcs: t.arcs,
            tag_sets: t.tag_sets,
        });
        Outcome::Complete { graph, report }
    }
}
