## 1. Semantic fixtures

- [x] 1.1 Generate oracle fixtures for disjoint, overlapping, deleting, inserting, and feature-changing sites
      (`pg-foma/tests/phase_c_simultaneous.rs`: sim-trivial, sim-nonoverlap-env, sim-overlap-env)
- [x] 1.2 Pin subrule priority and boundary behavior with negative witnesses
      (`sim-overlap-env` pins the D3 Refuse boundary behavior)

## 2. Compiler

- [x] 2.1 Implement simultaneous match/replacement compilation without iterative reuse
      (reuses plain/iterative sequential compose,
      `pg-foma/src/replace.rs::compile_and_compose_rules_with_budget`, per module doc)
- [x] 2.2 Add compile-size and apply-time accounting under existing resource APIs
      (`pg-foma/src/compose_budget.rs::check_size` uniformly wraps the compose calls)

## 3. Evidence

- [x] 3.1 Require exact analysis containment/multiplicity on fixtures
      (`sim_nonoverlap_env_...` asserts exact `fst==oracle` equality)
- [ ] 3.2 Re-run and diff the pinned Aweti manifest
      (not_run — `p6_aweti_gate.rs` still lists Aweti's real Simultaneous rule as skipped/unchanged;
      no re-run evidence found)
- [x] 3.3 Update ledger disposition only for combinations proven by witnesses
      (`pg-foma/src/capability.rs`: `SimultaneousRewrite` → `Disposition::ConfigPredicate` with a real
      `SimultaneousSubruleOverlapPredicate` in `default_registry()`)
