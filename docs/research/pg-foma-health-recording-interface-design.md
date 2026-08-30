# pg-foma health recording: interface design (design only — not implemented)

Scope: this document designs the Interface for a deepening of `crate::health_evaluator`'s
recording path in `pg-foma`. No code changes accompany it. Rust snippets below are illustrative
sketches of an Interface, not a diff to apply. Vocabulary is the `codebase-design` skill's
(**Module** / **Interface** / **Implementation** / **Depth** / **Seam** / **Adapter** /
**Leverage** / **Locality**) plus `CONTEXT.md`'s domain terms (**Compiler** / **Backend** /
**Compatibility report** / **Artifact** / **Lowering**).

## 1. The finding, and three corrections found while verifying it

The brief's finding stands: `evaluate_health` (`health_evaluator.rs:314-335`) takes four
independent, all-optional parameters and returns `Ideal` (empty findings) when every one is
absent — identically to what it returns when every one is present and clean. Reading the call
graph end to end turned up three refinements worth flagging before designing against it, because
they change which callers a migration actually has to move.

**(a) `pg-cli/src/pack.rs`'s "production call site" is test-only.** The entire file
(`rust/crates/pg-cli/src/pack.rs`, all 18 lines) is a single `#[cfg(test)] mod tests` block; its
`evaluate_health(Some(oversized), None, &[], &[])` call (line 10) is a regression pin, not a
runtime call site. Grepping the whole workspace for `evaluate_health` turns up exactly **one**
production call site: `worker.rs:331`. Every other call is inside `#[cfg(test)]` — in
`health_evaluator.rs` itself, `pack.rs`, and `pg-foma/tests/phase_c_chain_scale.rs`. This makes the
migration's blast radius smaller than "two production callers" suggests, and I'd flag it to a
reviewer explicitly since it changes the urgency/risk calculus for Section 6's migration order.

**(b) `ComposeError` is dropped in a second production path the brief didn't name, and the named
one (`peel.rs:552`) is a test.** `peel.rs:552` is inside
`deep_self_similar_chain_is_refused_deterministically_under_a_small_cap`, a unit test asserting the
error shape — not a production consumer. The real second production path is:
`peel::ReduplicationPeeler::peel_candidates` returns `Result<_, ComposeError>` up through
`composite.rs`'s `propose_candidates`/`propose_candidates_budgeted` (composite.rs:367-370,
815-818), which stores it on the public `FomaOutcome.peel_chain_depth_error: Option<ComposeError>`
field (composite.rs:162-172). The only reader of that field anywhere in the workspace is
`backend_runtime.rs:2108`, which flattens it to a **bool**:
`occurrence.peel_refusals = u64::from(peel_chain_depth_error.is_some())`. So there are two
production places a `ComposeError::ChainDepthExceeded` is produced and both drop the numbers:
`emit.rs`'s compound-chain-depth check (prose only) and `composite.rs`'s reduplication-chain-depth
check (a bare bool, via `backend_runtime.rs`). Neither reaches `health_evaluator::
compose_error_finding` (health_evaluator.rs:243-261), which is exactly what would have preserved
`depth`/`limit`/`site` — it is exercised only by tests.

