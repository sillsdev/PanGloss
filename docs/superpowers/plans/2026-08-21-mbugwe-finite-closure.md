# Mbugwe Finite-Closure Implementation Plan

> Execute this plan with `subagent-driven-development`; all Rust commands go through `rust/tools/pg.ps1`.

> **Deferred future work.** This plan is not part of the current Indonesian/Amharic/Aweti release
> slice and its Mbugwe scale-acceptance task is not a current shipping gate.

**Goal:** Replace the unsound three-extra-rule structural enumeration limit with deterministic, grammar-bounded closure, refuse incomplete construction, and certify every trusted Foma artifact.

**Boundary:** The committed `late-structural-anchor-five-rule-chain` and `complex-inserted-redup-later-allomorph` grammars are small PanGloss-only semantic fixtures. They prove compiler behavior, not Mbugwe corpus-scale coverage. Scale acceptance is a separate final task.

## Task 1: Pin the current omission and fail-closed behavior

**Files:**

- Modify: `rust/crates/pg-foma/tests/late_structural_anchor_recall.rs`
- Modify: `rust/crates/pg-foma/src/preexpand.rs`
- Modify: `rust/crates/pg-foma/src/emit.rs`
- Test: `rust/crates/pg-foma/tests/late_structural_anchor_recall.rs`
- Test: `rust/crates/pg-foma/tests/cover_unordered_morph_rules.rs`

1. Keep the existing `fedcbag` oracle/FST recall assertion and add a regression which constructs a live successor at the old depth boundary.
2. Add a typed terminal result shared by both closure paths:

```rust
pub enum ClosureTermination {
    Complete,
    ResourceEnvelope { pending_states: usize, reason: ResourceLimit },
    Unsupported { reason: UnsupportedClosure },
}
```

3. Before replacing recursion, make `preexpand::extend` and `emit::struct_extend` detect a legal successor when the old limit is reached. Return `ResourceEnvelope` with a nonzero pending count; do not return records as a successful artifact.
4. Run the focused RED test:

```powershell
.\rust\tools\pg.ps1 -Mode conformance-test -Scope local -Package pg-foma -TestTarget late_structural_anchor_recall -Filter five_rule_chain_with_late_structural_anchor
```

Expected: the existing success assertion fails because the construction is now explicitly incomplete, rather than silently emitting a partial FST.
5. Add an assertion that incomplete closure cannot produce `FomaTier::Full` or `FomaTier::Partial` and run it GREEN.
6. Commit: `fix(foma): refuse incomplete structural closure`

## Task 2: Add grammar-bounded application state

**Files:**

- Modify: `rust/crates/pg-foma/src/morphotactics.rs`
- Modify: `rust/crates/pg-foma/src/capability.rs`
- Test: `rust/crates/pg-foma/src/morphotactics.rs`
- Test: `rust/crates/pg-foma/tests/phase_c_strata_depth.rs`

1. Add a stable rule ordinal and per-rule application counts to `ChainState`; include both in equality/hash/order.
2. Derive the finite bound from authored `multipleApplication`, preserving the meaningful default of one. Do not infer a bound from observed depth.
3. Change `MorphotacticIndex::next_state` to increment/check the relevant counter and expose `successors(&ChainState)` in deterministic rule order.
4. Add tests for one-use, bounded repeated-use, ordered, unordered, and exhausted rules. Include a test proving that the same rule may be applied twice when its authored bound is two.
5. Run:

```powershell
.\rust\tools\pg.ps1 -Mode test -Package pg-foma -Filter morphotactic
```

6. Commit: `feat(foma): track grammar-bounded rule applications`

## Task 3: Replace fixed-depth recursion with deterministic closure

**Files:**

- Modify: `rust/crates/pg-foma/src/preexpand.rs`
- Modify: `rust/crates/pg-foma/src/emit.rs`
- Modify: `rust/crates/pg-foma/src/morphotactics.rs`
- Test: `rust/crates/pg-foma/tests/late_structural_anchor_recall.rs`
- Test: `rust/crates/pg-foma/tests/emit_underlying_templated_recursive_compound_chain.rs`

