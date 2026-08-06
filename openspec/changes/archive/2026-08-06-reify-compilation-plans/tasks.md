# Tasks — reify-compilation-plans

## 1. Plan as first-class data
- [x] 1.1 Compilation plan type (enumerable composition topology) — `plan.rs` (Step 1)
- [x] 1.2 Strategy enumerator emitting legal candidate plans for a grammar — `enumerate.rs::enumerate_default` (Step 2)
- [x] 1.3 Replace hardcoded should_run/probe_would_refuse/partition_entries branching — `emit.rs`'s `emit_with_budget_profiled` now builds the `Plan` once (`crate::emit::plan_topology_decisions`, calling `enumerate_default`) and derives its composite-emission/structural-composite topology decisions from the built `Plan`'s marker-leaf presence instead of independently re-deriving `should_run`/`structural_candidate_rules(...).is_empty()`; anti-drift test pins plan-derived == real-seam across 4 synthetic grammars covering all 4 (composite, structural) combinations. `partition_entries` (the third seam) is honestly NOT wired into `emit.rs`'s mainline: it belongs to `gate.rs`'s own, separate compile entry point (`compile_gated_grammar_with_budget`), which `emit.rs`'s lexc-emission path never calls — forcing it in would mean merging the two compile entry points, out of this task's scope (see design.md D2/task brief point 5).
- [x] 1.4 Make a node's compiled artifact a pure function of its NodeId (per-group Replace subrule mask) — `plan.rs`/`enumerate.rs`/`build.rs` (Step 3b)

## 2. Capability-safe selection
- [x] 2.1 Selection restricted to capability-passing plans (recall-preserving invariant) — `selection.rs::select_plan` filters `enumerate::enumerate_candidates` to candidates whose `capability::compose_envelope` decision is not `Refuse`; a differential-oracle test proves every admissible pair agrees, so selection trades only cost
- [x] 2.2 Deterministic default selection objective (states+arcs / size) — minimum measured `states + arcs` from `build::build_controllable`, tie-broken by root `NodeId` (D1 content address); unmeasurable candidates fall back deterministically to the smallest root

## 3. Differential-correctness oracle
- [x] 3.1 Build ≥2 plans; assert identical confirmed sets; report disagreement as predicate bug — `oracle.rs` (Step 3c; apply-based tier + proven non-vacuous). Cross-topology (equal-after-confirm) tier + confirm-engine integration still open.

## 4. Naming discipline
- [x] 4.1 Compilation modules named by composed parts, never by language

## 5. Design + specs
- [x] 5.1 design.md (enumerator, selection, oracle, blast radius on replace/gate/emit)
- [x] 5.2 specs delta
