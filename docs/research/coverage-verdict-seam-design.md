# A shared seam for pg-foma's coverage-verdict Modules

Status: design only, not implemented. Written for review before any code changes land.

## 0. Vocabulary note

This report uses **Module / Interface / Implementation / Depth / Seam / Adapter / Leverage /
Locality** exactly as `codebase-design` defines them, and the domain terms **Compiler / Backend /
Compatibility report / Selector / Artifact / Lowering** exactly as `CONTEXT.md` settles them
(2026-08-07). One clarification the report leans on throughout: `witnessed_coverage::
compile_with_backend` (`witnessed_coverage.rs:113`) calls the same entry points
(`FomaProposer::new`, `compile_templated_morphotactics`, `build_controllable` +
`finish_controllable_net`) the runtime uses, but it runs them **in-process, unsupervised**, not
inside the worker under `ExecutionLimits`. Per `CONTEXT.md`'s own definition, an **Artifact** is
specifically "produced ... inside the supervised worker under finite memory and time limits," so
this report never calls a witnessed-coverage compile attempt an Artifact — it is a direct,
unsupervised compile of one Backend's network, used only as measurement evidence. Quoted source
comments keep their original wording (including "compiler" used loosely for "Backend") because
they are quotations, not this report's own claims.

## 1. The finding, restated once

`witnessed_coverage::build_report` (`witnessed_coverage.rs:477`) and `faithfulness_coverage::
build_report` (`faithfulness_coverage.rs:576`) are the same function written twice, over two
different measurement mechanisms:

| | `witnessed_coverage` | `faithfulness_coverage` |
|---|---|---|
| Question | Did this Backend's own compile entry point return `Ok` for a grammar exhibiting `kind`? | Did this Backend's final proposal set contain every oracle identity, for a fixture exhibiting `kind`? |
| Mechanism | `compile_with_backend` (`:113`), in-process, unsupervised | `crate::backend_runtime::word_proposal_containment` over `evaluate_plans_observed_with_cache`'s output |
| Per-pair outcome | `BackendOutcome` (`:56`): `Compiled` / `RefusedBySelector` / `CompileFailed(String)` | `ContainmentOutcome` (`:57`): `Held` / `Failed{word,detail}` / `NotAttempted{reason}` |
| Per-unit observation | `GrammarObservation` (`:84`): label + kinds + `Vec<(EmissionStrategy, BackendOutcome)>` | `FixtureContainmentObservation` (`:79`): label + kinds + `Vec<(EmissionStrategy, ContainmentOutcome)>` (+ a `soundness` side-vector) |
| Fold | Double loop over `CharacteristicKind::ALL x ALL_STRATEGIES`, `BTreeSet`-based, into `witnessed`/`declared_cannot_represent`/`contradictions`/`gaps` | Same double loop, into `held`/`failed`/`not_attempted`/`over_generating` |
| Refused-by-selector branch | `selection.report_for(strategy).is_some_and(BackendReport::can_represent)` -> `RefusedBySelector` (`:215-219`) | Identical predicate -> `NotAttempted{reason:"refused-by-selector"}` (`:178-190`) |
| Position helper | `fn strategy_index` (`:605`) | `fn strategy_index` (`:699`), byte-identical body |
| Requirement type | `CompletenessRequirement` (`NonVacuity` / `NoGaps`) | `FaithfulnessRequirement` (`NonVacuity` / `NoMoreThan{failures}` / `NoFailures`) plus a second, structurally identical `SoundnessRequirement` (`NoMoreThan{over_generations}` / `NoOverGeneration`) for the orthogonal over-generation axis |
| Render | Denominator block, then per-strategy totals, then an inventory | Same three-part shape, different field names |

A third, independently-invented instrument sits one level up: `strategy_coverage_join.rs`'s
`JoinVerdict` (`:52`) compares the same table's `StrategyRepresentation` against a *third*
mechanism — full identity **equality** (`Certification::FullHcConfirmed`) — and
`examples/conf_matrix.rs` (its only caller of `measure_fixture_exact`) hand-rolls a **fourth**
per-fixture-per-strategy struct (`StrategyRow`/`FixtureRow`, `conf_matrix.rs:23-52`) with its own
`compiles: Result<(), String>` / `exact: bool` / `could_not_measure: Option<String>` fields — the
same shape as `BackendOutcome`/`ContainmentOutcome` reinvented a fourth time, in an example this
time rather than a library module. This is evidence the duplication is not a two-instrument
accident; it is what happens every time someone answers "did backend X measure OK on construct Y"
without a seam to answer it through.

Below the per-pair account: measured this session (method: `Select-String` over
`rust/crates/pg-foma/{src,tests}/*.rs`, so figures are this worktree's exact count, not the
finding's approximate ones — the two are close but I would not treat either as exact without
re-measuring):

