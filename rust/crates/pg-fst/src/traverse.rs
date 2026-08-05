//! Traversal of a frozen `Fst` over a linear segment sequence: the acceptor port of
//! `Fst.Transduce` (Fst.cs:304-414), the deterministic/nondeterministic FSA traversal methods,
//! and `ResultCompare` (Fst.cs:416-441).
//!
//! ## Input model
//! HermitCrab's `Fst<Word, int>` traverses shape annotations that are, for the FST arc path, a
//! **linear** sequence with integer offsets and per-node `Optional` (boundaries). We model exactly
//! that: segment `i` occupies physical range `[i, i+1)`; there are `n` segments over positions
//! `0..=n`. The overlap/hierarchy helpers (`GetNextNonoverlappingAnnotationIndex` etc.) collapse to
//! `±1`; the `Optional`-annotation branches of `Initialize`/`Advance` are kept (boundaries need
//! them).
//!
//! ## Direction (`Fst.cs`/`TraversalMethodBase.cs`)
//! C# bakes direction into the ordered annotation list and the `Range.GetStart/GetEnd(_dir)`
//! accessors, not into the walk arithmetic (which always increments `annIndex`). We do the same:
//! traversal index `j` maps to physical segment `phys(j)` — `j` for L2R, `n-1-j` for R2L
//! (`GetNodes(_dir)` + `CompareAnnotations`' sign flip, TraversalMethodBase.cs:41-73). The
//! offsets written into registers are **direction-relative** (`GetStart/GetEnd(_dir)`,
//! Range.cs:109-117): under R2L a segment's "start" is its physical *end*. `Fst::get_offsets`
//! un-swaps them back to physical `(min,max)` (Fst.cs:128-137). Because the same forward-built
//! automaton is walked from the opposite end, an R2L traversal accepts the *reversed* reading of
//! the input (pattern `a b c` matches physical `c b a`); the M3 loader owns whether a rule's
//! pattern nodes are pre-reversed so the composition matches HermitCrab end-to-end.
//! `ResultCompare` honors direction for its sign (Fst.cs:423-424); `next_ann` is the physical
//! `Range.Start` so that sign is applied to a direction-agnostic value, exactly as C#.
//!
//! Frozen FSTs have **no epsilon arcs** (removed by `Determinize`/`EpsilonRemoval`), so the
//! nondeterministic method's epsilon branch never fires; it is omitted and noted.

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
// std::time::Instant panics ("time not implemented on this platform") on wasm32-unknown-unknown;
// web-time is a drop-in replacement (Performance.now()-backed there, a re-export of std::time
// elsewhere) needed because the O2 profiling calls below are unconditional on every traversal,
// not just when HC_FST_PROFILE is actually read (pg-wasm builds this crate for the browser demo).
use web_time::Instant;

use crate::fst::Fst;
use crate::{Cmd, Direction, Register, CURRENT_POSITION};

// --- O2 profiling instrumentation (rust-optimizations-phase2.md O2) -------------------------
// A permanent diagnostic in the `HC_STEP_STATS` style (near-zero cost when unread: a few
// `Instant::now()` calls per `Transduce::run`/`distinct()` invocation, thread-local `Cell` adds).
// Kept (not reverted) because it is the load-bearing evidence for `docs/o2-profile-findings.md`
// and the Fable follow-up will want the same counters to confirm whatever fix it picks actually
// moves `distinct_ms`/`nondet_ms`. Read via `pg_fst::profile::snapshot()`, gated on
// `HC_FST_PROFILE=1` in `pg-cli` (see `main.rs`'s `FSTPROF` stderr line).
pub mod profile {
    use super::Cell;

