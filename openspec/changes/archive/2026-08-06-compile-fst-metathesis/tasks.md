## 1. Oracle matrix

- [x] 1.1 Enumerate switch, direction, environment, boundary, feature-class, and table variants
      (`pg-foma/tests/phase_c_metathesis.rs`: adjacent-singleton, multi-member, middle-context,
      reversed-tag, RTL, anchor variants)
- [x] 1.2 Add positive/negative HermitCrab-backed witnesses and honest-unsupported cases
      (positive: adjacent/multi-member; honest-unsupported: RTL/anchor; documented-gap:
      middle-context/reversed-tag)

## 2. Relation

- [ ] 2.1 Compile the dedicated metathesis swap relation through shared lowering
      (`pg-foma/src/replace.rs::compile_metathesis_rule` is its own dedicated per-branch cross-product
      swap function; it reuses `pattern_slots` but is not routed through `lower.rs`'s `lower_span`
      seam — same partial-lowering caveat as the quantifier construct)
- [x] 2.2 Preserve switch identity and boundary placement without iterative replacement reuse
      (swap preserves tag-agnostic physical-position semantics; verified against
      `pg_rules::metathesis::synthesize`)
- [x] 2.3 Charge own-net and composition states/arcs against existing budgets
      (tuple-cap charged pre-render; `compose_budget::check_size` charges net states/arcs
      post-compile)

## 3. Verification

- [ ] 3.1 Run all focused commands in `design.md` (not re-verified this pass)
- [x] 3.2 Update only metathesis ledger rows proven by the matrix
      (`pg-foma/src/capability.rs`: `Metathesis` → `Disposition::ConfigPredicate` with a real
      `MetathesisFaithfulSwapPredicate` in `default_registry()`)
