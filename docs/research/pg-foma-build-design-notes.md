# pg-foma build.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/build.rs` implementation comments so the
source can carry a one- or two-line pointer instead of the full argument.

## `reroute_null_shaped_affix_chains`: the epsilon-cycle precision regression

`uflexc`'s prefix/suffix continuation lexicons are deliberately self-looping so ordinary affixes can
stack in any order; taking the loop normally consumes at least one real surface character, so
recursion depth is bounded by query length. That bound breaks for an affix allomorph whose entire
underlying shape is drawn only from `Boundary`-kind characters: once the blanket boundary-cleanup
net deletes every character of that allomorph's spelling, its lexc line degenerates to a zero-width,
epsilon-tagged entry sitting on the self-loop — a free, unboundedly repeatable insertion of that
morph's tag. Because `apply_up` enumerates distinct accepting upper-tape strings and each repeat
count produces a different tag sequence, this multiplies proposals combinatorially (measured: a
127 -> 53992 blow-up, 425x, on one 5-word slice, 99.5% attributable to a single word).

Deleting the boundary characters is not itself the bug — the network is already correctly
unqueryable before cleanup runs, and cleanup has to happen for recall. The bug is a zero-width
transition landing back on a state that can be revisited. The fix reroutes exactly the null-shaped
lines, and only those, off the self-looping continuation onto a one-shot successor that cannot be
re-entered, so the null/zero morph behaves like an ordinary optional morph occurring at most once
per juncture (its actual grammatical meaning) instead of a free repeatable insertion.

**Rejected alternative:** an earlier version routed a null-shaped line straight to the "no more
affixes" terminal state. That is too narrow — the affix chain exists so ordinary affixes combine in
any order, and the ground-truth analyzer genuinely admits a real affix before or after a null one.
Routing straight to the terminal silently drops whichever order took the null affix first (caught by
a multiplicity-mismatch regression on a two-affix combination). The successor state after a
null/marker line must still admit every *ordinary* affix, in any quantity — just never a second
null-shaped line, which is what would reopen the epsilon cycle. Hence the duplicated `*NoNull` lexc
bodies: ordinary lines get a second copy whose continuation stays inside the "already used the
marker" universe, while marker lines themselves are never duplicated into that universe.

**Known limitation — the guard is name-scoped, not structural.** The rewrite recognizes exactly the
two lexicon names `uflexc` happened to emit for the top-level chains (`PrefixChain`/`SuffixChain`).
When `uflexc`'s bounded compound loop later added its own per-level self-looping prefix lexicons,
re-emitting every line (null-shaped ones included) with the level's own lexicon as continuation,
those new lexicons were exactly the same hazard and this rewrite's name-based match could not see
them — a name-based guard cannot defend a lexicon that did not exist when the guard was written. The
real fix for the compound levels lives in `uflexc` itself, applying the at-most-once discipline
directly at emission time; widening this `match` would only postpone the same regression to the next
lexicon someone adds. This function remains the only mechanism for the top-level pair (moving that to
emission time too would change an already-calibrated net shape).