**(c) The concrete "loses its numbers" defect is sharper than a dropped `ComposeError` — it's a
dead struct field on the very report `health_evaluator` already reads.** `emit.rs`'s
`compound_chain_depth_and_budget_check` (emit.rs:2055-2091) builds
`EnumBudgetExceeded { measure, value: depth, limit }` (emit.rs:2082-2086) and attaches it to
`EmitReport.enum_budget_exceeded`. That field's own doc (emit.rs:339-341) says: *"the field is
retained for compatibility with the compound-chain-depth refusal, whose measured value and
configured limit are useful to callers and diagnostics."* But `health_evaluator::
unsupported_tier_finding` (health_evaluator.rs:144-213) never reads `enum_budget_exceeded` — it
reads only `closure_refusal`, which is `None` for this call site — so today this exact case falls
into the generic branch and reports `MetricValue::Unbounded` with no structured number at all; the
depth and limit survive only inside the free-text `reason` string. This is the same `EmitReport`
value `worker.rs:331` feeds into `evaluate_health` on every real compile that trips this cap, so
it's a live, currently-shipping gap, not a hypothetical. I use it in Section 7 as the worked
"newly catchable" example, and I'd land it **standalone, before the interface redesign**, since
teaching the evaluator to also consult `enum_budget_exceeded` is a one-function fix independent of
which design below is chosen.

**A fourth thing worth a reviewer's eyes, not a correction but a gap in the same family:**
`backend_selection.rs`'s production selection path (`SelectionReport`'s builder, around
backend_selection.rs:503) calls `BackendReport::accepted(strategy, decision, Vec::new())`
unconditionally — an accepted backend's findings are **always empty** in that path, so
`characterization_findings`'s cost/coverage findings (characterization.rs:268-276) never reach the
selection report actually used to choose a compile path; they only reach `pg-cli`'s standalone
`pangloss fst-health` command (`pg-cli/src/fst_health.rs:11-13`, explicitly commented
"characterization only"). This is orthogonal to `evaluate_health`'s parameter shape, but it's the
same species of silent gap and bears on Section 6/8's answers about the two bypassing modules.

## 2. Constraints any interface must satisfy

- **Health is reported about a compile, never consulted during one** (`health.rs:4-7`'s own
  module doc). Whatever records measurements must be write-only from a compiler pass's point of
  view — no pass may branch on accumulated health.
- **`HealthFinding::new` already enforces severity/class agreement by panicking on mismatch**
  (health.rs:452-507) — any new construction path must still go through it, not a literal struct
  build (already refused by `tests/health_finding_seam.rs`, per health.rs:462-464).
- **The schema (`health.rs`, v7) is the most-versioned document in the crate.** A design that
  moves `HEALTH_SCHEMA_VERSION` forces every existing JSON consumer (including
  `CompileWorkerResult`, which crosses the worker's process boundary — worker.rs:1-15, 220-260) to
  re-validate; a design that doesn't is strictly cheaper to ship.
- **`CompileWorkerOutcome::Success`/`CompileFailed` carry a plain `HealthReport` across a process
  boundary** (worker.rs:222-260) via a length-prefixed JSON frame. Whatever the recording
  mechanism looks like *during* a compile, it must bottom out in a plain, `Serialize`/`Deserialize`
  `HealthReport` before the child writes its result frame — a live recorder object, a trait object,
  or anything with a lifetime cannot itself cross that boundary.
- **Distinguish "measured and healthy" from "measured nothing."** This is the finding itself:
  whatever replaces `evaluate_health` must make an all-absent input either unconstructable, or
  produce an output that is visibly different from a genuinely clean attempt.
- **Five modules currently produce `HealthFinding`s** (health_evaluator.rs ×7,
  characterization.rs ×4, health.rs ×3 — schema/threshold producers, not findings per se — backend_
  selection.rs ×2, worker.rs ×1), two of which (characterization.rs, backend_selection.rs) already
  build correct, schema-respecting findings **without** going through `evaluate_health` at all.
  Any design has to say what happens to those two, not just to the four-parameter function.

## 3. Design A — a Recorder threaded through the compile

A narrow, write-only sink, threaded by mutable reference only through the small number of
**orchestration** functions that already own one whole attempt (the worker child's per-request
handler, `backend_selection`'s per-backend build loop, `composite.rs`'s per-word apply path) —
never through `emit.rs`/`compose_budget.rs`/`peel.rs`/`characterization.rs` themselves, which keep
returning their existing rich types exactly as today.

```rust
/// Write-only: no accessor exists, so no compiler pass can read accumulated health mid-compile.
pub trait HealthSink {
    fn record(&mut self, finding: HealthFinding);
    fn phase_attempted(&mut self, phase: Phase);
}

pub struct HealthRecorder {
    findings: Vec<HealthFinding>,
    attempted: Vec<Phase>, // small, `Phase` has 3 values; a `Vec` is fine, no need for a bitset
}

