# Implementation readiness

OpenSpec `status: ready` means a change has proposal, design, specs, and tasks. It does not mean its
cross-change prerequisites are satisfied or that the whole change is one safe worktree dispatch.
`STAGING.md` remains authoritative.

## Safe initial dispatch after a clean planning commit

These are bounded merge units, not permission to apply every task in their parent change:

1. **Coverage ledger schema and one-time inventory** — audit the frozen model and classify every
   row; no permanent source-parser mechanism is required.
2. **Resource budget/error foundation** — validated config, cumulative logical tracker, checked-net
   API, and typed worker outcomes; no subprocess or terminal production routing in this unit.
3. **Diagnostic report schema/CLI skeleton** — types, argument parsing, and golden empty report;
   potentially adversarial grammar execution uses the worker watchdog when available.
4. **Reference gloss-signature goldens** — finalize and test the shared escaping/missing-gloss
   contract before C# or orchestration work.
5. **FST compilation-health schema** — Rust types, stable-code registry, severity/override rules,
   and golden JSON; no compiler instrumentation in this merge unit.

## Decisions and dispatch refinements

### R1. Frozen model inventory — resolved

HermitCrab and the Rust model are assumed complete apart from bug fixes. Implement a one-time
reviewed ledger over the current model; do not add source-AST/reflection infrastructure. Any future
model-shape extension is outside this assumption and must explicitly reopen the coverage contract.

### R2. Supervisor platform contract — resolved

Windows and Linux are equal, first-class native production targets. Both use one compiler worker,
one versioned request/result protocol, standard-library `Child::try_wait`/`Child::kill` wall-time
control, deterministic compiler budgets, bounded input/output, and sampled RSS through a
Rust-1.90-compatible `sysinfo` release. Production compilation launches no descendants, so Job
Objects, cgroups, process-tree management, Tokio, and `processkit` are out of scope. Sampled RSS is
reported with its interval and observed peak; it is not called a hard memory ceiling. WASM is
analysis-only and needs no compile watchdog.

### R2A. Precompiled WASM artifact container — resolved

The artifact is one self-contained file that supplies both halves of analysis: a precompiled FST
proposer and the matching runtime grammar data used by the Rust HermitCrab port for confirmation and
full analysis. One package fingerprint binds both payloads so they cannot be mixed across grammars.
The physical container uses fixed PanGloss magic bytes, an integer container version, a
length-prefixed canonical JSON manifest, a length-prefixed Rust HermitCrab runtime payload, a
length-prefixed existing foma binary payload, and a SHA-256 digest covering the manifest and both
payloads. Every length is validated against versioned limits before allocation. The foma payload
retains its existing binary-memory format; do not invent a second network format. The manifest
contains an optional license declaration and optional Ed25519 publisher signature. Signature state
is reported as `unsigned`, `valid`, or `invalid` and never controls analysis. There is no entitlement
or license server. This metadata describes WASM package deployment/provenance; it does not license or
restrict FieldWorks analysis.
Coordinate exclusive ownership of `pg-wasm` with the separately in-flight stem-input work.

### R3. Split the remaining-construct epic — resolved

The former `compile-remaining-fst-constructs` umbrella has been replaced by separate changes for:

- shared pattern/environment lowering;
- bounded/optional quantifiers;
- metathesis;
- circumfix/null-role and output-action sequences;
- template/truncation/reduplication boundary contracts;
- realizational/stem/family/blocking/co-occurrence constraints.

Each names its exclusive files, oracle fixtures, resource checks, and focused verification commands.
They merge serially in the order listed by `STAGING.md`; they are never dispatched as one epic.

### R4. Exact reference gloss encoding — resolved

Gloss signatures retain tagged `g:`/`m:`/`s:` components and encode each following value as an RFC
8785 canonical JSON string. Literal glosses use `g:<json-string>`, missing glosses use
`m:<owning-morpheme-id-as-json-string>`, and surface shape uses `s:<json-string>`. Separators are
recognized only outside JSON strings. Writers preserve the input Unicode sequence without
normalization, entries sort lexicographically by unsigned canonical UTF-8 bytes, and duplicates
remain present. Zero-analysis and `SKIPPED` rows retain the existing `-` signature.

### R5. Verification commands and evidence availability

Every worktree prompt must name its focused cargo/dotnet tests, ignored-gate invocation, required
gitignored corpora, optional `machine` submodule provisioning, and whether a measurement benefits
from a serial/quiet environment. Self-contained implementation and tests proceed without optional
external evidence. If a corpus, `machine`, oracle, platform runner, or quiet host is unavailable, the
agent records `not_run` plus the missing prerequisite and continues all independent work. Named
checks in prose are not enough for unattended agents.

### R6. FST compilation health — resolved architecture

