# 101: FieldWorks `<HC><MakePartialsFinal>` was never read, so final templates were enforced only by `--always-enforce-final-templates`

Kind: behavioural.
Status: fixed-in-rust. A project's `<HC><MakePartialsFinal>` now sets `Morpher.AlwaysEnforceFinalTemplates` as FieldWorks does. When the element is absent the value is `false`, which is what Rust always used, so a project that does not set it parses exactly as before. Rust-only gap, so no Machine issue.

## C# site

FieldWorks `Src/LexText/ParserCore/HCParser.cs::LoadParser` (FieldWorks `620cd9903`, cs:37, 168, 182, 201): `m_makePartialsFinal = makePartialsFinalElem != null ? (bool)makePartialsFinalElem : false`, assigned to `Morpher.AlwaysEnforceFinalTemplates`. The `(bool) XElement` cast accepts `true`/`false`/`1`/`0` in any case and throws otherwise, and FieldWorks then loads no parser. Machine `aab237b2` reads the property in `AnalysisAffixTemplateRule.cs:47`, `MorphologicalRules/AnalysisAffixProcessRule.cs:83`, `SynthesisAffixProcessRule.cs:67` and `SynthesisCompoundingRule.cs:65`: final templates are enforced even when `Morpher.IsPartial`.

## Rust site

`pg-fwdata/src/parser_params.rs` (`hc_parser_bool`) imports the element into `ParserParameters::make_partials_final` (`pg-snapshot`, omitted from JSON when `false`); a value other than `true`/`false`/`1`/`0` fails the import with `INVALID_PARSER_PARAMETER`, naming `HC.MakePartialsFinal` and the value. `pg-grammar/src/compile/mod.rs` copies it to `Grammar::always_enforce_final_templates`; `pg-grammar/src/load.rs` sets `false` for `<HermitCrabInput>` XML, which cannot carry it. `pg_parse::Morpher::new` starts its policy from that field; `Morpher::with_always_enforce_final_templates` overrides it. The Rust policy semantics themselves are the pre-existing port of C# `AlwaysEnforceFinalTemplates`; this entry changes only where the value comes from.

## CLI precedence

`pangloss batch --always-enforce-final-templates` (or `=true`) and `--always-enforce-final-templates=false` override the project; without the flag the project's value applies. The stats cache records the effective value, so a cache written under the project's `true` refuses a later explicit `false`. Help text (`pangloss` usage and `--describe`) says the same.

## What changed

Before: a FieldWorks project with `MakePartialsFinal` true parsed as if it were false unless the caller also passed the CLI flag; no other engine (serve, parse, Foma confirm, FFI) could honour it. After: every `Morpher` built from the compiled grammar follows the project.

## Can it change a parse?

Only for a project whose `<HC><MakePartialsFinal>` is true, where it now removes the partial-rule rescue analyses C# also removes.

## Coverage

- `pg-parse/tests/csharp_port_affix_process.rs::partial_rule` sets `Grammar::always_enforce_final_templates` on the `PartialRule` grammar with no builder call and requires the enforced identities (`sagds` and `sagstv` empty, `sagds` restored by an explicit `false`). With `Morpher::new` reverted to `false` it fails.
- `pg-cli` `project_make_partials_final_reaches_batch_and_an_explicit_flag_overrides_it`: a `.fwdata` project with `MakePartialsFinal` true records `true` in the stats cache with no flag, and an explicit `=false` is refused against that cache.
- `pg-fwdata` `make_partials_final_is_read_from_the_hc_element_and_defaults_to_false` (fails with the import reverted) and `malformed_make_partials_final_is_fatal_and_names_the_field_and_value`.
- `pg-grammar` `make_partials_final_parser_parameter_reaches_the_grammar` (fails with the compile reverted).

The enforced `PartialRule` identities are HC-Rust's existing `with_always_enforce_final_templates(true)` expectations, not observed from `hc.dll`; C#'s own `AlwaysEnforceFinalTemplates` tests (`AffixTemplateTests.cs` around cs:477 and `CliticAppliedAfterFinalTemplate`) are not ported. No conformance fixture can carry the setting until the fixture format has a Morpher-settings channel.
