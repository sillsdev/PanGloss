# Templated-underlying composed networks: `apply_up` can hang, not just run long

Moved out of `rust/crates/pg-foma/examples/p6_templated_replace_prototype.rs`'s spot-check section
so the source can carry a two-line pointer instead of the full argument.

## What was observed

For a large, many-template/many-slot grammar, composing a templated underlying-form lexc network
with its phonological-rule cascade and boundary cleanup, then minimizing, produces a network where
`apply_up` is fast for some query words and can run for a very long time — observed: many minutes,
stopped only by an external, process-level kill — for others, with no way to predict which from the
query alone. Individually verified (external-kill-safe):

- A bare, unaffixed root resolved at the very first raw result, sub-millisecond.
- A short affixed word completed 250,000 raw results quickly but never decoded the correct analysis
  anywhere in that window — a genuine miss, not a timeout.
- A different short word did not complete even 500 raw results within 45 seconds of wall clock and
  had to be killed externally.

No in-process technique reliably caught the hang before the external kill was needed, including a
worker-thread plus a receive-with-timeout, which was independently verified to bound a synthetic
spinning thread but did not reliably bound this real case.

## Root-cause hypothesis

Not fully proven at the composed-transition-table level, but well supported by the word-dependent
behavior above: a template-less suffix derivation chain re-offers the same set of standalone suffix
rules at every level of the chain, with no "each specific rule used at most once" bookkeeping. Some
of those rules have an "elsewhere" allomorph whose entire underlying insert text is a single
boundary-kind token; boundary cleanup turns that into a true epsilon-on-lower/tag-on-upper arc, and
minimization may then collapse many structurally-identical derivation levels into far fewer states
— plausibly introducing a genuine cycle (an unboundedly repeatable free tag insertion). That would
explain why some queries hang outright rather than merely running long: the search space reachable
from them is not just large, it may not be finite.

## The generalizable lesson

A derivation construction that always re-offers the same rule set at every level, combined with
epsilon-producing "elsewhere" allomorphs and post-compose minimization, can silently introduce
cycles in the composed network. Once that happens, bounding `apply_up` against it by wall-clock
timeout inside the same process is not reliable — only an external, process-level bound is. Any
future compiler tuned for this construct shape should add "each rule used at most once per chain"
bookkeeping (or an equivalent structural bound) rather than relying on a runtime cap to catch it.