FST health is a Rust compiler responsibility, not Python analytics and not general grammar-quality
review. Rust emits stable codes, exact metric inputs, affected rule/construct identifiers, and ranked
applicable remedies. Measurements come from the admission walker, budget tracker, and compile profile
once; the health evaluator consumes them without recomputation. FieldWorks, CLI, AI tools, and the
package builder consume the same canonical report. No Python package, notebook, IDE, playground, or
UI is added to this core repository for these warnings.

FST payload size uses decimal bytes: Ideal `<=10_000_000`; Info `>10_000_000..=20_000_000`;
Warning `>20_000_000..=100_000_000`; Error `>100_000_000..=500_000_000` with explicit recorded
override; Critical `>500_000_000` with no override. Size is one dimension; compile work,
intermediate nets, candidates, paths, time, and unknown/unbounded constructs may raise severity.
Unknown cost is not itself Critical when construction is recall-preserving: compilation proceeds
under the shared resource envelope and its observed outcome controls admission. Any uncertainty
that could omit an analysis fails closed.
The compiler never retries or raises limits automatically. A terminal finding returns the effective
envelope, reached metric, partial measurements, and grammar-first remedies promptly enough for AI
tooling to act. A caller may deliberately start a new attempt with a larger named, versioned envelope.
Deterministic logical counters are the primary fast-failure mechanism; cooperative elapsed checks
and the parent wall timeout are outer safeguards. Defaults are calibrated from evidence, but the
plans do not promise identical failure time across different machines.
Material operations reserve against the cumulative logical budget when an exact value or proven
conservative lower bound is available. A proven minimum that cannot fit stops before allocation;
heuristic estimates can warn but cannot reject, so uncertain work is attempted and counted.
Compiler diagnostics may recommend reordering, constraining, or decomposing grammar rules, but do not
apply those source changes. Automatic internal transformations require an owned correctness argument
that preserves the complete HermitCrab analysis set and record their profile evidence.
PanGloss is explicitly propose-and-confirm: recall-preserving FST overapproximation is allowed and
HermitCrab filters false positives. Candidate/path volume, confirmation count and work, and rejection
share remain first-class health metrics even when final results are completely correct.
Application results are atomic per word. A budget-exhausted word returns a typed incomplete outcome,
never definitive partial analyses; easy words already completed in the batch remain valid. Callers
may explicitly retry only the incomplete subset with caller-selected larger apply budgets.
Callers may also select an optional cumulative batch budget. When it is exhausted, completed words
remain valid, a started unfinished word is incomplete, and remaining unstarted words are explicitly
not attempted. The latter two subsets can be resubmitted without rerunning completed work.
All caller, host, and named-envelope values remain beneath versioned, hard-coded, deliberately high
absolute ceilings for every enforced logical, byte, and wall-time dimension. There is no unlimited
mode. These are emergency containment bounds; calibrated defaults provide earlier useful failures.
Runtime budget dimensions and absolute values form one portable set across Windows, Linux, and WASM.
Individual applications choose their own lower normal and retry values; ordinary defaults target
user PCs, and no app may redefine the shared maximum.
Earlier implementation centralizes explicit conservative provisional values so agents do not invent
independent constants. Final numerical calibration is a late gate after all frozen-model construct
work and production cascade profiling merge. Release policy cannot retain provisional values.
Final calibration requires both representative real-language workloads and synthetic one-factor,
pairwise, long-word, and ambiguity scaling. Correctness gates remain active; measurements from a
semantically invalid case cannot justify resource policy.
The current calibration platform is Windows. Reports capture its hardware/toolchain/build metadata
and mark Linux `not_run`; missing Linux measurements do not block implementation. Later credible
Linux evidence may conservatively revise the one portable policy, never fork it by platform.
Calibration is advisory and cannot rewrite production constants. It emits raw evidence, recipes,
headroom calculations, and a proposed diff; a human-reviewed commit and policy-version change are
required to activate new defaults or hard ceilings.
Runtime callers explicitly select either normal FST-propose-plus-HermitCrab-confirm or supported
HermitCrab-only analysis. The latter serves engine integration, parity, and detailed explanations of
why a word did not parse. Both share the budget/outcome contract, report their pipeline, and never
switch automatically during a request.
Completed combined and Rust-HermitCrab-only results compare by structured semantic analysis identity,
not bytes, order, timing, or traces; incomplete results are `not_comparable`. Native CLI/PowerShell
cross-engine validation accepts a word set and can additionally run C# HermitCrab for HC XML. It
reports evidence and availability only; applications own publication policy. C# never enters WASM.
Semantic parity deduplicates by complete structured analysis identity. Pre-dedup duplicate counts,
ratios, and rule/proposal-path provenance remain first-class health evidence: for example, 24 copies
still mean one semantic answer but expose an FST design problem useful to developers and AI agents.
The core identity follows Machine `WordAnalysis.Equals`: ordered stable morpheme IDs, root position,
and category/POS. Rust `guessed` is checked separately in Rust parity. Gloss, surface shape,
properties, duplicates, paths/traces, timing, counters, prose, and formatting remain diagnostics.
Every key semantic decision reviews and cites relevant Machine and applicable LibLCM precedent;
divergence requires a reasoned compatibility analysis and focused regression evidence.
Cross-engine identity preserves those Machine dimensions but resolves runtime ordinals through
stable source keys: HC XML `id`/`xml_key` or retained LCM GUID, plus stable POS symbol identity.
Missing/colliding mappings are `not_comparable`. C# `analysis-batch` emits this machine-delta set;
`gloss-batch` remains separate explanatory, duplicate-sensitive evidence.
Comparison reports bind both sides' grammar/package, identity schema, stems, options, pipeline,
budgets, completeness, and engine versions when available, but never refuse to run because they
differ. Strict-parity mode flags unexpected context drift; grammar-delta mode expects before/after
differences and reports gained, lost, unchanged, incomplete, and unattempted analyses and health.
Optional caller-supplied golden sets produce exact matching, missing, and unexpected identity diffs.
PanGloss never turns semantic deltas into grammar-quality labels or an aggregate closeness score;
only compiler/runtime health receives an objective better/worse assessment.
Golden inputs are immutable. PanGloss may write a separate context-bound proposed golden and exact
diff, but validation never accepts, overwrites, reformats, or re-versions the authoritative input;
adoption is an explicit caller-owned review action.
PanGloss owns factual breadcrumbs: stable rule/construct IDs, stages, proposal/confirmation paths,
outcomes, and completeness. Reports link participating or associated evidence to deltas and
duplicates but never invent causal certainty; developers and AI tools interpret the breadcrumbs.

