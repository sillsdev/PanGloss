# 097: FieldWorks `<HC><MaxAlternatives>` is not read; a positive value is not enforced

Kind: behavioural.
Status: open, not implemented. A project that sets a positive `MaxAlternatives` gets no limit in PanGloss, so a word for which FieldWorks reports "MaxAlternatives exceeded" (and no analyses) gets PanGloss's ordinary analyses instead. At the FieldWorks default 0 (unlimited) there is no difference. Not wired on purpose: a counter that matches only one of C#'s count sites would be a control that looks like it acts but throws on different words than FieldWorks does.

## C# site

FieldWorks `Src/LexText/ParserCore/HCParser.cs` (FieldWorks `620cd9903`): `LoadParser` (cs:158, 170, 179-180, 200) reads `int.Parse(maxAlternativesElem.Value)` into `Morpher.MaxAlternatives` (default 0; a non-integer throws and FieldWorks loads no parser). `ParseWord` (cs:97-104) catches any exception and returns a `ParseResult` whose `ErrorMessage` is `ParserCoreStrings.ksMaxAlternativesExceeded` ("MaxAlternatives exceeded", `ProcessParseException`, cs:625-628); `ParseFiler` (cs:229-231) then sets every existing analysis to no-opinion rather than disapproving it.

Machine `aab237b2` throws `MaxAlternativesExceededException` from several independent counters whenever `MaxAlternatives > 0`:

- `Morpher.SynthesizeSequential` / `SynthesizeParallel` (cs:379-380, 412-416): one count per expanded alternative handed to synthesis, across the whole word.
- `AnalysisStratumRule.Apply` (cs:144-152): one count per morphological-rule or template output yielded inside a stratum, before the merge fold, threaded across strata by `AnalysisLanguageRule` (cs:37).
- `CombinationRuleCascade`, `LinearRuleCascade` (`SIL.Machine/Rules`) and `MemoizedCombinationRuleCascade` (cs:54, 94, 113): `output.Count` of each cascade's `HashSet<Word>` after every insertion.

## What it would take

1. Import: `int.Parse` semantics like `HC.MaxRoots` (fatal on garbage), stored as `Option<u32>` with 0 meaning off, carried on `Grammar` like entries 094, 096 and 101.
2. A per-word outcome that is neither `ok` nor `CAP`: zero analyses plus a named error, a new batch status (and `serve`/`--analyses` rows) mirroring FieldWorks' error result, never partial analyses.
3. Counters at every C# site above with the same units. The synthesis count maps onto `pg_parse::Morpher`'s `expand_alternatives` loop directly. The analysis-side counts are sizes of C#'s internal candidate sets (pre-merge yields per stratum, per-cascade `HashSet` sizes). HC-Rust is allowed to search differently there (its own dedup key, folding and memoisation; entries 023-025, 034, 045), so matching which words throw needs either a proof that each Rust count equals C#'s at that point or a restructuring that recomputes C#'s counts. Neither exists.
4. Fixtures observed from `hc.dll` at a positive value, at least one word that throws only through an analysis-side counter and one only through the synthesis counter.

## Coverage

None. `machine/conformance/PROTOCOL.md` section 8 records that every fixture runs with `MaxAlternatives` 0, and the draft `morpher:` fixture channel deliberately does not accept it.
