//! Static structural inspection of a finished `foma::types::Fsm` — the SPEED half of the
//! candidate-screening split (the accuracy half is `crate::backend_accuracy`'s set containment).
//!
//! Nothing here applies a word. Every number is read off the compiled net's own line table, so the
//! whole inspection is `O(states + arcs)` and available before a single corpus word is proposed.
//!
//! # Why SIZE is not the proxy, and is in fact anti-correlated
//! The obvious cheap proxy — "prefer the smaller / less branchy candidate set" — is measured WRONG
//! on this codebase, not merely weak. On the private `sena` grammar:
//!
//! | | states | arcs | relative `apply_up` cost |
//! |---|---|---|---|
//! | plan-composed (`uflexc`) | 2,044 | 21,114 | ~1300x slower |
//! | hand-spun (`crate::emit`) | 106,365 | 702,364 | baseline |
//!
//! The plan-composed net is **50x smaller and ~1300x slower to apply**. Any metric monotone in
//! states, arcs, or total proposal count picks the wrong candidate here (proposal counts were
//! 575 vs 127 — "nearly tied" — across that same 1300x gap). So this module deliberately reports
//! size and reports it as *context*, never as a ranking term.
//!
//! # What IS predictive: the shape of the continuation graph
//! `crate::emit` builds BOUNDED, non-looping derivational chains where each slot appears exactly
//! once (that module's `build_deriv_chain`), so branching at any input position is small and
//! locally bounded. `crate::uflexc` builds SELF-LOOPING prefix/suffix chains, and `apply_up` must
//! keep "did the word take another turn through this loop" live at every loop state, with the
//! ambiguity compounding across the word. **The `apply_up` code is identical on both sides — the
//! gap is entirely automaton shape.**
//!
//! The sharpest case of that, and the only one this module is willing to call a *defect* rather
//! than a number, is a **zero-width cycle**: a cycle every arc of which consumes NOTHING on the
//! tape `apply` is reading. Going around it costs no input, and (whenever the other tape is not
//! also epsilon) each lap yields a genuinely different output string, so the traversal multiplies
//! out every lap count up to its own internal search bound. That is the exact mechanism behind
//! `crate::build::reroute_null_shaped_affix_chains`'s recorded 127 -> 53,992 proposals (425x), and
//! behind the same class reopening once per compound level when a name-scoped guard could not see a
//! lexicon added after it. A structural cycle check does not care what the lexicon is called.
//!
//! # Direction matters, and getting it backwards would make this vacuous
//! `apply_up` matches the **lower** tape (`foma::types::FsmState::out`) and *emits* the upper one;
//! `apply_down` is the mirror image (`foma`'s own `apply_binarysearch` selects `l_out` for `UP` and
//! `l_in` for `DOWN`). So "consumes nothing" means `out == EPSILON` when screening an `apply_up`
//! net, and `in == EPSILON` when screening an `apply_down` one. The null-morph pathology is
//! precisely an `in != EPSILON, out == EPSILON` self-loop: invisible in the down direction,
//! unbounded in the up direction. `ApplyDirection` is therefore a required argument, never
//! defaulted, and `shape_unit_tests::direction_decides_whether_a_zero_width_cycle_exists` pins
//! the asymmetry on a two-arc net.
//!
//! # What foma already provides, and why that was not enough
//! `foma::topsort::fsm_topsort` is a prefabricated linear Kahn's-algorithm pass that already answers
//! "is this net cyclic" (setting `is_loop_free` / a `PATHCOUNT_CYCLIC` pathcount), and
//! `examples/p6_templated_q1_cycle_check.rs` already used it for exactly that. It is deliberately
//! NOT re-implemented here for the coarse question: [`shape_unit_tests::
//! full_graph_cycle_detection_agrees_with_fomas_own_topsort`] cross-checks this module's own
//! full-graph answer against foma's, on six nets, so a bug in the walk below shows up as a
//! disagreement with a well-tested prefab rather than as a plausible-looking number.
//!
//! What `fsm_topsort` cannot do is the thing that matters: it returns one boolean for the WHOLE net,
//! over the WHOLE arc set. It cannot say a cycle is zero-width in a given apply direction, cannot
//! separate the deliberate input-consuming loops from the pathological free ones, and reports no
//! per-state distribution at all. On `uflexc`'s output its answer is "cyclic" both before and after
//! the fix that removed the explosion, so it cannot discriminate the case this module exists for.
//!
//! # HARD SCOPE: this is a first-pass filter and a regression tripwire, NEVER a certification signal
//! - This module computes a diagnostic value and nothing more: no `Score` field, ranking key,
//!   eligibility predicate, or certification verdict may consult it (grep to confirm). Deliberate:
//!   the owner accepted that a shape proxy
//!   *may mislead*, and that acceptance does not extend to letting it decide correctness.
//! - **A pathological verdict is INFORMATION, not permission to stop proposing.** Nothing here can
//!   skip, truncate, or prune a candidate or a proposal set — there is no code path from this module
//!   into one. Truncating a word's proposal set would be read by `crate::parity` as disagreement,
//!   which is worse than the cost the truncation saved.
//! - Only ONE property is asserted as a defect: presence of a zero-width cycle. That is a
//!   *structural* fact with no threshold to tune. Everything else (`NetShape::branching_max`, the
//!   quantiles, `NetShape::apply_ambiguity_total`) is reported as an **uncalibrated number**:
//!   this project has no complete grammar to calibrate against, and a fabricated threshold would
//!   read as a measurement.