PanGloss has three explicit deployment domains. Browser/WASM, word-processor, and native C
inference hosts load precompiled packages for analysis, spell checking, and glossing. FieldWorks and
AI-framework native build hosts use the C ABI/CLI for compilation, health, deltas, and diagnostics.
An optional native CLI/PowerShell lane invokes the pinned C# Machine oracle for HC-XML reference
validation. Capability discovery is versioned; absent operations return `unsupported_capability`,
and WASM has no compiler export. Assessment-delta and trace-diagnostic reports provide
machine-readable FieldWorks investigation handoffs; build reports remain compilation-only. PanGloss
owns no authoring UI, project history, publication policy, or FieldWorks invocation.

The suite uses a code/build/test/release model. Grammar and stems are source; native compilation
produces a separate immutable build report containing FST-health diagnostics. Caller-supplied word
runs produce immutable assessment reports, and semantic deltas compare those reports only. A
compile-and-assess convenience emits both artifacts. Compilation may stay in memory; release is the
optional serialization of one data-only `.pgpack` PanGloss Language Pack. The pack contains no
scripts, WASM modules, native libraries, or other executable extensions. Runtime and SDK are named
distributions: the SDK adds `pangloss-build` beside and dependent on the exact `pangloss-runtime`.
The pinned C# Machine comparison utility remains source-only developer/conformance tooling and is
not distributed in either product.
Each SDK bundle contains one exact tested Runtime build. Compatible patch-level substitution is an
advanced pairing allowed only when the declared ABI/package compatibility check succeeds.
Runtime may hold multiple isolated immutable Language Pack handles. Each analysis explicitly names
its handle and owns request-local scratch, budgets, tracing, and cancellation; native completed
models are concurrency-safe, while active compilation sessions remain single-owner.

## Conditional/later work

- Potentially adversarial diagnostic grammar runs use the completed worker watchdog; independent
  diagnostic implementation continues without them.
- WASM analysis waits for a compatible artifact from the native compilation authority; WASM is not
  part of compile profiling, compile-envelope calibration, or compiler coverage certification.
- FST health policy/schema may land before instrumentation; observed audit fields populate as their
  owning profile/budget changes merge and are never independently remeasured.
- Replacement-cascade profiling waits for Stage 2 production wiring and a matching network
  fingerprint.
- Pairwise interaction coverage waits for a pinned post-Stage-2 ledger.
- Calibration is split into harness implementation, quiet supervised sweeps, then policy
  publication.
- Four-language certification is a serial evidence run on one merged commit, not an implementation
  worktree.
- Aweti optimization is deliberately absent: measurement produces a new bounded change or
  `no safe lever`.

## “Just implement it” dispatch rule

That instruction dispatches every currently implementable merge unit. Unavailable external evidence
does not stall the queue: agents record it and continue independent work. The dispatcher follows
`STAGING.md` ownership and merge order, runs strict OpenSpec validation, and distinguishes unfinished
implementation from completed implementation whose optional evidence run is `not_run`.
