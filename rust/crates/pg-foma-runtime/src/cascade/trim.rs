//! Trimming (plan 3.3 steps 2-3) and the trimmed graph the read-out consumes.
//!
//! [`TrimmedGraph`] is the type W4/W5 adapt to: states are `(i, u, q_1 .. q_L)` records (word
//! position, edits used, one state per layer), arcs carry the written weight, the edits and a
//! tag-set id into `tag_sets`, and a self-loop (`src == dst`) is an arc like any other, so a tagged
//! epsilon loop survives as a tagged self-loop. For speller requests the arc key later also holds
//! the written character, folded unit and case mark (S.6): add them to [`GraphArc`] at W5.
//!
//! With a weight ceiling `theta` the graph is NOT exactly the `<= theta` subgraph: each arc passes
//! the creation test on its own (`g_min(src) + w + h <= theta`), so with a weak or absent `h` an
//! arc can lie only on accepting paths above `theta`. The read-out must filter by path weight.

use super::layer::Tag;

/// Words of key per state before the layer states: position and edits used.
pub const KEY_PREFIX: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphArc {
    pub src: u32,
    pub dst: u32,
    /// Fixed-point thousandths.
    pub weight: u32,
    pub edits: u8,
    /// Index into [`TrimmedGraph::tag_sets`]; the union over every move between the two states.
    pub tags: u32,
}

/// The product after the backward trim: only states on a path from the start to acceptance.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrimmedGraph {
    /// Number of layers `L`; each state key has `KEY_PREFIX + L` words.
    pub layers: usize,
    pub word_len: usize,
    /// `None` when the start is not on an accepting path (the word is proven out of the language).
    pub start: Option<u32>,
    pub keys: Vec<u32>,
    pub accepting: Vec<bool>,
    pub arcs: Vec<GraphArc>,
    /// Sorted, deduplicated tag sets.
    pub tag_sets: Vec<Box<[Tag]>>,
}

impl TrimmedGraph {
    pub fn state_count(&self) -> usize {
        self.accepting.len()
    }

    pub fn key(&self, s: u32) -> &[u32] {
        let w = KEY_PREFIX + self.layers;
        &self.keys[s as usize * w..(s as usize + 1) * w]
    }

    pub fn arc_tags(&self, a: &GraphArc) -> &[Tag] {
        &self.tag_sets[a.tags as usize]
    }
}

/// The untrimmed output of the forward pass.
pub struct RawGraph {
    pub layers: usize,
    pub word_len: usize,
    pub keys: Vec<u32>,
    pub accepting: Vec<bool>,
    pub arcs: Vec<GraphArc>,
    pub tag_sets: Vec<Box<[Tag]>>,
}

/// Backward trim: keep the states reachable from state 0 (the start) and co-reachable to acceptance.
pub fn trim(raw: RawGraph) -> TrimmedGraph {
    let n = raw.accepting.len();
    let mut radj: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut fadj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for a in &raw.arcs {
        radj[a.dst as usize].push(a.src);
        fadj[a.src as usize].push(a.dst);
    }
    let mut co = vec![false; n];
    let mut stack: Vec<u32> = (0..n as u32)
        .filter(|&s| raw.accepting[s as usize])
        .collect();
    for &s in &stack {
        co[s as usize] = true;
    }
    while let Some(s) = stack.pop() {
        for &p in &radj[s as usize] {
            if !co[p as usize] {
                co[p as usize] = true;
                stack.push(p);
            }
        }
    }
    let mut fw = vec![false; n];
    if n > 0 && co[0] {
        fw[0] = true;
        let mut stack = vec![0u32];
        while let Some(s) = stack.pop() {
            for &d in &fadj[s as usize] {
                if co[d as usize] && !fw[d as usize] {
                    fw[d as usize] = true;
                    stack.push(d);
                }
            }
        }
    }
    let mut map = vec![u32::MAX; n];
    let mut out = TrimmedGraph {
        layers: raw.layers,
        word_len: raw.word_len,
        start: None,
        keys: Vec::new(),
        accepting: Vec::new(),
        arcs: Vec::new(),
        tag_sets: raw.tag_sets,
    };
    let w = KEY_PREFIX + raw.layers;
    for s in 0..n {
        if fw[s] {
            map[s] = out.accepting.len() as u32;
            out.keys.extend_from_slice(&raw.keys[s * w..(s + 1) * w]);
            out.accepting.push(raw.accepting[s]);
        }
    }
    if n > 0 && fw[0] {
        out.start = Some(0);
    }
    for a in raw.arcs {
        if fw[a.src as usize] && fw[a.dst as usize] {
            out.arcs.push(GraphArc {
                src: map[a.src as usize],
                dst: map[a.dst as usize],
                ..a
            });
        }
    }
    out
}

/// Least remaining weight from every node to an accepting node (Dijkstra over the reversed arcs);
/// `u32::MAX` where there is none. `arcs` are `(src, dst, weight)`.
pub fn distance_to_accept(n: usize, arcs: &[(u32, u32, u32)], accepting: &[bool]) -> Vec<u32> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let mut radj: Vec<Vec<(u32, u32)>> = vec![Vec::new(); n];
    for &(s, d, w) in arcs {
        radj[d as usize].push((s, w));
    }
    let mut dist = vec![u32::MAX; n];
    let mut heap = BinaryHeap::new();
    for s in 0..n {
        if accepting[s] {
            dist[s] = 0;
            heap.push(Reverse((0u32, s as u32)));
        }
    }
    while let Some(Reverse((d, s))) = heap.pop() {
        if d > dist[s as usize] {
            continue;
        }
        for &(p, w) in &radj[s as usize] {
            let nd = d.saturating_add(w);
            if nd < dist[p as usize] {
                dist[p as usize] = nd;
                heap.push(Reverse((nd, p)));
            }
        }
    }
    dist
}
