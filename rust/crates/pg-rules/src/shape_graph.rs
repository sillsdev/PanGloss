//! The per-word shape graph: what each affix or realizational rule gives on each shape a word's analysis reaches.
//!
//! The analysis search walks a tree of `Word`s, but matching a rule's patterns reads the word's `Shape` and nothing
//! else, so the same (rule, shape) match was paid once for every `Word` that carried the shape and passed the rule's
//! gates. Here a node is an interned shape and a row is one rule's ordered `(allomorph, output node)` edges on one
//! node, an empty row meaning "no match". The first application of a rule to a shape matches and stores its row;
//! every later arrival reads it. Everything that varies per path (syntactic and realizational features, rule
//! counts, trail, template state, the cascade's recursion) stays in the callers and runs on every arrival: a row only
//! replaces the per-shape part of `AnalysisAffixProcessRule.Apply`, so results and their order are those of the
//! search without the graph.
//!
//! This is a bounded per-word table keyed by shape and rule, not a cache with a policy: nothing is admitted on a
//! heuristic and nothing is evicted, and rows are at most distinct shapes times rules. Compounding is not covered,
//! because its match also depends on the non-head root filter.
//!
//! The graph lives in one word's `WordBudget` and is dropped with it. Its real heap bytes are charged to the word's
//! memory meter as optional state (`WorkBudget::try_charge_optional`): the search's own charges ignore them, so the
//! graph never changes whether a word is `MEMCAP`. It takes at most an eighth of `--word-mem-cap-mib` and at most
//! what the search's own retained state leaves free; when a charge is refused the whole graph is dropped and the
//! rest of the word runs without it, which never changes a result.
//!
//! A row read is charged `1 + edges` work units, and never more than the search that computed the row cost, so
//! `--work-cap` still bounds the word and every hit still moves it. The charge is the same whether or not a trace
//! recorder or stats collector observes the application: an observed application charges the row, then re-runs the
//! match with its own reservations suspended so the observer sees every event of the real search.

use std::cell::RefCell;
use std::rc::Rc;

use pg_fst::work::{SuspendedWork, WorkBudget};
use pg_grammar_model::model::{AllomorphId, MRuleId};
use pg_shape::Shape;
use rustc_hash::FxHashMap;

use crate::word::Word;

#[cfg(test)]
thread_local! {
    static TEST_DISABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Turn the graph off on this thread, so a test can compare a run with and without it.
#[cfg(test)]
pub(crate) fn test_disable(disabled: bool) {
    TEST_DISABLED.with(|value| value.set(disabled));
}

/// The graph may take at most this fraction of its word's `--word-mem-cap-mib` allowance, so optional state cannot crowd out the search's own.
const ALLOWANCE_SHARE: usize = 8;

const NO_ALLOMORPH: u32 = u32::MAX;

/// An allomorph that matched (`None` for a rule kind that does not name one) and the shape it left.
pub type Matched = (Option<AllomorphId>, Shape);

/// One matched allomorph and the node its output shape is.
#[derive(Clone, Copy)]
struct Edge {
    allomorph: u32,
    node: u32,
}

/// Everything besides the shape that the match reads: the rule, and the grammar-constant copy-agreement flag.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RowKey {
    node: u32,
    rule: u32,
    prune_disagreeing_copies: bool,
}

struct Row {
    /// Work a read charges: `1 + edges`, never above what computing the row cost.
    charge: u32,
    /// `None` for no match at all, which allocates nothing.
    edges: Option<Rc<[Edge]>>,
}

const NODE_SLOT: usize = std::mem::size_of::<(Shape, u32)>();
const ROW_SLOT: usize = std::mem::size_of::<(RowKey, Row)>();
const SHAPE_SLOT: usize = std::mem::size_of::<Shape>();

/// A glibc chunk: 8-byte header, 16-byte steps, 32 bytes at least.
fn chunk(bytes: usize) -> usize {
    if bytes == 0 {
        0
    } else {
        ((bytes + 8 + 15) & !15).max(32)
    }
}

/// Buckets a hash table with this usable capacity has allocated.
fn buckets_of(capacity: usize) -> usize {
    match capacity {
        0 => 0,
        1..=7 => (capacity + 1).next_power_of_two(),
        _ => capacity / 7 * 8,
    }
}

/// Bytes of a hash table with this many buckets: slots, one control byte per bucket and a trailing group.
fn table_bytes(buckets: usize, slot: usize) -> usize {
    if buckets == 0 {
        0
    } else {
        chunk(((buckets * slot + 15) & !15) + buckets + 16)
    }
}