    thread_local! {
        static RUN_CALLS: Cell<u64> = const { Cell::new(0) };
        static RUN_NANOS: Cell<u128> = const { Cell::new(0) };
        static RUN_MAX_NANOS: Cell<u128> = const { Cell::new(0) };
        static NONDET_CALLS: Cell<u64> = const { Cell::new(0) };
        static NONDET_NANOS: Cell<u128> = const { Cell::new(0) };
        static NONDET_MAX_TRAVERSED: Cell<usize> = const { Cell::new(0) };
        static NONDET_TOTAL_TRAVERSED: Cell<u64> = const { Cell::new(0) };
        static DET_CALLS: Cell<u64> = const { Cell::new(0) };
        static DET_NANOS: Cell<u128> = const { Cell::new(0) };
        static DISTINCT_CALLS: Cell<u64> = const { Cell::new(0) };
        static DISTINCT_NANOS: Cell<u128> = const { Cell::new(0) };
        static DISTINCT_MAX_INPUT_LEN: Cell<usize> = const { Cell::new(0) };
        static DISTINCT_TOTAL_INPUT_LEN: Cell<u64> = const { Cell::new(0) };
    }

    pub(super) fn record_run(elapsed_nanos: u128) {
        RUN_CALLS.with(|c| c.set(c.get() + 1));
        RUN_NANOS.with(|c| c.set(c.get() + elapsed_nanos));
        RUN_MAX_NANOS.with(|c| c.set(c.get().max(elapsed_nanos)));
    }

    pub(super) fn record_nondet(elapsed_nanos: u128, traversed_len: usize) {
        NONDET_CALLS.with(|c| c.set(c.get() + 1));
        NONDET_NANOS.with(|c| c.set(c.get() + elapsed_nanos));
        NONDET_MAX_TRAVERSED.with(|c| c.set(c.get().max(traversed_len)));
        NONDET_TOTAL_TRAVERSED.with(|c| c.set(c.get() + traversed_len as u64));
    }

    pub(super) fn record_det(elapsed_nanos: u128) {
        DET_CALLS.with(|c| c.set(c.get() + 1));
        DET_NANOS.with(|c| c.set(c.get() + elapsed_nanos));
    }

    pub(super) fn record_distinct(elapsed_nanos: u128, input_len: usize) {
        DISTINCT_CALLS.with(|c| c.set(c.get() + 1));
        DISTINCT_NANOS.with(|c| c.set(c.get() + elapsed_nanos));
        DISTINCT_MAX_INPUT_LEN.with(|c| c.set(c.get().max(input_len)));
        DISTINCT_TOTAL_INPUT_LEN.with(|c| c.set(c.get() + input_len as u64));
    }

    /// (run_calls, run_total_ns, run_max_ns, nondet_calls, nondet_total_ns, nondet_max_traversed,
    /// nondet_total_traversed, det_calls, det_total_ns, distinct_calls, distinct_total_ns,
    /// distinct_max_input_len, distinct_total_input_len) -- snapshot only, never reset (matches
    /// `StepBudget::steps()`'s read-only style); the CLI reads this once per word since each word
    /// gets a fresh process invocation in `batch` mode's per-word timing loop.
    #[allow(clippy::type_complexity)]
    pub fn snapshot() -> (
        u64,
        u128,
        u128,
        u64,
        u128,
        usize,
        u64,
        u64,
        u128,
        u64,
        u128,
        usize,
        u64,
    ) {
        (
            RUN_CALLS.with(|c| c.get()),
            RUN_NANOS.with(|c| c.get()),
            RUN_MAX_NANOS.with(|c| c.get()),
            NONDET_CALLS.with(|c| c.get()),
            NONDET_NANOS.with(|c| c.get()),
            NONDET_MAX_TRAVERSED.with(|c| c.get()),
            NONDET_TOTAL_TRAVERSED.with(|c| c.get()),
            DET_CALLS.with(|c| c.get()),
            DET_NANOS.with(|c| c.get()),
            DISTINCT_CALLS.with(|c| c.get()),
            DISTINCT_NANOS.with(|c| c.get()),
            DISTINCT_MAX_INPUT_LEN.with(|c| c.get()),
            DISTINCT_TOTAL_INPUT_LEN.with(|c| c.get()),
        )
    }
}

/// One input segment: its symbolic-feature lanes and whether it is optional (C#
/// `Annotation.Optional`, e.g. a boundary node).
#[derive(Clone, Debug)]
pub struct Segment {
    pub lanes: Vec<u64>,
    pub optional: bool,
}

impl Segment {
    pub fn new(lanes: Vec<u64>) -> Self {
        Segment {
            lanes,
            optional: false,
        }
    }
    pub fn optional(lanes: Vec<u64>) -> Self {
        Segment {
            lanes,
            optional: true,
        }
    }
}

