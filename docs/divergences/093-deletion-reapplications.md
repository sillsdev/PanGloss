# 093: `Morpher.DeletionReapplications` was never read, so deletion unapplication always ran one pass

Kind: behavioural.
Status: fixed-in-rust. Rust now runs C#'s `1 + DeletionReapplications` passes. The default is 0, so a project that does not set it parses exactly as before. The C# test that sets `DeletionReapplications = 1` (`RewriteRuleTests.DeletionRules`, cs:1467) was ported and its expectation is the upstream-authored one; I could not run `hc.dll` here to re-observe it (the machine checkout's NuGet local source is missing), so the ported expectation is the only oracle evidence. Rust-only gap, so no Machine issue.

## C# site

`AnalysisRewriteRule.cs::Apply` (cs:151-164), case `ReapplyType.Deletion`. Every subrule whose LHS is longer than its RHS, or whose RHS is longer than a non-empty LHS (`NarrowAnalysisRewriteRuleSpec`, simultaneous mode), is `Deletion`. The loop runs the subrule once, and while the previous pass matched it runs another, up to `1 + Morpher.DeletionReapplications` passes. Each pass sees the optional nodes the previous pass inserted. `Morpher.DeletionReapplications` (`Morpher.cs:75`) defaults to 0 and is set only by FieldWorks: `HCParser.cs::LoadParser` reads `<ParserParameters><HC><DelReapps>` with `(int) element` (default 0 when the element is missing; a non-integer throws). `XmlLanguageLoader` has no such attribute, so `<HermitCrabInput>` grammars always run with 0.

## Rust site

`pg-rules/src/rewrite.rs::reapply_deletion`, wrapping the `Kind::Narrow` arm of `analyze`, `analyze_traced`, `analyze_cached` and `analyze_cached_traced`, reading `Grammar::deletion_reapplications` (`pg-grammar-model`). The value comes from `ParserParameters::deletion_reapplications` (`pg-snapshot`), imported by `pg-fwdata/src/parser_params.rs` from `<HC><DelReapps>` and copied by `pg-grammar/src/compile/mod.rs`. A negative value behaves as 0 in C# (`j > DR` breaks after one pass) and is stored as 0; a non-integer is an `INVALID_PARSER_PARAMETER` warning that keeps 0, where C# throws. HC XML (`pg-grammar/src/load.rs`) sets 0.

## What changed

Before: one pass always. A surface form that lost two consecutive segments to the same deletion rule could not reach the underlying form under `DelReapps >= 1`. After: up to `1 + DR` passes, each continuing only while the previous one matched, as C# does. Self-opaquing and normal subrules are untouched.

## Can it change a parse?

Only for a grammar with `DelReapps > 0`, where it adds the analyses C# adds. At 0 the loop body runs once and is the previous code path.

## Coverage

`pg-parse/tests/csharp_port_rewrite.rs::deletion_rules_deletion_reapplications` ports the `DeletionRules` `DeletionReapplications = 1` reconfiguration (`bubu` gains entry 27, `buiibuii`); it fails with the loop reverted to one pass. `pg-fwdata` `del_reapps_is_read_from_the_hc_element_and_defaults_to_zero` and `malformed_del_reapps_warns_and_keeps_zero` pin the import; `pg-grammar` `deletion_reapplications_parser_parameter_reaches_the_grammar` pins the compile.

## Open

No conformance fixture: `words.yaml` and the harnesses have no Morpher-settings channel (`machine/conformance/PROTOCOL.md` documents `DeletionReapplications` as a fixed host default of 0). A fixture needs a protocol change to carry it; the DR=0 negative (`buiibuii` unreachable from `bubu`) is already pinned by `deletion_rules_multi_position_reinsertion`.