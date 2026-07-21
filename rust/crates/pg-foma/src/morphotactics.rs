//! Morphotactic pruning for `crate::preexpand::extend`/`crate::emit::struct_extend` (plan
//! `docs/fst-plan/morphotactic-composite-pruning.md`, the Aweti scale fix): both composite chain
//! builders used to chain **every** candidate rule onto every root at every depth, gated only by
//! the cheap `required_syn_fs` unifiability pre-filter -- exploring rule orders the engine can
//! never actually produce in synthesis. This module builds, once per grammar, a subset-construction
//! automaton over the engine's own morphotactics (`pg-rules/src/stratum.rs`: `synth_apply_mrules`,
//! `synth_apply_templates`, `synth_slots_generic`, `slot_optional`) and exposes a single
//! [`MorphotacticIndex::next_state`] query the two builders consult immediately before recursing
//! on a candidate rule -- restricting the flat recursion to a strict subset of engine-legal chains,
//! never widening it (recall-preserving by construction: pruned exploration is always a subset of
//! flat exploration).
//!
//! ## The engine facts this automaton mirrors (plan doc "Fix" section)
//! 1. Strata fold in document order 0..n (`pg-parse/src/morpher.rs::synthesis_pipeline_selected`);
//!    a stratum applies only to words whose root stratum is not deeper -- so a chain's rules come
//!    from a **non-decreasing stratum sequence** starting at the root's own stratum. [`ChainState::
//!    free`] is that floor: `Some(s)` means "loose rules and template entries at strata >= s are
//!    legal right now"; `None` means "mid-template only" (see point 3).
//! 2. Loose rules (`sd.mrules`) run in a Linear or Unordered cascade (`synth_apply_mrules`,
//!    stratum.rs:1710-1712). v1 over-approximates Linear as Unordered (any order) -- sound,
//!    simpler, and the plan doc's explicitly named out-of-scope item.
//! 3. Template slot rules apply **only inside a template application, in ascending slot order**
//!    (`synth_slots_generic`, stratum.rs:1339-1388): a non-optional slot that produces nothing
//!    terminates the walk (`if !slot_optional(slot) { return; }`, stratum.rs:1373-1379). The
//!    template completes early (`out.entry(input)...`, stratum.rs:1386-1387) only when every
//!    remaining slot is optional. [`ChainState::mid`] tracks live `(template, slot)` positions;
//!    firing a rule can advance a position to any later slot as long as every slot strictly
//!    between is *skippable* (see the vacuous-slot trap below).
//! 4. Under `Unordered`, a changed template output recurses back into the mrules cascade
//!    (stratum.rs:1923-1939) -- so loose rules (and further templates) can only follow a
//!    *completed* template, never a mid-template position. A completed slot position therefore
//!    also grants a fresh `free` floor at the template's own owning stratum.
//! 5. Template application is gated by `is_unifiable(input.syn_fs, tmpl.required_syn_fs)`
//!    (stratum.rs:1861, the same gate this module's `next_state` re-checks) and by
//!    `!root_is_partial` (`root_is_partial`, stratum.rs:1743-1756, checked at
//!    `synth_apply_templates`'s stratum.rs:1855 `root_partial` gate) -- a partial root's word NEVER
//!    enters ANY template, for the lifetime of the whole chain (not just the current call), so
//!    [`ChainState::seed`] bakes this into `template_entry_disabled`, carried unchanged by every
//!    [`MorphotacticIndex::next_state`] transition.
//!
//! ## Recall trap: surface-vacuous rules in mandatory slots (plan doc "Recall trap")
//! A realizational rule whose allomorph RHS is EXACTLY `[Copy(0), Copy(1), .., Copy(n-1)]` (every
//! LHS part copied, in order, and NOTHING else -- no `InsertSegments`, no `Modify`, no
//! `InsertContext`) adds no surface material at all; the engine still applies it in a mandatory
//! slot, so a composite chain that only ever chains Prefix/Suffix/Infix candidates must be allowed
//! to jump over such a slot and still match the engine's own surface. [`rule_may_be_vacuous`] is
//! the STRICT (exact-match) version of this test -- deliberately narrower than the throwaway
//! `examples/aweti_probe.rs`'s own `rule_may_be_vacuous` (which treats "some allomorph inserts no
//! non-empty text" as vacuous, a looser and UNSOUND-for-pruning test: a rule whose RHS reorders or
//! drops LHS parts without inserting anything still changes the shape, so treating it as skippable
//! could cause `extend`/`struct_extend` to jump a slot the engine's real word would not have
//! skipped, which is a recall-losing direction the plan's iron rule forbids). Do not loosen this
//! back to the example's version without re-deriving the soundness argument.
//!
//! `slot_skippable(slot) = slot.rules.is_empty() || slot.optional || slot.rules.any(rule_may_be_vacuous)`
//! is used everywhere the engine walk uses `slot_optional` -- a strict, recall-safe
//! over-approximation (costs only extra exploration, never drops a legal chain). A `Compounding`
//! rule in a slot's rule list counts as non-vacuous unconditionally (per the plan doc: compounding
//! always consumes a real extra root, never a silent skip).
use std::sync::atomic::{AtomicUsize, Ordering};

use pg_featstruct::{is_unifiable, FeatureStruct, FsId, Interner};
use pg_grammar::model::{Grammar, MRuleId, MorphRuleDef, OutputAction, PartRef, SlotDef};
use rustc_hash::FxHashMap;

/// The flat/pruned escape hatch (plan doc "Wiring": "an internal parameter... NOT a runtime branch
/// tests can't control"). Threaded explicitly from every caller -- `crate::emit::emit_with_precision`
/// resolves this from `HC_PREEXPAND_FLAT` exactly once (via [`explore_mode_from_env`]) and passes it
/// down; unit/gate tests construct it directly, never through the env var, so parallel test
/// processes never race process-global env state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExploreMode {
    /// Consult [`MorphotacticIndex::next_state`] before every recursive step -- the production
    /// default.
    Pruned,
    /// Skip the automaton entirely (`Some(state.clone())` unconditionally) -- the pre-fix
    /// behavior, kept only for A/B measurement (plan doc "Sizing results" table; the Amharic
    /// subset gate compares this against `Pruned`).
    Flat,
}