impl HealthRecorder {
    pub fn new() -> Self { .. }
    /// The only way out. Consumes the recorder so nothing can push into it after this point.
    pub fn finish(self) -> HealthOutcome {
        if self.attempted.is_empty() {
            HealthOutcome::NotMeasured
        } else {
            HealthOutcome::Measured(HealthReport::new(self.findings))
        }
    }
}
impl HealthSink for HealthRecorder {
    fn record(&mut self, finding: HealthFinding) { self.findings.push(finding); }
    fn phase_attempted(&mut self, phase: Phase) { self.attempted.push(phase); }
}

pub enum HealthOutcome {
    /// No phase of this attempt ever ran. Structurally distinct from a clean `HealthReport`.
    NotMeasured,
    Measured(HealthReport),
}
```

**Usage sketch**, at `worker.rs`'s existing call site:

```rust
let mut recorder = HealthRecorder::new();
recorder.phase_attempted(Phase::Compile);
for finding in health_evaluator::findings_for_emit_report(report) { // today's `emit_report_findings`, unchanged
    recorder.record(finding);
}
// same call site now also has a place to put what it did NOT have before:
if let Some(err) = compose_error_observed {
    recorder.record(health_evaluator::finding_for_compose_error(&err));
}
let outcome = recorder.finish();
```

**Where the seam sits.** At each orchestration point's own natural end (worker child's request
handler, `backend_selection`'s per-backend loop iteration, `composite.rs`'s per-word call). There
is no single new "top" — the three attempts remain independently owned, each builds and finishes
its own `HealthRecorder`.

**How `Ideal` becomes distinct from `NotMeasured`:** structurally, via `HealthOutcome`'s two
variants — `finish()` on a recorder that never had `phase_attempted` called returns `NotMeasured`,
never a `HealthReport` at all.

**Schema version:** moves. `HealthOutcome` is a new wire type wrapping `HealthReport`; if it
replaces `HealthReport` in `CompileWorkerResult`, every existing JSON consumer sees a new shape,
so `HEALTH_SCHEMA_VERSION` bumps to 8. This is the most disruptive of the three on the wire.

## 4. Design B — a typed measurement-set, absence explicit

Flip the direction: no mutable object threaded down; the caller assembles an immutable,
already-complete value describing exactly what ran, and *cannot* spell "nothing ran" the same way
it spells "one phase ran cleanly."

```rust
/// The non-empty set of phases this attempt actually reached. Cannot be empty by construction —
/// there is no `NonEmptyPhaseSet::none()` and no `Default` impl.
pub struct AttemptedPhases(Vec<Phase>); // first element required by the constructor below
impl AttemptedPhases {
    pub fn starting_with(first: Phase) -> Self { Self(vec![first]) }
    pub fn and(mut self, phase: Phase) -> Self { self.0.push(phase); self }
}

/// What one compilation attempt measured. Every optional channel stays `Option`/`Vec` exactly as
/// today — that part of the interface was never the problem (Section 1(b) already shows
/// `ComposeError` arrives typed wherever it's actually passed in). The fix is `phases`: a caller
/// cannot build this type at all without naming at least one phase it reached.
pub struct CompileMeasurements {
    pub phases: AttemptedPhases,
    pub payload_bytes: Option<u64>,
    pub emit_report: Option<EmitReport>,
    pub compose_errors: Vec<ComposeError>,
    pub apply_budget_trips: Vec<ApplyBudgetTrip>,
}

