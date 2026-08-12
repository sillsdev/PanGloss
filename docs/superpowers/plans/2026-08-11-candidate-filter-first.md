# Candidate Filter First Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and certify a multi-pass, proof-carrying candidate filter before building the replacement local FST generator, so invalid candidate traces die cheaply and explainably while every oracle-valid analysis survives.

**Architecture:** `pg-foma` gains a deep `candidate_filter` module whose Rust passes consume rich symbolic witnesses, verify every rejection proof, and retain unknowns. It is developed first with synthetic traces and the five private read-only language oracles, then inserted in shadow mode between the current proposer and the unchanged HC confirmer; stable regular passes can subsequently compile into an internal trace DFA behind the same interface.

**Tech Stack:** Rust 2024 workspace, `pg-grammar`, `pg-rules`, `pg-parse`, `pg-foma`, pure-Rust foma only for the existing proposer, `serde` diagnostics, `rust/tools/pg.ps1`, `pg-conformance-fixtures` fail-closed corpus access.

**Governing contract:** `docs/superpowers/specs/2026-08-11-candidate-filter-contract.md`

---

## Revision note — read before implementing any task

**The contract supersedes this plan wherever they disagree.** Two things changed after Tasks 1-3
were built, and this document's earlier task text still describes the superseded design.

**1. There is no verifier in the pipeline.** Production performs no proof checking of any kind.
Verification is a post-hoc assertion in test-only code: a run records its rejections, and a test
then checks that every recorded proof re-derives against the witness it was emitted for.
`ProofCheckDepth`, `PassOutcome::ProofRejected`, `DeferReason::ProofVerificationFailed`, the
`verifier` field, the allow-list seam, and the proof-verification counters are all deleted. Task 2's
and Task 3's text below describes the machinery as originally built; it was replaced, and any
snippet naming those types is history, not instruction.

**2. Exhaustiveness is not the near-term bar.** Model checks should be modest and cheap. What a pass
must show is that it earns its keep: accept/defer/reject counts reported per pass, at least one real
rejection, and a cost figure. Full conformance grammars are coming and will expose the rest.

**A measurement gate now precedes Tasks 5-11, and it is a placement question, not a go/no-go on
filtering as such.** Every constraint can be enforced in one of three places, with different cost
shapes:

- **In the FST** — paid once at compile time (entry count, state count) plus traversal cost; the
  candidate is never proposed.
- **In a filter** — paid per proposed candidate; the HC cascade for that candidate is skipped.
- **In HC** — paid per candidate as full cascade cost, and always correct.

HC is the authority and is already correct, so a filter never buys correctness. It buys speed, and
only when BOTH conditions hold: the constraint cannot go into the FST cheaply, AND the candidates it
kills would otherwise be *expensive* to reject in HC.

The second condition is where the existing evidence bites.
`docs/superpowers/specs/2026-07-16-candidate-prefilter-plan.md` records a **NO-GO** from 2026-07-17
(census merged `571b8a3`): validity-gate rejections were a median ~3% of failing-candidate time,
while the mrule/template unapply cascade is 88-99% of confirm cost — those candidates fail *cheaply*
in HC, so predicting them saves little. Two of this plan's own structural passes hit the same wall
harder: `confirm_batch_impl` skips pin-failing candidates before any parse, and `ExploreMode::Pruned`
already consults `MorphotacticIndex` before every recursive emission step, so both target work the
production path already avoids.

So the first measurement is: **attribute HC cascade time by failure reason, then ask which reasons a
cheap check could predict.** A constraint earns a filter only when its candidates are expensive to
reject. The 2026-07-16 census harness (`rust/crates/pg-foma/examples/prefilter_census.rs`) already
does this for one class and is the thing to extend rather than rebuild.

Report the **distribution**, never a mean. Per-candidate HC cost is heavy-tailed: at roughly 250ns
per pass evaluation, avoiding one 1ms candidate pays for four thousand evaluations, so an aggregate
percentage can hide a real win. Total, median, p90, max, and the count of would-be-killed candidates
whose HC cost was effectively zero — that last number is what distinguishes a pass that removes
expensive candidates from one mirroring work the confirmer already skips.

**The second axis: a filter may pay by shrinking the FST, not by speeding up HC.** Precision in the
proposer is not free — it costs entry count, compile time, and memory. If a cheap filter absorbs the
over-proposals, the proposer can be built *less* precisely and get smaller. On that axis a pass that
merely duplicates an existing proposer check is not redundant; it is a candidate **replacement** for
it, and the gain appears as a smaller artifact rather than as confirmation time saved.

Two concrete instances, both testable with machinery that already exists:

- `ExploreMode::Flat` (`HC_PREEXPAND_FLAT=1`) skips the morphotactic automaton entirely and is kept
  in the source expressly for A/B measurement. Build Flat, measure the size and compile-time drop
  and the extra candidates, and ask whether the filter absorbs them for less than pruning costs. If
  it does, `StructuralTransitionPass` replaces `ExploreMode::Pruned` rather than shadowing it.
- Circumfix allomorphs are materialized as an N×M cross product at emit time. A partner-agreement
  check makes N+M viable. The agreement mechanism is only *needed* once materialization stops, which
  is exactly why it looks worthless while the cross product remains.

Neither instance is settled by the HC-time census alone; each needs a build-two-ways comparison of
artifact size, compile time, candidate count, and the filter's own cost.

Until that measurement identifies at least one constraint whose candidates are both filter-predictable
and expensive in HC, Tasks 5-11 are not authorized. Fire counts do not settle it: a pass can fire
constantly while saving nothing, which is precisely what the ownership pass would do.

### Axis 1 is measured — 2026-08-12, `8b8d8bd`, ideal-filter ceiling

Per-word distributions, three grammars, `filter_ceiling_census`. Confirmation time only; `propose` is
reported separately below because it changes the conclusion.

| grammar | n | p50 before → after | p99 before → after | p99 change | filter cost p99 |
|---|---|---|---|---|---|
| Sena | 859 | 6.269 → 0.933 ms | 354.639 → 24.557 ms | **−93.1%** | 0.678 ms |
| Amharic | 231 | 12.187 → 4.408 ms | 1508.509 → 1043.747 ms | −30.8% | 0.015 ms |
| Indonesian | 117 | 0.228 → 0.133 ms | 2.744 → 2.744 ms | **0.0%** | 0.004 ms |

**The gate is met by Sena and the cost condition is met everywhere.** Filter cost is 0.06 ms/word
mean on Sena against a 354 ms p99 — three orders of magnitude below this document's own 250ns × 4000
break-even reasoning, which was conservative by a wide margin.

Four findings that constrain what may be built next:

**Achieved saving is 0.00%, on every grammar.** These are oracle ceilings. Zero candidates were
rejected in any run, because every `TraceFact` arrives `Deferred` and a sound pass must then defer.
No number of additional passes changes that. The blocker is a *generator* that emits `Known` facts,
which is why Tasks 5-7 are now gated on that and not on this census.

**`steps` is not a portable work proxy and must not gate a decision across grammars.** Amharic runs
~13.6 ms per step (111 steps = 1508 ms); Sena ~0.024 ms per step (14994 steps = 355 ms) — a ~570×
spread. For Amharic, steps *overstate* the win: −64.9% on steps against −30.8% on time, and a 50.1%
step share against a 41.2% wall share. Decide on measured time; use steps only within one grammar.
This is the "report time, not percentages" rule one level down, and it caught a real misreading.