/// Buckets after the table next doubles.
fn grown(buckets: usize) -> usize {
    (buckets * 2).max(4)
}

/// Bytes of the reference-counted edge array of a row: two counts then the edges.
fn edges_bytes(edges: usize) -> usize {
    chunk(16 + edges * std::mem::size_of::<Edge>())
}

#[derive(Default)]
struct Inner {
    nodes: FxHashMap<Shape, u32>,
    /// The interned shape of each node, in node order.
    shapes: Vec<Shape>,
    rows: FxHashMap<RowKey, Row>,
    /// Heap bytes of the interned shapes, each distinct allocation once.
    shape_bytes: usize,
    /// Heap bytes of the rows' edge arrays.
    edge_bytes: usize,
    /// Bytes reserved against the word allowance so far.
    charged: usize,
    /// A charge was refused: the graph holds nothing and answers nothing for the rest of the word.
    dead: bool,
    hits: usize,
}

impl Inner {
    /// Real heap bytes held, tables included at one and a half times their size so a resize's old and new tables fit, after adding the given nodes, rows and bytes; zero while nothing is held.
    fn accounted(&self, extra_nodes: usize, extra_rows: usize, extra_bytes: usize) -> usize {
        if self.nodes.is_empty() && self.rows.is_empty() && extra_nodes == 0 && extra_rows == 0 {
            return 0;
        }
        let mut node_buckets = buckets_of(self.nodes.capacity());
        if self.nodes.len() + extra_nodes > self.nodes.capacity() {
            node_buckets = grown(node_buckets);
        }
        let mut row_buckets = buckets_of(self.rows.capacity());
        if self.rows.len() + extra_rows > self.rows.capacity() {
            row_buckets = grown(row_buckets);
        }
        let mut shapes_capacity = self.shapes.capacity();
        if self.shapes.len() + extra_nodes > shapes_capacity {
            shapes_capacity = (shapes_capacity * 2).max(4);
        }
        let tables = table_bytes(node_buckets, NODE_SLOT)
            + table_bytes(row_buckets, ROW_SLOT)
            + chunk(shapes_capacity * SHAPE_SLOT);
        self.shape_bytes + self.edge_bytes + extra_bytes + tables + tables / 2
    }

    /// The most accounted bytes the graph may hold now: an eighth of the word allowance, and what the search's own retained state leaves free, so the graph yields as the search grows.
    fn room(work: &WorkBudget) -> usize {
        match (work.word_cap(), work.word_headroom()) {
            (Some(allowance), Some(headroom)) => (allowance / ALLOWANCE_SHARE).min(headroom),
            _ => usize::MAX,
        }
    }

    /// Set the reservation against the word allowance to `want` bytes; false when growing it is refused.
    fn charge_to(&mut self, work: &WorkBudget, want: usize) -> bool {
        if want > self.charged {
            if want > Self::room(work) || !work.try_charge_optional(want - self.charged) {
                return false;
            }
        } else {
            work.release_optional(self.charged - want);
        }
        self.charged = want;
        true
    }

    /// Reserve room for the additions before making them.
    fn reserve(
        &mut self,
        work: &WorkBudget,
        extra_nodes: usize,
        extra_rows: usize,
        extra_bytes: usize,
    ) -> bool {
        let want = self.accounted(extra_nodes, extra_rows, extra_bytes);
        self.charge_to(work, want)
    }

    /// Give back what the additions did not use.
    fn settle(&mut self, work: &WorkBudget) {
        let held = self.accounted(0, 0, 0);
        if !self.charge_to(work, held) {
            self.kill(work);
        }
    }

    /// Drop everything and stop answering; the word carries on without the graph.
    fn kill(&mut self, work: &WorkBudget) {
        work.release_optional(self.charged);
        *self = Inner {
            dead: true,
            hits: self.hits,
            ..Inner::default()
        };
    }

    /// The node of `shape`, interning it when new; `None` once the graph is dead.
    fn node(&mut self, work: &WorkBudget, shape: &Shape) -> Option<u32> {
        if self.dead {
            return None;
        }
        if let Some(&node) = self.nodes.get(shape) {
            return Some(node);
        }
        let bytes = shape.heap_bytes();
        if !self.reserve(work, 1, 0, bytes) {
            self.kill(work);
            return None;
        }
        let node = self.shapes.len() as u32;
        self.shapes.push(shape.clone());
        self.nodes.insert(shape.clone(), node);
        self.shape_bytes += bytes;
        self.settle(work);
        (!self.dead).then_some(node)
    }
}

