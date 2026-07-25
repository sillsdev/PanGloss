## 1. Constraint inventory

- [x] 1.1 Enumerate realizational feature, stem-name/family, blocking, max-application, and co-occurrence variants
      (`pg-foma/tests/cover_realizational_morphology_constraints.rs`: `kib`/`zod`+`vem`/`tay`+`toy`/
      `fom` fixture isolates all four families)
- [x] 1.2 Assign compiled, overapproximated, confirm-only, or honest-unsupported disposition per row
      (`pg-foma/src/capability.rs`: `RealizationalMorphology`/`CoOccurrenceConstraint` both default to
      `Disposition::ConfirmOnly` unconditionally — already faithful, no compiled/overapproximated
      shape claimed; this construct predates this change's own conformance kit, per STAGING.md)
- [x] 1.3 Add positive/negative oracle witnesses that make each constraint non-vacuous
      (all 4 tests pass with explicit positive+negative rows: `kibid`/`kibesid`, `zod`/`zodut`,
      `tay`/`toy`/`tayut`, `fomut`/`fomon`/`fomuton`)

## 2. Safe proposer handling

- [x] 2.1 Add only admission filters proven to have no oracle false negatives
      (no new admission filter added; tests confirm `candidates_generated > 0` for every negative
      case, i.e. the proposer never filters)
- [x] 2.2 Preserve full-history/feature constraints for HermitCrab confirmation
      (`assert_confirm_matches_oracle` asserts exact structured-set equality vs `pg_parse::Morpher`
      for every case)
- [x] 2.3 Bound max-application enumeration and all overapproximated candidate growth
      (no new overapproximation growth introduced; pre-existing `pg-rules/tests/max_apps_gate.rs`
      still passes unchanged)

## 3. Verification

- [x] 3.1 Run all focused commands in `design.md`
      (`morph_gate`, `max_apps_gate`, `validity_gate`, `memo_gate`, `f4_composite_gate`,
      `p6_gate_parity` all run, non-ignored subset passes)
- [x] 3.2 Prove proposer-to-confirm recall and negative rejection per constraint row
      (each test proves both `candidates_generated>0` (recall) and `confirmed==0`/exact-match
      (rejection) per construct row)
- [ ] 3.3 Publish final ledger updates without family-level inference
      (not_run — no coverage-ledger artifact exists in the repo to verify against)
