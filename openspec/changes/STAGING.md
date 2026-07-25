# Staged OpenSpec execution

This is the authoritative dependency and worktree-ownership map for the active grammar-coverage
changes. Change artifacts define behavior; this file defines dispatch and merge order.

## Implementation status (2026-07-25)

Roadmap-level record of what has actually landed on `main`. Per-change `tasks.md` checkboxes are the
granular record; this is the spine-level view.

**Stage 0 — LANDED.** Characteristics profile + exhaustive default-deny characterizer + predicate
registry + envelope composition (`pg-foma/src/capability.rs`, `capability_entry.rs`); the gate runs on
real grammars and is **default-enforcing on the FST/foma path** (`pg-cli`: `--no-enforce-capability`
escapes, `--allow-unproven` overrides per ADR 0005). Conformance-coverage cross-check (advisory;
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

**Stage 2 — ALL 11 CONSTRUCTS LANDED.** Every construct moved from unconditional fail-closed to an
honest predicate, with proposer-to-confirm containment where the oracle supports it: multi-table
(owning-table threading), RTL (reversal + recall-safe union), simultaneous (admitted non-overlap),
bounded quantifiers (`^{min,max}`), metathesis (swap relation), circumfix/null-output (fixed a real
multi-`InsertSegments` recall bug), template/truncation/reduplication (chain-depth-budgeted peel,
incl. nested), realizational + constraints (already faithful; constraints are architecturally
confirm-only), compounding (license-gated head×non-head cross product, budget-bounded; recursive
fail-closed), unordered (existing derivation-chain superset + bounded/unbounded split), MPR groups
(Append non-tracking baseline; Overwrite permanently fail-closed).

**Downstream — PARTIAL.** Landed: `.pgpack` container + pack manifest (`pg-pack`: ADR 0004 feature
set, ADR 0005 trust stamp, health admission, non-gating Ed25519, validate-before-allocate); WASM
load-compat reworked to `required ⊆ provided` + trust stamp (`pg-wasm/src/pack.rs`); the
`pangloss diagnose` build/assessment reports reusing the signature + health units. **Explicitly NOT
done** (each change's own `tasks.md` is precise): `add-fst-compilation-health-audit` has only its
evaluator library — no preflight walker, proposal/confirmation counts, dedup tracking, or
`pangloss fst-health` command; `make-wasm-analysis-only` has NOT removed the compiler from WASM
(`PanGlossGrammar::new` still compiles from XML); `add-grammar-diagnostics` defers everything needing
a second pipeline, file artifacts, or the PowerShell/CI/skill layer; `add-reference-hermitcrab-parity`
has the Rust gloss-signature unit but zero of the C# oracle harness.

**Since then, two of those gaps closed.** `pangloss pack` writes a real `.pgpack` carrying the
persistent, indelible ADR 0005 capability-trust stamp (a `Refuse` without `--allow-unproven` writes no
artifact at all; an override records who/why/when plus every refused config, and the stamp provably
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
- **The capability override** (ADR 0005) lets a refused grammar force-compile, load, and run behind
  an indelible degraded-trust runtime signal — the on-ramp for promoting each construct.
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

`define-fst-compilation-health`: Rust-owned finding schema, stable codes, severity/override
semantics, size bands, and the boundary between compilation health (cost axis) and linguistic
grammar quality. Distinct from the capability-trust axis (ADR 0005).

### 0E. Reference oracle harness — pulled early

`add-reference-hermitcrab-parity`: the C# HermitCrab oracle harness that **authors** conformance
ground truth (gloss/analysis signatures) and supplies investigative parity evidence. Pulled into
Stage 0 because the 0A conformance gate depends on oracle-authored fixtures existing. It is
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

## Stage 4 — correctness proof (always-on CI, not a terminal audit)

`run-synthetic-conformance-matrix` is **reframed** (rename target: `run-synthetic-conformance-matrix`):
there is no terminal "certification" stage and no external reference languages. Correctness is proven
by **conformance integration tests over the in-repo synthetic `machine/conformance/` grammars**,
diffing the current engine against committed oracle-authored ground truth, enforced as the Stage 0A
CI gate. Actual-language data (Sena/Amharic/Indonesian/Aweti) is **not** migrated in; typological
coverage is expanded only with synthetic fixtures named by construct/composition (see the hard rule
above).

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
