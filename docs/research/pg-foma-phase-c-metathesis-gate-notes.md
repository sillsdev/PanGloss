# `PhonRuleDef::Metathesis` real FST semantics (`tests/phase_c_metathesis.rs`)

`pg_foma::replace::compile_metathesis_rule` compiles via a dedicated swap relation: a per-branch
literal cross-product union, mirroring `resolve_alpha_tuples`'s identity-preservation fix. Any
rule whose whole pattern is a shape `pg_foma::replace::pattern_slots` accepts (no
`Quantifier`/`Segments`/`Anchor`, no `Slot::Alpha`/`Slot::Repeat` anywhere) compiles to a real swap
relation, oracle-exact for the well-formed switch-tag convention, for `Dir::LeftToRight`.
`Dir::RightToLeft` compiles too, via the same mirror-and-reverse construction `compile_rtl_branch_net`
uses for RTL rewrite rules — a proven SAFE SUPERSET of the true RTL relation (`ConfirmOnly`, never
`Admit`), not proven oracle-exact the way the `Dir::LeftToRight` case is.

Synthetic fixtures, named by construct. Each compilable fixture is checked against
`pg_parse::Morpher`, following `tests/phase_c_right_to_left.rs`/`tests/two_table_symbol_divergence.rs`'s
established `fst_candidate_set`/`oracle_candidate_set` methodology.

## Scope compiled faithfully vs. left honestly unsupported

See `pg_foma::replace`'s module doc (the "Metathesis" section) for the full, cited scope line. In
short, EITHER `Dir` now: no `Anchor` anywhere in the pattern (not a metathesis-specific gap — the
identical refusal already applies to every `RewriteRuleDef` LHS/RHS/environment carrying one); no
`Quantifier`/`Segments`/disagree-polarity alpha var/`Slot::Alpha` anywhere. `Slot::Alpha` is
structurally IMPOSSIBLE for a `<MetathesisRule>` (`pg_grammar::load::load_metathesis_rule` resolves
every node against an EMPTY `VarTable`, so an `<AlphaVariable>` inside one errors the whole grammar
load); a `Slot::Repeat` occurrence IS structurally reachable, just never attested in any fixture
this crate has authored, and stays refused for either direction.

## `Dir::RightToLeft`: what this file pins for it

`metathesis_right_to_left_reversal_matches_oracle_exactly` is the load-bearing containment
witness: every analysis `pg_parse::Morpher` finds for its own words is a member of the FST
proposer's candidate set. It reuses `RIGHT_TO_LEFT_XML` unchanged — that grammar has exactly ONE
valid switch window per lexical entry, so it cannot by itself distinguish `Dir::RightToLeft` from
`Dir::LeftToRight` (see the empirical finding below).
`metathesis_right_to_left_differs_from_compiling_as_left_to_right` is the complementary,
oracle-free witness that the CONSTRUCTION itself is genuinely direction-aware.
`metathesis_right_to_left_switch_index_remap_matches_the_derived_formula` is an end-to-end
behavioral confirmation of the remap `pg_foma::replace::metathesis_mirror_switch_index_remap_tests`
already pins arithmetically.

**Empirical finding: `pg_rules::metathesis` is direction-blind, at least for the shape checked.**
A throwaway probe declared the SAME two-adjacent-same-class-switch `MetathesisRule` under both
`Dir::LeftToRight` and `Dir::RightToLeft` and called `pg_rules::metathesis::synthesize` directly on
an OVERLAPPING-window input ("pqp", positions 0-1 and 1-2 both matching the switch pattern): both
directions synthesized identically ("qpp", the LEFTMOST window's swap) —
`pg_rules::metathesis::match_candidates` sorts candidates ascending (leftmost-first) REGARDLESS of
`rule.dir`, and the application loop always takes the first sorted candidate. This is the SAME
empirical shape `tests/phase_c_right_to_left.rs`'s top doc found (before its own fix) for ordinary
`Iterative` rewrite rules — direction-blind pick-order, not direction-aware — and is exactly why
this file's containment witness reuses a NO-OVERLAP grammar (oracle recall is byte-identical
whichever direction is declared) while the DIFFERS-FROM-LTR witness is bare-automaton and
oracle-free (the one place an overlap genuinely needs to be constructed).

## Two `pg_rules::metathesis::build_analysis_pattern` invariants this suite pins

