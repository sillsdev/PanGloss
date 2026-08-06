# pg-foma e2_infix_probe.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/e2_infix_probe.rs` implementation comments
so the source can carry a one- or two-line pointer instead of the full argument.

## `has_unemittable_action_u`: a `Modify`/ablaut action is not an architectural gap

An allomorph whose RHS carries a `Modify`/`InsertContext` action has no literal text at all, which
looked at first like a pre-existing, architecture-independent limitation inherited from a doc
comment alone. It is not: `pg_rules::morph::synthesize` correctly *executes* a `Modify` action (it
changes the real `Shape`/feature identity directly; it never needs a literal string), so mainline's
production path already covers this shape when preexpand is on. The limitation is only in the
static leaf that renders an allomorph to literal insert text, not in the underlying synthesis
mechanism — so this construct is squarely in scope for the same splice mechanism as
Infix/structural rules, and dropping it would be a real recall regression, not a deferrable gap.

## `special_rules_u`: why the splice set is not "every affix rule in the grammar"

The obvious candidate set — every Prefix/Suffix/Infix rule — is wrong here: on a real grammar that
set can be the vast majority of all rules, and recursively chaining a bounded depth over a set that
large reproduces the exact `O(roots x rules^depth)` enumeration this module exists to avoid. The
insight that keeps the recursion's branching factor near-constant instead of scaling with the
grammar's rule count: an ordinary Prefix/Suffix rule is already correctly representable by the plain
concatenative deriv-chain/slot-chain leaf, and reaches this splice mechanism's own composite output
for free via the shared continuation an ordinary rule already attaches through after a spliced
composite. So the splice set only needs to range over the small, per-grammar set of genuinely
non-concatenative rules (Infix, structural/truncating, process-morph) — never the full rule
inventory.

## `encode_shape_variants`: the fallback path, and how it was found

The common case is every shape node still concretely identified (one token each, fast path). The
fallback handles a post-rewrite node whose identity was cleared by a feature-changing rule, or whose
own char-def no longer unifies with its current (rewritten) feature lanes — there is then no single
preferred token, so every table `Segment` char-def whose feature lanes unify with the node's current
lanes becomes a candidate, cartesian-producted across every such ambiguous position and capped.

This fallback was found because the *original*, ungated version of this function panicked on it: an
overflow guard fired on a sentinel value meant for "char table too large for the PUA token scheme,"
rather than silently mis-rendering the shape. A loud failure on an unhandled shape is what surfaced
this gap; a silent one would have produced a wrong lexc entry instead.
