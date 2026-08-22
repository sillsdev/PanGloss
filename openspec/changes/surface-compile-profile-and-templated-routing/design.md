## Context

The compiler has three backends and already distinguishes correctness capability from graded
compilation health. Indonesian is the concrete resource case for this change: its complete Tuned
Surface walk is just beyond the managed logical-work setting, while a larger test run has observed a
finite closure. That observation is not, by itself, a trusted build or a product resource-envelope
contract.

The repository definition of a resource envelope is the named, versioned combination of parent
worker limits, sampled resource guardrails, bounded communication, and deterministic logical work
budgets under which a pipeline is accepted. A Tuned Surface closure count is one field in that
envelope, not the envelope itself. The envelope and the result of an attempt are immutable evidence.

The current characterization seam also has two correctness hazards. It returns an optional finding
instead of a total terminal result, and its depth guard can observe a successor and then stop
without carrying that live work into the result. A production build must use the same transitions;
otherwise a static complete result can certify a different walk from the one that emits the FST.

Finally, capability selection currently has a static/report-only shape while runtime evaluation can
realize a different emitter (including fallback from a plan candidate to Tuned Surface). A backend
is selectable only when the named backend actually produced a complete, trusted artifact under the
named envelope. Corpus recall is evidence of that artifact; it is not a substitute for construction
completeness or capability admission.

## Goals / Non-Goals

**Goals:**

- Define a closed, versioned full `ResourceEnvelope` and immutable attempt evidence.
- Make caller-requested retries explicit, fresh, and linked to the prior terminal attempt.
- Make characterization total and fail closed on every incomplete or depth-bounded path.
- Prove parity between characterization transitions and production construction transitions.
- Require actual construction under the selected envelope before normal backend selection.
- Refuse backend skips, gaps, technical markers, closure refusals, and requested/realized mismatch.
- Make capability cards name only controls exposed by the caller API.

**Non-Goals:**

- Changing HermitCrab semantics, morphology lowering, or the sibling Templated coverage predicates.
- Automatic retry, automatic envelope escalation, partial FSTs, or best-effort trusted artifacts.
- Treating corpus recall as a capability proof or adding a language-specific routing exception.
- Adding process-environment configuration for product envelope selection.

## Decisions

### 1. The resource envelope is a complete immutable attempt profile

Introduce a closed `ResourceEnvelopeId` and a value `ResourceEnvelope`. The shipped profiles are
repository-defined constants, for example `managed-v1` and `tuned-surface-work-10k-v1`; callers
select an ID, never an arbitrary product-facing `usize`.

Each profile freezes the full set of controls that can affect acceptance of one native compile
attempt:

| Dimension | Recorded controls |
|---|---|
| Worker/watchdog | protocol version, parent wall-time limit, sampled-RSS guardrail and sample interval |
| Communication | request/result frame limits and captured output limits |
| Compose work | state, arc, tuple, group, line, chain-depth, and ordering-multiplicity caps |
| Enumeration work | composite-entry and pair-probe caps |
| Backend work | Tuned Surface closure-work cap plus any backend-specific deterministic counters |

The exact repository types (`WorkerLimits`, `WatchdogEnvelope`, `ComposeBudget`, and
`EnumerationBudget`) are the source of these fields; the profile must not silently read a process
environment after selection. A canonical serialization and digest cover every field, including
defaults and the envelope schema version. Changing any field produces a different envelope identity.
Assessment/apply/oracle budgets remain separate named assessment context, but if an acceptance gate
uses one it must record its effective value in the immutable evidence rather than omit it.

The 10,000 value therefore means the Tuned Surface member of a complete profile has a 10,000
logical-work cap. It does not create a closure-only `TunedClosureEnvelope`, and it does not waive
the worker, communication, compose, enumeration, or build-completeness controls.

Tests: the envelope contract pins IDs, schema version, canonical digest, every field, and the fact
that changing a non-closure field changes the digest. It rejects arbitrary numeric product limits and
the old environment-variable name.

### 2. Attempts and retries are immutable and explicitly linked

Extend the repository's canonical immutable `BuildReport`; do not create a parallel
`CompileAttempt` or `TrustedBuildReceipt`.  One `BuildReport` continues to describe exactly one
compilation attempt and additionally contains:

- a fresh attempt ID, grammar/source fingerprint, requested backend (when any), and envelope ID;
- the canonical envelope snapshot/digest and effective deterministic counters;
- one total terminal result and all findings, including the terminal resource finding; and
- `retry_of: Option<AttemptId>` (null for the first attempt); and
- the successful finalized Foma payload fingerprint, or no payload fingerprint on failure.

