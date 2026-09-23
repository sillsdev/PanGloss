//! Morphotactic pruning for `crate::preexpand::extend`/`crate::emit::struct_extend` (the Aweti
//! scale fix): both composite chain builders used to chain **every** candidate rule onto every
//! root at every depth, gated only by
//! the cheap `required_syn_fs` unifiability pre-filter -- exploring rule orders the engine can
//! never actually produce in synthesis. This module builds, once per grammar, a subset-construction
//! automaton over the engine's own morphotactics (`pg-rules/src/stratum.rs`: `synth_apply_mrules`,
//! `synth_apply_templates`, `synth_slots_generic`, `slot_optional`) and exposes a single
//! `MorphotacticIndex::next_state` query the two builders consult immediately before recursing
//! on a candidate rule -- restricting the flat recursion to a strict subset of engine-legal chains,
//! never widening it (recall-preserving by construction: pruned exploration is always a subset of
//! flat exploration).
//!
//! ## The engine facts this automaton mirrors
//! 1. Strata fold in document order 0..n (`pg-parse/src/morpher.rs::synthesis_pipeline_selected`);
//!    a stratum applies only to words whose root stratum is not deeper -- so a chain's rules come
//!    from a **non-decreasing stratum sequence** starting at the root's own stratum. [`ChainState::
//!    free`] is that floor: `Some(s)` means "loose rules and template entries at strata >= s are
//!    legal right now"; `None` means "mid-template only" (see point 3).
//! 2. Loose rules (`sd.mrules`) run in a Linear or Unordered cascade (`synth_apply_mrules`,
//!    stratum.rs:1710-1712). v1 over-approximates Linear as Unordered (any order) -- sound,
//!    simpler, and deliberately out of scope for this v1 to distinguish.
//! 3. Template slot rules apply **only inside a template application, in ascending slot order**
//!    (`synth_slots_generic`, stratum.rs:1339-1388): a non-optional slot that produces nothing
//!    terminates the walk (`if !slot_optional(slot) { return; }`, stratum.rs:1373-1379). The
//!    template completes early (`out.entry(input)...`, stratum.rs:1386-1387) only when every
//!    remaining slot is optional. `ChainState::mid` tracks live `(template, slot)` positions;
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
//!    `ChainState::seed` bakes this into `template_entry_disabled`, carried unchanged by every
//!    `MorphotacticIndex::next_state` transition.
//!
//! ## Recall trap: surface-vacuous rules in mandatory slots (plan doc "Recall trap")
//! A realizational rule whose allomorph RHS is EXACTLY `[Copy(0), Copy(1), .., Copy(n-1)]` (every
//! LHS part copied, in order, and NOTHING else -- no `InsertSegments`, no `Modify`, no
//! `InsertContext`) adds no surface material at all; the engine still applies it in a mandatory
//! slot, so a composite chain that only ever chains Prefix/Suffix/Infix candidates must be allowed
//! to jump over such a slot and still match the engine's own surface. `rule_may_be_vacuous` is
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
/// resolves this from `HC_PREEXPAND_FLAT` exactly once (via `explore_mode_from_env`) and passes it
/// down; unit/gate tests construct it directly, never through the env var, so parallel test
/// processes never race process-global env state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExploreMode {
    /// Consult `MorphotacticIndex::next_state` before every recursive step -- the production
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
/// construct `ExploreMode` directly, never call this, so parallel test threads/processes never
/// race process-global env state (plan doc "Wiring").
pub fn explore_mode_from_env() -> ExploreMode {
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
/// `ProbeBudget` at all in that case.
pub fn probe_cap_from_env() -> Option<usize> {
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
pub struct ProbeBudget<'a> {
    pub cap: usize,
    pub counter: &'a AtomicUsize,
}

impl<'a> ProbeBudget<'a> {
    /// Record one probe (module doc: "each probe increments"); panics with the cap and the total
    /// so far if this exceeds it. `Ordering::Relaxed` is enough -- this is a measurement-only abort
    /// guard, not a synchronization point for any other shared state.
    pub fn tick(&self) {
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

/// Subset-construction state for one in-progress composite chain (module doc). `free`/`mid` mirror
/// the plan doc's `ChainState` exactly; `template_entry_disabled` is the "carry a bool in the
/// state" option the plan doc names for the partial-root gate (module doc, engine fact 5) -- baked
/// in at `ChainState::seed` and carried unchanged by every `MorphotacticIndex::next_state`
/// transition (a partial root never enters a template for the chain's entire lifetime, not just
/// the current call).
///
/// `mid` stores template ids as `u16` (not `MorphotacticIndex`'s native `u32` `TemplateId`) --
/// `MorphotacticIndex::build` asserts every grammar's template count fits, which every reference/
/// edge-case/Aweti-scale grammar does by a wide margin; this halves this hot-loop state's size
/// versus a `u32`, cheap to justify since the whole point of a subset-construction state is to stay
/// small and `Clone`-cheap across a deep recursion. A `Vec` (not e.g. a `SmallVec`) is used
/// deliberately: this crate has no existing `smallvec` dependency, `mid` is empty or a handful of
/// entries for every grammar this pruning targets (normally one counter per authored rule), and
/// adding a new dependency for that shape is not worth it.
/// `applications` is indexed by the grammar's stable rule ordinal so it can enforce authored
/// bounds without conflating distinct rules that happen to share a site.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChainState {
    pub free: Option<u8>,
    pub mid: Vec<(u16, u8)>,
    pub template_entry_disabled: bool,
    /// Applications by stable `Grammar::mrules` ordinal.
    pub applications: Vec<u16>,
}

impl ChainState {
    /// Seeds a fresh chain at one root allomorph (`crate::preexpand::process_root_work`/
    /// `crate::emit::build_structural_composites`'s per-entry loop): `free = Some(entry_stratum)`
    /// (engine fact 1 -- the root's own stratum is where the fold starts), `mid` empty (no template
    /// entered yet), and `template_entry_disabled = root_is_partial` (engine fact 5 -- baked in
    /// once, for the chain's whole lifetime, not re-checked per step: C#'s own `root_is_partial`
    /// gate reads the SAME root morpheme's `IsPartial` at every `synth_apply_templates` call along
    /// a chain, so it can never flip mid-chain).
    pub fn seed(g: &Grammar, entry_stratum: u8, root_is_partial: bool) -> Self {
        ChainState {
            free: Some(entry_stratum),
            mid: Vec::new(),
            template_entry_disabled: root_is_partial,
            applications: vec![0; g.mrules.len()],
        }
    }
}

/// Per-template precomputed facts, indexed by `TemplateId.0 as usize` in `MorphotacticIndex::templates`.
struct TemplateInfo {
    /// The stratum that declared this template; a template has no stratum field of its own in `pg_grammar::model`, so this is precomputed rather than read off `required_syn_fs`'s owning rule.
    owning_stratum: u8,
    /// `AffixTemplateDef::required_syn_fs`, re-checked here exactly as `synth_apply_templates` checks it (stratum.rs:1861).
    required_syn_fs: FsId,
    /// `[slot k] -> completable(t,k)`: are all slots with index > k skippable? True exactly when firing slot k grants a fresh `free` floor at `owning_stratum`.
    completable: Vec<bool>,
    /// `first_reachable(t)`: entry positions reachable the moment the template becomes applicable, before any slot has fired.
    first_reachable: Vec<u8>,
    /// `[slot k] -> reachable target slot indices k' > k` (every slot strictly between k and k' skippable): the mid-template advance step.
    reach_from: Vec<Vec<u8>>,
}

/// Does `mid`'s owning rule have at least one allomorph whose RHS is exactly a straight copy of every input part, in order, with nothing else? `Compounding` is never vacuous.
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

/// `slot.rules.is_empty() || slot.optional || slot.rules.any(rule_may_be_vacuous)`, mirroring the engine walk's own `slot_optional` check (stratum.rs:1237-1239) but including vacuous rules too.
fn slot_skippable(g: &Grammar, slot: &SlotDef) -> bool {
    slot.rules.is_empty() || slot.optional || slot.rules.iter().any(|&r| rule_may_be_vacuous(g, r))
}

/// Every slot index reachable by walking forward from `start`, including the first stop and continuing only while the just-included slot is itself skippable; shared by `first_reachable` (`start = 0`) and `reach_from[k]` (`start = k + 1`).
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
pub struct MorphotacticIndex {
    /// `rule -> [stratum index]` for every stratum whose `sd.mrules` names this rule.
    rule_loose_sites: FxHashMap<MRuleId, Vec<u8>>,
    /// `rule -> [(template id, slot index)]` for every `(template, slot)` whose `slot.rules` names this rule.
    rule_slot_sites: FxHashMap<MRuleId, Vec<(u16, u8)>>,
    templates: Vec<TemplateInfo>,
    /// Authored `multipleApplication`, using the model's default of one.
    rule_application_bounds: Vec<u16>,
}

impl MorphotacticIndex {
    fn next_applications(&self, state: &ChainState, rule: MRuleId) -> Option<Vec<u16>> {
        let rule_index = rule.0 as usize;
        let current = *state.applications.get(rule_index)?;
        let bound = *self.rule_application_bounds.get(rule_index)?;
        if current >= bound {
            return None;
        }
        let mut applications = state.applications.clone();
        applications[rule_index] = current + 1;
        Some(applications)
    }

    /// Advances only the authored application counter. The diagnostic flat explorer deliberately
    /// ignores stratum/template sites, but it must still terminate at the grammar's real bounds.
    pub fn next_state_unpruned(&self, state: &ChainState, rule: MRuleId) -> Option<ChainState> {
        let applications = self.next_applications(state, rule)?;
        Some(ChainState {
            free: state.free,
            mid: state.mid.clone(),
            template_entry_disabled: state.template_entry_disabled,
            applications,
        })
    }

    pub fn build(g: &Grammar) -> Self {
        debug_assert!(
            g.templates.len() <= u16::MAX as usize,
            "ChainState::mid packs a template id into u16 -- this grammar has {} templates",
            g.templates.len()
        );

        let mut rule_loose_sites: FxHashMap<MRuleId, Vec<u8>> = FxHashMap::default();
        let mut rule_slot_sites: FxHashMap<MRuleId, Vec<(u16, u8)>> = FxHashMap::default();
        let mut template_owner: Vec<u8> = vec![0; g.templates.len()];

        for (s, sd) in g.strata.iter().enumerate() {
            let s = s as u8;
            for &mid in &sd.mrules {
                rule_loose_sites.entry(mid).or_default().push(s);
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
            rule_application_bounds: g.mrules.iter().map(MorphRuleDef::max_apps).collect(),
        }
    }

    /// Every `(template, slot index)` site whose own `slot.rules` names `rule` -- the site half of
    /// the relation [`next_state`](MorphotacticIndex::next_state) consults, exposed so a caller
    /// that needs the relation itself reads this index rather than rebuilding it from `g.templates`.
    pub(crate) fn slot_sites_of(&self, rule: MRuleId) -> &[(u16, u8)] {
        self.rule_slot_sites
            .get(&rule)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The stratum that declared `template` in its own `sd.templates`.
    pub(crate) fn template_stratum(&self, template: u16) -> Option<u8> {
        self.templates
            .get(template as usize)
            .map(|info| info.owning_stratum)
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
    pub fn next_state(
        &self,
        state: &ChainState,
        rule: MRuleId,
        base_fs: &FeatureStruct,
        fs_interner: &Interner<FeatureStruct>,
    ) -> Option<ChainState> {
        self.next_state_impl(state, rule, Some((base_fs, fs_interner)))
    }

    /// Site-aware transition used by finite-state reachability analysis.
    ///
    /// This retains the engine's loose-stratum/template-slot relation and authored application
    /// bounds, but deliberately ignores feature-structure compatibility. Reachability uses this
    /// conservative projection because it asks whether a dirty rule might be reachable from a
    /// clean ordinary successor; applying an FS filter here could incorrectly prove that the tail
    /// is clean when a later state has a compatible feature structure.
    pub fn next_state_fs_insensitive(
        &self,
        state: &ChainState,
        rule: MRuleId,
    ) -> Option<ChainState> {
        self.next_state_impl(state, rule, None)
    }

    fn next_state_impl(
        &self,
        state: &ChainState,
        rule: MRuleId,
        fs: Option<(&FeatureStruct, &Interner<FeatureStruct>)>,
    ) -> Option<ChainState> {
        let applications = self.next_applications(state, rule)?;

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

                // Template-entry contribution: only when not permanently disabled (partial root) and currently loose at or below this template's stratum.
                if !state.template_entry_disabled {
                    if let Some(f) = state.free {
                        if tmpl.owning_stratum >= f && tmpl.first_reachable.contains(&k) {
                            let feature_compatible = fs.is_none_or(|(base_fs, fs_interner)| {
                                let req = fs_interner.get(tmpl.required_syn_fs);
                                req.is_empty() || is_unifiable(base_fs, req)
                            });
                            if feature_compatible {
                                mid_grants.push((t, k));
                                if tmpl.completable[k as usize] {
                                    free_grants.push(tmpl.owning_stratum);
                                }
                            }
                        }
                    }
                }

                // Mid-template advance: some live position in this template can reach slot k directly (every slot strictly between is skippable).
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
            applications,
        })
    }
}

#[cfg(test)]
mod tests;
