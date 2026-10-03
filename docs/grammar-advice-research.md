# Research: FieldWorks advice for PanGloss grammar findings

Researched before code changes, 2026-10-03, baseline `755d1157`.

The catalog contains **88 registered kinds: six grammar-health checks and 82 importer/compiler codes**. All already have explanation and guidance at this baseline. This lane verifies that existing coverage, strengthens field-specific guidance, and records the limits below. The requested `report-sl-d3-w.md` is absent; `report-ln-d3-w.md` confirms Motif retains explanation and guidance from structured output. No finding code or JSON shape needs changing. There is no CONTRIBUTING file in this checkout.

## Evidence and method

Roots: `FW` = `/mnt/c/Users/johnm/Documents/repos/FieldWorks`; `LCM` = `/mnt/c/Users/johnm/Documents/repos/liblcm`; `HELP` = `/mnt/c/Users/johnm/Documents/repos/FwHelps`; `PG` = this PanGloss repository. Paths and line numbers below refer to those checkouts. UI paths denote area > tool > field unless a menu is explicitly named. The help below is the existing Markdown export of the conceptual introduction PDF under FwHelps; no CHM extraction or Windows interop was used. FieldWorks UI XML is the authority for current labels where older help differs. Importer/compiler-only findings are established by their PanGloss producers; FieldWorks source corroborates the underlying source model but does not emit every PanGloss kind.

