# pg-rules metathesis.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-rules/src/metathesis.rs` implementation comments so
the source can carry a one-line pointer instead of the full argument.

## `compile_switch_pattern`: capture groups and direction handling

Compiles `pattern` (anchors included; they lift to flags per `PatternBridge::compile_pattern`'s
convention), wrapping the compiled nodes at `left_idx`/`right_idx` (indices into the compiled,
anchor-excluded node sequence — see `compiled_index`) in named capture groups so
`match_candidates` can recover their matched spans after a match, mirroring
`pg_rules::rewrite::compile_env_impl`'s identical `CompileNode::Group` post-processing for
alpha-variable position recovery.

**Direction handling**, verified against `pg-fst`'s own test suite
(`crates/pg-fst/tests/fst.rs`'s "Class 5: RightToLeft traversal" guards — this module is the first
`pg-rules` caller to compile a multi-node pattern with `Direction::RightToLeft`, since
`pg_rules::rewrite`'s own LHS/RHS patterns never carry more than the rule-level direction their
synthesis side already uses, and its analysis-side environments are always compiled `LeftToRight`
regardless of the rule's own direction, sidestepping this entirely): a `Direction::RightToLeft`
FST's compiled node list is walked in traversal order, where traversal index 0 is the physically
last segment of whatever span it matches (`rtl_asymmetric_language_walks_right_to_left`). A
pattern's nodes must therefore be given to the compiler in physically-reversed order for
`RightToLeft`, or a multi-node pattern silently matches nothing (confirmed empirically while
building this module: the analysis pattern for `simple_rule`/`complex_rule` found zero matches
until this reversal was added). Anchors are similarly traversal-relative at the `Transduce` call
site (`rtl_start_anchor_binds_physical_end`/`rtl_end_anchor_binds_physical_start`): a
`RightToLeft` compile must swap which physical anchor (`PatternBridge`'s `anchor_start`/
`anchor_end`, always physical-left/physical-right regardless of direction) feeds `.anchored`'s
start/end argument.

## `compiled_index`: full-pattern-node-space vs compiled-index-space

Full-pattern-node-space index (`MetathesisRuleDef.left_switch`/`right_switch`'s own index space,
anchors included) → compiled/top-level-segment-matching index (anchors excluded — the space
`PatternBridge::compile_pattern`'s output `CompileNode` sequence uses, since `PatternNode::Anchor`
lifts to a flag rather than a node). An anchor can only ever be the very first or very last
element of `pattern.nodes` (only `initialBoundaryCondition`/`finalBoundaryCondition` produce one,
both applied outside the `<PhoneticSequence>` child loop), so a plain "how many non-anchor nodes
precede `full_idx`" count is exact.

## `build_analysis_pattern`: rebuilding the search pattern for `synthesis_reorder`

Rebuilds the search pattern analysis needs to recognize whatever `synthesis_reorder` actually
produces. Returns the rebuilt pattern plus the (now-reordered) switch nodes' own indices in that
pattern's full-node-space.

`pre` is every node strictly before the first (by original physical position) of the two switch
nodes (verbatim, original order); `post` is every node strictly after the last (verbatim). The two
switch nodes are re-added physical-position-first: whichever of `left_switch`/`right_switch` is
physically last in `pattern.nodes` goes first, physically first goes second — matching
`synthesis_reorder`'s own real behavior (physically-last-always-ends-up-first, tag-name-agnostic),
not C#'s literal tag-driven `leftGroupName`-always-first order (which happens to coincide with
this for every attested grammar, since `left_switch` always tags the physically-last node there,
but disagrees for the "reversed" tag convention `pg_grammar_gen`'s own recipe exercises).

Any node strictly between the two original switch positions is preserved, in its own slot, between
the two (now reordered) switch nodes — unless `is_boundary_node` reports it resolves to a
`CharDefKind::Boundary`, in which case it is dropped (a boundary never appears in the analysis
match sequence regardless of pattern shape, so requiring one here could never be satisfied).

## `synthesis_reorder`: why a literal port, not a "swap two blocks" shortcut

C# `SynthesisMetathesisRuleSpec.ApplyRhs`/`MoveNodesAfter` is ported literally rather than as a
"swap the two ranges as blocks" shortcut: a non-Segment node (e.g. a boundary) inside one switch's
own captured range does not itself move (C#: `if (node.Type() == HCFeatureSystem.Segment) {
Remove; AddAfter; }`), but the loop's cursor still advances past it (`cur = node`, unconditional)
— so a segment captured later in the same multi-node range anchors off that boundary's original
position, not off wherever an earlier segment of the same range ended up. This only matters for a
switch range wider than one node, which no grammar the real C# engine can load actually produces
— kept general anyway since the C# source itself is written generally and the algorithm is no
harder to get right in general form than to special-case down to width 1.

`left`/`right` are the two switch ranges as `ms.nodes` index lists, already resolved from the match
before any mutation, mirroring the C# implementation's own discipline: resolve everything to
concrete `MutNode` data up front and perform one final `Vec::splice`, rather than mutating
`ms.nodes` in place through a sequence of index-shifting operations.

`table` is the metathesis rule's own owning table (`crate::cache::owning_table_for_metathesis_rule`'s
result at the caller, never an implicit table-0 default), consulted to decide, per moved node,
whether that node's own `char_def` still means anything once re-interpreted against `table` — this
is deliberately not a blanket "is this grammar multi-table" toggle, which would silently re-encode
"table 0 is fine" as this function's own hidden default rather than actually checking.

## `synthesis_reorder`'s per-node `char_def` reset: why, and why it is safe on both single- and multi-table grammars

Every other identity-changing rewrite path (`rewrite::syn_feature`/`sim_feature`) resets a touched
node's `char_def` to `NO_CHAR_DEF` once its post-rule state can no longer be trusted to mean what
its original literal char-def said (see `syn_feature`'s own doc for the full "archiphoneme"
precedent this mirrors); this reorder must do the same rather than keeping a relocated segment's
pre-move `char_def` verbatim, because on a multi-table grammar (this rule's own owning stratum's
table can differ from wherever the segment was originally spelled) the node would go on carrying
its origin table's raw char-def index into `pg_parse::Morpher::is_match_traced`, which always
renders against the grammar's outermost stratum's table — an apples-to-oranges raw-index collision
specific to metathesis (the only rule kind that moves material without also erasing its concrete
identity).

Rather than a blanket "clear it whenever the grammar happens to be multi-table" toggle (which would
just re-encode "table 0/the origin table is fine" as this function's own hidden default in the
false branch — exactly the antipattern `owning_table` exists to remove), this checks this node's
own `char_def` directly against `table` (the rule's own table, already correctly resolved by the
caller, never a guess): valid (in bounds and its lanes still unify with the node's current lanes,
`flat_unifiable` — the identical predicate `pg_parse::surface::matching_reps_for_node`'s own
fallback path already uses) iff re-interpreting `char_def` against `table` still denotes a real,
meaning-consistent entry.

One code path, correct whether the grammar has one table or many: on a single-table grammar
`char_def` was always resolved against this same table to begin with, so the check always passes
and nothing is ever cleared there (confirmed by `pg-foma`'s own
`phase_c_metathesis.rs::metathesis_grammar_gen_recipe_confirms_the_reversed_tag_round_trip`, which
indexes a synthesized node's `char_def` straight into its single table and would panic on a
wrongly-cleared `NO_CHAR_DEF`/`u32::MAX`); on a multi-table grammar a genuinely cross-table node's
raw index either falls out of `table`'s own bounds or denotes a different, non-unifying entry
(this fixture's own deliberately-misaligned-indices design), so the check correctly detects and
clears only that staleness — and, symmetrically, a moved node whose raw index happens to still
denote the right entry in `table` (a genuine cross-table alias) keeps its real identity rather than
losing it for no reason.

Clearing (when it fires) makes `to_shape`'s plain `push_segment_with_lanes` path fall through to
the node's (default `Unrestricted`) stored `CdSet` — i.e. lane-based unification against `table`,
exactly like every other reset site; an untouched node elsewhere in the shape keeps its identity
lock, so this does not reopen the Sena zero-feature "match the whole inventory" bug the lock exists
to prevent.

## `move_nodes_after`

Walks `range` (original-local indices into `window`, in original left-to-right order) one node at
a time. A Segment-typed node is removed from wherever it currently sits in `order` and reinserted
immediately after `cur`'s current position (`None` = the position before the very start of
`order`); a non-Segment node is never moved. Either way `cur` advances to that node's identity
before the next iteration — this "advance even when not moving" detail matters for keeping later
segments in the same range anchored correctly.

Degenerate case, not reachable from either of `synthesis_reorder`'s two calls given a sane grammar
(where the switch named to end up first is not already adjacent-and-first): if `cur` itself is the
node currently being moved in `range` (step 2's `cur` can coincide with the left switch's own last
node when the two switches are adjacent with the left switch already physically first — a
self-defeating rule authoring nobody would write, since it asks the engine to move a span to
"right after itself"), `cur`'s position is looked up after removing it and is no longer found;
this falls back to appending at the current end of `order` rather than panicking. C#'s own
`ShapeNode.AddAfter` on a just-removed node is equally not a well-specified operation for this
input, so no attempt is made to reproduce a specific (undefined) C# outcome here.

## `ana_union`

C# `AnalysisMetathesisRuleSpec.ApplyRhs`: for each `(leftNode, rightNode)` pair (Segment-typed
only — zipped over the two switch ranges, so a non-Segment or length-mismatched pairing is simply
skipped, matching C#'s `if (tuple.Item1.Type() != Segment || tuple.Item2.Type() != Segment)
continue;`), union each node's `FeatureStruct` into the other's and mark both dirty.

`FeatureStruct.Union` (`FeatureStruct.cs:407-451`) keeps, per feature, the two sides' symbol-set
union where both sides have that feature, and drops (unconstrains) any feature only one side has.
This port's dense per-feature `u64` lanes represent "feature absent" identically to "feature fully
unconstrained" (`UNCONSTRAINED = u64::MAX`), so a plain bitwise OR across every lane reproduces
both halves of that rule at once: two concrete (pinned) lanes OR together into the correct widened
symbol set; a lane that is `UNCONSTRAINED` on either side stays `UNCONSTRAINED` after the OR
(matching "one side lacks the feature ⇒ result lacks it too") — this is exact, not an
approximation, given that representation (verified against
`SimpleFeatureValue.UnionImpl`/`UnionWith`'s bit-set semantics in the C# source).

Also resets both nodes' `char_def` to `NO_CHAR_DEF`, mirroring `pg_rules::rewrite::syn_feature`'s
identical, already-documented choice: after a lane-widening mutation, a node's stale literal
`char_def` identity would otherwise keep root-trie/surface lookups pinned to the pre-union
segment's own representations, unable to recognize the other (equally valid, now-unioned-in)
segment identity. C#'s `Union` has no analogous per-node identity dimension to reset (it is pure
`FeatureStruct` algebra), so this is this port's own addition to a representation gap C# does not
have — not a divergence in engine behavior.

## `is_boundary_node`

Whether `node` lowers to a `NodeKind::Boundary` shape node at segmentation time — the only
`PatternNode` kind that ever does is a literal `<Segment>`/`<BoundaryMarker>` resolving to a
`CharDefKind::Boundary` char def (a `Context` node's `SimpleContext` always names a segment
natural class). Used by `build_analysis_pattern` to decide whether a middle node between the two
switches must be dropped (a boundary, transparent to analysis matching either way) or preserved (a
real segment, required for a faithful round-trip with `synthesis_reorder`).
