# 132: hc.dll pairs a metathesis switch with an optional segment a deletion restored; PanGloss pairs only the switches

Kind: behavioural (hc.dll has a C# defect, sillsdev/machine#649, the mechanism of sillsdev/machine#520; PanGloss's pairing parses the 312 words hc.dll generates but cannot parse, though it still misses a few that need a skipped segment to travel, see Known gap and #392).
Status: temporary-permanent: remove when the Machine fix lands and the oracle pin moves. Then delete this entry, the `forward-synthesis` marker in the staged fixture and its `oracle-conformance-known-divergences.json` row. Upstream: sillsdev/machine#649 and the suite fixture in sillsdev/fieldworks-parser-conformance#50, which records the four words as `expect_fail`; once pinned, the `pangloss_expectations` rows in `oracle-conformance-known-divergences.json` excuse PanGloss's parses. PanGloss #381 tracks it. Retired entry 050 (part-of-speech-scoped deletion, sillsdev/machine#520) is the same mechanism.

## Decision

A deletion's analysis runs before a metathesis rule's analysis and restores the deleted segment as an optional node at every gap, including between or beside the two switches. When the metathesis matcher steps over that optional node, hc.dll adds it to the capture of the switch matched just before it and then pairs the two captures node by node. The optional node takes a switch's place, the swap is never undone, and no lexical entry matches.

hc.dll therefore cannot parse words it generates itself. Grammar q3f (prefix `aps+`, `s t -> t s`, `p -> 0`): `Morpher.GenerateWords(R0, [PRE])` at Machine `1ddef2bd` returns `ats`, and `ParseWord("ats")` and `ParseWord("at+s")` return nothing. Over the 2,241 review probes C# loads, PanGloss's pairing gives 432 analyses of 312 words in 51 grammars that hc.dll lacks, and C#'s `GenerateWords` produces the word from every one of them (58 with the affixes in another order).

Owner decision 2026-11-20 (PanGloss #374, round 7): this is a C# bug; PanGloss must not copy it, as for 050 and 127. PanGloss pairs only the nodes the switches consumed. Until the fix lands PanGloss gives these parses and hc.dll does not.

Known gap: in one review grammar (a51-226: prefix `pp+`, `s a` metathesis, then `p -> 0`) the skipped optional `p` is the segment that has to travel, so hc.dll's pairing finds `PR1+R2|asa` and `PR1+R0|astt` and PanGloss's does not. PanGloss gives `R2|asa` and `R0|astt` (and their `ZE0` forms), as v0.27.0 did, which hc.dll misses. Both engines generate every one of these, so each under-parses; finding both needs a search over where skipped optional nodes go. PanGloss #392 tracks it.

## C# site

`TraversalMethodBase.Advance` (re-runs the consuming arc's commands for each skipped optional annotation, so the skipped node joins the preceding group's capture); `AnalysisMetathesisRuleSpec.ApplyRhs` (zips `GetNodes(leftGroup.Range)` with `GetNodes(rightGroup.Range)` and unions each pair).

## Rust site

`pg-rules/src/metathesis.rs`, `analyze_with_pattern` (a candidate that skipped optional nodes keeps only the non-optional nodes of each switch group before the union).

## Verification

`conformance-staging/edge-cases/metathesis-deletion-restores-segment-between-switches` (forward-synthesis rows `ats`, `at+s`, `atsa`, `at+sa`; exact hc.dll mismatch in `rust/tools/oracle-conformance-known-divergences.json`) and `conformance-staging/edge-cases/pos-scoped-deletion-blocks-metathesis` (050). Unit pin `pg-parse/tests/metathesis_match_order.rs::a_skipped_optional_node_is_not_paired_with_a_switch`.