The default call creates one `managed-v1` attempt and stops. A retry is a caller operation that
names a different closed envelope and the prior attempt ID, starts from clean grammar/compiler
state, and emits a new `BuildReport`. It never mutates or replaces the prior report, and the new report
retains a machine-readable link and prior terminal finding. A larger profile is not an automatic
fallback from a smaller one, and a capability override is not a resource retry.

Word cases and semantic equality never enter `BuildReport`.  The canonical `AssessmentReport`
remains the one artifact for caller-supplied word runs and records the compiled-model fingerprint
and build-attempt ID it assessed.  Semantic deltas compare assessment reports joined on their
caller-issued case IDs; a retry does not mutate an earlier assessment.

Dependency direction and artifact ownership stay acyclic.  `pg-foma` defines one non-persisted
`CompletedBackendBuild { evidence, payload_bytes }`.  Its immutable `evidence` contains the
requested/realized strategy, grammar identity, envelope/certificate, gap/skip/marker counters, FST
measurements, and payload/model fingerprint; `payload_bytes` contains only the finalized Foma
network.  The worker and backend selector produce/consume the whole value inside the lower layer.
`pg-cli::diagnostics::BuildReport` embeds and serializes exactly the `evidence` projection (or a
typed failed outcome), never `payload_bytes`.  The pack/analysis-artifact seam consumes the bytes
and records their fingerprint.  `CompletedBackendBuild` is not a second report, receipt file, or
public evidence artifact, and `pg-foma` never imports `pg-cli`.

Tests: a managed Indonesian run ends once with its resource terminal; an explicit
`tuned-surface-work-10k-v1` request creates a distinct linked attempt; a spy proves the default
path performs no second envelope evaluation.

### 3. Characterization has a total terminal result and no silent depth drops

Replace the `Option<HealthFinding>` characterization seam with a total result, for example:

```text
CharacterizationResult {
    terminal: Complete | Incomplete(reason) | Refused(reason),
    evidence: ClosureEvidence,
}
```

`ClosureEvidence` always reports the effective envelope, work/probe/success counters, maximum depth,
per-depth counts, pending successor count and rule ordinals, and whether the worklist is empty.
Every exit path sets `terminal`: complete exhaustion, a logical/worker budget boundary, a depth
boundary, an unbounded transition, an unsupported transition, and an internal construction fault
are all named outcomes. There is no `None` that can be read as success and no partial network that
can be labeled complete.

The characterization walk and production walk share the same transition kernel and transition
ordering. Characterization may run in observation mode, but it must apply the production
morphotactic transition, feature check, synthesis, successor admission, depth accounting, and
work-budget checks exactly. If a depth boundary finds a legal successor, it records that successor
as pending and returns `Incomplete`; it never just `continue`s. A `Complete` result requires an empty
worklist, no pending successors, and no unreported transition. The fixed depth constants in the
existing emitters are resource evidence only; reaching one is never a success condition.

Production-transition parity is an acceptance invariant. The parity gate compares terminal state,
worklist emptiness, visited pair/probe counts, synthesized successors, pending ordinals, maximum
depth, and per-depth counters for characterization and the production construction on the same
grammar and envelope. Any mismatch is a typed failed attempt, not a warning and not a route to a
different backend.

Tests: synthetic below/at/over-bound and depth-boundary fixtures prove terminal completeness and
pending-work reporting. The Indonesian gate pins the complete walk (including its depth and empty
worklist) and compares the observation counters with the actual production transition trace.

### 4. Actual construction under the named envelope is acceptance

Static characterization is pre-build evidence only. A normal/proven build requires the same
`ResourceEnvelope` snapshot to be passed through the worker, production emitter, Foma compilation,
and canonical `BuildReport`. The successful build outcome must prove:

1. the backend's own capability decision is `Admit` or `ConfirmOnly`, not `Refuse`;
2. characterization is `Complete` for that backend and envelope;
3. the production emitter reached its terminal complete state and emitted a usable network;
4. no `uncovered` rule/material, skipped root/rule/subtree, technical marker, closure refusal,
   enumeration-budget breach, or compile failure remains; and
5. the compiled network fingerprint and envelope digest are recorded for the resulting artifact;
   and
6. the worker returned the finalized serialized Foma payload whose fingerprint is recorded, and
   selection/runtime/package writing consume those bytes rather than rebuilding the network.

For Tuned Surface, the explicit Indonesian retry must therefore run the real production
construction under `tuned-surface-work-10k-v1`; the static 3,290-pair/3,072-successor observation is
not acceptance by itself. A separate `AssessmentReport`, linked to that build-attempt/model
fingerprint, then checks complete analysis-set containment on the declared Indonesian suite. If the
private suite is unavailable, assessment evidence is `not_run`, never a passed substitute for the
successful build report.

