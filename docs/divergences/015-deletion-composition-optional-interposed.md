# 015 — Deletion-composition loses candidates behind an interposed Optional

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`FeatureAnalysisRewriteRuleSpec.cs:48,68-71`: each target-pattern row is compiled inside its own
named `Group`, and matched material is read out per-group rather than via a positional slice of the
overall match span.

## Rust site
`pg_rules::rewrite::ana_feature` (`rust/crates/pg-rules/src/rewrite.rs`), specifically the
`compile_lane_fst_grouped` fix and its callers.

## What differs
Adding a pure-deletion rule (`rule2`, which never actually fires on the test word) to the same
stratum as a 2-segment rewrite rule (`rule1`) made the whole composition lose every candidate, even
though each rule analyzes correctly alone.

Root cause: `rule1`'s analysis target match recovered each target-pattern row's matched segment via
a **positional** `node_of[s..e]` slice of the overall match span. `pg_fst::traverse::Transduce::advance`'s
"skip the next Optional annotation" mechanism (needed so a 2-segment target can transparently pass
over `rule2`'s newly-interposed Optional "t") reports every such match as a span *wider* than the
pattern, and since no alternative exactly-2-wide match exists either (every candidate site has an
Optional immediately inside the pair), the pre-existing `width_matches` guard — written assuming a
tight duplicate always survives alongside a wide one — discarded every candidate.

## Can it change a parse?
Yes: adding an inert-on-this-word second rule to the same stratum caused a first rule's own
independently-correct analysis to disappear entirely — a composition hazard, not an isolated bug,
since the two rules interact only through shared Optional-segment machinery neither author would
suspect.

## Evidence
Fix: `ana_feature`'s target FST now compiles each target-pattern row in its own named
`CompileNode::Group` and reads each row's matched segment from that group's own tag, recovering the
correct per-row position regardless of interposed Optional segments; `width_matches` is no longer
needed at this call site. `csharp_port_rewrite.rs::multiple_segment_rules_deletion_composition_finding`
pins the fix. A direction-dependent subtlety documented at the fix site: `LeftToRight` targets must
read each row's *start* tag (freshly computed on entry), while `RightToLeft` targets must read each
row's *end* tag instead, because the compiled node order is document-reversed and
`Fst::get_offsets` swaps `(start,end)` back for that direction.

## Upstream
None, not applicable — pure Rust-side architectural gap (positional-slice recovery vs. C#'s
per-row group capture); C# was already correct.

## Notes
`resolve_bindings`/`pattern_defaults_ok` were generalized from an implicit `node_of[s+k]`
contiguity assumption to an explicit `target_nodes: &[usize]` parameter so they work for both the
old contiguous-slice callers and `ana_feature`'s new non-contiguous list — a representational
side-effect of this fix, not a behavioural one on its own.
