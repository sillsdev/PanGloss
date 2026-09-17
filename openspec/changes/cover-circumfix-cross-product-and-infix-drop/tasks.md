# Tasks — cover-circumfix-cross-product-and-infix-drop

Merge units are independently reviewable and ordered. Unit 2's companion test pins the structural
candidate and ownership handoff before Unit 3 lands. Exclusive ownership per unit is noted; nothing here touches
`probe_would_refuse` or `fst_health.rs` (owned by `surface-compile-profile-and-templated-routing`,
which merges AFTER this change).

## 1. pg-grammar: circumfix cross-product port  [owner: pg-grammar/src/compile/{affixes.rs,tests.rs}]
- [x] 1.1 `build_circumfix_allomorphs` implementing the HCLoader 4-level cross-product
      (prefix × suffix × prefix-env × suffix-env; exact `MorphType::Prefix`/`Suffix` filter;
      loop-nesting order preserved for disjunctive allomorph indexing), LHS per design D4,
      RHS `[Insert(pfx+), Copy(0), Insert(+sfx)]`, one `EnvironmentDef` for external contexts.
- [x] 1.2 Replace the bail-out at `affixes.rs:60-67`: partition into prefix/suffix groups; empty
      group → warn + `None`; wire the built allomorphs into the existing
      `AffixProcessRuleDef` push AND the same slot-registration path ordinary inflectional
      rules use (`acc.slot_rules`), so owning templates gain the slot.
- [x] 1.3 MPR asymmetry: per-allomorph inflection-class MPRs sourced from the PREFIX half only;
      no `required_syn_fs` on circumfix allomorphs (both are faithful C# quirks — comment links
      to design D4, no restating).
- [x] 1.4 Tests: flip `circumfix_entry_is_unsupported_and_warns_rather_than_erroring` into a
      positive lowering assertion; add (a) both-envs-empty → AnyPlus, (b) prefix-only env,
      (c) suffix-only env, (d) both envs with external contexts merging into one
      `EnvironmentDef`, (e) 2 prefix × 2 suffix → 4 allomorphs in HCLoader nesting order,
      (f) empty-suffix-group → warn + None, (g) dotted-circle already stripped upstream.
- [x] 1.5 XML-loader parity test: `compile_project` and legacy HC-XML `load` agree on the emitted RHS action sequence (`Insert`, `Copy`, `Insert`) and environment count for the same semantic circumfix. LHS node shapes are intentionally not compared: the snapshot no-environment path synthesizes `PrefixNull()`/`AnyStar()`/`SuffixNull()` wildcards with no DTD equivalent. Source pin: `compile/tests.rs::circumfix_cross_product_matches_the_xml_loaders_generic_affix_process_rhs`. Verify: `rust/tools/pg.ps1 -Mode test -Package pg-grammar`.

## 2. Conformance fixture + structural-ownership companion test  [owner: conformance-staging/edge-cases/circumfix-cross-product-and-infix-drop/, pg-foma/tests/ (new sibling file only)]
- [x] 2.1 Authored `grammar.xml` + `words.yaml` + `STAGING.md` for the 2×2 `mrCross` cross-product and `mrInfixDrop`; words pin all four cells, the infix-drop word, bare-root control, and negative. Signatures are transcribed from `pg_parse::Morpher`.
- [x] 2.2 Companion FST-reachability test `circumfix_cross_product_and_infix_drop_candidate_selection.rs` pins all four `mrCross` cells and `bumat` recall. Current ownership truth: `mrCross` and `mrInfixDrop` are the structural candidates; `mrInfixDrop` is absent from `preexpand_candidates`, and `bumat` is reached through `build_structural_composites`.
- [x] 2.3 `pg.ps1 -Mode test -Package pg-parse -TestTarget conformance_fixtures_gate` — verified 2026-08-11: 3/3 gate tests passed, including `all_discovered_fixtures_match_oracle`.

## 3. pg-foma: Infix-with-drop capability  [owner: emit.rs candidate-selection region, capability.rs predicate + tests, coverage ledger]
- [x] 3.1 Verification probe (2026-08-10): the initial synthetic probe observed `bumat` recall through preexpand, but that was incidental coverage rather than predicate ground truth. The result led to the drop-aware structural widening and ownership handoff now recorded in 3.2/3.3; the current fixture/test truth is structural ownership.
- [x] 3.2 Widened `is_structural_rule` with the drop-aware `Role::Infix` arm; updated the predicate/emitter documentation to state that non-dropping `Infix` remains on preexpand.
- [x] 3.3 Ownership handoff: admitted Infix-with-drop rules leave `preexpand_candidates`; `circumfix_infix_ownership_handoff_is_clean`-style assertions preserve covered/uncovered clearing.
- [x] 3.4 Oracle containment fixture: synthetic Infix-with-drop grammar, generator + `Morpher` sweep, with 100% recall required and pinned by the companion reachability test.
- [x] 3.5 Predicate tests: Infix-with-drop is a positive `ConfirmOnly` pin; `Role::Reduplication` + drop remains the negative boundary. Coverage-ledger citation and golden are updated; the companion test is non-ignored under the structural ownership path.
- [x] 3.6 Docs: census C4 section in `docs/research/circumfix-composite-precedence-census.md`;
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
- [x] 5.1 `docs/hermitcrab-rust-port-audit.md`: record fwdata circumfix cross-product closed,
      explicitly distinguished from the C1/C2/C3 emit.rs items.
- [x] 5.2 STAGING.md entry: this change merges before `surface-compile-profile-and-templated-routing`;
      emit.rs region serialization noted.
