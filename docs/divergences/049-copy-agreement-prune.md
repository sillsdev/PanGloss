# 049 — Copy-agreement pruning of reduplication matches

Kind: optimization (result-neutral), with one deliberate step beyond C#
Status: permanent while the empty-copy rule differs from C#; the C# fix is proposed in sillsdev/machine#634. PanGloss issue: #235.

## C# site

Machine master (`SIL.Machine.Morphology.HermitCrab/MorphologicalRules/AnalysisMorphologicalTransform.cs`): `HasDisagreeingCopies`, `TryGetSegmentCopies`, `CanAlign` (machine PR #519).

- An affix process whose output copies the same input part more than once builds one capture group per copy (`GenerateAnalysisLhs`); nothing in the matcher ties the copies together, so the only link is the acceptance predicate `HasDisagreeingCopies`.
- It compares every pair of copies of an unmodified part: equal length and per-position `FeatureStruct.IsUnifiable`, or, when a copy holds optional segments (an unapplied deletion), a DP alignment in which an optional segment may be passed over or absorb a segment of the other copy.
- An empty capture is `Range.Null`, `GroupCapture.Success` is false, and `TryGetSegmentCopies` returns false: the match is kept.

## Rust site

`pg-rules/src/morph.rs`: `copy_agreement_refuses_match`, called from `ana_allomorph_matches` for every match of an allomorph whose output repeats a part. It compares unifiability of feature lanes (and, in a grammar with no phonological features, character identity as C# does through `StrRep`), never equality. `AnalyzerConfig::prune_disagreeing_copies` (default on) switches it off.

## Difference

1. **Empty copy (this entry's step beyond C#).** An empty capture is a zero-length copy, not an undecidable one. A part copied twice with a FieldWorks zero-or-more variable (`X => X X`) otherwise gets the spurious match "first copy = whole word, second copy = empty" at every reachable point of the unapplication tree, and the whole remaining search runs again under it. C# has the same leak (machine#634). Soundness condition: synthesis writes every copy of a part from one input, so a non-empty copy cannot equal an empty one; the rule refuses only when the two copies differ in length or fail to unify, and it is skipped whenever either copy holds an optional node (below). It removes only matches synthesis would reject, so readings are unchanged.
2. **Optional segments (unchanged, still weaker than C#).** A part with an optional node in any copy stays undecidable and kept, so deletion rules still defeat the prune (synthetic syncope repro: PanGloss keeps 8 splits of `salumsalm`, C# keeps 1). Porting C#'s `CanAlign` exactly was tried on this branch and is NOT sound here: it loses `ROOT+SUFFIX|ttp` in the six `conformance-staging/edge-cases/nullable-copy-partition-*` fixtures, which C# master accepts. The cause is the capture enumeration, not the alignment: PanGloss places every optional node inside some capture group, while C# can also skip an optional node between groups, so an analysis whose only PanGloss representative is a mis-partitioned match (for example first copy `n?`, second copy `t n?`) is refused by the alignment although C# reaches the same analysis through a match PanGloss never enumerates. Closing this needs the capture enumeration over optional runs to agree with C# first (sillsdev/machine#635, PanGloss#236); the alignment prune follows it.

## Evidence

Cebuano corpus list (44,833 words, work cap 20M, the live-copy grammar), stable build 0b3fe6e27 against this change: identical status and readings for every word except one, which the stable build stopped at the cap with no reading and the fixed build completes with the four readings an uncapped run of the stable build also gives (`Pintahag`, 21,250,000 -> 133,858 work). Parser work over the list falls 16.5% (14.98% without that word); no word does more work.

## Regression coverage

- `pg-parse/tests/copy_agreement_featureless.rs`: `an_empty_second_copy_is_refused` (zero-or-more variable copied twice: unpruned proposes the empty splits, pruned keeps only the true halves; a non-reduplicated word keeps none), `an_optional_copy_node_keeps_the_match` (optional copies stay undecidable).
- `pg-parse/tests/csharp_port_affix_process.rs`: `optional_left_prefix_keeps_copy_agreement_undecidable`.
- `conformance-staging/edge-cases/nullable-copy-partition-*`: the oracle fixtures that pin which capture partition of a nullable repeated copy reaches root lookup.
