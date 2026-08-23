# Grammar Compiler and Four-Language Recipe Parity Plan

> **Execution rule:** Use isolated reusable worktrees for implementation, commit-based handoff,
> managed Rust commands, Luna implementation agents at medium or higher effort, Luna research at
> xhigh, and fresh Sol/xhigh review for the architecture gates. The primary agent reviews every
> diff and owns integration.

> **Historical/superseded acceptance scope.** This four-language recipe-parity plan is retained as
> provenance. The current shipping slice is Indonesian, Amharic, and Aweti.

**Goal:** Establish one maintainable grammar-compilation architecture, then implement explainable,
optimizable subrecipes and certify recipe parity for Indonesian, Sena, Amharic, and Aweti without
language-name routing or incomplete-corpus claims.

**Architecture:** `pg-grammar::BoundGrammar` owns the exact source/compiler/options binding and
`ModelRevision`. `GrammarSemantics` is the only owner of typed linguistic fact derivation for that
revision. Capability,
registry applicability, mechanism providers, and recipe-space accounting are typed projections.
The Registry alone constructs an `ExecutableCandidate` containing a portable Plan, exact lowering
adapter, runtime requirements, mechanism bindings, and stable digests. Runtime lowers exactly that
candidate, caches the lowered result, and certifies canonical analysis sets against a versioned
`CorpusSnapshot`.

## Non-negotiable claim levels

| Level | Meaning |
|---|---|
| Observed | A typed grammar/source fact with resolvable provenance. |
| Derived | A capability, mechanism, applicability result, or Plan inferred from observations. |
| Executable | Registry validation proves a complete Plan/adapter/runtime contract exists. |
| Measured | A real build/run produced counters or timing, with cache state recorded. |
| Certified | The complete eligible corpus snapshot matches exactly with no oracle omission. |

Generality is aggregate evidence breadth, not a sixth candidate status. A dossier fixture is not an
executable candidate; an executable candidate is not a successful build; a measurement is not a
correctness proof; certification is scoped to exact semantic, candidate, and corpus digests.

## Compiler seam

```text
Grammar source
  -> CompilerInputDigest -> successfully compiled BoundGrammar + ModelRevision
  -> GrammarSemantics + typed revision-scoped provenance
  -> capability / applicability / mechanism / recipe-space projections
  -> Registry-owned ExecutableCandidate
  -> exact adapter lowering + run-scoped LoweredCandidateCache
  -> evaluation against CorpusSnapshot
  -> pg-assess AnalysisIdentity set + annotation/duplicate evidence lineage
  -> observed -> derived -> executable -> measured -> certified
```

Physical lowerers may read grammar payloads to construct FSTs. They may not rediscover whether a
linguistic mechanism applies or silently choose a different adapter.

## Round 1 — one semantic and evidence spine

### 1A. Typed `GrammarSemantics`

- [ ] Extract shared domain-framed SHA/JCS mechanics into a leaf digest crate; keep domain identity
      types with their owners.
- [ ] Move exact source/compiler-input binding into `pg-grammar`; a `ModelRevision` exists only for a
      successfully compiled `BoundGrammar` and binds canonical compiler input, compiler
      contract/build identity, and options. Keep compatibility re-exports from `pg-assess`.
- [ ] Add immutable, closed-domain `GrammarSemantics::derive(&BoundGrammar)` and typed errors.
- [ ] Move the authoritative capability traversal into that owner. A temporary
      `capability::characterize` wrapper may delegate; it may not retain another implementation.
- [ ] Add a versioned `CapabilityProjectionDigest` over the typed capability projection. It is
      intentionally many-to-one and must never identify a grammar, artifact, candidate, cache,
      corpus, or certification scope.
- [ ] Preserve authored order and typed model/source provenance. Dense ordinals are meaningful only
      under `ModelRevision`.
- [ ] Prove fresh-load stability, name independence, authored-order sensitivity, unordered-set
      stability, and compatibility-projection equality.

Do not put mechanism graphs, Registry facts, Plan seeds, string fact keys, opaque payloads, or a
generic evidence rule engine inside the semantic snapshot.

### 1B. Delete the other grammar-truth walkers

- [ ] Make registry applicability consume typed semantic features; delete
      `Applicability::matches(&Grammar)` and local reduplication discovery.
- [ ] Make recipe-space counts consume the snapshot; delete `GrammarFacts::from_grammar` and its
      local gate/template/rule walkers.
- [ ] Make capability entry, preflight, and planning consumers share one derived snapshot per load.
- [ ] Add a source boundary test/scan preventing new semantic grammar walkers outside the owner.

### 1C. Canonical evidence