**Indonesian's null result WAS an artifact of the whole-chunk-only accounting rule.** Confirmed the
same day by replacing the model with a real pruned re-run (`918f8642`): the census had computed
"after" by deleting wholly-doomed chunks and copying every surviving chunk's cost through unchanged,
an assumption `shadow.rs` states outright. A genuine second `confirm_batch_attributed` call over only
the non-doomed candidates gives, on the deterministic step counter:

| Indonesian | before | after-modelled | after-measured |
|---|---|---|---|
| steps/word p99 | 33 | 33 | **5** |
| steps/word max | 158 | 104 | **16** |

The model said the tail could not move at all; it moves 33 → 5. **Every grammar's measured result
beats its modelled one**, so the ceilings above are floors, not estimates:

| grammar | p99 confirm ms: before → modelled → measured |
|---|---|
| Sena | 348.0 → 24.3 → **15.6** |
| Amharic | 1446.3 → 985.1 → **651.5** (−30.8% becomes −55.0%) |
| Indonesian | 2.115 → 2.115 → **1.479** |

**The mechanism is re-grouping, not within-chunk narrowing.** Sena has 1343 surviving chunks but the
pruned run makes only 981 parse calls: removing the doomed candidates lets surviving candidates
re-fuse into ~360 fewer calls than the survivors needed while the doomed ones were still forcing
their own grouping. So a chunk's cost is indeed fixed by its `root_key`/`union_rules` before
membership re-enters — but pruning changes *which chunks exist*, which the slack bound does not cap.
The feared opposite (pruning splitting a cross-root-set fusion and costing MORE parse calls) never
occurred: no grammar printed the FUSION BROKE line.

Controls: a third call repeating the full run's exact work prices cache warmth directly, and warmth
never dominated (Indonesian p99 −3.4% warmth against −30.1% pruning; Sena −1.9% against −95.5%). No
step-count nondeterminism appeared, and `timed_out` was 0 on all three grammars. Caveat on precision:
at n=117 Indonesian's p99 is effectively its second-worst word, and two runs of the same measurement
gave −11.6% and −30.1%; treat wall-time percentiles there as noisy and prefer the step counter within
a grammar.

**Axis 2 is now the stronger axis for Sena, on this evidence.** Once the filter works, the proposer
dominates: `propose` p99 24.875 ms against filtered confirm p99 24.557 ms, and means of 15.399 against
2.239. Sena's FST emits ~178 candidates/word at 98.1% empty buckets. Confirm-side headroom on Sena is
nearly exhausted, so the remaining gain is a smaller, sloppier proposer — the axis-2 thesis above,
now with a number behind it, and pointing away from the three failed attempts to push precision into
the network.

Two grammars are still unmeasured: Mbugwe and Aweti yield zero candidates through this path (a census
defect, not a grammar property). No decision to drop passes or shrink the backend set may rest on the
three-grammar result while the two most differently-shaped grammars are absent.

---

## Scope and sequencing

This is one filter-only project. It does not build the new FST generator and does not alter HC
semantics. It ends with a tested input contract that the generator must implement.

The private Indonesian, Sena, Amharic, Aweti, and Mbugwe projects, word lists, and oracle outputs are
immutable test inputs. Do not copy them into a worktree, derive committed fixtures from them, edit
them, normalize them in place, or commit any of their contents. Corpus tests read them through
`pg_conformance_fixtures::corpus` and must fail closed when absent.

Implementation agents work in isolated worktrees, commit their slice before handback, and run only
the minimum target named in that task. All Rust commands go through `rust/tools/pg.ps1`. The primary
agent reviews every diff and runs the merged-tip gates.

## File map

**New production modules**

- `rust/crates/pg-foma/src/candidate_filter/mod.rs` — small public façade and profile construction.
- `rust/crates/pg-foma/src/candidate_filter/model.rs` — proposal, witness, trace-unit, span, and deferred-fact types.
- `rust/crates/pg-foma/src/candidate_filter/decision.rs` — pass decisions, stable proof vocabulary, and defer reasons.
- `rust/crates/pg-foma/src/candidate_filter/pipeline.rs` — deterministic multi-pass evaluation and any-witness-survives semantics.
- `rust/crates/pg-foma/src/candidate_filter/proof.rs` — fail-closed rejection-proof verification.
- `rust/crates/pg-foma/src/candidate_filter/index.rs` — immutable grammar-derived facts shared by passes and proof verification.
- `rust/crates/pg-foma/src/candidate_filter/passes/mod.rs` — pass trait and certified pass registry.
- `rust/crates/pg-foma/src/candidate_filter/passes/structural.rs` — identity, ownership, and certain transition passes.
- `rust/crates/pg-foma/src/candidate_filter/passes/symbolic.rs` — finite partner, circumfix, slot, and co-occurrence passes.
- `rust/crates/pg-foma/src/candidate_filter/passes/local.rs` — allomorph-set, exact-span, POS/MPR, and certified local-environment passes.
- `rust/crates/pg-foma/src/candidate_filter/passes/dfa.rs` — optional compiled recognizer for stable regular trace predicates.
- `rust/crates/pg-foma/src/candidate_filter/report.rs` — deterministic counters plus compact and bounded per-witness trace sinks.

**Existing production modules**

- `rust/crates/pg-foma/src/lib.rs` — export `candidate_filter`.
- `rust/crates/pg-foma/src/tags.rs` — keep `Candidate` as the HC-facing identity; no generator trace encoding in this project.
- `rust/crates/pg-foma/src/confirm.rs` — expose the existing pin-resolution fact internally; keep confirmation behavior unchanged.
- `rust/crates/pg-foma/src/morphotactics.rs` — expose a narrow transition query from the existing morphotactic authority; do not duplicate it.
- `rust/crates/pg-foma/src/composite.rs` — add `Off | Shadow | Enforce` orchestration after proposal/peeling and before `confirm_batch_with_diagnostics`.
- `rust/crates/pg-foma/src/analyzer.rs` — extend proposal diagnostics only with counts needed to distinguish raw identities from witness-bearing proposals.
- `rust/crates/pg-rules/src/validity.rs` — extract only pure necessary predicates currently private so HC and filter use one semantic implementation.

**New focused tests**

- `rust/crates/pg-foma/tests/candidate_filter_contract.rs` — type, witness, pipeline, budget, and fail-open contract tests.
- `rust/crates/pg-foma/tests/candidate_filter_passes.rs` — positive/negative/ambiguous cases for every pass and proof category.
- `rust/crates/pg-foma/tests/candidate_filter_model_check.rs` — exhaustive small-domain comparisons with reference predicates.
- `rust/crates/pg-foma/tests/candidate_filter_shadow_gate.rs` — current-proposer shadow integration and HC disagreement detection.
- `rust/crates/pg-foma/tests/candidate_filter_oracle_survival.rs` — read-only five-language corpus gate.
- `rust/crates/pg-foma/tests/candidate_filter_dfa_equivalence.rs` — compiled recognizer versus imperative-pass equivalence.
- `rust/crates/pg-foma/tests/candidate_filter_promotion_gate.rs` — enforced-profile identity equality and non-vacuous mechanism counters.

## Task 1: Freeze the candidate/witness input model

**Files:**

- Create: `rust/crates/pg-foma/src/candidate_filter/mod.rs`
- Create: `rust/crates/pg-foma/src/candidate_filter/model.rs`
- Modify: `rust/crates/pg-foma/src/lib.rs` near the `confirm` export
- Test: `rust/crates/pg-foma/tests/candidate_filter_contract.rs`

- [ ] **Step 1: Write the failing model tests**

Add tests that construct two witnesses with the same `Candidate` identity and assert that both remain
distinct, that a known allomorph choice is non-empty, that unknown allomorph/role/span facts are
typed `Deferred` rather than sentinel IDs, that known absence differs from unknown, and
that lexical origin includes a revision for runtime roots:

```rust
#[test]
fn proposal_preserves_distinct_witnesses_for_one_identity() {
    let identity = candidate(&[10, 20, 30], 1);
    let proposal = ProposedCandidate::new(
        identity.clone(),
        vec![witness(1, &[101]), witness(2, &[102])],
    )
    .unwrap();

    assert_eq!(proposal.identity, identity);
    assert_eq!(proposal.witnesses.len(), 2);
    assert_ne!(proposal.witnesses[0].witness_id, proposal.witnesses[1].witness_id);
}

#[test]
fn runtime_origin_is_revisioned() {
    let origin = LexicalOrigin::RuntimeOverlay { revision: 42 };
    assert_eq!(origin.revision(), 42);
}
```

- [ ] **Step 2: Run the contract target and verify RED**

Run:

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_contract
```

Expected: compilation fails because `pg_foma::candidate_filter` and its model types do not exist.

- [ ] **Step 3: Implement the minimal model**

Define stable newtypes and the witness-bearing proposal:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WitnessId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonEmpty<T> {
    head: T,
    tail: Vec<T>,
}

impl<T> NonEmpty<T> {
    pub fn new(head: T, tail: Vec<T>) -> Self { Self { head, tail } }
    pub fn iter(&self) -> impl Iterator<Item = &T> { std::iter::once(&self.head).chain(&self.tail) }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LexicalOrigin {
    StaticGrammar,
    RuntimeOverlay { revision: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposedCandidate {
    pub identity: crate::tags::Candidate,
    pub witnesses: NonEmpty<CandidateWitness>,
}
```

`ProposedCandidate::new` rejects duplicate `WitnessId`s and an empty vector with a typed
`ProposalModelError`. `CandidateWitness` and `TraceUnit` carry the fields fixed by the contract;
every possibly unavailable value uses `TraceFact::Known` or `TraceFact::Deferred`. A known absent
slot/span is `Known(None)`; `Deferred` means the producer lacks the fact. No guessed or sentinel
allomorph represents unknown.
`WitnessId` uniqueness is candidate-local. Add a test showing two candidates may both use witness ID
`1`; later reports disambiguate them with the filter-assigned candidate ordinal and full identity.

- [ ] **Step 4: Run the contract target and verify GREEN**

Run the Step 2 command. Expected: all model tests pass.

- [ ] **Step 5: Commit the model slice**

```powershell
git add rust/crates/pg-foma/src/candidate_filter rust/crates/pg-foma/src/lib.rs rust/crates/pg-foma/tests/candidate_filter_contract.rs
git commit -m "feat(pg-foma): define candidate filter witness contract"
```

## Task 2: Implement deterministic multi-pass and any-witness-survives behavior

**Files:**

- Create: `rust/crates/pg-foma/src/candidate_filter/decision.rs`
- Create: `rust/crates/pg-foma/src/candidate_filter/pipeline.rs`
- Create: `rust/crates/pg-foma/src/candidate_filter/passes/mod.rs`
- Create: `rust/crates/pg-foma/src/candidate_filter/report.rs` with the sink traits and compact counters
- Modify: `rust/crates/pg-foma/src/candidate_filter/mod.rs`
- Test: `rust/crates/pg-foma/tests/candidate_filter_contract.rs`

- [ ] **Step 1: Add failing pipeline tests**

Pin these invariants with named fake passes:

```rust
#[test]
fn candidate_survives_when_any_witness_survives() {
    let filter = CandidateFilter::new(vec![boxed(reject_witness(1)), boxed(keep_all())]);
    let outcome = filter.filter(FilterMode::Enforce, one_candidate_with_witnesses(&[1, 2]), unlimited());
    assert_eq!(outcome.retained.len(), 1);
    assert_eq!(outcome.report.witnesses_rejected, 1);
    assert_eq!(outcome.report.candidates_rejected, 0);
}

#[test]
fn budget_exhaustion_passes_unvisited_candidates_through() {
    let outcome = reject_all_filter().filter(FilterMode::Enforce, three_candidates(), steps(1));
    assert_eq!(outcome.status, FilterCompletion::Incomplete(FilterStopReason::StepBudget));
    assert_eq!(outcome.retained.len(), 2);
}
```

Also assert stable pass order, first-rejection termination per witness, `Defer` retention, `Off`
bypass, `Shadow` retaining all inputs, incremental output before end-of-input, and a budget trip
switching the remaining iterator to bypass without first collecting it.
Task 2 defines the complete closed proof enum from the governing contract so the slice compiles;
its allow-list verifier is test-only. Task 3 implements the sole production verifier.

- [ ] **Step 2: Run the contract target and verify RED**

Run the Task 1 test command. Expected: the new pipeline symbols are missing.

- [ ] **Step 3: Implement the pass interface and pipeline**

Use this seam:

```rust
pub trait CandidateFilterPass: Send + Sync {
    fn id(&self) -> StablePassId;
    fn evaluate(&self, context: &FilterContext<'_>, witness: &CandidateWitness) -> PassDecision;
}

pub(crate) trait ProofVerifier: Send + Sync {
    fn verify(
        &self,
        context: &FilterContext<'_>,
        witness: &CandidateWitness,
        proof: &RejectionProof,
    ) -> Result<(), ProofVerificationError>;
}

pub enum PassDecision {
    Keep,
    Reject(RejectionProof),
    Defer(DeferReason),
}

pub struct CandidateFilter {
    passes: Vec<Box<dyn CandidateFilterPass>>,
    verifier: Box<dyn ProofVerifier>,
}

impl CandidateFilter {
    pub fn filter_into<I, R, T>(
        &self,
        mode: FilterMode,
        input: I,
        retained: &mut R,
        trace: &mut T,
        budget: FilterBudget,
    ) -> FilterCompletion
    where
        I: IntoIterator<Item = ProposedCandidate>,
        R: RetainedCandidateSink,
        T: FilterTraceSink;
}
```

The nested loop is candidate → witness → pass. A verified rejection ends that witness’s loop. The
candidate is removed only when all witnesses have a verified rejection. `filter_into` emits
retained candidates incrementally. On budget exhaustion, it changes to bypass, forwards every
remaining input unchanged, and returns `Incomplete`; a collecting convenience wrapper exists only
for tests and the current slice-based HC adapter.

Keep `CandidateFilter::new` crate-private. Task 2 tests inject a strict allow-list verifier for their
fake proofs; no public or production constructor may accept an arbitrary verifier. Task 3 supplies
the sole production `RejectionProofVerifier` and profile construction wires it unconditionally.

- [ ] **Step 4: Run the contract target and verify GREEN**

Run the Task 1 command. Expected: all pipeline tests pass, including shadow and incomplete behavior.

- [ ] **Step 5: Commit the pipeline slice**

```powershell
git add rust/crates/pg-foma/src/candidate_filter rust/crates/pg-foma/tests/candidate_filter_contract.rs
git commit -m "feat(pg-foma): add recall-preserving filter pipeline"
```

## Task 3: Add the production proof verifier and bounded per-witness death ledger

**Files:**

- Create: `rust/crates/pg-foma/src/candidate_filter/proof.rs`
- Modify: `rust/crates/pg-foma/src/candidate_filter/report.rs` to add the bounded detailed ledger
- Modify: `rust/crates/pg-foma/src/candidate_filter/decision.rs`
- Modify: `rust/crates/pg-foma/src/candidate_filter/pipeline.rs`
- Test: `rust/crates/pg-foma/tests/candidate_filter_contract.rs`
- Test: `rust/crates/pg-foma/tests/candidate_filter_passes.rs`