/// Resolves the flat/pruned choice for the PRODUCTION `emit`/`emit_with_precision` path from
/// `HC_PREEXPAND_FLAT` (plan doc's env-gated-diagnostic precedent, mirroring the repo's existing
/// `CENSUS_DUMP_D5` convention). Read exactly ONCE per `emit_with_precision` call -- tests must
/// construct [`ExploreMode`] directly, never call this, so parallel test threads/processes never
/// race process-global env state (plan doc "Wiring").
pub(crate) fn explore_mode_from_env() -> ExploreMode {
    match std::env::var("HC_PREEXPAND_FLAT") {
        Ok(v) if v == "1" => ExploreMode::Flat,
        _ => ExploreMode::Pruned,
    }
}

/// `HC_PREEXPAND_PROBE_CAP=<n>` (measurement-only, off by default): the total probe ceiling shared
/// across BOTH `crate::preexpand::build_composites_with_mode` and
/// `crate::emit::build_structural_composites` for one `emit`/`emit_with_precision` call (plan doc
/// "Instrumentation" -- "measuring Aweti can never OOM the machine again"). `None` (the env var
/// unset) means production behavior, completely unchanged -- callers must not build a
/// [`ProbeBudget`] at all in that case.
pub(crate) fn probe_cap_from_env() -> Option<usize> {
    std::env::var("HC_PREEXPAND_PROBE_CAP")
        .ok()
        .and_then(|v| v.parse().ok())
}

/// A live, shared ceiling for `HC_PREEXPAND_PROBE_CAP`-gated measurement runs. `counter` is a
/// reference into an `AtomicUsize` owned by the caller's own stack frame (no `Arc` needed: every
/// parallel worker this gets threaded into -- both builders' rayon pools -- runs strictly within
/// the lifetime of the `emit_with_precision`/`build_composites` call that owns the counter), so one
/// flat total spans both builders: a per-builder-only cap would let one silently blow the budget
/// while the other stayed capped, defeating the "never OOM again" promise. `Copy`: this is a cap
/// integer plus a fat reference, cheap to pass by value into every `ExtendCtx`/`StructCtx`.
#[derive(Clone, Copy)]
pub(crate) struct ProbeBudget<'a> {
    pub cap: usize,
    pub counter: &'a AtomicUsize,
}

impl<'a> ProbeBudget<'a> {
    /// Record one probe (module doc: "each probe increments"); panics with the cap and the total
    /// so far if this exceeds it. `Ordering::Relaxed` is enough -- this is a measurement-only abort
    /// guard, not a synchronization point for any other shared state.
    pub(crate) fn tick(&self) {
        let n = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        assert!(
            n <= self.cap,
            "HC_PREEXPAND_PROBE_CAP={} exceeded: {n} probes attempted across build_composites + \
             build_structural_composites. This is a measurement-only safety valve (plan doc \
             docs/fst-plan/morphotactic-composite-pruning.md's 'Instrumentation' section), not a \
             production limit -- re-run without the cap only once you understand why the dynamic \
             tree is this large.",
            self.cap
        );
    }
}

// --- Default-on enumeration budget (Fix 1: fail-fast on the Aweti-scale blow-up) ----------------
//
// `ProbeBudget` above is measurement-only: off by default (`HC_PREEXPAND_PROBE_CAP` unset), and it
// PANICS when tripped -- a deliberate choice for a diagnostic tool a developer runs by hand, but
// wrong for production, where a caller needs a typed `Result`, never an unwind. `EnumerationBudget`
// is its default-ON, non-panicking sibling: always live in `crate::emit::emit`/`emit_with_precision`,
// it just sets a shared, cross-thread latch the instant either of its two measures crosses its
// threshold; every recursive enumeration call (`crate::preexpand::extend`, `crate::emit::
// struct_extend`) checks that latch before doing further work and bails out immediately if it is
// set -- the same "check once, return early" shape those functions already use for their own
// `depth >= MAX_EXTRA_RULES`/`STRUCT_MAX_EXTRA_RULES` guards. `crate::emit::emit_with_precision`
// reads the latch once both composite builders return and turns a trip into `FomaTier::Unsupported`
// plus a structured `EnumBudgetExceeded`, which `FomaProposer::new` (crate::analyzer) turns into a
// typed `FomaError::EnumerationBudgetExceeded` -- an honest, specific error, never a panic, never a
// silent OOM.
//
// ## Why two measures, not one
// A "pairs probed" cap alone does not catch every blow-up shape. The investigation that motivated
// this fix found the Aweti grammar (855 roots, 123 rules, 3 strata, 14 templates) probes "only"
// ~8.37 million (root, rule) pairs -- a number that looks merely large in isolation -- before
// producing 2,833,559 composite (fusion) entries, a 691 MB / 9.7M-line `.lexc`, and eventually an
// ~8.8 GB `apply_up` allocation that kills the process on the very first word. The number that
// actually predicts the disaster is the RESULT of those probes -- composite lexc entries emitted
// (fusion + interdigitation + structural) -- not the probe count itself. So this budgets on BOTH:
// composite entries (the primary, disaster-predicting measure) and pairs probed (a cheap secondary
// backstop for a grammar whose search runs away without yet producing many entries).
//
// ## Default thresholds
// - `DEFAULT_ENTRY_BUDGET = 200_000` composite entries (fusion + interdigitation + structural,
//   combined -- the same three counters `EmitCounts` already reports). Amharic, the largest
//   reference grammar that must keep working, produces 22,775 fusion entries and zero
//   interdigitation/structural entries at this writing -- comfortably under this cap by ~8.8x.
//   Aweti's 2,833,559 crosses it after roughly 7% of its own full enumeration -- well before the
//   691 MB lexc / 8.8 GB allocation, and (empirically) in low tens of seconds rather than 551s.
// - `DEFAULT_PROBE_BUDGET = 3_000_000` (root, rule) pairs probed. Amharic probes ~305k pairs for
//   `build_composites`'s own mechanism (structural composites add zero for it) -- ~9.8x margin.
//   Aweti probes ~8.37M -- comfortably over this cap -- so a grammar shaped like Aweti but with a
//   lower composite-entry yield per probe (so the entry cap alone would not catch it quickly) still
//   trips fast on pure search-tree size.
//
// ## Env override
// `HC_ENUM_ENTRY_BUDGET=<n>` / `HC_ENUM_PROBE_BUDGET=<n>` (parsed as `usize`; unset or unparsable
// falls back to the default above), mirroring the existing `HC_PREEXPAND_PROBE_CAP` convention: a
// power user who understands the grammar's shape can raise either cap (or set it to a huge value to
// effectively disable that measure) and re-run.
pub(crate) const DEFAULT_ENTRY_BUDGET: usize = 200_000;
pub(crate) const DEFAULT_PROBE_BUDGET: usize = 3_000_000;

