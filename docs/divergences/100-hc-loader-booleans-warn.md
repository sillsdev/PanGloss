# 100: A malformed `NotOnClitics`, `NoDefaultCompounding` or `AcceptUnspecifiedGraphemes` is a warning in PanGloss but loads no parser in FieldWorks

Kind: diagnostic.
Status: open, found while porting entries 094, 096 and 101; not changed.

## C# site

FieldWorks `Src/LexText/ParserCore/HCLoader.cs` constructor (FieldWorks `620cd9903`, cs:106-109) reads the three with `(bool?) hcElem.Element(...)`. That cast throws `FormatException` on anything but `true`/`false`/`1`/`0` (case-insensitive), so FieldWorks loads no HermitCrab parser for such a project.

## Rust site

`pg-fwdata/src/parser_params.rs` reads them through `hc_bool`, which records an `INVALID_PARSER_PARAMETER` warning and keeps the FieldWorks default. The settings `HCParser.LoadParser` reads (entries 093, 094, 096 and 101: `DelReapps`, `MaxRoots`, `MakePartialsFinal`, `MergeAnalyses`) instead fail the import, naming the field and the value.

## Can it change a parse?

Only for a project FieldWorks itself cannot parse at all: PanGloss parses it with the default in place of the malformed value. Making these fatal like entries 093, 094, 096 and 101 is a one-function change (`hc_parser_bool`) plus updating `pg-fwdata` tests that pin the warning; it is left for a decision because it turns an import that works today into a refusal.