/// A match result (C# `FstResult`, acceptor projection). `registers` is the flat
/// `register_count * 2` scaffold; use `Fst::get_offsets` to read a group span.
#[derive(Clone, Debug)]
pub struct FstResult {
    pub id: Option<String>,
    pub registers: Vec<Register>,
    /// Accept priority (`AcceptInfo.Priority`); `-1` when the state had no accept info.
    pub priority: i32,
    pub is_lazy: bool,
    /// Position of `NextAnnotation` (the not-yet-consumed segment index, or `n` at end).
    pub next_ann: i32,
    /// Insertion order within a start position (`FstResult.Order`).
    pub order: usize,
}

impl FstResult {
    /// C# `FstResult.Equals` (acceptor): same id and value-equal registers (output is unchanged).
    fn result_eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.registers.len() == other.registers.len()
            && self
                .registers
                .iter()
                .zip(&other.registers)
                .all(|(a, b)| a.value_eq(b))
    }
}

/// A live traversal instance (C# `TraversalInstance`).
///
/// `registers` is copy-on-write shared (`Rc`): the nondeterministic traversal clones an instance
/// once per matching arc plus once more for the visited-set key, but most arcs carry **no**
/// register commands (`cmd_lo == cmd_hi`), so eagerly deep-copying the `register_count * 2`
/// scaffold on every clone dominated the confirm path (`docs/o2-profile-findings.md`:
/// `nondet_max_traversed` reached 360K–542K on the pathological Amharic words). Cloning an `Inst`
/// is now an O(1) refcount bump; `Rc::make_mut` deep-copies lazily, only at the moment an arc's
/// non-empty command range actually writes (see `Transduce::advance`). Purely a representation
/// change: every observable value (register contents, results, ordering) is identical.
#[derive(Clone)]
struct Inst {
    state: usize,
    ann_index: usize,
    registers: Rc<Vec<Register>>,
}

/// Visited-set key wrapper for the shared register scaffold of the nondeterministic traversal.
///
/// Semantically identical to the plain `Vec<Register>` key it replaces (derived `Eq`/`Hash` over
/// the register contents — the same key the C# reference uses, keeping every distinct
/// `(state, ann_index, registers)` thread alive; do NOT collapse threads Pike-VM-style), but it
/// stores the instance's `Rc` (a refcount bump) instead of a second deep clone of the registers.
///
/// - `Eq`: `Rc::ptr_eq` fast path (same allocation ⇒ trivially content-equal), falling back to
///   full content equality. This is a pure optimization: it can never disagree with content
///   equality, because pointer-equal implies content-equal.
/// - `Hash`: hashes the **content** (exactly what `Vec<Register>`'s derived `Hash` did). Hashing
///   the pointer instead would break the `Eq`/`Hash` consistency contract (content-equal keys in
///   different allocations must collide); the win here is eliminating the extra deep clone, not
///   the hashing.
struct RegKey(Rc<Vec<Register>>);

impl PartialEq for RegKey {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}
impl Eq for RegKey {}

impl std::hash::Hash for RegKey {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        // Content-based, delegating to Vec<Register>'s derived Hash — bit-for-bit the hash the
        // old `(usize, usize, Vec<Register>)` key produced for the registers component.
        self.0.hash(h);
    }
}

/// The traversal driver.
pub struct Transduce<'f> {
    fst: &'f Fst,
    segs: Vec<Segment>,
    dir: Direction,
    start_anchor: bool,
    end_anchor: bool,
}

impl<'f> Transduce<'f> {
    pub fn new(fst: &'f Fst, segs: Vec<Segment>) -> Self {
        Transduce {
            fst,
            dir: fst.direction(),
            segs,
            start_anchor: false,
            end_anchor: false,
        }
    }

    /// Convenience constructor from raw lane vectors (no optional segments).
    pub fn from_lanes(fst: &'f Fst, segs: Vec<Vec<u64>>) -> Self {
        Transduce::new(fst, segs.into_iter().map(Segment::new).collect())
    }

    pub fn anchored(mut self, start: bool, end: bool) -> Self {
        self.start_anchor = start;
        self.end_anchor = end;
        self
    }

