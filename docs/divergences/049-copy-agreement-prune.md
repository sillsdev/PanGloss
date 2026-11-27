# 049 — Copy-agreement pruning of reduplication matches

Kind: efficiency (result-neutral), with one deliberate step beyond C#
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
2. **Optional segments (aligned, conservative).** A copy that holds optional nodes (restored deletions) is compared with the C# `CanAlign` dynamic programme: an optional node may be passed over or absorb a node of the other copy, and a pair is compared with the same unifiability (and, in a grammar without phonological features, character identity) as the plain case. PanGloss differs from C# in one respect that matters for soundness: it puts every optional node inside some capture group and enumerates one canonical tiling per run of identical optional nodes (divergence 130), while C# can also skip an optional node between groups. A verbatim port therefore refuses analyses C# keeps (it loses `ROOT+SUFFIX|ttp` in the six `conformance-staging/edge-cases/nullable-copy-partition-*` fixtures). So each copy is first extended over the unbroken run of optional nodes next to it, which makes the verdict independent of the tiling, and the match is refused only when no alignment of the extended copies exists. Two further rules keep it sound: a non-empty copy made only of optional nodes keeps the match (C# can read it as an empty capture), and an empty copy is placed next to its right neighbour (or at the word start when no optional node exists), or the part is left undecidable and kept when that place cannot be found. Synthetic syncope repro: `salumsalm` keeps 1 split instead of 8, as C# does.

Soundness argument: the prune refuses a match only when no tiling of the optional nodes into the two copies makes them unifiable position by position, and synthesis writes both copies from one input, so such a match cannot synthesise to the word. Every other case is kept. Readings are unchanged; the reject fixtures (`nullable-copy-partition-reject-*`) pin that C#'s own prune is semantic and still refuses what it should.

## Evidence

Cebuano corpus list (live-copy grammar, work cap 20M), readings identical in every comparison below:

- The empty-copy step alone (main at 73f950c0 against the same plus the step): parser work 804,685,251 to 799,096,731 (-0.69%), 1,288 words with less work, none with more.
- Six reduplication words with the rule: 626,958 to 162,679 (-74.05%) against main at 0aab159c.
- A 4,279-word sample with the rule: 163,593,134 to 102,729,293 (-37.20%), no word with more work.
- Whole list, 44,833 words, work cap 20M, the grammar without the rule: 792,943,926 to 758,735,459 (-4.31%), 2,368 words with less work, none with more, 0 reading differences.
- `Pintahag`: identical readings, work unchanged (93,068 and 92,126 on the two grammars).

## Regression coverage

- `pg-parse/tests/copy_alignment_optional.rs`: the syncope reduplication (`salumsalm`) is pruned to the aligned split and keeps every reading (red without the change).
- `pg-parse/tests/copy_agreement_featureless.rs`: `an_empty_second_copy_is_refused`, `with_optional_second_consonant`, `a_copy_of_only_an_optional_node_keeps_the_match`, `an_optional_node_that_cannot_stand_for_its_partner_refuses_the_split`.
- `pg-parse/tests/csharp_port_affix_process.rs`: `optional_left_prefix_keeps_the_aligned_split`.
- `pg-parse/tests/stats_collector_gate.rs`: `keadilan` pinned to the pruned counts.
- `conformance-staging/edge-cases/nullable-copy-partition-*`: the six oracle fixtures that pin which capture partition of a nullable repeated copy reaches root lookup.
