# Staged OpenSpec execution

This is the authoritative dependency and worktree-ownership map for the active grammar-coverage
changes. Change artifacts define behavior; this file defines dispatch and merge order.

**Current policy overlay (2026-08-23):** Correctness/representability is binary; production
readiness is graded; containment is operational. Production requires a correctness-admitted,
complete, finalized, parity-verified result at health no worse than Warning under its managed
envelope. A complete exact Error result may be attempted as developer stress evidence but remains
production-unready. Hidden developer-only `--allow-unproven` is a correctness override that may
omit valid parses and is rejected for production/publication/certification. Hidden
`--remove-size-limits` disables only internal deterministic size/work caps and retains worker
isolation, bounded I/O, external watchdog/RSS/absolute ceilings, capability checks, completion,
payload, and parity. `--no-enforce-capability` is legacy developer-only/non-production. Neither
switch makes partial, truncated, skipped, or parity-unverified output accurate.

The three-language sequence below remains the production-certification slice. In parallel, the
current developer stress loop covers Indonesian, Amharic, Aweti, Sena, and Mbugwe to establish
representability, identify backend pain, and fix regressions. An Error-level stress result may be
complete and accurate without joining the production slice; Mbugwe is deferred only from production
certification, not from this stress work.

## Active three-language backend expansion (2026-08-22)

The Indonesian, Amharic, and Aweti production-route work is staged as two changes with three
serialized implementation phases. Mbugwe is outside that production sequence but remains in the
separate five-grammar developer stress loop described above.

The historical change names `surface-fst-complete-build-envelope` and
`cover-amharic-aweti-structural-morphology` are aliases for the registered changes
`surface-compile-profile-and-templated-routing` and
`cover-circumfix-cross-product-and-infix-drop`, respectively.

1. **`surface-compile-profile-and-templated-routing`, phase A — closure certification and named
   resource envelopes.** Owns `characterization.rs`, the shared closure traversal kernels and
   production-trace/characterization regions of `preexpand.rs` and `emit.rs`, and the
   resource-attempt evidence types. It may refactor production traversal only to share transition
   semantics and emit parity evidence; it must make every closure walk terminate with an explicit
   complete or incomplete result and must not change backend selection yet.
2. **`cover-circumfix-cross-product-and-infix-drop` — templated morphology coverage.** Starts only
   after phase A releases `emit.rs`. Owns `structural_allomorph.rs`, the templated morphology regions
   of `emit.rs`, the corresponding `capability.rs` predicate, `strategy_coverage.rs`, and
   `templated_compile.rs`. It must
   preserve default-deny behavior and return a complete artifact only when no recipe, rule, subtree,
   or technical marker is missing.
3. **`surface-compile-profile-and-templated-routing`, phase B — trusted selection and realized-build
   evidence.** Starts after the morphology change. Owns `backend_selection.rs`,
   `backend_runtime.rs`, `worker.rs`, the canonical build-report schema in
   `pg-cli/src/diagnostics.rs`, the narrow finalized-payload consumption seam in
   `pg-cli/src/pack.rs`, backend-card catalog data, and their focused tests. It extends the existing
   build report rather than creating a parallel receipt, returns the finalized Foma payload from
   the contained worker, and couples the selected backend to that exact constructed artifact; it
   may not substitute another backend or hide a failed construction. Assessment/corpus results
   remain in the separate canonical assessment report linked by attempt/model fingerprint.

No two phases edit `emit.rs` concurrently. Changes to `replace.rs` or `gate.rs` are not authorized by
this staging entry; if implementation proves either necessary, update this ownership map before
dispatch. The integration branch receives all three phases before the single authoritative merged-tip
verification.

## Implementation status (2026-07-25)

Roadmap-level record of what has actually landed on `main`. Per-change `tasks.md` checkboxes are the
granular record; this is the spine-level view.

**Stage 0 — LANDED.** Characteristics profile + exhaustive default-deny characterizer + predicate
registry + envelope composition (`pg-foma/src/capability.rs`, `capability_entry.rs`); the gate runs on
real grammars and is **default-enforcing on the FST/foma path**. The legacy
`--no-enforce-capability` escape and hidden `--allow-unproven` correctness override are
developer-only/non-production; the latter may omit valid parses. Conformance-coverage cross-check
(advisory;
build-breaking flip deferred). Chain-depth budget dimension (ADR 0003) + apply-path `ApplyBudget`.
FST-health schema + evaluator (`health.rs`, `health_evaluator.rs`). Gloss-signature unit
(`pg-realize/src/signature.rs`, PROTOCOL §3-4 / R4). CI conformance gate (`.github/workflows/
conformance-ci.yml`).