    fn n(&self) -> usize {
        self.segs.len()
    }

    // --- offset helpers (linear model, direction-aware) ---------------------------------------

    /// Physical segment index for traversal index `j` (`GetNodes(_dir)` ordering,
    /// TraversalMethodBase.cs:41-73). L2R is the identity; R2L reverses.
    #[inline]
    fn phys(&self, j: usize) -> usize {
        match self.dir {
            Direction::LeftToRight => j,
            Direction::RightToLeft => self.n() - 1 - j,
        }
    }

    /// The input segment at traversal index `j` (physically remapped under R2L). Callers guard
    /// `j < n`.
    #[inline]
    fn seg(&self, j: usize) -> &Segment {
        &self.segs[self.phys(j)]
    }

    /// Direction-relative start offset `Range.GetStart(_dir)` (Range.cs:109-112): physical left of
    /// `phys(j)` under L2R, physical right under R2L.
    #[inline]
    fn ann_start(&self, j: usize) -> i32 {
        match self.dir {
            Direction::LeftToRight => j as i32,
            Direction::RightToLeft => (self.n() - j) as i32,
        }
    }
    /// Direction-relative end offset `Range.GetEnd(_dir)` (Range.cs:114-117): physical right of
    /// `phys(j)` under L2R, physical left under R2L.
    #[inline]
    fn ann_end(&self, j: usize) -> i32 {
        match self.dir {
            Direction::LeftToRight => j as i32 + 1,
            Direction::RightToLeft => (self.n() - 1 - j) as i32,
        }
    }
    /// Direction-relative end-of-data offset: `GetLast(_dir).Range.GetEnd(_dir)`
    /// (TraversalMethodBase.cs:267). Physical right end `n` under L2R, physical left end `0` under
    /// R2L.
    #[inline]
    fn end_pos(&self) -> i32 {
        match self.dir {
            Direction::LeftToRight => self.n() as i32,
            Direction::RightToLeft => 0,
        }
    }
    /// The **physical** `Range.Start` of the next annotation (or end-of-data) — C#
    /// `FstResult.NextAnnotation`, which `ResultCompare` compares via the direction-agnostic
    /// `Range.CompareTo` before applying its own R2L sign flip (Fst.cs:422-424).
    #[inline]
    fn next_ann_pos(&self, j: usize) -> i32 {
        if j < self.n() {
            self.phys(j) as i32
        } else {
            match self.dir {
                Direction::LeftToRight => self.n() as i32,
                Direction::RightToLeft => 0,
            }
        }
    }

    // --- command execution (TraversalMethodBase.ExecuteCommands) -----------------------------

    fn execute_commands(registers: &mut [Register], cmds: &[Cmd], start: Register, end: Register) {
        for cmd in cmds {
            let d = cmd.dest as usize;
            if cmd.src == CURRENT_POSITION {
                registers[d * 2] = start;
                registers[d * 2 + 1] = end;
            } else {
                let s = cmd.src as usize;
                registers[d * 2] = registers[s * 2];
                registers[d * 2 + 1] = registers[s * 2 + 1];
            }
        }
    }

    fn state_arcs(&self, state: usize) -> &[crate::fst::Arc] {
        let m = &self.fst.states[state];
        &self.fst.arcs[m.arc_lo as usize..m.arc_hi as usize]
    }

    fn check_input_match(&self, arc: &crate::fst::Arc, ann_index: usize) -> bool {
        ann_index < self.n()
            && self
                .fst
                .constraint(arc.constraint)
                .matches(&self.seg(ann_index).lanes)
    }

    /// C# `CheckAccepting` (TraversalMethodBase.cs:130-191), acceptor path.
    fn check_accepting(
        &self,
        ann_index: usize,
        registers: &[Register],
        state: usize,
        cur_results: &mut Vec<FstResult>,
    ) {
        let meta = self.fst.states[state];
        if !meta.accepting {
            return;
        }
        if self.end_anchor && ann_index != self.n() {
            return;
        }
        let next_ann = self.next_ann_pos(ann_index);
        let mut match_registers = registers.to_vec();
        let fin = &self.fst.commands[meta.fin_lo as usize..meta.fin_hi as usize];
        Self::execute_commands(
            &mut match_registers,
            fin,
            Register::unset(),
            Register::unset(),
        );

        let infos = &self.fst.accept_infos[meta.accept_lo as usize..meta.accept_hi as usize];
        if infos.is_empty() {
            cur_results.push(FstResult {
                id: None,
                registers: match_registers,
                priority: -1,
                is_lazy: false,
                next_ann,
                order: cur_results.len(),
            });
        } else {
            for info in infos {
                cur_results.push(FstResult {
                    id: info.id.clone(),
                    registers: match_registers.clone(),
                    priority: info.priority,
                    is_lazy: false,
                    next_ann,
                    order: cur_results.len(),
                });
            }
        }
    }