- [ ] **Step 1: Write failing proof and death-trace tests**

```rust
#[test]
fn invalid_rejection_proof_defers_instead_of_killing() {
    let outcome = collect_with_ledger(filter_with(forged_pairing_proof()), FilterMode::Enforce, one_candidate(), unlimited());
    assert_eq!(outcome.retained.len(), 1);
    assert_eq!(outcome.report.proof_verification_failures, 1);
    assert!(matches!(outcome.report.events[0].outcome, PassOutcome::ProofRejected(_)));
}

#[test]
fn death_record_names_the_first_fatal_pass_for_every_witness() {
    let outcome = collect_with_ledger(two_pass_filter(), FilterMode::Enforce, two_witness_candidate(), unlimited());
    let death = &outcome.report.candidate_deaths[0];
    assert_eq!(death.witness_deaths[0].pass_id.as_str(), "structural.ownership.v1");
    assert_eq!(death.witness_deaths[1].pass_id.as_str(), "symbolic.partner.v1");
}

#[test]
fn bounded_ledger_reports_omitted_deaths_without_changing_filtering() {
    let outcome = collect_with_ledger_cap(reject_all_filter(), ten_candidates(), unlimited(), 2);
    assert_eq!(outcome.ledger.candidate_deaths.len(), 2);
    assert_eq!(outcome.ledger.omitted_candidate_deaths, 8);
    assert_eq!(outcome.retained.len(), 0);
}

#[test]
fn same_witness_id_in_different_candidates_has_distinct_report_keys() {
    let outcome = collect_with_ledger(reject_all_filter(), two_candidates_each_with_witness(1), unlimited());
    assert_ne!(outcome.ledger.events[0].candidate_ordinal, outcome.ledger.events[1].candidate_ordinal);
    assert_eq!(outcome.ledger.events[0].witness_id, outcome.ledger.events[1].witness_id);
}
```

Add one forged-proof test for every `ProofCategory`. For each category, independently corrupt the
pass ID, rule ID, candidate identity, witness ID/index, grammar/lexicon revision, and category-specific
payload. Include out-of-range spans, incomplete allomorph-alternative exhaustion, stale ownership,
wrong co-occurrence key, signature mismatch, and local-environment mismatch. Every corruption must
produce `PassOutcome::ProofRejected` and retain the candidate.

- [ ] **Step 2: Run the two focused targets and verify RED**

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_contract
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_passes
```

Expected: missing proof verifier and report types.

- [ ] **Step 3: Implement stable proofs and reports**

Use closed, serializable categories and structured witnesses:

```rust
pub enum ProofCategory {
    MalformedIdentity,
    ImpossibleOwnership,
    ForbiddenTransition,
    MissingRequiredPartner,
    StaticCoOccurrenceViolation,
    NoCompatibleAllomorph,
    StaticSignatureConflict,
    ImpossibleSurfaceSpan,
    ImpossibleLocalEnvironment,
}

