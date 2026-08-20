# Circumfix composite-mechanism precedence (C1-C5)

Five precedence/admission cases for `build_structural_composites` (`pg_foma::emit`). C1-C3 are all
about `classify_affix` misclassifying a genuinely-circumfix shape (`Role::CircumfixPrefix`); C4 and
C5 are genuinely different roles (`Role::Infix`, then any dropping allomorph hidden behind a
`Role::Reduplication`-classified allomorph 0) that `is_structural_rule` widened to admit directly.
Each is pinned by a proposer-to-confirm containment check plus an ownership-handoff check that the
mechanism losing the rule actually relinquishes it.

## C1: non-first-allomorph selection (`emit::rule_role` / `emit::is_structural_rule`)

A rule with two allomorphs — an ordinary suffix at index 0, a circumfix at index 1 — must be
selected as a structural candidate regardless of which allomorph is declared first. The bug this
pins classified a rule by its first allomorph alone, which silently dropped the circumfix reading
when it was declared second (`conformance-staging/edge-cases/circumfix-non-first-allomorph-selection`).
`circumfix_allomorph_selection_is_order_independent` builds the identical rule with the two
allomorphs in both declaration orders and asserts both are selected identically.

## C2: reduplication-preempts-circumfix (`emit::classify_affix`)

An allomorph that is simultaneously circumfixing and reduplicating (the same LHS part copied twice,
wrapped by a leading and trailing insert) must classify as `Role::CircumfixPrefix`, and
`crate::peel::ReduplicationPeeler` must then relinquish it entirely — not merely stop preferring it
while still nominally claiming it. `ReduplicationPeeler`'s four scan kinds are each one-sided
surface matches that cannot recall a genuine wrap-both-sides-plus-reduplication surface, so
`has_redup_rules()` must be `false` for a grammar whose only rule is this one, once `classify_affix`
stops calling the shape `Role::Reduplication`.

## C3: infix-preempts-circumfix (`emit::classify_affix`)

An allomorph that is simultaneously circumfixing and infixing must classify as
`Role::CircumfixPrefix`, and `crate::preexpand`'s own candidate set (read via
`emit::composite_candidate_rules`'s `preexpand_candidates`) must drop it cleanly the moment it
reclassifies — never double-claimed by both mechanisms, never silently dropped by both.

## C4: genuinely-Infix admission (`emit::is_structural_rule`)

Unlike C1-C3, this is not a `classify_affix` misclassification: a rule whose primary allomorph
reads a genuine, non-circumfixing `Role::Infix` (an interior `InsertSegments` strictly between two
`CopyFromInput`s, no leading/trailing insert) that also drops LHS material (some LHS part is never
copied into the RHS) used to be categorically excluded from `build_structural_composites` by
`is_structural_rule`'s `_ => false` catch-all, regardless of the drop. `crate::preexpand` already
resynthesizes many such shapes via the real engine, but that coverage is bounded by its own
enumeration budget/pruning and is not a proven exact-containment argument — the real-world grammar
that surfaced this gap had allomorphs of this exact shape sitting in its own uncovered list, even
though a minimal synthetic construction of the same shape
(`conformance-staging/edge-cases/circumfix-cross-product-and-infix-drop`) happens to already be
covered by `crate::preexpand` incidentally; see that fixture's own `STAGING.md`, "Findings for unit
3", for why the incidental case does not generalize.

`is_structural_rule` now admits a `Role::Infix` rule the identical way it already admits
`None`/`Prefix`/`Suffix`: `allomorphs_of(g, mid).iter().any(rhs_drops_lhs_material)`. The ownership
handoff mirrors C3's: `preexpand.rs`'s own `candidate_rules` excludes any `Infix` rule
`is_structural_rule` now claims, so the two mechanisms never double-claim the same rule. A
non-dropping `Infix` rule is unaffected — it stays `crate::preexpand`'s job exactly as before.
`Role::Reduplication` is deliberately NOT widened (`crate::peel::ReduplicationPeeler`'s one-sided
scan kinds cannot recall a genuinely-dropping surface any more than they can a circumfix one, C2's
own reasoning) — it remains the fail-closed boundary.

