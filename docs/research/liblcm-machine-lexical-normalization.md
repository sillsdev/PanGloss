# LibLCM and Machine lexical-form alignment

## Findings

### Machine (HermitCrab)

- `CharacterDefinitionTable` normalizes every declared string representation to Unicode NFD for lookup, segments input after NFD normalization, and uses longest-match segmentation. Canonically equivalent NFC/NFD input therefore resolves identically, but case is not folded implicitly. See `../machine/src/SIL.Machine.Morphology.HermitCrab/CharacterDefinitionTable.cs:56-87`, `:108-133`, and `:339-351`.
- A character definition may have several string representations. They share one phonological feature structure; `GetMatchingStrReps` retrieves every spelling whose feature structure subsumes a shape node (`CharacterDefinitionTable.cs:92-104`). Thus spelling aliases declared by the grammar become the same segment for morphology.
- `Segments` deliberately retains both the supplied textual `Representation` and the frozen segmented `Shape` (`../machine/src/SIL.Machine.Morphology.HermitCrab/Segments.cs:7-27`, `:35-43`). This is the closest existing model for PanGloss's split between display/export spelling and grammatical lookup key.
- Root lookup is not string-keyed. `RootAllomorphTrie` compiles each root's sequence of segment feature structures into an FST and searches a segmented `Shape` using unification (`../machine/src/SIL.Machine.Morphology.HermitCrab/RootAllomorphTrie.cs:13-33`, `:37-67`, `:70-78`). Distinct declared spellings with equivalent segment structures consequently match the same root behavior.
- A `Morpher` snapshots the grammar lexicon when constructed: it scans every stratum entry, adds non-pattern allomorphs to per-stratum tries, and compiles analysis/synthesis rules (`../machine/src/SIL.Machine.Morphology.HermitCrab/Morpher.cs:31-55`). Although `Stratum.Entries` is mutable and maintains back-references (`../machine/src/SIL.Machine.Morphology.HermitCrab/Stratum.cs:78-89`, `:115-118`), changing it later does not update an existing `Morpher` trie. A no-recompile PanGloss overlay should therefore be a separate runtime lookup source (or rebuild a small overlay index), not mutation of `Stratum.Entries` under an existing `Morpher`.
- HermitCrab retains a root's authored spelling (`Segments.Representation`), while generated surface rendering selects the first matching string representation for each shape node (`../machine/src/SIL.Machine.Morphology.HermitCrab/HermitCrabExtensions.cs:251-268`). PanGloss should not assume synthesized spelling necessarily reproduces the spelling used in lookup.
- A Machine `LexEntry` already separates identity/category data from forms: it owns multiple `RootAllomorph`s, an MPR feature set, a syntactic feature structure/POS, and optional family (`../machine/src/SIL.Machine.Morphology.HermitCrab/LexEntry.cs:20-25`, `:53-100`). This supports an overlay entry ID independent of stem spelling and permits multiple entries at the same segmented key.

### LibLCM

- LCM string properties are stored as NFD; generated setters explicitly normalize incoming values to `NormalizationForm.FormD` (for example `../LibLCM/src/SIL.LCModel/DomainImpl/GeneratedClasses.cs:2389-2390`, repeated throughout generated string properties). Serialization commonly emits NFC, so normalization form is an interchange/storage concern rather than lexical identity.
- Wordform repository lookup is scoped by writing-system handle and keyed by NFD-normalized text. It builds one dictionary per writing system and normalizes both stored and queried forms before comparison (`../LibLCM/src/SIL.LCModel/Infrastructure/Impl/RepositoryAdditions.cs:876-895`). It does not collapse homographs into a semantic identity model; this cache maps orthographic wordform records.
- Case-insensitive behavior is explicit fallback behavior. `TryGetObject(..., fIncludeLowerCaseForm)` first tries the exact form, then optionally tries a lowercase form (`RepositoryAdditions.cs:854-867`). Lexical entry lookup similarly retries in lowercase only after an exact search fails (`RepositoryAdditions.cs:1470-1487`).
- Lowercasing is writing-system/locale aware. `CaseFunctions(CoreWritingSystemDefinition)` uses the writing system's `CaseAlias` when present and otherwise its ICU locale (`../LibLCM/src/SIL.LCModel.Core/Text/CaseFunctions.cs:16-18`, `:27-40`, `:49-56`). LCM explicitly warns that bypassing the writing-system case alias causes problems. PanGloss should therefore never apply invariant or generic Unicode lowercasing as an unconditional overlay key transformation.
- LCM objects have stable GUID identity independent of their mutable lexical fields (`../LibLCM/src/SIL.LCModel/DomainImpl/CmObject.cs:341-352`, with new IDs assigned at `:1370` and `:1410`). This aligns with PanGloss-owned immutable IDs for supplied entries: correcting spelling, gloss, or class should not change identity.

## Consequences for the PanGloss runtime overlay

1. Preserve the exact supplied stem string in the entry and exported JSON, but validate and segment it with the grammar's relevant `CharacterDefinitionTable`.
2. Build the grammatical lookup index from Machine-compatible segmented shapes/feature structures, not generic normalized strings. This automatically follows NFD equivalence, multi-character segmentation, and grammar-declared spelling aliases.
3. Allow multiple overlay entries at one segmented key. IDs—not spelling or `(spelling, class, gloss)` tuples—are the update/remove identity.
4. On analysis, preserve the actual input surface string separately from the stored lexical spelling and entry ID. Return both when useful; do not overwrite one with the other.
5. Do not perform unconditional lowercasing. If PanGloss later supports case fallback, make it an explicit policy supplied by grammar/writing-system metadata and apply it after exact lookup, matching LibLCM's ordering.
6. Keep the overlay index beside the immutable compiled `Morpher`. Mutating Machine's `Stratum.Entries` alone is insufficient because its root tries are construction-time snapshots.
7. Treat overlay revision changes as full analysis-cache invalidations. This is consistent with the separate compiled root index and also covers gloss-only changes in returned analyses.

In short: the strongest common alignment is **stable entry identity + exact authored form + grammar-derived segmented shape**. Unicode canonical normalization belongs inside segmentation/storage boundaries; case equivalence must remain writing-system policy, not PanGloss-wide identity.