use foma::types::Fsm;

/// `foma`'s epsilon symbol number, narrowed from `foma::types::EPSILON` rather than re-spelled as a literal `0` so the two can never drift.
const EPSILON_LABEL: i16 = foma::types::EPSILON as i16;

/// Which tape an `apply` traversal CONSUMES. Determines which label counts as "consumes nothing".
///
/// See the module doc: `foma`'s traversal reads `foma::types::FsmState::out` for `Up` and
/// `foma::types::FsmState::in` for `Down`. Screening the wrong direction does not fail loudly —
/// it silently reports a clean net.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ApplyDirection {
    /// `apply_up`: consumes the lower tape (surface), emits the upper tape (analysis tags) -- the direction every proposer in this crate queries.
    Up,
    /// `apply_down`: consumes the upper tape, emits the lower tape.
    Down,
}

impl ApplyDirection {
    /// The label this direction consumes, for one arc.
    #[inline]
    fn consumed(self, arc: &Arc) -> i16 {
        match self {
            ApplyDirection::Up => arc.out,
            ApplyDirection::Down => arc.r#in,
        }
    }
}

/// One arc, flattened out of the net's line table so the walks below never re-decode it.
#[derive(Copy, Clone, Debug)]
struct Arc {
    target: u32,
    r#in: i16,
    out: i16,
}

/// A cycle census over one graph: how many cyclic components, and how many states sit in one.
///
/// "Cyclic component" means a strongly-connected component of size > 1, OR a single state carrying
/// a self-loop. A size-1 component with no self-loop is not a cycle and is not counted — that
/// distinction is the whole difference between "this net has states" and "this net has a loop".
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct CycleCensus {
    /// Number of cyclic components.
    pub cycles: u64,
    /// Number of states belonging to some cyclic component.
    pub states: u64,
    /// Number of arcs from a state back to itself.
    pub self_loop_arcs: u64,
    /// Number of distinct states carrying at least one self-loop arc.
    pub self_loop_states: u64,
    /// Size of the largest cyclic component, in states. `0` when there is no cycle at all.
    pub largest_cycle_states: u64,
}

impl CycleCensus {
    /// Whether this graph contains any cycle at all.
    #[inline]
    pub fn any(&self) -> bool {
        self.cycles > 0
    }
}

