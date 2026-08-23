//! The per-word `--stats` collector: seven `u64` counters per `(object, stratum, allomorph,
//! direction)` row, gated behind `Option<&StatsCollector>` so an ordinary parse allocates nothing.
//!
//! Two storage shapes, because one does not fit both: **dense** `Vec<Cell<Counters>>` for
//! `(direction, stratum, rule)` triples — a grammar's rule count is dozens to hundreds — and a
//! **sparse** `RefCell<HashMap<..>>` for every `allomorph != ALLOMORPH_NONE` row plus every
//! lexical-entry row, since a lexicon runs to 10^4-10^5 entries and a word matches only a handful;
//! a dense array there would be megabytes of zeros per word. `.rows()` emits dense rows first, then
//! sparse rows sorted by `(kind, stratum, object_index, allomorph, direction)` for a deterministic,
//! golden-testable order.

use std::cell::{Cell, RefCell};

use pg_grammar::model::{Grammar, LexEntryId, MRuleId, PRuleId, StratumId};
use rustc_hash::FxHashMap as HashMap;

/// The allomorph-dimension sentinel: cost belonging to no allomorph (rule-level setup, or a
/// candidate/lookup with no allomorph dimension at all).
pub const ALLOMORPH_NONE: u32 = 0;

/// Which pass produced a fact row: unapplying the surface form toward a root (`Analysis`), or
/// reapplying rules forward to build/confirm a surface form (`Synthesis`). Part of the fact key
/// alongside `stratum`/`allomorph`, never a counted object of its own.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Direction {
    Analysis,
    Synthesis,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Analysis => "analysis",
            Direction::Synthesis => "synthesis",
        }
    }
}

/// Finer-grained self-time phase inside the ANALYSIS rule-body invocation `ObjectKind::MorphRule`
/// already times; orthogonal to `ObjectKind` and never a counted `StatsRow` -- a `pangloss
/// calibrate`-style diagnostic dimension only, read via `StatsCollector::phase_totals`. `Other` is
/// entered as the outermost region around the whole invocation, so its self-time (after every
/// phase below is subtracted out as a nested region) is exactly the unattributed remainder.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AnalysisPhase {
    Other,
    AnaSynFs,
    SegsOf,
    AnaAffixAllomorph,
    FstTraversal,
    AnaRealizational,
    AnaCompound,
}

/// Which grammar object a fact row is attributed to.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ObjectKind {
    MorphRule,
    PhonRule,
    LexEntry,
    RootIndex,
    /// The lexical-pattern root guesser: no grammar-resident object, one pseudo-row per grammar.
    Guesser,
    /// The runtime supplied-root overlay: no grammar-resident object, one pseudo-row per grammar.
    Overlay,
}

/// The seven counters for one `(object, stratum, allomorph)` row.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Rule-level only: one unit per `StepBudget` tick, regardless of how many allomorphs were tried; every allomorph-dimension row (`allomorph != ALLOMORPH_NONE`) carries `attempts == 0`.
    pub attempts: u64,
    pub work: u64,
    pub outputs: u64,
    /// Both rule-level (the `ALLOMORPH_NONE` row, one per invocation) and per-allomorph (one per failing try); a per-object report must sum only the former to stay comparable with `attempts`.
    pub not_applied: u64,
    pub no_root: u64,
    pub surface_mismatch: u64,
    pub uses: u64,
}

/// One collected fact row, drained at end of word.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StatsRow {
    pub kind: ObjectKind,
    /// The object's own dense id within `kind` (an `MRuleId`/`PRuleId`/`LexEntryId`, or the stratum
    /// index for `RootIndex`, which has no id of its own).
    pub object_index: u32,
    pub stratum: StratumId,
    pub allomorph: u32,
    pub direction: Direction,
    pub counters: Counters,
}

/// One rule-kind's counters, dense across every `(direction, stratum, rule)` triple, always at `ALLOMORPH_NONE`, laid out as two direction-major blocks so decoding a flat index is exact division/modulo.
struct DenseTable {
    num_strata: usize,
    num_rules: usize,
    cells: Box<[Cell<Counters>]>,
}

