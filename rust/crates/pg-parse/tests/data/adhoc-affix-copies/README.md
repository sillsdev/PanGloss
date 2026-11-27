# Ad hoc allomorph prohibitions over a multi-MSA affix entry

`project.fwdata` is the `adhoc-allomorph-copies` project with `MergeMSAs` off and a second `MoInflAffMsa` (`...0203`, Verb, same prefix slot; MSA merging would otherwise fold the two equal MSAs into one rule) on the prefix entry `ta-`. The one prefix allomorph (`...0201`) therefore compiles to two morphological rules, one per MSA, each with its own allomorph id. `tada` parses once per prefix MSA copy.

`adhoc_affix_copies.rs` adds one `MoAlloAdhocProhib` (anywhere) in each direction and expects no analysis. Before the fix the prohibition reached only the first prefix copy. `HCLoader.LoadAllomorphCoOccurrenceRules` keeps every copy of the prefix allomorph.

Provenance is Rust-only (hand-written, not produced by LibLCM; no C# loader run).