/// A deterministic out-degree distribution: the "how wide does the traversal fan out" half.
///
/// Quantiles use **nearest-rank on the sorted vector**, index `min(len - 1, floor(p * len))` — an
/// exactly reproducible definition with no interpolation, so two runs on the same net cannot
/// disagree. `mean_milli` is the mean scaled by 1000 and truncated, for the same reason: a float
/// mean is not a stable thing to print in a gate's evidence line.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct DegreeDistribution {
    pub max: u64,
    pub p50: u64,
    pub p90: u64,
    pub p99: u64,
    /// Arithmetic mean x 1000, truncated. Integer so it is byte-stable in output.
    pub mean_milli: u64,
    /// How many states this distribution was computed over.
    pub sampled_states: u64,
}

/// The whole static inspection of one finished net, in one direction.
///
/// Every field is a deterministic count. **No wall clock appears here, by construction** — this
/// machine runs several worktrees' builds concurrently, so elapsed time cannot separate a real
/// effect from a neighbour's load, and a time-derived field would be an invitation to rank on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetShape {
    /// Which tape the screened traversal consumes.
    pub direction: ApplyDirection,
    /// Reported for CONTEXT only. See the module doc's measured table for why neither of these two
    /// numbers may be used to prefer one candidate over another.
    pub states: u64,
    /// Reported for CONTEXT only, as `Self::states`.
    pub arcs: u64,
    /// Cycles over the FULL continuation graph, labels ignored. A high count here is normal and not
    /// by itself a problem: `uflexc`'s self-looping affix chains are deliberate, and a loop whose
    /// every lap consumes a real surface character is bounded by the query's own length.
    pub cycles: CycleCensus,
    /// Cycles over the subgraph of arcs that consume NOTHING in `Self::direction`. **This is the
    /// defect.** Any cycle here can be traversed an unbounded number of times at a single input
    /// position.
    pub zero_width_cycles: CycleCensus,
    /// Arcs consuming nothing in `Self::direction`. Individually harmless (an ordinary epsilon
    /// transition on an acyclic path is just a jump); counted because a zero-width cycle cannot
    /// exist without them, so a `0` here is a positive proof of absence.
    pub zero_width_arcs: u64,
    /// Out-degree over all states, labels ignored.
    pub branching: DegreeDistribution,
    /// Per-state count of DISTINCT consumed labels — the fan-out a traversal actually has to choose
    /// between at one input position, with duplicate labels collapsed.
    pub distinct_label_branching: DegreeDistribution,
    /// Largest number of arcs leaving ONE state that consume the SAME label (an epsilon "label"
    /// included). `1` means the net is deterministic in `Self::direction`; `n > 1` means a
    /// traversal at that state forks `n` ways on one input symbol.
    pub apply_ambiguity_max: u64,
    /// Summed excess: over every (state, consumed label) pair, `count - 1`. The total number of
    /// extra forks the traversal can be forced into across the whole net.
    ///
    /// **UNCALIBRATED.** Reported, never thresholded. See the module doc's scope section.
    pub apply_ambiguity_total: u64,
    /// States from which no arc leaves.
    pub sink_states: u64,
}

/// The screen's verdict. One structural property, deliberately not a score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeVerdict {
    /// No cycle in the net can be traversed without consuming input; lap count is bounded by input length.
    ZeroWidthBounded,
    /// At least one cycle consumes nothing in the screened direction, so `apply` can revisit a state at a fixed input position; not a ranking term and never suppresses a proposal.
    ZeroWidthCycle {
        cycles: u64,
        states: u64,
        largest_cycle_states: u64,
    },
}

impl ShapeVerdict {
    /// Whether this verdict names the zero-width-cycle defect.
    #[inline]
    pub fn is_pathological(&self) -> bool {
        matches!(self, ShapeVerdict::ZeroWidthCycle { .. })
    }
}