`CompileWorkerOutcome::Success` therefore transports the finalized
`FomaProposer::foma_binary_payload()` bytes and their digest, subject to the named envelope's result
frame/payload limit.  It also returns the parsed grammar/source identity and effective envelope
digest.  The parent verifies all three against the confirmer grammar and compile request,
reconstructs the proposer from those exact bytes, and writes those bytes into any analysis artifact.
A payload too large for the result bound
is a typed incomplete resource outcome.  A worker result containing only state/arc counts is useful
health evidence but is not a successful trusted build and cannot be selected.  This change does not
invent a second FST format: it uses foma's existing binary-memory encoding and existing reader.

An Error resource finding keeps the prior evidence and emits no trusted artifact. A clean retry may
produce one only after the conditions above hold. A development capability override, if already
available, remains visibly unproven and cannot enter normal selection.

### 5. Selection is coupled to the actually realized trusted build

Keep one report for every committed backend, including refused, missing, and failed reports. Split
diagnostic characterization from normal selection:

- a report made from capability/envelope characterization alone explains possible routes but has no
  selectable normal candidate;
- a normal candidate additionally requires a successful `CompletedBackendBuild` for the same
  requested backend, grammar fingerprint, envelope digest, complete certificate, and realized
  network/payload fingerprint;
- `requested_strategy == realized_strategy` is mandatory; a report for another emitter cannot
  satisfy the request; and
- a completed-build value with any skipped material, uncovered/gap diagnostic, technical marker, pending closure,
  or backend construction failure becomes `Failed`/`Refused` and remains visible in the report.

The selector must not fall back from Plan Composed to Tuned Surface (or from Templated to another
backend) while retaining the original candidate name. If another backend was independently built,
it receives its own completed-build value and eventual build report. `preferred()`/`selected()`
return only matching successful completed-build values; otherwise they return no normal path while
preserving every reason.

Tests: a successful named Tuned build selects Tuned with matching completed-build fields; a static-admitted
candidate with a different realized strategy, marker, skip, gap, or closure refusal is not selected.
The test also proves no fallback build is attributed to the candidate that requested another backend.

### 6. Cards name only real controls

The Tuned Surface card's control is the caller-visible compile-request field, such as
`CompileRequest.resource_envelope`, with the closed IDs `managed-v1` and
`tuned-surface-work-10k-v1`. It must not mention the old closure-budget environment variable, an
internal numeric argument, or any other environment variable. A card uses `Inherent` when no caller control exists;
it uses `SwitchControlled` only when its switch ID resolves to an actual public request field.

Cards stay static, language-free, and corpus-free. They describe contributors, asymptotic shape,
source references, and cataloged remedies; they do not claim that a corpus pass proves capability or
that a named profile guarantees a build for every grammar.

Tests: card rendering checks every switch ID against the public-control registry, lists both closed
profile IDs, links valid advice entries, and rejects the removed environment-variable spelling.

## File and phase ownership

Phase A owns the full envelope/evidence value types and total characterization in
`characterization.rs` plus the shared traversal kernels and production-trace/characterization
regions of `preexpand.rs`/`emit.rs`.  Production behavior may change only as needed to consume the
same transition kernel and expose parity evidence.  Phase A must release the production-transition
parity and clean terminal-result contract before morphology coverage work proceeds. It does not
publish a second build-report type.

After the sibling circumfix/template change, phase B owns `backend_selection.rs`,
`backend_runtime.rs`, `worker.rs`, the canonical `pg-cli/src/diagnostics.rs` build-report schema, the
narrow `pg-cli/src/pack.rs` payload-consumption seam, backend-card data, and focused routing tests.
Phase B consumes the envelope and certificate from phase A, returns the actual finalized Foma
payload from the worker, and extends the existing `BuildReport`; it does not weaken a capability
predicate, introduce a parallel receipt, or substitute corpus evidence for a build. No other
morphology or reference-language scope is added here.

## Migration and rollback

1. Add the closed full-envelope type, immutable attempt fields, and failing acceptance tests while
   preserving the managed default's single-attempt behavior.
2. Thread the complete profile through characterization and production construction; remove the
   product-facing closure-only limit and make all terminal outcomes explicit.
3. Add the production-transition parity and Indonesian explicit-retry gates.
4. After the sibling morphology change, make worker success return the finalized payload, extend
   the canonical build report, require matching successful build evidence in selection, and remove
   backend fallback attribution.
5. Update cards and run the focused merged-tip gates.

Rollback is a revert of the envelope/build-evidence/selection change. It must not restore silent depth
drops, automatic retry, or a card control that does not exist; any compatibility shim remains
diagnostic-only and may not select a trusted backend.

## Open Questions

None. The public envelope schema, retry linkage, terminal states, build-evidence requirements, and card
control rule are part of this change; a future persisted artifact schema can version these fields
without weakening the acceptance boundary.