1. Remove `MAX_EXTRA_RULES` and `STRUCT_MAX_EXTRA_RULES` as success boundaries.
2. Use a `BTreeSet`/ordered queue worklist keyed by the complete `ChainState` plus the generated structural state. Maintain separate `queued` and `completed` sets.
3. Continue until the worklist is empty or a named resource-envelope limit is reached. Maximum observed depth is a metric only.
4. Replace rule-identity deduplication with the per-rule count checks from Task 2.
5. Aggregate deterministic evidence: explored/completed states, pending states, applications per rule, maximum observed depth, entries, probes, and termination reason.
6. Run the late-anchor test; expected GREEN with `fedcbag` present and zero pending states.
7. Run:

```powershell
.\rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget emit_underlying_templated_recursive_compound_chain
.\rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget phase_c_strata_depth
```

8. Commit: `feat(foma): exhaust finite structural closure`

## Task 4: Require a completeness certificate for trust

**Files:**

- Create: `rust/crates/pg-foma/src/completeness.rs`
- Modify: `rust/crates/pg-foma/src/lib.rs`
- Modify: `rust/crates/pg-foma/src/emit.rs`
- Modify: `rust/crates/pg-foma/src/analyzer.rs`
- Modify: `rust/crates/pg-foma/src/readiness_verdict.rs`
- Modify: `rust/crates/pg-foma/src/worker.rs`
- Modify: `rust/crates/pg-cli/src/pack.rs`
- Test: `rust/crates/pg-foma/tests/readiness_certification_gate.rs`
- Test: `rust/crates/pg-foma/tests/backend_runtime_cache_gate.rs`

1. Define `FstCompletenessCertificate` with schema version, grammar/backend/route identity, component classifications, authored bounds, cycle classification, zero-surface-cycle evidence, closure counters, resource measurements, over-approximations, and pending worklist count.
2. Make `is_valid()` require a matching identity, only certified component classifications, and `pending_states == 0`.
3. Attach the certificate to successful `EmitResult`; incomplete/unsupported emission returns a typed error and no trusted artifact.
4. Require the certificate in `FomaProposer::new`, readiness certification, workers, and pack trust. Development override may retain an unproven artifact, but never a valid certificate.
5. Add RED/GREEN tests proving that empty `uncovered` without a certificate is insufficient and that a nonempty worklist cannot be overridden into a proven pack.
6. Run:

```powershell
.\rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget readiness_certification_gate
.\rust\tools\pg.ps1 -Mode test -Package pg-foma -TestTarget backend_runtime_cache_gate
.\rust\tools\pg.ps1 -Mode test -Package pg-cli -Filter pack
```

7. Commit: `feat(foma): certify complete FST construction`

## Task 5: Remove known-underproposal from normal selection

**Files:**

- Modify: `rust/crates/pg-foma/src/backend_mechanism.rs`
- Modify: `rust/crates/pg-foma/src/capability.rs`
- Modify: `rust/crates/pg-foma/src/strategy_coverage.rs`
- Modify: `rust/crates/pg-foma/src/health_evaluator.rs`
- Test: `rust/crates/pg-foma/tests/backend_mechanism_graph.rs`
- Test: `rust/crates/pg-foma/tests/strategy_aware_capability_gate.rs`

1. Add tests showing `RepresentsWithKnownGap` cannot become `ConfirmOnly` when the gap can omit analyses.
2. Map those rows to typed refusal unless the route supplies an explicit proposer-superset proof.
3. Report incomplete construction as Error and unsupported/unknown completeness as Critical. Neither may be normally selected.
4. Run both test targets through `pg.ps1` and commit: `fix(foma): reject known underproposal routes`.

## Task 6: Mbugwe semantic and scale acceptance

**Files:**

- Modify only if evidence changes: `rust/tools/corpus-manifest.json`
- Create: `docs/fst-plan/2026-08-21-mbugwe-finite-closure-results.md`

1. Run both PanGloss-only conformance grammars and record exact results.
2. Run the real Mbugwe smoke gate:

```powershell
.\rust\tools\pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget mbugwe_corpus_smoke_gate -TestThreads 1
```

3. Run the named real-corpus batch command derived from the manifest with one thread and a 5,000 ms word timeout. Record build outcome, certificate, states/arcs, elapsed time, full recall/timeout/skipped counts, and every backend report.
4. Acceptance is either: a certified complete FST with measured corpus results; or a typed Error/Critical refusal with no trusted artifact and actionable backend-specific evidence. A miniature fixture pass is never scale acceptance.
5. Commit: `test(foma): record Mbugwe closure evidence`
