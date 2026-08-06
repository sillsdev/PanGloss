# Circumfix composite-mechanism precedence (C1/C2/C3)

Three precedence cases between the circumfix composite mechanism (`pg_foma::emit`, classifying
`Role::CircumfixPrefix`) and the other composite mechanisms it can collide with. Each is pinned by a
proposer-to-confirm containment check (`assert_full_containment` in
`pg-foma/tests/circumfix_candidate_selection.rs`) plus an ownership-handoff check that the mechanism
losing the rule actually relinquishes it.

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

## Regression pin

`c1_and_c3_selection_is_unperturbed_by_the_c2_fix` re-runs C1's and C3's own staged fixtures through
the same public diagnostics their dedicated tests use, confirming the C2 reordering in
`classify_affix` did not shift either outcome.
