# Cover-compounding fixture notes (`pg-foma/tests/cover_compounding.rs`)

Proposer-to-confirm containment for `MorphRuleDef::Compounding`'s non-recursive case: the
license-gated head/non-head cross product `crate::emit::compound_license` proposes is checked
against `pg_parse::Morpher` (the full-HC oracle) via `pg_foma::composite::FomaAnalyzer`. The
synthetic fixture declares one `CompoundingRuleDef` ("cr1"), one subrule, linear ordering, and
default (1) `multipleApplication` — the exact shape `crate::capability`'s
`compounding_recursive` characterizes as non-recursive, so this fixture composes to
`ConfirmOnly`, never `Refuse` (`fixture_is_non_recursive_and_confirm_only`).

## The (un)group-awareness contract this fixture pins

`cr1.headProdRestrictionsMprFeatures="mpr1 mpr2"` is a RULE-level field, tested with
`MprSet::compound_match` — group-**unaware**. `mpr1`/`mpr2` belong to an `all`-type `MprGroup`.
`headA`'s own `ruleFeatures="mpr1 mpr3 mpr4"` carries only `mpr1` from that group:
`compound_match` admits it (flat overlap), but a group-aware `mpr_required_ok` reading of the
same field would demand both `mpr1` and `mpr2` (the `all`-type semantics) and would wrongly
exclude it — the exact "silently refusing stems `compound_match` would admit" bug this fixture
pins. `head_a_word_over_propose_confirm_prune` is the load-bearing witness: headA must still be
proposed (`candidates_generated > 0`) and confirmed (`confirmed == oracle exact`), proving
`crate::emit::compound_license` uses `compound_match`, not the group-aware helper, for this field.

The **subrule's** own `requiredMPRFeatures="mpr3 mpr4"` is tested with the group-**aware**
`Grammar::mpr_group_ok`, the opposite direction, and belongs to a second `all`-type `MprGroup`.
`headB` carries `mpr3` but not `mpr4` — `mpr_group_ok` correctly excludes it, matching confirm's
own `synth_compound`/`synth_compound_subrule` gate exactly
(`subrule_group_gate_excludes_partial_match_like_confirm`) — proving the subrule field is not
loosened to the flat `compound_match` test (which would have admitted `headB`, since
`{mpr3,mpr4}` overlaps `headB`'s own `{mpr1,mpr3}` on `mpr3`).

`headC` carries no MPR features at all, so `head_prod_restrictions_mpr`'s `compound_match`
(self non-empty, stem empty) rejects it outright — a negative control proving the rule-level gate
genuinely restricts something rather than vacuously admitting everything.

## Left to confirm, deliberately

`cr1.nonHeadPartsOfSpeech="posHead"` (a syntactic-FS gate) is never checked by
`crate::emit::compound_license` at all. `head_a_plus_bad_pos_non_head_over_propose_confirm_prune`
proves a non-head candidate the coarse MPR gate licenses (`non_head_prod_restrictions_mpr` is
empty/vacuous) but whose own part of speech disagrees is still proposed (over-approximation) and
pruned entirely by confirm's `is_unifiable` check — never silently dropped by propose, never
silently kept past confirm.

## A pre-existing compound-loop surface-order finding

`crate::emit`'s bounded compound loop concatenates head-root-text then non-head-root-text
unconditionally (its own physical lexc continuation order), regardless of a
`CompoundingSubruleDef`'s own `MorphologicalOutput` action order. This fixture's
`<MorphologicalOutput>` therefore copies `h0` then `n0` (head-first) to match; an earlier draft
used the non-head-first order and found the FST proposer never proposes the corresponding
"non-head+head" spelling at all when the two differ — a genuine, pre-existing scope limitation of
the compound loop's over-approximation, surfaced (not introduced) by this file's oracle-containment
run.