impl DenseTable {
    fn new(num_strata: usize, num_rules: usize) -> Self {
        let num_rules = num_rules.max(1);
        let num_strata = num_strata.max(1);
        let n = 2 * num_strata * num_rules;
        DenseTable {
            num_strata,
            num_rules,
            cells: (0..n).map(|_| Cell::new(Counters::default())).collect(),
        }
    }

    fn dir_offset(direction: Direction) -> usize {
        match direction {
            Direction::Analysis => 0,
            Direction::Synthesis => 1,
        }
    }

    fn index(&self, direction: Direction, stratum: StratumId, rule_index: u32) -> usize {
        let per_direction = self.num_strata * self.num_rules;
        Self::dir_offset(direction) * per_direction
            + stratum.0 as usize * self.num_rules
            + rule_index as usize
    }

    fn with_row(
        &self,
        direction: Direction,
        stratum: StratumId,
        rule_index: u32,
        f: impl FnOnce(&mut Counters),
    ) {
        let cell = &self.cells[self.index(direction, stratum, rule_index)];
        let mut c = cell.get();
        f(&mut c);
        cell.set(c);
    }
}

/// Sparse-table key: identifies one row outside the dense `(stratum, rule)` grid.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct SparseKey {
    kind: ObjectKind,
    stratum: StratumId,
    object_index: u32,
    allomorph: u32,
    direction: Direction,
}

/// Bundles what a phonological-rule instrumentation site needs to attribute its counters: which
/// collector, which stratum context it ran under, and the rule's own id. `analyze`'s standalone
/// test callers have no grammar-resident `PRuleId` to hand in, which is why `stats` is `Option`
/// here rather than the collector alone.
#[derive(Copy, Clone)]
pub struct PRuleStatsCtx<'a> {
    pub stats: &'a StatsCollector,
    pub stratum: StratumId,
    pub id: PRuleId,
    pub direction: Direction,
}

/// Mirrors `PRuleStatsCtx` for the morphological-rule allomorph loops in `crate::morph`.
#[derive(Copy, Clone)]
pub struct MRuleStatsCtx<'a> {
    pub stats: &'a StatsCollector,
    pub stratum: StratumId,
    pub id: MRuleId,
    pub direction: Direction,
}

/// The per-word collector. Constructed once per `parse_word` call, alongside its `StepBudget`,
/// and threaded down by `Option<&StatsCollector>` so every instrumentation site is one branch.
pub struct StatsCollector {
    morph: DenseTable,
    phon: DenseTable,
    sparse: RefCell<HashMap<SparseKey, Counters>>,
    /// `pangloss calibrate`'s self-time accumulator; a no-op unless built with `stats-calibrate`.
    calib: crate::stats_calibrate::SelfTimeAccumulator<ObjectKind>,
    /// The finer `AnalysisPhase` breakdown; independent accumulator, same no-op-off-feature contract as `calib`.
    phase_calib: crate::stats_calibrate::SelfTimeAccumulator<AnalysisPhase>,
}

impl StatsCollector {
    pub fn new(g: &Grammar) -> Self {
        StatsCollector {
            morph: DenseTable::new(g.strata.len(), g.mrules.len()),
            phon: DenseTable::new(g.strata.len(), g.prules.len()),
            sparse: RefCell::new(HashMap::default()),
            calib: crate::stats_calibrate::SelfTimeAccumulator::new(),
            phase_calib: crate::stats_calibrate::SelfTimeAccumulator::new(),
        }
    }

