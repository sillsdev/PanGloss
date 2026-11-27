# 120: A named `<Strata>` move keeps the item in its new stratum; C# leaves it with `Stratum == null`

Kind: behavioural.
Status: accepted-divergence. The director decided on 2026-11-03 not to reproduce the C# result. Reported upstream as [sillsdev/machine#618](https://github.com/sillsdev/machine/issues/618).

## C# behavior

FieldWorks `HCLoader.MoveMatchingItems` (`Src/LexText/ParserCore/HCLoader.cs:509-518`, FieldWorks `936ae91fe`) runs `target.Add(item); source.Remove(item);` for each item a `<Strata>` name selects (entries, morphological rules, phonological rules, affix templates). Machine `Stratum.cs:50-91` (`74ee09bf`) sets `Stratum = this` when an entry, morphological rule or template is added to a stratum and `Stratum = null` when it is removed from one. The Remove runs second, so the item is in the target collection with `Stratum == null`.

Read from the code, not run: a word whose root is a moved lexical entry throws a `NullReferenceException` in `SynthesisStratumRule.Apply` (`Morpheme.Stratum.Depth`), which FieldWorks reports as a parse error. A moved morphological rule or template is still applied in the target stratum, but the stratum's remaining-rules check and the `Stratum == stratum` checks no longer see it. Phonological rules have no `Stratum` field and move correctly.

## PanGloss behavior

`pg-grammar/src/compile/strata.rs` (`create`) moves a named item into the target stratum and rewrites the morpheme's stratum to that stratum's final position. The word parses, with the item applied in the stratum the `Strata` string put it in. Pinned by `custom_strata_move_a_named_entry_and_fold_the_predefined_names`.

## Why

Reproducing the C# result would mean reproducing a crash for named lexical entries and a silent misplacement for rules and templates, both from the order of two calls in a loader. The string's evident meaning is the placement. The predefined names (`Phonology`, `CompoundRules`, `Templates`, `Clitics`, `Morphology`) use `MoveRules` and are not affected, and PanGloss matches C# there.
