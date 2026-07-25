## 1. Semantics

- [x] 1.1 Add C#/full-HC oracle witnesses for RTL application order, environments, anchors, deletion, and epenthesis
      (`pg-foma/tests/phase_c_right_to_left.rs`: rtl-plain, feature-environment, deletion,
      epenthesis fixtures)
- [x] 1.2 Implement reversal-based compilation and boundary cleanup
      (`pg-foma/src/replace.rs::compile_rtl_branch_net` — reversal + safety-net union via
      `fsm_reverse`)
- [x] 1.3 Preserve typed unsupported results for unimplemented RTL combinations
      (non-`Iterative`/non-`RightToLeft` shapes still return `Ok(None)`/skipped)

## 2. Evidence

- [x] 2.1 Require complete analysis containment on synthetic witnesses
      (exact containment asserted on rtl-plain/deletion/feature-env; one witness documents a genuine
      oracle gap and asserts a safe superset instead of exact equality — recorded honestly, not
      silently passed)
- [ ] 2.2 Re-run the exact Aweti manifest and attribute newly recalled analyses to RTL rules
      (not_run — `pg-foma/tests/p6_aweti_gate.rs` explicitly documents this as NOT RUN; the
      `samples/data/aweti.json` fixture this needs is absent from this checkout)
- [x] 2.3 Update the coverage ledger disposition and resource counts
      (`pg-foma/src/capability.rs`: `RightToLeftRewrite` → `Disposition::ConfigPredicate` with a real
      `RightToLeftRewriteFaithfulReversalPredicate` in `default_registry()`)
