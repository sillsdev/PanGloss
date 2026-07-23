# Staged OpenSpec execution

This is the authoritative dependency and worktree-ownership map for the active grammar-coverage changes. Change artifacts define behavior; this file defines dispatch and merge order.

## Deployment domains

Inference deployments (browser/WASM, word processors, and native C hosts) consume precompiled
packages for analysis, spell checking, and glossing. Native build deployments (FieldWorks and AI
frameworks through the C ABI/CLI) additionally compile, audit, and compare grammars. Native
reference validation is a separate explicit CLI/PowerShell lane using pinned C# Machine for HC XML.
Every build exposes a versioned capability profile; WASM contains no compiler. PanGloss emits build
artifacts, reports, and investigation handoffs but never launches FieldWorks or owns caller history,
publication policy, or diagnostic UI.

## Lifecycle ownership

Grammar and stems are source. Compilation creates an immutable build report with FST-health
diagnostics; a caller-supplied word run creates a separate immutable assessment report. Build-report
and assessment-report comparison are separate operations, while CLI conveniences may compose them.
Compilation may remain in memory for iterative tests. Release optionally writes one data-only
`.pgpack` PanGloss Language Pack; packages contain no executable extensions. The PanGloss SDK adds
`pangloss-build` beside and dependent on the exact Runtime build bundled and tested with that SDK.
An external patch-level Runtime may substitute only after the declared ABI/package compatibility
check passes; major/minor lines match and there is no initial old-runtime targeting. The C# Machine
utility is maintained as source-only conformance/investigation tooling, not shipped product.

## Stage 0 — parallel foundations

### 0A. Coverage contract

`define-grammar-coverage-contract`: ledger schema/inventory, oracle identity and containment library,
then migration of named Phase-C/Aweti gates. These are serial merge units inside the change.

### 0B. Resource-safety foundation

`harden-foma-resource-safety`, budget/error foundation: validated configuration, cumulative logical
trackers, checked operations, and typed outcomes. This may run in parallel with 0A.

Coverage owns ledger/schema/test-support files. Resource safety owns `compose_budget.rs` and new
budget types.

### 0C. FST compilation-health contract

`define-fst-compilation-health` defines the Rust-owned finding schema, stable codes, severity and
override semantics, FST-payload size bands, and the boundary between compilation health and
linguistic grammar quality. It may proceed with 0A/0B and owns no compiler instrumentation.

## Stage 1 — diagnostic foundation and trustworthy baseline

### 1A. Rust grammar diagnostics

`add-grammar-diagnostics` depends on the Stage 0 coverage schema and worker-watchdog contract.
It owns the separate Rust build/assessment report schemas, single-grammar CLI, gloss output, shared diagnostic events,
PowerShell/incoming orchestration, CI smoke test, and diagnostic skill.

Schema, CLI, PowerShell, rendering, and self-contained tests may proceed against the Stage 0
contract. Potentially adversarial diagnostic compile/parse execution—including Aweti—uses the Stage
1E compiler-worker watchdog and effective policy when that execution is reached. Diagnostics never
substitutes its own cap or runs an uncapped Morpher; unavailable external evidence is recorded as
`not_run` while independent implementation continues.

### 1B. Production-emitter compile profile

`profile-fst-compilation` Phase A depends on 1A's report/event schema. It profiles the active
`emit_with_budget`/lexc production path only: top-line compile time, emitter/probe/lexc stages,
per-template lines, final states/arcs, and resource outcomes.

It exclusively owns compile events and `emit.rs` instrumentation. Diagnostics consumes these
events; it does not add competing emitter/build counters.

### 1B2. FST compilation-health audit

`add-fst-compilation-health-audit` consumes 0C plus the Stage 0 budget APIs and Stage 1B profile
events. Rust performs preflight and observed health evaluation, emits canonical JSON/Markdown and
compiler warnings, and supplies artifact admission metadata. It reuses measurements; it does not
recalculate profile metrics or judge linguistic quality.

### 1C. Reference HermitCrab parity

`add-reference-hermitcrab-parity` depends on 1A's Rust gloss-signature API. It is an otherwise
parallel harness lane: HC-XML-only `gloss-batch`, real `-i/-s` invocation, and duplicate-sensitive
gloss-chain/surface-shape multiset comparison.

### 1D. Aweti baseline

`reconcile-aweti-baseline` depends on the coverage gate library and shared diagnostic-event API. It
owns the exact Aweti manifest, shared Aweti network constructor, gate adaptation, bare-root evidence,
and Aweti-specific measurement. Historical 68/104 remains non-comparable; existing 32/104 remains a
word-level floor until replaced by exact manifest evidence.

### 1E. Compiler-worker safety

After the typed schema lands, the single-worker watchdog and terminal production routing proceed.
Resource safety owns the worker protocol, standard-library timeout/kill loop, sampled-RSS monitor,
bounded IPC, and `composite.rs` routing on Windows and Linux. It does not own process-tree, Job
Object, cgroup, Tokio, or `processkit` infrastructure. Diagnostics only serializes outcomes. Work
proceeds independently, while unsafe real-grammar executions use the watchdog rather than run uncapped.

