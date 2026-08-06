# pg-foma lower.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/lower.rs` implementation comments so the
source can carry a one-line pointer instead of the full argument. Each section corresponds to one
call site; the site names the function/type so this doc can be found from either direction.

## `Slot::Repeat`: why an unbounded quantifier is a native construction, not a scope limit

`PatternNode::Quantifier`'s own `max` field is `Option<u32>`: `None` is the DTD's `max="-1"`
unbounded sentinel and its default (an absent `max` attribute defaults to `-1`), so unbounded is
the DTD's default shape, not an exotic corner. The backend has a native, exact, finite-size
construction for the unbounded case too — foma's xre parses `E^>N`/`E*`, building them with no
cutoff — so refusing an unbounded quantifier would be a scope restriction, not a genuine
feasibility limit. `MAX_QUANTIFIER_BOUND` applies only when `max` is `Some(_)`: an unbounded
quantifier's own compiled net size does not depend on any repetition count, so there is nothing for
that ceiling to bound, and `max: None` is never coerced to `Some(_)` to force the check to run
(that would round an honest refusal toward false acceptance, the direction this crate's
under-approximation rule forbids).

Rendered (`render_slots`) as foma's own native repetition xre operator, never a hand-rolled
expansion: `[<children text>]^{min,max}` for the finite case, `[<children text>]*`/
`[<children text>]^>{min-1}` for the unbounded case. Compiled size is linear in `min`, and for the
unbounded case independent of any repetition count at all, since a native Kleene star/plus's
compiled net size does not scale with how many times it can match.

`children: Vec<Slot>` rather than a second `Pattern`: `slots_from_nodes` (this variant's own
builder) already turns a `Quantifier`'s `children: Vec<PatternNode>` into slots via the identical
recursive call it uses for the pattern's own top-level nodes — one node-to-slot mapping, reused,
not re-derived. Storing already-resolved `Slot`s means `render_slots` can render a nested
quantifier the same way it renders every other slot list, with no special-cased second
PatternNode-to-text path.

No `Slot::Alpha` may ever appear (transitively) inside `children`: `resolve_alpha_tuples`'s own
occurrence-flattening walks the top-level LHS/RHS/left-env/right-env lists at exactly one level, so
an `Alpha` occurrence buried inside a `Slot::Repeat`'s own `children` would never be discovered,
never receive a resolved assignment, and would panic `render_slots`'s own render-time expect the
first time anyone tried to render it. Refusing to build the `Slot::Repeat` in the first place
(rather than teaching `resolve_alpha_tuples` to recurse) keeps that invariant enforced at
construction time, not merely by convention.

## `Slot::Anchor`: why the carried `AnchorSide` is never read, and why it stays

Renders (`render_slots`) as foma's own `.#.` xre atom identically regardless of which `AnchorSide`
this occurrence carries: the compiled meaning ("start of word" vs "end of word") comes entirely
from which side of the rule's own focus marker the rendered text sits on — `Anchor(Left)` is always
prepended to `left_env`, `Anchor(Right)` always appended to `right_env` — never from the tag itself.
This is what makes `compile_rtl_branch_net`'s mirror-and-reverse construction swap an anchor to the
correct opposite edge with no anchor-specific code at all: `reversed_slots` reverses this slot's own
position within its containing environment list and swaps `left_env`/`right_env` wholesale, so a
`Right`-anchor that was the last slot of the original `right_env` becomes the first slot of the
mirror's own `left_env`, rendered as a leading `.#.` there — which `fsm_reverse` then correctly
turns into "end of the real string" for the final network. Pinned empirically, not just argued, by
`tests/phase_c_right_to_left.rs`'s `rtl_anchor_reversal_swaps_the_correct_edge`.

`#[allow(dead_code)]` on the carried `AnchorSide`: kept anyway, not collapsed to a unit variant,
because it is real structural information a future caller (a diagnostic, or a stricter
position-validity check) may legitimately want, and because `PatternNode::Anchor(AnchorSide)` (the
node this variant mirrors) carries it too — dropping it here would be a lossy projection for no
code-size benefit worth mentioning.

## `render_slots`'s unbounded-`min` off-by-one

Foma's `E^>N` operator ("more than N") builds `n` mandatory copies followed by one or more further
copies — `n` mandatory plus at least 1 more is strictly more than `n` copies, i.e. `n+1` or more.
So `^>N` means "more than N", not "N or more": rendering `min` "or more" therefore requires
`^>(min-1)`, never `^>min`, which would wrongly demand `min+1` or more. Pinned by
`render_slots_unbounded_min_off_by_one_boundary`, which distinguishes `min` occurrences (must match)
from `min-1` (must not).
