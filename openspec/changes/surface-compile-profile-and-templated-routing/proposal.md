## Why

Indonesian currently reaches a typed Tuned Surface resource finding under the managed attempt,
although the complete static walk is just beyond the default logical-work setting. The existing
test-scoped larger run is useful evidence, but it is not yet a product resource-envelope contract:
it passes a closure-only number, has no immutable attempt identity or retry link, and static backend
admission is not the same thing as a trusted production build. A characterization path must also
have a total terminal result; a depth guard must never turn live successors into an apparently
complete result.

The historical Indonesian P6 result used the separate `uflexc + replace` prototype and reached
97/97 only inside its declared non-reduplication scope.  The production hand-spun emitter was not
retired; it is today's `TunedSurfaceProbed` backend.  This change therefore certifies the capacity,
completion, and artifact identity of that still-live production route.  It does not treat the P6
prototype result as evidence that a Tuned Surface artifact was built or selected.

Templated routing has the complementary problem. A capability report can say that a backend is
admissible while the selected candidate is later skipped, marked, or built by a different emitter.
That cannot produce a trusted route. Selection must be coupled to the backend actually realized by
the production compiler, with every omission and construction failure visible and fail-closed.

## What Changes

- Define a closed, named, versioned `ResourceEnvelope` for the complete immutable compile attempt.
  It records the repository's worker watchdog and communication bounds, deterministic compose and
  enumeration budgets, and Tuned Surface closure work; it is not a closure-only alias or an
  arbitrary numeric limit.
- Extend the repository's canonical immutable `BuildReport` for one compilation attempt; do not
  introduce a parallel attempt or receipt artifact. An explicit retry creates a new build report
  carrying its envelope identity/digest and `retry_of` link to the prior report; no default path
  enlarges a budget or retries behind the caller's back.
- Replace partial/optional characterization evidence with a total terminal result. The
  characterization and production walk share the same transition semantics, report pending work
  and depth-bound successors, and reject any result that is not complete with an empty worklist.
- Treat actual construction under the named envelope as acceptance. A static report may explain
  capability and cost, but it cannot select a normal backend or claim a trusted artifact without a
  successful production build and proposer-to-confirm evidence.
- Couple backend selection to the lower-layer completed-build value: requested and realized backend, plan/build
  fingerprint, envelope identity, and completeness must agree. Refuse candidates with skipped
  material, uncovered/gap diagnostics, technical markers, closure refusals, or a different realized
  backend; never fall back from one backend to another while retaining the original name. The sole
  canonical `BuildReport` embeds/serializes that value; `pg-foma` never depends upward on `pg-cli`.
- Make worker success transport the finalized Foma network payload (within the named communication
  bound), its fingerprint, the parsed-grammar identity, and the envelope digest. Counts from a
  throwaway child build cannot certify a different in-process rebuild; the parent verifies grammar
  and envelope identity, and selection/runtime/package writing consume the returned payload. The
  canonical build report records only evidence and fingerprints, never the FST bytes owned by the
  analysis artifact.
- Correct the static cards so controls name only the public caller API and closed envelope IDs. No
  card may name a nonexistent process environment variable or imply that corpus recall alone is a
  capability proof.
- Add focused Indonesian resource/retry, characterization terminal/parity, realized-routing, and
  card-control gates. The gates must distinguish static evidence from actual construction evidence.
- Preserve backend provenance in every historical and current result: `uflexc + replace` prototype
  evidence cannot certify `emit` / `TunedSurfaceProbed`, even when both accept the same words.

## Capabilities

### New Capabilities

- None as a standalone `openspec/specs/` capability. The contracts are enforced by the named Rust
  APIs, immutable attempt evidence, and focused tests below.

### Modified Capabilities

- None in an `openspec/specs/` tree; executable behavior remains in code and tests.

## Scope and Non-goals

The scope is Indonesian resource-envelope/retry behavior and Templated/Tuned backend routing after
the sibling morphology coverage change. It does not change HermitCrab semantics, add candidate
filters, turn corpus recall into capability proof, or address any other reference language.

It does not authorize automatic retry, partial or best-effort FSTs, arbitrary per-call product
limits, process-environment configuration for the named envelopes, or backend substitution after a
build failure.

## Dependencies and Impact

The implementation depends on the existing worker protocol/watchdog, `ComposeBudget`, enumeration
budget, capability reports, production emitters, and typed health findings. Primary modules are
`characterization.rs`, the shared traversal-kernel and trace regions of `preexpand.rs`/`emit.rs`, the attempt
evidence seam, `backend_selection.rs`, `backend_runtime.rs`, and `backend_cards_data.rs`. The
Templated capability predicates remain owned by the sibling morphology change.

The public effect is that callers choose a named complete attempt envelope and can request a linked
retry. A compatibility report remains useful diagnostic evidence, but only a matching, complete,
trusted production build is selectable for normal generation.

## Acceptance Evidence

- Managed Indonesian characterization terminates with a typed incomplete/resource terminal result;
  it never reports completion merely because a fixed depth or closure counter stopped the walk.
- An explicit larger named envelope reruns from clean state, links to the managed attempt, records
  the prior terminal finding, and reaches a complete terminal result with empty worklist.
- The same larger envelope is used by the actual Tuned Surface production construction. The build
  report's completed-build outcome proves the requested backend, realized backend, envelope identity, complete closure,
  absence of skips/gaps/markers, and the fingerprint of the returned FST payload. A separate
  canonical `AssessmentReport`, linked by that compiled-model/build-attempt fingerprint, proves
  proposer-to-confirm parity on the declared Indonesian suite.
- A synthetic candidate whose requested backend differs from the realized backend, or whose build
  has a marker/gap/skip, is refused even when another backend would succeed.
- Card contract tests accept only controls backed by the caller API and reject the old nonexistent
  environment-variable name.