/// One word's shape graph.
pub struct ShapeGraph {
    inner: RefCell<Inner>,
    work: WorkBudget,
}

impl ShapeGraph {
    pub fn new(work: WorkBudget) -> Self {
        Self {
            inner: RefCell::default(),
            work,
        }
    }

    /// Row reads that found a stored row so far.
    pub fn hits(&self) -> usize {
        self.inner.borrow().hits
    }

    /// Accounted real heap bytes held now, resize margin included.
    pub fn bytes(&self) -> usize {
        self.inner.borrow().accounted(0, 0, 0)
    }

    /// Interned shapes.
    pub fn nodes(&self) -> usize {
        self.inner.borrow().shapes.len()
    }

    /// Stored rows, empty ones included.
    pub fn rows(&self) -> usize {
        self.inner.borrow().rows.len()
    }

    /// Whether a refused charge dropped the graph.
    pub fn is_dead(&self) -> bool {
        self.inner.borrow().dead
    }

    /// Store the row of `rule` on `input`: the ordered `(allomorph, output shape)` edges its patterns gave, and what computing them cost in work units. Returns the interned shapes of the outputs, in order, or `None` when nothing was stored. A row is not stored when it cost nothing to compute.
    #[doc(hidden)]
    pub fn record(
        &self,
        rule: MRuleId,
        prune_disagreeing_copies: bool,
        input: &Shape,
        outputs: &[Matched],
        miss_work: usize,
    ) -> Option<Vec<Shape>> {
        if miss_work == 0 {
            return None;
        }
        let work = &self.work;
        let mut inner = self.inner.borrow_mut();
        let node = inner.node(work, input)?;
        let key = RowKey {
            node,
            rule: rule.0,
            prune_disagreeing_copies,
        };
        if inner.rows.contains_key(&key) {
            return None;
        }
        let mut edges = Vec::with_capacity(outputs.len());
        let mut shapes = Vec::with_capacity(outputs.len());
        for (allomorph, shape) in outputs {
            let node = inner.node(work, shape)?;
            edges.push(Edge {
                allomorph: allomorph.map_or(NO_ALLOMORPH, |a| a.0),
                node,
            });
            shapes.push(inner.shapes[node as usize].clone());
        }
        let charge = (1 + edges.len()).min(miss_work) as u32;
        let bytes = if edges.is_empty() {
            0
        } else {
            edges_bytes(edges.len())
        };
        if !inner.reserve(work, 0, 1, bytes) {
            inner.kill(work);
            return None;
        }
        inner.edge_bytes += bytes;
        let edges = (!edges.is_empty()).then(|| Rc::from(edges));
        inner.rows.insert(key, Row { charge, edges });
        inner.settle(work);
        (!inner.dead).then_some(shapes)
    }

    /// The stored row of `rule` on `shape`: its charge and edges, counting a hit.
    #[doc(hidden)]
    pub fn read(
        &self,
        rule: MRuleId,
        prune_disagreeing_copies: bool,
        shape: &Shape,
    ) -> Option<(usize, Vec<Matched>)> {
        let mut inner = self.inner.borrow_mut();
        let node = *inner.nodes.get(shape)?;
        let row = inner.rows.get(&RowKey {
            node,
            rule: rule.0,
            prune_disagreeing_copies,
        })?;
        let (charge, edges) = (row.charge as usize, row.edges.clone());
        inner.hits += 1;
        let out = edges
            .iter()
            .flat_map(|edges| edges.iter())
            .map(|edge| (allomorph_of(edge), inner.shapes[edge.node as usize].clone()))
            .collect();
        Some((charge, out))
    }
}

fn allomorph_of(edge: &Edge) -> Option<AllomorphId> {
    (edge.allomorph != NO_ALLOMORPH).then_some(AllomorphId(edge.allomorph))
}

impl Drop for ShapeGraph {
    fn drop(&mut self) {
        let charged = self.inner.get_mut().charged;
        self.work.release_optional(charged);
    }
}

/// A stored row to replay into the caller's word.
pub(crate) struct Replay<'g> {
    graph: &'g ShapeGraph,
    edges: Option<Rc<[Edge]>>,
}