pub(crate) fn entry_budget_from_env() -> usize {
    std::env::var("HC_ENUM_ENTRY_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_ENTRY_BUDGET)
}

pub(crate) fn probe_budget_from_env() -> usize {
    std::env::var("HC_ENUM_PROBE_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_PROBE_BUDGET)
}

/// Which measure tripped [`EnumerationBudget`] -- surfaced through
/// `crate::analyzer::FomaError::EnumerationBudgetExceeded`'s error message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnumMeasure {
    CompositeEntries,
    PairsProbed,
}

impl EnumMeasure {
    pub(crate) fn label(self) -> &'static str {
        match self {
            EnumMeasure::CompositeEntries => {
                "composite lexc entries (fusion + interdigitation + structural)"
            }
            EnumMeasure::PairsProbed => "(root, rule) pairs probed",
        }
    }
}

/// Shared, cross-thread-safe, ALWAYS-ON budget state (module doc above). Built ONCE per
/// `crate::emit::emit_with_precision` call and shared by reference across both composite builders
/// (`crate::preexpand::build_composites_with_mode`, `crate::emit::build_structural_composites`) and
/// every one of their parallel per-root workers -- mirrors [`ProbeBudget`]'s own sharing shape, but
/// every field here is a plain `AtomicUsize` (no `Mutex`, no interior `Cell`), so the whole struct is
/// `Sync` for free and needs no `unsafe` to share across rayon's pool.
pub(crate) struct EnumerationBudget {
    entry_cap: usize,
    entry_count: AtomicUsize,
    probe_cap: usize,
    probe_count: AtomicUsize,
    /// Latch: `0` = untripped; `1` = tripped by [`EnumMeasure::CompositeEntries`]; `2` = tripped by
    /// [`EnumMeasure::PairsProbed`]. Whichever thread's `compare_exchange` wins first "owns" the
    /// recorded reason -- purely a diagnostic tie-break (both measures are checked at both call
    /// sites, so a grammar that crosses both thresholds in the same instant could latch either);
    /// every checker treats "tripped" as one boolean regardless of which measure caused it.
    tripped: AtomicUsize,
}

impl EnumerationBudget {
    /// Production entry point: thresholds from `HC_ENUM_ENTRY_BUDGET`/`HC_ENUM_PROBE_BUDGET` (module
    /// doc), or the documented defaults when unset/unparsable.
    pub(crate) fn from_env() -> Self {
        Self::with_caps(entry_budget_from_env(), probe_budget_from_env())
    }

    /// Explicit-caps constructor -- what tests use (mirroring this crate's existing convention,
    /// `crate::morphotactics::ExploreMode`'s own doc: "tests must construct ... directly, never call
    /// [the env-reading fn], so parallel test threads/processes never race process-global env
    /// state"). Also used internally by [`Self::from_env`].
    pub(crate) fn with_caps(entry_cap: usize, probe_cap: usize) -> Self {
        EnumerationBudget {
            entry_cap,
            entry_count: AtomicUsize::new(0),
            probe_cap,
            probe_count: AtomicUsize::new(0),
            tripped: AtomicUsize::new(0),
        }
    }

    /// A budget that can never trip (`usize::MAX` on both measures) -- for callers/tests that need
    /// an `&EnumerationBudget` to satisfy a function signature but aren't exercising this mechanism
    /// (mirrors passing `None` for the sibling [`ProbeBudget`] parameter). Test-only today (the
    /// `pruning_tests`/`budget_tests` modules); production callers always go through
    /// [`Self::from_env`].
    #[cfg(test)]
    pub(crate) fn unbounded() -> Self {
        Self::with_caps(usize::MAX, usize::MAX)
    }

