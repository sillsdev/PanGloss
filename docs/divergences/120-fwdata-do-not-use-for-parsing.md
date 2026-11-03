# 120: PanGloss honours `LexEntry.DoNotUseForParsing`; FieldWorks parsers ignore it

Kind: behavioural.
Status: permanent (accepted-divergence). The project owner accepted this on 2026-10-28: a flagged ("junk") entry is never a parse unit, so a Motif Trial can leave it out of parsing. A Trial is PanGloss parsing a proposed grammar change before it is written; Apply is writing that change to the FieldWorks project. PanGloss keeps the exclusion and declares that FieldWorks does not share it.

## What differs

The LCM field `LexEntry.DoNotUseForParsing` is documented as "the parser is not to use the morphs or MSA info in its parsing". FieldWorks stores, exports and imports the flag (LIFT trait, legacy import, generated detail layout) but **no parser reads it**. A flagged entry is loaded by HermitCrab and by XAMPLE exactly as an unflagged one, so after Apply FieldWorks still parses every word the entry parses.

PanGloss's `.fwdata` import reads the flag and drops the whole flagged entry, so the same project gives fewer analyses here than in FieldWorks. In the evidence project below the word `pat` has one analysis in FieldWorks whether the flag is true or false, and one analysis in PanGloss with the flag false and none with it true.

This is a behavioural difference from FieldWorks' HermitCrab (and XAMPLE) input, not from `hc.dll` or the HermitCrab engine: the flag is not an HCML field, so no engine ever sees it.

## C# behavior

- FieldWorks `Src/LexText/ParserCore/HCLoader.cs:269-306` at FieldWorks `936ae91fe7bb2d5b9759e47eb5e95fc9bd8b388d` (origin/main, 2026-10-29): `HCLoader.Load` loops `foreach (ILexEntry entry in m_cache.LanguageProject.LexDbOA.Entries)` and loads every entry's stem and affix allomorphs. The only per-form gates are the form-validity checks (`IsValidLexEntryForm`, `IsValidRuleForm`, `HCLoader.cs:549-602`); none reads the flag. `git log -S"DoNotUseForParsing"` over `Src/LexText/ParserCore`, `ParserUI` and `Src/Common` finds nothing, so a filter was never there.
- liblcm `src/SIL.LCModel/DomainServices/M3ModelExportServices.cs:355-358` at liblcm `018befb81c428cb40e0086c0e1339db1fa46f6a3` (upstream/master): `ExportEntries` iterates `entryRepos.AllInstances()` and the exported `LexEntry` element carries no flag attribute (lines 359-377). `XAmpleParser.cs:117` feeds this export to the M3-to-XAMPLE stylesheets, which therefore could not filter on it. Senses, MSAs and allomorphs are exported from their own repositories (from line 380, `ExportMsas`).
- The field is defined at liblcm `src/SIL.LCModel/MasterLCModel.xml:2627-2630`. No parser code touches it; the other users are `LiftExporter.cs:271-272`, `LiftMerger.cs:2255-2259, 2429-2434`, `LLImportPhase3.xsl:3586-3590` and `OverridesLing_Lex.cs:1662` (copy when a sense moves to a new entry).
- Installed FieldWorks 9.3.10.1452: `ParserCore.dll`, `SIL.Machine.Morphology.HermitCrab.dll` and `XAmpleManagedWrapper.dll` contain no occurrence of the string `DoNotUseForParsing`, and the run in the evidence confirms the behaviour.
- machine (`sillsdev/machine`) has no reference to the flag, and `HermitCrabInput.dtd` has no such field.

## PanGloss behavior

At PanGloss `f4ed795c` (v0.14.0):

- `rust/crates/pg-fwdata/src/extract/lexicon.rs:34-36` reads `<DoNotUseForParsing val="..."/>` (a present but unparseable value refuses the import, `ImportError::InvalidDoNotUseForParsing`, `lib.rs:51`). A true flag (lines 37-62) drops the entry, with all its forms (`LexemeForm`, `AlternateForms`), MSAs and senses (nested senses included), and records a `not_considered` load decision with reason `doNotUseForParsing` (`pangloss import` prints the count, `pg-cli/src/main.rs:496-506`).
- `lexicon.rs:72-110` keeps an MSA or allomorph that an unflagged entry still shares, and removes references to the excluded entry or its senses from other entries' variant and complex-form `component_lexemes` (other components stay), so a variant cannot borrow an excluded analysis while a complex form with its own analysis stays parseable.
- `rust/crates/pg-fwdata/src/extract/morphology.rs:647-714` (`extract_adhoc_prohibitions`, reason recorded at line 707) drops, as whole rules, ad hoc prohibitions (`MoMorphAdhocProhib`, `MoAlloAdhocProhib`) that name an excluded MSA or allomorph, with the same reason.

Consequence: a Trial that retires an entry through the flag shows words as lost that FieldWorks keeps parsing after Apply. Motif's composer help already says a FieldWorks parse still uses the entry.

## Why a conformance-suite fixture cannot witness it

The conformance suite compares engines on `grammar.xml` (HCML). The flag is not in `grammar.xml`, FieldWorks' `HCLoader` never reads it, and the suite's `phonology-mutations.yaml` vocabulary is closed (`remove_phoneme`, `remove_all_phonemes`), so the suite cannot express a flag mutation and `hc.dll` can only ever show "same grammar, same parse". No witness in the suite or in `conformance-staging` carries `val="True"`. The evidence is therefore a run of FieldWorks' own loader and parsers on a FieldWorks-producible project: [`evidence/120/`](evidence/120/README.md).

## Coverage

- FieldWorks side: the `xample-projector` runs in the evidence (HermitCrab via `HCLoader.Load` and `Morpher.ParseWord`, XAMPLE via `XAmpleManagedWrapper`), one project with the flag false and the same project with it true.
- PanGloss side: `pangloss import` and `parse` on the same two projects (evidence), plus `pg-fwdata/tests/fixture_tests.rs` and `pg-parse/tests/do_not_parse.rs`, which pin the exclusion on synthetic projects.
- Not covered: other FieldWorks releases (only origin/main and the installed 9.3.10 were checked) and the running FieldWorks UI.

Reopen this entry if FieldWorks starts honouring the flag in `HCLoader` and the XAMPLE export; the exclusion would then match and the entry would retire. Asking upstream whether they should is a separate, optional step.
