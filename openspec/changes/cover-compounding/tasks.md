# Tasks — cover-compounding

## 1. Configuration-predicate boundary

- [ ] 1.1 Register `compounding.non-recursive` and `compounding.recursive` as distinct
      `CapabilityPredicate`s with `add-capability-characteristics-check`'s registry (`design.md` D2/D3)
- [ ] 1.2 Implement the recursion-reachability check `compounding.non-recursive`'s `evaluate()`
      needs (a graph pass over `Grammar.mrules`, not a per-rule check — `design.md` Novelty/risk)
- [ ] 1.3 Confirm `compounding.recursive` stays `FailClosed`; document the ADR 0005 override as its
      on-ramp

## 2. Proposal on the reified model — blocked on `reify-compilation-plans`

- [ ] 2.1 Author the `Union(Gate(head-trie) × Gate(non_head-trie))` node shape per subrule
      (`design.md` D3) once `Plan`/`Gate`/`Compose` land
- [ ] 2.2 Wire `Gate` partitions to `MprSet::compound_match` (rule-level restrictions) and
      `mpr_group_ok` (subrule `required_mpr`/`excluded_mpr`) — **never the reverse** (`design.md` D4)
- [ ] 2.3 Leave `output_prod_restrictions_mpr`/`out_syn_fs`/`obligatory_features` to confirm; do not
      narrow propose past the licensed cross product

## 3. Oracle witnesses

- [ ] 3.1 Positive witness: a licensed head/non-head pair the exact HC oracle accepts
- [ ] 3.2 Negative witness: a pair licensed by the coarse propose gate but rejected by
      `head_lhs`/`non_head_lhs` pattern match or output-side gates at confirm
- [ ] 3.3 Witness specifically pinning the group-(un)awareness trap: a stem admitted by
      `compound_match` but excluded by the group-aware `mpr_required_ok`/`mpr_excluded_ok` reading —
      must confirm, must never be silently dropped by propose (`design.md` D4)

## 4. Synthetic conformance fixture

- [ ] 4.1 Add a `machine/conformance/` fixture named by construct/composition (e.g. a head+non-head
      combination grammar), family/typology in comments only, covering `compounding.non-recursive`
- [ ] 4.2 Exercise the MPR-group interaction witness (3.3) inside the same fixture, not a separate
      undiscoverable one

## 5. Cost characterization

- [ ] 5.1 Measure/estimate `(states + arcs)` for `Compose(head-trie, non_head-trie)` against
      `harden-foma-resource-safety` budgets — both operands are lexicon-scale, unlike affix
      concatenation's lexicon×small-affix-set shape (`design.md` D2 item 2)
- [ ] 5.2 Propose a resource threshold to `calibrate-fst-resource-envelopes`; warn, never hard-fail,
      on cost alone (ADR 0001: cost and capability are gated by different standards)

## 6. Runtime-feature declaration

- [ ] 6.1 Declare `compounding.non-recursive`'s ADR 0004 required-runtime-feature set (default:
      fully lowered, contributes nothing, unless the boundary phon-cascade composition needs a new
      runtime operation — confirm before declaring)

## 7. Promotion

- [ ] 7.1 Promote `compounding.non-recursive` from `FailClosed` to `ConfirmOnly` via the Stage 0A
      gate only after tasks 1-6 pass; `compounding.recursive` is not promoted by this change

## 8. Open questions carried forward (not resolved by this change)

- [ ] 8.1 Record, for `cover-mpr-groups`, the group-(un)awareness contract this change depends on
      (`design.md` D4) so a later `MprGroup` change cannot silently violate it
- [ ] 8.2 Record, for `cover-template-truncation-reduplication` (already merged) and for any future
      Stage 3 pairwise work, that compounding-into-template-slot is an unproven composition node
      (`design.md` D4)
- [ ] 8.3 Record recursive/self-feeding compounding's chain-depth interaction as an explicitly open,
      not silently dropped, follow-on (`design.md` D2 item 3)
