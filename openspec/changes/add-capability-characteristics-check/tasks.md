# Tasks — add-capability-characteristics-check (SCAFFOLD, pending grill/authoring)

## 1. Profile + envelope + predicate types
- [ ] 1.1 Characteristics profile type projected from grammar + stem data
- [ ] 1.2 Capability envelope + per-stage/interaction predicate types; bottom-up composition
- [ ] 1.3 Capability evidence provenance field (behavioral vs structural)

## 2. Default-deny characterizer
- [ ] 2.1 Exhaustive characterizer over frozen `model.rs`, no catch-all (build breaks on new variant)
- [ ] 2.2 Mark Compounding / Unordered / MprGroup / all unproven configs fail-closed

## 3. Hard-fail gate
- [ ] 3.1 Profile↔envelope match → typed compile-time refusal diagnostic
- [ ] 3.2 Configuration-predicate granularity; over-refuse-never-under-refuse discipline

## 4. Capability override + trust signal (ADR 0005)
- [ ] 4.1 Explicit override that force-compiles; indelible unproven/recall-unsafe stamp in manifest
- [ ] 4.2 Runtime degraded-trust signal (pack-level load + per-analysis flag)
- [ ] 4.3 Override record (who/when/why/which configs); never passes conformance

## 5. Conformance-coverage CI gate
- [ ] 5.1 Cross-check capability manifest against `machine/conformance/` coverage; break build on gap

## 6. Design + specs
- [ ] 6.1 design.md (envelope composition, interaction predicates, provenance)
- [ ] 6.2 specs delta for the capability-boundary contract
