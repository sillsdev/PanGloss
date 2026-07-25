## 1. Character-table threading

- [x] 1.1 Add failing positive/negative witnesses for two distinct tables
      (`pg-foma/tests/two_table_symbol_divergence.rs`, `tests/phase_c_multi_table.rs`)
- [x] 1.2 Thread table identity through pattern rendering and alpha-tuple resolution
      (`pg-foma/src/replace.rs` `owning_table`/`owning_table_for_metathesis`/
      `owning_table_for_prule_position`; `resolve_alpha_tuples` takes an explicit `&CharDefTable`)
- [x] 1.3 Remove implicit table-zero assumptions on the composition path
      (no `table_of`-style hardcode remains; `owning_table` resolves per-rule everywhere)

## 2. Interaction gate

- [ ] 2.1 Add a deterministic multi-table × alpha-variable × multi-stratum recipe
      (`phase_c_multi_table.rs` covers multi-table × multi-stratum; the alpha-variable leg is not
      exercised)
- [ ] 2.2 Require complete proposer-to-confirm analysis containment
      (`two_table_symbol_divergence.rs` proves exact `fst_candidate_set == oracle_candidate_set`
      containment, but `phase_c_multi_table.rs`'s recipe only checks one-directional
      `gate_template::recall_reachable`, not full containment)
- [x] 2.3 Change the ledger disposition from honest unsupported/wrong-detected to compiled only after all gates pass
      (`pg-foma/src/capability.rs`: `CharacteristicKind::MultiTable` → `Disposition::ConfigPredicate`
      with a real `MultiTableFaithfulThreadingPredicate` in `default_registry()`)