    /// C# `Advance` (TraversalMethodBase.cs:248-351), linear acceptor path. Returns the produced
    /// continuation instances.
    fn advance(
        &self,
        mut inst: Inst,
        arc: &crate::fst::Arc,
        optional: bool,
        cur_results: &mut Vec<FstResult>,
    ) -> Vec<Inst> {
        let next_index = inst.ann_index + 1; // GetNextNonoverlappingAnnotationIndex, linear
        let end = self.ann_end(inst.ann_index);
        // Borrowed straight from the CSR pool (`execute_commands` is an associated fn; `self` is
        // never mutably borrowed here, so no defensive copy is needed). Most arcs have an EMPTY
        // command range — the `is_empty` guards below skip `execute_commands` entirely for those
        // (a no-op either way: it just loops over the commands), so `Rc::make_mut` deep-copies the
        // shared register scaffold only when an arc genuinely writes. Finisher command ranges
        // (`fin_lo..fin_hi`) are untouched by this: `check_accepting` runs them on its own
        // per-result copy of the registers, as before.
        let arc_cmds = &self.fst.commands[arc.cmd_lo as usize..arc.cmd_hi as usize];

        if next_index < self.n() {
            let next_offset = self.ann_start(next_index);
            let next_start = true;
            let mut out: Vec<Inst> = Vec::new();

            // optional next annotation -> also spawn the "skip it" path (registers pre-command).
            if self.seg(next_index).optional {
                let mut ti = inst.clone(); // O(1): registers shared, deep-copied only on write
                ti.ann_index = next_index;
                out.extend(self.advance(ti, arc, true, cur_results));
            }

            if !arc_cmds.is_empty() {
                let regs: &mut Vec<Register> = Rc::make_mut(&mut inst.registers);
                Self::execute_commands(
                    regs,
                    arc_cmds,
                    Register::at(next_offset, next_start),
                    Register::at(end, false),
                );
            }
            if !optional || self.end_anchor {
                self.check_accepting(
                    next_index,
                    &inst.registers,
                    arc.target as usize,
                    cur_results,
                );
            }
            inst.state = arc.target as usize;
            inst.ann_index = next_index;
            out.push(inst);
            out
        } else {
            let next_offset = self.end_pos();
            if !arc_cmds.is_empty() {
                let regs: &mut Vec<Register> = Rc::make_mut(&mut inst.registers);
                Self::execute_commands(
                    regs,
                    arc_cmds,
                    Register::at(next_offset, false),
                    Register::at(end, false),
                );
            }
            self.check_accepting(self.n(), &inst.registers, arc.target as usize, cur_results);
            inst.state = arc.target as usize;
            inst.ann_index = self.n();
            vec![inst]
        }
    }

    /// C# `Initialize` (TraversalMethodBase.cs:193-246), linear model. Advances `ann_index` past
    /// the (single, in the linear model) co-starting annotation and returns seed instances.
    fn initialize(
        &self,
        ann_index: &mut usize,
        registers: &mut [Register],
        cmds: &[Cmd],
        init_anns: &mut HashSet<usize>,
    ) -> Vec<Inst> {
        let mut insts: Vec<Inst> = Vec::new();
        let offset = self.ann_start(*ann_index);

        if self.start_anchor && self.seg(*ann_index).optional {
            let next_index = *ann_index + 1;
            if next_index != self.n() {
                let mut regs = registers.to_vec();
                let mut ni = next_index;
                insts.extend(self.initialize(&mut ni, &mut regs, cmds, init_anns));
            }
        }

        Self::execute_commands(
            registers,
            cmds,
            Register::at(offset, true),
            Register::unset(),
        );

        // linear: exactly one annotation starts at `offset`.
        if *ann_index < self.n() && !init_anns.contains(ann_index) {
            insts.push(Inst {
                state: self.fst.start() as usize,
                ann_index: *ann_index,
                registers: Rc::new(registers.to_vec()),
            });
            init_anns.insert(*ann_index);
        }
        *ann_index += 1;
        insts
    }