impl NetShape {
    /// Inspect `net` statically. Applies no word, allocates `O(states + arcs)`, and mutates nothing.
    ///
    /// The graph walks are **iteratively** implemented, not recursively, and that is deliberate
    /// rather than stylistic: two conformance fixtures currently kill the test process with
    /// `STATUS_STACK_BUFFER_OVERRUN` on deep/optional/nested structure, and a screen that overflows
    /// the stack on the very nets it exists to flag would be worse than no screen.
    pub fn inspect(net: &Fsm, direction: ApplyDirection) -> NetShape {
        let adjacency = Adjacency::from_net(net);
        let n = adjacency.state_count();

        // The full continuation graph, labels ignored.
        let full = census(n, &adjacency, |_| true);

        // Zero-width subgraph: only arcs that consume nothing in `direction`.
        let zero_width = census(n, &adjacency, |arc| {
            direction.consumed(arc) == EPSILON_LABEL
        });
        let zero_width_arcs = adjacency
            .all_arcs()
            .filter(|arc| direction.consumed(arc) == EPSILON_LABEL)
            .count() as u64;

        // Branching.
        let mut degrees: Vec<u64> = Vec::with_capacity(n);
        let mut distinct: Vec<u64> = Vec::with_capacity(n);
        let mut ambiguity_max = 0u64;
        let mut ambiguity_total = 0u64;
        let mut sinks = 0u64;
        let mut labels: Vec<i16> = Vec::new();
        for s in 0..n {
            let arcs = adjacency.arcs(s);
            degrees.push(arcs.len() as u64);
            if arcs.is_empty() {
                sinks += 1;
                distinct.push(0);
                continue;
            }
            // Sorting a reused per-state scratch vector keeps this O(d log d) without a HashMap per state.
            labels.clear();
            labels.extend(arcs.iter().map(|a| direction.consumed(a)));
            labels.sort_unstable();
            let mut distinct_here = 0u64;
            let mut run = 1u64;
            for i in 1..labels.len() {
                if labels[i] == labels[i - 1] {
                    run += 1;
                } else {
                    distinct_here += 1;
                    ambiguity_max = ambiguity_max.max(run);
                    ambiguity_total += run - 1;
                    run = 1;
                }
            }
            distinct_here += 1;
            ambiguity_max = ambiguity_max.max(run);
            ambiguity_total += run - 1;
            distinct.push(distinct_here);
        }

        NetShape {
            direction,
            states: net.statecount.max(0) as u64,
            arcs: net.arccount.max(0) as u64,
            cycles: full,
            zero_width_cycles: zero_width,
            zero_width_arcs,
            branching: DegreeDistribution::from_samples(&mut degrees),
            distinct_label_branching: DegreeDistribution::from_samples(&mut distinct),
            apply_ambiguity_max: ambiguity_max,
            apply_ambiguity_total: ambiguity_total,
            sink_states: sinks,
        }
    }

    /// The one structural property this module is willing to call a defect.
    pub fn verdict(&self) -> ShapeVerdict {
        if self.zero_width_cycles.any() {
            ShapeVerdict::ZeroWidthCycle {
                cycles: self.zero_width_cycles.cycles,
                states: self.zero_width_cycles.states,
                largest_cycle_states: self.zero_width_cycles.largest_cycle_states,
            }
        } else {
            ShapeVerdict::ZeroWidthBounded
        }
    }

