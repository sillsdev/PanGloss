# 099: FieldWorks `<HC><MergeMSAs>` is not read; PanGloss never merges equal MSAs

Kind: behavioural.
Status: open, not implemented. FieldWorks merges equal grammatical-info (MSA) records of one entry by default; PanGloss loads one HC entry or rule per MSA, which is FieldWorks' `MergeMSAs` false behaviour. For an entry with two equal MSAs PanGloss therefore returns two analyses where FieldWorks returns one. Projects whose entries have no duplicate MSAs are unaffected. Left unwired because it is a loader-wide change plus an output mapping, not a parser switch.

## C# site

FieldWorks `Src/LexText/ParserCore/HCParser.cs` (FieldWorks `620cd9903`): `LoadParser` (cs:167, 181, 184-192) sets `m_mergeMSAs = mergeMSAsElem != null ? (bool) mergeMSAsElem : true` and, when true, loads with `HCLoader.Load(cache, logger, out m_representativeMSAs)`; `ParseWord` passes `m_representativeMSAs` to every `ParseAnalysis` (cs:115-117), and trace selection maps through `RepresentativeMSAHvo` (cs:257-273). `Src/LexText/ParserCore/HCLoader.cs` (cs:37-46, 87-93, 675-704): `RepresentativeMSA` maps an MSA to the first MSA of the same entry and class that `EqualsMsa` it and has a sense (`entry.SenseWithMsa(msa2) != null`), never for `IMoDerivStepMsa`; `UniqueMSAs` de-duplicates through it, and it is applied to stems (`LoadLexEntries`, variants of senses), affix rules (`LoadMorphologicalRules`) and ad hoc co-prohibitions (cs:2265). `EqualsMsa` lives in liblcm (`SIL.LCModel/DomainImpl/OverridesLing_MoClasses.cs`, one override per MSA class: stem, inflectional, derivational, unclassified affix, derivation step).

## Rust site

`pg-grammar/src/compile/lexicon.rs` fans out one `LexEntryDef` per stem MSA (module doc) and one affix rule per affix MSA, with no representative mapping; `pg-fwdata` extracts every MSA.

## What it would take

1. Import `MergeMSAs` (strict boolean, default `true`).
2. Port `EqualsMsa` for each MSA class from liblcm over the snapshot's MSA records (part of speech, from/to categories, inflection classes, slots, feature structures, production restrictions, and whatever else each override compares), with a test per class.
3. Port `RepresentativeMSA` / `UniqueMSAs` into the compile at every HCLoader site (stems, variants of a sense, variants of an entry, affix rules, ad hoc prohibitions), keeping the sense-bearing-first and `IMoDerivStepMsa` rules.
4. Map output identities back as `ParseAnalysis` does, so `--analyses` rows and grammar-health stored-analysis comparison name the representative MSA consistently.
5. Decide the default question shared with entry 098: FieldWorks merges by default, so honouring it changes analysis multiplicity for affected projects.

## Coverage

None.