impl Replay<'_> {
    /// One output word per edge, in the order the match produced them: a clone of `word` with the stored shape, then `set` writes whatever the rule kind propagates.
    pub(crate) fn words(
        self,
        word: &Word,
        mut set: impl FnMut(&mut Word, Option<AllomorphId>),
    ) -> Vec<Word> {
        let Some(edges) = self.edges else {
            return Vec::new();
        };
        let inner = self.graph.inner.borrow();
        edges
            .iter()
            .map(|edge| {
                let mut w = word.clone();
                w.shape = inner.shapes[edge.node as usize].clone();
                set(&mut w, allomorph_of(edge));
                w
            })
            .collect()
    }
}

/// A rule application that ran the match and may store its row.
pub(crate) struct Run<'g> {
    probe: Option<Probe<'g>>,
    /// Observed application that found a row: its re-run spends nothing, the row's charge having been paid.
    _suspended: Option<SuspendedWork>,
}

struct Probe<'g> {
    graph: &'g ShapeGraph,
    rule: MRuleId,
    prune_disagreeing_copies: bool,
    input: Shape,
    work_before: usize,
}

impl Run<'_> {
    /// The match is done and `output` is its result: store it as the row, unless the search was stopped or the graph holds nothing.
    pub(crate) fn finish(self, output: &mut [Word]) {
        let Some(probe) = self.probe else { return };
        if pg_fst::work::stopped() {
            return;
        }
        let used = pg_fst::work::active_budget().map_or(0, |b| b.used());
        let edges: Vec<Matched> = output
            .iter()
            .map(|w| (w.analysis_allomorph, w.shape.clone()))
            .collect();
        let miss_work = used.saturating_sub(probe.work_before);
        if let Some(shapes) = probe.graph.record(
            probe.rule,
            probe.prune_disagreeing_copies,
            &probe.input,
            &edges,
            miss_work,
        ) {
            // The interned shapes are equal in value; sharing them lets later arrivals find their node by pointer.
            for (w, shape) in output.iter_mut().zip(shapes) {
                w.shape = shape;
            }
        }
    }
}

