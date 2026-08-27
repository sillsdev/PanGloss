# Tasks — add-capability-characteristics-check

## 1. Profile + envelope + predicate types
- [x] 1.1 Characteristics profile type projected from grammar + stem data — `capability.rs::characterize` (Step 1)
- [x] 1.2 Capability envelope + per-stage/interaction predicate types; bottom-up composition — `capability.rs::compose_envelope` over the reified plan (Step 2). NOTE: interaction predicates for Union/Compose nodes (parallel-independence) are not yet implemented — blocked on `lower-fst-pattern-environments` (Stage 1B).
- [x] 1.3 Capability evidence provenance field (behavioral vs structural) — `EvidenceProvenance`

## 2. Default-deny characterizer
- [x] 2.1 Exhaustive characterizer over frozen `model.rs`, no catch-all (build breaks on new variant)
- [x] 2.2 Mark Compounding / Unordered / MprGroup / all unproven configs fail-closed

## 3. Hard-fail gate
- [x] 3.1 Profile↔envelope match → typed compile-time refusal diagnostic — `CompileDecision::Refuse(diagnostics)`, now surfaced by `pg_foma::capability_entry::evaluate_capability` (Step 2 wraps `characterize`+`enumerate_default`+`compose_envelope` for callers)
- [x] 3.2 Configuration-predicate granularity; over-refuse-never-under-refuse discipline — `CapabilityPredicate`/`PredicateVerdict`; `default_registry()` now ships **11 real predicates** (multi-table, RTL, simultaneous, quantifier, metathesis, circumfix, reduplication, compounding, unordered, MPR-append, MPR-overwrite), not just the one `SimultaneousSubruleOverlapPredicate` noted here previously — only `epenthesis.placeholder` remains a `FailClosedPlaceholder`
- [x] 3.3 Wire the gate into the production compile path (the flip: block a real compile) — DONE for the CLI path: `pg-cli/src/main.rs`'s `capability_gate`/`run_capability_gate` call `evaluate_capability` and default-enforce on `--engine=foma`. **Policy follow-up remains:** hide and reject `--allow-unproven` and legacy `--no-enforce-capability` in production builds; both are currently reachable there. NOTE: this is the CLI entry point, not `emit.rs`/`gate.rs`/`replace.rs` themselves refusing internally — the compile functions still run unconditionally once the CLI-level gate lets a grammar through.

## 5. Conformance-coverage CI gate
- [ ] 5.1 Cross-check capability registry against `machine/conformance/` coverage; break build on gap
      (the cross-check itself is real and non-blocking: `pg-foma/src/conformance_coverage.rs`
      (`construct_ids_for` mapping + a pure gap function) plus
      `pg-foma/tests/conformance_coverage_gate.rs` (replays every discovered fixture against
      `pg_parse::Morpher` to build the "passing" set) — matching `STAGING.md`'s own "advisory;
      build-breaking flip deferred." Left unchecked because the task's own text is "break build on
      gap," which this explicitly does not do yet)

## 6. Design + specs
- [x] 6.1 design.md (envelope composition, interaction predicates, provenance)
- [x] 6.2 specs delta for the capability-boundary contract
