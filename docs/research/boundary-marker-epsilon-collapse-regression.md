# The boundary-marker epsilon-collapse regression (`pg-foma/tests/boundary_marker_epsilon_collapse_gate.rs`)

## The shape

An affix allomorph whose entire underlying shape is composed only of `Boundary`-kind characters
(e.g. a null/zero-morph marker immediately followed by an ordinary separator, `^0+`) degenerates
after `pg_foma::uflexc`'s boundary-cleanup step: every character of the allomorph is deleted, so its
lexc line becomes a bare, zero-width, epsilon-tagged entry whose continuation loops back to the
state it came from. That is a free, repeatable insertion point available at every prefix juncture,
not a one-off oddity — `pg_foma::uflexc`'s prefix/suffix continuation classes are deliberately
self-looping.

## The fix

Not "exclude the marker family from cleanup" — tried first and rejected, since excluding any
`Boundary` char-def from deletion is a straight recall regression
(`MultiplicityMismatch { word: "s", expected: 2, actual: 1 }`, pinned by
`null_morph_prefix_does_not_collapse_to_a_free_epsilon_loop`).

The actual fix is `build::reroute_null_shaped_affix_chains`, applied to a group's raw `uflexc` lexc
source before it is compiled, so a line whose entire underlying text is drawn only from boundary
tokens never reaches the compiled `Fsm` sitting on a self-looping continuation in the first place.
This mirrors what `crate::emit` already does: boundary characters never go onto the queryable tape
at all, instead of being emitted and then deleted.

## Measured gate limitation

The synthetic fixture in this file pins the recall half only. It does not pin precision: verified by
bypassing `reroute_null_shaped_affix_chains` with `pg-foma` rebuilt, after which the fixture still
reports `total_proposals <= 20` and passes — its words are too short and it has too few root rules
for the epsilon cycle to multiply past the ceiling. The precision half is pinned separately, on the
real grammar where the defect manifests
(`corpus_large_lexicon_proposals_stay_bounded_after_the_reroute`): 575 proposals with the fix,
53,992 with it bypassed, over the same deterministic 5-word slice — confirmed to fail on the broken
build.

## The second regression: compound-level self-looping

`reroute_null_shaped_affix_chains` de-loops the two lexicons it knows by name (`PrefixChain`,
`SuffixChain`). The bounded compound loop later added a per-level self-looping prefix lexicon of its
own (`UCmpPfx0`, `UCmp2Pfx0`, ...) built by re-emitting every line in `prefix_lines` — null-shaped
ones included — with the level's own lexicon as the continuation. The name-scoped guard could not
see those lexicons, so the same epsilon cycle reopened once per compound level.

This class is pinned structurally (on the emitted lexc text,
`compound_level_null_shaped_prefix_is_not_a_free_epsilon_loop`) rather than by a proposal ceiling,
because the measured limitation above means a ceiling on a fixture this small cannot discriminate a
fixed build from a broken one. The fix is likewise structural, in `uflexc`'s own `prefix_hop` at
emission time: a name-based guard cannot defend a lexicon that did not exist when it was written.

## Delanguaging

Every fixture in this file is a synthetic construction (`s`/`p`/`t` segments) pinning this specific
FST-construction defect, not a language sample, per this repo's conformance-grammar convention.
