# copy-agreement-featureless-redup

Synthetic CV reduplication (`redCV`: copy the first consonant and vowel, then the rest) in a grammar with
no phonological features. In such a grammar C# keeps each segment's spelling on its node, so
`AnalysisMorphologicalTransform.CanAlign` compares characters and refuses an analysis whose two copies
differ (pa, sa). PanGloss compared only feature lanes, which are empty here, so it proposed the
analysis, looked the candidate up and rejected it in synthesis. The parses were already right; the
fixture pins them (19 words, including the disagreeing-copy neighbours and a segment with two spellings)
and the wasted work is pinned by `rust/crates/pg-parse/tests/copy_agreement_featureless.rs`, because
the signature of a parse cannot show an analysis that was proposed and then failed.

Oracle: hc.dll, Machine `aab4c59819adb7b366915e09df2fb28c62b8c03a`, all 19 rows, replayed through
`machine/conformance/adapters/hc-dotnet-wrapper.sh`.
C# source: `src/SIL.Machine.Morphology.HermitCrab/MorphologicalRules/AnalysisMorphologicalTransform.cs`
`HasDisagreeingCopies` (line 65) and `CanAlign` (lines 122-165); featureless segments carry `StrRep` from
`XmlLanguageLoader.LoadCharacterDefinitionTable` (line 671, `fs = null` when the feature system is empty)
and `CharacterDefinitionTable.Add` (lines 66-75).
FieldWorks: no `fieldworks/` witness; `author` cannot express a repeated copy (see the notes in `words.yaml`).
Upstream PR: none (Rust-only, a speedup; not for #480 until the director lands it).