Pinned by:
- `rust/crates/pg-foma/tests/phase_c_circumfix.rs::infix_with_drop_structural_recall_parity` —
  oracle containment through the structural route specifically (candidate-set membership, not just
  a count, so the test cannot pass via `crate::preexpand`'s own incidental coverage).
- `rust/crates/pg-foma/tests/circumfix_cross_product_and_infix_drop_candidate_selection.rs`'s
  `mr_cross_and_mr_infix_drop_are_the_structural_candidates` /
  `mr_infix_drop_leaves_preexpand_candidates_after_the_structural_widening` — the ownership handoff,
  on the staged cross-product-and-infix-drop fixture.
- `rust/crates/pg-foma/src/capability.rs`'s
  `circumfix_output_action_predicate_confirm_only_for_infix_role_drop` (flipped from a `Refuse` pin)
  and `circumfix_output_action_predicate_refuses_reduplication_role_drop` (the remaining negative
  boundary) — `CircumfixStructuralCompositePredicate`'s verdict.

## C5: non-first-allomorph drop hidden behind `Role::Reduplication` (`emit::is_structural_rule`)

C1's own bug (a rule classified by its first allomorph alone) recurs for the drop check C4 added:
a rule whose FIRST allomorph reads a genuine, non-dropping-relevant `Role::Reduplication` (or any
role outside `None`/`Prefix`/`Suffix`/`Infix`) but which carries a LATER allomorph that both drops
LHS material and itself classifies `None`/`Prefix`/`Suffix`/`Infix` used to be categorically
excluded, because `is_structural_rule`'s drop-aware arms keyed on `rule_role(g, mid)` — the FIRST
allomorph's classification alone — before ever checking the later allomorph's own shape. The
real-world grammar that surfaced this had exactly such an allomorph sitting in its own uncovered
list: "mrule 189 allomorph #4 (LHS-material-dropping output action)".

`is_structural_rule` now checks the drop condition per allomorph, never gated by `rule_role`: it
admits the rule the instant ANY allomorph both drops LHS material and classifies
`None`/`Prefix`/`Suffix`/`Infix` on its OWN RHS, regardless of what allomorph 0 (or any other
allomorph) classifies as. This subsumes C4's own widening (a rule genuinely `Role::Infix` at
allomorph 0 that drops is still admitted, since checking every allomorph includes allomorph 0) and
extends it to a rule whose first allomorph is `Role::Reduplication`.

The C4 negative boundary is preserved, restated per-allomorph rather than per-rule: a rule whose
ONLY dropping allomorph(s) classify `Role::Reduplication` still refuses, even when some OTHER,
non-dropping allomorph of the same rule classifies `None`/`Prefix`/`Suffix`/`Infix` — the
copy-count semantics of a genuine reduplication genuinely differ from a plain drop, so a dropping
`Role::Reduplication` shape never earns admission on its own, no matter which position it sits at
or what else the rule contains. `Role::CircumfixPrefix` and `Role::Process` need no equivalent
widening: both are already admitted unconditionally by earlier, allomorph-order-independent checks
in `is_structural_rule` (`any_allomorph_is_circumfix_prefix` and the `has_unemittable_action` scan),
so a rule carrying either shape at any allomorph index was never subject to the C1-shaped bug C5
fixes.

The ownership handoff needs no corresponding change in `preexpand.rs`'s own `candidate_rules`: that
module's role filter admits only `Role::Prefix`/`Suffix`/`Infix` rules (by `rule_role`, i.e.
allomorph 0's classification), and C5's target shape has allomorph 0 classifying `Reduplication` —
never a `candidate_rules` member to begin with, so there is nothing to relinquish. A rule whose
allomorph 0 IS `Prefix`/`Suffix`/`Infix` and which also carries a later `Reduplication`-shaped
dropping allomorph elsewhere is unaffected by C5 in the other direction too: `candidate_rules`
already calls `is_structural_rule(g, mid)` directly (never a re-derived copy of its logic), so any
future change to that predicate is automatically reflected in the handoff without a second edit.

Pinned by:
- `tests/phase_c_circumfix.rs::redup_first_allomorph_then_dropping_prefix_allomorph_structural_recall_parity`
  — candidate-set membership for the rule, the (vacuous) ownership-handoff check, and full
  proposer-to-confirm containment against the real oracle for BOTH allomorphs (the
  Reduplication-shaped one and the dropping Prefix-shaped one), swept over their own disjoint roots.
- `rust/crates/pg-foma/src/capability.rs`'s
  `circumfix_output_action_predicate_confirm_only_for_redup_first_then_prefix_drop_later` (the new
  positive `ConfirmOnly` pin) and `circumfix_output_action_predicate_refuses_reduplication_role_drop`
  (the C4 negative boundary, confirmed still refusing unperturbed) —
  `CircumfixStructuralCompositePredicate`'s verdict.

## Regression pin

`c1_and_c3_selection_is_unperturbed_by_the_c2_fix` re-runs C1's and C3's own staged fixtures through
the same public diagnostics their dedicated tests use, confirming the C2 reordering in
`classify_affix` did not shift either outcome.