    fn check_accepting_start_state(
        &self,
        init_anns: &HashSet<usize>,
        registers: &[Register],
        cur_results: &mut Vec<FstResult>,
    ) {
        let start = self.fst.start() as usize;
        if !self.fst.states[start].accepting {
            return;
        }
        for &ann_index in init_anns {
            self.check_accepting(ann_index, registers, start, cur_results);
        }
    }

    // --- per-start-position traversal ---------------------------------------------------------

    fn traverse_from(
        &self,
        ann_index: &mut usize,
        init_registers: &mut [Register],
        cmds: &[Cmd],
        init_anns: &mut HashSet<usize>,
    ) -> Vec<FstResult> {
        let deterministic = self.fst.is_deterministic();
        let seeds = self.initialize(ann_index, init_registers, cmds, init_anns);
        let mut stack: Vec<Inst> = seeds;
        let mut cur_results: Vec<FstResult> = Vec::new();

        if deterministic {
            let __o2_det_start = Instant::now();
            while let Some(inst) = stack.pop() {
                // `Arc` is `Copy` and `state_arcs` borrows straight from the frozen `Fst`'s CSR
                // pool, so the slice can be iterated in place instead of cloned into a fresh
                // `Vec` every pop — no other borrow of `self` conflicts (`check_input_match`/
                // `advance` are themselves `&self` methods). Same arcs, same order, same result.
                let arcs = self.state_arcs(inst.state);
                let mut advanced = false;
                for arc in arcs {
                    if self.check_input_match(arc, inst.ann_index) {
                        for ni in self.advance(inst, arc, false, &mut cur_results) {
                            stack.push(ni);
                        }
                        advanced = true;
                        break;
                    }
                }
                let _ = advanced;
            }
            profile::record_det(__o2_det_start.elapsed().as_nanos());
        } else {
            let __o2_nondet_start = Instant::now();
            // Same visited-set key as before (and as the C# reference): the full
            // (state, ann_index, register contents) triple — two insts at the same
            // (state, ann_index) with different registers are genuinely different analyses and
            // both stay live. `RegKey` only changes the key's *representation* (shares the
            // instance's Rc instead of deep-cloning the registers a second time).
            let mut traversed: HashSet<(usize, usize, RegKey)> = HashSet::new();
            let n = self.n();
            let min_hops = self.fst.min_hops_to_accept();
            while let Some(inst) = stack.pop() {
                // See the deterministic branch's comment above: `Arc` is `Copy` and this slice
                // borrows straight from the frozen `Fst`, so no per-pop clone is needed.
                let arcs = self.state_arcs(inst.state);
                for arc in arcs {
                    // frozen FSTs have no epsilon arcs; only the input-match branch fires.
                    if self.check_input_match(arc, inst.ann_index) {
                        // Min-hops-to-accept pruning (`Fst::min_hops_to_accept`, an admissible
                        // lower bound computed at freeze time). After this arc consumes the
                        // segment at `ann_index` (`check_input_match` guarantees
                        // `ann_index < n`), at most `n - ann_index - 1` further arcs can ever be
                        // taken: every arc consumes >= 1 segment (frozen FSTs are epsilon-free),
                        // and `advance`'s optional-segment skips consume *extra* segments without
                        // taking arcs, so they only lower the true count — `remaining` stays an
                        // upper bound in both traversal directions (`ann_index` is a traversal
                        // index; `phys()` only remaps which physical segment it denotes, not how
                        // many are left). If even `remaining` hops cannot reach an accepting
                        // state from `arc.target`, no thread through this arc can ever produce a
                        // result — results are only emitted by `check_accepting` at accepting
                        // states, and an accepting `arc.target` itself (min_hops == 0) is never
                        // pruned since `remaining >= 0`. Dropping the thread loses nothing.
                        let remaining = (n - inst.ann_index - 1) as u32;
                        if min_hops[arc.target as usize] > remaining {
                            continue;
                        }
                        for new_inst in self.advance(inst.clone(), arc, false, &mut cur_results) {
                            let key = (
                                new_inst.state,
                                new_inst.ann_index,
                                RegKey(Rc::clone(&new_inst.registers)),
                            );
                            if traversed.insert(key) {
                                stack.push(new_inst);
                            }
                        }
                    }
                }
            }
            profile::record_nondet(__o2_nondet_start.elapsed().as_nanos(), traversed.len());
        }

        self.check_accepting_start_state(init_anns, init_registers, &mut cur_results);
        cur_results
    }

