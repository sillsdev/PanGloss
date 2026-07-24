# Tasks — cover-mpr-groups

## 1. Configuration-predicate boundary

- [ ] 1.1 Register `mpr-group.append-output` and `mpr-group.overwrite-output` as distinct
      `CapabilityPredicate`s with `add-capability-characteristics-check`'s registry (`design.md`
      D1-D3), including the realizational-rule (D4, shared `AffixAllomorphDef` field surface, no
      special case) and rule-ordering (D4, `cover-unordered-morph-rules`) interactions
- [ ] 1.2 Implement the operation-algebra check `evaluate()` needs to distinguish `Append`
      (commutative, monotone) from `Overwrite` (history-dependent) per touched `MprGroup`
      (`design.md` Novelty/risk — a new predicate-input kind, not a graph or per-rule check)
- [ ] 1.3 Confirm `mpr-group.overwrite-output` stays `FailClosed`; document the ADR 0005 override as
      its on-ramp

## 2. Proposal on the reified model — blocked on `reify-compilation-plans`

- [ ] 2.1 Author the non-narrowing propose baseline for `mpr-group.append-output` as a
      derivation-state-dependent `Gate` position (`design.md` D5) once `Plan`/`Gate` land
- [ ] 2.2 Positively verify the baseline never uses tracked accumulated MPR state to reject a
      candidate (D2 blocker 2) — `ConfirmOnly`, not an unproven `Admit` filter
- [ ] 2.3 Leave every `required_mpr`/`excluded_mpr` gate downstream of an `out_mpr`-bearing allomorph
      to confirmation; do not narrow propose past the licensed superset

## 3. Oracle witnesses

- [ ] 3.1 Positive witness: an `Append`-only group configuration the exact HC oracle accepts
- [ ] 3.2 Negative witness: a candidate licensed by the non-narrowing propose gate but rejected by
      the exact `mpr_group_ok`/`mpr_add_output` fold at confirm
- [ ] 3.3 Witness specifically pinning the `Append`/`Overwrite` order-(in)dependence distinction
      (`design.md` D4): the same rule multiset under two admissible orderings, differing only in
      final MPR state for an `Overwrite`-policy group

## 4. Synthetic conformance fixture

- [ ] 4.1 Add a `machine/conformance/` fixture named by construct, family/typology in comments only,
      covering `mpr-group.append-output`
- [ ] 4.2 Exercise the order-(in)dependence witness (3.3) inside the same fixture

## 5. Cost characterization

- [ ] 5.1 Measure/estimate the derivation-state-dependent `Gate` position's cost against
      `harden-foma-resource-safety` budgets
- [ ] 5.2 Propose a resource threshold to `calibrate-fst-resource-envelopes`; warn, never hard-fail,
      on cost alone (ADR 0001: cost and capability are gated by different standards)

## 6. Runtime-feature declaration

- [ ] 6.1 Declare `mpr-group.append-output`'s ADR 0004 required-runtime-feature set (default: fully
      lowered, contributes nothing, unless the state-dependent `Gate` needs a new runtime operation)

## 7. Promotion

- [ ] 7.1 Promote `mpr-group.append-output` from `FailClosed` to `ConfirmOnly` via the Stage 0A gate
      only after tasks 1-6 pass; `mpr-group.overwrite-output` is not promoted by this change

## 8. Open questions carried forward (not resolved by this change)

- [ ] 8.1 Record, for `cover-unordered-morph-rules`, the order-(in)dependence contract this change
      depends on (`design.md` D4) so a later `Unordered` change cannot silently violate it
- [ ] 8.2 Record, for `cover-compounding`, that a `CompoundingRuleDef` reachable as a loose stratum
      rule is an unproven composition with this change's predicates (`design.md` D4)
- [ ] 8.3 Record `Overwrite`-group admission-filtering as an explicitly open, not silently dropped,
      follow-on (`design.md` D3)
