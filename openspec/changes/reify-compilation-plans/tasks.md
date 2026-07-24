# Tasks — reify-compilation-plans

## 1. Plan as first-class data
- [ ] 1.1 Compilation plan type (enumerable composition topology)
- [ ] 1.2 Strategy enumerator emitting legal candidate plans for a grammar
- [ ] 1.3 Replace hardcoded should_run/probe_would_refuse/partition_entries branching

## 2. Capability-safe selection
- [ ] 2.1 Selection restricted to capability-passing plans (recall-preserving invariant)
- [ ] 2.2 Deterministic default selection objective (states+arcs / size)

## 3. Differential-correctness oracle
- [ ] 3.1 Build ≥2 plans; assert identical confirmed sets; report disagreement as predicate bug

## 4. Naming discipline
- [ ] 4.1 Compilation modules named by composed parts, never by language

## 5. Design + specs
- [x] 5.1 design.md (enumerator, selection, oracle, blast radius on replace/gate/emit)
- [x] 5.2 specs delta
