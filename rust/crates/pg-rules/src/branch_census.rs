//! `HC_BRANCH_CENSUS=1` diagnostic (env-gated, off by default, never prunes): the hindsight
//! ceiling on pruning analysis branches that can never reach a lexical root.
//!
//! Every analysis word is a node named by a hash of its `WordKey`. Unapplication records a
//! parent-to-child edge and charges the attempt's steps, work and time to its input node; a
//! word folded into a canonical as an alternative is an alias that shares the canonical's fate.
//! After the parse, a node is lexically live when a lookup on it, an alias, or a descendant
//! found a root, and parse-live when such a lookup also produced a surface match. Cost charged
//! to dead nodes is the most any prune that only removes root-unreachable branches could save.
//!
//! Dead cost is split three ways: leaves (no unapplication succeeded on them), internal nodes that
//! could still split off a compound non-head, and the remaining internal nodes, the only bucket a
//! summary blind to both the rule matchers and non-head lookup could hope to cut.
//!
//! The same pass counts lookup calls against distinct `(stratum, shape)` pairs, top-level and
//! compound non-head separately.
//!
//! Zero cost when unset: [`enabled`] caches one env read per thread.

use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};

use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use smallvec::SmallVec;

use crate::word::Word;

#[derive(Default, Clone, Copy)]
struct Cost {
    steps: u64,
    work: u64,
    ns: u64,
    lookups: u64,
    lookup_work: u64,
    lookup_ns: u64,
    syn_work: u64,
    syn_ns: u64,
}

#[derive(Default)]
struct Census {
    parents: FxHashMap<u64, SmallVec<[u64; 2]>>,
    aliases: FxHashMap<u64, SmallVec<[u64; 2]>>,
    cost: FxHashMap<u64, Cost>,
    lex_hits: FxHashSet<u64>,
    parse_hits: FxHashSet<u64>,
    top_lookups: u64,
    top_shapes: FxHashSet<u64>,
    non_head_lookups: u64,
    non_head_shapes: FxHashSet<u64>,
    phase_analysis_work: u64,
    phase_analysis_ns: u64,
    dup_lookup_work: u64,
    may_compound: FxHashSet<u64>,
}

thread_local! {
    static ENABLED: Cell<Option<bool>> = const { Cell::new(None) };
    static STATE: RefCell<Option<Census>> = const { RefCell::new(None) };
}

/// Cached `HC_BRANCH_CENSUS` read (one env lookup per thread).
pub fn enabled() -> bool {
    ENABLED.with(|c| {
        if let Some(v) = c.get() {
            return v;
        }
        let v = std::env::var("HC_BRANCH_CENSUS").is_ok();
        c.set(Some(v));
        v
    })
}

/// A wall-clock stopwatch for callers without their own `web_time` dependency.
#[derive(Clone, Copy)]
pub struct Timer(web_time::Instant);

impl Timer {
    pub fn start() -> Self {
        Timer(web_time::Instant::now())
    }

    pub fn ns(&self) -> u64 {
        self.0.elapsed().as_nanos() as u64
    }
}

fn with<R>(f: impl FnOnce(&mut Census) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().get_or_insert_with(Census::default)))
}

/// The node id of `w`: a hash of its `WordKey`, which already includes its stratum.
pub fn node(w: &Word) -> u64 {
    let mut h = FxHasher::default();
    w.dedup_key().hash(&mut h);
    h.finish()
}

fn shape_id(stratum: pg_grammar_model::model::StratumId, shape: &pg_shape::Shape) -> u64 {
    let mut h = FxHasher::default();
    stratum.hash(&mut h);
    shape.hash(&mut h);
    h.finish()
}

/// `child` was derived from `parent` (an unapplication output, or a stratum-boundary rewrite).
pub fn edge(parent: u64, child: u64) {
    if parent == child {
        return;
    }
    with(|c| {
        let ps = c.parents.entry(child).or_default();
        if !ps.contains(&parent) {
            ps.push(parent);
        }
    });
}

/// `alt` was folded into `canonical`; it is live exactly when the canonical is.
pub fn alias(canonical: u64, alt: u64) {
    if canonical == alt {
        return;
    }
    with(|c| c.aliases.entry(canonical).or_default().push(alt));
}

/// `node` could still split off a compound non-head, so a summary blind to non-head lookup cannot call it dead.
pub fn mark_may_compound(node: u64) {
    with(|c| {
        c.may_compound.insert(node);
    });
}

