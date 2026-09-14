# 012 — RTL analysis direction inversion for multi-node targets

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`PatternNode.GenerateNfa`, which enumerates children in `fsa.Direction` order — so a `RightToLeft`
matcher matches the same physical substring an LTR matcher would, just traversed backward.

## Rust site
`pg_rules::rewrite::compile_lane_fst` (`rust/crates/pg-rules/src/rewrite.rs`).

## What differs
For a rule whose RHS is 2+ nodes, `compile_lane_fst` compiled multi-node analysis targets in
**document order** for a `RightToLeft` traversal. Under `pg_fst`'s "nodes follow traversal order"
convention, that matched the physically **reversed** sequence instead of the intended one, so
`ana_epenthesis` never marked the target nodes optional and no candidate root ever reached
synthesis-confirm.

## Can it change a parse?
Yes: any `RightToLeft` rule with a multi-node epenthesis target silently lost analysis-side recovery
of the epenthesized material, invisible on any reference grammar's single-node analysis targets
(where document order and reversed order coincide).

## Evidence
Fixed by reordering document→traversal inside `compile_lane_fst`. Unit gate:
`pg-rules/tests/rewrite_gate.rs::epenthesis_analysis_multi_node_target_matches_document_order`.

## Upstream
None, not applicable — pure Rust-side bug; C#'s `GenerateNfa` already orders by traversal direction.

## Notes
Paired with entry 011 under the same C# test group (`boundary_rules`, sub-cases (5)/(6), MPR-gated
epenthesis) — both had to be fixed before that sub-case passed, since each masked the other's
symptom (the test returned empty either way until both were fixed).
