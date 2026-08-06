# pg-foma recipe_registry.rs: why partition refinement was added, and why the gate is not vacuous

Moved out of `rust/crates/pg-foma/tests/recipe_partition_refinement_gate.rs`'s module doc so the
source can carry a short pointer instead of the full argument.

## What was wrong

`recipe_registry`'s seed table declared seven families but only three `SafeTransform` values, and
`Materializer for SeededFamily` dispatches on the transform alone — family identity and each
family's own `topology` parameter (a one-value domain) were both ignored. Two families were
`Identity` (byte-identical to the baseline) and four shared one `UnionPermutation`.
`materialize_distinct` then content-address-dedups the collisions, so the reachable space was at
most {baseline, gate-permutation, union-permutation}. Measured consequence: one reference
template-less grammar yielded 3 candidates with identical state/arc counts, and a large-lexicon
grammar yielded exactly 1. The optimizer could demonstrate correctness but never a comparison,
because there was nothing distinct to compare.

## What changed, and why it is safe

Two families now use `oracle::refine_gate_partition`, which already existed and was already argued
sound, just never wired to an applicability that would select it. Refinement changes a `Gate`
node's partition cardinality rather than its order — a genuinely different axis from the two
permutations already in use. It is safe because composition distributes over union —
`(A ∪ B) .o. R == (A .o. R) ∪ (B .o. R)` — so splitting a group's entries while keeping that
group's own unchanged `Replace` node, then re-unioning, reproduces the original net.

## Why the gate needs three separate assertions, not one

- **Distinctness.** The refined plans must have different root content addresses from the
  baseline, or `materialize_distinct` dedups them and nothing was added. A count-only assertion
  would pass on a registry that merely relabelled duplicates — exactly the defect being fixed.
- **Both granularities represented.** The two refinement families must each own a surviving
  distinct plan. Counting alone is too weak: two different partition strategies can coincide on a
  fixture whose groups are small enough (e.g. every group has at most two entries), silently
  dedup-collapsing one of them while a bare count threshold is still satisfied by an unrelated
  permutation.
- **Equivalence.** Each non-baseline plan must agree with the baseline under the differential
  oracle, which runs real query words through both compiled nets' `apply_up` and compares result
  sets — this repository's established predicate for plan equality, used because two nets can
  differ in shape and still denote the same relation, so a structural comparison would prove the
  wrong thing. A distinctness-only assertion would happily accept a transform that changed the
  language.
- **Non-vacuity of the equivalence check itself.** The differential oracle reports agreement
  whenever both result sets are equal for every word, including when both are empty — which is
  what an absent net or a word that fails to encode yields. So agreement alone proves nothing
  unless it is first established, against the baseline only, that the corpus really does produce
  analyses.