- [ ] Reuse `pg-assess::AnalysisIdentity` v1 as the sole public cross-engine identity: ordered stable
      morpheme source keys, root position, and optional stable category/POS key. Do not add a parallel
      `AnalysisKey` type or use full Rust `WordAnalysis::Eq` for public certification.
- [ ] Compare deduplicated identity sets for selectability. Preserve repeated corpus occurrences,
      distinct identities, guessed annotations, and duplicate-discovery counts as separate typed
      evidence; duplicate copies of one identity do not change selectability.
- [ ] Make shared analysis projection and set construction fallible. Reject empty/colliding stable
      source keys, unresolved ordinals, duplicate-count overflow, conflicting `guessed`
      annotations, and supplied-root/sentinel ambiguity instead of debug-asserting or OR-merging.
- [ ] Project oracle results when `PreparedCorpus` is created and candidate results through the same
      shared projector; retain each repeated corpus row as a distinct occurrence.
- [ ] Bind every set comparison to identity profile, authority, source/model revision, semantic
      `ModelRevision`, parse options, and corpus snapshot. Naked identity values are not comparable.
- [ ] Add `CorpusSnapshot` with schema, raw/requested/eligible digests, occurrence order,
      normalization policy, exclusions, oracle settings/outcomes, and `ModelRevision`.
- [ ] Make missing requested occurrences, multiplicity mismatches, caps, timeouts, invalid inputs,
      or revision/scope mismatch typed non-certifying outcomes. Never silently continue.
- [ ] Domain-frame every digest. Do not hash concatenated raw inputs without type and length framing.
- [ ] Reject supplied roots as v1-non-comparable; four-language certification uses grammar-only
      provenance with guessing disabled. A future supplied-root v2 must be versioned rather than
      widening v1 in place.
- [ ] Keep traces diagnostic-only and explicitly exclude trace parity. Current tracing changes merge
      behavior, uses compiler-local sources, and has no Foma counterpart.
- [ ] Replace recipe runtime's full `WordAnalysis::Eq`, exact-vector-length check, and dense tuple
      helpers with shared identity-set comparison, separate annotation mismatch, duplicate-count
      evidence, and typed non-comparable reasons.

### 1D. Research dossiers

- [x] Maintain all six dossiers with scope/non-scope, two family/construct anchors, chosen and
      rejected architectures, correctness/failure obligations, Big-O, two exercises where possible,
      exact proposed tuples, mutations, research log, and fits/refines/splits-adds decisions.
- [x] Record source/model evidence, resource caps, and counters as measured or canonically
      unmeasured. Proposed fixture IDs use `proposed:*`; an evidence status is never an ID.
- [ ] Replace proposed tuples with executable fixture/model IDs and real counters only as each
      vertical slice lands.

**Round 1 exit:** one semantic owner; canonical corpus/equality definitions; no silent evidence
omission; dossiers remain honestly classified as research until executable evidence exists.

## Round 2 — one executable candidate and lowering path

### 2A. Portable physical Plan

- [ ] Add an executable `PlanDocument` in `plan.rs` covering every node payload and rejecting
      dangling roots/children, cycles, duplicates, and invalid combinations.
- [ ] Round-trip to/from `Plan`; compute canonical schema-tagged SHA-256 `PlanDigest` independent of
      insertion order and CRLF/platform text conventions.
- [ ] Rename the current diagram projection to `PlanExplanationDocument`; it must not deserialize as
      an executable Plan.
- [ ] Keep FNV `NodeId` only for in-process interning, never artifacts or persistent cache identity.

### 2B. Registry-owned `ExecutableCandidate`

- [ ] Give the Registry sole construction authority over private candidate fields: typed role,
      model revision, recipe identity, mechanism graph/bindings, portable Plan, exact adapter,
      existing runtime requirements, provenance, and candidate digest.
- [ ] Separate `ExecutableInputDigest` (model revision, Plan, adapter/lowerer identity/options,
      runtime requirements) from `CandidateDigest` (executable input plus recipe/version/parameters,
      mechanism bindings, and registry/policy schema). Two candidates may share one lowered artifact
      while retaining distinct provenance.
- [ ] Include every execution-affecting Plan, adapter option/version, runtime manifest, and binding;
      exclude labels, language names, fixture names, and ephemeral IDs.
- [ ] Delete `CandidatePlan`, positional baseline booleans, duplicate wire/runtime projections, and
      selectable identity/permutation families that do not implement their named construct.

### 2C. Exact lowering and reuse

- [ ] Remove the implicit `PlanComposed -> TunedSurfaceProbed` fallback. Failure is typed; a candidate
      never executes an adapter other than the one it declares.
- [ ] Make runtime operations such as reduplication peeling explicit candidate requirements.
- [ ] Cache lowering by typed `ExecutableInputDigest`, not `CandidateDigest`, across pilot/full
      evaluation. Cache hits retain original
      measurements plus cache status; missing measurements are unknown, never zero.
