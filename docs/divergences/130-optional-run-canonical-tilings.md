# 130: An affix match enumerates one tiling per run of identical optional nodes, where hc.dll enumerates them all

Kind: efficiency (same parses, less work).
Status: permanent.
Issues: sillsdev/PanGloss#236, sillsdev/machine#635 (hc.dll enumerates every choice).

## Decision

Unapplying a deletion rule re-inserts the deleted segment as an optional node, and later unapplications see those nodes, so a shape can hold runs of identical optional nodes (2, 6, 14, 30 in a row). The analysis pattern of an affix allomorph is matched anchored at both ends with every match returned, and the traversal spawns a skip path for every optional node. A pattern such as `C V V C V V` therefore yields one match per way of picking the consumed nodes out of each run (`C(15,2) x C(31,2)` on one real word) and nearly all of them produce the same analysis.

PanGloss enumerates one match per tiling class instead, and adds a second, independent prune: under an end anchor an arc is not taken when the required (non-optional) segments still ahead exceed the longest remaining path to an accepting state. The set of analyses that survive `RemoveDuplicates`, and the analysis that survives for each duplicate class, are those of hc.dll.

## C# site

`TraversalMethodBase.Advance` (a skip instance per optional annotation, no feasibility look-ahead) and `Fst.Transduce` with `allMatches`; `AnalysisMorphologicalTransform` consumes every match.

## Scope of the two prunes

Prune B (tilings) is confined to `ana_allomorph_matches`: it is set only there, only for left-to-right traversal, and never for phonological or metathesis matching or synthesis.

Prune A (feasibility) is general: it applies to every end-anchored nondeterministic traversal, including the rewrite-rule and metathesis environment matchers (`rewrite.rs`, `metathesis.rs`), not only affix matches. It is result-neutral there for the same reason as here: it refuses only an arc from which no thread can reach an accepting state. The bound depends only on the arc's target and the next input index, so skipping the visited-set insertion for a refused instance loses nothing.

## Rust site

`pg-fst/src/traverse.rs` (`Transduce::interchangeable_runs`, the `guard` argument of `advance`, `required_suffix_counts`), `pg-fst/src/optimize.rs` (`max_hops_to_accept`), `pg-fst/src/fst.rs` (`arc_group_events`), and `pg-rules/src/morph/optional_runs.rs` (the plan built in `ana_allomorph_matches`).

## Why it cannot change a parse

Prune A (feasibility). Every arc consumes exactly one segment and only optional segments are skipped, so under an end anchor a match must still consume every non-optional segment ahead of the arc's target with distinct later arcs. If that count exceeds the longest path from the target to an accepting state (taken only when the automaton is acyclic) no thread through the arc can accept, so it yields no match.

Prune B (tilings). A node skipped after a consumed node is folded into the group that node closes, so two matches that differ only in how many run nodes are skipped between two consumed run nodes differ only in where the boundary between two neighbouring capture groups `X` (closed on that arc) and `Y` (opened on it) falls. The caller reads a match only through (1) `generate_shape`, which emits the first copy of each part, and (2) `copy_agreement_refuses_match`.
- Spans other than `X` and `Y`, and every position after the second consumed node, are unchanged.
- If `X` and `Y` are first copies of consecutive parts their spans are emitted back to back, so the emitted node sequence is the same. If neither is a first copy neither is emitted. A mixed pair is not canonicalised. Every part involved must match at least one node, so its first copy is always set.
- Both groups still contain a consumed optional node, so both are undecidable to the copy-agreement test in both matches, and that test reads no other span.
- Nodes count as one run only when every value either reader looks at is equal (kind, character definition, lanes, flags, character-definition set, unresolved-variable and generated-full feature sets), and they are adjacent in both the segment list and the shape.
- The traversal keeps the match with no skipped node inside the run, which it visits first (fewer skips first, depth first), so the first member of each duplicate class in result order is kept and `RemoveDuplicates` keeps the same analysis. Merged runs and modified parts are excluded.

## Landing order: dependency on the copy-agreement test

The argument for prune B uses `copy_agreement_refuses_match` as it is on main: a copy whose span contains an optional node is undecidable and the match is kept. The branch for sillsdev/PanGloss#235 replaces that with an alignment check, after which a copy containing run nodes is decidable and its verdict depends on span length. Then two matches that differ only in where a run straddling the X|Y boundary is split can get different verdicts (a second copy of X's part that agrees only with the longer span accepts one and refuses the other), and the canonical tiling could be refused while a pruned tiling would have been accepted.

Constraint: this change lands first; the #235 fix must then make the tiling choice and the copy alignment agree (for example by enumerating the split of a run that straddles two capture groups instead of fixing it), with a test for a run straddling the X|Y boundary. `pg-fst/tests/canonical_tilings.rs` pins the present behaviour: the kept tiling is the one with a single run node in the left group.

## Verification

Differential tests compare the pruned and the exhaustive enumeration: `rust/crates/pg-fst/tests/feasibility_prune.rs` (prune (a)), `rust/crates/pg-fst/tests/canonical_tilings.rs` (symmetry reduction (b)) and, end to end, `rust/crates/pg-parse/tests/optional_run_tilings.rs`. A stable `pangloss` and this build were also compared reading for reading on the toy reduplication grammars and the full Cebuano word list.