Numbered because comments throughout this file refer to them as "gap 1" and "gap 2".

**Gap 1 — physical position, not tag name, decides switch order.**
`pg_rules::metathesis::synthesize`'s `synthesis_reorder`/`move_nodes_after` algorithm is driven by
PHYSICAL position: whichever switch is physically LAST in `pattern.nodes` always ends up FIRST in
the output, tag-name-agnostic. `build_analysis_pattern` mirrors that: it orders its rebuilt search
pattern by PHYSICAL position, not by tag name, so analysis and synthesis agree even for a rule
whose `left_switch` node is physically first.

**Gap 2 — a context node between the switches is kept, unless it is a boundary.**
A context node strictly between the two switches keeps its slot in the rebuilt search pattern
(`synthesis_reorder` does not drop it either) UNLESS it resolves to a `CharDefKind::Boundary`,
which is excluded from the analysis match sequence regardless of pattern shape.

`replace.rs`'s swap-relation construction is tag-name-agnostic and does not drop the middle node
either, so its proposals are already the semantically correct ones under both rules above.

## The bare-automaton proof that the RTL construction is genuinely direction-aware

`metathesis_right_to_left_differs_from_compiling_as_left_to_right` is the complementary,
oracle-free witness that the CONSTRUCTION itself is genuinely direction-aware, mirroring
`tests/phase_c_right_to_left.rs`'s "aa -> b" worked example exactly (bare automaton, single-shot
`apply_down`, no grammar/oracle involved). Needed because the containment witness above
deliberately uses a NO-OVERLAP grammar and so cannot, by itself, rule out the construction silently
degenerating to "compiled as if `Dir::LeftToRight`" — without this test, that regression would
leave every other test in this file still green.

A metathesis switch pattern can never exhibit a genuine same-branch overlap at width 2 (the two
switch positions would have to hold EQUAL values, making the swap a no-op) — but a 4-node pattern
`[v0, v1, v2, v3]` with switches at `{0, 3}` and a period-2 assignment `[a, b, a, b]` genuinely
self-overlaps (shift 2) against the input `"ababab"`. The literal branch for this ONE assignment is
`"a b a b -> b b a a"` (`rhs_vals` with positions 0 and 3 transposed). Plain foma `->` prefers the
LEFTMOST non-overlapping match: `apply_down` on `"ababab"` gives `"bbaaab"`. The mirror rule for
switches `{0, 3}` in a 4-slot pattern remaps to `{n - 1 - 0, n - 1 - 3} = {3, 0}` — the SAME set —
so the mirror pattern is `reversed_slots([a,b,a,b]) = [b,a,b,a]`, and its own swap gives mirror-RHS
`[a,a,b,b]`, i.e. the branch `"b a b a -> a a b b"`. `fsm_reverse` of that compiled branch, applied
to the SAME `"ababab"`, gives `"abbbaa"` — the RIGHTMOST-preferring result — PROVABLY DIFFERENT
from the plain branch's `"bbaaab"`, entirely independent of any oracle.

This deliberately does NOT extend to a full grammar-level compile comparison (tried while authoring
this test, then removed): once the plain/reversed-mirror branches above are unioned with the OTHER
cross-product branches a real multi-position natural-class pattern needs, `apply_down`'s single-shot
exploration order over the larger unioned automaton stopped reliably favoring the "abab"-literal
branch's transformation at all (empirically: it found an IDENTITY path first for BOTH the real RTL
compile and a `dir`-forced-LeftToRight clone of the same rule). That is an artifact of `fsm_union`'s
state-numbering/exploration order, not evidence the underlying MECHANISM stopped being
direction-aware — the bare two-branch proof above isolates that mechanism directly.

## The switch-index remap end-to-end confirmation

`metathesis_right_to_left_switch_index_remap_matches_the_derived_formula` confirms the remap
`pg_foma::replace::metathesis_mirror_switch_index_remap_tests` already pins arithmetically
in-crate: an ASYMMETRIC 5-node pattern (`Segment`s `a,b,c,d,e`, switches at indices 0 and 1, three
trailing fixed context nodes) has no natural-class alternation at all, so its entire compiled
relation is exactly one literal mapping — any remap error (an off-by-one landing on the WRONG pair
of the mirror's slots) would either panic or silently produce a DIFFERENT literal output than the
one derived by hand, so this test fails loudly either way under a regression.