**Stage 1A (reify) — SUBSTRATE LANDED, production migration open.** Content-addressed AND-OR DAG
(`plan.rs`), enumerator mirroring today's topology (`enumerate.rs`), controllable interpreter proven
apply-equivalent to `compile_gated_grammar` (`build.rs`), differential-correctness oracle proven
non-vacuous (`oracle.rs`), node purity (per-group `Replace` masks). OPEN: routing production `emit`
through `build(plan)` (task 1.3) and capability-safe plan selection (2.x).

**Stage 1B — SLICE LANDED.** Shared pattern-span lowering (`lower.rs`) + the real
simultaneous-overlap automaton intersection. OPEN: migrating `replace.rs`'s own rewrite compilation
onto the seam.

**Stage 2 — ALL 11 CONSTRUCTS LANDED (code-path availability only).** Every construct moved from unconditional fail-closed to an
honest predicate, with proposer-to-confirm containment where the oracle supports it: multi-table
(owning-table threading), RTL (reversal + recall-safe union), simultaneous (admitted non-overlap),
bounded quantifiers (`^{min,max}`), metathesis (swap relation), circumfix/null-output (fixed a real
multi-`InsertSegments` recall bug), template/truncation/reduplication (chain-depth-budgeted peel,
incl. nested), realizational + constraints (already faithful; constraints are architecturally
confirm-only), compounding (license-gated head×non-head cross product, budget-bounded; recursive
fail-closed), unordered (existing derivation-chain superset + bounded/unbounded split), MPR groups
(Append non-tracking baseline; Overwrite permanently fail-closed).

“Landed” here means that a compiler code path and its predicate boundary exist. It does not mean
that Indonesian, Amharic, or Aweti has a complete, identity-bound, trusted FST; current
three-language certification still requires the active changes' artifact and semantic gates.

