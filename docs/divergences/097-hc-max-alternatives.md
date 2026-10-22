# 097: FieldWorks `<HC><MaxAlternatives>` is a documented PanGloss parity exception

Kind: behavioural.
Status: accepted-divergence. The user accepted this long-term parity exception on 2026-10-22. PanGloss does not import or enforce a positive HC `MaxAlternatives` value; this is not an implementation promise.

## Accepted scope and classification

FieldWorks may stop parsing a word when one of its `MaxAlternatives` counters exceeds a positive limit. It reports the typed `MaxAlternativesExceededException` as an error and returns no analyses. PanGloss does not read this setting into `ParserParameters` or `Grammar`, and it does not produce that HC overflow status. It may return ordinary analyses for a word that triggers the HC error. That observable difference is accepted for the long term.

PanGloss's analysis-attempt cap, shared parse-work cap, and per-word timeout are separate resource-containment controls. They bound work; they do not emulate HC's setting or reproduce its count units. When one is reached, PanGloss marks the outcome capped or timed out, which means the parse is incomplete. That is not equivalent to HC's overflow error. This exception does not relax complete-result parse parity requirements.

At HC's `MaxAlternatives = 0` default, the C# counters are unlimited. PanGloss still applies its own resource controls, and any cap or timeout remains incomplete. The settings-variant fixture channel rejects `MaxAlternatives` rather than suggesting that it is supported; see [the settings contract](../conformance/parser-settings-variants.md).

## C# behavior

FieldWorks `Src/LexText/ParserCore/HCParser.cs` at FieldWorks commit `620cd9903`: `LoadParser` (cs:158, 170, 179-180, 200) reads `int.Parse(maxAlternativesElem.Value)` into `Morpher.MaxAlternatives` (default 0; a non-integer throws and FieldWorks loads no parser). `ParseWord` (cs:97-104) catches any exception and returns a `ParseResult` whose `ErrorMessage` is `ParserCoreStrings.ksMaxAlternativesExceeded` (“MaxAlternatives exceeded”, `ProcessParseException`, cs:625-628); `ParseFiler` (cs:229-231) then sets every existing analysis to no-opinion rather than disapproving it.

Machine commit `aab237b2` throws `MaxAlternativesExceededException` from several independent counters whenever `MaxAlternatives > 0`:

- `Morpher.SynthesizeSequential` / `SynthesizeParallel` (cs:379-380, 412-416): one count per expanded alternative handed to synthesis, across the whole word.
- `AnalysisStratumRule.Apply` (cs:144-152): one count per morphological-rule or template output yielded inside a stratum, before the merge fold, threaded across strata by `AnalysisLanguageRule` (cs:37).
- `CombinationRuleCascade`, `LinearRuleCascade` (`SIL.Machine/Rules`) and `MemoizedCombinationRuleCascade` (cs:54, 94, 113): `output.Count` of each cascade's `HashSet<Word>` after every insertion.

## PanGloss behavior

At PanGloss commit `e2627c9aa2ec3e4f51550ced1c0b72941c470394`, `pg-fwdata/src/parser_params.rs` reads the supported HC parameter fields but does not read `MaxAlternatives`; the `ParserParameters` model has no such value, and `pg-grammar` does not pass one to the parser. `pg-parse::Morpher` instead carries its own analysis-attempt cap, shared work cap, and per-word timeout. `ParseOutcome` exposes `capped` and `timed_out` so incomplete work remains distinguishable from a complete zero-analysis result.

The Machine settings channel rejects `MaxAlternatives` as unsupported. Existing coverage includes `pg-conformance-fixtures/src/tests.rs::a_morpher_block_the_harness_cannot_apply_is_refused`; `incomplete_replay_cannot_pass_as_an_expected_zero_analysis` verifies that exhausting a zero work budget cannot be mistaken for a completed expected rejection. Neither test claims that PanGloss reproduces the HC threshold or overflow error. There is no positive-value HC parity fixture by design.

## Historical parity investigation (not an implementation plan)

The original parity investigation proposed four steps, which are retained here as provenance rather than planned work:

1. Import with `int.Parse` semantics like `HC.MaxRoots` (fatal on garbage), store as `Option<u32>` with 0 meaning off, and carry it on `Grammar` like entries 094, 096 and 101.
2. Add a per-word outcome distinct from `ok` and `CAP`: zero analyses plus a named error, a new batch status (and `serve` / `--analyses` rows) mirroring FieldWorks' error result, never partial analyses.
3. Reproduce counters at every C# site above with the same units. The synthesis count maps onto the expansion loop in `pg_parse::Morpher`; the analysis-side counts are candidate-set sizes before merging. HC-Rust uses different search, deduplication, folding and memoization (entries 023-025, 034, 045), and no proof or recomputation establishes which words should overflow under each C# counter.
4. Observe positive-value fixtures from `hc.dll`, including a word that overflows only through an analysis-side counter and one only through the synthesis counter.

The accepted exception does not carry out these steps. No cross-engine sweep establishes threshold correspondence, and no claim is made that every alternate output is correct or harmless. No Machine issue or fix PR is proposed as part of this accepted product-scope decision.

## Coverage

The shared `MaxAlternatives` option-matrix has no positive-value fixture: the Machine harness and adapter refuse that key. The five supported settings and their observations are documented in [the settings-variant contract](../conformance/parser-settings-variants.md). This entry remains a known behavioural divergence even though the long-term parity exception is accepted.