    /// Enter a calibration self-time region for `kind`; real only when built with the
    /// `stats-calibrate` feature, a zero-cost no-op otherwise.
    pub fn calibrate_enter(
        &self,
        kind: ObjectKind,
        work: u64,
    ) -> crate::stats_calibrate::RegionGuard<'_, ObjectKind> {
        self.calib.enter(kind, work)
    }

    /// This collector's accumulated calibration totals; empty unless built with `stats-calibrate`.
    pub fn calibration_totals(&self) -> HashMap<ObjectKind, crate::stats_calibrate::KindTotals> {
        self.calib.totals()
    }

    /// Enter an `AnalysisPhase` self-time region; same no-op-unless-`stats-calibrate` contract as
    /// `Self::calibrate_enter`, on the independent phase accumulator.
    pub fn phase_enter(
        &self,
        phase: AnalysisPhase,
        work: u64,
    ) -> crate::stats_calibrate::RegionGuard<'_, AnalysisPhase> {
        self.phase_calib.enter(phase, work)
    }

    /// This collector's accumulated `AnalysisPhase` totals; empty unless built with
    /// `stats-calibrate`.
    pub fn phase_totals(&self) -> HashMap<AnalysisPhase, crate::stats_calibrate::KindTotals> {
        self.phase_calib.totals()
    }

    fn sparse_with_row(&self, key: SparseKey, f: impl FnOnce(&mut Counters)) {
        let mut map = self.sparse.borrow_mut();
        f(map.entry(key).or_default());
    }

    /// One morphological-rule attempt: `attempts += 1`, `work += segments`. Call only from the
    /// tick site — a pre-tick gate rejection is not an attempt. Also used as the `ALLOMORPH_NONE`
    /// residual by `crate::morph`'s allomorph-loop sites when no allomorph was reached at all.
    pub fn record_mrule_attempt(
        &self,
        stratum: StratumId,
        id: MRuleId,
        direction: Direction,
        segments: u64,
    ) {
        self.morph.with_row(direction, stratum, id.0, |c| {
            c.attempts += 1;
            c.work += segments;
        });
    }

    /// One morphological-rule outcome, after the rule body ran: `outputs += n`, and
    /// `not_applied += 1` when `n == 0`.
    pub fn record_mrule_outcome(
        &self,
        stratum: StratumId,
        id: MRuleId,
        direction: Direction,
        outputs: u64,
    ) {
        self.morph.with_row(direction, stratum, id.0, |c| {
            c.outputs += outputs;
            if outputs == 0 {
                c.not_applied += 1;
            }
        });
    }

    /// One phonological-rule attempt, mirroring `Self::record_mrule_attempt`.
    pub fn record_prule_attempt(
        &self,
        stratum: StratumId,
        id: PRuleId,
        direction: Direction,
        segments: u64,
    ) {
        self.phon.with_row(direction, stratum, id.0, |c| {
            c.attempts += 1;
            c.work += segments;
        });
    }

    /// One phonological-rule outcome, mirroring `Self::record_mrule_outcome`.
    pub fn record_prule_outcome(
        &self,
        stratum: StratumId,
        id: PRuleId,
        direction: Direction,
        outputs: u64,
    ) {
        self.phon.with_row(direction, stratum, id.0, |c| {
            c.outputs += outputs;
            if outputs == 0 {
                c.not_applied += 1;
            }
        });
    }

    /// The rule invocation's single `attempts` tick, for the case where at least one allomorph was
    /// reached (the zero-reached case ticks via `Self::record_mrule_attempt` instead). Recorded on
    /// the rule's own `ALLOMORPH_NONE` row, never on an allomorph row — `attempts` is rule-level,
    /// not per-allomorph; call at most once per invocation (the first allomorph reached).
    pub fn record_mrule_reach_attempt(
        &self,
        stratum: StratumId,
        id: MRuleId,
        direction: Direction,
    ) {
        self.morph
            .with_row(direction, stratum, id.0, |c| c.attempts += 1);
    }

    /// One allomorph/subrule's own match cost and outcome, independent of the `attempts` claim
    /// above: every allomorph actually reached (not gated/skipped) gets its own `work`/`outputs`.
    pub fn record_mrule_allomorph_try(
        &self,
        stratum: StratumId,
        id: MRuleId,
        allomorph: u32,
        direction: Direction,
        segments: u64,
        outputs: u64,
    ) {
        self.sparse_with_row(
            SparseKey {
                kind: ObjectKind::MorphRule,
                stratum,
                object_index: id.0,
                allomorph,
                direction,
            },
            |c| {
                c.work += segments;
                c.outputs += outputs;
                if outputs == 0 {
                    c.not_applied += 1;
                }
            },
        );
    }

    /// The rule invocation reached at least one allomorph but none of them produced output;
    /// ticks `not_applied` on the rule's own `ALLOMORPH_NONE` row (not an allomorph row), keeping
    /// this counter one-per-invocation like `attempts` rather than one-per-allomorph-try.
    pub fn record_mrule_invocation_not_applied(
        &self,
        stratum: StratumId,
        id: MRuleId,
        direction: Direction,
    ) {
        self.morph
            .with_row(direction, stratum, id.0, |c| c.not_applied += 1);
    }

    /// `no_root`: an analysis candidate's lexical lookup matched nothing, charged to the last rule
    /// applied on that candidate. Analysis-only: a failed lookup can only arise while peeling
    /// toward a root, never while confirming one forward.
    pub fn record_no_root_mrule(&self, stratum: StratumId, id: MRuleId) {
        self.morph
            .with_row(Direction::Analysis, stratum, id.0, |c| c.no_root += 1);
    }

    /// `no_root` charged to a phonological rule, when that is the last object applied instead.
    pub fn record_no_root_prule(&self, stratum: StratumId, id: PRuleId) {
        self.phon
            .with_row(Direction::Analysis, stratum, id.0, |c| c.no_root += 1);
    }

    /// One lexical-entry candidate materialized (one per matched entry × allomorph); analysis-only,
    /// since lexical lookup runs only on the way toward a root.
    pub fn record_lex_entry_attempt(&self, stratum: StratumId, entry: LexEntryId, allomorph: u32) {
        self.sparse_with_row(
            SparseKey {
                kind: ObjectKind::LexEntry,
                stratum,
                object_index: entry.0,
                allomorph,
                direction: Direction::Analysis,
            },
            |c| c.attempts += 1,
        );
    }

    /// One root-trie walk against a stratum's shared trie (the trie itself, not any one entry);
    /// `segments` is the walked shape's length, the same "segments touched" unit every kind uses.
    pub fn record_root_index_attempt(&self, stratum: StratumId, segments: u64) {
        self.sparse_with_row(
            SparseKey {
                kind: ObjectKind::RootIndex,
                stratum,
                object_index: 0,
                allomorph: ALLOMORPH_NONE,
                direction: Direction::Analysis,
            },
            |c| {
                c.attempts += 1;
                c.work += segments;
            },
        );
    }

    /// `no_root` charged to the stratum's own root lookup, for a candidate whose trail applied no
    /// rule at all -- the case `Self::record_no_root_mrule` cannot attribute to any rule.
    pub fn record_no_root_root_index(&self, stratum: StratumId) {
        self.sparse_with_row(
            SparseKey {
                kind: ObjectKind::RootIndex,
                stratum,
                object_index: 0,
                allomorph: ALLOMORPH_NONE,
                direction: Direction::Analysis,
            },
            |c| c.no_root += 1,
        );
    }

    /// One lexical-pattern guess attempt against a word's shape (one call to the guesser).
    pub fn record_guesser_attempt(&self, stratum: StratumId, segments: u64) {
        self.sparse_with_row(
            SparseKey {
                kind: ObjectKind::Guesser,
                stratum,
                object_index: 0,
                allomorph: ALLOMORPH_NONE,
                direction: Direction::Analysis,
            },
            |c| {
                c.attempts += 1;
                c.work += segments;
            },
        );
    }

    /// One supplied-root overlay candidate materialized.
    pub fn record_overlay_attempt(&self, stratum: StratumId, segments: u64) {
        self.sparse_with_row(
            SparseKey {
                kind: ObjectKind::Overlay,
                stratum,
                object_index: 0,
                allomorph: ALLOMORPH_NONE,
                direction: Direction::Analysis,
            },
            |c| {
                c.attempts += 1;
                c.work += segments;
            },
        );
    }

    /// `surface_mismatch`: this root was rebuilt by synthesis and did not match the actual word --
    /// synthesis-only by construction, mirroring `Self::record_no_root_mrule`'s analysis-only tag.
    pub fn record_surface_mismatch(&self, stratum: StratumId, entry: LexEntryId, allomorph: u32) {
        self.sparse_with_row(
            SparseKey {
                kind: ObjectKind::LexEntry,
                stratum,
                object_index: entry.0,
                allomorph,
                direction: Direction::Synthesis,
            },
            |c| c.surface_mismatch += 1,
        );
    }

    /// `uses`: a morphological rule appeared in at least one surviving (gate-passing) analysis.
    /// Tagged `Synthesis`: commit-on-pass fires from the re-synthesis confirm walk, alongside that
    /// same rule's synthesis-direction attempts.
    pub fn record_use_mrule(&self, stratum: StratumId, id: MRuleId) {
        self.morph
            .with_row(Direction::Synthesis, stratum, id.0, |c| c.uses += 1);
    }

    /// `uses`: a phonological rule appeared in at least one surviving analysis.
    pub fn record_use_prule(&self, stratum: StratumId, id: PRuleId) {
        self.phon
            .with_row(Direction::Synthesis, stratum, id.0, |c| c.uses += 1);
    }

    /// `uses`: a lexical entry appeared as the resolved root of a surviving analysis.
    pub fn record_use_lex_entry(&self, stratum: StratumId, entry: LexEntryId, allomorph: u32) {
        self.sparse_with_row(
            SparseKey {
                kind: ObjectKind::LexEntry,
                stratum,
                object_index: entry.0,
                allomorph,
                direction: Direction::Synthesis,
            },
            |c| c.uses += 1,
        );
    }

    /// Every non-zero row, for a later task to persist. A word parsed with stats off never calls
    /// this, since it never has a `StatsCollector` to call it on. Sparse rows are sorted by
    /// `(kind, stratum, object_index, allomorph, direction)` after the dense rows so repeated runs
    /// (and runs across `--threads` values) produce byte-identical output.
    pub fn rows(&self) -> Vec<StatsRow> {
        let mut out = Vec::new();
        collect_rows(&self.morph, ObjectKind::MorphRule, &mut out);
        collect_rows(&self.phon, ObjectKind::PhonRule, &mut out);

        let mut sparse_rows: Vec<StatsRow> = self
            .sparse
            .borrow()
            .iter()
            .filter(|(_, c)| **c != Counters::default())
            .map(|(k, c)| StatsRow {
                kind: k.kind,
                object_index: k.object_index,
                stratum: k.stratum,
                allomorph: k.allomorph,
                direction: k.direction,
                counters: *c,
            })
            .collect();
        sparse_rows.sort_by_key(|r| {
            (
                r.kind,
                r.stratum.0,
                r.object_index,
                r.allomorph,
                r.direction,
            )
        });
        out.extend(sparse_rows);
        out
    }
}

