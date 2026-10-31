# Ad hoc allomorph prohibitions over a multi-MSA stem

`project.fwdata` is a hand-written synthetic FieldWorks-shaped project (derived from the `pg-fwdata` fixture, not produced by LibLCM). One stem entry `da` (allomorph `...0101`) has two `MoStemMsa` rows, Verb first and Noun second, so it compiles to two grammar entries with two allomorph ids. One prefix entry `ta-` (allomorph `...0201`) is a `MoInflAffMsa` accepted only on Verb, in the Verb template's optional prefix slot.

`adhoc_allomorph_copies.rs` adds one `MoAlloAdhocProhib` (anywhere) in each direction and parses `tada`. Expected: one analysis without a prohibition, none with either direction. Before the fix both directions still parsed, because the prohibition reached only the last (Noun) copy while the parse used the Verb copy. `HCLoader.LoadAllomorphCoOccurrenceRules` attaches the rule to every copy (and the cross product of copies on the `rest` side).

Provenance is Rust-only for the end-to-end result: LibLCM did not produce this project and the C# FieldWorks loader has not read it. The HC evidence is an `hc.dll` replay of the equivalent XML (rule on the last copy only: `tada` parses; rule on every copy, as HCLoader emits: no parse).
