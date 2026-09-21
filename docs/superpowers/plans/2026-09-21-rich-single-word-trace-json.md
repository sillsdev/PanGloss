# Rich Single-Word Trace JSON Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an explicit single-word rich trace mode that emits versioned JSON with attempts, diagnostic evidence, authoritative completion, successful analyses, and per-detour timing.

**Architecture:** `pg-rules` records typed decision-owned evidence and optional monotonic spans in the existing trace tree. `pg-parse` supplies the authoritative parse outcome and root timing. `pg-cli` validates `--trace-details` and serializes a versioned envelope without changing ordinary parse or tree-only trace output.

**Tech Stack:** Rust, `serde`/`serde_json`, PanGloss trace sinks, managed `rust/tools/pg.ps1` gates.

---

## File map

- Modify `rust/crates/pg-rules/src/trace.rs`: typed diagnostic payloads, detailed word snapshots, optional timing, and trace-tree aggregates.
- Modify the existing refusal owners under `rust/crates/pg-rules/src/`: publish diagnostic evidence at the decision seam and time attempted operations without changing the decisions.
- Modify `rust/crates/pg-parse/src/morpher.rs`: expose authoritative termination and root elapsed time for a traced parse.
- Create `rust/crates/pg-cli/src/rich_trace.rs`: typed, serializable versioned envelope and grammar-aware reference resolution.
- Modify `rust/crates/pg-cli/src/trace_render.rs`: share source/shape resolution needed by the rich serializer while leaving compact renderers unchanged.
- Modify `rust/crates/pg-cli/src/main.rs`: parse and validate `--trace-details`, execute detailed tracing only for one-word `parse`, and render the envelope.
- Modify `docs/formats/trace-format.md`: document the opt-in JSON schema, timing semantics, and single-word restriction.
- Create or modify focused tests beside each owner; update `docs/divergences/README.md` and `docs/divergences/by-module.md` only if review confirms the diagnostic representation belongs in the oracle divergence ledger.

### Task 1: Add the rich trace data model and timing spans

**Files:**
- Modify: `rust/crates/pg-rules/src/trace.rs`
- Test: `rust/crates/pg-rules/src/trace.rs`

- [ ] **Step 1: Write failing tests for opt-in detail and timing**

Add tests that construct both ordinary and detailed sinks and assert:

```rust
let ordinary = TreeTraceSink::new();
assert!(!ordinary.records_details());
assert!(ordinary.node(ordinary.analyze_word(&word)).timing.is_none());

let detailed = TreeTraceSink::new_detailed();
assert!(detailed.records_details());
let root = detailed.analyze_word(&word);
let node = detailed.node(root);
assert!(node.timing.is_some());
```

Add a fake monotonic clock or explicit test timestamps so a parent with two non-overlapping children
asserts `started_ns`, inclusive `elapsed_ns`, and `self_elapsed_ns` exactly. Add aggregate assertions
that distinguish total nodes from morphological, phonological, and compounding attempt nodes.

- [ ] **Step 2: Run the focused test and observe the expected failure**

Run:

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-rules -Filter trace
```

Expected: compile failure because detailed mode, timing records, and aggregate accessors do not exist.

- [ ] **Step 3: Add typed diagnostic records**

Introduce closed trace types along these lines, adapting names to existing grammar types:

```rust
pub struct TraceTiming {
    pub started_ns: u64,
    pub elapsed_ns: u64,
    pub self_elapsed_ns: u64,
}

