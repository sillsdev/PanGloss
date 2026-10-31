# Ad hoc morpheme prohibitions over a variant of a multi-MSA stem

`project.fwdata` is the `adhoc-affix-copies` project with `MergeMSAs` off and these changes: the prefix `ta-` (`...0200`) has one MSA (`...0202`, Verb); the stem `da` (`...0100`) has two stem MSAs (Verb `...0102`, Noun `...0103`); a variant entry `di` (`...0300`, no senses, no MSAs, one `LexEntryRef` whose component is `da`) borrows both of them; an enclitic stem entry `=z` (`...0400`, stem MSA `...0402`, Verb) is added with phonemes `i` and `z`.

`adhoc_morpheme_variant_copies.rs` adds `MoMorphAdhocProhib` rules (anywhere) in both directions: Verb MSA with the prefix MSA, and the enclitic MSA with each main MSA. A morpheme prohibition on the main MSA must also hold for the variant `di`, as `HCLoader.AddEntry` registers every variant entry in `m_morphemes[mainMsa]` and `LoadMorphemeCoOccurrenceRules` attaches the rule to every copy. Before the fix only the main entry `da` was covered.

Provenance is Rust-only (hand-written, not produced by LibLCM; no C# loader run).