pub fn evaluate(measurements: CompileMeasurements) -> HealthReport { .. } // today's `evaluate_health`, body unchanged
```

**Usage sketch:**

```rust
let measurements = CompileMeasurements {
    phases: AttemptedPhases::starting_with(Phase::Compile),
    payload_bytes: None,
    emit_report: Some(report.clone()),
    compose_errors: vec![],       // legitimately none observed, now sits beside a *named* phase
    apply_budget_trips: vec![],
};
let health = health_evaluator::evaluate(measurements);
```

**Where the seam sits.** Unchanged from today: at whichever call site already has the measurements
in hand and wants a report — `worker.rs:331`'s exact spot. No new orchestration function is
required; only its signature call changes shape (four positional parameters collapse to one named
value plus a required, non-empty `phases`).

**How `Ideal` becomes distinct from `NotMeasured`:** at the *type* level, on the input side: there
is no way to construct a `CompileMeasurements` without asserting at least one phase, so there is no
value that means "nothing happened" — a caller with genuinely nothing to report (the process-fault
case `worker.rs::build_process_failure_health` already special-cases, worker.rs:584-594) uses a
different, separately named constructor (already true today, just not load-bearing against
`evaluate_health`'s own all-`None` shape). `HealthReport` itself, and its "empty findings vec is a
legitimate `Ideal`" convention (`an_empty_report_admits_within_limits`, health.rs:757-761), is
untouched — the fix lives entirely in what it takes to construct the *input*, not in `HealthReport`.

**Schema version:** does **not** move. `HealthReport`'s wire shape is untouched; only the
in-process Rust construction API for producing one changes. This is the cheapest of the three on
the worker-wire-compatibility constraint from Section 2.

## 5. Design C — an event log folded at the end

Neither a mutable sink nor a pre-assembled struct: an ordinary, owned, appendable `Vec<CompileEvent>`
threaded through by value (moved in, appended to, moved back out — a literal fold, no interior
mutability, no trait object, no lifetime). Existing producers keep returning their existing rich
types (`EmitReport`, `ComposeError`, …) exactly as now; each orchestration point additionally
pushes the value it already has onto its own log, and calls a pure fold function once, at its own
natural end, to get a report.

```rust
#[derive(Debug, Clone)]
pub enum CompileEvent {
    PhaseEntered(Phase),
    PayloadMeasured { bytes: u64 },
    EmitReported(EmitReport),
    ComposeErrorObserved(ComposeError),
    ApplyBudgetTripped(ApplyBudgetTrip),
    /// Escape hatch for a producer that has already correctly built its own `HealthFinding`
    /// (characterization.rs, backend_selection.rs today) — see Section 8's answer on those two
    /// modules for why this variant exists instead of forcing them through the raw shapes above.
    FindingProduced(HealthFinding),
}