pub enum FailureDetail {
    RequiredSyntacticFeatures { actual: FeatureStruct, required: FeatureStruct },
    RequiredMprFeatures { actual: MprSet, required: MprSet },
    ExcludedMprFeatures { actual: MprSet, excluded: MprSet },
    StemName { stem_name: String },
    Environment { environment: String, allomorph: Option<AllomorphId> },
    CompetingAllomorph { attempted: AllomorphId, selected: AllomorphId },
    ApplicationCount { maximum: u32, actual: u32 },
    // Add the remaining FieldWorks categories from the approved design as typed variants.
}
```

Extend `TraceNode` with optional attempted-allomorph, failure-detail, diagnostic input/output snapshot,
and timing fields. Keep the existing `input`, `output`, `source`, and `failure_reason` fields so compact
renderers and cursor behavior stay unchanged.

- [ ] **Step 4: Make timing strictly opt-in**

Give `TreeTraceSink` ordinary and detailed constructors. Add `records_details()` to `TraceSink` so call
sites can avoid clock reads and diagnostic clones unless detailed tracing is active. Use a monotonic
root origin and explicit begin/end span operations. Compute self time from direct child intervals;
assert that child spans are contained and non-overlapping before subtraction.

- [ ] **Step 5: Run focused tests and commit**

Run the focused command from Step 2, then:

```powershell
rust/tools/pg.ps1 -Mode check -Package pg-rules
git add rust/crates/pg-rules/src/trace.rs
git commit -m "feat(trace): add rich diagnostic records"
```

Expected: focused tests and package check pass with ordinary trace behavior unchanged.

### Task 2: Publish failure evidence and time attempts at owner seams

**Files:**
- Modify: relevant owners under `rust/crates/pg-rules/src/morph.rs`, `validity.rs`, `rewrite.rs`, `metathesis.rs`, `stratum.rs`, and related focused modules discovered from `TraceSink` call sites
- Modify: `rust/crates/pg-parse/src/morpher.rs`
- Test: existing focused unit/integration tests in `rust/crates/pg-rules/tests/` and `rust/crates/pg-parse/tests/`

- [ ] **Step 1: Build a differential test before changing call sites**

Add a test that parses one synthetic word with ordinary tracing and detailed tracing and compares the
complete analysis identity multiset, `capped`, `timed_out`, `invalid_shape`, and parser step count in
both directions. Add representative failing assertions for feature, environment, allomorph,
co-occurrence, and maximum-application failures. Each should currently fail because `failure_detail`
or `attempted_allomorph` is absent.

- [ ] **Step 2: Run the focused tests and confirm the missing-evidence failures**

Run:

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-parse -TestTarget trace_gate
rust/tools/pg.ps1 -Mode test -Package pg-rules -Filter trace
```

Expected: new diagnostic assertions fail while the ordinary/detailed parse multiset comparison passes.

- [ ] **Step 3: Thread evidence from each decision owner**

At every existing refusal callback, pass the value already used by the refusal. Extract a shared
decision result when the owner currently collapses several reasons into a boolean. Do not inspect
diagnostic text and do not turn candidate helpers into refusal decisions. Add one-way assertions that
a claimed diagnostic reason agrees with the refusal branch that emitted it.

Pass attempted allomorphs from the rule application loop that selected the subrule. Publish actual and
required feature structures from their unification gate, environments from their match gate,
co-occurrence rules from validity checks, and application counts from the existing cap owner.

- [ ] **Step 4: Time complete attempted operations**

Start the span immediately before the owner attempts a rule, template, stratum, lexical lookup, or
terminal synthesis check and close it on every success or refusal return. Use scope guards where early
returns would otherwise leave a node open. Perform no clock work when `records_details()` is false.

- [ ] **Step 5: Return authoritative traced outcome data**

Keep `ParseOutcome` as the owner of `capped`, `timed_out`, `invalid_shape`, steps, analyses, and
signatures. Add only the minimal elapsed/root timing access needed by the CLI. A cap must retain the
partial trace and return `completed = false`; zero analyses without a cap remains completed.

- [ ] **Step 6: Verify effect and commit**

Run:

```powershell
rust/tools/pg.ps1 -Mode check -Package pg-rules
rust/tools/pg.ps1 -Mode check -Package pg-parse
rust/tools/pg.ps1 -Mode test -Package pg-parse -TestTarget trace_gate
rust/tools/pg.ps1 -Mode quick -Package pg-rules
```

Review the test counters to confirm every intended failure-detail branch fired. Then commit only the
task's source and tests:

```powershell
git commit -m "feat(trace): capture attempt evidence and timing"
```

### Task 3: Serialize the versioned rich JSON envelope

**Files:**
- Create: `rust/crates/pg-cli/src/rich_trace.rs`
- Modify: `rust/crates/pg-cli/src/trace_render.rs`
- Modify: `rust/crates/pg-cli/src/main.rs`
- Test: `rust/crates/pg-cli/src/rich_trace.rs`
- Test: `rust/crates/pg-cli/src/main.rs`

- [ ] **Step 1: Write failing argument and schema tests**

Add parser tests proving that `--trace-details` is rejected without `--trace`, rejected with text
format, and accepted only by single-word `parse`. Add a golden structural test expecting
`schemaVersion`, `search`, `result`, `counts`, and `trace`. Assert typed termination, complete
analyses, source references, diagnostic snapshots, failure details, and attempt counts. Normalize or
structurally inspect timing integers; never compare recorded durations to golden values.

