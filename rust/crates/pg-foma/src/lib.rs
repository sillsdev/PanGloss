//! Phase P0 viability spike (docs/fst-plan/foma-fst-plan.md §P0, gate F0), P1 stage 1 (emitter core
//! + Sena, gate F1), P1 stage 2 (junction-aware phonology + Indonesian, gate F1), P1d (Amharic
//!   capability stage — rule-application pre-expansion + boundary-fusion composites), and P2
//!   (propose→confirm composite, gate F2).
//!
//! - [`tags`]: the `<R:nnnn>`/`<M:nnnn>` tag codec (D2) — escaped lexc spellings, decoded literal
//!   text, an `apply_up`-output decoder, and the `Candidate` split for compound (multi-root) paths.
//! - [`emit`]: `Grammar -> lexc source` (D3) — see that module's doc for the full design.
//! - [`junctions`]: [`junctions::PhonologyProbe`], the pre-probed surface-variant/deletion-junction
//!   machinery `emit` drives for a grammar with real phonological rules (stage 2) — a `None`-safe
//!   no-op for a grammar without any (stage 1's Sena stays byte-identical).
//! - [`morphotactics`] (crate-internal, `docs/fst-plan/morphotactic-composite-pruning.md`): the
//!   `MorphotacticIndex`/`ChainState` subset-construction automaton over the engine's own
//!   `pg-rules/src/stratum.rs` morphotactics, shared by `preexpand::extend`/`emit::struct_extend`
//!   to prune their composite-chain recursion to engine-legal rule adjacencies (the Aweti scale
//!   fix — see that module's doc for the full design).
//! - [`preexpand`] (P1d, crate-internal): rule-application pre-expansion (interdigitation —
//!   `Role::Infix` rules applied to each root via the engine's own `pg_rules::morph::synthesize`)
//!   and boundary-fusion composite probing (Ge'ez glyph coalescence), emitted as multi-tag
//!   composite entries in the engine's own morph order and wired into `emit`'s shared `Composites`
//!   lexicon — see `tests/f3_amharic_gate.rs` (Amharic recall 100%, asserted).
//! - [`analyzer`]: `FomaProposer`, the thin `emit + compile + apply-up` wrapper.
//! - [`confirm`] (P2): a fresh port of `hc-hybrid/src/replay.rs`'s confirm half — `MorphemeOwner`,
//!   `build_morpheme_owners`, and `confirm_all` (D4's multiplicity recovery: every matching analysis
//!   in the pinned `parse_word_selected` outcome, not just the first).
//! - [`peel`] (P2, D6): a fresh port of `hc-hybrid/src/proposers.rs::ReduplicationProposer`, its
//!   recursion target swapped to the foma proposer (`ReduplicationPeeler::peel_candidates`).
//! - [`plan`] (`openspec/changes/reify-compilation-plans` Step 1, design.md D1): the reified,
//!   content-addressed compilation-`Plan` data type -- a closed node-kind enum (`Leaf`, `Compose`,
//!   `Union`, `Gate`, `Replace`) plus the interning arena that makes identical subtrees dedup.
//!   Built by [`enumerate`], interpreted into real `Fsm`s by [`build`]; does not rewire
//!   [`emit`]/[`preexpand`]'s own seams.
//! - [`composite`] (P2): `FomaAnalyzer`, the public propose→confirm product API — `analyze_word`
//!   mirrors `pg_parse::ParseOutcome`'s `analyses`/`structured` shape, plus diagnostics.
//! - [`precision`] (P6 step 1, `docs/superpowers/specs/2026-07-15-fst-precision-knob-design.md`):
//!   the FST precision knob's `ConstraintCatalog`/`PrecisionAction`/`PrecisionConfig` for the
//!   GATE-CONSTRAINT ENVIRONMENT family, plus the `AllFlags` preset's flag-emission runtime
//!   (`crate::emit::emit_with_precision` is the opt-in entry point; `crate::emit::emit` always
//!   passes `PrecisionConfig::Strip`, byte-identical to before this step).
//!
//! `tests/f0_viability.rs` remains the P0 gate's record (proves the pure-Rust `foma` crate,
//! crates.io v0.1.1, github.com/divvun/foma-rs, compiles and behaves correctly on Windows and
//! wasm32); `tests/f1_sena_gate.rs` is the P1 stage-1 gate (emit+compile Sena, recall vs. the full
//! engine, `mbali`, overgeneration sanity); `tests/f2_indonesian_gate.rs` is the P1 stage-2 gate
//! (emit+compile Indonesian, recall minus the reduplication exclusion list, junction spot-checks,
//! overgeneration sanity, plus a Sena regression re-run); `tests/f3_amharic_gate.rs` is the P1d
//! gate (Amharic: emit+compile with the infix items gone from `uncovered`, recall asserted 100%,
//! end-to-end multiset parity vs the full engine, overgeneration sanity);
//! `tests/f4_composite_gate.rs` is the P2 gate (over-generation pruning, `mbali` multiplicity,
//! Indonesian redup round-trip, empty-on-miss, and a mini-parity smoke pass).
//!
//! ## `pg-parse` is now a NORMAL dependency (P2)
//! Through P1, this crate's lib target never depended on `pg-parse` (the verifier engine) — only
//! `pg-grammar`/`pg-featstruct` and (stage 2) `pg-rules`/`pg-shape` for [`junctions`]'s probe
//! machinery. P2's whole point is *confirm* — pinning `pg_parse::Morpher::parse_word_selected` to a
//! candidate's root(s)/rules (plan §2) — so [`confirm`] and [`composite`] necessarily depend on
//! `pg_parse::{Morpher, ParseOptions, WordAnalysis}` for real, not just as a dev-dependency oracle.
//! This is not a new wasm32 risk: `pg-wasm` already links `pg-parse` directly (it IS the verifier
//! engine `pg-wasm`'s own demo runs today), so this crate depending on it too adds no new transitive
//! dependency to that build — see `tests/f2_indonesian_gate.rs`'s wasm32 `cargo check`, still green
//! with `pg-parse` promoted, and this crate's own wasm32 check in CI/`README`.
#![forbid(unsafe_code)]

