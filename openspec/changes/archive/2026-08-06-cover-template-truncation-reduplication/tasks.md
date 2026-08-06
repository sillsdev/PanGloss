## 1. Template and truncation matrix

- [ ] 1.1 Witness template depth/order/alternatives, final/partial flags, and template-less paths
      (only pre-existing, untouched-by-this-change tests exercise this — `pg-rules/tests/
      template_partial_gate.rs`, `pg-foma/tests/p6_gate_parity.rs`; template/truncation was found
      "already-faithful (unchanged)," no new witness added by this change)
- [ ] 1.2 Witness leading/trailing/multi-step truncation alone and with templates
      (same as 1.1 — pre-existing `f1_sena_gate.rs`/`f2_indonesian_gate.rs` (mostly `#[ignore]`d,
      needing local corpus data) and `w91_affix_shapes_covered_by_upstream_fixtures`; no new dedicated
      truncation test added)
- [x] 1.3 Add preflight and cumulative budgets for emitted alternatives and preexpanded chains
      (pre-existing `EnumerationBudget` in `preexpand.rs`, unchanged by this change but genuinely in
      force)

## 2. Reduplication boundary

- [x] 2.1 Witness complete/partial and prefix/suffix reduplication peeler variants
      (`pg-foma/tests/f6_reduplication_peel_chain_depth.rs`:
      `kimbiakimbia_reduplication_is_recovered_with_oracle_containment`; `redup_and_free_fluctuation_
      gate.rs` covers prefix/suffix hint variants)
- [ ] 2.2 Prove peeler candidates retain complete proposer-to-confirm recall and multiplicity
      (depth-1 proven exactly by the test above; nested depth≥2 containment/multiplicity is left open
      — no in-repo fixture, per this change's own commit message)
- [x] 2.3 Keep reduplication ledger rows `peeled` unless a separate exact compiler is proven
      (`pg-foma/src/capability.rs`: `Reduplication` stays `Disposition::ConfigPredicate`/`ConfirmOnly`
      (peeled), never promoted to compiled; `RealizationalRule`-owned reduplication is `Refuse`d, not
      silently zero-recall)

## 3. Verification

- [x] 3.1 Run all focused cargo commands in `design.md`
      (`cargo test -p pg-foma --test f6_reduplication_peel_chain_depth`, `-p pg-rules --test
      template_partial_gate --test redup_and_free_fluctuation_gate --test max_apps_gate` all pass;
      `f1`/`f2`/`p6` are mostly `#[ignore]`d for missing corpus data)
- [x] 3.2 Run or record `not_run` for the named truncate conformance fixture
      (`machine/conformance/edge-cases/truncate-morphotactic` exists and is actually discovered/run
      by `all_discovered_fixtures_match_oracle` plus
      `w91_affix_shapes_covered_by_upstream_fixtures` — passing, not `not_run`)
- [ ] 3.3 Update ledger rows individually for template, truncation, and reduplication variants
      (not_run — no coverage-ledger artifact exists in the repo to update)