pub struct RejectionProof {
    pub pass_id: StablePassId,
    pub rule_id: StableRuleId,
    pub category: ProofCategory,
    pub witness: ProofWitness,
}
```

Assign monotonically increasing event ordinals during the deterministic traversal. Define
`FilterTraceSink`, a compact `CountingTraceSink`, and an opt-in `BoundedDeathLedger`. Do not
serialize wall-clock durations in the canonical ledger. Store summary counters by stable pass ID in
a `BTreeMap`, not a hash map. Reaching the ledger cap increments omitted-record counters and never
changes a filter decision.
Use checked `u64` event/candidate ordinals and a `u16` pass ordinal. Add a test-only counter seed near
`u64::MAX`; overflow must switch the detailed ledger to summary-only, set `ordinal_overflow`, retain
correct filtering results, and never wrap or collide event keys.

- [ ] **Step 4: Run both focused targets and verify GREEN**

Run the Step 2 commands. Expected: proof-forgery retains the candidate; death records are stable.

- [ ] **Step 5: Commit proof and reporting**

```powershell
git add rust/crates/pg-foma/src/candidate_filter rust/crates/pg-foma/tests/candidate_filter_contract.rs rust/crates/pg-foma/tests/candidate_filter_passes.rs
git commit -m "feat(pg-foma): verify filter rejections and record trace deaths"
```

## Task 3c: Per-pass conformance fixtures and the fire-count harness

Every pass must earn its place on evidence, not on argument. This task front-loads the slow,
judgment-heavy half of that — authoring a synthetic grammar that provokes one specific pass — so
each later pass slice lands against a fixture already waiting for it.

It deliberately precedes the passes themselves. A fixture pins HC-rust's own analyses for a
construct, which is exactly the reference a pass must not perturb, and that reference is authorable
before any pass exists.

**Files:**

- Create: `conformance-staging/filter-passes/<pass>/grammar.xml` and `words.yaml`, one directory per
  planned pass
- Create: `conformance-staging/filter-passes/<pass>/filter-expectation.json`
- Create: `rust/crates/pg-foma/tests/candidate_filter_fixture_weight.rs`
- Modify: `conformance-staging/STAGING.md` to record each new fixture and its target pass

- [ ] **Step 1: Author one fixture per planned pass**

Directories: `ownership`, `structural-transition`, `slot-order`, `co-occurrence`,
`static-signature`, `allomorph-compatibility`, `exact-span`, `local-environment`.

`partner-pairing` gets a directory containing only `filter-expectation.json` with
`"status": "not-yet-provokable"` and a note recording why: the grammar model compiles circumfix
halves into one cross-product allomorph, so no authored grammar can currently produce the partner
events that pass consumes. Recording that as data, not prose, keeps it from being mistaken for an
oversight.

Follow `.claude/skills/conformance-grammars/SKILL.md`. **Synthetic data only** — no actual-language
data, and no file, feature, or symbol named after a language. Where a fixture mimics a real
language's pathology, name the pathology and put the family in a comment, exactly as the existing
staged fixtures do.

Each grammar must provoke its target pass and, as far as practical, not the others: a fixture that
trips four passes cannot show that any one of them earns its place.

- [ ] **Step 2: Pin expected analyses against HC-rust, transcribed not hand-derived**

Drive `pg_parse::Morpher` directly and transcribe its output verbatim into `words.yaml`, following
the header convention the existing staged fixtures use — which records the oracle used and states
plainly that it is this repo's Rust engine, not the C# founding oracle. Do not hand-derive a
signature; a hand-derived expectation is a guess with a test around it.

Include, per fixture, both words the target pass must not disturb and words whose invalid analyses
it should eventually remove.

- [ ] **Step 3: Declare each fixture's expectation as data**

```json
{
  "pass_id": "structural.ownership.v1",
  "min_fire_count": 1,
  "status": "awaiting-pass"
}
```

`status` is `awaiting-pass`, `wired`, or `not-yet-provokable`. `min_fire_count` is the number of
verified rejections that pass must produce over this fixture's words once it exists.

- [ ] **Step 4: Write the harness and its anti-rot gate**

`candidate_filter_fixture_weight.rs` walks `conformance-staging/filter-passes/**` and, per fixture:

- asserts `Off` and `Enforce` yield identical deduplicated `AnalysisIdentity` sets and identical
  exact `WordAnalysis` multisets — the property that holds at every stage, including now, when no
  passes exist;
- for a `wired` fixture, asserts its declared pass fired at least `min_fire_count` times.

The anti-rot gate is the point of the `status` field and must fail loudly in three cases: a fixture
still `awaiting-pass` whose `pass_id` now exists in the pass registry; a fixture naming a `pass_id`
neither registered nor in the plan's declared pass list; and a registered pass with no fixture at
all. Without it, a fixture that never gets wired reads as coverage while asserting nothing.

- [ ] **Step 5: Run the fixture gate**

```powershell
& rust\tools\pg.ps1 -Mode conformance-test -Scope local -Package pg-foma -TestTarget candidate_filter_fixture_weight
```

Expected: every fixture parses, every expectation file is well-formed, every `Off`/`Enforce`
comparison is equal, and the anti-rot gate passes. With no passes yet built, every fixture is
`awaiting-pass` and no fire count is asserted — that is the correct state, and the gate reports it
rather than showing a vacuous green.

- [ ] **Step 6: Confirm the fixtures join the existing suite cleanly**

```powershell
& rust\tools\pg.ps1 -Mode conformance-test -Scope local
```

Expected: the pre-existing staged fixtures still pass and the new ones are discovered and validated
by the same machinery, since they live under `conformance-staging/**`.

- [ ] **Step 7: Commit the fixtures and harness**

```powershell
git add conformance-staging/filter-passes conformance-staging/STAGING.md rust/crates/pg-foma/tests/candidate_filter_fixture_weight.rs
git commit -m "test(conformance): stage per-pass filter fixtures and the fire-count gate"
```

Each later pass task adds its own fixture wiring: flip `status` to `wired`, set the real
`min_fire_count`, and let the harness enforce it.

## Task 4: Build the immutable grammar fact index and structural passes

**Files:**

- Create: `rust/crates/pg-foma/src/candidate_filter/index.rs`
- Create: `rust/crates/pg-foma/src/candidate_filter/passes/structural.rs`
- Modify: `rust/crates/pg-foma/src/candidate_filter/passes/mod.rs`
- Modify: `rust/crates/pg-foma/src/confirm.rs` at `resolve_pins`
- Modify: `rust/crates/pg-foma/src/morphotactics.rs` at `MorphotacticIndex`
- Test: `rust/crates/pg-foma/tests/candidate_filter_passes.rs`
- Test: `rust/crates/pg-foma/tests/candidate_filter_model_check.rs`

- [ ] **Step 1: Write failing structural-pass tests**

Cover root index below zero/out of range, designated root not owned by a lexical entry, unowned
non-root morpheme, valid compound extra roots, unordered strata, missing slot/stratum metadata, and
a legal transition:

```rust
#[test]
fn unknown_transition_metadata_defers() {
    let decision = StructuralTransitionPass.evaluate(&context(), &witness_without_slots());
    assert_eq!(decision, PassDecision::Defer(DeferReason::MissingTraceFact(TraceFact::Slot)));
}

#[test]
fn impossible_designated_root_has_verified_proof() {
    assert_verified_reject(
        &index(),
        OwnershipPass.evaluate(&context(), &witness_with_mrule_as_root()),
        ProofCategory::ImpossibleOwnership,
    );
}
```

- [ ] **Step 2: Run the pass and model-check targets and verify RED**

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_passes
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_model_check
```

Expected: `FilterIndex`, `OwnershipPass`, and `StructuralTransitionPass` are missing.

- [ ] **Step 3: Implement `FilterIndex` and share pin resolution**

Construct `FilterIndex` once per grammar. It owns immutable arrays/maps for morpheme ownership,
rule ownership, POS/MPR signatures, and pass capability. It must reuse the existing
`pg-foma::morphotactics::MorphotacticIndex` as the authority for slot/stratum transitions, exposing
a narrow shared query API instead of rebuilding that relation. The current grammar model has no
independent circumfix-half/partner-class identity, so `FilterIndex` must not invent one.
Change `confirm.rs` from a private `resolve_pins` to a `pub(crate)` pure fact function whose return
type exposes only the resolved root/rule/extra-root sets needed by both confirmation and structural
proof verification. Do not change `confirm_batch_impl` behavior.

- [ ] **Step 4: Implement the two structural passes**

`OwnershipPass` rejects only facts already rejected by pin resolution. `StructuralTransitionPass`
checks transitions through the shared `MorphotacticIndex`; templates, unordered strata, missing trace facts,
or unclassified rule shapes return `Defer`.

- [ ] **Step 5: Exhaustively model-check small ownership/transition spaces**

Enumerate roots/non-roots, root positions, three roles, and two ordered slots for traces of length
zero through four. Compare pass decisions against a simple exhaustive reference relation and assert:

```rust
assert!(!matches!(actual, PassDecision::Reject(_)) || !reference_accepts);
```

Expected: the test exercises at least one accept, defer, and verified reject for every structural
pass; report the counts in assertion messages to prevent vacuity.

- [ ] **Step 6: Run the focused targets and verify GREEN**

Run Step 2. Expected: all structural and model-check tests pass.

- [ ] **Step 7: Commit structural filtering**

```powershell
git add rust/crates/pg-foma/src/candidate_filter rust/crates/pg-foma/src/confirm.rs rust/crates/pg-foma/src/morphotactics.rs rust/crates/pg-foma/tests/candidate_filter_passes.rs rust/crates/pg-foma/tests/candidate_filter_model_check.rs
git commit -m "feat(pg-foma): filter impossible candidate structure"
```

## Task 5: Add finite symbolic passes

**Files:**

- Create: `rust/crates/pg-foma/src/candidate_filter/passes/symbolic.rs`
- Modify: `rust/crates/pg-foma/src/candidate_filter/index.rs`
- Modify: `rust/crates/pg-foma/src/candidate_filter/passes/mod.rs`
- Modify: `rust/crates/pg-rules/src/validity.rs` near `co_occurrence_rule_ok`
- Test: `rust/crates/pg-foma/tests/candidate_filter_passes.rs`
- Test: `rust/crates/pg-foma/tests/candidate_filter_model_check.rs`

- [ ] **Step 1: Extract and pin shared co-occurrence semantics in `pg-rules`**

Write tests around a new pure function used by both HC validity and the filter:

```rust
pub fn morpheme_co_occurrence_rules_ok(
    sequence: &[MorphemeId],
    key: MorphemeId,
    rules: &[MorphemeCoOccurrenceRuleDef],
) -> bool;
```

Preserve the existing helper’s essential `key` argument: HC can check rules owned by one morpheme
against a different candidate key, particularly on guessed-root paths. Add direct guessed-root,
disjunctive-allomorph, adjacency, and non-adjacency tests before redirecting HC to the extracted
function.

Run:

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-rules -Filter morpheme_co_occurrence
```

Expected before extraction: compilation failure for the missing public function. After extraction:
existing validity tests and the new predicate tests pass with no semantic change to HC.

- [ ] **Step 2: Write failing pairing, ordering, and co-occurrence tests**

Cover matched and mismatched circumfix classes, missing open/close halves, two independent pairs,
non-nested ordering, bounded nesting declared unsupported, required/forbidden co-occurrence,
adjacency, and missing partner metadata. Mismatched finite IDs must reject; unbounded nesting and
missing IDs must defer.

- [ ] **Step 3: Implement finite symbolic passes**

Add:

```rust
pub struct PartnerPairingPass;
pub struct SlotOrderPass;
pub struct StaticCoOccurrencePass;
pub struct StaticSignaturePass;
```

Each proof names the two trace-unit indices or the absent partner class that caused rejection. The
proof verifier independently checks those indices and their explicit partner-event provenance. A
candidate with alternative witnesses dies only if each alternative has its own verified proof.

`PartnerPairingPass` consumes only explicit `LocalEvent::PartnerOpen/PartnerClose` facts. Because the
current grammar model has already collapsed circumfix halves into cross-product allomorphs, the
legacy adapter always defers this pass and no enforced profile includes it before the generator
provides a stable `PartnerProvenanceCatalog`. Synthetic fixtures provide that catalog for algorithm
and proof-verifier tests; they do not certify production availability.

- [ ] **Step 4: Model-check finite pairing and ordering**

Enumerate all traces up to six events over two partner classes plus neutral events. Compare the
imperative passes with exhaustive stack-free reference predicates for the supported non-nested
language. Assert that unsupported nesting always defers.

- [ ] **Step 5: Run focused gates and verify GREEN**

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_passes
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_model_check
```

Expected: every finite symbolic proof category fires non-vacuously and no reference-accepted trace is
rejected.

- [ ] **Step 6: Commit finite symbolic filtering**

```powershell
git add rust/crates/pg-rules/src/validity.rs rust/crates/pg-foma/src/candidate_filter rust/crates/pg-foma/tests/candidate_filter_passes.rs rust/crates/pg-foma/tests/candidate_filter_model_check.rs
git commit -m "feat(pg-foma): add sound symbolic candidate filters"
```

## Task 6: Add allomorph, exact-span, and certified local-environment passes

**Files:**

- Create: `rust/crates/pg-foma/src/candidate_filter/passes/local.rs`
- Modify: `rust/crates/pg-foma/src/candidate_filter/index.rs`
- Modify: `rust/crates/pg-foma/src/candidate_filter/passes/mod.rs`
- Modify: `rust/crates/pg-rules/src/validity.rs` beside cached environment evaluation
- Test: `rust/crates/pg-foma/tests/candidate_filter_passes.rs`
- Test: `rust/crates/pg-foma/tests/candidate_filter_model_check.rs`

- [ ] **Step 1: Write failing ambiguity and locality tests**

Pin “reject only if all alternatives fail”:

```rust
#[test]
fn one_compatible_allomorph_keeps_the_witness() {
    let witness = witness_with_allomorphs(&[bad_allomorph(), good_allomorph()]);
    assert!(!matches!(AllomorphPass.evaluate(&context(), &witness), PassDecision::Reject(_)));
}

#[test]
fn uncertain_surface_span_defers() {
    let witness = witness_with_span(None);
    assert!(matches!(ExactSpanPass.evaluate(&context(), &witness), PassDecision::Defer(_)));
}
```

Also cover empty/non-empty spans, UTF-8/NFD boundaries, a local rewrite explicitly marked relaxed,
phonology outside the certified window, all alternatives impossible, and exact two-morpheme context.

- [ ] **Step 2: Extract a shared pure local-environment predicate**

Refactor `pg-rules::validity` so HC and the filter call the same matcher for an exact surface window.
The function accepts explicit left/focus/right spans and the already-built `RuleCache`; it does not
construct a partial `Word` or reproduce matching logic in `pg-foma`.

- [ ] **Step 3: Implement local passes with explicit capability decline**

Add `AllomorphCompatibilityPass`, `ExactSurfaceSpanPass`, and `CertifiedLocalEnvironmentPass`.
`FilterIndex` classifies every relevant rule as `CertifiedLocal`, `RequiresDeferredFact`, or
`Unsupported`. Only the first can reject. The others return a stable `DeferReason` naming the missing
fact or construct.

- [ ] **Step 4: Model-check alternative exhaustion**

Enumerate all non-empty subsets of three allomorph alternatives and every truth assignment for the
local predicate. Assert rejection exactly when every alternative is proven false; any unknown member
forces retention.

- [ ] **Step 5: Run focused gates and verify GREEN**

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-rules -Filter local_environment
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_passes
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_model_check
```

Expected: all local passes fire in negative cases, keep positives, and defer uncertain cases.

- [ ] **Step 6: Commit local filtering**

```powershell
git add rust/crates/pg-rules/src/validity.rs rust/crates/pg-foma/src/candidate_filter rust/crates/pg-foma/tests/candidate_filter_passes.rs rust/crates/pg-foma/tests/candidate_filter_model_check.rs
git commit -m "feat(pg-foma): filter impossible local allomorph traces"
```

## Task 7: Compile stable regular passes behind the same filter interface

**Files:**

- Create: `rust/crates/pg-foma/src/candidate_filter/passes/dfa.rs`
- Modify: `rust/crates/pg-foma/src/candidate_filter/passes/mod.rs`
- Test: `rust/crates/pg-foma/tests/candidate_filter_dfa_equivalence.rs`

- [ ] **Step 1: Write failing imperative-versus-DFA equivalence tests**

Enumerate the same finite partner/order/co-occurrence trace domain from Task 5. For each trace, compare
the imperative pass decision with a compiled recognizer’s proposed proof. Assert identical
keep/reject/defer classification and identical verified proof category.

- [ ] **Step 2: Run the DFA target and verify RED**

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_dfa_equivalence
```

Expected: missing `CompiledTracePass` and `TraceAlphabet`.

- [ ] **Step 3: Implement a pure-Rust table-driven DFA adapter**

Use a versioned finite alphabet derived only from stable symbolic event classes:

```rust
pub struct CompiledTracePass {
    pass_id: StablePassId,
    alphabet_version: u16,
    transitions: Vec<Vec<StateId>>,
    rejecting: BTreeMap<StateId, ProofTemplate>,
}
```

The DFA never directly deletes a candidate. It produces a structured `RejectionProof`, and the
post-hoc verification checks that proof like any other. Unknown alphabet symbols transition to a
non-rejecting defer state.

- [ ] **Step 4: Run the DFA target and verify GREEN**

Run Step 2. Expected: exhaustive equivalence passes and at least one DFA rejection is proof-verified.

- [ ] **Step 5: Commit the hybrid filter adapter**

```powershell
git add rust/crates/pg-foma/src/candidate_filter/passes/dfa.rs rust/crates/pg-foma/src/candidate_filter/passes/mod.rs rust/crates/pg-foma/tests/candidate_filter_dfa_equivalence.rs
git commit -m "feat(pg-foma): compile regular candidate filters to trace DFA"
```

## Task 8: Add the read-only five-language oracle-survival harness

**Files:**

- Create: `rust/crates/pg-foma/tests/candidate_filter_oracle_survival.rs`
- Modify: `rust/tools/corpus-manifest.json` only if the new test target must be named in `requiring_tests`; do not add corpus files or outputs
- Modify: `rust/crates/pg-foma/src/analyzer.rs` only to replace stale test comments that instruct workers to copy private corpora; direct them to `PANGLOSS_CORPUS_ROOT`
- Reuse: `rust/crates/pg-conformance-fixtures/src/corpus.rs`
- Reuse: `rust/crates/pg-foma/src/parity.rs`

- [ ] **Step 1: Write the failing corpus test harness**

For each manifest language, call `pg_conformance_fixtures::corpus::require` and load the grammar in
its declared format: Indonesian/Sena/Amharic HC XML, Aweti JSON, and Mbugwe fwdata. Resolve all
private paths through `PANGLOSS_CORPUS_ROOT`; do not use hard-coded worktree-relative copies.
Obtain authoritative analyses from a complete existing oracle source, adapt each oracle-positive analysis into a
conservative `CandidateWitness` without modifying the input. Run production-certified profiles in
`Enforce`; run `BoundaryLocalV1` and partner filtering in `Shadow` until their required generator
facts and provenance exist. `WordAnalysis` exposes morpheme IDs, root position, POS/MPR, and
provenance but not the chosen
allomorph, role, slot, stratum, exact surface spans, or local events. The adapter therefore marks
those fields `TraceFact::Deferred`; it never invents a unique value, substitutes
`AllomorphId::GUESSED`, or treats every grammar allomorph as evidence that a particular derivation
used it. Corpus certification before the generator consequently applies only to passes whose facts
`WordAnalysis` actually supplies.

The current repository gates are evidence to reuse, not substitutes for this gate: Indonesian has a
121-word parity run; Sena parity is sampled; Amharic excludes timed-out rows; Aweti is bounded and
partial; and no committed complete Mbugwe word-parity harness exists. A timeout-censored scratch TSV
is not an oracle. If a complete authoritative source for any of the five is unavailable, record that
language as `NotDetermined` and fail certification. Do not generate, repair, or commit an oracle to
make the gate pass.
Compare `AnalysisIdentity` values, not display strings:

```rust
for oracle_analysis in oracle.structured.iter() {
    let proposal = oracle_witness_adapter::from_analysis(&grammar, oracle_analysis)?;
    let filtered = filter.filter(FilterMode::Enforce, vec![proposal], unlimited());
    assert_eq!(filtered.retained.len(), 1, "oracle-positive analysis killed: {death:?}");
}
```

The failure message prints language, word occurrence, `AnalysisIdentity`, witness ID, first fatal
pass, stable rule ID, proof category, and proof witness. It must not print or write the complete
private corpus. Call `corpus::record_cases` for each executed word so `pg.ps1 -Mode corpus-test` can
distinguish a real five-language run from self-skipping tests.

- [ ] **Step 2: Prove missing corpora fail closed**

Run the existing PowerShell manifest tests:

```powershell
& rust\tools\tests\corpus-manifest.tests.ps1
```

Expected: PASS, including the existing required-mode behavior. Do not weaken skip/fail semantics.

- [ ] **Step 3: Run the five-language gate**

```powershell
& rust\tools\pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget candidate_filter_oracle_survival -TestThreads 1
```

Expected: all five logical languages execute, each reports a nonzero oracle-positive analysis count,
and zero oracle-positive witness deaths for every pass whose required facts the adapter actually
supplies. The report lists per-pass defer counts so missing trace facts cannot masquerade as
coverage. Aweti and other bounded oracle calls preserve their typed
timeout/incomplete status; an incomplete word is reported as non-certifying, never as a pass or an
empty oracle set.

- [ ] **Step 4: Commit only harness code and manifest references**

Before staging, run:

```powershell
git status --short
git diff -- rust/tools/corpus-manifest.json rust/crates/pg-foma/tests/candidate_filter_oracle_survival.rs rust/crates/pg-foma/src/analyzer.rs
```

Confirm that no file under `samples/data`, no language project, no word list, no oracle output, and no
derived language data appears. Then commit:

```powershell
git add rust/crates/pg-foma/tests/candidate_filter_oracle_survival.rs rust/tools/corpus-manifest.json rust/crates/pg-foma/src/analyzer.rs
git commit -m "test(pg-foma): preserve five-language oracle analyses through filters"
```

## Task 9: Insert the filter in shadow mode before HC confirmation

**Files:**

- Modify: `rust/crates/pg-foma/src/composite.rs` at `FomaWordDiagnostics`, every confirmation call, `from_cached`, and `into_parts`
- Modify: `rust/crates/pg-foma/src/analyzer.rs` at `ProposalDiagnostics`
- Modify: `rust/crates/pg-foma/src/tags.rs` beside `Candidate`
- Test: `rust/crates/pg-foma/tests/candidate_filter_shadow_gate.rs`

- [ ] **Step 1: Write failing shadow-integration tests**

Use a synthetic grammar and a stub filter with one known false rejection. Assert that shadow mode
still returns the HC-confirmed analysis but records a disagreement. Then use a valid rejection and
assert it records a death, sends the candidate to HC anyway, and increments nonzero fire counters.
Run the same assertions through normal, budgeted, diagnostic, detached worker/pool, and multiword
entry points. Reconstruct an analyzer through `from_cached`, and round-trip `into_parts`, to prove
filter index/profile/revision state is neither lost nor silently reset.

- [ ] **Step 2: Run the shadow target and verify RED**

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_shadow_gate
```

Expected: `FomaAnalyzer` has no filter mode/profile configuration or filter diagnostics.

- [ ] **Step 3: Add a conservative legacy-witness adapter**

The current proposer returns only `Candidate`. Add a crate-internal adapter that wraps each identity
in one witness with morpheme IDs/root position known and all allomorph/role/slot/stratum/span/local-
event facts marked `TraceFact::Deferred`. Do not infer missing data or use sentinel IDs.
Consequently, only passes whose proofs need no new generator
metadata may fire before the new generator exists.

- [ ] **Step 4: Wire the shadow pipeline**

Add filter configuration to the analyzer builder and a single crate-private `filter_then_confirm`
helper owning filter evaluation, candidate projection, `confirm_batch`/diagnostic selection, shadow
correlation, and counters. Route every public confirmation path in `composite.rs` through that helper;
no direct `confirm_batch` call remains outside the helper or confirmation unit tests. Carry the
immutable filter index/profile/revision through `from_cached` and `into_parts`.

Default production behavior remains `Off` until a profile is promoted. In `Shadow`, evaluate before
confirmation, pass the original candidate slice to HC, then correlate verified shadow deaths with
returned confirmation buckets. Any non-empty bucket for a would-die identity increments
`shadow_false_rejections` and includes its bounded death record in diagnostics.

Extend deterministic diagnostics with:

```rust
pub raw_candidate_identities: usize,
pub candidate_witnesses: usize,
pub filter_steps: u64,
pub filter_defers: u64,
pub filter_verified_rejections: u64,
pub filter_candidates_removed: u64,
pub shadow_false_rejections: u64,
pub hc_candidates_received: usize,
```

- [ ] **Step 5: Run the shadow target and verify GREEN**

Run Step 2. Expected: false-rejection detection fires in the deliberately broken case, valid shadow
rejection never changes returned analyses, and every entry point reports the same filter profile and
candidate counts. Search `composite.rs` for direct `confirm_batch` calls and verify every production
match is inside `filter_then_confirm`.

- [ ] **Step 6: Commit shadow integration**

```powershell
git add rust/crates/pg-foma/src/composite.rs rust/crates/pg-foma/src/analyzer.rs rust/crates/pg-foma/src/tags.rs rust/crates/pg-foma/tests/candidate_filter_shadow_gate.rs
git commit -m "feat(pg-foma): shadow candidate filters before HC confirmation"
```

## Task 10: Certify profiles and enable proof-verified enforcement

**Files:**

- Modify: `rust/crates/pg-foma/src/candidate_filter/mod.rs`
- Modify: `rust/crates/pg-foma/src/composite.rs`
- Create: `rust/crates/pg-foma/tests/candidate_filter_promotion_gate.rs`
- Modify: `rust/crates/pg-foma/tests/candidate_filter_oracle_survival.rs`

- [ ] **Step 1: Write failing profile and promotion tests**

Pin exact ordered pass membership for `StructuralV1`, `SymbolicV1`, and `BoundaryLocalV1`. Add a
negative test proving an internal uncertified pass cannot be selected in `Enforce` mode. Add an
`Off` versus `Enforce` comparison with two separate authorities per word occurrence:

1. equal deduplicated `AnalysisIdentity` sets through the existing parity projection; and
2. equal exact HC output multisets, including duplicate `WordAnalysis` values and duplicate rendered
   signatures.

Do not call an `AnalysisIdentity` collection a multiset: the existing parity authority deliberately
deduplicates it. Implement a test-only O(n²) `assert_word_analysis_multiset_eq` using `WordAnalysis`’s
full `Eq`, removing one matched occurrence at a time, so duplicates cannot disappear unnoticed.

- [ ] **Step 2: Run the promotion target and verify RED**

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_promotion_gate
```

Expected: certified profile construction and enforced orchestration are missing.

- [ ] **Step 3: Implement certified profile construction**

Use a closed public enum:

```rust
pub enum CandidateFilterProfile {
    StructuralV1,
    SymbolicV1,
    BoundaryLocalV1,
}
```

Profile construction returns the exact ordered pass list plus a schema version. There is no public
API for arbitrary enforcement booleans. Unsupported grammar facts cause individual-pass defers, not
profile-construction failure and not rejection.

Freeze pre-generator membership as follows: `StructuralV1` contains ownership and shared-authority
structural transitions; `SymbolicV1` adds slot order, keyed co-occurrence, and static signature
passes; `BoundaryLocalV1` adds allomorph, exact-span, and certified local-environment passes.
`PartnerPairingPass` belongs to none of them and remains internal shadow-only. A future
`PartnerAwareV1` is a new profile/version after generator provenance, never a mutation of
`SymbolicV1`.

Before the new generator exists, only a profile with nonzero firing on facts supplied by the legacy
adapter may be marked production-certified. `BoundaryLocalV1` remains shadow-only if exact spans or
allomorph choices are unavailable; synthetic firing alone does not promote it for real-language use.

- [ ] **Step 4: Prove the mechanism engages deterministically**

On committed synthetic cases and named private-corpus words, require:

```rust
assert_eq!(off_identities, enforce_identities);
assert_word_analysis_multiset_eq(&off.structured, &enforce.structured);
assert_eq!(off.signature(), enforce.signature());
assert!(enforce.report.filter_verified_rejections > 0);
assert!(enforce.report.hc_candidates_received < off.report.hc_candidates_received);
assert!(enforce.report.confirmation_steps <= off.report.confirmation_steps);
```

If batching makes calls/groups non-monotone, retain them as observations but do not fail on them;
candidate count and HC step count are the deterministic product measures. A corpus profile cannot
promote on zero firings.

- [ ] **Step 5: Run focused and corpus promotion gates**

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_promotion_gate
& rust\tools\pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget candidate_filter_oracle_survival -TestThreads 1
```

Expected: synthetic gate proves nonzero filtering; all available private oracle positives survive;
`Off` and `Enforce` identity sets, structured-analysis multiplicity, and rendered-signature
multiplicity are equal for every completed word. Any oracle timeout is
typed non-certifying evidence and blocks profile promotion for that corpus slice.

- [ ] **Step 6: Commit enforcement**

```powershell
git add rust/crates/pg-foma/src/candidate_filter/mod.rs rust/crates/pg-foma/src/composite.rs rust/crates/pg-foma/tests/candidate_filter_promotion_gate.rs rust/crates/pg-foma/tests/candidate_filter_oracle_survival.rs
git commit -m "feat(pg-foma): enforce certified candidate filter profiles"
```

## Task 11: Record the filter assessment and freeze the generator handoff

**Files:**

- Create: `docs/fst-plan/candidate-filter-assessment.md`
- Modify: `docs/superpowers/specs/2026-08-11-candidate-filter-contract.md` only to record the final certified profile versions and evidence references
- Test: `rust/crates/pg-foma/tests/candidate_filter_promotion_gate.rs`
- Test: `rust/tools/tests/candidate-filter-private-data.tests.ps1`

- [ ] **Step 1: Generate an aggregate report without corpus content**

Report per logical language and filter pass only aggregate values: completed/incomplete words,
candidate witnesses seen, defers, verified deaths, candidates sent to HC, HC confirmation steps, and
oracle-positive kills. Do not include word lists, analyses, language-project data, or reconstructed
fixtures.

- [ ] **Step 2: Write the assessment with an explicit promotion table**

For every pass record one of `ShadowOnly`, `CertifiedStructuralV1`, `CertifiedSymbolicV1`, or
`CertifiedBoundaryLocalV1`, with links to its unit/model/oracle gates and nonzero fire-count evidence.
Record the earlier validity-only prefilter NO-GO separately: it measured a narrower class and does not
invalidate trace-structural pruning.

- [ ] **Step 3: Freeze generator acceptance tests**

In `candidate_filter_promotion_gate.rs`, add a test helper that accepts any future
`IntoIterator<Item = ProposedCandidate>` and runs the complete contract suite: no duplicate witness
IDs, no missing required stable IDs, explicit deferred features, filter survival, and HC identity
containment. The test helper is unused by production code but is the required seam for the generator
plan.

- [ ] **Step 4: Add and run the merged-tip private-data boundary test**

The PowerShell test reads `rust/tools/corpus-manifest.json`, asserts none of its declared private
files is tracked under `samples/data`, and inspects `git diff --name-only <merge-base>...HEAD` against
an explicit allowlist containing only the production modules, focused tests, manifest metadata, and
design/assessment documents named in this plan. It fails if the branch tracks a language project,
word list, oracle TSV/signature, `.fwdata`, corpus-derived log, `.tmp` output, or any other path not
on the allowlist.

Run final integration in a clean isolated worktree with private inputs supplied only through
`PANGLOSS_CORPUS_ROOT`; do not use the existing dirty research worktree. The test separately checks
`git diff --cached --name-only` and `git status --porcelain --untracked-files=all`. It rejects staged
or newly untracked `samples/data/**`, `.tmp/**`, `*.fwdata`, `*-words.txt`, `*-hc.xml`, `*.tsv`,
`*.log`, and `*.pg-output.txt` artifacts. Because the integration worktree starts clean, any such
untracked path was created by this program. Existing user files in other worktrees are neither
inspected as evidence, staged, deleted, moved, nor modified.
Also assert that `candidate_filter::report` exposes sinks/serializable values but no API that opens a
file or chooses an output path.

Run:

```powershell
& rust\tools\tests\candidate-filter-private-data.tests.ps1
```

Expected: PASS with five manifest languages inspected and zero private or derived paths tracked by
the candidate-filter branch.

- [ ] **Step 5: Run the merged filter gates**

```powershell
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_contract
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_passes
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_model_check
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_dfa_equivalence
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_shadow_gate
& rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget candidate_filter_promotion_gate
& rust\tools\pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget candidate_filter_oracle_survival -TestThreads 1
& rust\tools\tests\candidate-filter-private-data.tests.ps1
git diff --check
```

Expected: all focused targets pass, the corpus gate covers all five declared languages without
committing their data, zero oracle-positive analyses die, at least one certified filter fires, and
the enforced path reduces deterministic pre-HC work on named evidence.

- [ ] **Step 6: Commit the assessment and privacy gate**

```powershell
git add docs/fst-plan/candidate-filter-assessment.md docs/superpowers/specs/2026-08-11-candidate-filter-contract.md rust/crates/pg-foma/tests/candidate_filter_promotion_gate.rs rust/tools/tests/candidate-filter-private-data.tests.ps1
git commit -m "docs: certify candidate filter and freeze generator contract"
```

## Integration review and stop condition

After all task branches are rebased and integrated, the primary agent must inspect every commit and
run the Task 11 merged-tip gates once from the clean isolated integration worktree. A fresh
independent architecture reviewer then checks:

- every recorded rejection re-derives under post-hoc verification;
- every uncertainty/budget/error path retains candidates;
- candidate death requires all witnesses to die;
- shadow and enforced reports cannot claim corpus coverage when private inputs are missing;
- no private language data or derivatives entered Git;
- HC confirmation semantics and `AnalysisIdentity` multiplicity remain unchanged;
- the future generator has one explicit witness-bearing input seam.

Stop after this review. The next project is the local FST generator; it consumes the frozen contract
and does not reopen candidate-filter semantics unless oracle evidence exposes a soundness defect.