- **33 test files** call `pg_conformance_fixtures::discover()`. Of those, **9** walk it with `for
  fixture in discover()` (a full-fixture-set sweep): `all_fixtures_foma_analyzer_new_no_panic.rs`,
  `conformance_coverage_gate.rs`, `envelope_agrees_with_compiler_gate.rs`,
  `exercises_tag_liveness.rs`, `net_dedup_sizing_census.rs`, `orthogonal_basis_group_b.rs`,
  `parity_divergence_census.rs`, `plan_interaction_coverage_gate.rs`,
  `structural_witness_gate.rs`. (`witnessed_strategy_coverage_gate.rs` and
  `faithfulness_coverage_gate.rs` bind `discover()` to a variable first, so they don't match this
  exact regex, but they are the same shape.) The other ~24 use `discover().into_iter().find(...)`
  to look up one named fixture — a different, much smaller pattern this report does not propose
  touching.
- **16 files** reference `catch_unwind` across `src/` and `tests/`.
- **38 call sites** reference `RunEvaluationCache::prepare`.

Six verdict enums for structurally similar questions, no shared supertype: `StrategyRepresentation`
(`strategy_coverage.rs:69`), `JoinVerdict` (`strategy_coverage_join.rs:52`), `BackendOutcome`
(`witnessed_coverage.rs:56`), `ContainmentOutcome` (`faithfulness_coverage.rs:57`), `Agreement`
(`envelope_agrees_with_compiler_gate.rs:16`, test-file-private), `AccuracyVerdict`
(`backend_accuracy.rs:222`). Section 8 works out which of these actually collapse.

## 2. Correctly separate today — left alone, confirmed by reading

- **`conformance_coverage::CoverageStatus`** (`:318`, `Covered`/`Uncovered`/`Unmappable`) asks
  whether the in-repo conformance suite exercises a construct at all — a fixture-authoring
  question, upstream of everything this report discusses. It never asks what a Backend measured.
- **`characterization::ClosureTerminal`** (`:99`, `Complete`/`Incomplete(reason)`/`Refused(reason)`)
  is a work-budget fact on the closure-construction readiness axis (`completed_build.rs` consumes
  it to decide `CompiledClosure` completeness), not a per-(kind, strategy) coverage fact.
