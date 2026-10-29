# 111: The required-slot look-ahead tests order, edges and one-node-per-slot as well as presence

Kind: optimization (argued parse-preserving).
Status: optimized-in-rust. Machine PR #576 (open, branch `perf/hc-ordered-slot-lookahead`, commit `28cbee19`) is the C# change; it builds on entry 109.

## C# site

`RequiredSlotMaterial.cs` (`SlotWalk`, `Embeds`, `AllPresent`, `Advance`, per-allomorph `Pre`, `Post`, `Infix`, `IsOrdered`) on Machine `perf/hc-ordered-slot-lookahead`.

## Rust site

`pg-rules/src/required_slots.rs` (`summarize`, `cannot_unapply`, `advance`), `pg-rules/src/required_slots/embed.rs` (`SlotWalk`, `embeds`, `all_present`), `pg-rules/src/morph.rs` (`AnalysisLhs::inserted_material`).

## What changed

Entry 109 asked only whether each inserted segment unifies with some segment of the shape. The test
now also asks whether the inserted segments of one allomorph embed in order into distinct segment
nodes. An allomorph keeps `pre` constraints before its first copied part, `post` after its last, and
is an infix when an insert sits between copies or nothing is copied. A pure rule that copies each
part at most once and in left-hand order is ordered. While every slot walked since the shape was taken
is ordered:

- the prefix must start within `reach_left` non-optional nodes of the left edge (and the suffix within
  `reach_right` of the right), where the reach is what the slots walked so far insert on that side
  (unbounded for an infix, a compounding rule or a pattern that did not build);
- when a required ordered slot passes, the nodes its prefix and suffix used bound the window the
  slots before it must match in (`lo`, `hi`), so two slots inserting the same segment need two nodes.

A walked slot that can reorder or repeat what it copies resets the window and falls back to entry 109's
presence test for the slots before it. Optional nodes do not count against the reach because the
matcher may skip them. Each node tested still costs one `work::consume` unit.

## Why it cannot change a parse

The analysis matcher is anchored at both ends and sees only segment nodes. A pure ordered rule's
analysis keeps the shape's nodes in order, so the shape the slot sees is a subsequence of the shape it
is given, minus at most `reach` nodes at each edge for edge-anchored rules. The greedy embedding finds
the earliest embedding when one exists, so no embedding means no unapplication. The window is the
tightest range any passing allomorph allows (minimum prefix end, maximum suffix start over every
passing allomorph). Entry 109's purity condition still ends the loop at the first impure slot.

## Evidence

Shadow: 0 violated and 0 completed under a fired check on every conformance fixture
(`required_slot_shadow_gate`) and the five reference corpora; `work_steps` per corpus is in the slice
report (`result-pg-s2-template-walk.md`).

## Coverage

`pg-rules/src/stratum/required_slot_order_tests.rs`:
`a_prefix_that_is_not_at_the_left_edge_cannot_unapply` (fails with the reach ignored);
`a_node_serves_only_one_of_two_slots_that_insert_the_same_segment` (fails with the window never
narrowed); `a_slot_that_reorders_what_it_copies_leaves_only_presence_testable`;
`the_ordered_look_ahead_never_changes_an_output` (on and off agree for every template and every word
up to five segments).
