# Tasks — cover-unordered-morph-rules

## 1. Configuration-predicate boundary

- [ ] 1.1 Register `unordered-application.chain-depth-bounded` and
      `unordered-application.unbounded` as distinct `CapabilityPredicate`s with
      `add-capability-characteristics-check`'s registry (`design.md` D1)
- [ ] 1.2 Implement the cardinality check `chain-depth-bounded`'s `evaluate()` needs: rule count ×
      derivation-chain-depth against the calibrated joint bound (`design.md` D1/Novelty)
- [ ] 1.3 Confirm `unordered-application.unbounded` stays `FailClosed`; document the ADR 0005 override
      as its on-ramp

## 2. Proposal on the reified model — blocked on `reify-compilation-plans`

- [ ] 2.1 Author the ordering-union search-discipline widening on the stratum's existing chain
      subtree (`design.md` D2) once `Plan`/`Compose`/`Union` land
- [ ] 2.2 Prove the widened recursion's proposed language equals the union over every admissible
      ordering under `combination_rec`'s exact semantics (`design.md` D1 blocker 2) — do not rely on
      the existing morphotactic-legality convention (`morphotactics.rs`) as a substitute proof
- [ ] 2.3 Extend the ADR 0003 chain-depth budget with a calibrated ordering-multiplicity dimension
      (`design.md` D1 blocker 3 / D3)

## 3. Oracle witnesses

- [ ] 3.1 Positive witness: a rule ordering within the calibrated bound the exact HC oracle accepts
- [ ] 3.2 Negative witness: an ordering licensed by the union proposal but rejected by the exact
      combination-cascade fold at confirm
- [ ] 3.3 Witness distinguishing the existing morphotactic-legality over-approximation (chain-
      attachment pruning only) from this change's proposal-language claim (`design.md` D1 blocker 2)

## 4. Synthetic conformance fixture

- [ ] 4.1 Add a `machine/conformance/` fixture named by construct, family/typology in comments only,
      covering `unordered-application.chain-depth-bounded`
- [ ] 4.2 Include a word whose analysis is reachable only via a non-document-order application
      sequence, inside the same fixture

## 5. Cost characterization

- [ ] 5.1 Measure/estimate the ordering-union subtree's `(states + arcs)` against
      `harden-foma-resource-safety` budgets, distinct from plain chain-length cost
- [ ] 5.2 Propose the calibrated ordering-multiplicity bound to `calibrate-fst-resource-envelopes`
      (governed like ADR 0003's own calibration: evidence + proposed diff + human-reviewed commit)

## 6. Runtime-feature declaration

- [ ] 6.1 Declare `unordered-application.chain-depth-bounded`'s ADR 0004 required-runtime-feature set
      (default: fully lowered, contributes nothing, unless the widened recursion needs a new runtime
      operation — confirm before declaring)

## 7. Promotion

- [ ] 7.1 Promote `unordered-application.chain-depth-bounded` from `FailClosed` to `ConfirmOnly` via
      the Stage 0A gate only after tasks 1-6 pass; `unordered-application.unbounded` is not promoted
      by this change

## 8. Open questions carried forward (not resolved by this change)

- [ ] 8.1 Record, for `cover-mpr-groups`, the order-(in)dependence contract this change depends on
      (`design.md` D3) so a later `MprGroup` change cannot silently violate it
- [ ] 8.2 Record, for `cover-compounding`, that a `CompoundingRuleDef` reachable as a loose stratum
      rule is an unproven composition with this change's ordering multiplication (`design.md` D3)
- [ ] 8.3 Record whether ordering-multiplicity composes additively or multiplicatively with plain
      chain-depth as an explicitly open follow-on for the ADR 0003 calibration (`design.md`
      Novelty/risk)
