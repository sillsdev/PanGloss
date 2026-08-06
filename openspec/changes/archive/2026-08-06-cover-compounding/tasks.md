# Tasks — cover-compounding

## 1. Configuration-predicate boundary

- [x] 1.1 Register `compounding.non-recursive` and `compounding.recursive` as distinct
      `CapabilityPredicate`s with `add-capability-characteristics-check`'s registry (`design.md` D2/D3)
      (`pg-foma/src/capability.rs::CompoundingRecursionSafePredicate`, id `"compounding.non-recursive"`;
      `compounding.recursive` is a `Refuse` arm inside the same predicate rather than a second struct —
      same discharge contract, one file)
- [x] 1.2 Implement the recursion-reachability check `compounding.non-recursive`'s `evaluate()`
      needs (a graph pass over `Grammar.mrules`, not a per-rule check — `design.md` Novelty/risk)
      (`capability.rs::compounding_recursive()`, a real graph pass)
- [x] 1.3 Confirm `compounding.recursive` stays `FailClosed`; document the ADR 0005 override as its
      on-ramp (`Refuse` arm + ADR 0005 citation inside `evaluate()`)

## 2. Proposal on the reified model — blocked on `reify-compilation-plans`

- [ ] 2.1 Author the `Union(Gate(head-trie) × Gate(non_head-trie))` node shape per subrule
      (`design.md` D3) once `Plan`/`Gate`/`Compose` land
      (not done — `design.md` itself says this node has not landed; production propose still lives
      directly in emit/replace/gate, not on the reified `Plan`)
- [ ] 2.2 Wire `Gate` partitions to `MprSet::compound_match` (rule-level restrictions) and
      `mpr_group_ok` (subrule `required_mpr`/`excluded_mpr`) — **never the reverse** (`design.md` D4)
      (not done as a Plan-level `Gate` wiring — same blocker as 2.1; the correct compound_match/
      mpr_group_ok split IS exercised end-to-end today, just not through a reified `Gate` node — see
      task 3.3's witnesses)
- [ ] 2.3 Leave `output_prod_restrictions_mpr`/`out_syn_fs`/`obligatory_features` to confirm; do not
      narrow propose past the licensed cross product
      (discipline documented in `design.md`, but not yet implemented as code since 2.1/2.2 aren't
      built)

## 3. Oracle witnesses

- [x] 3.1 Positive witness: a licensed head/non-head pair the exact HC oracle accepts
      (`pg-foma/tests/cover_compounding.rs::head_a_word_over_propose_confirm_prune`)
- [x] 3.2 Negative witness: a pair licensed by the coarse propose gate but rejected by
      `head_lhs`/`non_head_lhs` pattern match or output-side gates at confirm
      (`cover_compounding.rs::head_a_plus_bad_pos_non_head_over_propose_confirm_prune`, the POS gate)
- [x] 3.3 Witness specifically pinning the group-(un)awareness trap: a stem admitted by
      `compound_match` but excluded by the group-aware `mpr_required_ok`/`mpr_excluded_ok` reading —
      must confirm, must never be silently dropped by propose (`design.md` D4)
      (`cover_compounding.rs::subrule_group_gate_excludes_partial_match_like_confirm` +
      `head_c_excluded_by_rule_level_gate_like_confirm`)

## 4. Synthetic conformance fixture

- [ ] 4.1 Add a `machine/conformance/` fixture named by construct/composition (e.g. a head+non-head
      combination grammar), family/typology in comments only, covering `compounding.non-recursive`
      (staged, not yet graduated — `conformance-staging/edge-cases/compounding-non-recursive/
      {grammar.xml,words.yaml,STAGING.md}`; runs today via `conformance_fixtures_gate.rs` discovery but
      has not been proposed upstream to `machine/conformance/` yet)
- [x] 4.2 Exercise the MPR-group interaction witness (3.3) inside the same fixture, not a separate
      undiscoverable one
      (the `fasubel`/`tikubel` pair lives inside that same staged fixture, per its `STAGING.md`)

## 5. Cost characterization

- [ ] 5.1 Measure/estimate `(states + arcs)` for `Compose(head-trie, non_head-trie)` against
      `harden-foma-resource-safety` budgets — both operands are lexicon-scale, unlike affix
      concatenation's lexicon×small-affix-set shape (`design.md` D2 item 2)
      (not done — `compose_budget.rs` has no compounding/head-trie/non_head-trie-specific entries;
      `design.md` D2 item 2 itself states no threshold has been derived)
- [ ] 5.2 Propose a resource threshold to `calibrate-fst-resource-envelopes`; warn, never hard-fail,
      on cost alone (ADR 0001: cost and capability are gated by different standards)
      (not done — no threshold proposed anywhere)

## 6. Runtime-feature declaration

- [ ] 6.1 Declare `compounding.non-recursive`'s ADR 0004 required-runtime-feature set (default:
      fully lowered, contributes nothing, unless the boundary phon-cascade composition needs a new
      runtime operation — confirm before declaring)
      (not done — no ADR 0004/runtime-feature declaration found near `CompoundingRecursionSafePredicate`
      or in `design.md`, unlike `cover-unordered-morph-rules`/`cover-mpr-groups` which explicitly
      declare an empty set)

## 7. Promotion

- [ ] 7.1 Promote `compounding.non-recursive` from `FailClosed` to `ConfirmOnly` via the Stage 0A
      gate only after tasks 1-6 pass; `compounding.recursive` is not promoted by this change
      (code-level promotion already exists — `default_registry()` returns `CompileDecision::ConfirmOnly`
      for a non-recursive `Compounding` rule, proven by `capability_entry.rs`'s own test — but this
      task's own stated precondition, "only after tasks 1-6 pass," is not met: 2.x/5.x/6.1 above are
      still open. Left unchecked to match the stated gate, not the runtime behavior)

## 8. Open questions carried forward (not resolved by this change)

- [x] 8.1 Record, for `cover-mpr-groups`, the group-(un)awareness contract this change depends on
      (`design.md` D4) so a later `MprGroup` change cannot silently violate it
      (recorded in `design.md`)
- [x] 8.2 Record, for `cover-template-truncation-reduplication` (already merged) and for any future
      Stage 3 pairwise work, that compounding-into-template-slot is an unproven composition node
      (`design.md` D4)
      (recorded in `design.md`)
- [x] 8.3 Record recursive/self-feeding compounding's chain-depth interaction as an explicitly open,
      not silently dropped, follow-on (`design.md` D2 item 3)
      (recorded in `design.md`)
