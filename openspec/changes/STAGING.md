# Staged OpenSpec execution

This is the authoritative dependency and worktree-ownership map for the active grammar-coverage
changes. Change artifacts define behavior; this file defines dispatch and merge order.

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

`certify-four-language-matrix` is **reframed** (rename target: `run-synthetic-conformance-matrix`):
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
- `reconcile-aweti-baseline` — folds into a synthetic Aweti-shaped conformance fixture + a
  construct-driven target; the honest 32/104 floor and non-comparable 68/104 history are preserved as
  provenance, not as actual-language data.

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
