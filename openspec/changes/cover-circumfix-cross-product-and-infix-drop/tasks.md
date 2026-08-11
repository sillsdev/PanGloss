# Tasks — cover-circumfix-cross-product-and-infix-drop

Merge units are independently reviewable and ordered. Unit 2's companion test MUST be red before
Unit 3 lands (TDD gate). Exclusive ownership per unit is noted; nothing here touches
`probe_would_refuse` or `fst_health.rs` (owned by `surface-compile-profile-and-templated-routing`,
which merges AFTER this change).

## 1. pg-grammar: circumfix cross-product port  [owner: pg-grammar/src/compile/{affixes.rs,tests.rs}]
- [ ] 1.1 `build_circumfix_allomorphs` implementing the HCLoader 4-level cross-product
      (prefix × suffix × prefix-env × suffix-env; exact `MorphType::Prefix`/`Suffix` filter;
      loop-nesting order preserved for disjunctive allomorph indexing), LHS per design D4,
      RHS `[Insert(pfx+), Copy(0), Insert(+sfx)]`, one `EnvironmentDef` for external contexts.
- [ ] 1.2 Replace the bail-out at `affixes.rs:60-67`: partition into prefix/suffix groups; empty
      group → warn + `None`; wire the built allomorphs into the existing
      `AffixProcessRuleDef` push AND the same slot-registration path ordinary inflectional
      rules use (`acc.slot_rules`), so owning templates gain the slot.
- [ ] 1.3 MPR asymmetry: per-allomorph inflection-class MPRs sourced from the PREFIX half only;
      no `required_syn_fs` on circumfix allomorphs (both are faithful C# quirks — comment links
      to design D4, no restating).
- [ ] 1.4 Tests: flip `circumfix_entry_is_unsupported_and_warns_rather_than_erroring` into a
      positive lowering assertion; add (a) both-envs-empty → AnyPlus, (b) prefix-only env,
      (c) suffix-only env, (d) both envs with external contexts merging into one
      `EnvironmentDef`, (e) 2 prefix × 2 suffix → 4 allomorphs in HCLoader nesting order,
      (f) empty-suffix-group → warn + None, (g) dotted-circle already stripped upstream.
- [ ] 1.5 XML-loader parity test: same semantic circumfix authored as a snapshot fixture through
      `compile_project` and as HC-XML through `load`; resulting allomorph defs structurally
      equal modulo naming.
      Verify: `rust/tools/pg.ps1 -Mode test -Package pg-grammar`.

## 2. Conformance fixture + red companion test  [owner: conformance-staging/edge-cases/circumfix-cross-product-and-infix-drop/, pg-foma/tests/ (new sibling file only)]
- [ ] 2.1 Author `grammar.xml` + `words.yaml` + `STAGING.md` per the conformance-grammars skill:
      4-subrule 2×2 cross-product rule (`mrCross`) + Infix-with-drop rule (`mrInfixDrop`);
      words pin all four cross-product cells (incl. the discriminating both-conditions cell),
      the infix-drop word, bare-root control, and an `expect_fail` negative. Synthetic-only
      naming; oracle signatures transcribed from `pg_parse::Morpher` directly.
- [ ] 2.2 Companion FST-reachability test (new file, `circumfix_candidate_selection.rs` pattern):
      all 4 `mrCross` subrules present in the structural candidate set + every oracle analysis
      of the cross-product words reachable in the compiled net; `mrInfixDrop`'s word asserted
      reachable — EXPECTED RED here (undergeneration witness). Mark the red assertion
      `#[ignore]` with reason "red until unit 3" only if CI must stay green; otherwise land
      units 2+3 in one PR with the red→green commit sequence visible.
- [ ] 2.3 `pg.ps1 -Mode test -Package pg-parse -TestTarget conformance_fixtures_gate` — fixture
      replays; checked-case count increases.

## 3. pg-foma: Infix-with-drop capability  [owner: emit.rs candidate-selection region, capability.rs predicate + tests, coverage ledger]
- [x] 3.1 Verification probe — ANSWERED BY UNIT 2 (2026-08-10), with a split verdict that
      CONFIRMS the D1 choice: on the synthetic fixture, preexpand DOES cover `mrInfixDrop`
      (word reachable, uncovered list empty — the companion test pins this green, so no
      ignored-red witness exists); but on the motivating real grammar the same construct's
      allomorphs sat in the 73-entry UNCOVERED list, i.e. preexpand coverage is INCIDENTAL
      (reached-by-enumeration luck), not guaranteed. Therefore Option 0 (blanket-credit
      preexpand) is unsound as a predicate ground truth, and 3.2/3.3 proceed as designed.
      Unit 3's recall witness is the containment fixture (3.4) + candidate-set membership,
      NOT a red-to-green flip of the unit-2 companion test; the real-grammar witness is 4.1.
- [ ] 3.2 Widen `is_structural_rule`: `Role::Infix => allomorphs_of(g, mid).iter().any(rhs_drops_lhs_material)`;
      update the function doc and the `emit.rs:2274-2283` doc that currently states Infix is
      excluded.
- [ ] 3.3 Ownership handoff: Infix-with-drop rules leave `preexpand_candidates`; handoff test
      mirroring `circumfix_infix_ownership_handoff_is_clean`; `covered_infix_rules`/uncovered-
      clearing behavior preserved (design D2).
- [ ] 3.4 Oracle containment fixture in `tests/phase_c_circumfix.rs` style: synthetic
      Infix-with-drop grammar, generator + `Morpher` sweep, 100% recall required.
- [ ] 3.5 Predicate tests: `circumfix_output_action_predicate_refuses_infix_role_drop` flips to
      a positive (ConfirmOnly) pin; add the remaining negative boundary
      (`Role::Reduplication` + drop still refuses). Coverage ledger citation string +
      `coverage_ledger_golden.json` regenerated. Companion test from 2.2 goes green;
      un-`#[ignore]` if it was ignored.
- [ ] 3.6 Docs: census C4 section in `docs/research/circumfix-composite-precedence-census.md`;
      predicate doc block gains the Infix-with-drop disposition.
      Verify: `pg.ps1 -Mode test -Package pg-foma` (includes conformance_coverage_gate,
      structural_witness_gate, plan_interaction_coverage_gate unchanged-count assertions).

## 4. Local verification against the motivating project (NOT a conformance artifact)
- [ ] 4.1 Re-run the capability gate on the local `.fwdata` (gitignored): expect the mrule-166
      Refuse gone (ConfirmOnly path), the two import warnings gone, and the two paradigm-cell
      words analyzable on the default engine. Record numbers in the PR, no data committed.
- [ ] 4.2 Confirm the new cross-product rules do NOT newly trip any capability predicate on
      `--engine=foma` (they classify `CircumfixPrefix`; expected non-event, assert it).

## 5. Bookkeeping
- [ ] 5.1 `docs/hermitcrab-rust-port-audit.md`: record fwdata circumfix cross-product closed,
      explicitly distinguished from the C1/C2/C3 emit.rs items.
- [ ] 5.2 STAGING.md entry: this change merges before `surface-compile-profile-and-templated-routing`;
      emit.rs region serialization noted.