/// The `(kind, counter)` pairs this collector actually populates today -- the single source of
/// truth for which cells a coverage row may mark `Measured`, kept next to the recording methods
/// above so wiring and this list cannot drift apart; pinned by
/// `wired_counters_matches_reality`. Orthogonal to `Direction`: which counters exist for a kind
/// does not depend on which direction wrote them, so this list needs no direction dimension of its
/// own -- `MorphRule`'s `attempts` is `Measured` whether the row's direction is analysis or
/// synthesis.
pub const WIRED_COUNTERS: &[(ObjectKind, &str)] = &[
    (ObjectKind::MorphRule, "attempts"),
    (ObjectKind::MorphRule, "work"),
    (ObjectKind::MorphRule, "outputs"),
    (ObjectKind::MorphRule, "not_applied"),
    (ObjectKind::MorphRule, "no_root"),
    (ObjectKind::MorphRule, "uses"),
    (ObjectKind::PhonRule, "attempts"),
    (ObjectKind::PhonRule, "work"),
    (ObjectKind::PhonRule, "outputs"),
    (ObjectKind::PhonRule, "not_applied"),
    (ObjectKind::LexEntry, "attempts"),
    (ObjectKind::LexEntry, "surface_mismatch"),
    (ObjectKind::LexEntry, "uses"),
    (ObjectKind::RootIndex, "attempts"),
    (ObjectKind::RootIndex, "work"),
    (ObjectKind::RootIndex, "no_root"),
    (ObjectKind::Guesser, "attempts"),
    (ObjectKind::Guesser, "work"),
    (ObjectKind::Overlay, "attempts"),
    (ObjectKind::Overlay, "work"),
];

fn collect_rows(table: &DenseTable, kind: ObjectKind, out: &mut Vec<StatsRow>) {
    let per_direction = table.num_strata * table.num_rules;
    for (i, cell) in table.cells.iter().enumerate() {
        let counters = cell.get();
        if counters == Counters::default() {
            continue;
        }
        let direction = if i / per_direction == 0 {
            Direction::Analysis
        } else {
            Direction::Synthesis
        };
        let rem = i % per_direction;
        out.push(StatsRow {
            kind,
            object_index: (rem % table.num_rules) as u32,
            stratum: StratumId((rem / table.num_rules) as u8),
            allomorph: ALLOMORPH_NONE,
            direction,
            counters,
        });
    }
}
