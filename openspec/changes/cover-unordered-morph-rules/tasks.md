# Tasks — cover-unordered-morph-rules

## 1. Configuration-predicate boundary

- [x] 1.1 Register `unordered-application.chain-depth-bounded` and
      `unordered-application.unbounded` as distinct `CapabilityPredicate`s with
      `add-capability-characteristics-check`'s registry (`design.md` D1)
      (`pg-foma/src/capability.rs::UnorderedOrderingUnionPredicate`, id
      `"unordered-application.chain-depth-bounded"`; unbounded is a `Refuse` arm in the same predicate)
- [x] 1.2 Implement the cardinality check `chain-depth-bounded`'s `evaluate()` needs: rule count ×
      derivation-chain-depth against the calibrated joint bound (`design.md` D1/Novelty)
      (`pg-foma/src/unordered.rs::stratum_metrics`/`check_unordered_strata_bound`, gated by
      `DEFAULT_ORDERING_MULTIPLICITY_BUDGET` in `compose_budget.rs`)
- [x] 1.3 Confirm `unordered-application.unbounded` stays `FailClosed`; document the ADR 0005 override
      as its on-ramp
      (`unordered.rs::check_unordered_strata_bound` returns `ComposeError::OrderingMultiplicityExceeded`;
      predicate's `Refuse` arm cites ADR 0005)

## 2. Proposal on the reified model — blocked on `reify-compilation-plans`

- [ ] 2.1 Author the ordering-union search-discipline widening on the stratum's existing chain
      subtree (`design.md` D2) once `Plan`/`Compose`/`Union` land
      (not done — no Plan-level widening node authored; the module doc argues `build_deriv_chain`, a
      pre-existing non-Plan mechanism, already achieves the widening — a repurposing, not the
      D2-specified `Plan` node)
- [ ] 2.2 Prove the widened recursion's proposed language equals the union over every admissible
      ordering under `combination_rec`'s exact semantics (`design.md` D1 blocker 2) — do not rely on
      the existing morphotactic-legality convention (`morphotactics.rs`) as a substitute proof
      (partial — `unordered.rs`'s module doc gives a reasoned inspection-based argument (grep-confirmed
      no `MorphRuleOrder` branching in `crate::emit`) plus oracle containment via test, but this is an
      inspection argument, not a formal proof of language-equality)
- [x] 2.3 Extend the ADR 0003 chain-depth budget with a calibrated ordering-multiplicity dimension
      (`design.md` D1 blocker 3 / D3)
      (`compose_budget.rs`: `ComposeBudget::ordering_multiplicity_cap`/`check_ordering_multiplicity`,
      a real calibrated dimension distinct from plain chain-depth)

## 3. Oracle witnesses

- [x] 3.1 Positive witness: a rule ordering within the calibrated bound the exact HC oracle accepts
      (`pg-foma/tests/cover_unordered_morph_rules.rs::non_document_order_analysis_is_proposed_and_confirmed`)
- [x] 3.2 Negative witness: an ordering licensed by the union proposal but rejected by the exact
      combination-cascade fold at confirm
      (`cover_unordered_morph_rules.rs::linear_variant_of_the_same_grammar_does_not_confirm_the_reverse_order`)
- [x] 3.3 Witness distinguishing the existing morphotactic-legality over-approximation (chain-
      attachment pruning only) from this change's proposal-language claim (`design.md` D1 blocker 2)
      (`cover_unordered_morph_rules.rs::no_phonology_isolates_build_deriv_chain_from_the_legality_pruning_convention`)

## 4. Synthetic conformance fixture

- [ ] 4.1 Add a `machine/conformance/` fixture named by construct, family/typology in comments only,
      covering `unordered-application.chain-depth-bounded`
      (not done — no `conformance-staging`/`machine/conformance` fixture dedicated to this predicate;
      `optional-template-composite`'s `morphologicalRuleOrder="unordered"` attribute is borrowed for a
      different, Aweti composite-explosion purpose, not this predicate's own fixture)
- [ ] 4.2 Include a word whose analysis is reachable only via a non-document-order application
      sequence, inside the same fixture
      (not done — only covered by the unit test in 3.1, not a fixture)

## 5. Cost characterization

- [x] 5.1 Measure/estimate the ordering-union subtree's `(states + arcs)` against
      `harden-foma-resource-safety` budgets, distinct from plain chain-length cost
      (`compose_budget.rs`: real, tested `ordering_multiplicity_cap`, calibrated dimension genuinely
      distinct from chain-depth)
- [x] 5.2 Propose the calibrated ordering-multiplicity bound to `calibrate-fst-resource-envelopes`
      (governed like ADR 0003's own calibration: evidence + proposed diff + human-reviewed commit)
      (env-var-overridable default (100) with tests, e.g.
      `ordering_multiplicity_from_env_defaults_to_a_calibrated_bound`)

## 6. Runtime-feature declaration

- [x] 6.1 Declare `unordered-application.chain-depth-bounded`'s ADR 0004 required-runtime-feature set
      (default: fully lowered, contributes nothing, unless the widened recursion needs a new runtime
      operation — confirm before declaring)
      (`unordered.rs` explicitly declares "None required")

## 7. Promotion

- [ ] 7.1 Promote `unordered-application.chain-depth-bounded` from `FailClosed` to `ConfirmOnly` via
      the Stage 0A gate only after tasks 1-6 pass; `unordered-application.unbounded` is not promoted
      by this change
      (code-level promotion already exists — `default_registry()` returns `ConfirmOnly` unconditionally
      when bounded — but task 2.1 (the Plan-level widening node) is not built, so promotion is ahead of
      this task's own stated "after tasks 1-6 pass" gate. Left unchecked to match the stated gate)

## 8. Open questions carried forward (not resolved by this change)

- [x] 8.1 Record, for `cover-mpr-groups`, the order-(in)dependence contract this change depends on
      (`design.md` D3) so a later `MprGroup` change cannot silently violate it
      (recorded in `design.md`)
- [x] 8.2 Record, for `cover-compounding`, that a `CompoundingRuleDef` reachable as a loose stratum
      rule is an unproven composition with this change's ordering multiplication (`design.md` D3)
      (recorded in `design.md`)
- [x] 8.3 Record whether ordering-multiplicity composes additively or multiplicatively with plain
      chain-depth as an explicitly open follow-on for the ADR 0003 calibration (`design.md`
      Novelty/risk)
      (recorded in `design.md`)
