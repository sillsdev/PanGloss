//! PROTOTYPE (PanGloss#481, not for landing): one word's unapplication graph over shapes.
//!
//! A node is a distinct `Shape` the word's analysis reached. A row is what one affix or
//! realizational rule's patterns give on that shape: the ordered `(allomorph, output shape)` edges.
//! The row depends on the rule and the shape alone, so every search path that arrives at the node
//! reads it; the per-path gates (syntactic and realizational FS, rule counts, trail, template state)
//! stay in the callers and run on every arrival.

use std::cell::RefCell;
use std::sync::OnceLock;

use pg_grammar_model::model::AllomorphId;
use pg_shape::Shape;
use rustc_hash::FxHashMap;

/// One matched edge: the allomorph that matched (affix rules) and the shape it leaves.
pub(crate) type Edge = (Option<AllomorphId>, Shape);

#[derive(Default)]
struct Graph {
    nodes: FxHashMap<Shape, u32>,
    /// (node, rule key, prune flag) -> edges; an empty row is a rule that does not match the shape.
    rows: FxHashMap<(u32, u32, bool), Box<[Edge]>>,
    last: Option<(Shape, u32)>,
    edges: usize,
    hits: usize,
    hit_edges: usize,
    miss_work: usize,
}

thread_local! {
    static GRAPH: RefCell<Option<Graph>> = const { RefCell::new(None) };
}

fn disabled() -> bool {
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| std::env::var_os("PG481_OFF").is_some())
}

fn report() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PG481_STATS").is_some())
}

/// Scope of one word's graph; dropped with the word.
pub struct WordGraph(bool);

/// Start a word's graph on this thread. Nested starts share the outer graph.
pub fn begin() -> WordGraph {
    if disabled() {
        return WordGraph(false);
    }
    GRAPH.with(|g| {
        let mut g = g.borrow_mut();
        if g.is_some() {
            WordGraph(false)
        } else {
            *g = Some(Graph::default());
            WordGraph(true)
        }
    })
}

impl Drop for WordGraph {
    fn drop(&mut self) {
        if !self.0 {
            return;
        }
        let graph = GRAPH.with(|g| g.borrow_mut().take());
        if let (true, Some(graph)) = (report(), graph) {
            // Bytes the graph itself holds beyond the shapes, which the output words also hold.
            let own_bytes = graph.nodes.capacity() * 16
                + graph.rows.capacity() * 40
                + graph.edges * std::mem::size_of::<Edge>();
            eprintln!(
                "PG481 nodes={} rows={} edges={} hits={} hit_edges={} miss_work={} own_bytes={}",
                graph.nodes.len(),
                graph.rows.len(),
                graph.edges,
                graph.hits,
                graph.hit_edges,
                graph.miss_work,
                own_bytes
            );
        }
    }
}

/// The node of `shape`, or `None` when no word graph is active.
pub(crate) fn node(shape: &Shape) -> Option<u32> {
    GRAPH.with(|g| {
        let mut g = g.borrow_mut();
        let graph = g.as_mut()?;
        if let Some((last, id)) = &graph.last {
            if last.same_allocation(shape) {
                return Some(*id);
            }
        }
        let next = graph.nodes.len() as u32;
        let id = *graph.nodes.entry(shape.clone()).or_insert(next);
        graph.last = Some((shape.clone(), id));
        Some(id)
    })
}

/// The stored row of `rule` on `node`.
pub(crate) fn row(node: u32, rule: u32, prune: bool) -> Option<Vec<Edge>> {
    GRAPH.with(|g| {
        let mut g = g.borrow_mut();
        let graph = g.as_mut()?;
        let edges = graph.rows.get(&(node, rule, prune))?.to_vec();
        graph.hits += 1;
        graph.hit_edges += edges.len();
        Some(edges)
    })
}

/// Record the row of `rule` on `node`; `work` is what computing it cost.
pub(crate) fn set_row(node: u32, rule: u32, prune: bool, edges: Vec<Edge>, work: usize) {
    GRAPH.with(|g| {
        let mut g = g.borrow_mut();
        if let Some(graph) = g.as_mut() {
            graph.edges += edges.len();
            graph.miss_work += work;
            graph
                .rows
                .insert((node, rule, prune), edges.into_boxed_slice());
        }
    })
}