**Downstream — PARTIAL.** Landed: `.pgpack` container + pack manifest (`pg-pack`: ADR 0004 feature
set, ADR 0005 trust stamp, health admission, non-gating Ed25519, validate-before-allocate); WASM
load-compat reworked to `required ⊆ provided` + trust stamp (`pg-wasm/src/pack.rs`); the
`pangloss diagnose` build/assessment reports reusing the signature + health units. **Explicitly NOT
done** (each change's own `tasks.md` is precise — **except where audited and found false, below**):

~~`add-fst-compilation-health-audit` has only its evaluator library — no preflight walker,
proposal/confirmation counts, dedup tracking, or `pangloss fst-health` command~~ — **FALSE, audited
2026-08-06.** All four exist: `preflight.rs` (27KB, 13 functions), `ProposalVolume`/`ConfirmationWork`
counts, `DuplicateAnalysisOverlap`, and the shipped `pangloss fst-health` command (19KB module doing
both preflight-only and observed modes). Genuinely missing were remedy population on the CLI's own
findings, refusal on Critical admission, and the change's own verification run. Archived; succeeded by
`recipe-scoped-fst-health`. Note the caveat this repairs: "each change's `tasks.md` is precise" is not
safe to assume — those notes were true when written and carry no timestamp.

`make-wasm-analysis-only` has NOT removed the compiler from WASM
(`PanGlossGrammar::new` still compiles from XML); `add-grammar-diagnostics` defers everything needing
a second pipeline, file artifacts, or the PowerShell/CI/skill layer; `add-reference-hermitcrab-parity`
has the Rust gloss-signature unit but zero of the C# oracle harness.

**Since then, two of those gaps closed.** `pangloss pack` writes a real `.pgpack` carrying the
persistent, indelible ADR 0005 capability-trust stamp (a `Refuse` without the hidden,
developer-only `--allow-unproven` writes no artifact at all; a developer override records
who/why/when plus every refused config, and the stamp provably
survives write→read with no field a consumer can flip). Its two payload sections are honestly-labelled
placeholders — no Rust-HermitCrab runtime-payload serializer or foma binary-memory export exists yet —
stated in the module doc and re-printed on stderr at pack time. And `harden-foma-resource-safety`'s
**watchdog now exists**: a killable compile worker (versioned protocol, validate-before-allocate
framing, `try_wait`/`kill` wall-time control, sampled RSS that is explicitly *not* a hard ceiling),
opt-in via `pangloss pack --watchdog` with the default in-process path byte-identical.

**Delanguaging — A + B + C LANDED (C with a measured caveat).** Real-language data removed and
artifacts renamed in-repo; the `machine` conformance fixtures renamed by inspected construct and pushed
(`sillsdev/machine` `conformance-framework`); Part C renamed every remaining language-named test/example
(`pg-foma`, `pg-parse`, `pg-ffi`, `tools/fst-poc`) and added a synthetic deep-affix-chain generator
(`pg-grammar-gen/src/build/chain.rs`).

**MEASURED, AND THE ANSWER IS NO FOR THE DEEP CHAIN — the real-corpus perf anchor cannot be retired.**
The grill recorded an open risk that synthetic shapes must be *proven* to reproduce the real OOM/timing
cliffs. Measured: the synthetic deep-chain shape at N=24 (the real per-zone scale) compiles in ~3ms /
1154 states and `propose()` on a deliberately worst-case query (C(24,12) = 2,704,156 raw placements)
returns in **13.5 microseconds** — no cliff. Likely cause (flagged, unproven): foma minimizes "which of
N levels fired" into a single state when the branches are bisimilar, which holds for a content-free
chain but not for real phonologically-conditioned rules plus two independent per-zone chains. So the
corpus-dependent perf tests are **renamed but still `#[ignore]`d**, each carrying a doc note naming the
gitignored fixture it still needs; none was promoted. Today's actual (small) envelope is pinned as a
non-ignored regression test rather than discarded.

**The large-cascade anchor DID reproduce, guard included.** Compile time grows 1ms → 8.6s as
roots × rules goes 3 → 384, and the production `EnumerationBudget` (whose own doc cites the historical
8.8 GB OOM) correctly tripped `EnumerationBudgetExceeded` at product = 576 after 18s — no OOM, no hang.
That validates the budget guard against its own motivating case.

**Confirm-engine (oracle) gaps found while implementing Stage 2 — all four now resolved.** None ever
caused overclaiming (confirm-only means confirm decides); they limited *end-to-end recall* for
constructs the FST proposes correctly. Outcomes:

1. **FIXED** — `pg_rules::rewrite` iterative pick-order was direction-blind (RTL behaved as LTR). Now
   direction-aware, matching C# (`IterativePhonologicalPatternRule.cs:17-48`), including the subtlety
   that analysis scans the *opposite* direction from synthesis (`AnalysisRewriteRule.cs:33`).
2. **FIXED** — `pg_rules::metathesis`'s `build_analysis_pattern` disagreed with synthesis on switch
   ordering (tag-name vs physical position) and dropped a middle context node; analysis is now a true
   inverse of synthesis.
3. **NOT OURS (a C# bug)** — a `Quantifier` used as the LHS/RHS *focus* makes C# throw
   `InvalidCastException` at `Morpher` construction (`Cast<Constraint>` over children whose
   `Quantifier` is a *sibling* class), though the DTD permits the shape. There is no C# semantics to
   match, so `width_matches` is deliberately unchanged; Rust's behavior (silently ignore the grouping,
   never crash or mis-group) is strictly safer and is pinned by a test.
4. **DID NOT REPRODUCE** — "`ana_epenthesis` finds no analysis" was a mis-attribution: the cited
   fixture returns oracle-correct results in both directions via both `rewrite::analyze` and the full
   `Morpher` pipeline. Documented as a non-finding with a regression guard.

Each outcome is pinned by a test at its site, and every fix left conformance at exact baseline.

**Measured conformance (2026-07-25, fresh release builds).** Default engine: **15 passed + 1 known
divergence** (`edge-cases/simultaneous-epenthesis-cascade`, which pins a C# synthesis-side crash bug) —
the standing baseline, unchanged by every change above. FST/foma engine under the now-default-enforcing
gate: **10 passed, 5 unexpected failures** (down from 6). Every foma refusal is the gate working as
designed — a loud capability Refuse instead of silent recall loss — and each remaining one is
principled: an `Overwrite` MPR group (permanently fail-closed, the history-dependent trap), a
`Modify`/`InsertContext` process-morph allomorph (honestly skipped, not mis-compiled), plus
metathesis/compounding shapes outside their proven boundaries. The capability registry now contains
**zero bare `FailClosedPlaceholder`s**: every `FailClosed`/`ConfigPredicate` characteristic has a real,
evidenced predicate.

**Stage 4 — REFRAMED.** `certify-four-language-matrix` is renamed `run-synthetic-conformance-matrix`
(its specs subdir likewise); the body already dropped the terminal-certification framing and the four
actual languages. The always-on CI gate it describes is the committed `conformance-ci.yml`.

**Still open — and each is blocked or large, not merely unstarted:**

- **`calibrate-fst-resource-envelopes`** — the last STAGING item, and it is *data*-blocked. Eight
  constants ship as documented placeholders (ordering-multiplicity cap 100, the health 80%-of-budget
  threshold, `MAX_QUANTIFIER_BOUND` 512, the compound-pair budget, the chain-depth ceiling, the
  rule-product threshold, and the worker wall-timeout/RSS defaults). The large-cascade axis has a
  reproducing synthetic anchor and could be calibrated now; the deep-chain axis does not (see the Part C
  measurement above). Per R6 calibration is advisory regardless: it emits evidence plus a proposed diff
  and requires a human-reviewed commit and policy-version bump — it cannot self-activate.
- **WASM compiler removal** (`make-wasm-analysis-only` 3.2/3.3, §4) — one step closer now that
  `pangloss pack` writes a genuinely reloadable foma proposer half, but still needs the consumer-side
  "reconstruct an analyzer from `.pgpack` bytes" path *and* the runtime-grammar payload serializer,
  which is itself blocked: `pg_grammar::model::Grammar` derives serde on almost none of its types and
  `pg_featstruct::Interner<FeatureStruct>` has no impl at all.
- **The C# HermitCrab oracle harness** (`add-reference-hermitcrab-parity` §§2/2A/3/4/5) — zero code
  exists; the Rust gloss-signature half is done.
- **`reify-compilation-plans`' parked follow-on** (`add-compilation-cost-planner`): the projected-cost
  model with error bounds, the committed-plan cache, and profile-guided autotuning — parked by ADR 0002
  itself until real multi-topology pressure exists.
- **`plan-construct-coverage-completion`** — **substantially executed as of 2026-07-26; this entry is
  superseded by the "Stage 3+ status" block below.** Closed since it was written: the upstream
  `constructs.txt` PR is filed (sillsdev/machine#465) and `Unmappable` is zero; the gate's
  `Proven`-only scope gap is fixed *and* found to have been hiding a real overclaim; both
  NEEDS-DECISION rows were resolved **PROVABLE** with a written record rather than deferred;
  `CircumfixOutputAction`'s census is done and split into three named gaps; `MultiTable`'s design is
  done and **withdrew the disjoint-encoding approach as the wrong fix** (it entrenches a
  false-*negative*); the unbounded-quantifier build landed with its own openspec change; and both
  finish-line gates are flipped. Still genuinely open: tasks.md §4's remaining builds
  (`Compounding.recursive`, `RightToLeftRewrite`'s extra shapes, circumfix C1/C3/C2, `MultiTable`
  4.4b, `Metathesis` RTL) — each with a written design, none blocked on a decision.
- **The `machine` submodule pointer is a live fragility, not a nicety.** It references
  `4560e9e`, which exists on the remote only as PR #465's head branch — **not** on
  `origin/conformance-framework`. It resolves today and will dangle the moment that PR merges and its
  branch is deleted. Since the coverage cross-check is now build-breaking and needs those four
  `constructs.txt` rows to resolve, a deleted branch takes the build down. Re-point on merge
  (tasks.md 2.3).
- **The upstream conformance suite still contains actual-language DATA.**
  `machine/conformance/languages/` was delanguaged by *name* only — 8 fixtures, 217 words, with real
  lexemes and glosses under construct-named directories. Deliberately not acted on: it is SIL's suite
  on a shared branch, rewriting it is outward-facing, and it would mean re-deriving every ground-truth
  signature the conformance baseline rests on. **Needs a human decision** on whether the
  synthetic-only rule was meant to reach the pre-existing upstream suite or only fixtures we author.
  Our own `conformance-staging/` fixtures are genuinely synthetic (verified).

Everything else in this spine is implemented. `partition_entries` remains the one seam not derived from
the plan (it belongs to `gate.rs`'s separate compile entry point; wiring it means merging the two entry
points — recorded, not pretended).

This spine was reorganized on 2026-07-24 to reflect the honest-capability architecture recorded in
`docs/adr/0001`–`0005`. The governing facts that reshaped it:

- **The characteristics check is the contract** (ADR 0001): a first-class, dynamic, composed gate
  that hard-fails any grammar it cannot faithfully compile. It is not a passive ledger.
- **Multi-topology is the compilation model, not an optimizer** (ADR 0002): nothing ships until it
  exists, so the compile step is refactored to the plan-reified model *as the model*, and capability
  is grown one construct at a time. There is no ad-hoc selection to preserve.
- **The capability override** (ADR 0005) is a hidden developer-only correctness switch for
  grounding refused grammars behind an indelible degraded-trust runtime signal; it may omit valid
  parses and cannot publish or certify. Resource stress is separate: `--remove-size-limits` may
  remove only internal deterministic size/work caps while retaining containment and exact-result
  checks.
- **Packaging/WASM/compat are downstream**: with nothing shipping yet, they trail the compilation
  spine.

## Hard rule: synthetic data only

Every conformance grammar and fixture uses **synthetic data only** — invented lexemes, segment
inventories, and rules. **No actual-language data, ever.** Files and directories are **not** named by
language; features/knobs are **not** named by language; a language **family or typology** may appear
**only in a comment** for context, never in a filename or identifier. Each FST compilation module is
named by **the parts it composes** (or another disambiguating scheme), never by a language. This rule
governs new work and is a standing review criterion for existing fixtures.

## Deployment domains

Inference deployments (browser/WASM, word processors, and native C hosts) consume precompiled
packages for analysis, spell checking, and glossing. Native build deployments (FieldWorks and AI
frameworks through the C ABI/CLI) additionally compile, audit, and compare grammars. Native
reference validation is a separate explicit CLI/PowerShell lane using pinned C# Machine for HC XML.
Every build exposes a versioned capability profile; WASM contains no compiler. PanGloss emits build
artifacts, reports, and investigation handoffs but never launches FieldWorks or owns caller history,
publication policy, or diagnostic UI.

## Lifecycle ownership

Grammar and stems are source. Compilation is gated by the characteristics check and creates an
immutable build report with capability disposition and FST-health diagnostics; a caller-supplied
word run creates a separate immutable assessment report. Compilation may remain in memory for
iterative tests. Release optionally writes one data-only `.pgpack` PanGloss Language Pack; packages
contain no executable extensions and carry a capability-trust status (proven, or overridden/unproven
per ADR 0005). The C# Machine utility is maintained as source-only conformance/investigation tooling,
not shipped product.

---

## Stage 0 — the contract and foundations (parallel)

### 0A. Capability characteristics check — the contract *(NET-NEW, keystone)*

`add-capability-characteristics-check`: the load-bearing gate (ADR 0001). Projects a grammar + stem
data into a characteristics profile, composes the capability envelope from per-stage/interaction
predicates, and **hard-fails** any not-proven-faithful configuration with a typed diagnostic. Owns:
the profile/envelope/predicate types, the default-deny characterizer (no catch-all — adding a
`model.rs` variant breaks the build), the **capability override + unproven-trust runtime signal**
(ADR 0005), and the **conformance-coverage CI gate** (supported ⟺ a passing in-repo
`machine/conformance/` fixture, else the build breaks). Its first act marks every unproven
configuration — including `MorphRuleDef::Compounding`, `MorphRuleOrder::Unordered`, and `MprGroup` —
**fail-closed**. Everything else sequences behind this.

### 0B. Coverage ledger — evidence into the gate

`define-grammar-coverage-contract`, **demoted to evidence role**: the one-time audited ledger over
the frozen `pg-grammar/src/model.rs`, oracle records, and proposer-to-confirm containment gates feed
*into* the Stage 0A gate. The ledger is not itself the gate.

### 0C. Resource-safety foundation

`harden-foma-resource-safety`, budget/error foundation: validated configuration, cumulative logical
trackers, checked operations, typed outcomes, pre-allocation reservations. **Extension:** add the
derivation/unapplication **chain-depth** dimension (ADR 0003) that deterministically closes stack
overflow (the Aweti 24-level chain, the 1 GiB-stack workaround). Owns `compose_budget.rs` and new
budget types.

### 0D. FST compilation-health contract

`recipe-scoped-fst-health` (successor; `define-fst-compilation-health` archived 2026-08-08 with its
schema shipped and its six open tasks carried over). Rust-owned finding schema, stable codes,
severity/readiness semantics, size bands, and the boundary between compilation health (cost axis)
and linguistic grammar quality. Distinct from the capability-trust axis (ADR 0005): health Error
is production-unready but may be stress-attempted when complete; correctness Critical remains a
trusted-output refusal. The open half is scoping a finding to the backend that produced it,
populating remedies, and recalibrating the size bands — which are a stated target, not a measurement.

### 0E. Reference oracle harness — pulled early

`add-reference-hermitcrab-parity`: **shrunk 2026-08-08 from a standing harness to an on-demand
procedure** — the upstream adapter (`machine/conformance/adapters/hc-dotnet-wrapper.sh`) and the
already-oracle-authored fixtures cover the consuming case, so what remained was documenting how to
reach the oracle when a NEW fixture needs ground truth. That now lives in
`.claude/skills/conformance-grammars/SKILL.md`; the submodule's sparse checkout omits `machine/src`,
which is why the oracle looks unavailable by default. The original text follows for context. It is
investigative evidence, never the gate itself.

## Stage 1 — the compilation refactor

### 1A. Reify compilation plans *(NET-NEW)*

`reify-compilation-plans`: the "massive refactor" of the compile step to the plan-reified
multi-topology model (ADR 0002). Replaces hardcoded topology branching
(`should_run`/`probe_would_refuse`/`partition_entries`) with first-class enumerable plans,
capability-safe selection, and the **differential-correctness oracle** (build ≥2 plans, assert
identical confirmed sets). The cost-model, projected-cost error bounds, committed-plan cache, and
profile-guided autotuning are a **parked follow-on** (`add-compilation-cost-planner`) triggered by
real multi-topology pressure.

### 1B. Shared pattern/environment lowering

`lower-fst-pattern-environments`: the one shared IR/lowering seam (`pg-foma/src/lower.rs`) the
reified model and every Stage-2 construct build on. Compiler plumbing, grants no new capability.

### 1C. Production-emitter compile profile

`profile-fst-compilation`: compile-time stage instrumentation feeding cost signals. Owns compile
events and `emit.rs` instrumentation.

## Stage 2 — constructs, one at a time, on the new model

Each construct ships its **full kit**: a configuration-predicate capability boundary (never
variant-level), oracle witnesses, a synthetic `machine/conformance/` fixture, big-O characterization
+ resource thresholds, a per-construct runtime-feature declaration (ADR 0004; default: fully lowered,
contributes nothing), and diagnostics. A construct is promoted from fail-closed to supported only via
the Stage 0A gate. Merge in an order that respects `replace.rs`/`gate.rs` single-ownership:

1. `fix-multitable-fst-compilation`
2. `compile-right-to-left-rewrites` — **rewrite spec to config-predicate granularity**
3. `compile-simultaneous-rewrites` — **surface the subrule-overlap predicate as an explicit
   requirement** (ADR 0001's own worked example)
4. `compile-bounded-fst-quantifiers`
5. `compile-fst-metathesis`
6. `cover-circumfix-null-output-actions`
7. `cover-template-truncation-reduplication` — wire reduplication-peel as an ADR 0004
   required-runtime-feature and an ADR 0003 chain-depth/allocation-budgeted apply op
8. `cover-realizational-morphology-constraints`
9. `cover-compounding` *(NET-NEW — parity hole, no prior owner)*
10. `cover-unordered-morph-rules` *(NET-NEW — `MorphRuleOrder::Unordered`, no prior owner)*
11. `cover-mpr-groups` *(NET-NEW — `MprGroup`, no prior owner)*

The coverage ledger has one integration owner. Semantic worktrees produce evidence/row-update
fragments; the integration owner applies ledger changes after merge.

### Stage 2+ gate: replacement-cascade compilation profile

`profile-fst-compilation` Phase B is blocked until Stage 2 wires the replacement-rule cascade into
the production network constructor used for lookup. Before that switch, cascade metrics are labeled
`experimental_composition`.

## Stage 3 — interaction and scale

- `add-pairwise-grammar-interaction-coverage` runs against a pinned post-Stage-2 ledger version and
  **feeds its proven/declared pairs back into the capability manifest** (ADR 0001: proving
  orthogonality retires combination space; a declared pairwise-only limitation is stamped, not
  hidden).
- `calibrate-fst-resource-envelopes`: final sweeps after every Stage-2 construct, production
  cascade/profile, and pairwise gate merge. **Extension:** explicitly instrument and calibrate the
  ADR 0003 chain-depth and pre-allocation logical-memory dimensions. Governance: evidence + proposed
  diff + human-reviewed committed policy version; no automatic write-back. Also serves as ADR 0002's
  periodic re-validation of projected-cost estimates.

### Stage 3+ status, 2026-07-26 — BOTH FINISH-LINE GATES ARE FLIPPED

The plan below is largely executed. What actually happened, since the plan's own prose now reads as
future tense:

**Both gates are build-breaking.** `conformance_coverage_gate.rs` asserts zero `Uncovered`/`Unmappable`
across all 20 `CharacteristicKind`s (tasks.md 6.2); `plan_interaction_coverage_gate.rs` asserts zero
uncovered required adjacency tuples (6.3). Each was proven to bite by sabotage-then-revert, and each
asserts its own non-vacuity so a shrunken report cannot pass trivially.

**Three prerequisite gates had to be built first**, because the naive flip would have enshrined an
overclaim rather than prevented one:
- `coverage_citation_liveness.rs` — a `FailClosed` row's `Covered` verdict rests entirely on a
  hand-written citation string; nothing had checked those resolved. It caught a real dangling citation
  on its first encounter with live work.
- `exercises_tag_liveness.rs` — three fixtures tagged *characteristic names* where a `constructs.txt`
  **row id** is required, so their evidence had been counting for nothing, silently.
  `constructs.txt`'s own header explains how that went unnoticed: an unknown tag is "a soft
  warning… never a hard error."
- `structural_witness_gate.rs` — four row ids are each mapped by TWO characteristics, so the finer one
  could report `Covered` on the coarser sibling's evidence. Three now have a mechanized grammar-shape
  witness; the fourth pair is excluded by derivation. Reasoning:
  `docs/conformance/shared-construct-id-analysis.md`.

**The governing rule that came out of it:** *a green build-breaking gate that can silently start lying
is worse than an advisory report, because the green light is what gets cited.* 20/20 Covered was true
before the witnesses existed — and still not sufficient to flip.

**Two things will never close, and are not gaps.** `MprGroupOverwrite` is a permanent carve-out
present in **all three** reference grammars (`docs/benchmark-matrix.md`), so no reference grammar can
ever clear the `--engine=foma` gate without the hidden developer-only ADR 0005
`--allow-unproven` override — by design, not by omission. That override may omit valid parses and
cannot publish or certify. And `SimultaneousRewrite`'s overlapping-subrule configuration stays
oracle-blocked until a real `hc.dll` harness exists, which ADR 0001 itself names.

**Do not read row-level coverage as completeness.** `Covered` means "evidenced at its own
disposition," never `Admit` (ten rows are `ConfigPredicate`, three `ConfirmOnly`) and never
"every configuration inside the row is closed." The open configuration work is
`plan-construct-coverage-completion` tasks.md §4, with per-item designs in
`docs/conformance/{circumfix-structural-composite-census,needs-decision-resolutions,multitable-shared-representation-design}.md`.

### Stage 3+ — construct coverage completion (successor to Stage 2/3's per-construct work)

`plan-construct-coverage-completion` *(NET-NEW, planning/design only)*: the consolidated plan — read
from `pangloss coverage`'s own ledger output and `plan_interaction_coverage.rs`'s tuple/retirement
model, not assumed — for taking every remaining construct to full, evidenced coverage. Defines the
promotion ladder once (ADR 0001: `ConfirmOnly` is a legitimate permanent rest, `Admit` is a separate
optional optimization, never required); tables all 14 non-`Proven` `CharacteristicKind`s with a
PROVABLE/NEEDS-ORACLE/PERMANENT-CARVE-OUT/NEEDS-DECISION verdict each; shows how the reified plan
tree's closed 7-tuple adjacency set plus its two proven orthogonality retirements bound the fixture-
authoring obligation to linear-in-open-gaps rather than combinatorial; schedules the upstream
`sillsdev/machine` `constructs.txt` PR the 4 `Unmappable` kinds are blocked on; separates which
promotions need the (still-unstarted) C# oracle harness from which can proceed against this repo's own
confirm engine; and states the crisp definition of done — including fixing `conformance_coverage.rs`'s
own `Proven`-only scope gap before flipping the cross-check from advisory to build-breaking, the actual
finish line. Hands off PROVABLE items to future one-construct-one-kit worktrees and NEEDS-DECISION
items to a human/architect decision record; implements no `.rs` change itself.

## Stage 4 — correctness proof (always-on CI, not a terminal audit)

`run-synthetic-conformance-matrix` is **reframed** (rename target: `run-synthetic-conformance-matrix`):
there is no terminal "certification" stage and no external reference languages. Correctness is proven
by **conformance integration tests over the in-repo synthetic `machine/conformance/` grammars**,
diffing the current engine against committed oracle-authored ground truth, enforced as the Stage 0A
CI gate. Actual-language data (Sena/Amharic/Indonesian/Aweti) is **not** migrated in; typological
coverage is expanded only with synthetic fixtures named by construct/composition (see the hard rule
above).

## Stage 5 — language readiness: make the compiler's answer visible and formal *(historical roadmap; superseded for current shipping by the active three-language section above)*

Requested directly. Four deliverables that turn "the compiler works" into "this language will work on a
device, and here is the evidence". Two changes, because visualization is genuinely useful alone while
the other three form a dependency chain.

`visualize-compilation-plan` *(NET-NEW)*: serialize a `Plan` to versioned JSON, render it as mermaid.
This is what ADR 0002's reified plan was for — the DAG already exists and every node's identity is a
content address, so a diagram is stable across runs and diffable between grammar revisions **for
free**. Nodes are labelled by the linguistic work they do (stratum, template, rule class, construct),
not by node kind, because the question is "how is my language handled". Two constraints that keep it
honest: a plan over a realistic lexicon must summarize rather than emit an unreadable graph *and say it
summarized*; and a node's capability verdict must be rendered from the real evaluation, never inferred
from the node's presence — a node exists in the plan whether or not it was admitted, so an
inferred-from-presence label would make the picture lie more persuasively than prose could.

`certify-language-readiness` *(NET-NEW)*: timing → thresholds → report.
1. **Timing in the conformance suite**, both engine modes, CSV plus a markdown table, with speedup
   attributable **per typology** — the fixtures are named by construct, so that is the question worth
   answering. Two measured constraints from `docs/benchmark-matrix.md` are folded in as requirements:
   `elapsed_ms` is integer milliseconds so a sub-ms result must never be emitted as `0`, and a
   capability refusal must be recorded as its own outcome rather than a zero or a missing row — since
   refusal is the *common* case on the compiled path, not an edge case.
2. **A tiered certification** over declared, versioned thresholds (size, lexicon scale, token analysis
   rate, p50/p90/p99 against a named device class). The tiering is the design's core: **not-yet**
   (thresholds missed — the language team can act) is kept distinct from **not-supported** (a refused
   construct — only compiler work moves it), and the second tier names the refusing predicate. That
   second tier is the point: it flags languages that need support so someone can ask for it.
3. **`pangloss make-report`**: one markdown file per language — build time, size, percentiles, the plan
   diagram, and the conformance verdict with **every failing point named**, because a bare "not passing"
   is useless to someone deciding whether to ask for support.

**The honesty rules here are load-bearing and specified as requirements, not aspirations.** A
certificate is a green light, and green lights get cited — the same reasoning that gated the
conformance-gate flip. So: a `trust=unproven` pack (ADR 0005 override) can **never** certify, or the
override becomes the shortest path to a stamp and the stamp becomes decorative; "held out of authoring"
is recorded as an **attestation with an attestor**, because nothing in the artifact can verify it and a
check that cannot fail is not a check; coverage is worded as a **token-level analysis rate**, never as
accuracy, since a token can receive a wrong analysis and still count; and an unassessed check must
never render as a passed one.

**Expected first result, stated up front so it is not mistaken for a bug:** per
`docs/benchmark-matrix.md`, all three reference grammars are currently refused on the compiled path,
two of them by a permanent carve-out. Under an honest bar **none of them certifies today**. That is the
correct output of a bar set honestly, and the change must not be softened to produce cheerful output.

## Downstream — post-multi-topology, pre-ship

These assume shippable packs and trail the compilation spine:

- `make-wasm-analysis-only` — **reworked** to ADR 0004's `required ⊆ provided` append-only runtime
  compatibility (replacing the monolithic engine-compatibility-identifier equality check), adding the
  manifest required-runtime-feature-set field and the ADR 0005 capability-trust stamp. Reconcile the
  manifest's FST-health admission field with `add-fst-compilation-health-audit`.
- `.pgpack` packaging/release path.
- `add-grammar-diagnostics` — **fix** the apply-path containment to ADR 0003's in-process cooperative
  magnitude budgets (not "the watchdog", which is compile-only).
- `add-fst-compilation-health-audit`.
- `reconcile-deep-truncation-baseline` — folds into a synthetic deep-truncation-chain-shaped
  conformance fixture + a construct-driven target; the honest 32/104 floor and non-comparable
  68/104 history are preserved as provenance, not as actual-language data.

## Merge hotspots

- `replace.rs` / `gate.rs`: one semantic owner at a time.
- `emit.rs`: one owner at a time across profiling and semantic compiler work.
- `lower.rs`: shared IR seam owner (Stage 1B) before Stage-2 constructs consume it.
- Plan-reification core: one owner (Stage 1A) before constructs are authored on the new model.
- `preexpand.rs` / `peel.rs` / `morphotactics.rs`: one Stage-2 morphology owner at a time.
- Coverage ledger: one integration owner.
- Characteristics-check gate + capability manifest: one contract owner; construct work contributes
  registered predicates and conformance fixtures.
- `analyzer.rs`: diagnostic event producer; apply-budget adapters consume its stable API.
- `composite.rs`: terminal-outcome routing owner.
- `pg-wasm`: one owner at a time across stem-input integration and the analysis-only boundary.

## Initial parallel dispatch set

After prompts name exact file ownership, these can start in parallel once the planning commit lands:

1. Capability characteristics-check types + default-deny characterizer skeleton (0A first unit)
2. Coverage ledger schema/inventory (0B first unit)
3. Resource budget/error foundation incl. chain-depth (0C)
4. FST compilation-health schema and golden findings (0D)
5. Reference gloss/analysis-signature goldens (0E)

All work touching `replace.rs`, `gate.rs`, or overlapping `emit.rs` regions remains serialized.
Missing `machine`, private/gitignored corpora, an oracle executable, or a platform runner never
freezes this queue: the owning agent records the unavailable evidence as `not_run` and continues
every self-contained task.