/// One unapplication attempt with `node` as input.
pub fn charge_analysis(node: u64, steps: u64, work: u64, ns: u64) {
    with(|c| {
        let e = c.cost.entry(node).or_default();
        e.steps += steps;
        e.work += work;
        e.ns += ns;
    });
}

/// One top-level lexical lookup on `node`; `hit` when it supplied at least one root.
pub fn charge_lookup(
    node: u64,
    stratum: pg_grammar_model::model::StratumId,
    shape: &pg_shape::Shape,
    hit: bool,
    work: u64,
    ns: u64,
) {
    with(|c| {
        let e = c.cost.entry(node).or_default();
        e.lookups += 1;
        e.lookup_work += work;
        e.lookup_ns += ns;
        c.top_lookups += 1;
        if !c.top_shapes.insert(shape_id(stratum, shape)) {
            c.dup_lookup_work += work;
        }
        if hit {
            c.lex_hits.insert(node);
        }
    });
}

/// One compound non-head root search, already inside a charged unapplication attempt.
pub fn record_non_head_lookup(
    stratum: pg_grammar_model::model::StratumId,
    shape: &pg_shape::Shape,
) {
    if !enabled() {
        return;
    }
    with(|c| {
        c.non_head_lookups += 1;
        c.non_head_shapes.insert(shape_id(stratum, shape));
    });
}

/// Synthesis of `node`'s lookup results; `matched` when one of them reproduced the surface.
pub fn charge_synthesis(node: u64, work: u64, ns: u64, matched: bool) {
    with(|c| {
        let e = c.cost.entry(node).or_default();
        e.syn_work += work;
        e.syn_ns += ns;
        if matched {
            c.parse_hits.insert(node);
        }
    });
}

/// The whole analysis phase's work and time, attributed or not.
pub fn record_analysis_phase(work: u64, ns: u64) {
    with(|c| {
        c.phase_analysis_work += work;
        c.phase_analysis_ns += ns;
    });
}

/// Per-word totals: `dead_*` is cost charged to lexically dead nodes, `pdead_*` to nodes that
/// are not parse-live.
#[derive(Debug, Clone, Copy, Default)]
pub struct BranchCensusSnapshot {
    pub nodes: u64,
    pub dead_nodes: u64,
    pub pdead_nodes: u64,
    pub phase_analysis_work: u64,
    pub phase_analysis_ns: u64,
    pub steps: u64,
    pub work: u64,
    pub ns: u64,
    pub dead_steps: u64,
    pub dead_work: u64,
    pub dead_ns: u64,
    pub pdead_steps: u64,
    pub pdead_work: u64,
    pub pdead_ns: u64,
    pub lookups: u64,
    pub lookup_hits: u64,
    pub lookup_work: u64,
    pub lookup_ns: u64,
    pub dead_lookups: u64,
    pub dead_lookup_work: u64,
    pub dead_lookup_ns: u64,
    pub distinct_lookup_shapes: u64,
    pub non_head_lookups: u64,
    pub distinct_non_head_shapes: u64,
    pub syn_work: u64,
    pub syn_ns: u64,
    pub pdead_syn_work: u64,
    pub pdead_syn_ns: u64,
    pub dead_leaf_work: u64,
    pub dead_leaf_lookup_work: u64,
    pub dup_lookup_work: u64,
    pub dead_compound_work: u64,
    pub dead_internal_plain_work: u64,
    pub dead_internal_plain_lookup_work: u64,
}

impl BranchCensusSnapshot {
    /// `name=value` fields, tab-separated, in declaration order.
    pub fn to_tsv(&self) -> String {
        let fields: [(&str, u64); 34] = [
            ("nodes", self.nodes),
            ("dead_nodes", self.dead_nodes),
            ("pdead_nodes", self.pdead_nodes),
            ("phase_ana_work", self.phase_analysis_work),
            ("phase_ana_ns", self.phase_analysis_ns),
            ("steps", self.steps),
            ("work", self.work),
            ("ns", self.ns),
            ("dead_steps", self.dead_steps),
            ("dead_work", self.dead_work),
            ("dead_ns", self.dead_ns),
            ("pdead_steps", self.pdead_steps),
            ("pdead_work", self.pdead_work),
            ("pdead_ns", self.pdead_ns),
            ("lookups", self.lookups),
            ("lookup_hits", self.lookup_hits),
            ("lookup_work", self.lookup_work),
            ("lookup_ns", self.lookup_ns),
            ("dead_lookups", self.dead_lookups),
            ("dead_lookup_work", self.dead_lookup_work),
            ("dead_lookup_ns", self.dead_lookup_ns),
            ("distinct_lookup_shapes", self.distinct_lookup_shapes),
            ("non_head_lookups", self.non_head_lookups),
            ("distinct_non_head_shapes", self.distinct_non_head_shapes),
            ("syn_work", self.syn_work),
            ("syn_ns", self.syn_ns),
            ("pdead_syn_work", self.pdead_syn_work),
            ("pdead_syn_ns", self.pdead_syn_ns),
            ("dead_leaf_work", self.dead_leaf_work),
            ("dead_leaf_lookup_work", self.dead_leaf_lookup_work),
            ("dup_lookup_work", self.dup_lookup_work),
            ("dead_compound_work", self.dead_compound_work),
            ("dead_internal_plain_work", self.dead_internal_plain_work),
            (
                "dead_internal_plain_lookup_work",
                self.dead_internal_plain_lookup_work,
            ),
        ];
        fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\t")
    }
}

