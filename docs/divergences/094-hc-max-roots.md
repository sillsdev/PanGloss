# 094: FieldWorks `<HC><MaxRoots>` was never read, so every project compounded at most two stems

Kind: behavioural.
Status: fixed-in-rust. A project's `<HC><MaxRoots>` now sets `Morpher.MaxStemCount` as FieldWorks does. When the element is absent the value is C#'s constructor default 2, which is what Rust always used, so a project that does not set it parses exactly as before. Rust-only gap, so no Machine issue.

## C# site

FieldWorks `Src/LexText/ParserCore/HCParser.cs::LoadParser` (FieldWorks `620cd9903`, cs:157, 169, 177-178, 198): `maxStemCount` starts at 2 and becomes `int.Parse(maxRootsElem.Value)` when `<ParserParameters><HC><MaxRoots>` is present, then is assigned to `Morpher.MaxStemCount`. `int.Parse` throws on a non-integer or an out-of-range value, and FieldWorks then loads no parser. The only reader in Machine (`aab237b2`) is `AnalysisCompoundingRule.Apply` (`MorphologicalRules/AnalysisCompoundingRule.cs:45`), which refuses to unapply once `NonHeadCount + 1 >= MaxStemCount`; any value below 1 therefore behaves exactly like 1. The FieldWorks parameters dialog clamps the value to 1-10 (`ParserParametersDlg.cs::ValidateValues`) but `LoadParser` does not. `<XAmple><MaxRoots>` is a different, XAmple-only setting.

## Rust site

`pg-fwdata/src/parser_params.rs::hc_max_roots` (via `hc_int`) imports the element into `ParserParameters::max_roots` (`pg-snapshot`, `Option<u32>`, omitted from JSON when absent); a value below 1 is stored as 1, and a non-integer or out-of-range value fails the import with `INVALID_PARSER_PARAMETER`, naming `HC.MaxRoots` and the value. `pg-grammar/src/compile/mod.rs` copies it to `Grammar::max_stem_count` (2 when absent); `pg-grammar/src/load.rs` sets 2 for `<HermitCrabInput>` XML, which cannot carry it. `pg_parse::Morpher::new` starts its cap from `Grammar::max_stem_count`; `Morpher::with_max_stem_count` (entry 022) still overrides it. Because `Morpher::new` reads it, every engine that builds a `Morpher` from the compiled grammar (batch, parse, serve, the Foma confirm stage, FFI, grammar health) follows the project.

## What changed

Before: every FieldWorks project ran with 2 regardless of its setting, so a project with `MaxRoots` 3 or more lost every analysis with three or more compounded stems, and a project with `MaxRoots` 1 gained the two-stem compounds FieldWorks refuses. After: the cap is the project's.

## Can it change a parse?

Only for a project whose `<HC><MaxRoots>` is present and not 2, where it now finds exactly the compound depth C# finds. The Foma proposer's compounding unroll is bounded by rule counts, not by this cap, and its containment against a raised cap is pinned by `pg-foma-backend/tests/cover_compounding_recursive_depth_bound.rs`.

## Coverage

- `pg-parse/tests/csharp_port_compounding.rs::simple_rules_4_three_root_compound_from_grammar_max_stem_count` ports the `SimpleRules` `MaxStemCount = 3` reconfiguration (cs:76-90) through `Grammar::max_stem_count` with no builder call. With `Morpher::new` reverted to a fixed 2 it fails (`got {}`, want `{"5 8 41", "5 9 41"}`).
- `pg-fwdata` `hc_max_roots_is_read_from_the_hc_element_apart_from_xample_max_roots` (fails with the import reverted to `None`) and `malformed_hc_max_roots_is_fatal_and_names_the_field_and_value`.
- `pg-grammar` `max_roots_parser_parameter_reaches_the_grammar_and_defaults_to_two` (fails with the compile reverted to a fixed 2).
- `pg-snapshot` `morpher_settings_are_omitted_at_their_defaults_and_round_trip_when_set`.

The expectation is C#'s own `SimpleRules` test, ported unchanged; no conformance fixture can carry the setting until the fixture format has a Morpher-settings channel.
