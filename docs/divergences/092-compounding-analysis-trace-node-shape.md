# 092 — Compounding analysis trace nodes: PanGloss emits `CompoundingRuleAnalysis` with the subrule index where C# master emits `MorphologicalRuleAnalysis`

Kind: diagnostic (no parse divergence; trace shape only)
Status: open. Retire this entry for the failure shape if and when sillsdev/machine#534 merges (below).

## Sites

- C# site: `SIL.Machine.Morphology.HermitCrab/MorphologicalRules/AnalysisCompoundingRule.cs::Apply`
  and `TraceManager.cs` (sillsdev/machine `master`, 8983141f).
- Rust site: `pg-rules/src/morph.rs::ana_compound_traced` (the `compounding_rule_unapplied` call at
  line 735 and the per-subrule `compounding_rule_not_unapplied` call at line 741);
  `pg-rules/src/trace.rs` `TreeTraceSink::compounding_rule_not_unapplied` and
  `compounding_rule_unapplied` (lines 1200-1230).

## Difference

No parse path changes; this is only which trace node a compounding analysis attempt becomes.

For each subrule `i` of a compounding rule during analysis:

| Event | C# master | PanGloss |
|---|---|---|
| Subrule produced no output | `MorphologicalRuleNotUnapplied(rule, i, input)` (`AnalysisCompoundingRule.cs:161-162`) becomes a `MorphologicalRuleAnalysis` node (`TraceManager.cs:84-88`) | `CompoundingRuleAnalysis` node with `subrule = i` and `failureReason = Pattern` (`morph.rs:741`, `trace.rs:1200-1215`) |
| Subrule produced an output | `MorphologicalRuleUnapplied(rule, i, input, output)` (`AnalysisCompoundingRule.cs:156`) becomes a `MorphologicalRuleAnalysis` node (`TraceManager.cs:72-82`) | `CompoundingRuleAnalysis` node with `subrule = i` and the output (`morph.rs:735`, `trace.rs:1217-1230`) |
| A rule-level gate rejects the rule before any subrule | none: the `Apply` preamble returns an empty list silently (`AnalysisCompoundingRule.cs:51-58`) | `CompoundingRuleAnalysis` node, no subrule, `failureReason = HeadRequiredSyntacticFeatureStruct` (`morph.rs:694-700`) |
| The non-head root fails `NonHeadProdRestrictionsMprFeatures` | `CompoundingRuleNotUnapplied(rule, -1, ...)` becomes a `CompoundingRuleAnalysis` node (`AnalysisCompoundingRule.cs:88-99`, `TraceManager.cs:91-107`) | same node type; see [the gap check below](#non-head-productivity-rejection) |

So in C# master only the productivity rejection is typed `CompoundingRuleAnalysis`; PanGloss types
every compounding analysis attempt that way, and a per-subrule failure carries the subrule index
`i`, which C# master gives to a `MorphologicalRuleAnalysis` node instead. A consumer that finds
compounding attempts by `type == "CompoundingRuleAnalysis"` sees all of them in PanGloss and only
the productivity rejections in C# master.

## The subrule index of a rule-level gate

C# stores "no subrule" as `-1` (`AnalysisCompoundingRule.cs:95`; `TraceManager` keeps it).
PanGloss stores it as `None` (`trace.rs:1213` and `1229`: `(subrule >= 0).then_some(subrule)`), and
the JSON and text renderers omit it (`pg-cli/src/trace_render.rs:87` and `133-135`). The two are
equivalent; only the encoding differs. See `docs/formats/trace-format.md`.

## Upstream

sillsdev/machine#534 ("Trace HermitCrab failure reasons and template slot outcomes", open, not
merged, deferred from the October 2026 release) has the failed compounding subrule emit
`CompoundingRuleAnalysis` with the subrule index `i` (`CompoundingRuleRejected`,
`AnalysisCompoundingRule.cs:197-198` on `fix/hc-trace-reasons`), which is PanGloss's shape. Its
successful-subrule path still calls `MorphologicalRuleUnapplied` (cs:183-184), so the success
shape would remain different after #534. When #534 lands, retire the failure half of this entry
(`Status: accepted-upstream`) and keep the success half open or fold it into a new entry.

## Can it change a parse?

No. Trace sinks observe; `parse_word_traced` is pinned to produce the same outcome as the plain
parse (`pg-parse/tests/trace_gate.rs`).

## Coverage

`pg-cli/src/rich_trace/tests.rs::compound_analysis_attempts_keep_owner_reason_and_subrule` pins
PanGloss's shape (`CompoundingRuleAnalysis` nodes with a failure reason and a `subrule`). No test
compares against a C# trace.