fn closure(c: &Census, seeds: &FxHashSet<u64>) -> FxHashSet<u64> {
    let mut live: FxHashSet<u64> = FxHashSet::default();
    let mut stack: Vec<u64> = seeds.iter().copied().collect();
    while let Some(n) = stack.pop() {
        if !live.insert(n) {
            continue;
        }
        if let Some(ps) = c.parents.get(&n) {
            stack.extend(ps.iter().copied());
        }
        if let Some(alts) = c.aliases.get(&n) {
            stack.extend(alts.iter().copied());
        }
    }
    live
}

/// Compute this thread's per-word totals since the last [`reset`].
pub fn snapshot() -> BranchCensusSnapshot {
    with(|c| {
        let lex_live = closure(c, &c.lex_hits);
        let parse_live = closure(c, &c.parse_hits);
        let mut all: FxHashSet<u64> = c.cost.keys().copied().collect();
        all.extend(c.parents.keys().copied());
        for ps in c.parents.values() {
            all.extend(ps.iter().copied());
        }
        for alts in c.aliases.values() {
            all.extend(alts.iter().copied());
        }
        let mut s = BranchCensusSnapshot {
            nodes: all.len() as u64,
            dead_nodes: all.iter().filter(|n| !lex_live.contains(n)).count() as u64,
            pdead_nodes: all.iter().filter(|n| !parse_live.contains(n)).count() as u64,
            phase_analysis_work: c.phase_analysis_work,
            phase_analysis_ns: c.phase_analysis_ns,
            lookup_hits: c.lex_hits.len() as u64,
            distinct_lookup_shapes: c.top_shapes.len() as u64,
            non_head_lookups: c.non_head_lookups,
            distinct_non_head_shapes: c.non_head_shapes.len() as u64,
            dup_lookup_work: c.dup_lookup_work,
            ..Default::default()
        };
        let has_child: FxHashSet<u64> = c.parents.values().flatten().copied().collect();
        for (n, e) in &c.cost {
            s.steps += e.steps;
            s.work += e.work;
            s.ns += e.ns;
            s.lookups += e.lookups;
            s.lookup_work += e.lookup_work;
            s.lookup_ns += e.lookup_ns;
            s.syn_work += e.syn_work;
            s.syn_ns += e.syn_ns;
            if !lex_live.contains(n) {
                s.dead_steps += e.steps;
                s.dead_work += e.work;
                s.dead_ns += e.ns;
                s.dead_lookups += e.lookups;
                s.dead_lookup_work += e.lookup_work;
                s.dead_lookup_ns += e.lookup_ns;
                if !has_child.contains(n) {
                    s.dead_leaf_work += e.work;
                    s.dead_leaf_lookup_work += e.lookup_work;
                } else if c.may_compound.contains(n) {
                    s.dead_compound_work += e.work + e.lookup_work;
                } else {
                    s.dead_internal_plain_work += e.work;
                    s.dead_internal_plain_lookup_work += e.lookup_work;
                }
            }
            if !parse_live.contains(n) {
                s.pdead_steps += e.steps;
                s.pdead_work += e.work;
                s.pdead_ns += e.ns;
                s.pdead_syn_work += e.syn_work;
                s.pdead_syn_ns += e.syn_ns;
            }
        }
        s
    })
}

/// Clear this thread's per-word state while preserving the cached env gate.
pub fn reset() {
    STATE.with(|s| *s.borrow_mut() = None);
}

#[cfg(test)]
mod tests;