- **`backend_mechanism::ExecutionDisposition`** (`:657`, `ExactFst`/`ConfirmOnly`/`Peeled`/
  `Refused`) is confirmed, by reading `MechanismBinding::derive` (`backend_mechanism.rs:694-716`),
  to be a pure re-derivation: it folds `representation_of` (the same function
  `StrategyRepresentation`'s table exposes) over a mechanism node's `construct_requirements` with a
  `worse_of` reduction, and adds exactly one extra case (`CopyProcess` -> `Peeled`) that
  `StrategyRepresentation` itself has no vocabulary for. It answers a graph-node's disposition, not
  a `(CharacteristicKind, EmissionStrategy)` pair's — a strict aggregation of a fact this deepening
  already treats as the input side (Section 8), never itself a measured Verdict.
- **`Certification`** (`backend_optimizer.rs:253`) is the lower-level, richer primitive several of
  the six enums read from indirectly (`RuntimeEvaluation.certification`), but none of the six IS
  `Certification` and `Certification` answers a different-grained question (one candidate's
  corpus-wide identity comparison against the oracle, seven variants including budget/build
  failures that have no `(kind, strategy)` meaning at all). It is correctly a separate, lower-level
  type this design reads but does not fold in.

## 3. The shared vocabulary every design below needs

Every one of `BackendOutcome`, `ContainmentOutcome`, and `AccuracyVerdict` answers the same
three-way question — "did the measurement find nothing wrong, find a defect, or never run" — and
each currently spells "never ran" differently (an enum variant with no payload, a `NotAttempted
{reason: String}`, a free-text reason folded into `NotDetermined`). This is the finding's own
"no shared vocabulary" observation, made precise:

```rust
/// Why a measurement never happened. Two causes, deliberately not one bag of strings: a
/// Selector refusal is a fact about the Backend/grammar pair that cannot change without a
/// capability or Selector change, so it can never be closed by "try harder"; every other
/// non-attempt (an oracle-preparation fault, a truncated proposal set, an empty corpus) is a
/// fault that a fix CAN close. Conflating the two is exactly how a capability-refused Backend
/// gets counted, unintentionally, in the same backlog as a real defect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotAttemptedReason {
    RefusedBySelector,
    Fault(String),
}

/// One (kind, strategy) — or (fixture, strategy) — pair's outcome, generic over what a FAILURE
/// carries as evidence. `Failure` is the only place `BackendOutcome`, `ContainmentOutcome`, and
/// `AccuracyVerdict` differ; the three-way shape itself is identical across all three today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict<Failure> {
    Held,
    Failed(Failure),
    NotAttempted(NotAttemptedReason),
}
```

The deliberate choice: **do not replace `BackendOutcome`/`ContainmentOutcome` with `Verdict`
itself.** `Compiled` and `Held` are not synonyms a reader should have to translate — each name is
already the right domain word for its own question, and CLAUDE.md's own instruction elsewhere on
this file ("collapsing two that answer different questions would be worse than the status quo")
argues against merging vocabulary that still needs to say different things. Instead, a trait lets
each concrete enum keep its name while being *provably* the same shape underneath:

```rust
/// The fold contract: whatever an instrument's own enum is called, this is recoverable from it at
/// zero cost. Implementing this is what lets a per-pair enum plug into the shared matrix/fold
/// (Section 5) without becoming, or losing information to, `Verdict` itself.
pub trait MeasuredOutcome {
    type Failure: Clone;
    fn classify(&self) -> Verdict<Self::Failure>;
}

impl MeasuredOutcome for BackendOutcome {
    type Failure = String; // the compile-failure reason
    fn classify(&self) -> Verdict<String> {
        match self {
            Self::Compiled => Verdict::Held,
            Self::CompileFailed(reason) => Verdict::Failed(reason.clone()),
            Self::RefusedBySelector => Verdict::NotAttempted(NotAttemptedReason::RefusedBySelector),
        }
    }
}

impl MeasuredOutcome for ContainmentOutcome {
    type Failure = (String, String); // (word, detail)
    fn classify(&self) -> Verdict<(String, String)> {
        match self {
            Self::Held => Verdict::Held,
            Self::Failed { word, detail } => Verdict::Failed((word.clone(), detail.clone())),
            Self::NotAttempted { reason } if reason == "refused-by-selector" =>
                Verdict::NotAttempted(NotAttemptedReason::RefusedBySelector),
            Self::NotAttempted { reason } => Verdict::NotAttempted(NotAttemptedReason::Fault(reason.clone())),
        }
    }
}
```

Note the wart this surfaces immediately: `ContainmentOutcome::NotAttempted` spells selector-refusal
as the **string literal** `"refused-by-selector"` (`faithfulness_coverage.rs:187`), where
`BackendOutcome` has a real variant for it. `classify()` has to string-match to recover the same
fact `BackendOutcome` already types. This is worth fixing as part of any design below — either
`ContainmentOutcome::NotAttempted` grows a typed `reason: NotAttemptedReason` field directly (the
cleanest fix, and a small, mechanical, behavior-preserving change on its own), or the seam accepts
the string-match as a documented, tested wart. I'd fix it; it's cheap and it removes a second place
the "refused-by-selector" spelling could drift.

## 4. Design A — an Observer trait invoked during one sweep

```rust
/// What one instrument needs from a single fixture, computed at most once per fixture per sweep,
/// on first demand. A compile-only observer never triggers oracle preparation at all.
pub struct FixtureContext<'a> {
    pub label: &'a str,
    pub grammar: &'a Grammar,
    words: &'a [String],
    semantics: OnceCell<GrammarSemantics<'a>>,
    selection: OnceCell<BackendSelection>,
    oracle_cache: OnceCell<Result<RunEvaluationCache, OraclePreparationFault>>,
}

impl<'a> FixtureContext<'a> {
    pub fn semantics(&self) -> &GrammarSemantics<'a> { /* memoized */ }
    pub fn admits(&self, strategy: EmissionStrategy) -> bool { /* consults `selection` once */ }
    pub fn oracle_cache(&self) -> Result<&RunEvaluationCache, &OraclePreparationFault> { /* lazy */ }
}

/// One measurement mechanism. An instrument implements this once; it never re-walks
/// `pg_conformance_fixtures::discover`, never re-derives the refused-by-selector branch, and never
/// re-implements the (kind, strategy) fold — `run_sweep` owns all three.
pub trait CoverageObserver {
    type Failure: Clone;
    fn name(&self) -> &'static str;
    /// Called only for a (fixture, strategy) pair `run_sweep` has already confirmed the Selector
    /// admits — `run_sweep` applies `NotAttemptedReason::RefusedBySelector` itself, uniformly, so
    /// no observer can spell that branch differently or forget it.
    fn measure(&self, fixture: &FixtureContext<'_>, strategy: EmissionStrategy) -> Verdict<Self::Failure>;
}

/// Walks every fixture `scope` claims once, characterizes it once, and asks `observer` to measure
/// every Backend the Selector admits for it. Owns: the fixture walk, the panic recovery (a
/// `measure` panic becomes `Verdict::Failed` for THIS observer only, via the same
/// `panic::catch_unwind` discipline `witnessed_coverage.rs:222` already uses — it never aborts
/// the sweep), the refused-by-selector branch, and the lazy oracle cache.
pub fn run_sweep<O: CoverageObserver>(scope: ConformanceScope, observer: &O) -> CoverageMatrix<O::Failure> { .. }

/// The same walk, sharing one `FixtureContext` cache across multiple `run_sweep` calls in the same
/// process — e.g. a future combined CLI report that wants both accounts without preparing the
/// oracle for the same fixture twice.
pub fn run_sweep_with_cache<O: CoverageObserver>(
    scope: ConformanceScope,
    observer: &O,
    cache: &mut FixtureCache,
) -> CoverageMatrix<O::Failure> { .. }
```

**A real Rust-shaped constraint worth stating rather than hand-waving:** `CoverageObserver::Failure`
is an associated type, so `&[&dyn CoverageObserver]` cannot hold two observers with different
`Failure` types in one slice without erasing it (e.g. `Box<dyn Display>`), which would lose the
static typing `AccuracyMiss`/`ContainmentGap`-shaped failures currently have. So `run_sweep` is
generic over exactly **one** observer type per call — a caller wanting both accounts calls it
twice, exactly mirroring today's two separate gate binaries. The `FixtureCache` parameter is what
lets a caller opt into cross-call sharing when it wants it, without forcing it.

`witnessed_coverage::observe_grammar`/`faithfulness_coverage::observe_fixture_containment` each
become one `impl CoverageObserver`, roughly 20-30 lines carrying only their own compile/containment
logic; `compile_with_backend` and `word_proposal_containment` stay exactly where they are and are
called from inside `measure`.

## 5. Design B — a typed matrix instruments query (pull, post-hoc)

```rust
/// Everything any current instrument needs about one fixture's one Backend, collected once. A
/// superset union of what `witnessed_coverage` and `faithfulness_coverage` separately collect.
pub struct BackendEvidence {
    pub strategy: EmissionStrategy,
    pub admitted: bool,
    pub compile: Result<(), String>,
    /// `None` when compile failed, the fixture has no words, or oracle preparation faulted.
    pub containment: Option<Vec<WordEvidence>>,
    pub divergence: Option<IdentityDivergence>,
}

pub struct FixtureEvidence {
    pub label: String,
    pub kinds: Vec<CharacteristicKind>,
    pub backends: Vec<BackendEvidence>, // one per ALL_STRATEGIES, admitted or not
}

/// Walks every fixture once, compiles with every admitted Backend, and — only for a fixture with
/// non-empty words — also runs the oracle-backed evaluation. One fixture visited once for BOTH
/// today's accounts.
pub fn collect_evidence(scope: ConformanceScope) -> (usize, Vec<FixtureEvidence>) { .. }

pub type Classifier<F> = fn(&BackendEvidence) -> Verdict<F>;

pub fn build_matrix<F>(
    scope: &str, discovered: usize, fixtures: &[FixtureEvidence], classify: Classifier<F>,
) -> CoverageMatrix<F> { .. }

pub fn witnessed_classifier(e: &BackendEvidence) -> Verdict<String> {
    if !e.admitted { return Verdict::NotAttempted(NotAttemptedReason::RefusedBySelector); }
    e.compile.clone().map_or_else(|reason| Verdict::Failed(reason), |()| Verdict::Held)
}
```

This makes adding a **new** instrument free of new collection code — a new instrument is one pure
function of `&BackendEvidence`, unit-testable with a hand-built value and no compile step or oracle
in the loop at all (the cheapest possible test surface of the three designs). But `BackendEvidence` is a
god-struct every classifier depends on: a field added for one future instrument is a recompile (and
a "what does this mean for me" question) for every existing one, and — the sharper cost — it forces
a fixture with no oracle-relevant use case to pay for oracle preparation anyway, unless
`collect_evidence` grows its own laziness, at which point it has re-invented Design A's
`FixtureContext` inside a struct rather than a trait, with a worse seam (data, not behavior, drawn
at the collect/classify line rather than at the walk/measure line — Section 7 says why that
matters).

## 6. Design C — a shared walk + a shared fold, instrument-specific measurement stays put

```rust
/// Any instrument's per-unit observation: a labelled unit, the constructs it exhibits, and one
/// measured outcome per strategy. Replaces `GrammarObservation` and
/// `FixtureContainmentObservation`, which are this exact shape twice, keyed on different `E`.
pub struct Observation<E> {
    pub label: String,
    pub kinds: Vec<CharacteristicKind>,
    pub outcomes: Vec<(EmissionStrategy, E)>,
}

/// Owns exactly the walk + panic recovery every full-sweep gate re-implements today
/// (`witnessed_strategy_coverage_gate.rs:17-34`, `faithfulness_coverage_gate.rs:20-53`, both
/// hand-rolling `panic::take_hook`/`catch_unwind`/`discover()` identically). `observe_one` is the
/// ONLY instrument-specific code left — exactly today's `observe_grammar`/
/// `observe_fixture_containment` bodies, called as a closure instead of owning their own loop.
pub fn collect_observations<E>(
    scope: ConformanceScope,
    observe_one: impl Fn(&FixtureRef, &Grammar) -> Observation<E>,
) -> (usize, Vec<Observation<E>>) { .. }

/// Folds any instrument's observations into the shared matrix. This is the literal deletion of
/// both `build_report` bodies and both `strategy_index` copies.
pub fn build_report<E: MeasuredOutcome>(
    scope: &str, discovered: usize, observations: &[Observation<E>],
) -> CoverageMatrix<E::Failure> { .. }

/// The refused-by-selector predicate, extracted once so both instruments call it instead of each
/// re-deriving `is_some_and(BackendReport::can_represent)` and re-spelling the not-attempted case.
pub fn admitted_strategies(selection: &BackendSelection) -> (Vec<EmissionStrategy>, Vec<(EmissionStrategy, NotAttemptedReason)>) { .. }
```

`witnessed_coverage`'s and `faithfulness_coverage`'s own compile/containment logic is untouched:
`compile_with_backend`, `word_proposal_containment`, `RunEvaluationCache::prepare` all stay exactly
where they are, called from inside the `observe_one` closure each instrument still owns. What
disappears is: two `build_report` bodies, two `strategy_index` copies, two independently-spelled
refused-by-selector branches, and (with `collect_observations`) two independently hand-rolled
`discover()` + `catch_unwind` walks.

## 7. Comparison

| | Depth | Locality | Leverage | Seam placement |
|---|---|---|---|---|
| **A — Observer trait, live sweep** | Deepest: one trait method (`measure`) is the whole interface a new instrument must write; walk, panic recovery, refusal, lazy oracle sharing, fold, and render are all hidden. | High on both halves: a walk/refusal bug is fixed once for every present and future instrument; a measurement bug stays local to one `impl`. | Highest ongoing: a genuinely new instrument (e.g. a per-construct soundness account, closing `backend-measurement-instruments.md` defect 3) is one `impl CoverageObserver`, with lazy oracle sharing for free if run alongside `faithfulness_coverage`'s observer via `run_sweep_with_cache`. | A NEW seam, between "does this (fixture, backend) get looked at" and "what does looking at it mean" — doesn't exist in the code today (today those two questions are one function). |
| **B — Evidence-first matrix, pull** | Deep on the query side (a classifier is a pure `fn(&BackendEvidence) -> Verdict<F>`, the smallest surface of the three), shallow on the collection side (`collect_evidence` grows without bound as instruments differ in what they need). | Good for a classifier bug (fully isolated, no compile attempt or oracle needed to test it). Poor for the collector: every instrument now depends on one struct's shape, and a change made for one instrument's sake reaches every other instrument's compile step even when semantics don't change — the coupling risk CLAUDE.md's "never re-derive a decision another module makes" section warns about, in the opposite direction (here it's "every module MUST read the same over-wide struct," not "a module re-derives a fact"). | High for a classifier-only instrument; negative for one needing evidence `BackendEvidence` doesn't carry yet (a widening, felt everywhere). Forces oracle preparation onto every fixture even for a compile-only question, unless it re-invents A's laziness with worse ergonomics. | The seam matches the domain most closely (Compatibility-report-like "what's objectively true" vs. "what does it mean"), but it is the seam most likely to grow unboundedly — every future question is a struct field, not a new, independent unit. |
| **C — Shared walk + shared fold, instrument-specific measurement** | Shallowest of the three: an instrument still writes its own `observe_one`, i.e. still owns compile-vs-evaluate mechanism entirely; only the walk and the fold move behind a seam. | Excellent for exactly the duplication actually measured (Section 1): one `build_report`, one `strategy_index`, one refusal predicate, one walk-plus-panic-recovery. No new cross-instrument coupling — `collect_observations<E>` never sees another instrument's `E`. | Real but bounded: a new instrument still writes a full measurement function; it only gets the fold and walk for free, not lazy cross-instrument sharing. | The seam stays where it already is (`observe_one`'s body is where fixture-walking and meaning are fused, exactly as `observe_grammar`/`observe_fixture_containment` are today); the only NEW seam is narrow — `Observation<E>` -> `CoverageMatrix<F>`. |