- **LEX**: FW `DistFiles/Language Explorer/Configuration/Lexicon/Edit/toolConfiguration.xml:6` (Lexicon Edit); `Configuration/Parts/LexEntry.fwlayout:7,38,43` (Lexeme Form, Allomorphs); `Configuration/Parts/LexSenseParts.xml:550` (Grammatical Info.); `Configuration/Parts/MorphologyParts.xml:220,254,281,432,465,490,509,536,557,676,708` (Form, Environments, Morph Type, Required Features, Stem Allomorph Label, Inflection Classes, Category, Slots). The `Configuration/` paths here and below are under FW `DistFiles/Language Explorer/`.
- **PHON**: FW `Configuration/Grammar/Edit/toolConfiguration.xml:74,90,113`; `Configuration/Parts/Morphology.fwlayout:545` (In Orthography as); `Configuration/Parts/MorphologyParts.xml:2689` (Phonological Features). HELP `tools/out/pdf/WW-ConceptualIntro/ConceptualIntroFLEx.md:2707-2765` (insert graphemes and use the feature chooser). LCM `src/SIL.LCModel/MasterLCModel.xml:4326-4370` (Codes, Features). FW `Src/LexText/ParserCore/HCLoader.cs:2668-2710` (load representations, reject empty and duplicate graphemes, generated null marker).
- **NC**: FW `Configuration/Grammar/Edit/toolConfiguration.xml:191`; `Configuration/Parts/MorphologyParts.xml:2728,2742` (Phonemes and Phonological Features choosers). HELP conceptual introduction `:2797-2823`. LCM model `:4386-4415` (Features, Segments). HCLoader `:2788-2829` (segment- and feature-defined natural classes).
- **ENV**: FW `Configuration/Grammar/Edit/toolConfiguration.xml:225`; `Configuration/Parts/MorphologyParts.xml:254,2764` (allomorph environment selection and String Representation). HELP conceptual introduction `:2829-2895` (/, _, brackets, #, and editing locations). LCM model `:4425-4446` (StringRepresentation); `:2989-3025,3661-3685` (PhoneEnv, Position). HCLoader `:2268` (environment parser), `:1273-1330` (circumfix environments).
- **TEMPLATE**: FW `Configuration/Grammar/Edit/toolConfiguration.xml:6`; `Configuration/Parts/Morphology.fwlayout:162-170` (Affix Templates, Affix Slots); `Configuration/Parts/MorphologyParts.xml:552-565` (Category, Slots, Inflection Features). HELP conceptual introduction `:933-977` (Insert Slot before/after Stem, Add inflectional affix(es)). LCM model `:3310-3450` (template slot references, Disabled, analysis Slots). HCLoader `:976-1024,1673-1740` (partial rules, slot loading).
- **STEMLABEL**: FW `Configuration/Parts/Morphology.fwlayout:186,219-231` (Stem Allomorph Labels, Feature Sets, Feature Set); `Configuration/Parts/MorphologyParts.xml:184,490` (From Stem Allomorph Label, allomorph Stem Allomorph Label). HELP conceptual introduction `:3215-3270`. LCM model `:3185,3675,3687-3710`; HCLoader `:204-219,832,961`.
- **MSA**: LEX UI plus FW `Configuration/Parts/MorphologyParts.xml:154-195,568-571,676-708` (From/To Inflection Class and Exception "Features"). HELP conceptual introduction `:1152-1167,2190-2205`. LCM model `:2528-2568,3133-3205,3421-3455`; HCLoader `:691-710,926-1024`.
- **COMPOUND**: FW `Configuration/Grammar/Edit/toolConfiguration.xml:38`; `Configuration/Parts/MorphologyParts.xml:2162,2238` (exception restrictions). HELP conceptual introduction `:2240-2490` (compounds). LCM model `:3073-3132,3245-3275,4602-4620` (compound rule restrictions, LeftMsa, RightMsa, OverridingMsa, ToMsa); HCLoader `:1842-1950`.
- **PROCESS**: LEX plus FW `Configuration/Parts/Morphology.fwlayout:98,139` (Affix Process Rule). HELP conceptual introduction `:3770-3970` (processes and their input/output parts). LCM model `:3049-3070` (Input, Output); HCLoader `:1334-1440` (input/output mappings).
- **CIRCUMFIX**: LEX plus HELP conceptual introduction `:3608-3652` (Lexeme Form has circumfix morph type; prefix and suffix allomorphs are required). LCM model `:2637-2642,3282-3298`; HCLoader `:519-532,1048-1073`.
- **VARIANT**: FW `Configuration/Lists/Edit/toolConfiguration.xml:353` (Variant Types); `Configuration/Parts/LexEntryParts.xml:1135,1163` (Variant Type, Variant of); `Configuration/Parts/MorphologyParts.xml:565` (Inflection Features). HELP conceptual introduction `:3334-3356` (irregularly inflected types in Lists > Variant Types). LCM model `:5176-5205` (InflFeats, Slots); HCLoader `:733-800,1771-1800` (variant analysis and generated null affix).
- **RULE**: FW `Configuration/Grammar/Edit/toolConfiguration.xml:258`; `Configuration/Parts/Morphology.fwlayout:581,617` (Rule Formula); `Configuration/Parts/MorphologyParts.xml:2905-2955` (Required/Excluded Properties). HELP conceptual introduction `:4350-4365`; LCM model `:5037-5112` (StrucDesc, RightHandSides, StrucChange); HCLoader `:2033-2160`.
- **ADHOC**: FW `Configuration/Grammar/Edit/toolConfiguration.xml:292`; `Configuration/Parts/MorphologyParts.xml:2318,2330,2510,2522` (Key/Other Allomorph(s), Key/Other Morpheme(s)). HELP conceptual introduction `:2558-2590,3448-3475`. LCM model `:4540-4578`; HCLoader `:2163-2255`.
- **SETTINGS**: FW `Configuration/Words/areaConfiguration.xml:9,26-30,277-305` (Words area's **Parser** menu, Choose Parser, Edit Parser Parameters...); `Configuration/Main.xml:126-129` (writing systems and Restore a Project...). LCM model `:3537,5249-5267,5303` (ParserParameters, writing systems); FW `Src/LexText/ParserCore/ParserWorker.cs:63` (ActiveParser); HCLoader `:92-104` (settings and strata). HELP conceptual introduction `:3550-3558` confirms the Parser menu; HELP `tools/out/pdf/FieldWorks_Writing_Systems.md:299-303` explains writing-system specification files; current editing paths come from the UI XML.
- **INTEGRITY**: LCM `src/SIL.LCModel/MasterLCModel.xml:5233-5359` (project roots and references); PG `rust/crates/pg-fwdata/src/extract/codes.rs:15`, `rust/crates/pg-snapshot/src/validate.rs:43`, `rust/crates/pg-grammar/src/compile/issue_codes.rs:5-90`, and `rust/crates/pg-grammar/src/compile/issues.rs:14` (source/snapshot/compiler findings). These do not establish a universal editable FLEx field. No speculative menu path is proposed.

Existing technical titles are not evidence of an editable UI field. In particular boundary Codes exist in the model but that alone does not prove a boundary editor exists. Unsupported accurate data should be preserved and reported, not changed to silence a parser limitation. A known inspection location is distinguished below from a guaranteed correction.

## Complete inventory and proposed action

Each entry names the actual trigger/behavior from the catalog, the confirmed editing location and conditional fix, or an explicit limit. Sources include the owning PanGloss catalog line and the evidence keys above (each key expands to file paths and lines).

### `hc-undeclared-segment`

Cause: A loaded form or rule inserts a segment absent from the declared inventory.

Location and action: Lexicon > Lexicon Edit > Form: correct unintended spelling. For intended phonemes, Grammar > Phonemes > In Orthography as: insert their graphemes. For compound output, inspect Grammar > Compound Rules.

Sources: PG `rust/crates/pg-grammar/src/grammar_health.rs:101`; LEX, PHON, COMPOUND.

### `hc-duplicate-feature-bundle`

Cause: Declared phonemes have identical feature values while a phonological feature system exists.

Location and action: Grammar > Phonemes > Phonological Features: use the chooser to correct missing or unintended values when the sounds must be distinguished. Preserve intentional identical bundles and report a parser limitation if the distinction is needed.

Sources: PG `rust/crates/pg-grammar/src/grammar_health.rs:107`; PHON.

### `hc-stem-no-grammatical-category`

Cause: The stem analysis has no Category and is marked partial.

Location and action: Lexicon > Lexicon Edit > Grammatical Info. > Category: select the intended stem category.

Sources: PG `rust/crates/pg-grammar/src/grammar_health.rs:113`; LEX, MSA.

### `hc-inflectional-affix-missing-template-slot`

Cause: The inflectional analysis has no Slots and is marked partial.

Location and action: Grammar > Category Edit > Affix Templates: right-click the intended slot and add the existing inflectional affix. Check Lexicon > Lexicon Edit > Grammatical Info. > Category and Slots.

Sources: PG `rust/crates/pg-grammar/src/grammar_health.rs:119`; TEMPLATE, MSA.

### `hc-unclassified-affix`

Cause: The affix analysis is unclassified and is marked partial.

Location and action: Lexicon > Lexicon Edit > Grammatical Info.: choose the intended inflectional or derivational analysis, then supply that analysis's category and restrictions.

Sources: PG `rust/crates/pg-grammar/src/grammar_health.rs:125`; LEX, MSA.

### `hc-partial-reason-unspecified`

Cause: A morpheme is marked partial without a recorded cause.

Location and action: No verified FieldWorks correction: the kind does not identify a missing field. Keep the analysis and report the finding if it is complete.

Sources: PG `rust/crates/pg-grammar/src/grammar_health.rs:131`; LEX, MSA.

### `fwdata.dangling-reference`

Cause: An item refers to a target that is absent from the imported project data, so that reference cannot be followed.

Correction status: **no verified grammar correction for this kind**. The code covers references on many classes; no single FieldWorks owner, tool, or editable field is determined by the kind alone. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Use the finding description to identify the affected item and field. No single FieldWorks correction is known for this kind. If the project opens and shows the intended data, report the finding and PanGloss version to PanGloss; ask FieldWorks support for help if the project data is damaged.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:90`; INTEGRITY.

### `fwdata.unexpected-class`

Cause: A reference resolves to an item class that the field being imported does not accept.

Correction status: **no verified grammar correction for this kind**. The code covers many fields and may describe a valid object the importer mishandles; no universal grammar edit is established. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Use the finding description to identify the affected item and field. No single FieldWorks correction is known for this kind. If the project opens and shows the intended data, report the finding and PanGloss version to PanGloss; ask FieldWorks support for help if the project data is damaged.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:97`; INTEGRITY.

### `fwdata.missing-required-field`

Cause: A required source value was absent at the owner and field identified in the warning.

Correction status: **no verified grammar correction for this kind**. The required field and owning class vary by producer; no universal FieldWorks field can be named. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Use the finding description to identify the affected item and field. No single FieldWorks correction is known for this kind. If the project opens and shows the intended data, report the finding and PanGloss version to PanGloss; ask FieldWorks support for help if the project data is damaged.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:104`; INTEGRITY.

### `fwdata.missing-lang-project`

Cause: The imported project data does not contain its required language project.

Location and action: Open the intended project in FieldWorks and save it. If FieldWorks cannot open it, use File > Restore a Project... with a known good backup; if it opens normally, report the reading failure to PanGloss.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:111`; SETTINGS.

### `fwdata.only-first-used`

Cause: This import path reads the first phoneme set and ignores later sets.

Location and action: In Grammar > Phonemes, check that the intended inventory appears in the first set. If it does, no change is needed; otherwise report the limitation before reorganizing the project.

Verified inspection destinations: Grammar > Phonemes (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:118`; PHON.

### `fwdata.empty-representation`

Cause: A phoneme, boundary marker, or terminal mapping has no usable text representation.

Location and action: For a phoneme, inspect In Orthography as in Grammar > Phonemes. For an affix process terminal mapping, inspect Allomorphs > Affix Process Rule in Lexicon > Lexicon Edit and correct an unintended phoneme reference or spelling. No editable boundary field has been verified; report that case.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:125`; INTEGRITY, PHON, PROCESS.

### `fwdata.unrecognized-enum-value`

Cause: A source value is not recognized by this importer.

Correction status: **no verified grammar correction for this kind**. The enum and owner vary; a parser limitation must not be presented as a known grammar correction. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Use the finding description to identify the affected item and field. No single FieldWorks correction is known for this kind. If the project opens and shows the intended data, report the finding and PanGloss version to PanGloss; ask FieldWorks support for help if the project data is damaged.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:132`; INTEGRITY.

### `fwdata.metathesis-approximation`

Cause: The FieldWorks importer converts this rule to an approximation rather than preserving its exact metathesis behavior.

Location and action: In Grammar > Phonological Rules, review the emitted rule description and compare affected words; report a valid rule whose behavior is not preserved.

Verified inspection destinations: Grammar > Phonological Rules (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:139`; RULE.

### `fwdata.stale-adhoc-prohibition`

Cause: An ad hoc morpheme rule names an inflectional affix whose slot is outside every enabled template.

Location and action: In Grammar > Ad hoc Rules, inspect Key Morpheme and Other Morpheme(s); in Grammar > Category Edit, inspect the affix's Affix Templates. Restore the intended assignment if the affix should be usable.

Verified inspection destinations: Grammar > Ad hoc Rules > Key Morpheme; Grammar > Ad hoc Rules > Other Morpheme(s); Grammar > Category Edit > Affix Templates.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:146`; TEMPLATE, ADHOC.

### `fwdata.no-usable-allomorphs`

Cause: The lexical entry has no allomorph the importer can use.

Location and action: In Lexicon > Lexicon Edit, inspect Lexeme Form and Allomorphs, including Morph Type. Correct an empty or incorrect form; if the form is accurate, review the specific finding explaining why PanGloss skipped it.

Verified inspection destinations: Lexicon > Lexicon Edit > Lexeme Form; Lexicon > Lexicon Edit > Allomorphs.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:153`; LEX.

### `fwdata.unsupported-morph-type`

Cause: The allomorph uses a morph type this importer cannot represent.

Location and action: In Lexicon > Lexicon Edit, check the warning's named morph type; select a supported type only if it matches the intended analysis.

Verified inspection destinations: Lexicon > Lexicon Edit > Morph Type.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:160`; LEX.

### `fwdata.unknown-morph-type-guid`

Cause: The allomorph has a morph-type GUID that is not recognized as a known FieldWorks morph type.

Location and action: In Lexicon > Lexicon Edit, inspect the named form's Morph Type. Select the intended type if the classification is wrong; if FieldWorks shows the correct type, report the reading failure to PanGloss.

Verified inspection destinations: Lexicon > Lexicon Edit > Morph Type.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:167`; LEX.

### `fwdata.reference-not-in-scope`

Cause: An affix process output mapping refers to an input part outside the process's own input list.

Location and action: In Lexicon > Lexicon Edit, inspect Allomorphs > Affix Process Rule. Correct a mapping to use an intended input part in that rule; if FieldWorks shows a valid rule, report the mapping and reference details.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Affix Process Rule.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:174`; LEX.

### `fwdata.invalid-parser-parameter`

Cause: A parser parameter has a value this importer cannot use.

Location and action: In the Words area, open Parser > Edit Parser Parameters... and correct the named setting if it is malformed. If the saved setting is valid, report the reading failure to PanGloss.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:181`; SETTINGS.

### `invalid-source.active-parser`

Cause: PanGloss could not determine a supported active parser from the saved project.

Location and action: In the Words area, open Parser > Choose Parser and check the selected parser. If FieldWorks shows the intended parser, save the project and retry; report the reading failure to PanGloss if it persists.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:188`; SETTINGS.

### `invalid-source.duplicate-guid`

Cause: Two source items share an identity that must be unique.

Correction status: **no verified grammar correction for this kind**. Object identity is not a linguist-editable grammar field; support or project recovery is needed. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Report the named records to FieldWorks support and PanGloss. Internal identities are not editable grammar fields. Restore a known good backup if FieldWorks cannot open the project.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:195`; INTEGRITY.

### `invalid-source.missing-guid`

Cause: A source item has no stable identity for import.

Correction status: **no verified grammar correction for this kind**. Object identity is not a linguist-editable grammar field; support or project recovery is needed. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Report the record details to FieldWorks support and PanGloss. No verified editable identity field repairs this finding; restore a known good backup if the project cannot open.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:202`; INTEGRITY.

### `fwdata.writing-system-store-unreadable`

Cause: The project writing-system data could not be read.

Location and action: In Tools > Configure > Set up Vernacular Writing Systems... and Set up Analysis Writing Systems..., check the project data and retry the import.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:209`; SETTINGS.

### `snapshot.dangling-reference`

Cause: Snapshot validation found a reference whose target is absent.

Correction status: **no verified grammar correction for this kind**. This is an intermediate-snapshot integrity check across several classes; a direct FieldWorks repair cannot be inferred from the kind. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Use the finding description to identify the affected item and field. No single FieldWorks correction is known for this kind. If the project opens and shows the intended data, report the finding and PanGloss version to PanGloss; ask FieldWorks support for help if the project data is damaged.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:216`; INTEGRITY.

### `snapshot.feature-structure-unresolved`

Cause: A snapshot reference to a feature or value cannot be resolved in the source data.

Correction status: **no verified grammar correction for this kind**. Feature structures occur on several classes; the kind does not determine an editable owner field. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Use the finding description to identify the affected item and field. No single FieldWorks correction is known for this kind. If the project opens and shows the intended data, report the finding and PanGloss version to PanGloss; ask FieldWorks support for help if the project data is damaged.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:223`; INTEGRITY.

### `snapshot.rule-feature-unresolved`

Cause: A rule or exception feature reference resolves to neither an inflection class nor an exception feature.

Location and action: In Grammar > Phonological Rules, inspect the named rule's required or excluded rule features. The unresolved target may be an inflection class or an exception feature.

Verified inspection destinations: Grammar > Phonological Rules (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:230`; RULE.

### `snapshot.reference-out-of-scope`

Cause: A sense references a grammatical analysis outside its owning entry's analysis list.

Location and action: In Lexicon > Lexicon Edit, inspect the sense's Grammatical Info. and select the intended analysis belonging to that entry. If FieldWorks already displays a valid analysis, report the conversion problem.

Verified inspection destinations: Lexicon > Lexicon Edit > Grammatical Info..

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:237`; LEX.

### `grammar.phoneme.no-representation`

Cause: The phoneme has no grapheme representation available to the parser.

Location and action: In Grammar > Phonemes, select the named phoneme. Under In Orthography as, use Insert Grapheme and enter the spelling used in lexical forms. If the phoneme already has that spelling, report the loading failure to PanGloss.

Verified inspection destinations: Grammar > Phonemes > In Orthography as.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:244`; PHON.

### `grammar.phoneme.nfd-collision`

Cause: A phoneme spelling collides with an earlier phoneme or boundary after Unicode normalization.

Location and action: In Grammar > Phonemes, compare the named phoneme's In Orthography as values. Correct an unintended duplicate; retain a linguistically intentional distinction and report the limitation.

Verified inspection destinations: Grammar > Phonemes > In Orthography as.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:251`; PHON.

### `grammar.phoneme.feature-unresolved`

Cause: A phoneme feature structure contains an unresolved feature or value.

Location and action: In Grammar > Phonemes, select the named phoneme and open the chooser for Phonological Features. Reselect the intended feature values. If a feature or value is missing, check Grammar > Phonological Features; report the finding if FieldWorks already shows valid values.

Verified inspection destinations: Grammar > Phonemes > Phonological Features; Grammar > Phonological Features (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:258`; PHON.

### `grammar.phoneme.complex-feature-unsupported`

Cause: The phoneme has a complex feature value this parser cannot represent.

Location and action: In Grammar > Phonemes, keep the intended analysis and report it if the value is valid; use a supported value only when linguistically equivalent.

Verified inspection destinations: Grammar > Phonemes > Phonological Features; Grammar > Phonological Features (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:265`; PHON.

### `grammar.boundary.nfd-collision`

Cause: A boundary marker representation normalizes to an earlier phoneme or boundary representation, so the later item is not distinct to the parser.

Correction status: **no verified grammar correction for this kind**. The model defines boundary codes, but no editable FLEx boundary representation control was verified. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Compare any named phoneme's In Orthography as values in Grammar > Phonemes. No editable FieldWorks boundary field has been verified; report an intentional collision to PanGloss or FieldWorks support.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:272`; PHON, INTEGRITY.

### `grammar.boundary.no-representation`

Cause: The boundary marker has no usable representation.

Correction status: **no verified grammar correction for this kind**. The model defines boundary codes, but no editable FLEx boundary representation control was verified. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Use the description to identify the marker. No editable FieldWorks boundary field has been verified; report the missing representation to PanGloss or FieldWorks support.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:279`; PHON, INTEGRITY.

### `grammar.boundary.morph-marker-unresolved`

Cause: The compiler cannot resolve its fixed morpheme-boundary representation (+) and uses a null boundary instead.

Correction status: **no verified grammar correction for this kind**. The compiler selects its fixed + marker internally; no FieldWorks setting controls that selection. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Report the project and boundary details to PanGloss. No configurable FieldWorks parser setting or verified editable field selects this compiler boundary.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:286`; PHON, INTEGRITY.

### `grammar.natclass.segments-member-unresolved`

Cause: An explicit segment-list natural class contains a phoneme that cannot be resolved, so the whole class is skipped.

Location and action: In Grammar > Natural Classes, select the named class and open the Phonemes chooser. Select the intended existing phonemes. Add a missing language phoneme in Grammar > Phonemes first; if all members are present, report the loading failure.

Verified inspection destinations: Grammar > Natural Classes > Phonemes.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:293`; NC.

### `grammar.natclass.feature-constraint-unresolved`

Cause: A feature-defined natural class has an unresolved feature constraint; other constraints can still load.

Location and action: In Grammar > Natural Classes, select the named class and open the Phonological Features chooser. Reselect the intended values, checking Grammar > Phonological Features if a feature or value is missing. Report the finding if the definition is already valid.

Verified inspection destinations: Grammar > Natural Classes > Phonological Features; Grammar > Phonological Features (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:300`; NC, PHON.

### `grammar.natclass.complex-feature-unsupported`

Cause: A natural-class constraint uses a complex feature value the parser cannot represent; remaining constraints may still load.

Location and action: In Grammar > Natural Classes, keep valid feature data and report the unsupported value, or use an equivalent supported constraint if one exists.

Verified inspection destinations: Grammar > Natural Classes > Phonological Features.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:307`; NC.

### `grammar.feature.phon-complex-unsupported`

Cause: A phonological feature definition uses a complex value the parser cannot represent.

Location and action: In Grammar > Phonological Features, preserve the intended feature system and report the unsupported value unless an equivalent closed feature is appropriate.

Verified inspection destinations: Grammar > Phonological Features (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:314`; PHON.

### `grammar.stem-name.build-failed`

Cause: The stem allomorph label could not be represented in the compiled grammar.

Location and action: In Grammar > Category Edit, inspect Stem Allomorph Labels and the label's Feature Sets. Correct an invalid feature reference; if FieldWorks shows a valid definition, report the loading failure.

Verified inspection destinations: Grammar > Category Edit > Stem Allomorph Labels > Feature Sets.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:321`; TEMPLATE, STEMLABEL, MSA.

### `grammar.stem-name.empty-regions`

Cause: The stem allomorph label has no non-empty inflection feature set.

Location and action: In Grammar > Category Edit, inspect Stem Allomorph Labels > Feature Sets and supply an intended Feature Set if the label should constrain an allomorph. Otherwise remove an unintended label assignment in Lexicon > Lexicon Edit.

Verified inspection destinations: Grammar > Category Edit > Stem Allomorph Labels > Feature Sets.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:328`; TEMPLATE, STEMLABEL, MSA.

### `grammar.compound-rule.build-failed`

Cause: The compound rule could not be represented in the compiled grammar.

Location and action: In Grammar > Compound Rules, check the rule's constituent categories; report it if those categories are valid.

Verified inspection destinations: Grammar > Compound Rules (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:335`; COMPOUND.

### `grammar.compound-rule.side-pos-unresolved`

Cause: A compound rule input or output refers to a grammatical category that cannot be resolved.

Location and action: In Grammar > Compound Rules, inspect Category on the named input or output and select the intended existing category.

Verified inspection destinations: Grammar > Compound Rules > Category.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:342`; COMPOUND.

### `grammar.compound-rule.side-exception-feature-unresolved`

Cause: A compound rule exception refers to a feature that cannot be resolved.

Location and action: In Grammar > Compound Rules, repair the named exception feature reference if it is stale.

Verified inspection destinations: Grammar > Compound Rules > Exception "Features".

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:349`; COMPOUND.

### `grammar.msa.build-failed`

Cause: The grammatical analysis could not be represented in the compiled grammar.

Location and action: In Lexicon > Lexicon Edit, inspect the named sense's Grammatical Info. and Grammatical Info. Details. Check Category and the feature values named in the finding. Correct an unintended value; if the analysis is valid, report the loading failure to PanGloss.

Verified inspection destinations: Lexicon > Lexicon Edit > Grammatical Info..

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:356`; LEX, MSA.

### `grammar.msa.no-allomorphs`

Cause: The lexical entry has no allomorph available to this analysis.

Location and action: In Lexicon > Lexicon Edit, inspect Lexeme Form, Allomorphs and Morph Type. Resolve the specific allomorph loading findings before adding a new form.

Verified inspection destinations: Lexicon > Lexicon Edit > Lexeme Form; Lexicon > Lexicon Edit > Allomorphs.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:363`; LEX, MSA.

### `grammar.msa.no-rule-form-allomorphs`

Cause: This analysis has no usable affix-form allomorph.

Location and action: In Lexicon > Lexicon Edit, inspect Allomorphs, their forms, Morph Type and Environments. Review the specific finding for each rejected form.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:370`; LEX, MSA.

### `grammar.msa.exception-feature-unresolved`

Cause: The analysis refers to an exception feature that cannot be resolved.

Location and action: In Lexicon > Lexicon Edit, inspect Grammatical Info. Details and the named Exception "Features", From Exception "Features", or To Exception "Features" field. Reselect the intended existing item; if the selection is already valid, report the loading failure.

Verified inspection destinations: Lexicon > Lexicon Edit > Exception "Features"; Lexicon > Lexicon Edit > From Exception "Features"; Lexicon > Lexicon Edit > To Exception "Features".

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:377`; LEX, MSA.

### `grammar.msa.inflection-class-unresolved`

Cause: The analysis's inflection-class reference cannot be resolved.

Location and action: In Lexicon > Lexicon Edit, inspect Grammatical Info. Details and the named Inflection Class, From Inflection Class, or To Inflection Class field. Select the intended existing class. Use Grammar > Category Edit > Inflection Class Info to check its definition if it is missing.

Verified inspection destinations: Lexicon > Lexicon Edit > Inflection Class; Lexicon > Lexicon Edit > From Inflection Class; Lexicon > Lexicon Edit > To Inflection Class; Grammar > Category Edit > Inflection Class Info.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:384`; LEX, TEMPLATE, MSA.

### `grammar.msa.stem-name-unresolved`

Cause: The analysis's required stem allomorph label cannot be resolved.

Location and action: In Lexicon > Lexicon Edit, inspect From Stem Allomorph Label. In Grammar > Category Edit, check Stem Allomorph Labels and select the intended existing label.

Verified inspection destinations: Lexicon > Lexicon Edit > From Stem Allomorph Label; Grammar > Category Edit > Stem Allomorph Labels.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:391`; LEX, TEMPLATE, STEMLABEL, MSA.

### `grammar.msa.lex-entry-infl-type-unresolved`

Cause: An analysis refers to an entry inflection type that cannot be resolved.

Location and action: In Lexicon > Lexicon Edit, check Variant Type on the variant entry. In Lists > Variant Types, check that the intended irregularly inflected type exists, then reselect it if the reference is stale. If FieldWorks shows the correct type, report the loading failure.

Verified inspection destinations: Lexicon > Lexicon Edit > Variant Type; Lists > Variant Types (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:398`; LEX, VARIANT, MSA.

### `grammar.variant.component-unresolved`

Cause: A variant refers to an entry or sense that cannot be resolved.

Location and action: In Lexicon > Lexicon Edit, inspect Variant of and select the intended entry or sense. If FieldWorks displays it correctly, report the reference.

Verified inspection destinations: Lexicon > Lexicon Edit > Variant of.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:405`; LEX, VARIANT.

### `grammar.allomorph.unsegmentable`

Cause: The allomorph spelling cannot be divided into the parser's phoneme and boundary inventory.

Location and action: In Lexicon > Lexicon Edit, check Lexeme Form or Allomorphs > Form and its Environments as named in the finding. Correct unintended spelling. If the spelling is intended, check Grammar > Phonemes > In Orthography as for every phoneme used in it; report a valid form or environment that still cannot be loaded.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Form.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:412`; LEX.

### `grammar.allomorph.morph-type-unsupported`

Cause: The allomorph's morph type is not supported for this compiler path.

Location and action: In Lexicon > Lexicon Edit, check the named morph type and report the case if the source analysis is valid.

Verified inspection destinations: Lexicon > Lexicon Edit > Morph Type.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:419`; LEX.

### `grammar.allomorph.morph-type-unsupported-as-rule-form`

Cause: A circumfix or discontiguous phrase has a whole form that is not loaded as a rule form; its separate parts are loaded through their own path.

Location and action: In Lexicon > Lexicon Edit, no change is needed when the separate parts are the intended parser representation.

Verified inspection destinations: Lexicon > Lexicon Edit > Morph Type.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:426`; LEX.

### `grammar.allomorph.not-a-rule-form`

Cause: An affix form is empty, or an infix has no position environment, so this allomorph cannot be loaded through that affix-rule path.

Location and action: In Lexicon > Lexicon Edit, inspect Allomorphs > Form or Infix Positions as identified by the warning description. Supply missing source data only if the allomorph should be an affix.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Form; Lexicon > Lexicon Edit > Allomorphs > Infix Positions.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:433`; LEX.

### `grammar.allomorph.reduplication-unsupported`

Cause: The allomorph uses a reduplication pattern that this parser does not run.

Location and action: In Lexicon > Lexicon Edit, keep the intended pattern and report it; do not replace it unless an equivalent supported analysis is known.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Form.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:440`; LEX.

### `grammar.allomorph.process-build-failed`

Cause: The affix process mappings could not be represented in the compiled grammar.

Location and action: In Lexicon > Lexicon Edit, open Allomorphs > Affix Process Rule and inspect the named input or output part. Correct an unintended mapping to the rule's own input or to the intended phoneme or natural class. If the rule is valid in FieldWorks, report it to PanGloss.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Affix Process Rule.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:447`; LEX, PROCESS.

### `grammar.allomorph.inflection-class-unresolved`

Cause: The allomorph refers to an inflection class that cannot be resolved.

Location and action: In Lexicon > Lexicon Edit, open the named form's Inflection Classes chooser and select the intended existing class. In Grammar > Category Edit > Inflection Class Info, check that class's definition if it is missing. If the assignment is valid, report the loading failure.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Inflection Classes; Grammar > Category Edit > Inflection Class Info.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:454`; LEX, TEMPLATE.

### `grammar.allomorph.feature-build-failed`

Cause: The allomorph's morphosyntactic features could not be represented.

Location and action: In Lexicon > Lexicon Edit, inspect Allomorphs > Required Features and reselect valid intended values. If FieldWorks already shows valid feature data, report the loading failure.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Required Features.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:461`; LEX.

### `grammar.allomorph.environment-build-failed`

Cause: The allomorph environment could not be converted for parsing.

Location and action: In Lexicon > Lexicon Edit, inspect Environments on the named Lexeme Form or Allomorphs. In Grammar > Environments, check the selected environment's String Representation. Correct unintended phonemes, natural-class abbreviations, or notation; if the restriction is valid, report the loading failure.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Environments; Grammar > Environments > String Representation.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:468`; LEX, ENV.

### `grammar.circumfix.missing-half`

Cause: The circumfix entry does not have both required prefix and suffix parts.

Location and action: In Lexicon > Lexicon Edit, check that Lexeme Form has Morph Type circumfix and that Allomorphs contains both a prefix and a suffix, each with its own Form and Morph Type. Add the missing half if the entry is intended to be a circumfix; report the finding if both halves are already present.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs; Lexicon > Lexicon Edit > Morph Type.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:475`; LEX, CIRCUMFIX.

### `grammar.circumfix.environment-combination-skipped`

Cause: An environment prevented this circumfix-half combination from being compiled.

Location and action: In Lexicon > Lexicon Edit, inspect Environments on both parts under Allomorphs. In Grammar > Environments, inspect their String Representation. If both express the intended restriction, report this unsupported combination.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Environments; Grammar > Environments > String Representation.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:482`; LEX, ENV, CIRCUMFIX.

### `grammar.environment.unresolved`

Cause: A referenced named environment is missing, so this finding is an import error.

Location and action: In Lexicon > Lexicon Edit, inspect Allomorphs > Environments; in Grammar > Environments, inspect String Representation. Restore the intended environment or reselect it only if the reference is stale.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Environments; Grammar > Environments > String Representation.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:489`; LEX, ENV.

### `grammar.environment.invalid`

Cause: The environment expression could not be parsed and is ignored as a restriction.

Location and action: In Grammar > Environments, check String Representation for phonological environment '{subject}'. An environment uses / and _ to separate the surrounding context from the allomorph position, square brackets for a natural-class abbreviation, and # for a word boundary. Correct unintended notation or names; check phonemes in Grammar > Phonemes and class abbreviations in Grammar > Natural Classes. Report valid syntax the parser rejects.

Verified inspection destinations: Grammar > Environments > String Representation.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:496`; ENV.

### `grammar.template.slot-unresolved`

Cause: An affix template refers to a slot that cannot be resolved.

Location and action: In Grammar > Category Edit, select the category and inspect Affix Templates and Affix Slots. Restore the intended slot or select an existing slot in the template if the reference is stale. If FieldWorks already shows a valid slot, report the loading failure.

Verified inspection destinations: Grammar > Category Edit > Affix Templates; Grammar > Category Edit > Affix Slots.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:503`; TEMPLATE.

### `grammar.template.slot-no-rules`

Cause: No usable inflectional affix was loaded for this template slot.

Location and action: In Grammar > Category Edit > Affix Templates, right-click the intended slot and choose Add inflectional affix(es) to that slot. Select an appropriate existing affix only if the slot is unintentionally empty. If affixes are already assigned, resolve their individual loading findings; check Grammatical Info. > Category and Slots in Lexicon > Lexicon Edit.

Verified inspection destinations: Grammar > Category Edit > Affix Templates; Grammar > Category Edit > Affix Slots.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:510`; TEMPLATE.

### `grammar.template.no-slots`

Cause: The affix template has no slots containing a loaded affix rule; slots can exist in FieldWorks and still be unavailable here.

Location and action: In Grammar > Category Edit > Affix Templates, inspect the named template. If an intended slot is missing, right-click STEM and choose Insert Slot before Stem or Insert Slot after Stem, then add the intended inflectional affixes to it. If slots and affixes already exist, resolve their loading findings; an intentionally unused template needs no new data.

Verified inspection destinations: Grammar > Category Edit > Affix Templates; Grammar > Category Edit > Affix Slots.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:517`; TEMPLATE.

### `grammar.template.build-failed`

Cause: The template could not be represented from its slots and inflectional affixes.

Location and action: In Grammar > Category Edit, check the template and its referenced slots and affixes; report a valid unsupported arrangement.

Verified inspection destinations: Grammar > Category Edit > Affix Templates; Grammar > Category Edit > Affix Slots.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:524`; TEMPLATE.

### `grammar.null-affix.mpr-unresolved`

Cause: An internal restriction for an irregular inflection type could not be resolved.

Location and action: In Lists > Variant Types, inspect the named type. No editable restriction field has been verified for this registry failure; report the type and failure details to PanGloss.

Verified inspection destinations: Lists > Variant Types (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:531`; VARIANT.

### `grammar.null-affix.syn-fs-failed`

Cause: Features for an internally generated null affix could not be represented.

Location and action: In Lists > Variant Types, inspect Inflection Features and choose valid intended values. If the values are valid, report them and the detected cause to PanGloss.

Verified inspection destinations: Lists > Variant Types > Inflection Features.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:538`; VARIANT.

### `grammar.null-affix.segment-failed`

Cause: PanGloss could not build its generated empty-affix marker for this irregular inflection type.

Correction status: **no verified grammar correction for this kind**. The failing null-affix marker is generated by PanGloss, rather than supplied through a FieldWorks form field. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Report the type and marker details to PanGloss. The failing marker is generated internally; no verified FieldWorks form correction is known.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:545`; VARIANT, INTEGRITY.

### `grammar.rule.metathesis-unsupported`

Cause: The compiler skips this metathesis rule; it does not approximate or run it.

Location and action: In Grammar > Phonological Rules, keep the valid rule and report it if words rely on its behavior; inspect affected words with a parse trace.

Verified inspection destinations: Grammar > Phonological Rules (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:552`; RULE.

### `grammar.rule.build-failed`

Cause: The phonological rule could not be represented in the compiled grammar.

Location and action: In Grammar > Phonological Rules, select the named rule and inspect Rule Formula, including its input, output, and context. Correct unintended phoneme or natural-class references. If FieldWorks shows the intended rule, report the loading failure to PanGloss.

Verified inspection destinations: Grammar > Phonological Rules (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:559`; RULE.

### `grammar.rule.feature-constraint-unresolved`

Cause: The rule refers to a feature-constraint object that cannot be resolved.

Location and action: In Grammar > Phonological Rules, inspect the named rule part. Correct a stale constraint reference; if FieldWorks displays a valid constraint, report the rule and missing-object details.

Verified inspection destinations: Grammar > Phonological Rules (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:566`; RULE.

### `grammar.rule.feature-constraint-phon-feature-unresolved`

Cause: A rule constraint refers to a missing phonological feature.

Location and action: In Grammar > Phonological Rules, inspect the named constraint; in Grammar > Phonological Features, check the intended feature and reselect it in the rule.

Verified inspection destinations: Grammar > Phonological Rules (tool verified; no specific editable field established); Grammar > Phonological Features (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:573`; RULE, PHON.

### `grammar.rule.rule-feature-unresolved`

Cause: A phonological rule feature reference cannot be resolved.

Location and action: In Grammar > Phonological Rules, inspect Required Properties and Excluded Properties on the named rule. Reselect an intended existing property if the reference is stale; if FieldWorks shows a valid selection, report the loading failure.

Verified inspection destinations: Grammar > Phonological Rules (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:580`; RULE.

### `grammar.strata.custom-unsupported`

Cause: Custom strata are replaced by the parser's default strata.

Correction status: **no verified grammar correction for this kind**. Custom strata are valid data that PanGloss replaces with defaults; no equivalent grammar correction is established. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: No grammar edit is required for valid custom strata; report the configuration if it is needed for the analysis.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:587`; SETTINGS, INTEGRITY.

### `grammar.adhoc-prohibition.unresolved`

Cause: An ad hoc prohibition refers to forms the compiler cannot resolve.

Location and action: In Grammar > Ad hoc Rules, inspect Key Morpheme and Other Morpheme(s), or Key Allomorph and Other Allomorph(s), as named in the finding. Reselect the intended existing items. If the items exist but were skipped during loading, resolve their individual findings first; report a valid rule that still cannot be loaded.

Verified inspection destinations: Grammar > Ad hoc Rules (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:594`; ADHOC.

### `grammar.mrule.unreachable-compacted`

Cause: An affix rule unreachable from the compiled grammar was compacted.

Location and action: No change is needed if the affix is intentionally unused. Otherwise inspect its category's Affix Templates in Grammar > Category Edit and its Grammatical Info. in Lexicon > Lexicon Edit.

Verified inspection destinations: Grammar > Category Edit > Affix Templates; Lexicon > Lexicon Edit > Grammatical Info..

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:601`; LEX, TEMPLATE.

### `grammar.cooccurrence.target-unreachable`

Cause: An ad hoc rule whose target cannot be reached in the compiled grammar was compacted.

Location and action: No change is needed if the rule is intentionally unused. Otherwise check its target affix's Grammatical Info. in Lexicon > Lexicon Edit and Affix Templates in Grammar > Category Edit, then review Grammar > Ad hoc Rules.

Verified inspection destinations: Lexicon > Lexicon Edit > Grammatical Info.; Grammar > Category Edit > Affix Templates; Grammar > Ad hoc Rules (tool verified; no specific editable field established).

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:608`; LEX, TEMPLATE, ADHOC.

### `grammar.natclass.unreferenced-compacted`

Cause: An eligible natural class not referenced by the compiled grammar was compacted.

Location and action: No change is needed if the class is intentionally unused. If it should be used, inspect the relevant rule in Grammar > Phonological Rules or String Representation in Grammar > Environments.

Verified inspection destinations: Grammar > Phonological Rules (tool verified; no specific editable field established); Grammar > Environments > String Representation.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:615`; ENV, RULE.

### `conversion.source-provenance-unknown`

Cause: Conversion provenance is missing or invalid, so PanGloss cannot certify the completeness of the conversion.

Correction status: **no verified grammar correction for this kind**. Missing conversion provenance belongs to the intermediate grammar, not an editable FieldWorks identity field. Existing support/recovery guidance may remain; no editable grammar field is proposed.

Location and action: Report the project and diagnostic details to PanGloss. No FieldWorks identity field can repair this finding; preserve source provenance when regenerating an intermediate grammar.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:622`; INTEGRITY.

### `conversion.unsegmentable-form`

Cause: The substrate allomorph cannot be divided using the imported phoneme and boundary inventory.

Location and action: In Lexicon > Lexicon Edit, check the named Lexeme Form or Allomorphs > Form. Use the finding's character position to locate unintended spelling. If the spelling is intended, check Grammar > Phonemes > In Orthography as for its phonemes; report a valid form that still cannot be loaded.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Form.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:629`; LEX.

### `substrate.classification-ambiguous`

Cause: PanGloss lacks evidence to classify an allomorph character and skips the form instead of guessing.

Location and action: In Lexicon > Lexicon Edit, inspect the form. Correct an unintended character; if it is a genuine phoneme, define it in Grammar > Phonemes > In Orthography as. If spelling and inventory are correct, inspect the vernacular writing system and report missing classification evidence.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Form; Grammar > Phonemes > In Orthography as.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:636`; LEX, PHON.

### `migration.inferred-segment-with-feature-rule`

Cause: A character inferred from an allomorph has no authored phonological features yet matches a feature-defined natural class.

Location and action: If the character is a language phoneme, define its In Orthography as and Phonological Features in Grammar > Phonemes. Otherwise correct the unintended allomorph spelling in Lexicon > Lexicon Edit.

Verified inspection destinations: Grammar > Phonemes > In Orthography as; Grammar > Phonemes > Phonological Features; Lexicon > Lexicon Edit > Allomorphs > Form.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:643`; LEX, PHON.

### `conversion.unsupported-construct`

Cause: PanGloss cannot check this reduplication pattern against the phoneme inventory.

Location and action: In Lexicon > Lexicon Edit, inspect the allomorph form. Keep an accurate pattern and report it; change it only if an equivalent supported representation is known.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Form.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:650`; LEX.

### `substrate.position-unmapped`

Cause: PanGloss's character-position mapping failed for this allomorph.

Location and action: In Lexicon > Lexicon Edit, inspect the named position in the form. Correct an unintended character; if FieldWorks displays the intended spelling, report the mapping problem and diagnostic details.

Verified inspection destinations: Lexicon > Lexicon Edit > Allomorphs > Form.

Sources: PG `rust/crates/pg-snapshot/src/warning_metadata.rs:657`; LEX.

## Implementation decisions

Retain the existing advice mechanism and all 88 identifiers. Preserve guidance for support, recovery, intentionally unused data, and unsupported accurate data; those are next steps, not promises of a FieldWorks grammar fix. Strengthen the confirmed corrections above. Add a test covering the **runtime diagnostics** for all catalog kinds so a nonempty catalog string that fails to reach Motif is caught. Also require a verified tool/menu destination or an explicit per-kind reason when none is established. Unknown future producer codes remain outside the registered catalog and retain the existing raw-message fallback.

The malformed-project and intermediate-snapshot kinds cannot be assigned a universal grammar correction. The no-correction reasons above are deliberate. `hc-partial-reason-unspecified` likewise has no specific correction. Metathesis, complex phonological values, reduplication, custom strata, and other valid unsupported constructs have known inspection locations or reporting steps; the sources do not prove a generally equivalent rewrite.

The first managed check command, `pwsh -NoProfile -File rust/tools/pg.ps1 -Mode check -Package pg-grammar`, exited 21 before Cargo: no finite memory.max cap in visible current cgroup ancestry. Validation requires the configured host cgroup hierarchy; it must not be bypassed with bare Cargo. Herdr delegation is unavailable inside this lane (socket access denied), and the named agent-handoff skill was not present in the searched skill locations.