- [ ] **Step 2: Run the CLI tests and observe failure**

Run:

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-cli -Filter rich_trace
rust/tools/pg.ps1 -Mode test -Package pg-cli -Filter trace_details
```

Expected: compile or assertion failures because the flag and serializer do not exist.

- [ ] **Step 3: Implement typed serialization**

Define `Serialize` records in `rich_trace.rs` for `pangloss.trace-details.v1`. Map grammar IDs to a
reference containing kind, compiled index, optional authored ID, and optional name. Map diagnostic
word snapshots and `FailureDetail` variants without exposing Rust ownership or cache fields.

Derive termination in this order: invalid shape, timeout, step cap, completed. Derive
`completed` from termination rather than the presence of analyses. Count attempts only from the six
rule analysis/synthesis node kinds; count successful and failed paths from explicit terminal nodes.

- [ ] **Step 4: Wire and validate `--trace-details`**

Parse the flag only in `run_parse`. Reject it unless trace destination is present and format is JSON.
Construct `TreeTraceSink::new_detailed()` only for this path. Serialize the rich envelope to the same
stdout/file destination that `--trace` already controls. Preserve the result line and all default
tree-only output exactly when the flag is absent.

- [ ] **Step 5: Prove default compatibility and capped behavior**

Add a test that captures existing compact JSON before and after rich support and compares exact bytes.
Add a small-cap unit-level harness around the rich envelope builder if `parse` cannot select a cap;
feed it a real capped `ParseOutcome` and partial tree, then assert `termination == "stepCap"`,
`completed == false`, and a non-null trace.

- [ ] **Step 6: Verify and commit**

Run:

```powershell
rust/tools/pg.ps1 -Mode check -Package pg-cli
rust/tools/pg.ps1 -Mode quick -Package pg-cli
```

Then commit:

```powershell
git add rust/crates/pg-cli/src/rich_trace.rs rust/crates/pg-cli/src/trace_render.rs rust/crates/pg-cli/src/main.rs
git commit -m "feat(cli): emit rich single-word trace JSON"
```

### Task 4: Document, audit, and run authoritative gates

**Files:**
- Modify: `docs/formats/trace-format.md`
- Modify if required by primary review: `docs/divergences/README.md`
- Modify if required by primary review: `docs/divergences/by-module.md`
- Create if required: next numbered `docs/divergences/NNN-*.md`

- [ ] **Step 1: Document the public contract**

Document the exact invocation, one-word restriction, version string, termination states, source-ID
limits, failure-detail variants, and inclusive/self timing semantics. State that detailed timings
measure the unmerged instrumented diagnostic search and are not batch performance measurements.

- [ ] **Step 2: Audit the oracle ledger classification**

Compare Machine base trace, FieldWorks rich adapter, and the landed PanGloss representation. If the
gap meets the ledger's representational definition, add a stable numbered entry and both module index
links. Record immutable revisions and distinguish source parity from demonstrated fixture coverage.

- [ ] **Step 3: Run managed checks before expensive tests**

Run:

```powershell
rust/tools/pg.ps1 -Mode check -Package pg-rules
rust/tools/pg.ps1 -Mode check -Package pg-parse
rust/tools/pg.ps1 -Mode check -Package pg-cli
rust/tools/pg.ps1 -Mode quick -Package pg-rules
rust/tools/pg.ps1 -Mode quick -Package pg-parse
rust/tools/pg.ps1 -Mode quick -Package pg-cli
```

- [ ] **Step 4: Run authoritative verification**

After recording fresh resource headroom, run:

```powershell
rust/tools/pg.ps1 -Mode test -Package pg-rules
rust/tools/pg.ps1 -Mode test -Package pg-parse
rust/tools/pg.ps1 -Mode test -Package pg-cli
rust/tools/pg.ps1 -Mode conformance-test -Scope all
```

Run before/after binaries through `pg.ps1 -Mode run` and compare complete batch TSVs with
`rust/tools/parse_compare.py`. Accept `IDENTICAL` or `MULTISET_EQUAL`; reject missing rows, caps,
timeouts, skips, errors, or `SET_EQUAL` alone.

- [ ] **Step 5: Review and commit documentation**

Inspect the complete branch diff against the design, verify no unrelated dirty files entered any
commit, and commit documentation with:

```powershell
git commit -m "docs: describe rich trace JSON"
```

After independent spec and code-quality review, rebase on local `main`, rerun the authoritative
managed tests on the rebased result, and fast-forward `main`.