- [ ] Centralize enumeration, accounting, lowering cache, evaluation, interruptions, and claim
      transitions in one recipe-run module. The CLI loads inputs and serializes results only.
- [ ] Remove inert branch-and-bound/provable-tie production signals until a real admissible bound
      exists.

### 2D. Build orchestration cleanup

- [ ] Preserve shared bounded caches and at most two measured 19 GB jobs when headroom supports them.
- [ ] Port the direct ProcGov invocation fix; retain resource limits without `Start-Process` terminal
      minimization behavior.
- [ ] Simplify ownership/lease cleanup and prove stale descendants before retrying; a timeout never
      launches a duplicate build blindly.
- [ ] Keep source isolation separate from build-cache isolation: agents use distinct worktrees and a
      shared warm cache unless binary conflicts require otherwise.

**Round 2 exit:** declared adapter equals realized adapter; portable Plan round-trip/digest gates are
green; pilot/full reuse the same lowered artifact; all production semantic consumers use the single
snapshot; managed pg-foma and pg-cli gates pass.

## Mandatory second cleanup audit

- [ ] After Round 2 exit and before any mechanism becomes selectable, run a time-boxed Luna/xhigh
      cleanup audit of duplicate ownership, compatibility wrappers, fake measurements, portability,
      and deep-module boundaries.
- [ ] Have the primary agent verify every cited survivor/deletion and resolve blockers.
- [ ] Use a fresh Sol/xhigh reviewer if the audit exposes a consequential architecture choice.

The audit must specifically search for raw-grammar applicability, the old recipe-space walker,
capability mirror traversal, `CandidatePlan`, `EmissionStrategy`, implicit fallback, positional
baseline arrays, duplicate artifact/runtime types, diagram Plan execution, FNV persistence, and
zero-valued missing measurements.

## Round 3 — typed mechanism vertical slices

Shared mechanism types stay small; provider-specific logic lives in separate modules and consumes
only `GrammarSemantics`. Implement sequentially:

1. [ ] `Morphotactics -> BoundaryCleanup`.
2. [ ] `StaticPartition -> OrderedPhonology`.
3. [ ] `StructuralAllomorph`.
4. [ ] bounded and explicitly peeled `CopyProcess`.
5. [ ] multi-stratum, compounding, zero-morphology, and remaining interactions.

Each mechanism must have at least two orthogonal synthetic exercises where possible, typed source
facts/contracts, no language-name routing, exact `AnalysisIdentity` set checks plus separate
annotation/duplicate evidence, and measured resource counters/caps. Do not claim priority depth,
partition stability, boundary state, exactness, or copy
bounds unless the semantic snapshot proves them.

## Cross-cutting review wave

After the six mechanisms have at least two exercises, run 2–4 Luna/xhigh read-only reviews, at most
two concurrently:

1. semantic provenance, generalization, and language-name-routing;
2. Plan/adapter/cache portability, Windows behavior, and resource accounting;
3. runtime parity, certification scope, and measurement honesty;
4. computational-linguist explanation and deep-module locality.

The primary agent reviews the evidence and diffs. A fresh Sol/xhigh agent then adjudicates the
combined findings and implements—or directs isolated implementation of—the accepted set before the
four-language certification run.

## Four-language exit gate

- [ ] Indonesian: versioned raw/requested/eligible snapshot, deterministic contamination policy,
      zero oracle omissions, exact analysis-set parity with annotation agreement.
- [ ] Sena: preserve valid apostrophe-bearing Sena rows; deterministically classify actual debris,
      zero oracle omissions, exact analysis-set parity across the full eligible 7,121-row source.
- [ ] Amharic: deterministic header/character policy, explicit templatic/runtime requirements, zero
      oracle omissions, exact full eligible-corpus parity.
- [ ] Aweti: deterministic invalid/pathological-row policy, bounded resource run, zero oracle
      omissions, exact full eligible-corpus parity.
- [ ] Publish raw/source/eligible digests, exclusions, oracle configuration, all candidates,
      measurements/cache state, certification, Pareto frontier, and unsupported constructs.

Say “certified on eligible corpus digest,” not “full raw corpus,” when deterministic input policy
excludes malformed rows. Synthetic fixtures and pilots remain separate evidence classes.

## Final merge gate

- [ ] Managed focused, package, CLI, and corpus suites pass from the integrated branch.
- [ ] Fresh Sol/xhigh final architecture/correctness review has no unresolved P0/P1 findings.
- [ ] Merge and push main only after primary diff review and requirement-by-requirement evidence
      audit.
- [ ] Rebase owned active worktrees; retain warm worktrees with related gates/follow-up work; retire
      only genuinely completed or week-inactive fully merged worktrees.