### 1F. WASM analysis boundary

`make-wasm-analysis-only` removes FST compilation from the WASM dependency graph and public API,
then loads one-file validated analysis packages produced by the native compilation authority. Each
package binds the precompiled FST proposer to the runtime grammar data consumed by the Rust
HermitCrab port. Artifact contract and loader work may begin after the foma binary-memory contract
is pinned; production artifact generation depends on the Stage 1E native worker watchdog.

Artifact publication also consumes the Rust FST-admission result. Warning packages publish normally;
Error packages require an explicit recorded override; Critical packages cannot publish.
Preflight distinguishes correctness from prediction: possible analysis loss fails closed, while
recall-preserving work with unknown growth is attempted under the shared watchdog and logical
budgets. Cost uncertainty alone is not Critical.
Resource termination never triggers automatic retry or limit escalation. Diagnostics return the
effective envelope, reached metric, partial measurements, and grammar-first remedies; a caller may
start an explicit new attempt using a larger named, versioned envelope.
Deterministic work counters provide the normal early cutoff and construct attribution. Cooperative
elapsed checks and the parent wall timeout remain outer safeguards for stalls and uninstrumented
native work, without imposing an identical elapsed-time promise across platforms or machines.
Exact values and proven conservative lower bounds reserve cumulative logical work before material
allocation and stop operations that provably cannot fit. Heuristic estimates may warn but never
reject by themselves; uncertain work proceeds under actual counters.
Potentially meaning-changing grammar improvements are advisory only. Automatic compiler lowering or
optimization requires semantics-preservation evidence owned and verified by the relevant compile
change; applied internal transformations are recorded in profile evidence.
The pipeline preserves PanGloss's propose-and-confirm architecture: recall-preserving FST
overapproximation is valid, and Rust HermitCrab filters false positives. Candidate/path volume,
confirmation work, and rejection share remain first-class health dimensions.
Apply-budget exhaustion is atomic per word: completed batch members remain valid, but the exhausted
word returns only a typed incomplete outcome plus diagnostic evidence. A caller may explicitly retry
the incomplete subset with larger selected apply limits; no automatic retry occurs.
An optional caller-selected cumulative batch budget preserves completed results and distinguishes a
started incomplete word from remaining not-attempted words, so callers can resume either subset.
Every configurable resource dimension also has a versioned, hard-coded, deliberately high absolute
ceiling. Defaults, named envelopes, hosts, and callers may select only lower effective values; no
unlimited mode exists. Calibration keeps normal fail-fast limits distinct from emergency ceilings.
Runtime application has one budget schema and absolute ceiling set across Windows, Linux, and WASM.
Apps select lower normal or retry values from that same contract; ordinary defaults target user PCs.

This lane is not a compiler-coverage lane. WASM uses logical apply budgets but does not need or
emulate a child-process compile supervisor. The separately in-flight stem-input work may extend the
analysis artifact/input data but SHALL NOT add an engine mutation or compilation path. Because both
may touch `pg-wasm`, they merge serially with one owner at a time.

## Stage 2 — serialized rewrite correctness

Merge in this exact order because these changes own `pg-foma/src/replace.rs`, relevant `gate.rs`
entry points, and dedicated gates:

1. `fix-multitable-fst-compilation`
2. `compile-right-to-left-rewrites`
3. `compile-simultaneous-rewrites`
4. `lower-fst-pattern-environments`
5. `compile-bounded-fst-quantifiers`
6. `compile-fst-metathesis`
7. `cover-circumfix-null-output-actions`
8. `cover-template-truncation-reduplication`
9. `cover-realizational-morphology-constraints`

The coverage ledger has one integration owner. Semantic worktrees produce evidence/row-update
fragments; the integration owner applies ledger changes after merge.

### Stage 2+ gate: replacement-cascade compilation profile

`profile-fst-compilation` Phase B is blocked until Stage 2 wires the replacement-rule cascade into
the production network constructor used for lookup. The mere existence of experimental
`replace.rs` functions does not satisfy this dependency. Before that switch, any cascade metrics
are labeled `experimental_composition`; after it, the profile may add per-rule own-net,
alpha/group, and running state/arc curve metrics without observer-induced minimization.

## Stage 3 — interaction and scale