    /// Record `n` more composite entries (fusion + interdigitation + structural, combined -- module
    /// doc); latches the budget if the running total now exceeds `entry_cap`.
    pub(crate) fn add_entries(&self, n: usize) {
        let total = self.entry_count.fetch_add(n, Ordering::Relaxed) + n;
        if total > self.entry_cap {
            let _ = self.tripped.compare_exchange(
                0,
                EnumMeasure::CompositeEntries as usize + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
    }

    /// Record one more (root, rule) pair probed; latches the budget if the running total now
    /// exceeds `probe_cap`.
    pub(crate) fn tick_probe(&self) {
        let total = self.probe_count.fetch_add(1, Ordering::Relaxed) + 1;
        if total > self.probe_cap {
            let _ = self.tripped.compare_exchange(
                0,
                EnumMeasure::PairsProbed as usize + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
    }

    /// Cheap check every recursive enumeration call makes before doing further work.
    /// `Ordering::Relaxed`: a stale "not yet tripped" read costs at most a little extra harmless
    /// work before the next check catches it (same tolerance this crate's `ProbeBudget` already
    /// accepts) -- this is a fail-fast guard, not a correctness-critical synchronization point.
    pub(crate) fn is_tripped(&self) -> bool {
        self.tripped.load(Ordering::Relaxed) != 0
    }

    /// `Some((measure, value, limit))` once tripped -- exactly the numbers
    /// `crate::analyzer::FomaError::EnumerationBudgetExceeded`'s message reports. `None` while
    /// untripped.
    pub(crate) fn trip_reason(&self) -> Option<(EnumMeasure, usize, usize)> {
        match self.tripped.load(Ordering::Relaxed) {
            0 => None,
            1 => Some((
                EnumMeasure::CompositeEntries,
                self.entry_count.load(Ordering::Relaxed),
                self.entry_cap,
            )),
            2 => Some((
                EnumMeasure::PairsProbed,
                self.probe_count.load(Ordering::Relaxed),
                self.probe_cap,
            )),
            _ => unreachable!("tripped latch is only ever set to 0, 1, or 2"),
        }
    }
}

/// Subset-construction state for one in-progress composite chain (module doc). `free`/`mid` mirror
/// the plan doc's `ChainState` exactly; `template_entry_disabled` is the "carry a bool in the
/// state" option the plan doc names for the partial-root gate (module doc, engine fact 5) -- baked
/// in at [`ChainState::seed`] and carried unchanged by every [`MorphotacticIndex::next_state`]
/// transition (a partial root never enters a template for the chain's entire lifetime, not just
/// the current call).
///
/// `mid` stores template ids as `u16` (not `MorphotacticIndex`'s native `u32` `TemplateId`) --
/// [`MorphotacticIndex::build`] asserts every grammar's template count fits, which every reference/
/// edge-case/Aweti-scale grammar does by a wide margin; this halves this hot-loop state's size
/// versus a `u32`, cheap to justify since the whole point of a subset-construction state is to stay
/// small and `Clone`-cheap across a deep recursion. A `Vec` (not e.g. a `SmallVec`) is used
/// deliberately: this crate has no existing `smallvec` dependency, `mid` is empty or a handful of
/// entries for every grammar this pruning targets (a chain is bounded by `MAX_EXTRA_RULES`/
/// `STRUCT_MAX_EXTRA_RULES` = 3), and adding a new dependency for that shape is not worth it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChainState {
    pub free: Option<u8>,
    pub mid: Vec<(u16, u8)>,
    pub template_entry_disabled: bool,
}

impl ChainState {
    /// Seeds a fresh chain at one root allomorph (`crate::preexpand::process_root_work`/
    /// `crate::emit::build_structural_composites`'s per-entry loop): `free = Some(entry_stratum)`
    /// (engine fact 1 -- the root's own stratum is where the fold starts), `mid` empty (no template
    /// entered yet), and `template_entry_disabled = root_is_partial` (engine fact 5 -- baked in
    /// once, for the chain's whole lifetime, not re-checked per step: C#'s own `root_is_partial`
    /// gate reads the SAME root morpheme's `IsPartial` at every `synth_apply_templates` call along
    /// a chain, so it can never flip mid-chain).
    pub(crate) fn seed(_g: &Grammar, entry_stratum: u8, root_is_partial: bool) -> Self {
        ChainState {
            free: Some(entry_stratum),
            mid: Vec::new(),
            template_entry_disabled: root_is_partial,
        }
    }
}

/// Per-template precomputed facts (module doc); indexed by `TemplateId.0 as usize` in
/// [`MorphotacticIndex::templates`].
struct TemplateInfo {
    /// The stratum (index into `g.strata`) that declared this template (`sd.templates`) -- engine
    /// fact 1's floor check reads this, not `g.templates[t].required_syn_fs`'s owning rule (a
    /// template has no stratum field of its own in `pg_grammar::model`).
    owning_stratum: u8,
    /// `AffixTemplateDef::required_syn_fs` (empty FS if the grammar omitted the attribute) --
    /// engine fact 5's `is_unifiable` gate, re-checked here exactly as `synth_apply_templates`
    /// checks it (stratum.rs:1861).
    required_syn_fs: FsId,
    /// `[slot index k] -> completable(t,k)`: are ALL slots with index > k skippable? A rule
    /// occupying slot k grants a fresh `free` floor at `owning_stratum` exactly when this is true
    /// (engine fact 4: the template only "completes" -- and only a completed template's output
    /// recurses back into the loose mrules cascade -- when every remaining slot could have been
    /// skipped).
    completable: Vec<bool>,
    /// `first_reachable(t)` (plan doc formula: `{k : all slots < k skippable}`) -- entry positions
    /// reachable the moment the template becomes applicable at all (before any slot has fired).
    first_reachable: Vec<u8>,
    /// `[slot index k] -> reachable target slot indices k' > k` (every slot strictly between k and
    /// k' skippable) -- the mid-template advance step (engine fact 3).
    reach_from: Vec<Vec<u8>>,
}

/// Does `mid`'s owning rule have at least one allomorph whose RHS is EXACTLY
/// `[Copy(Input(0)), Copy(Input(1)), .., Copy(Input(lhs.len()-1))]`, in that order, and nothing
/// else? Module doc's STRICT vacuous-rule test (deliberately narrower than `examples/
/// aweti_probe.rs`'s own looser version -- see the module doc for why the strict one is the sound
/// one). `Compounding` is never vacuous (module doc / plan doc: "a Compounding rule in a slot
/// counts as non-vacuous").
fn rule_may_be_vacuous(g: &Grammar, mid: MRuleId) -> bool {
    let allomorphs = match &g.mrules[mid.0 as usize] {
        MorphRuleDef::AffixProcess(def) => &def.allomorphs,
        MorphRuleDef::Realizational(def) => &def.allomorphs,
        MorphRuleDef::Compounding(_) => return false,
    };
    allomorphs.iter().any(|a| {
        let expected: Vec<OutputAction> = (0..a.lhs.len() as u16)
            .map(|i| OutputAction::Copy(PartRef::Input(i)))
            .collect();
        a.rhs == expected
    })
}

/// `slot_skippable(slot) = slot.rules.is_empty() || slot.optional || slot.rules.any(rule_may_be_vacuous)`
/// (module doc) -- used everywhere the engine walk (`synth_slots_generic`, stratum.rs:1373) uses
/// `slot_optional` (`slot.rules.is_empty() || slot.optional`, stratum.rs:1237-1239).
fn slot_skippable(g: &Grammar, slot: &SlotDef) -> bool {
    slot.rules.is_empty() || slot.optional || slot.rules.iter().any(|&r| rule_may_be_vacuous(g, r))
}

/// Every slot index reachable by walking forward from `start`, always including the first stop and
/// continuing only while the just-included slot is itself skippable -- used for both
/// `first_reachable(t)` (`start = 0`) and `reach_from[k]` (`start = k + 1`). Yields `{p : all slots
/// in start..p are skippable}` (inclusive of the first non-skippable slot reached, per the plan
/// doc's `first_reachable(t) = {k : all slots < k skippable}` formula -- vacuously true for `k =
/// start`).
fn reachable_forward(start: usize, skippable: &[bool]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut p = start;
    while p < skippable.len() {
        out.push(p as u8);
        if skippable[p] {
            p += 1;
        } else {
            break;
        }
    }
    out
}

/// Built once per grammar (module doc), shared read-only by both composite builders'
/// `ExtendCtx`/`StructCtx`. Indexes EVERY rule id's engine-legal "sites" (loose-in-stratum-s /
/// slot-in-template-t-at-k) regardless of which builder's own candidate-role filter (`Role::{Infix,
/// Prefix, Suffix}` for `crate::preexpand`; `is_structural_rule`/`probe_would_refuse`-widened for
/// `crate::emit`) selected it -- [`next_state`](MorphotacticIndex::next_state) is queried per
/// specific candidate rule id, so this index must answer for any rule id either builder might ever
/// pass it.
pub(crate) struct MorphotacticIndex {
    /// `rule -> [stratum index]` for every stratum whose `sd.mrules` names this rule (engine fact
    /// 1/2's loose membership).
    rule_loose_sites: FxHashMap<MRuleId, Vec<u8>>,
    /// `rule -> [(template id, slot index)]` for every `(template, slot)` whose `slot.rules` names
    /// this rule (engine fact 3's slot membership).
    rule_slot_sites: FxHashMap<MRuleId, Vec<(u16, u8)>>,
    templates: Vec<TemplateInfo>,
    /// `[stratum index] -> [rule ids loose in that stratum]` -- the reverse of `rule_loose_sites`,
    /// kept for diagnostics/tests (e.g. asserting the free-floor-monotone property over an entire
    /// stratum's loose set) even though `next_state` itself only ever needs the per-rule direction.
    #[allow(dead_code)]
    loose_by_stratum: Vec<Vec<MRuleId>>,
}

impl MorphotacticIndex {
    pub(crate) fn build(g: &Grammar) -> Self {
        debug_assert!(
            g.templates.len() <= u16::MAX as usize,
            "ChainState::mid packs a template id into u16 -- this grammar has {} templates",
            g.templates.len()
        );

        let mut rule_loose_sites: FxHashMap<MRuleId, Vec<u8>> = FxHashMap::default();
        let mut rule_slot_sites: FxHashMap<MRuleId, Vec<(u16, u8)>> = FxHashMap::default();
        let mut loose_by_stratum: Vec<Vec<MRuleId>> = vec![Vec::new(); g.strata.len()];
        let mut template_owner: Vec<u8> = vec![0; g.templates.len()];

        for (s, sd) in g.strata.iter().enumerate() {
            let s = s as u8;
            for &mid in &sd.mrules {
                rule_loose_sites.entry(mid).or_default().push(s);
                loose_by_stratum[s as usize].push(mid);
            }
            for &tid in &sd.templates {
                template_owner[tid.0 as usize] = s;
            }
        }

        for (ti, t) in g.templates.iter().enumerate() {
            debug_assert!(
                t.slots.len() <= u8::MAX as usize,
                "ChainState::mid packs a slot index into u8 -- template {ti} has {} slots",
                t.slots.len()
            );
            for (k, slot) in t.slots.iter().enumerate() {
                for &mid in &slot.rules {
                    rule_slot_sites
                        .entry(mid)
                        .or_default()
                        .push((ti as u16, k as u8));
                }
            }
        }

        let templates: Vec<TemplateInfo> = g
            .templates
            .iter()
            .enumerate()
            .map(|(ti, t)| {
                let skippable: Vec<bool> = t.slots.iter().map(|s| slot_skippable(g, s)).collect();
                let n = skippable.len();
                let completable: Vec<bool> =
                    (0..n).map(|k| (k + 1..n).all(|i| skippable[i])).collect();
                let first_reachable = reachable_forward(0, &skippable);
                let reach_from: Vec<Vec<u8>> = (0..n)
                    .map(|k| reachable_forward(k + 1, &skippable))
                    .collect();
                TemplateInfo {
                    owning_stratum: template_owner[ti],
                    required_syn_fs: t.required_syn_fs,
                    completable,
                    first_reachable,
                    reach_from,
                }
            })
            .collect();

        MorphotacticIndex {
            rule_loose_sites,
            rule_slot_sites,
            templates,
            loose_by_stratum,
        }
    }

    /// Diagnostic/test accessor (module doc's `loose_by_stratum` field comment) -- every rule id
    /// loose in stratum `s`'s own `sd.mrules`.
    #[cfg(test)]
    pub(crate) fn loose_rules_in_stratum(&self, s: u8) -> &[MRuleId] {
        self.loose_by_stratum
            .get(s as usize)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Subset construction (module doc / plan doc "The automaton"): every legal way `rule` can fire
    /// from `state`, merged into ONE resulting state (a rule application is a single atomic event;
    /// the "site" a specific application used is exactly the nondeterminism a subset-construction
    /// state must summarize, not enumerate). Returns `None` iff `rule` has NO contribution at all --
    /// in particular, a rule with no loose site AND no slot site anywhere in the grammar (module
    /// doc: unreachable in engine synthesis, since `sd.mrules` union template slots is the only way
    /// a rule ever enters the stratum machinery, `synth_apply_mrules`/`synth_slots_generic`) always
    /// returns `None` here, for every state.
    ///
    /// - **Loose contribution** (engine fact 1/2): for every stratum `s` in `rule`'s loose sites
    ///   with `state.free = Some(f)` and `s >= f`, contributes a `free` grant of `s`.
    /// - **Template-entry contribution** (engine fact 3/5): for every `(t, k)` in `rule`'s slot
    ///   sites where `k` is in `t`'s `first_reachable`, `state.free = Some(f)` with `t`'s owning
    ///   stratum `>= f`, `!state.template_entry_disabled` (engine fact 5's partial-root gate), and
    ///   `t.required_syn_fs` is empty or unifies with `base_fs` (engine fact 5's own gate, re-run
    ///   here exactly) -- contributes a `mid` grant of `(t, k)`, PLUS a `free` grant of `t`'s
    ///   owning stratum when `t.completable[k]` (engine fact 4).
    /// - **Mid-template advance** (engine fact 3): for every `(t, k)` in `rule`'s slot sites where
    ///   some `(t, k')` is already in `state.mid` and `k` is in `t.reach_from[k']` (every slot
    ///   strictly between `k'` and `k` is skippable) -- same `mid`/`free` grants as above.
    ///
    /// The new state's `free` = the MINIMUM of every `free` grant (`None` if there was none); its
    /// `mid` = the sorted, deduped union of every `mid` grant. `template_entry_disabled` carries
    /// forward unchanged (engine fact 5: permanent for the chain's lifetime).
    pub(crate) fn next_state(
        &self,
        state: &ChainState,
        rule: MRuleId,
        base_fs: &FeatureStruct,
        fs_interner: &Interner<FeatureStruct>,
    ) -> Option<ChainState> {
        let mut free_grants: Vec<u8> = Vec::new();
        let mut mid_grants: Vec<(u16, u8)> = Vec::new();

        if let Some(f) = state.free {
            if let Some(strata) = self.rule_loose_sites.get(&rule) {
                for &s in strata {
                    if s >= f {
                        free_grants.push(s);
                    }
                }
            }
        }

        if let Some(sites) = self.rule_slot_sites.get(&rule) {
            for &(t, k) in sites {
                let tmpl = &self.templates[t as usize];

                // Template-entry contribution: only when the template isn't permanently disabled
                // (partial root, engine fact 5) and the caller is currently in "loose" territory at
                // or below this template's own stratum.
                if !state.template_entry_disabled {
                    if let Some(f) = state.free {
                        if tmpl.owning_stratum >= f && tmpl.first_reachable.contains(&k) {
                            let req = fs_interner.get(tmpl.required_syn_fs);
                            if req.is_empty() || is_unifiable(base_fs, req) {
                                mid_grants.push((t, k));
                                if tmpl.completable[k as usize] {
                                    free_grants.push(tmpl.owning_stratum);
                                }
                            }
                        }
                    }
                }

                // Mid-template advance contribution: some live position in this same template can
                // reach slot k directly (every slot strictly between is skippable).
                for &(mt, mk) in &state.mid {
                    if mt == t && tmpl.reach_from[mk as usize].contains(&k) {
                        mid_grants.push((t, k));
                        if tmpl.completable[k as usize] {
                            free_grants.push(tmpl.owning_stratum);
                        }
                    }
                }
            }
        }

        if free_grants.is_empty() && mid_grants.is_empty() {
            return None;
        }

        mid_grants.sort_unstable();
        mid_grants.dedup();

        Some(ChainState {
            free: free_grants.into_iter().min(),
            mid: mid_grants,
            template_entry_disabled: state.template_entry_disabled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_grammar::model::LexEntryId;

    /// Five-slot template exercising every slot-skippability shape the "New tests" list (plan doc)
    /// needs in ONE fixture: `s0`/`s1` mandatory+non-vacuous (must fire in order), `s2` optional,
    /// `s3` mandatory+VACUOUS (rhs is a bare `CopyFromInput`, no insert), `s4` mandatory+non-vacuous
    /// again. `mrX` deliberately sits in BOTH `s2` and `s4`'s rule lists (state-normalization test:
    /// firing it from a state that can reach both must merge into one sorted/deduped `mid`).
    /// `mrOrphan` is declared but referenced by NEITHER the stratum's `morphologicalRules` attribute
    /// NOR any slot -- the "rule with no sites" case. Mirrors `crates/pg-foma/src/emit.rs`'s own
    /// `#[cfg(test)]` fixture pattern (`edge-cases/deep-optional-affix-nesting/grammar.xml`'s
    /// skeleton), built inline rather than checked in under `machine/conformance` since nothing else
    /// needs this exact shape.
    const FIXTURE_SLOTS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>MtSlots</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cC"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cO"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrA" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>a</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subA">
                <MorphologicalInput><PhoneticSequence id="stemA"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments><CopyFromInput index="stemA" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>A</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrB" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>b</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subB">
                <MorphologicalInput><PhoneticSequence id="stemB"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>b</PhoneticShape></InsertSegments><CopyFromInput index="stemB" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>B</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrC" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>c</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subC">
                <MorphologicalInput><PhoneticSequence id="stemC"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>c</PhoneticShape></InsertSegments><CopyFromInput index="stemC" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>C</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrV" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>vac</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subV">
                <MorphologicalInput><PhoneticSequence id="stemV"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemV" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>V</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrD" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>d</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subD">
                <MorphologicalInput><PhoneticSequence id="stemD"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments><CopyFromInput index="stemD" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>D</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrX" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>x</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subX">
                <MorphologicalInput><PhoneticSequence id="stemX"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="stemX" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>X</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrOrphan" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>orphan</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subOrphan">
                <MorphologicalInput><PhoneticSequence id="stemO"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>o</PhoneticShape></InsertSegments><CopyFromInput index="stemO" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>Orphan</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>T</Name>
            <Slot morphologicalRules="mrA"><Name>s0</Name></Slot>
            <Slot morphologicalRules="mrB"><Name>s1</Name></Slot>
            <Slot optional="true" morphologicalRules="mrC mrX"><Name>s2</Name></Slot>
            <Slot morphologicalRules="mrV"><Name>s3</Name></Slot>
            <Slot morphologicalRules="mrD mrX"><Name>s4</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

    /// Two strata + two categories, exercising the properties `FIXTURE_SLOTS`'s single stratum
    /// cannot: the free-floor monotone-non-decreasing property (a loose rule per stratum),
    /// `AffixTemplateDef::required_syn_fs` mismatch (`TG` requires `posN`; every root here is
    /// `posV`), and the partial-root gate (`eKP` is `partial="true"`; `TP` has no
    /// `requiredPartsOfSpeech` at all, so it is otherwise always enterable).
    const FIXTURE_STRATA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>MtStrata</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
      <PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cL"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrL0">
        <Name>S0</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrL0" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>l0</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subL0">
                <MorphologicalInput><PhoneticSequence id="stemL0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>l</PhoneticShape></InsertSegments><CopyFromInput index="stemL0" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>L0</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrP" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>p</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subP">
                <MorphologicalInput><PhoneticSequence id="stemP"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="stemP" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>P</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate>
            <Name>TP</Name>
            <Slot morphologicalRules="mrP"><Name>sp0</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eKP" partOfSpeech="posV" partial="true">
            <Allomorphs><Allomorph id="aKP"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>KP</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrL1">
        <Name>S1</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrL1" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>l1</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subL1">
                <MorphologicalInput><PhoneticSequence id="stemL1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>m</PhoneticShape></InsertSegments><CopyFromInput index="stemL1" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>L1</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrG" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>g</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subG">
                <MorphologicalInput><PhoneticSequence id="stemG"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>g</PhoneticShape></InsertSegments><CopyFromInput index="stemG" /></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>G</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posN">
            <Name>TG</Name>
            <Slot morphologicalRules="mrG"><Name>sg0</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries></LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

    fn load(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
    }

    /// Mirrors `crates/pg-foma/src/emit.rs`'s own `structural_and_pattern_tests::entry_id_of`
    /// (module doc) -- finds a rule by the XML `id` its owning morpheme's `xml_key` recorded
    /// (`pg-grammar/src/load.rs`: `xml_key: mr.attr("id")...`, the loader's convention for EVERY
    /// morpheme-bearing element, not just lexical entries).
    fn mrule_id_of(g: &Grammar, xml_key: &str) -> MRuleId {
        for (i, r) in g.mrules.iter().enumerate() {
            let m = match r {
                MorphRuleDef::AffixProcess(d) => d.morpheme,
                MorphRuleDef::Realizational(d) => d.morpheme,
                MorphRuleDef::Compounding(_) => continue,
            };
            if g.morphemes[m.0 as usize].xml_key == xml_key {
                return MRuleId(i as u32);
            }
        }
        panic!("no rule with xml id {xml_key:?}");
    }

    fn entry_id_of(g: &Grammar, xml_key: &str) -> LexEntryId {
        LexEntryId(
            g.entries
                .iter()
                .position(|e| g.morphemes[e.morpheme.0 as usize].xml_key == xml_key)
                .unwrap_or_else(|| panic!("no entry with xml id {xml_key:?}")) as u32,
        )
    }

    fn entry_fs<'g>(g: &'g Grammar, xml_key: &str) -> &'g FeatureStruct {
        let e = &g.entries[entry_id_of(g, xml_key).0 as usize];
        g.fs_interner.get(e.syn_fs)
    }

    #[test]
    fn slot_order_is_enforced() {
        let g = load(FIXTURE_SLOTS);
        let mt = MorphotacticIndex::build(&g);
        let fs = entry_fs(&g, "eK");
        let seed = ChainState::seed(&g, 0, false);

        // B (slot 1) is not first-reachable (slot 0 is mandatory/non-vacuous) and `seed.mid` is
        // empty -- no route to it yet.
        let b = mrule_id_of(&g, "mrB");
        assert!(
            mt.next_state(&seed, b, fs, &g.fs_interner).is_none(),
            "slot 1's rule must not be reachable before slot 0 fires"
        );

        // A (slot 0) IS first-reachable.
        let a = mrule_id_of(&g, "mrA");
        let after_a = mt
            .next_state(&seed, a, fs, &g.fs_interner)
            .expect("slot 0's rule must be reachable from the seed state");
        assert_eq!(after_a.mid, vec![(0, 0)]);
        assert_eq!(
            after_a.free, None,
            "slot 0 alone does not complete the template"
        );
    }

    #[test]
    fn mandatory_non_vacuous_slot_blocks_jump() {
        let g = load(FIXTURE_SLOTS);
        let mt = MorphotacticIndex::build(&g);
        let fs = entry_fs(&g, "eK");
        let after_a = ChainState {
            free: None,
            mid: vec![(0, 0)],
            template_entry_disabled: false,
        };
        // C lives in slot 2; slot 1 (mandatory, non-vacuous) sits strictly between and blocks it.
        let c = mrule_id_of(&g, "mrC");
        assert!(
            mt.next_state(&after_a, c, fs, &g.fs_interner).is_none(),
            "must not be able to jump the mandatory non-vacuous slot 1"
        );
    }

    #[test]
    fn optional_slot_is_jumped() {
        let g = load(FIXTURE_SLOTS);
        let mt = MorphotacticIndex::build(&g);
        let fs = entry_fs(&g, "eK");
        let after_b = ChainState {
            free: None,
            mid: vec![(0, 1)],
            template_entry_disabled: false,
        };
        // V lives in slot 3; slot 2 (optional) sits strictly between and must be jumpable.
        let v = mrule_id_of(&g, "mrV");
        let next = mt
            .next_state(&after_b, v, fs, &g.fs_interner)
            .expect("must be able to jump the optional slot 2");
        assert_eq!(next.mid, vec![(0, 3)]);
        assert_eq!(
            next.free, None,
            "slot 4 remains mandatory/non-vacuous -- not yet completable"
        );
    }

    #[test]
    fn mandatory_but_vacuous_slot_is_jumped() {
        let g = load(FIXTURE_SLOTS);
        let mt = MorphotacticIndex::build(&g);
        let fs = entry_fs(&g, "eK");
        let after_c = ChainState {
            free: None,
            mid: vec![(0, 2)],
            template_entry_disabled: false,
        };
        // D lives in slot 4; slot 3 is mandatory but VACUOUS (rhs = bare CopyFromInput) and must be
        // jumpable -- the module doc's whole recall trap.
        let d = mrule_id_of(&g, "mrD");
        let next = mt
            .next_state(&after_c, d, fs, &g.fs_interner)
            .expect("must be able to jump the mandatory-but-vacuous slot 3");
        assert_eq!(next.mid, vec![(0, 4)]);
    }

    #[test]
    fn completion_grants_loose() {
        let g = load(FIXTURE_SLOTS);
        let mt = MorphotacticIndex::build(&g);
        let fs = entry_fs(&g, "eK");
        let after_c = ChainState {
            free: None,
            mid: vec![(0, 2)],
            template_entry_disabled: false,
        };
        let d = mrule_id_of(&g, "mrD");
        let next = mt.next_state(&after_c, d, fs, &g.fs_interner).unwrap();
        // Slot 4 is the template's last slot -> completable[4] = true -> firing D grants a fresh
        // `free` floor at the template's own owning stratum (engine fact 4).
        assert_eq!(next.free, Some(0));
    }

    #[test]
    fn free_floor_is_monotone_non_decreasing() {
        let g = load(FIXTURE_STRATA);
        let mt = MorphotacticIndex::build(&g);
        let fs = entry_fs(&g, "eK");
        let l0 = mrule_id_of(&g, "mrL0");
        let l1 = mrule_id_of(&g, "mrL1");

        let seed = ChainState::seed(&g, 0, false);
        let after_l0 = mt.next_state(&seed, l0, fs, &g.fs_interner).unwrap();
        assert_eq!(after_l0.free, Some(0));

        let after_l1 = mt.next_state(&after_l0, l1, fs, &g.fs_interner).unwrap();
        assert_eq!(
            after_l1.free,
            Some(1),
            "stratum 1's loose rule advances the floor"
        );

        // Once the floor has advanced to stratum 1, stratum 0's loose rule must no longer be legal
        // (the floor can only move forward, never back).
        assert!(
            mt.next_state(&after_l1, l0, fs, &g.fs_interner).is_none(),
            "the free floor must never decrease"
        );
    }

    #[test]
    fn partial_root_never_enters_template() {
        let g = load(FIXTURE_STRATA);
        let mt = MorphotacticIndex::build(&g);
        let fs = entry_fs(&g, "eK");
        let p = mrule_id_of(&g, "mrP"); // TP has no required_syn_fs -- always otherwise enterable.

        let seed_ok = ChainState::seed(&g, 0, false);
        assert!(
            mt.next_state(&seed_ok, p, fs, &g.fs_interner).is_some(),
            "a non-partial root must be able to enter TP"
        );

        let seed_partial = ChainState::seed(&g, 0, true);
        assert!(
            mt.next_state(&seed_partial, p, fs, &g.fs_interner)
                .is_none(),
            "a partial root must never enter any template"
        );
    }

    #[test]
    fn template_required_syn_fs_gate_is_honored() {
        let g = load(FIXTURE_STRATA);
        let mt = MorphotacticIndex::build(&g);
        let fs = entry_fs(&g, "eK"); // posV
        let gr = mrule_id_of(&g, "mrG"); // TG requires posN -- must never unify with a posV root.
        let at_stratum1 = ChainState {
            free: Some(1),
            mid: Vec::new(),
            template_entry_disabled: false,
        };
        assert!(
            mt.next_state(&at_stratum1, gr, fs, &g.fs_interner)
                .is_none(),
            "a posV word must not enter a template requiring posN"
        );
    }

    #[test]
    fn rule_with_no_sites_returns_none() {
        let g = load(FIXTURE_SLOTS);
        let mt = MorphotacticIndex::build(&g);
        let fs = entry_fs(&g, "eK");
        let orphan = mrule_id_of(&g, "mrOrphan");
        let seed = ChainState::seed(&g, 0, false);
        assert!(mt.next_state(&seed, orphan, fs, &g.fs_interner).is_none());
        // Also true from an in-template state -- an orphan rule has no site anywhere, period.
        let mid_state = ChainState {
            free: None,
            mid: vec![(0, 1)],
            template_entry_disabled: false,
        };
        assert!(mt
            .next_state(&mid_state, orphan, fs, &g.fs_interner)
            .is_none());
    }

    #[test]
    fn state_normalization_is_deterministic() {
        let g = load(FIXTURE_SLOTS);
        let mt = MorphotacticIndex::build(&g);
        let fs = entry_fs(&g, "eK");
        let after_b = ChainState {
            free: None,
            mid: vec![(0, 1)],
            template_entry_disabled: false,
        };
        // X sits in BOTH slot 2 and slot 4's rule lists; both are reachable from slot 1 (slot 2
        // directly, slot 4 by jumping the optional slot 2 and the vacuous-mandatory slot 3) -- one
        // rule firing must merge both into a single, sorted/deduped `mid`.
        let x = mrule_id_of(&g, "mrX");
        let next = mt.next_state(&after_b, x, fs, &g.fs_interner).unwrap();
        assert_eq!(next.mid, vec![(0, 2), (0, 4)]);
        assert_eq!(
            next.free,
            Some(0),
            "slot 4's completion still grants a fresh floor"
        );

        // Calling again from the same input must be byte-for-byte identical (pure function).
        let next2 = mt.next_state(&after_b, x, fs, &g.fs_interner).unwrap();
        assert_eq!(next, next2);
    }

    #[test]
    fn loose_rules_in_stratum_reports_membership() {
        let g = load(FIXTURE_STRATA);
        let mt = MorphotacticIndex::build(&g);
        let l0 = mrule_id_of(&g, "mrL0");
        let l1 = mrule_id_of(&g, "mrL1");
        assert_eq!(mt.loose_rules_in_stratum(0), &[l0]);
        assert_eq!(mt.loose_rules_in_stratum(1), &[l1]);
    }
}