    /// One deterministic line naming every counter, for a gate's evidence output.
    ///
    /// Deliberately exhaustive: a gate that prints only the number it asserts on cannot be used to
    /// diagnose the run that failed it, and this crate's existing convention
    /// (`boundary_marker_epsilon_collapse_gate`'s own printed lines) is to emit the whole
    /// deterministic vector unconditionally, passing or failing.
    pub fn evidence_line(&self) -> String {
        format!(
            "dir={:?} states={} arcs={} \
             ZERO_WIDTH_CYCLES={} zero_width_cycle_states={} zero_width_largest_cycle={} \
             zero_width_arcs={} \
             cycles={} cyclic_states={} self_loop_states={} self_loop_arcs={} largest_cycle={} \
             branch_max={} branch_p99={} branch_p90={} branch_p50={} branch_mean_milli={} \
             distinct_label_branch_max={} distinct_label_branch_p90={} \
             apply_ambiguity_max={} apply_ambiguity_total={} sink_states={}",
            self.direction,
            self.states,
            self.arcs,
            self.zero_width_cycles.cycles,
            self.zero_width_cycles.states,
            self.zero_width_cycles.largest_cycle_states,
            self.zero_width_arcs,
            self.cycles.cycles,
            self.cycles.states,
            self.cycles.self_loop_states,
            self.cycles.self_loop_arcs,
            self.cycles.largest_cycle_states,
            self.branching.max,
            self.branching.p99,
            self.branching.p90,
            self.branching.p50,
            self.branching.mean_milli,
            self.distinct_label_branching.max,
            self.distinct_label_branching.p90,
            self.apply_ambiguity_max,
            self.apply_ambiguity_total,
            self.sink_states,
        )
    }
}

impl DegreeDistribution {
    /// Sorts `samples` in place; takes `&mut` since the caller owns a throwaway vector either way.
    fn from_samples(samples: &mut [u64]) -> DegreeDistribution {
        if samples.is_empty() {
            return DegreeDistribution::default();
        }
        samples.sort_unstable();
        let len = samples.len();
        let at = |p_milli: u64| -> u64 {
            // Nearest-rank, no interpolation (see the type's own doc).
            let idx = ((p_milli as u128 * len as u128) / 1000u128) as usize;
            samples[idx.min(len - 1)]
        };
        let sum: u128 = samples.iter().map(|&d| d as u128).sum();
        DegreeDistribution {
            max: samples[len - 1],
            p50: at(500),
            p90: at(900),
            p99: at(990),
            mean_milli: ((sum * 1000) / len as u128) as u64,
            sampled_states: len as u64,
        }
    }
}

/// Flattened adjacency over a net's line table, built once and shared by both cycle walks so a net is decoded from its CSR blocks exactly once.
struct Adjacency {
    /// `arc_offsets[s]..arc_offsets[s + 1]` indexes `Self::arcs` for state `s`.
    arc_offsets: Vec<u32>,
    arcs: Vec<Arc>,
}

impl Adjacency {
    fn from_net(net: &Fsm) -> Adjacency {
        // `state_no` is used as the index rather than block position: the line-table contract does not promise blocks arrive in state order.
        let mut max_state = net.statecount.max(0) as usize;
        for (block, _) in net.states.iter_blocks() {
            if block.state_no >= 0 {
                max_state = max_state.max(block.state_no as usize + 1);
            }
        }
        let mut per_state: Vec<Vec<Arc>> = vec![Vec::new(); max_state];
        for (block, arcs) in net.states.iter_blocks() {
            if block.state_no < 0 {
                continue;
            }
            let s = block.state_no as usize;
            for arc in arcs {
                if arc.target < 0 || arc.target as usize >= max_state {
                    // A negative target is foma's arc-less marker; neither case is worth panicking over in a read-only screen.
                    continue;
                }
                per_state[s].push(Arc {
                    target: arc.target as u32,
                    r#in: arc.r#in,
                    out: arc.out,
                });
            }
        }
        let mut arc_offsets = Vec::with_capacity(max_state + 1);
        let mut flat = Vec::new();
        for state_arcs in &per_state {
            arc_offsets.push(flat.len() as u32);
            flat.extend_from_slice(state_arcs);
        }
        arc_offsets.push(flat.len() as u32);
        Adjacency {
            arc_offsets,
            arcs: flat,
        }
    }

    #[inline]
    fn state_count(&self) -> usize {
        self.arc_offsets.len().saturating_sub(1)
    }

    #[inline]
    fn arcs(&self, state: usize) -> &[Arc] {
        let lo = self.arc_offsets[state] as usize;
        let hi = self.arc_offsets[state + 1] as usize;
        &self.arcs[lo..hi]
    }