## 8. Recommendation

**Design C first; Design A as a deliberate follow-on Design C makes cheap, not a competing choice.**

Design C is a strict subset of Design A: `run_sweep`'s eventual Implementation, if A is ever built,
would itself construct `Observation<O::Failure>` values per fixture and hand them to Design C's
`build_report` at the end. Choosing C now does not foreclose A later — it is the exact code C
leaves in place (`observe_one`'s closure body) that would later become `CoverageObserver::measure`'s
body, unchanged, when and if a third or fourth full-sweep instrument actually needs `run_sweep`'s
lazy cross-instrument oracle sharing.

The case for C, not A, **right now**: the finding in hand (Section 1) is that the AGGREGATION is
duplicated verbatim — one `build_report`, one `strategy_index`, one refusal predicate — not that
the WALK needs unifying. Only two library modules currently do a full sweep-and-fold
(`witnessed_coverage`, `faithfulness_coverage`); `conf_matrix.rs` is a third, but it's an example
answering a different, richer question (`StrategyRow` carries certification debug text, exact/
soundness/legal-overgeneration counts `BackendOutcome`/`ContainmentOutcome` don't have at all) and
folding it into a shared trait-driven sweep is real, separate design work this report has not done
— flagging it, not deciding it. Design A's leverage (free lazy oracle sharing across instruments,
one trait method per new instrument) is a bet on a THIRD library-level full-sweep instrument
arriving; that bet may well pay off (the whole point of `backend-measurement-instruments.md`'s
"what the canonical instrument should be" section is that one is coming — a per-construct
soundness account), but paying for it now, before that instrument exists, means designing
`FixtureContext`'s laziness and `run_sweep_with_cache`'s cross-call sharing against a need that is
still hypothetical. Design B I would not recommend as the primary move at all: it couples every
instrument to one struct's shape for a collection-cost saving that has not been measured to
matter — nothing today shows the two gates' redundant compile cost across two separate `cargo
nextest` binaries is a real build-time problem, and CLAUDE.md's own repeated stance ("a gate that
taxes every ordinary build gets switched off and then protects nobody") argues for measuring before
paying a coupling cost, not the reverse.

So: ship C's shared vocabulary (`Verdict`, `NotAttemptedReason`, `MeasuredOutcome`, `Observation<E>`,
`build_report<E>`, `admitted_strategies`) now. Revisit A specifically at the moment a genuine third
full-sweep instrument is proposed (the per-construct soundness account is the visible near-term
candidate) — at that point `collect_observations`/`build_report` are already generic over `E`, so
lifting `observe_one` into `CoverageObserver::measure` is a mechanical, low-risk step, not a
redesign.

## 9. The six verdict enums — what collapses, what stays distinct

- **`BackendOutcome` and `ContainmentOutcome` collapse structurally, keep their names.** Both are
  exactly `Verdict<Failure>` under `MeasuredOutcome` (Section 3): `Compiled`/`Held` are the same
  "nothing wrong found" case, `CompileFailed`/`Failed{..}` are the same "found a defect, named"
  case, and their two spellings of "never measured" (a bare variant vs. a stringly-typed reason)
  are the exact duplication the finding names. They keep their OWN enum identities — forcing both
  into one literal enum would either drop `ContainmentOutcome`'s `(word, detail)` evidence or add
  useless `word`/`detail` fields to a compile-only outcome that has no word in play at all. The
  trait, not the enum, is what's shared.
- **`AccuracyVerdict` is the same three-way shape (`NoLoss`/`Undergenerated{misses}`/
  `NotDetermined{reason}`) but a different GRAIN.** `BackendOutcome`/`ContainmentOutcome` are keyed
  on `(CharacteristicKind, EmissionStrategy)`; `AccuracyVerdict` (`backend_accuracy.rs:222`) is
  keyed on one candidate's whole corpus run, with no `kinds`/denominator bookkeeping anywhere in
  `backend_accuracy.rs` at all. It CAN implement `MeasuredOutcome` (worth doing on its own merits —
  `NotDetermined`'s reason today conflates a peel-refusal, an apply-refusal, a vacuous-oracle case,
  and (at its call sites, not shown in this file) possibly a Selector refusal, the identical
  "several unrelated causes, one string" defect), but it does **not** fold into `CoverageMatrix`
  without a wrapping Adapter that first attributes one accuracy run to the construct(s) its corpus
  exhibits — exactly the attribution `strategy_coverage_join.rs`'s own doc says is
  unsound in the reverse direction ("a fixture measured NOT exact... may be failing on a different
  construct"). Flagged as real follow-on work, not blocking this deepening, and genuinely uncertain
  until someone tries it.
- **`StrategyRepresentation` stays distinct — it is not a measurement at all.** It is a
  hand-curated, REVIEWED prediction ("can this Backend's proposer represent this construct"),
  never itself run against a fixture (`strategy_coverage.rs`'s own doc: "a reviewer reads the
  emitter and writes the row"). It is the DECLARED half of a join, never a `Verdict`.
- **`JoinVerdict` stays distinct — it answers "does the declared table agree with a measurement,"
  a different question from "what did the measurement find."** It is a two-input comparator over
  (`StrategyRepresentation`, a bool derived from `Certification::FullHcConfirmed` exactness) — a
  THIRD mechanism (full identity equality, not compile success or containment), so it is not even
  comparing against `BackendOutcome`/`ContainmentOutcome`'s `Verdict`. Unifying it into the shared
  matrix would be scope creep this deepening should not attempt.
- **`Agreement` (`envelope_agrees_with_compiler_gate.rs:16`, test-file-private) is the SAME shape
  of question as `JoinVerdict`** — a comparator between a declared fact (the capability envelope's
  admit/refuse) and a measured fact (whether the compile attempt actually succeeded) — just over a
  different declared/measured pair, and keyed on `(fixture, strategy)` rather than
  `(CharacteristicKind, strategy)`, so it never touches the matrix this deepening builds at all. It
  is also deliberately test-local (not `pub` in `src/`), which reads as an intentional "this is a
  one-off falsification harness, not crate vocabulary" choice. Worth a NAME note for whoever next
  touches that file — `Agreement`'s `{Agree, TooLax(reason), TooStrict}` and `JoinVerdict`'s
  `{Agreed, Contradicted, Unsupported, NoEvidence}` are recognizably the same "declared vs.
  measured, and which direction diverges" family and COULD share a small generic
  `enum Comparison<F> { Agree, TooLax(F), TooStrict }` if a third comparator of this shape ever
  shows up — but that is a smaller, separate cleanup, not part of this coverage-matrix seam, and I
  am not recommending it now.

## 10. Does `certify_word`'s home move?

**No — not into this seam.** `certify_word`/`certify_word_measured`/`certify_corpus`/
`certify_corpus_measured` (`backend_runtime.rs:833-1012`), plus `WordEvidence` (`:659`) and
`ContainmentGap`/`word_proposal_containment` (`:692-753`), are the single-word/single-corpus
oracle-comparison primitives that `measure_and_certify_inner` (`:1228`, confirmed by reading
`:1327` calling `certify_corpus_measured` directly) uses to build every `RuntimeEvaluation`. Their
callers, measured directly (not through either coverage module): `backend_runtime.rs` itself,
`examples/adjudicate_templated_backend.rs`, `tests/cross_compiler_equivalence_gate.rs`, and
`tests/backend_runtime_oracle_bound_gate.rs`. None of `witnessed_coverage.rs` or
`faithfulness_coverage.rs` calls `certify_word`/`certify_corpus` at all — both go through
`evaluate_plans_observed_with_cache`/`word_proposal_containment`, one level higher, and read
`WordEvidence`/`RuntimeEvaluation` as already-computed input. So `certify_word`'s actual home is a
different concern at a different grain (one word/corpus vs. `evaluate_plans*`'s per-candidate score
vs. this deepening's per-(kind, strategy) matrix), with its own three independent callers that have
nothing to do with per-construct coverage. Moving it under whichever module hosts the new coverage
seam would COUPLE those three unrelated callers to the coverage seam for no reason — a locality
regression, not an improvement, and exactly the "one used to make two" trap this crate's own
comments warn against elsewhere.

That said, the separate finding this report was told about ("a separate finding says it belongs
behind this seam") is pointing at something real, just mis-aimed: `backend_runtime.rs` is 2,271
lines mixing at least three concerns — oracle preparation (`PreparedCorpus`, `RunEvaluationCache`),
single-word/corpus certification (the cluster above), and per-candidate scoring/evaluation
(`measure_and_certify*`, `evaluate_plans*`, `RuntimeEvaluation`). Splitting the certification
cluster into its own module (e.g. `word_certification.rs`) is a legitimate, independent cleanup —
but its destination should be a module of its own, not a subordinate of the coverage-matrix seam,
because its three existing callers have no relationship to `(CharacteristicKind, EmissionStrategy)`
coverage at all.

## 11. The 36 fixture walks and 9 `catch_unwind` scaffolds — what a gate looks like afterward

Of the measured 9 full-sweep files (Section 1) plus the two coverage gates, only
`witnessed_strategy_coverage_gate.rs` and `faithfulness_coverage_gate.rs` are actually the two
`build_report` duplicates this deepening targets. Under Design C:

```rust
// tests/witnessed_strategy_coverage_gate.rs, after
fn report() -> CoverageMatrix<String> {
    let (discovered, observations) = pg_foma::coverage_seam::collect_observations(
        claimed_scope(),
        |fixture, grammar| pg_foma::witnessed_coverage::observe_grammar(&fixture.label(), grammar),
    );
    pg_foma::coverage_seam::build_report(claimed_scope().label(), discovered, &observations)
}
```

The gate's own assertions (`report.check(REQUIREMENT)`, the "non-default backends are among the
exercised ones" pin, the falsification tests) are unchanged in SHAPE — they read the same fields off
`CoverageMatrix<String>` that they read off `CompletenessReport` today, since `CoverageMatrix` is
that type generalized, not a new question. What disappears from the two gate files: the hand-rolled
`panic::take_hook`/`set_hook`/`catch_unwind` dance (both `witnessed_strategy_coverage_gate.rs:21-32`
and `faithfulness_coverage_gate.rs:24-51`, currently identical in structure), and the `discover()`
loop itself. The other 7 full-sweep files (`all_fixtures_foma_analyzer_new_no_panic.rs`,
`conformance_coverage_gate.rs`, `exercises_tag_liveness.rs`, `net_dedup_sizing_census.rs`,
`orthogonal_basis_group_b.rs`, `parity_divergence_census.rs`, `plan_interaction_coverage_gate.rs`,
`structural_witness_gate.rs`) are answering DIFFERENT questions (conformance-fixture coverage,
tag-liveness, net-dedup sizing, a parity census, plan-interaction coverage, structural-witness
coverage) that do not fold into a `(CharacteristicKind, EmissionStrategy)` matrix at all — this
report does not propose touching them, and I flag that as a deliberate scope limit, not an
oversight: forcing them onto this seam would be the exact "collapsing two that answer different
questions" the task warns against.

## 12. Migration order keeping the tree green at every step

1. **Add the shared vocabulary as new, additive code**: `NotAttemptedReason`, `Verdict<F>`,
   `MeasuredOutcome`, `Observation<E>`, `collect_observations`, `build_report<E: MeasuredOutcome>`,
   `admitted_strategies`, in a new module (e.g. `coverage_seam.rs`). Nothing existing changes yet;
   this step cannot regress anything because nothing calls the new code.
2. **Implement `MeasuredOutcome` for `BackendOutcome` and `ContainmentOutcome`** (fixing
   `ContainmentOutcome::NotAttempted`'s stringly-typed selector-refusal case as part of this step,
   per Section 3 — a small, mechanical, behavior-preserving field change). Add a throwaway
   differential test: for a fixed fixture set, `witnessed_coverage::build_report`'s output and
   `coverage_seam::build_report` fed `BackendOutcome`-classified observations must agree on every
   field, byte-for-byte. This is CLAUDE.md's own stated discipline ("build the differential
   measurement before the change, not after") applied to a refactor rather than a behavior change —
   the risk here is not "is the new logic right," it's "did the port actually preserve the old
   fold's semantics," and only a diff against the old implementation answers that.
3. **Port `witnessed_coverage.rs` first** (lower stakes: its gate asserts
   `CompletenessRequirement::NonVacuity` only, no ratchet to preserve) — delete its own
   `build_report`/`strategy_index`/`CompletenessReport`, keep `observe_grammar`/
   `compile_with_backend` exactly as is, re-point `witnessed_strategy_coverage_gate.rs` at the
   shared seam. Confirm green with `pg.ps1 -Mode test -Package pg-foma -TestTarget
   witnessed_strategy_coverage_gate` (a real build slot, not skipped, per this repo's own
   verification discipline) before touching the next module.
4. **Port `faithfulness_coverage.rs` second**, with the differential check from step 2 repeated
   against ITS output (`FaithfulnessReport`'s fields, including `over_generating`/
   `SoundnessRequirement`) before deleting the old `build_report`. **The ratchet constant
   `NoMoreThan { failures: 19 }` (`faithfulness_coverage_gate.rs:14`) and
   `SoundnessRequirement::NoOverGeneration` (`:17`) do not move in VALUE at this step, in either
   direction.** This is a pure code-motion refactor; the whole point of the differential check is
   to prove `failed.len()` and `over_generating.len()` are identical before and after, so the
   ratchet's value staying at 19 (and the soundness floor staying at 0) is the SUCCESS CRITERION
   for this step, not a decision made in it. If the port changes either count, that is a bug in the
   port, not a finding about the grammar — fix the port, not the ratchet.
5. **A later, separate, deliberate step — not mechanically forced by this migration — is where the
   ratchet legitimately could move.** Today `not_attempted` (17 pairs, per the prompt's own count;
   I have not independently re-measured it) folds every non-attempt reason into one string picked
   via `reasons.into_iter().next()` (`faithfulness_coverage.rs:654`), discarding every OTHER
   exhibiting fixture's reason for the same pair. Once `NotAttemptedReason` is a real, typed field
   (step 2) rather than string prose, a maintainer can for the first time separate "refused by the
   Selector, and therefore unmeasurable until a capability changes" from "faulted for a fixable
   reason" within that 17. That split is what could motivate a NEW, second ratchet — e.g.
   `NoMoreThan { unmeasured_faults: N }` counting only the `Fault(_)` share of `not_attempted`,
   deliberately excluding `RefusedBySelector` pairs since those can never shrink without a
   capability change and so should never count against a "shrink this backlog" ratchet. This would
   be a WIDENING (a new ratchet added alongside the existing one), not a move of the existing `19`
   in either direction, and it is a decision for whoever reads that split first, not something this
   report is deciding now.

## 13. What becomes assertable that is not assertable today

- **A ratchet on genuinely-fixable non-attempts, separate from Selector-refused ones** (Section 12,
  step 5) — today `not_attempted`/`gaps` conflate "can't measure by design" with "can't measure
  because something is broken," so a regression in oracle-preparation robustness is invisible
  unless someone reads the printed inventory by eye.
- **Cross-instrument consistency**: "does `(kind, strategy)` read `Held`/`Compiled` under one
  instrument while reading `RefusedBySelector` under another, for the SAME grammar?" Both
  instruments call `select_backends(&semantics)` on what should be the same characterization, but
  nothing today joins a `witnessed_coverage` row to a `faithfulness_coverage` row for the same
  fixture and asserts they used the same Selector answer. With one shared `Observation<E>` shape
  and one shared fold, writing that gate is a matter of running both instruments' `Observation`s
  through a shared `admitted_strategies` call and diffing — a gate that cannot be written cheaply
  today because the two Selector calls happen inside two independently-typed, non-comparable
  observation shapes.
- **A new instrument (e.g. the per-construct soundness account
  `backend-measurement-instruments.md` names as the outstanding "what the canonical instrument
  should be" work, closing its defect 3) costs one `observe_one` function plus one
  `impl MeasuredOutcome`, not a new `build_report`/`strategy_index`/render from scratch.**

## 14. Uncertainty flagged explicitly

- The finding's "36 test files" / "9 `catch_unwind` scaffolds" / "~30 `RunEvaluationCache::prepare`
  sites" and this report's measured 33 / 16 / 38 are close but not identical — different counting
  conventions (files vs. call sites; which grep pattern counts as "a scaffold"). Re-measure before
  citing either figure as exact in an implementation PR.
- The `not_attempted.len() == 17` figure in Section 12 is taken from the task prompt verbatim; I
  have not independently re-run the gate to confirm it (no builds were run for this report, per
  its own read-only/no-build constraint).
- Whether `AccuracyVerdict` is worth wrapping into `MeasuredOutcome` on its own (Section 9) without
  also solving its attribution problem is a real open question — flagged, not decided.
- Whether `conf_matrix.rs`'s `StrategyRow`/`FixtureRow` should eventually become a fourth
  `CoverageObserver`/classifier is out of scope for this report; it's cited only as a fourth data
  point for the underlying duplication (Section 1), not analyzed further.