#[cfg(test)]
mod test_support;

pub mod analyzer;
/// Step 3a of `openspec/changes/reify-compilation-plans` (design.md D3): [`build::
/// build_controllable`], a [`plan::Plan`] interpreter for the controllable subtree (the `Gate`
/// node and its per-group `Compose{LexiconFragment, Replace}` children [`enumerate::
/// enumerate_default`] emits) into a real, composed [`foma::types::Fsm`] -- proven equivalent to
/// [`gate::compile_gated_grammar_with_budget`]'s own direct-compile output by an apply-based test.
/// Composite/structural-composite markers stay out of scope (that path's artifact type is a lexc
/// `String`, not this module's `Fsm`); see that module's own doc for the full scope and the
/// per-group-Replace-variance obstacle this step surfaced.
pub mod build;
/// Step 1 of `openspec/changes/add-capability-characteristics-check` (design.md D1/D2/D3): the
/// `CharacteristicsProfile` projection, the `CapabilityPredicate` trait + `PredicateVerdict`, the
/// exhaustive default-deny `characterize`, and the `simultaneous.subrule-overlap` predicate.
/// Gates SELECTION, not compilation: `compose_envelope_for_strategy` decides what [`selection`]
/// may offer, while the compile passes themselves are untouched.
pub mod capability;
/// A production-shaped convenience entry point into [`capability::compose_envelope`]:
/// [`capability_entry::evaluate_capability`] assembles `characterize` + `enumerate_default`'s
/// inputs the way [`emit::emit_with_budget`]'s own setup does (`surface_table` + [`replace::
/// SegAlphabet`], `PhonologyProbe::new`, stratum-cascade `prules_in_order`) and returns the
/// resulting `CompileDecision` from one call. Check-only -- see that
/// module's own doc.
pub mod capability_entry;
/// Composition-path budget guards:
/// [`morphotactics::EnumerationBudget`]'s sibling for the composition path ([`replace`],
/// [`gate`], [`uflexc`]) -- size/count caps plus an opt-in wall-clock deadline for every
/// compose/union/minimize call on that path. See that module's own doc for the full design.
pub mod compose_budget;
pub mod composite;
pub mod confirm;
/// Task 5.1 of `openspec/changes/add-capability-characteristics-check` (ADR 0001): the
/// conformance-coverage cross-check — [`conformance_coverage::construct_ids_for`] (the
/// [`capability::CharacteristicKind`] → `machine/conformance/constructs.txt` identifier mapping,
/// the contract) and [`conformance_coverage::supported_coverage_report`]/[`conformance_coverage::
/// supported_uncovered`] (the pure cross-check over a caller-supplied passing-construct set).
/// **NON-BLOCKING PREVIEW**: this module reports gaps, it does not enforce `gaps.is_empty()` —
/// see its own doc for the mapping, today's reported gaps, and the deferred build-breaking end
/// state. The fixture-replay glue that builds the "passing" set lives in
/// `tests/conformance_coverage_gate.rs` (dev-dependency-only).
pub mod conformance_coverage;
/// `openspec/changes/define-grammar-coverage-contract` (demoted to an evidence role — `docs/adr/
/// 0001-honest-capability-boundary.md`; `openspec/changes/STAGING.md` Stage 0B): the one-time,
/// audited coverage LEDGER over the frozen `pg-grammar/src/model.rs` construct set —
/// [`coverage_ledger::LedgerRow`]/[`coverage_ledger::CoverageLedger`]/[`coverage_ledger::
/// build_ledger`] (a consolidated, serializable view over [`capability::CharacteristicKind`]'s
/// existing disposition/predicate/conformance-mapping facts) plus [`coverage_ledger::
/// containment_evidence_for`] (the curated, hand-reviewed proposer-to-confirm containment-test
/// citation table). **Evidence, not a gate** — nothing here is consulted by any compile path; see
/// that module's own doc for the full per-task assessment of what already existed vs. what this
/// module adds.
pub mod coverage_ledger;
/// E2 feasibility probe (not mainline; see that module's doc): does token-space Infix-rule
/// splicing (Amharic root-and-pattern interdigitation) reach 100% recall composed with
/// [`replace`]'s rule cascade? Standalone, additive, same status as [`replace`]/[`uflexc`].
pub mod e2_infix_probe;
pub mod emit;
/// `enumerate_default`, which
/// builds today's compilation topology for a `Grammar` as a single reified [`plan::Plan`], verified
/// structurally against the real `preexpand::should_run`/`emit::probe_would_refuse`/`gate::
/// partition_entries` seams. `crate::emit::plan_topology_decisions` flips
/// `emit.rs`'s own compile path to DERIVE its composite-emission/structural-composite topology
/// decisions from a `Plan` this function builds, rather than re-deriving `should_run`/
/// `probe_would_refuse`/`structural_candidate_rules` a second, independent time -- still does not
/// build/execute a `Plan` into real FSTs itself (that stays data-only). `gate::
/// partition_entries` belongs
/// to `gate.rs`'s own, separate compile entry point; see that module's own doc for full scope and
/// the judgment calls it surfaces.
pub mod enumerate;
/// The validated
/// [`executable_candidate::ExecutableCandidate`] and the portable, round-trippable
/// [`executable_candidate::PortablePlan`] document it binds. Its sole constructor is
/// [`recipe_registry::Registry::executable_candidate`], enforced by a
/// [`recipe_registry::RegistryAuthority`] no other module can produce. Builds and verifies portable
/// data only -- see that module's own doc for
/// why the artifact identity is domain-framed SHA-256 rather than the plan's FNV root, and for the
/// typed refusals that replace an implicit fallback.
pub mod executable_candidate;
/// Static MPR/POS subrule gating, a sibling of [`replace`]/[`uflexc`]. See that module's doc for
/// the design and why it is a flag-free static partition rather than a flag-diacritics encoding.
pub mod gate;
/// [`grammar_semantics::
/// GrammarSemantics`], the ONE immutable typed owner of this crate's grammar-derived semantic
/// facts. Capability ([`capability`]/[`capability_entry`]/[`preflight`]/[`selection`]), registry
/// applicability ([`recipe_registry::Applicability`]), recipe-space accounting
/// ([`recipe_space::GrammarFacts`]) and the phonology existence gate ([`junctions::PhonologyProbe`])
/// are projections over it rather than four independent grammar walks. See that module's own
/// doc for what it deliberately does NOT own (`conformance_coverage`'s independent structural
/// witnesses, and the compile paths themselves) and for the declared-vs-cascade phonology split.
pub mod grammar_semantics;
/// The FST
/// compilation-health finding schema -- [`health::Severity`]/[`health::severity_for_size_bytes`]
/// (exact decimal-byte size bands), the immutable [`health::FindingCode`] `PGFdddd` registry,
/// [`health::HealthFinding`]/[`health::HealthReport`], and canonical JSON. Health is REPORTED
/// about a compile, never consulted during one; [`health_evaluator`] produces the findings.
pub mod health;
/// The real health EVALUATOR --
/// [`health_evaluator::evaluate_health`] turns available compile measurements (final FST payload
/// size, [`emit::EmitReport`], [`compose_budget::ComposeError`], per-word
/// [`health_evaluator::ApplyBudgetTrip`]s) into [`health::HealthFinding`]s + a [`health::
/// HealthReport`], consuming [`health`]'s schema without recomputing any measurement itself. See
/// that module's own doc for the exact input -> finding mapping and which finding kinds stay
/// unpopulated pending real compile-profile instrumentation.
pub mod health_evaluator;
pub mod junctions;
/// The shared
/// pattern/environment → FST lowering seam -- [`lower::lower_span`] lowers one subrule's
/// `left_env · lhs_focus · right_env` triple into foma acceptors, [`lower::spans_overlap`] tests
/// two such spans for a non-empty intersection. [`lower::Slot`]/[`lower::pattern_slots`]/
/// [`lower::resolve_alpha_tuples`]/[`lower::render_slots`]/[`lower::AlphaAssignment`]/
/// [`lower::TupleReport`] are defined HERE, the canonical pattern-lowering vocabulary, with
/// [`replace`] re-exporting them at their old paths for its own rewrite-rule compilation and every
/// other existing caller; see that module's own doc for full scope, what stayed in
/// [`replace`] (`SegAlphabet`, `owning_table`) and why, and the judgment calls it surfaces.
pub mod lower;
/// The ONE derivation of a
/// [`recipe_mechanism::MechanismGraph`], taking [`grammar_semantics::GrammarSemantics`] and no
/// `&Grammar` at all. Builds and verifies data only -- see that module's own doc for why the
/// signature is the enforcement.
pub mod mechanism_provider;
pub(crate) mod morphotactics;
/// **The speed question's cheap first-pass filter, split off from the accuracy question.** A purely
/// STATIC structural inspection of a finished [`foma::types::Fsm`] -- cycles and self-loops in the
/// continuation graph, zero-width (input-epsilon) cycles specifically, and the per-state
/// branching/ambiguity distribution -- computed without applying a single word. Deliberately says
/// NOTHING about size as a preference: on the private `sena` grammar the plan-composed net is 50x
/// SMALLER and ~1300x slower to apply than the hand-spun one, so every size metric picks the wrong
/// candidate. Read that module's own doc for the mechanism, and for the hard scope rule: it is a
/// first-pass filter and a regression tripwire that can never suppress a proposal, and no `Score`
/// field, ranking key, eligibility predicate or certification verdict consults it -- grep for
/// callers of its public functions to confirm.
pub mod net_shape;
/// The
/// differential-correctness oracle -- [`oracle::differential_oracle`] builds two [`plan::Plan`]s
/// via [`build::build_controllable`] and compares their `apply_up` result sets, reporting the
/// shortest disagreeing word plus symmetric difference on mismatch; [`oracle::permute_gate_groups`]
/// is the second same-relation topology generator this module's own tests diff against. Cheap,
/// always-on tier only (an expensive exact-equivalence stretch tier and any real confirm-engine
/// integration are explicitly out of scope; see that module's own doc).
pub mod oracle;
/// The recipe parity RELATION, stated once: deduplicated [`pg_parse::identity::AnalysisIdentity`]
/// set equality per word occurrence, plus the typed faults that make a candidate non-selectable
/// without ever being reported as disagreement. [`recipe_runtime::certify_word`] is the only
/// production consumer; it lives here rather than there so the relation can be read, tested, and
/// changed without reading the evaluator that applies it.
pub mod parity;
pub mod peel;
/// The content-addressed compilation-`Plan` data type -- compilation topology as enumerable data,
/// so a compiler can be composed rather than hand-written. Built by [`enumerate`], interpreted
/// into real `Fsm`s by [`build`].
pub mod plan;
/// `openspec/changes/visualize-compilation-plan`: renders a [`plan::Plan`] as a versioned JSON
/// document ([`plan_diagram::PlanDocument`], schema-versioned like [`coverage_ledger`]/[`health`])
/// and, from that document, a mermaid diagram ([`plan_diagram::render_mermaid`]) labelled by the
/// linguistic work each node performs and marked with its REAL [`capability`] verdict (never
/// inferred from a node's mere presence). Read-only over `plan`/`enumerate`/`capability`/
/// `plan_interaction_coverage` — no compile path is touched. See that module's own doc for the full
/// contract, including the honest summarization convention for large plans.
pub mod plan_diagram;
/// Stage 3 of `openspec/changes/add-pairwise-grammar-interaction-coverage` (the REFRAMED design):
/// tree-structured node/subtree interaction coverage over the reified compilation plan --
/// [`plan_interaction_coverage::AdjacencyTuple`] extraction + tagging,
/// [`plan_interaction_coverage::retired_interactions`] (orthogonality pruning, evidence-cited, never
/// invented), [`plan_interaction_coverage::compute_interaction_coverage`] (the required/covered/
/// uncovered/contains-unsupported report), and [`plan_interaction_coverage::
/// fuzz_gate_group_reordering_for_grammar`] (targeted Gate-node subtree fuzzing via
/// [`oracle::permute_gate_groups`] + [`oracle::differential_oracle`]). **BUILD-BREAKING as of
/// 2026-07-26** (`openspec/changes/plan-construct-coverage-completion` tasks.md 6.3): see that
/// module's own doc for the mapping and `tests/plan_interaction_coverage_gate.rs` for the flipped
/// gate.
pub mod plan_interaction_coverage;
pub mod precision;
pub(crate) mod preexpand;
/// `openspec/changes/add-fst-compilation-health-audit`, tasks.md section 1 ("Preflight"): the
/// cheap, pre-compile health pass -- [`preflight::preflight_findings`] turns
/// [`capability::characterize`]'s already-computed [`capability::CharacteristicsProfile`] and
/// [`capability_entry::evaluate_capability`]'s already-resolved [`capability::CompileDecision`]
/// into [`health::HealthFinding`]s BEFORE any foma compile is attempted -- semantic uncertainty
/// (`Refuse`), cost uncertainty (`ConfirmOnly`/unbounded quantifiers), and bounded-product findings
/// (`Unordered`-stratum rule counts, a grammar-wide mrule x prule product). See that module's own
/// doc for the full design and judgment calls.
pub mod preflight;
/// The compile-time
/// **profile** type -- [`profile::CompileProfile`]/[`profile::CompileStage`]/[`profile::
/// GroupLineCount`]/[`profile::ProfileLabel`] -- collected from the PRODUCTION
/// `emit::emit_with_budget_profiled` -> `foma::lexcread::fsm_lexc_parse_string` path
/// (`analyzer::FomaProposer::new_with_budget`). See that module's
/// own doc for the more expensive profiling this stays clear of, and
/// [`health_evaluator::profile_findings`] for how this feeds profile-sourced health findings.
pub mod profile;
/// The declared,
/// versioned threshold policy — [`readiness_policy::ThresholdPolicy`]/[`readiness_policy::
/// Threshold`]/[`readiness_policy::Calibration`] — a certification verdict ([`readiness_verdict`])
/// is measured against, same "define the versioned schema" precedent
/// as [`health`]/[`plan_diagram`]. See that module's own doc for exactly which seeded values are
/// measured vs. explicitly-marked un-calibrated placeholders.
pub mod readiness_policy;
/// The tiered
/// certification verdict — [`readiness_verdict::certify`] evaluates a grammar's REAL capability
/// decision (always calling [`capability_entry::evaluate_capability`] itself), its ADR-0005 trust
/// status, and its measured facts against a [`readiness_policy::ThresholdPolicy`], producing a
/// [`readiness_verdict::ReadinessReport`] that distinguishes `not-yet` (actionable by the language
/// team) from `not-supported` (actionable only by compiler work) and never lets an override-
/// trusted or unassessed check render as passing. See that module's own doc for the full honesty-
/// rule contract.
pub mod readiness_verdict;
/// **The accuracy question, split off from the speed question.** Confirmation-free undergeneration
/// detection by admission-key set containment against the run's already-shared oracle result — zero
/// full-HC confirmation calls per candidate. Deliberately says NOTHING about cost; ranking stays
/// [`recipe_optimizer::Score`]'s job. Read that module's own doc for the soundness argument and the
/// one hazard it is counted against.
pub mod recipe_accuracy;
/// Serializable mechanism vocabulary and fail-closed executable-recipe graph validation.
pub mod recipe_mechanism;
/// Extensible, budget-aware offline search and confirmed-only recipe selection.
pub mod recipe_optimizer;
/// Extensible registry of realizable compilation-recipe families.
pub mod recipe_registry;
/// Schema-versioned machine and human views over recipe-optimization runs.
pub mod recipe_report;
pub mod recipe_runtime;
/// Grammar-derived recipe-space bounds, pruning accounting, and pilot measurements.
pub mod recipe_space;
/// Replace-calculus rule compilation + underlying-form lexc -- the relational encoding of a
/// rewrite rule, used by [`build`] and [`gate`].
pub mod replace;
/// [`selection::
/// select_plan`] -- filters [`enumerate::enumerate_candidates`]'s candidate list to those whose
/// [`capability::compose_envelope`] decision is not `Refuse` (capability-safe by construction),
/// then picks the minimum measured `states + arcs` (via [`build::build_controllable`]), tie-broken
/// by root [`plan::NodeId`] (a content address). A library capability -- no production compile
/// path calls it today, a deliberately separate and still-open question; see that module's own doc
/// for the full filter/rank/tie-break contract.
pub mod selection;
/// The per-STRATEGY construct-coverage account: which of this crate's compilers can actually
/// PROPOSE each [`capability::CharacteristicKind`]. [`capability::Disposition::ConfirmOnly`] is
/// defined as "recall-preserving only if the proposer proposes the superset", and until this module
/// existed nothing checked WHICH proposer was in use -- so one compiler's coverage was silently
/// inherited by all three. Consulted by [`capability::compose_envelope_for_strategy`], which
/// [`selection::select_plan`] calls at the point a candidate becomes selectable.
pub mod strategy_coverage;
/// Exact shared P6 templated-morphotactics compile pipeline and its stage profile.
pub mod structural_allomorph;
pub mod tags;
pub mod templated_compile;
/// P6 feasibility prototype sibling of [`replace`]: the underlying-form lexc emitter.
pub mod uflexc;
/// `openspec/changes/cover-unordered-morph-rules`: the `MorphRuleOrder::Unordered` chain-depth-
/// bounded/unbounded compile-time cardinality gate -- [`unordered::check_unordered_strata_bound`],
/// wired into [`analyzer::FomaProposer::new_with_budget`], the second real production consumer of
/// [`compose_budget::ComposeBudget`]'s chain-depth-shaped budget discipline after
/// [`peel::ReduplicationPeeler`]. See that module's own doc for the full design, including this
/// change's own load-bearing finding that the "ordering-union proposal" design.md calls for is an
/// EXISTING mechanism (`crate::emit::build_deriv_chain`), not a new one.
pub(crate) mod unordered;
/// `openspec/changes/harden-foma-resource-safety` section 3/4 (`openspec/changes/
/// IMPLEMENTATION-READINESS.md` R2): the compile-worker watchdog -- a versioned request/result
/// protocol, a killable native worker process (`std::process::Command`/`Child::try_wait`/
/// `Child::kill`, sampled RSS via `sysinfo`), and typed outcomes mapped into [`health`]'s existing
/// vocabulary. ADR 0003's COMPILE-side containment, distinct from [`compose_budget`]'s in-process
/// cooperative APPLY-side budgets -- see that module's own doc. `#[cfg(not(target_arch =
/// "wasm32"))]`: this crate's own wasm32 dependency-graph discipline (R2: "WASM is analysis-only
/// and needs no compile watchdog") -- see that module's own top doc for why its three extra
/// dependencies are scoped to the identical target cfg in `Cargo.toml`.
#[cfg(not(target_arch = "wasm32"))]
pub mod worker;

/// Re-exported so downstream crates (and the P0 tests) have a single, versioned door into the
/// `foma` runtime rather than depending on it directly.
pub use foma as foma_runtime;