    // --- Transduce (Fst.cs:304-414) -----------------------------------------------------------

    fn run(&self, all_matches: bool) -> Vec<FstResult> {
        let __o2_start = Instant::now();
        let __o2_result = self.run_inner(all_matches);
        profile::record_run(__o2_start.elapsed().as_nanos());
        __o2_result
    }

    fn run_inner(&self, all_matches: bool) -> Vec<FstResult> {
        if self.n() == 0 {
            return Vec::new();
        }
        let reg_slots = self.fst.register_count() as usize * 2;
        let mut result_list: Vec<FstResult> = Vec::new();
        let mut init_anns: HashSet<usize> = HashSet::new();
        let mut ann_index = 0usize;

        while ann_index < self.n() {
            let mut init_registers = vec![Register::unset(); reg_slots];
            let offset = self.ann_start(ann_index);
            // initializers: dest==0 set directly; others -> cmds (Fst.cs:376-387).
            let mut cmds: Vec<Cmd> = Vec::new();
            for cmd in &self.fst.initializers {
                if cmd.dest == 0 {
                    init_registers[0] = Register::at(offset, true);
                } else {
                    cmds.push(*cmd);
                }
            }

            let mut cur =
                self.traverse_from(&mut ann_index, &mut init_registers, &cmds, &mut init_anns);
            if !cur.is_empty() {
                cur.sort_by(|a, b| self.result_compare(a, b));
                result_list.append(&mut cur);
                if !all_matches {
                    break;
                }
            }

            if self.start_anchor {
                break;
            }
        }

        if all_matches {
            let __o2_distinct_start = Instant::now();
            let __o2_distinct_input_len = result_list.len();
            let __o2_distinct_result = distinct(result_list);
            profile::record_distinct(
                __o2_distinct_start.elapsed().as_nanos(),
                __o2_distinct_input_len,
            );
            __o2_distinct_result
        } else {
            result_list
        }
    }

    /// All matches (C# `Transduce(..., out IEnumerable results)` / `Matcher.AllMatches`).
    pub fn all_matches(&self) -> Vec<FstResult> {
        self.run(true)
    }

    /// The single best match (C# `Transduce(..., out FstResult)` → `results.First()`).
    pub fn first_match(&self) -> Option<FstResult> {
        self.run(false).into_iter().next()
    }

    /// Does the automaton accept? (any match exists).
    pub fn accepts(&self) -> bool {
        self.first_match().is_some()
    }

    /// C# `ResultCompare` (Fst.cs:416-441), minus the `Priorities` zip tiebreak (T-C,
    /// rust-optimizations-phase2.md "Tear-out candidates"): C#'s nondeterministic branch breaks
    /// ties on accept-priority + `NextAnnotation` by comparing the arc-priority trail, but that
    /// trail is byte-parity-only machinery — removing it was verified order-invariant by A/B
    /// diffing Indonesian, Amharic, and a Sena probe under step-caps low enough to truncate most
    /// or all words (forcing exactly the code path the trail exists for), at multiple cap levels;
    /// every comparison came back byte-identical, not just set-equal. The deterministic branch's
    /// `IsLazy` flip is unaffected and stays.
    pub fn result_compare(&self, x: &FstResult, y: &FstResult) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let c = x.priority.cmp(&y.priority);
        if c != Ordering::Equal {
            return c;
        }
        // -x.NextAnnotation.CompareTo(y.NextAnnotation)
        let mut c = y.next_ann.cmp(&x.next_ann);
        if self.fst.direction() == Direction::RightToLeft {
            c = c.reverse();
        }
        if self.fst.is_deterministic() && x.is_lazy {
            c = c.reverse();
        }
        if c != Ordering::Equal {
            return c;
        }
        x.order.cmp(&y.order)
    }
}

