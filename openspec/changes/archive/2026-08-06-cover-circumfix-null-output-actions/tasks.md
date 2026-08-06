## 1. Variant matrix

- [x] 1.1 Inventory circumfix roles, null/copied LHS parts, truncation, and ordered output actions
      (`pg-foma/src/capability.rs` `CircumfixOutputActionDetail`/`circumfix_output_action_details()`;
      `is_structural_rule`/`classify_affix` in `emit.rs`)
- [x] 1.2 Add positive/negative oracle witnesses and assign a disposition per variant
      (`pg-foma/tests/phase_c_circumfix.rs`:
      `ordered_multi_insert_no_first_insert_shortcut_recall_parity`,
      `null_role_structural_drop_recall_parity` (positive),
      `process_role_drop_stays_honestly_unsupported` (negative); disposition assigned per shape by
      `CircumfixStructuralCompositePredicate`)

## 2. Proposer boundary

- [x] 2.1 Preserve discontinuous single-morpheme identity for supported circumfix outputs
      (`null_role_structural_drop_recall_parity` proves single discontinuous-morpheme identity
      preserved)
- [x] 2.2 Handle null-role and ordered Copy/InsertSegments combinations without first-insert shortcuts
      (real bug fix: `emit.rs` renamed `first_insert_text`→`insert_action_texts`, now concatenates
      every `InsertSegments` in order — this is the "fixed a real multi-InsertSegments recall bug"
      STAGING.md refers to; proven by `ordered_multi_insert_no_first_insert_shortcut_recall_parity`)
- [x] 2.3 Route Modify/InsertContext and other non-emittable variants honestly
      (`process_role_drop_stays_honestly_unsupported` proves `Role::Process`/`ModifyFromInput` is
      reported in `EmitReport::uncovered`, never silently compiled)
- [x] 2.4 Charge preexpansion paths, emitted lines, candidates, and outputs to existing budgets
      (pre-existing `EnumerationBudget` from `preexpand.rs`; circumfix's structural path routes
      through the same charged `build_structural_composites` code)

## 3. Verification

- [x] 3.1 Run all focused commands in `design.md`
      (`cargo test -p pg-foma --test phase_c_circumfix`; `pg-rules --test morph_gate`/`stratum_gate`
      pass)
- [ ] 3.2 Update only individually witnessed ledger rows
      BLOCKED ON Q2, not on a missing artifact. The earlier note claimed no coverage ledger exists;
      it does — `pg-foma/src/coverage_ledger.rs` plus a 22-row golden — and `circumfix_output_action`
      has a row reading config_predicate / covered. The real blocker is that ALL 22 rows read
      `covered`, the blanket claim Q2 in docs/open-questions.md calls vacuous.
