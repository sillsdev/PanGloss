# Tasks — reify-compilation-plans

## 1. Plan as first-class data
- [x] 1.1 Compilation plan type (enumerable composition topology) — `plan.rs` (Step 1)
- [x] 1.2 Strategy enumerator emitting legal candidate plans for a grammar — `enumerate.rs::enumerate_default` (Step 2)
- [ ] 1.3 Replace hardcoded should_run/probe_would_refuse/partition_entries branching (production flip — pending; enumerate_default currently mirrors them, build_controllable interprets, but emit is not yet routed through build())
- [x] 1.4 Make a node's compiled artifact a pure function of its NodeId (per-group Replace subrule mask) — `plan.rs`/`enumerate.rs`/`build.rs` (Step 3b)

## 2. Capability-safe selection
- [ ] 2.1 Selection restricted to capability-passing plans (recall-preserving invariant) — pending (needs the characteristics gate wired into enumeration)
- [ ] 2.2 Deterministic default selection objective (states+arcs / size) — pending

## 3. Differential-correctness oracle
- [x] 3.1 Build ≥2 plans; assert identical confirmed sets; report disagreement as predicate bug — `oracle.rs` (Step 3c; apply-based tier + proven non-vacuous). Cross-topology (equal-after-confirm) tier + confirm-engine integration still open.

## 4. Naming discipline
- [x] 4.1 Compilation modules named by composed parts, never by language

## 5. Design + specs
- [x] 5.1 design.md (enumerator, selection, oracle, blast radius on replace/gate/emit)
- [x] 5.2 specs delta