/// Hash a `FstResult` consistently with `FstResult::result_eq`: the `id`, the register count,
/// and each register in **canonicalized** form — an unset register (`has == false`) contributes
/// only its `has` bit, mirroring how `Register::value_eq` ignores `offset`/`start` when unset.
/// (Today `Register::unset()` is the only `has == false` constructor and always zeroes those
/// fields, so `Register`'s derived `Hash` would coincide in practice — but canonicalizing here
/// removes the reliance on that invariant: `result_eq`-equal results hash identically by
/// construction, not by bit-pattern luck.) `priority`/`is_lazy`/`next_ann`/`order` are excluded,
/// exactly as `result_eq` excludes them.
fn result_hash<H: std::hash::Hasher>(r: &FstResult, hasher: &mut H) {
    use std::hash::Hash;
    r.id.hash(hasher);
    r.registers.len().hash(hasher);
    for reg in &r.registers {
        reg.has.hash(hasher);
        if reg.has {
            reg.offset.hash(hasher);
            reg.start.hash(hasher);
        }
    }
}

/// C# `Enumerable.Distinct` over `FstResult.Equals`, order-preserving (first occurrence wins).
///
/// Hash-backed (O2 fix, `docs/o2-profile-findings.md`): the original implementation linearly
/// scanned everything kept so far with pairwise `result_eq` — `O(n × kept)`, and `n` reached
/// 327K–501K on the pathological Amharic words, making this single step 59–81% of total
/// wall-clock. C#'s `Enumerable.Distinct(IEqualityComparer)` is hash-set-backed; this mirrors it
/// with a hash → first-occurrence-indices table (buckets are almost always singletons), falling
/// back to `result_eq` within a bucket so equality semantics are bit-for-bit the old scan's.
/// Which duplicate survives matters — `result_eq` ignores `priority`/`next_ann`/`order`, and
/// downstream consumers (`first_match`, rule application order) depend on the sorted result
/// order — so the output is exactly the old scan's: first occurrence wins, order preserved.
fn distinct(results: Vec<FstResult>) -> Vec<FstResult> {
    use std::collections::HashMap;
    use std::hash::{BuildHasher, Hasher};

    let state = std::collections::hash_map::RandomState::new();
    let mut buckets: HashMap<u64, Vec<usize>> = HashMap::with_capacity(results.len());
    let mut out: Vec<FstResult> = Vec::new();
    for r in results {
        let mut hasher = state.build_hasher();
        result_hash(&r, &mut hasher);
        let bucket = buckets.entry(hasher.finish()).or_default();
        if bucket.iter().all(|&i| !out[i].result_eq(&r)) {
            bucket.push(out.len());
            out.push(r);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    /// `RegKey`'s Eq/Hash contract: content-equal keys in DIFFERENT allocations must be equal and
    /// must collide (hash is content-based, never pointer-based); the `Rc::ptr_eq` fast path is
    /// only ever an accelerator for the same-allocation case.
    #[test]
    fn regkey_eq_and_hash_are_content_based() {
        let a = RegKey(Rc::new(vec![Register::at(1, true), Register::unset()]));
        let b = RegKey(Rc::new(vec![Register::at(1, true), Register::unset()])); // distinct alloc
        let c = RegKey(Rc::clone(&a.0)); // same alloc (ptr_eq fast path)
        let d = RegKey(Rc::new(vec![Register::at(2, true), Register::unset()]));
        assert!(a == b, "content-equal keys in different allocations");
        assert!(a == c, "shared-allocation keys");
        assert!(a != d, "different contents differ");
        assert_eq!(hash_of(&a), hash_of(&b), "equal keys must hash equal");
        assert_eq!(hash_of(&a), hash_of(&c));
        // and RegKey's hash must equal the plain Vec<Register> hash it replaced (same visited-set
        // behavior as the old (usize, usize, Vec<Register>) key, representation aside).
        assert_eq!(hash_of(&a), hash_of(&*a.0));
    }
}