/// What consulting the graph before a rule's match found.
pub(crate) enum Consult<'g> {
    /// The row, already charged: build the outputs from it.
    Replay(Replay<'g>),
    /// A row exists but the work cap stopped its charge.
    Stopped,
    /// No usable row (or no graph): run the match, then `finish`.
    Run(Run<'g>),
}

fn run_without_graph<'g>() -> Consult<'g> {
    Consult::Run(Run {
        probe: None,
        _suspended: None,
    })
}

/// Look up the row of `rule` on `shape` after the caller's per-path gates have passed.
///
/// A found row is charged here. When `observed` (a trace recorder, a tree trace or a stats collector wants every event of the real search) the caller still re-runs the match, with its reservations suspended so the word is charged exactly the row.
pub(crate) fn consult<'g>(
    graph: Option<&'g ShapeGraph>,
    rule: MRuleId,
    prune_disagreeing_copies: bool,
    shape: &Shape,
    observed: bool,
) -> Consult<'g> {
    let Some(graph) = graph else {
        return run_without_graph();
    };
    #[cfg(test)]
    if TEST_DISABLED.with(std::cell::Cell::get) {
        return run_without_graph();
    }
    let mut inner = graph.inner.borrow_mut();
    let Some(node) = inner.node(&graph.work, shape) else {
        return run_without_graph();
    };
    let key = RowKey {
        node,
        rule: rule.0,
        prune_disagreeing_copies,
    };
    let Some(row) = inner.rows.get(&key) else {
        drop(inner);
        return Consult::Run(Run {
            probe: Some(Probe {
                graph,
                rule,
                prune_disagreeing_copies,
                input: shape.clone(),
                work_before: pg_fst::work::active_budget().map_or(0, |b| b.used()),
            }),
            _suspended: None,
        });
    };
    let (charge, edges) = (row.charge, row.edges.clone());
    inner.hits += 1;
    drop(inner);
    for _ in 0..charge {
        if !pg_fst::work::consume() {
            return Consult::Stopped;
        }
    }
    if observed {
        return Consult::Run(Run {
            probe: None,
            _suspended: pg_fst::work::active_budget().map(|budget| budget.suspend()),
        });
    }
    Consult::Replay(Replay { graph, edges })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_shape::ShapeBuilder;

    fn shape(cd: u32) -> Shape {
        let mut builder = ShapeBuilder::new();
        builder.push_segment(cd);
        builder.finish()
    }

    const A: AllomorphId = AllomorphId(1);
    const R: MRuleId = MRuleId(3);

    fn graph() -> ShapeGraph {
        ShapeGraph::new(WorkBudget::new(usize::MAX))
    }

    #[test]
    fn a_row_is_read_back_in_order_by_rule_flag_and_shape() {
        let g = graph();
        let outs = [(Some(A), shape(7)), (None, shape(8))];
        g.record(R, true, &shape(1), &outs, 50).unwrap();
        assert!(g.read(MRuleId(4), true, &shape(1)).is_none());
        assert!(g.read(R, false, &shape(1)).is_none());
        assert!(g.read(R, true, &shape(2)).is_none());
        let (charge, edges) = g.read(R, true, &shape(1)).unwrap();
        assert_eq!(charge, 3);
        assert_eq!(edges, outs.to_vec());
        assert_eq!(g.hits(), 1);
    }

    #[test]
    fn an_empty_row_records_no_match_and_costs_one_unit_to_read() {
        let g = graph();
        g.record(R, true, &shape(1), &[], 40).unwrap();
        assert_eq!(g.read(R, true, &shape(1)), Some((1, Vec::new())));
        assert_eq!(g.rows(), 1);
    }

    #[test]
    fn a_read_never_charges_more_than_the_search_that_made_the_row() {
        let g = graph();
        g.record(
            R,
            true,
            &shape(1),
            &[(Some(A), shape(7)), (Some(A), shape(8))],
            2,
        )
        .unwrap();
        assert_eq!(g.read(R, true, &shape(1)).unwrap().0, 2);
        assert!(g.record(R, true, &shape(2), &[], 0).is_none());
        assert!(g.read(R, true, &shape(2)).is_none());
    }

    #[test]
    fn equal_shapes_share_one_node_whatever_their_allocation() {
        let g = graph();
        let out = shape(7);
        g.record(
            R,
            true,
            &shape(1),
            std::slice::from_ref(&(Some(A), out.clone())),
            9,
        )
        .unwrap();
        let again = g
            .record(MRuleId(9), true, &shape(1), &[(Some(A), shape(7))], 9)
            .unwrap();
        assert!(again[0].same_allocation(&out));
        assert_eq!(g.nodes(), 2);
        assert_eq!(g.rows(), 2);
    }

    const SMALL: usize = 60_000;

    #[test]
    fn the_graph_takes_at_most_an_eighth_of_the_word_allowance_then_drops_itself() {
        let allowance = SMALL * ALLOWANCE_SHARE;
        let work = WorkBudget::new(usize::MAX).with_word_cap(Some(allowance));
        let g = ShapeGraph::new(work.clone());
        let mut stored = 0;
        for i in 1..2000 {
            if g.record(R, true, &shape(i), &[(Some(A), shape(i + 5000))], 9)
                .is_some()
            {
                stored += 1;
                assert!(g.bytes() <= SMALL, "{} over {SMALL}", g.bytes());
            } else {
                break;
            }
        }
        assert!(stored > 4 && stored < 1999, "{stored} rows");
        assert!(g.is_dead());
        assert_eq!(g.bytes(), 0, "a dropped graph holds nothing");
        assert!(g.read(R, true, &shape(1)).is_none());
        drop(g);
        assert!(
            work.try_charge_optional(allowance),
            "dropping releases the bytes"
        );
    }

    #[test]
    fn the_graph_yields_so_a_word_just_under_the_allowance_is_not_capped() {
        let allowance = SMALL * ALLOWANCE_SHARE;
        let work = WorkBudget::new(usize::MAX).with_word_cap(Some(allowance));
        let g = ShapeGraph::new(work.clone());
        for i in 1..20 {
            g.record(R, true, &shape(i), &[(Some(A), shape(7))], 9)
                .unwrap();
        }
        assert!(g.rows() > 10);
        assert!(
            work.charge_word(allowance),
            "main completes this word under the allowance"
        );
        assert!(!work.word_capped());
        assert!(g
            .record(R, true, &shape(99), &[(Some(A), shape(7))], 9)
            .is_none());
        assert!(g.is_dead());
        assert!(!work.word_capped());
    }

    #[test]
    fn a_refusal_from_the_word_allowance_stops_the_graph_not_the_search() {
        let work = WorkBudget::new(usize::MAX).with_word_cap(Some(1));
        let g = ShapeGraph::new(work.clone());
        assert!(g
            .record(R, true, &shape(1), &[(Some(A), shape(7))], 9)
            .is_none());
        assert!(g.read(R, true, &shape(1)).is_none());
        assert!(!work.word_capped());
    }
}
