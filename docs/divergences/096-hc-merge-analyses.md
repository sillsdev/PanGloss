# 096: FieldWorks `<HC><MergeAnalyses>` was never read, so analyses were always merged

Kind: behavioural.
Status: fixed-in-rust. A project's `<HC><MergeAnalyses>` now sets `Morpher.MergeEquivalentAnalyses` as FieldWorks does. When the element is absent the value is `true`, which is what Rust always used outside tracing, so a project that does not set it parses exactly as before. Rust-only gap, so no Machine issue.

## C# site

FieldWorks `Src/LexText/ParserCore/HCParser.cs::LoadParser` (FieldWorks `620cd9903`, cs:35, 63, 166, 175-176, 199): `m_mergeAnalyses` starts `true` in the constructor and becomes `(bool) mergeAnalysesElem` when the element is present; it is assigned to `Morpher.MergeEquivalentAnalyses`. The `(bool) XElement` cast accepts `true`/`false`/`1`/`0` in any case and throws otherwise, and FieldWorks then loads no parser. Machine `aab237b2` reads the property only in `AnalysisStratumRule.Apply` (cs:132): `mergeEquivalentAnalyses = _morpher.MergeEquivalentAnalyses && !_morpher.TraceManager.IsTracing`.

## Rust site

`pg-fwdata/src/parser_params.rs` (`hc_parser_bool`) imports the element into `ParserParameters::merge_analyses` (`pg-snapshot`, default `true`, omitted from JSON when `true`; a snapshot written before the field existed reads as `true`). A malformed value fails the import with `INVALID_PARSER_PARAMETER`, naming `HC.MergeAnalyses` and the value. `pg-grammar/src/compile/mod.rs` copies it to `Grammar::merge_equivalent_analyses`; `pg-grammar/src/load.rs` sets `true` for `<HermitCrabInput>` XML. `pg_parse::Morpher::new` starts from that field, `Morpher::with_merge_equivalent_analyses` overrides it, and `AnalyzerConfig::merge_equivalent` is `merge_equivalent_analyses && !trace.is_tracing()`, the C# condition. There is no CLI flag.

## Can it change a parse?

Not by design: merged analyses are expanded again before lexical lookup (C# `Word.ExpandAlternatives`), so `MergeAnalyses` is a search-cost setting. With it off, Rust's stratum output dedups on `WordKey`, which is at least as fine as C# `Word.ValueEquals` (both exclude the syntactic feature structure; Rust also keeps the source morph trail), so turning merging off cannot drop an analysis C# keeps. The unmerged path was already exercised by every traced parse.

## Coverage

- `pg-parse/tests/merged_analyses_fs_generalization.rs::merge_analyses_off_from_the_grammar_keeps_the_analyses_and_stops_folding`: a prefix and a suffix that commute on one unordered stratum, so analysis reaches the root along two merged paths. With `Grammar::merge_equivalent_analyses = false` the analysis multiset is unchanged and the parse costs strictly more work units; the builder restores the merged cost. With `Morpher::new` reverted to ignore the setting it fails.
- `pg-fwdata` `merge_analyses_is_read_from_the_hc_element_and_defaults_to_true` (fails with the import reverted) and `malformed_merge_analyses_is_fatal_and_names_the_field_and_value`.
- `pg-grammar` `merge_analyses_parser_parameter_reaches_the_grammar` (fails with the compile reverted).
- `pg-snapshot` `morpher_settings_are_omitted_at_their_defaults_and_round_trip_when_set`, `a_snapshot_without_merge_analyses_reads_the_fieldworks_default_true`.

The parse-preservation claim is HC-Rust's own observation on one synthetic grammar, not an `hc.dll` comparison; no conformance fixture can carry the setting until the fixture format has a Morpher-settings channel.
