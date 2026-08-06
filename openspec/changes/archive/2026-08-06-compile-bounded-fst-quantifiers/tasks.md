## 1. Variant contract

- [ ] 1.1 Enumerate optional, bounded, unbounded, nested, grouped, and environment quantifier rows
      (`pg-foma/tests/phase_c_quantifier.rs` covers optional/bounded/unbounded/environment rows; no
      explicit nested/grouped-quantifier row found)
- [x] 1.2 Add positive/negative oracle witnesses for each supported row and unsupported detection tests
      (`quantifier_bounded_environment_compiles_and_matches_oracle` +
      `quantifier_unbounded_environment_stays_honestly_unsupported`)

## 2. Compilation

- [ ] 2.1 Compile optional and bounded regular variants through the shared lowering IR
      (`pg-foma/src/replace.rs::Slot::Repeat` compiles via foma's native `^{min,max}` and shares
      `pattern_slots` with `lower.rs`, but `replace.rs`'s own rewrite compilation is not migrated onto
      the `lower.rs::lower_span` seam yet — "shared lowering IR" is only partially true)
- [x] 2.2 Preflight alternative/repetition growth and charge logical build budgets
      (cross-product/tuple-cap preflight before regex render, mirroring `resolve_alpha_tuples`)
- [x] 2.3 Reject unbounded/non-regular or over-budget variants without finite-cutoff substitution
      (unbounded `max="-1"` stays `None`/skipped, verified by test)

## 3. Verification

- [ ] 3.1 Run all focused commands in `design.md` (not re-verified this pass)
- [x] 3.2 Update only individually proven quantifier ledger rows
      (`pg-foma/src/capability.rs`: `QuantifierPattern` → `Disposition::ConfigPredicate` with a real
      `QuantifierBoundedExpansionPredicate` in `default_registry()`)