- `add-pairwise-grammar-interaction-coverage` runs against a pinned post-Stage-2 ledger version.
- `calibrate-fst-resource-envelopes` may build its harness earlier, but final sweeps wait until every
  Stage-2 construct, production cascade/profile, correctness gate, and pairwise gate has merged.
  Earlier stages use centralized values explicitly marked provisional. Serial sweeps replace them
  with reviewed portable defaults and hard ceilings; release is blocked while provisional markers remain.
  Final evidence combines representative real-language workloads with generated one-factor and
  pairwise scaling cases plus long/ambiguous words. All retain semantic correctness gates.
  Current calibration evidence runs on Windows and records Linux as `not_run`; this is honest missing
  evidence, not an implementation blocker. Later Linux evidence may conservatively revise the same
  portable runtime policy but does not create platform-specific limits.
  Calibration produces evidence, headroom reasoning, and a proposed policy diff only. It cannot
  rewrite or adapt production constants; activation requires a human-reviewed committed policy version.
Runtime APIs expose explicitly named combined and HermitCrab-only pipelines. The latter supports
engine integration and parse-failure explanation. Both share the portable budget/outcome contract,
and a request never switches pipelines automatically.
Native word-set cross-engine validation compares completed combined and Rust-HermitCrab-only results
by structured semantic identity and may also invoke C# HermitCrab for HC XML. C# remains a native
CLI/PowerShell reference tool and has no WASM dependency or export. PanGloss reports comparison and
availability evidence; consuming applications own publication allow/deny policy.
Semantic parity compares deduplicated structured-analysis sets. Duplicate copies retain counts,
ratios, and available rule/proposal-path provenance as health evidence so developers and AI agents
can diagnose overlapping FST paths without misclassifying duplicates as linguistic answers.
One coverage-contract owner defines identity from Machine `WordAnalysis.Equals`: ordered stable
morpheme IDs, root position, and category/POS; Rust `guessed` is a separate parity annotation.
Diagnostics reuse it while gloss/shape/properties/duplicates/paths remain evidence. Key semantic
decisions cite Machine and applicable LibLCM precedent; divergences require reviewed rationale,
compatibility impact, and focused regression tests.
C# `analysis-batch` owns authoritative native machine-delta output using stable XML keys, root, and
category; `gloss-batch` remains explanatory. The loader exposes a supported object-to-source-key map,
and missing/colliding mappings yield typed `not_comparable` rather than silent identity collapse.
Reports capture both sides' full available context without gating execution on equality. Strict
engine parity highlights unexpected drift; intentional grammar-delta mode compares differing
grammars/options/data and reports added, removed, unchanged, incomplete, and unattempted analyses.
Optional golden sets yield exact matching/missing/unexpected diffs. Semantic changes never receive a
PanGloss quality verdict or aggregate closeness score; compiler/runtime health remains separate.
Validation treats input goldens as immutable. Optional proposed goldens are distinct context-bound
artifacts with exact diffs; adoption is an explicit caller-owned action outside the validation run.
Compiler/runtime-owned breadcrumbs attach stable rule/construct IDs, stages, paths, outcomes, and
completeness to deltas and duplicates. Reports describe participation/association, never unsupported
causation; downstream developers and AI tooling own interpretation.

## Stage 4 — final evidence

`certify-four-language-matrix` runs serially on one quiet, fully merged commit. It consumes the
versioned diagnostic reports from Stage 1 and the compile profiles/resource policy from later
stages; it does not reimplement timing, gloss, completeness, or resource calculations. It changes
reports and status only. Any code failure opens a new bounded OpenSpec change.

## Merge hotspots

- `replace.rs` / `gate.rs`: one semantic owner at a time.
- `emit.rs`: one owner at a time across production-profile sink threading and semantic compiler work.
- `preexpand.rs` / `peel.rs` / `morphotactics.rs`: one Stage-2 morphology owner at a time.
- Coverage ledger: one integration owner.
- `analyzer.rs`: diagnostic event producer; apply-budget adapters consume its stable API.
- `confirm.rs`: confirm instrumentation owner.
- `composite.rs`: terminal-outcome routing owner; automatic alternate-engine routing is removed.
- `p6_aweti_gate.rs`: Aweti owner until Stage 1 merges.
- `pg-cli/main.rs`: diagnostics/gloss owner until Stage 1A merges.
- C# HermitCrab.Tool command files: reference-parity owner during Stage 1C.
- `pg-wasm`: one owner at a time across stem-input integration and the analysis-only boundary.
- FST health finding registry/schema: one contract owner; compiler work contributes registered codes.

## Initial parallel dispatch set

After prompts name exact file ownership, the following can start in parallel:

1. Coverage ledger schema/inventory (0A first unit)
2. Resource budget/error foundation (0B)
3. Diagnostic report schema/CLI skeleton after consuming the agreed schema boundary (1A first unit)
4. PowerShell/incoming/report rendering after report schema stabilizes
5. C# `gloss-batch` golden contract after the Rust/shared signature format stabilizes
6. FST compilation-health schema and golden findings (0C)

All work touching `replace.rs`, `gate.rs`, or overlapping `emit.rs` regions remains serialized.
Missing `machine`, private/gitignored corpora, an oracle executable, a platform runner, or a quiet
benchmark host never freezes this queue. The owning agent records the unavailable evidence and
continues every self-contained task; certification status remains unclaimed until its evidence runs.