/// Pure: same log in, same `HealthOutcome` out, every time. No I/O, no `&mut` anywhere.
pub fn fold_health(events: &[CompileEvent]) -> HealthOutcome {
    if events.is_empty() {
        return HealthOutcome::NotMeasured;
    }
    let mut findings = Vec::new();
    for event in events {
        match event {
            CompileEvent::PhaseEntered(_) => {}
            CompileEvent::PayloadMeasured { bytes } => findings.extend(payload_size_finding(*bytes)),
            CompileEvent::EmitReported(report) => findings.extend(emit_report_findings(report)),
            CompileEvent::ComposeErrorObserved(err) => findings.push(compose_error_finding(err)),
            CompileEvent::ApplyBudgetTripped(trip) => findings.push(apply_budget_trip_finding(trip)),
            CompileEvent::FindingProduced(finding) => findings.push(finding.clone()),
        }
    }
    HealthOutcome::Measured(HealthReport::new(findings))
}
```

**Usage sketch**, at `worker.rs`'s call site:

```rust
let mut events = vec![CompileEvent::PhaseEntered(Phase::Compile)];
events.push(CompileEvent::EmitReported(report.clone()));
if let Some(err) = compose_error_observed {
    events.push(CompileEvent::ComposeErrorObserved(err));
}
let outcome = health_evaluator::fold_health(&events);
```

**Where the seam sits.** Same answer as Design A: each of the three independently-owned attempts
(worker child, per-backend build loop, per-word apply) builds and folds its own log at its own
existing end point. Unlike Design A, the *log itself* — not just its folded `HealthReport` — is
plain, ordinary, `Serialize`-able data (every variant wraps an existing `Serialize` type), so it
can cross the worker wire unfolded if a future caller wants the raw sequence (e.g. to build a
CONTEXT.md-style "analysis failure narrative" or "FieldWorks investigation handoff" later) without
inventing a second recording mechanism for that.

**How `Ideal` becomes distinct from `NotMeasured`:** identically to Design A, via a two-variant
`HealthOutcome`, decided by `events.is_empty()` — an empty log never arises from a real attempt
(every orchestration point pushes at least a `PhaseEntered` before doing anything else), so it is
reserved for the "nothing was ever attempted" case.

**Schema version:** moves, same as Design A and for the same reason — `HealthOutcome` is a new
wire shape if it (or the log) reaches `CompileWorkerResult`. If `fold_health`'s *output* stays a
plain `HealthReport` (never `HealthOutcome`) and only a *new, additional* field is added
(`attempted: bool` alongside today's `findings`), the bump is smaller but still real: it's a
structural addition to `HealthReport` either way.

## 6. Comparison

| | Design A (Recorder) | Design B (Typed measurement-set) | Design C (Event log) |
|---|---|---|---|
| **Depth** | Deep once wired: one sink, many producers push into it without knowing about each other. Depth is diluted by needing a `HealthSink` reference threaded to every orchestration point. | Shallow by design on purpose: the interface is one struct, built once, evaluated once — the whole point is a small, honest, one-shot interface, not a facility. | Deep: one fold function, arbitrarily many cheap `Vec::push`es feeding it; the log itself needs no knowledge of `HealthFinding`'s construction rules. |
| **Locality** | Good within one attempt (all evidence lands in one recorder); poor across attempts (three independent recorders, no shared code forces them to agree on when to call `phase_attempted`). | Best: the *type* enforces the one invariant (non-empty phases) that actually mattered; nothing else to get wrong. | Good: `CompileEvent`'s variants are the single place every producer's "how do I report myself" question is answered once. |
| **Leverage** | Medium: callers get a sink, but must still remember to call `phase_attempted` — an easy step to forget, and forgetting it silently reproduces the exact bug being fixed. | High per call, low in breadth: one call site is trivially fixed; the two bypassing modules (Section 1's Section 2 constraint) get no help from this design at all, since they don't call `evaluate`. | High: the `FindingProduced` variant is a genuine leverage point for callers (like characterization.rs) that already do their own correct translation — they join the same lifecycle-wide record without being forced to un-translate and re-translate. |
| **Seam placement** | At each orchestration point, chosen well (matches where "one attempt" already lives) but requires a new object type threaded through call signatures that don't take one today. | Unchanged — the existing `evaluate_health` call site, reshaped. Cheapest to land, least invasive. | At each orchestration point, same as A, but the seam's *data* (not an object) can be handed to something else later (a narrative/handoff builder) without a second recording mechanism. |
| **Worker-wire safety** (Section 2) | Must convert to `HealthReport`/`HealthOutcome` before crossing; the `HealthSink` trait itself is correctly never exposed to serde. | Cleanest: nothing new crosses the wire at all. | Same as A — the *events* could cross the wire (they're plain data), but `HealthOutcome`/`HealthReport` is what worker.rs would still send. |
| **Fixes the "measured nothing" bug** | Yes, via `HealthOutcome`. | Yes, via unconstructable input. | Yes, via `HealthOutcome`. |
| **Unifies the 5 fragmented producers** (Section 1(d), 2's last bullet) | Possible, but only if all three orchestration points remember to route their local findings into a *shared* recorder — nothing structural forces that; today's fragmentation (worker.rs's compile-only report, `fst-health`'s characterization-only report never joining it) could persist unchanged. | No help — B only reshapes one function's parameters; characterization.rs and backend_selection.rs still have nothing to call. | Best positioned: `FindingProduced` gives every existing correct producer, including the two bypassing modules, one call (`events.push(CompileEvent::FindingProduced(finding))`) to join whichever attempt's log is in scope, with zero change to how they classify a finding. |

## 7. Recommendation

**Design C (event log), with Design B's "absence must be named" discipline applied to how a log is
started.** Reasoning, not a hedge:

- The actual defects found in Section 1 are not "the function signature is awkward" — they are
  "typed evidence exists and nothing routes it anywhere" (the dropped `EnumBudgetExceeded`, the two
  `ComposeError` sites that never reach the evaluator, `backend_selection.rs`'s always-empty
  `Vec::new()` on Accept). Design B fixes the parameter list but gives none of those three
  producers anywhere new to send evidence; the bug it targets (all-`None` reads `Ideal`) is real
  but narrower than the evidence found while investigating it.
- Design A and C both actually change something structural for the multi-producer problem; C wins
  on locality and worker-wire fit because its unit of data (one `CompileEvent`) is plain,
  serializable, and requires no new trait object plumbed through function signatures that don't
  already return one — a producer that already returns `EmitReport`/`ComposeError` today needs no
  signature change at all, only its orchestrator's one extra `.push(...)` line.
- Fold in Design B's core discipline anyway: `fold_health` should refuse to distinguish
  `NotMeasured` from `Ideal` by "did the `Vec` happen to be empty" alone, because an orchestrator
  could trivially forget to push anything and get a silently-wrong `Ideal` — the same shape of bug
  as today's, one level up. Require the log's first element to be a `PhaseEntered` (a cheap,
  checkable invariant, pinned by a test in the style of this crate's existing one-way gates, e.g.
  `the_published_closure_fact_never_over_claims_a_refusal`) rather than trusting "non-empty" alone.

## 8. The specific questions

**How does `ComposeError` reach the report typed, rather than flattened to prose at
`emit.rs:2067`? What happens to the two local consumers?**
It already arrives typed wherever anyone bothers to hand it to `evaluate_health`/`compose_error_
finding` (health_evaluator.rs:243-261) — that function's signature was never the problem for this
question. The problem is the two production sites that *don't*: `emit.rs`'s
`compound_chain_depth_and_budget_check` (destructures the error at emit.rs:2064 to build a prose
`reason` for `FomaTier::Unsupported`) and `composite.rs`'s peel path (threads it to `FomaOutcome.
peel_chain_depth_error`, flattened to a bool at `backend_runtime.rs:2108`). Under the recommended
design, both keep doing exactly what they do today (the prose stays, because a human-readable
`EmitResult`/`FomaTier::Unsupported` reason is still wanted, and `FomaOutcome`'s
`peel_chain_depth_error: Option<ComposeError>` field stays for its existing readers) — they
**additionally** push `CompileEvent::ComposeErrorObserved(err.clone())` onto whatever log is in
scope for that attempt. Nothing is removed; a second, structured channel is added beside the
existing prose/bool one. `emit.rs`'s dead `EnumBudgetExceeded` field (Section 1(c)) should also
gain a reader inside `fold_health`'s emit-report handling, independent of this question but
touching the same code path.

**Threading a recorder through the compile risks becoming a parameter passed everywhere — how does
this design avoid that? Is `GrammarSemantics`/`mechanism_provider.rs`'s "enforced by signature"
precedent applicable?**
Directly applicable, and it's why C is a `Vec<CompileEvent>` returned/threaded by value at three
fixed orchestration points, never a parameter added to `emit.rs`/`compose_budget.rs`/`peel.rs`/
`characterization.rs` themselves. `mechanism_provider.rs`'s doc (lines 4-11) states the discipline
precisely: *"`derive_mechanism_graph` takes `&GrammarSemantics` and no `&Grammar`. That is not a
convention this module promises to keep; it is the whole surface."* — enforced because
`GrammarSemantics::grammar()` is never called in that file, so the module structurally cannot
re-derive a fact it should be projecting instead (`lib.rs:221-225`: *"why the signature is the
enforcement"*). The analogous rule here: a compiler **pass** (`emit::emit`, `compose_budget::
ComposeBudget::check_chain_depth`, `peel::ReduplicationPeeler::peel_candidates`,
`characterization::characterization_findings`) never takes `&mut Vec<CompileEvent>` or any log
type in its signature — it keeps returning its own already-well-typed result exactly as today.
Only an **orchestrator** (a function that already owns "one whole attempt," identified in Section
2 as three existing places: the worker child's handler, `backend_selection`'s per-backend loop,
`composite.rs`'s per-word apply call) is allowed to hold a log, and it only ever *appends a value
it just received from a return*, never passes the log itself downward. This is call-site
discipline copying `GrammarSemantics`'s pattern one level up: instead of narrowing a pass's *input*
to one owning type, it narrows *who is allowed to accumulate* to a fixed, small set of already-
existing owners.

**What happens to the five finding-producing modules — do `backend_selection.rs` and
`characterization.rs` start reporting in, or stay as they are, and why?**
They start reporting in, but **their existing translation logic does not change**. Both modules
already call `HealthFinding::new` correctly and respect the severity/class agreement it enforces
(backend_selection.rs:279-323's `attach_capability_refusal`, :325-343's
`attach_operational_failure`; characterization.rs:291-323's `semantic_uncertainty_finding`,
:326-345's `cost_uncertainty_finding`) — they are already deep, correct producers, just
disconnected ones. Forcing them to un-translate into raw `CompileEvent` variants and let
`fold_health` re-translate would be exactly the "re-derive a decision another module makes"
mistake CLAUDE.md warns against, since the class/severity mapping these two modules encode (e.g.
`Refuse` → `Representability`/`CannotRepresent`, `ConfirmOnly` → `Readiness`/`LargeMultiplier`) is
their own domain knowledge, correctly placed. `CompileEvent::FindingProduced(HealthFinding)`
(Section 5) is exactly this: a typed escape hatch letting an already-deep producer join a shared
per-attempt record with a single `.push(...)`, contributing a finished fact rather than raw
measurements. Whether this is worth doing **today** is a separate question from whether the
*mechanism* should support it: Section 1's Section 2 constraint found no caller today that spans
characterization and compile in one attempt (fst-health is characterization-only by its own doc;
worker.rs is compile-only) — so wiring `characterization.rs` into the same log as a compile is
follow-on work gated on a caller that actually wants both, not a step this migration should force
in Section 9's ordering.

**Migration order keeping the tree green at every step, given (corrected) one real production
caller:**
1. Land the `EnumBudgetExceeded` read (Section 1(c)) inside today's `unsupported_tier_finding`,
   standalone, with a before/after differential test on the compound-chain-depth fixture. Zero
   interface change; pure bugfix; independently valuable regardless of which design ships next.
2. Add `CompileEvent`/`fold_health` as new, additive code beside the untouched `evaluate_health`.
   Implement `fold_health` by delegating to the *same* private helper functions
   (`payload_size_finding`, `emit_report_findings`, `compose_error_finding`,
   `apply_budget_trip_finding`) `evaluate_health` already calls — a pure refactor with no behavior
   change, and `evaluate_health` becomes a two-line wrapper that builds a 4-event log and calls
   `fold_health`. Every existing test (the ~350 lines) keeps passing unmodified, because
   `evaluate_health`'s observable behavior is unchanged.
3. Migrate `worker.rs:331`, the one real production caller, to build its own local
   `Vec<CompileEvent>` and push the `ComposeError` it can now observe from the compile it just ran
   (this may require `FomaProposer::new_proposer_with_profile` or its callee to also surface any
   `ComposeError` it hit, alongside the `EmitReport` it already returns — a scoped, named follow-on,
   not hidden inside this step). Green because the wrapper from step 2 still exists for anything
   not yet migrated, and worker.rs's own tests assert on `CompileWorkerOutcome`'s content, not on
   `evaluate_health`'s call shape.
4. Migrate `composite.rs`'s apply-time path (`peel_chain_depth_error`) to also emit a
   `ComposeErrorObserved` event into a per-word log, additive beside the existing `Option<
   ComposeError>` field so `backend_runtime.rs:2108`'s existing bool-collapsing behavior is
   unaffected until it opts in to reading the richer form.
5. Once steps 3-4 are shipped and their own new tests are green, delete the `evaluate_health`
   wrapper and its now-redundant call sites (`pack.rs`'s test, `health_evaluator.rs`'s own
   parameter-threading tests) in favor of calling `fold_health` directly — Section 9 covers exactly
   which of the ~350 lines survive this deletion and which don't.
6. `characterization.rs`/`backend_selection.rs` joining a shared log (the `FindingProduced`
   escape hatch) is explicitly **not** part of this ordering — it's follow-on work, gated on a
   caller that spans phases in one attempt, per the answer above.

**What replaces the ~350 lines of unit tests, and what becomes newly catchable?**
Most of them are not waste. The golden-JSON round-trip tests
(`fst_health_evaluator_golden_json`/`_golden_admission_is_cannot_represent`/`_golden_round_trips`,
health_evaluator.rs:800-869) pin `HealthReport`'s wire *schema*, which none of the three designs
touch — they belong to, and should probably move to, `health.rs`'s own test module (already true
of an equivalent golden there, health.rs:866-906), independent of this redesign. The per-
measurement-type mapping tests (`fst_health_evaluator_partial_tier_is_cannot_represent_coverage_
gap`, `..._unsupported_tier_with_no_closure_refusal_is_cannot_represent`, `..._chain_depth_
exceeded_is_apply_phase`, `..._apply_budget_trip_decoded_paths`/`_candidates`, and the closure-
refusal-cause tests) exercise the pure helper functions (`partial_tier_finding`,
`unsupported_tier_finding`, `compose_error_finding`, `apply_budget_trip_finding`) that `fold_health`
delegates to unchanged — these survive verbatim, because those functions don't change shape under
any design here. What gets **replaced** is the outer parameter-threading tests that only exercise
`evaluate_health`'s four-`Option`/`&[]` combinatorics with no real producer behind them —
`fst_health_evaluator_within_limits_payload_produces_no_finding` through `..._oversized_payload_
remains_not_production_ready_readiness`, and `fst_health_evaluator_empty_report_is_within_limits`
— by a small number of tests over `fold_health`: one asserting `fold_health(&[])` (or a log missing
a leading `PhaseEntered`) returns `NotMeasured`, never `Ideal`, and one per real orchestration path
(worker.rs, once migrated) confirming its log always starts with `PhaseEntered`. The concrete,
previously-uncatchable defect this makes catchable is the one named in Section 1(c): feed the
compound-chain-depth-exceeded `EmitReport` fixture (the exact shape `emit.rs:2076-2091` builds)
into the evaluator and assert the resulting finding's `value` is `MetricValue::Count(depth)` with
`threshold = Some(MetricValue::Count(limit))` — a test that would fail **today** (it would observe
`MetricValue::Unbounded`) and passes only once step 1 of Section 9 lands.

**Does `evaluate_health` survive at all, or dissolve?**
Dissolves as a public entry point, survives as internals. Its four private helper functions
(`payload_size_finding`, `partial_tier_finding`, `unsupported_tier_finding`,
`backend_compilation_failed_finding`, `emit_report_findings`, `compose_error_finding`,
`apply_budget_trip_finding`) are already correctly factored, deep, and independent of the
parameter-shape problem — they become `fold_health`'s implementation, unchanged. The public,
four-`Option`/`&[]` signature at health_evaluator.rs:314-319 is exactly the shallow part (a thin
pass-through matching this file's own "deletion test": deleting `evaluate_health` today moves
almost nothing, because two callers each pass one measurement) and is what Section 9 deletes once
`fold_health` has a real caller.

## 9. Open items for the reviewer

- Section 1(a)'s correction (one real production caller, not two) changes how much migration risk
  Section 9 actually carries — worth confirming against a fresh `grep` before relying on it, in
  case another caller was added between this document and implementation.
- `FomaProposer::new_proposer_with_profile`/its callee do not currently surface a `ComposeError`
  to `worker.rs` at all (only an `EmitReport`) — Section 9 step 3 depends on a small, separate
  signature change there that this document has not designed; flagging it rather than hiding it
  inside the migration step.
- Section 8's `characterization.rs`/`backend_selection.rs` answer assumes no caller today spans
  characterization and compile in one attempt. I searched for one and found none, but a reviewer
  closer to the CLI roadmap (`pangloss make-report`, referenced in `docs/research/pg-cli-make-
  report-design-notes.md` per `fst_health.rs:2`) may know of a planned one that would move step 6
  from "follow-on" to "in scope now."
