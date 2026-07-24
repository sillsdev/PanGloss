# Tasks — add-capability-characteristics-check

## 1. Profile + envelope + predicate types
- [x] 1.1 Characteristics profile type projected from grammar + stem data — `capability.rs::characterize` (Step 1)
- [x] 1.2 Capability envelope + per-stage/interaction predicate types; bottom-up composition — `capability.rs::compose_envelope` over the reified plan (Step 2). NOTE: interaction predicates for Union/Compose nodes (parallel-independence) are not yet implemented — blocked on `lower-fst-pattern-environments` (Stage 1B).
- [x] 1.3 Capability evidence provenance field (behavioral vs structural) — `EvidenceProvenance`

## 2. Default-deny characterizer
- [x] 2.1 Exhaustive characterizer over frozen `model.rs`, no catch-all (build breaks on new variant)
- [x] 2.2 Mark Compounding / Unordered / MprGroup / all unproven configs fail-closed

## 3. Hard-fail gate
- [x] 3.1 Profile↔envelope match → typed compile-time refusal diagnostic — `CompileDecision::Refuse(diagnostics)` (CHECK-ONLY so far; not yet wired to block a real compile path)
- [x] 3.2 Configuration-predicate granularity; over-refuse-never-under-refuse discipline — `CapabilityPredicate`/`PredicateVerdict`, conservative `SimultaneousSubruleOverlapPredicate`
- [ ] 3.3 Wire the gate into the production compile path (the flip: block/stamp a real compile) — pending (branch work)

## 4. Capability override + trust signal (ADR 0005)
- [ ] 4.1 Explicit override that force-compiles; indelible unproven/recall-unsafe stamp in pack manifest
- [ ] 4.2 Runtime degraded-trust signal (pack-level load + per-analysis flag)
- [ ] 4.3 Override record (who/when/why/which configs); never passes conformance

## 5. Conformance-coverage CI gate
- [ ] 5.1 Cross-check capability registry against `machine/conformance/` coverage; break build on gap

## 6. Design + specs
- [x] 6.1 design.md (envelope composition, interaction predicates, provenance)
- [x] 6.2 specs delta for the capability-boundary contract