    #[inline]
    fn all_arcs(&self) -> impl Iterator<Item = &Arc> {
        self.arcs.iter()
    }
}

/// Strongly-connected components of the subgraph `adjacency` restricted to arcs satisfying `keep`, reduced to a `CycleCensus`. Iterative Tarjan, not recursive, so it survives the nets most likely to overflow a stack elsewhere.
fn census(n: usize, adjacency: &Adjacency, keep: impl Fn(&Arc) -> bool) -> CycleCensus {
    const UNVISITED: u32 = u32::MAX;

    let mut idx = vec![UNVISITED; n];
    let mut low = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut component_of = vec![UNVISITED; n];
    let mut component_sizes: Vec<u64> = Vec::new();
    let mut tarjan_stack: Vec<u32> = Vec::new();
    // (state, cursor into that state's kept arcs)
    let mut frames: Vec<(u32, usize)> = Vec::new();
    let mut next_idx: u32 = 0;

    let mut self_loop_arcs = 0u64;
    let mut self_loop_states = 0u64;

    for root in 0..n {
        if idx[root] != UNVISITED {
            continue;
        }
        idx[root] = next_idx;
        low[root] = next_idx;
        next_idx += 1;
        tarjan_stack.push(root as u32);
        on_stack[root] = true;
        frames.push((root as u32, 0));

        while let Some(&(v, cursor)) = frames.last() {
            let vs = v as usize;
            let arcs = adjacency.arcs(vs);
            if cursor < arcs.len() {
                frames.last_mut().expect("frame just observed").1 = cursor + 1;
                let arc = &arcs[cursor];
                if !keep(arc) {
                    continue;
                }
                let w = arc.target as usize;
                if idx[w] == UNVISITED {
                    idx[w] = next_idx;
                    low[w] = next_idx;
                    next_idx += 1;
                    tarjan_stack.push(w as u32);
                    on_stack[w] = true;
                    frames.push((w as u32, 0));
                } else if on_stack[w] {
                    low[vs] = low[vs].min(idx[w]);
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                let p = parent as usize;
                low[p] = low[p].min(low[vs]);
            }
            if low[vs] == idx[vs] {
                let component = component_sizes.len() as u32;
                let mut size = 0u64;
                loop {
                    let w = tarjan_stack
                        .pop()
                        .expect("a root always has itself on the stack");
                    on_stack[w as usize] = false;
                    component_of[w as usize] = component;
                    size += 1;
                    if w == v {
                        break;
                    }
                }
                component_sizes.push(size);
            }
        }
    }

    // Self-loops, over the SAME kept arc set.
    let mut singleton_with_self_loop = vec![false; component_sizes.len()];
    // `s` is a state id read three ways here — arc source, arc target and component index — not one slice's cursor.
    #[allow(clippy::needless_range_loop)]
    for s in 0..n {
        let mut has = false;
        for arc in adjacency.arcs(s) {
            if keep(arc) && arc.target as usize == s {
                self_loop_arcs += 1;
                has = true;
            }
        }
        if has {
            self_loop_states += 1;
            // Bound check is defensive, not expected: a read-only screen must never panic on a malformed table.
            let c = component_of[s] as usize;
            if c < component_sizes.len() && component_sizes[c] == 1 {
                singleton_with_self_loop[c] = true;
            }
        }
    }

    let mut cycles = 0u64;
    let mut cyclic_states = 0u64;
    let mut largest = 0u64;
    for (c, &size) in component_sizes.iter().enumerate() {
        let cyclic = size > 1 || singleton_with_self_loop[c];
        if cyclic {
            cycles += 1;
            cyclic_states += size;
            largest = largest.max(size);
        }
    }

    CycleCensus {
        cycles,
        states: cyclic_states,
        self_loop_arcs,
        self_loop_states,
        largest_cycle_states: largest,
    }
}

#[cfg(test)]
mod shape_unit_tests;
