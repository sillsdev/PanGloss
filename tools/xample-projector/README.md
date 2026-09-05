# XampleProjector

A version-pinned managed helper that opens a FieldWorks 9 project via LibLCM and produces the
same two real FieldWorks projections of it that PanGloss needs to measure XAMPLE-to-HC migration
differences: the HC XML (`HCLoader` + `XmlLanguageWriter`) and the XAMPLE control/dictionary/
grammar files (the same XSL transforms and GAFAWS step `M3ToXAmpleTransformer`/`XAmpleParser`
drive internally, replicated here because that class is `internal`). Both sides read the same
opened `LcmCache`, so they can never diverge on which project state they saw.

Task 3, slice A shipped the tool scaffold plus three subcommands: `inspect` (read-only phonology
survey), `project` (full HC + XAMPLE projection), and `--validate-capture` (portable, FieldWorks-free
schema check). Task 3, slice B (this slice) adds `author` -- back a HermitCrab conformance fixture
(`grammar.xml`) out into a brand-new FieldWorks project via LibLCM, refusing every construct
outside a documented supported subset -- and `verify-parity`, a structural + HC-engine proof that
`project` on an authored project reproduces the fixture it came from. A later slice adds direct
`xample64.dll` parsing.

## Contract

Every producing mode writes a JSON response with `schemaVersion: 1`.

`inspect --project <path-to-.fwdata> --out <response.json>`:
```
{
  "schemaVersion": 1,
  "fieldWorksVersion": "9.3.10.26161",
  "sourcePath": "...", "sourceSha256": "<64 hex>",
  "projectName": "...", "activeParser": "HC" | "XAmple",
  "phonemes": [{ "guid", "representations": ["..."], "inboundReferences": [{ "guid", "class" }] }],
  "boundaryMarkers": [ ... same shape as phonemes ... ],
  "naturalClasses": [{ "guid", "name", "kind": "segments" | "features", "memberGuids": [...] }],
  "diagnostics": ["..."]
}
```

`project --project <path-to-.fwdata> --out-dir <dir> --database <name>` writes, into `<dir>`
(never `%TEMP%`, unlike the FieldWorks internals this replicates): `<database>.hc.xml`,
`<database>adctl.txt`, `<database>gram.txt`, `<database>lex.txt`,
`<database>XAmpleWordGrammarDebugger.xsl`, any GAFAWS intermediate files a POS-with-templates
pass produced, and `response.json`:
```
{
  "schemaVersion": 1, "mode": "project",
  "fieldWorksVersion": "...", "assemblyVersions": { "<dll>": "<file version>", ... },
  "sourcePath": "...", "sourceSha256": "<64 hex>", "database": "...",
  "generated": [{ "path", "sha256": "<64 hex>", "bytes" }],
  "hcLoadDiagnostics": [{ "kind", "message" }],
  "diagnostics": []
}
```
Every file in `generated` is described from its FINAL on-disk state after all writes complete
(some paths, e.g. the GAFAWS intermediate, are legitimately overwritten once per part-of-speech
that has slotted affix templates, so an early snapshot would go stale by the time the response is
written).

`author --grammar <grammar.xml> --out-dir <dir> --name <ProjectName> [--vernacular-ws <icu>]
[--xample-max-prefixes N] [--xample-max-analyses N]` parses and validates `grammar.xml` against the
supported subset below (refusing, exit 7, before ever touching LibLCM if anything falls outside it),
then creates `<out-dir>\<ProjectName>\<ProjectName>.fwdata` (+ `WritingSystemStore\*.ldml`) from it
and writes `<out-dir>\author-response.json`:
```
{
  "schemaVersion": 1, "mode": "author",
  "fieldWorksVersion": "...",
  "grammarPath": "...", "grammarSha256": "<64 hex>",
  "projectPath": "...", "projectSha256": "<64 hex>",
  "authored": { "<LCM class name>": <count>, ... },
  "unmapped": [ "<attribute/value ignored, with reason>", ... ],
  "guidMap": { "<fixture id>": "<lcm guid>", ... },
  "diagnostics": []
}
```
`guidMap` has one entry per created LCM object, keyed by the fixture id (or, for a `Slot`/
`AffixTemplate`, its `Name` text, or for a `MorphemeCoOccurrenceRule`/`AllomorphCoOccurrenceRule`,
`morphemeCoOccurrence[N]`/`allomorphCoOccurrence[N]` by document order -- the DTD gives none of
those elements an `id` attribute) that produced it: parts of speech, phonemes, natural classes
(segment-based), MPR features (as `ProdRestrict` possibilities), each `MorphologicalRule`'s
synthetic `LexEntry`, each subrule's `MoAffixAllomorph`, each `LexicalEntry`'s `LexEntry` and
allomorphs, each `Slot`'s `MoInflAffixSlot`, each `AffixTemplate`'s `MoInflAffixTemplate`, and each
co-occurrence rule's `MoMorphAdhocProhib`/`MoAlloAdhocProhib`. `unmapped` records attributes that
are read but have no FieldWorks equivalent (`Stratum@morphologicalRuleOrder`,
`Stratum@morphologicalRules`) rather than silently dropping them. After authoring, `author`
reconciles every construct `GrammarParser` produced against `guidMap`: anything parsed but never
authored refuses (exit 7, `author.unconsumed-construct`, naming the element and its id) rather than
producing a project silently missing it -- a parser addition with no matching authoring code fails
loudly instead of shipping a construct nobody can see is gone.

**Determinism.** Authoring the same `grammar.xml` twice produces two `.fwdata` files whose
`authored` counts and `guidMap` key sets are identical, and whose full text is identical once each
run's own `guidMap` guids are replaced by their fixture id and every remaining guid/timestamp is
blanked (LibLCM assigns every real guid randomly, and each object's `DateCreated`/`DateModified`
naturally differ by run) -- verified in `build.ps1 -Mode test`.

`verify-parity --grammar <grammar.xml> --hc-xml <projected.hc.xml> --guid-map <author-response.json>
--expect WORD=COUNT [--expect WORD=COUNT ...]` re-parses `grammar.xml`, loads the HC XML `project`
produced from the authored project (the SAME `HermitCrabInput`-shaped schema `XmlLanguageWriter`
writes -- see `Src\LexText\ParserCore\HCLoader.cs` for the loader and
`machine\src\SIL.Machine.Morphology.HermitCrab\XmlLanguageWriter.cs` for the writer, both
element-for-element compatible with the input DTD), and asserts. **Every structural expectation
below is derived from the re-parsed grammar.xml itself, never hardcoded to one fixture** -- this
command is meant to run against any grammar `author` can accept, not only the pilot fixture:
- rule/lex-entry/segment/slot counts and shapes match the fixture: each traced-back fixture
  subrule's own `InsertShape` (see the Gloss trace below) plus the automatic morph-boundary
  character every FieldWorks affix representation adds -- a PREFIX's shape round-trips with the
  marker trailing (fixture `"x"` -> `"x+"`, confirmed empirically via a live author+project run).
  No suffix fixture has been round-tripped yet (see the coverage table below), so a leading marker
  (`"+x"`) is this command's best-understood but UNCONFIRMED mirror of that same convention for a
  suffix; the expected character-table representations are the fixture's own declared phoneme
  representations; the expected XAMPLE `lex.txt` `\lx ` record count is one per authored morph
  (every affix subrule plus every lexical entry allomorph);
- the produced `AffixTemplate`'s slot order, traced back to the fixture's own `MorphemeId`s via each
  produced rule's `Gloss` (the only trace an authored rule carries forward -- HCLoader never lets a
  fixture force its own literal `MorphemeId` onto the HC engine's `Morpheme.Id`, see `author`'s own
  Gloss-from-MorphemeId fallback below), equals the fixture's declaration order or its exact reverse
  -- **see "Slot ordering" below for which, and why**; `adctl.txt`/`gram.txt` are asserted non-empty;
- loading the HC XML with `SIL.Machine.Morphology.HermitCrab.XmlLanguageLoader.Load` and parsing with
  `new Morpher(new TraceManager(), language)` (the exact construction `conformance/PROTOCOL.md`
  section 8 pins every fixture's `expected.tsv` against -- no `Morpher` property is ever assigned)
  reproduces, for every `--expect WORD=COUNT` the caller supplies, exactly COUNT analyses of WORD.
  **An analysis count is the one thing this command cannot derive from grammar.xml alone** -- only an
  oracle (or, for a fixture this tool owns outright, reasoning about its own designed semantics: see
  the `AllomorphCoOccurrenceRule` probe row below) knows the correct count for a word, so
  `verify-parity` refuses (exit 2, usage) if zero `--expect` flags are given rather than silently
  running the engine check over no words at all;
- **`--guid-map` is bound against the live authored project**, opened read-only via the same
  `LcmCache.CreateCacheFromExistingData` path `inspect`/`project` use (path read from the guid-map
  file's own `projectPath`, relative to that file's directory): every `guidMap` guid resolves to an
  object of the expected LCM class; the produced `AffixTemplate`'s `PrefixSlotsRS`/`SuffixSlotsRS`
  guid sequence equals the fixture's slot declaration order mapped through `guidMap`, per direction;
  each slot rule's `MoInflAffMsa.SlotsRC` contains exactly its own slot's guid; and each
  `LexicalEntry`'s `LexemeFormOA`/`AlternateFormsOS` guids match the documented allomorph ordering
  rule above. A `--guid-map` that resolves an id to the wrong object, a scrambled slot order, or a
  missing key is refused here, not silently accepted -- pinned by two deliberately-corrupted copies
  of the pilot's own `author-response.json` in `build.ps1 -Mode test` (swapped slot guids; an emptied
  `guidMap`), both asserted to exit 8.

On success it prints a JSON report (structural counts, `slotOrder`, `slotOrderingRule`,
`guidMapVerifiedCount`, engine counts) to stdout and exits 0; on the first mismatch it exits 8
naming that mismatch.

`--validate-capture <response.json>`: schema validation only, no FieldWorks install required.
Checks required fields are present for the response's own `mode` (`inspect`, `project`, or
`author`), `schemaVersion == 1`, every `sha256`-shaped field is 64 lowercase hex, `assemblyVersions`'s
keys are exactly the pinned set (when present), and no path field is absolute. This is the
portable-CI path.

## `author`'s supported subset

Refusal is always loud: exit 7, naming the offending element/attribute and its fixture id, decided
by a full validation pass over `grammar.xml` (`GrammarParser.cs`) **before** any LibLCM project is
created -- a refused grammar leaves no partial project on disk. This includes every check that once
lived inside `GrammarAuthor` and could only fire after `AuthorSession.Run` had already created and
locked a real `.fwdata` (a slot mixing prefix and suffix rules; a `MorphemeCoOccurrenceRule`/
`AllomorphCoOccurrenceRule` referencing an unknown id; an unsupported `adjacency` value):
`GrammarAuthor` itself refuses nothing, per its own doc comment. `GrammarParser.Parse` also tracks
every `id` attribute seen across the whole document and refuses the first one reused across element
kinds (naming both) -- the DTD types `id` as document-global (XML ID) but `DtdProcessing.Ignore`
means nothing else enforces that, and `alloFormMap`/`AuthorResult.GuidMap` are plain dictionaries
keyed by fixture id that would otherwise silently overwrite on a collision (`AuthorResult.Note`
throwing on a duplicate key is a defensive backstop for this, not the primary check).

Supported:
- `PartsOfSpeech/PartOfSpeech` -> `IPartOfSpeechFactory`.
- `CharacterDefinitionTable/SegmentDefinitions/SegmentDefinition` -> `PhPhonemeSet` + `IPhPhoneme`
  (one `PhCode` per `Representation`). `BoundaryDefinitions` whose sole representation is `+` or `#`
  -> `IPhBdryMarker` at the matching well-known guid (`LangProjectTags.kguidPhRuleMorphBdry` /
  `kguidPhRuleWordBdry`); any other boundary representation is refused. A `+` marker is always
  authored even when the fixture declares none -- `HCLoader.LoadCharacterDefinitionTable` indexes the
  character table by `"+"` unconditionally (`HCLoader.cs:2712`), so its absence crashes `project`
  with a `KeyNotFoundException` on ANY grammar, not just ones that use it.
- `NaturalClasses/SegmentNaturalClass` -> `IPhNCSegments`. A `FeatureNaturalClass` with no features,
  referenced ONLY as the "any stem" pattern (a single `OptionalSegmentSequence min="1" max="-1"` over
  it inside a `MorphologicalInput`) is NOT authored -- `HCLoader` represents an unconstrained affix
  input with its own built-in "any segment" pattern regardless of what LCM natural classes exist, so
  authoring one would be inert. Any other use of a `FeatureNaturalClass`, or one that declares
  features, is refused.
- Exactly one `Stratum` (more -> refused). `morphologicalRuleOrder`/`morphologicalRules` are recorded
  in `unmapped`, never authored -- FieldWorks orders rules via slots/templates, not a stratum list.
- `LexicalEntry` -> `ILexEntryFactory.Create(stem morph type, ..., SandboxGenericMSA{kStem, MainPOS})`.
  Multiple `Allomorph`s: the fixture's LAST allomorph becomes `LexemeFormOA`, every earlier one an
  `IMoStemAllomorphFactory`-built `AlternateFormsOS` entry, in order -- `HCLoader` emits
  `AlternateForms` before `LexemeForm`, so this ordering is what makes the round trip reproduce the
  fixture's own allomorph order. `ruleFeatures` -> `ProdRestrictOA` possibilities on
  `MoStemMsa.ProdRestrictRC`.
- A `MorphologicalRule` referenced by exactly one `AffixTemplate` slot -> a synthetic affix
  `LexEntry` (`MoInflAffMsa{MainPOS}`, via `SandboxGenericMSA`; `author` adds the produced slot to
  `MoInflAffMsa.SlotsRC` itself, in `CreateAffixTemplatesAndSlots`, once the slot exists -- the
  factory has no slot to add at MSA-construction time). Supported subrule shape: one
  `MorphologicalInput` matching the any-stem pattern above, and a `MorphologicalOutput` that is
  exactly `InsertSegments`+`CopyFromInput` (prefix) or `CopyFromInput`+`InsertSegments` (suffix) --
  anything else is refused. `RequiredEnvironments` -> `IPhEnvironment.StringRepresentation` rebuilt
  in FieldWorks syntax (`FieldWorksEnvironmentSyntax.cs`): natural-class contexts, literal segments,
  `#` word-boundary anchors, and parenthesized optionality are supported; anything else (raw
  `Segments`, `BoundaryMarker`) is refused. `requiredMPRFeatures` on a rule's subrules -> `FromProdRestrictRC`,
  but ONLY if identical across every subrule of that rule -- `MoInflAffMsa.FromProdRestrictRC` is one
  set shared by the whole affix, so subrules gated by genuinely different features (disjunctive
  allomorphy keyed by different MPR values) cannot be represented and are refused. `MPRFeatures` on a
  subrule's `MorphologicalOutput` is always refused: every rule this slice authors is, by
  construction, a slot/inflectional affix, and FieldWorks has no field for an inflectional affix to
  SET an MPR feature (only `MoDerivAffMsa`'s `ToProdRestrictRC` can, and derivational rules are out of
  scope here). A `MorphologicalRule` not referenced by any slot is refused (derivational back-out is
  a later expansion), and so is `requiredPartsOfSpeech != outputPartOfSpeech` on a slot rule.
- `AffixTemplate` -> `IMoInflAffixTemplate`; each `Slot` -> `IMoInflAffixSlot` in
  `pos.AffixSlotsOC`, added to `PrefixSlotsRS` if every referenced rule is a prefix, `SuffixSlotsRS`
  if every one is a suffix (mixed -> refused). See "Slot ordering" below for insertion order.
- `MorphemeCoOccurrenceRule`/`AllomorphCoOccurrenceRule` with `type="exclude"` -> `IMoMorphAdhocProhib`/
  `IMoAlloAdhocProhib` (`Adjacency` int per `HCLoader.GetAdjacency`, `HCLoader.cs:2241-2255`: 0
  anywhere, 1 somewhereToLeft, 2 somewhereToRight, 3 adjacentToLeft, 4 adjacentToRight).
  `isActive="no"` on one of these sets `.Disabled = true`; `type="require"`, or `isActive="no"`
  anywhere else in the document, is refused.
- `Gloss` -> the LexSense gloss FieldWorks' `HCLoader.GetGloss` reads; when the fixture gives none,
  the fixture's own `MorphemeId` is used as the gloss instead -- this is also what lets
  `verify-parity` trace a produced rule back to its fixture origin (see above), since HCLoader never
  lets a fixture force its own `MorphemeId` onto the HC engine's internal id.

Refused, always, naming the construct: `PhonologicalFeatureSystem`, `HeadFeatures`, `FootFeatures`,
`StemNames`, `Families`, `PhonologicalRuleDefinitions`, `SyntacticRules`, `RealizationalRule`,
`CompoundingRule`, `ExcludedEnvironments`, `partial="true"`, and any attribute referencing one of
those unsupported systems (`requiredStemName`, `requiredSubcategorizedRules`,
`outputObligatoryFeatures`, `family`, `subcategorizations`, `obligatoryHeadFeatures`/
`obligatoryFootFeatures`, ...).

**What the pilot fixture actually exercises vs. what's implemented but unexercised.**
`build.ps1 -Mode test`'s live `author`/`verify-parity` run is
`edge-cases/deep-optional-affix-nesting` -- 12 single-allomorph prefix rules, one lexical entry
with one allomorph, no environments, no co-occurrence rules. Passing that fixture is evidence for
the rows marked pilot below; the rest are exercised only by the smaller probes named beside them
(or, honestly, not at all yet):

| Construct | Exercised by |
|---|---|
| Single-allomorph `LexicalEntry`, single-subrule affix rules, slot ordering | pilot fixture |
| Multi-`Allomorph` `LexicalEntry` (`AlternateFormsOS` ordering) | implemented (`CreateStemEntries`), no live `author`/`verify-parity` fixture yet |
| `RequiredEnvironments` / `FieldWorksEnvironmentSyntax` string building | implemented, no live fixture yet |
| `MorphemeCoOccurrenceRule type="exclude"` | implemented, no live fixture yet |
| `AllomorphCoOccurrenceRule type="exclude"` | `testdata/allomorph-cooccurrence-probe.grammar.xml` (`build.ps1 -Mode test`'s own probe, not a `machine` conformance fixture; `verify-parity --expect k=1 --expect xk=0` additionally proves the exclusion binds in the live HC engine, not just that an `IMoAlloAdhocProhib` object exists -- these two counts follow from the probe's own designed semantics (one optional prefix slot, excluded "anywhere" from the sole lexical entry), not an oracle, since this fixture is not a `machine` conformance fixture) |
| `verify-parity --guid-map` binding (class, slot order, `SlotsRC`, allomorph order) | pilot fixture (positive case) + two deliberately-corrupted copies of its own `author-response.json` (negative cases, exit 8) |

None of the "no live fixture yet" rows are refused -- `GrammarParser`/`GrammarAuthor` accept them --
they simply have no live `author`+`verify-parity` round trip pinning them the way the pilot fixture
does the rest.

`ParserParameters` is generated, never read from the fixture:
```
<ParserParameters><ActiveParser>XAmple</ActiveParser><XAmple><MaxNulls>0</MaxNulls>
<MaxPrefixes>{N}</MaxPrefixes><MaxInfixes>0</MaxInfixes><MaxRoots>1</MaxRoots>
<MaxSuffixes>{N}</MaxSuffixes><MaxInterfixes>0</MaxInterfixes><MaxAnalysesToReturn>{M}</MaxAnalysesToReturn>
</XAmple><HC><NoDefaultCompounding>true</NoDefaultCompounding>
<AcceptUnspecifiedGraphemes>false</AcceptUnspecifiedGraphemes></HC></ParserParameters>
```
`N` defaults to `max(5, total slot count across all AffixTemplates)`, overridable with
`--xample-max-prefixes` (the one value feeds both `MaxPrefixes` and `MaxSuffixes` -- there is no
separate suffix knob); `M` defaults to 1000, overridable with `--xample-max-analyses`.

Vernacular writing system defaults to `en` (`--vernacular-ws` overrides); analysis is always `en`.
`CreateCacheWithNewBlankLangProj`'s bootstrap only ever adds the ANALYSIS default to
`CurrentAnalysisWritingSystems` -- the vernacular default is never added to the base
`VernacularWritingSystems` collection by anything in the bootstrap path, so `author` adds it itself
(mirroring `HCLoaderTests.cs:129`'s identical fixup); skipping this makes a LATER `project`/`inspect`
reopen of the authored project crash with a `NullReferenceException` inside
`BackendProvider.BootstrapExtantSystem`, since that method reads `LangProject.VernWss`
unconditionally. Commits are asynchronous (`XMLBackendProvider.Commit` only enqueues); `author` calls
`IUndoStackManager.Save()` (the same public flush `ProjectLockingService`/`ProjectBackupService`
use) before computing `projectSha256`, since `Dispose()` alone was measured leaving a bare
80-byte skeleton `.fwdata` on disk.

## Slot ordering

`author` adds slots to `IMoInflAffixTemplate.PrefixSlotsRS`/`.SuffixSlotsRS` in the fixture's own
declaration order (its own convention: innermost-to-outermost for a prefixing template -- confirmed
against `edge-cases/deep-optional-affix-nesting`'s own comment and oracle-verified `words.yaml`).
`HCLoader.LoadLanguage` then builds the loaded `AffixTemplate.Slots` list as
`template.SuffixSlotsRS.Concat(template.PrefixSlotsRS.Reverse())` (`HCLoader.cs:297`) -- confirmed
independently against `HCLoaderTests.cs`'s own `AffixTemplate` test (:729-768): there,
`PrefixSlotsRS` is built as `[prefixSlot1 (added 1st), prefixSlot2 (added 2nd)]`, yet the loaded
`Language.Strata[0].AffixTemplates[0].Slots` comes back as `[suffixSlot, prefixSlot2, prefixSlot1]`
-- suffix slots first (their own `SuffixSlotsRS` order), then prefix slots in the EXACT REVERSE of
their `PrefixSlotsRS` insertion order.

So `author`ing the pilot fixture's 12 declared prefix slots `slot1..slot12` (mrP1..mrP12) in
declaration order reproduces, via that reversal, the outward highest-slot-first order
`words.yaml`'s own oracle-confirmed analyses already expect (e.g. `xxxxxxk`'s
`P10+P5+P4+P3+P2+P1+K` rule chain lists the fired slots in descending P-number) with NO reversal on
`author`'s own side -- `verify-parity`'s `slotOrder`/`slotOrderingRule` fields report this directly
from a live run rather than assuming it.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | ok |
| 2 | usage (also `verify-parity` given zero `--expect WORD=COUNT` flags, or one not shaped `WORD=COUNT`) |
| 3 | pin mismatch (an assembly/native DLL under the FieldWorks install does not match the pinned file version) |
| 4 | project open failure (missing file, locked by another application, needs FLEx migration, or the target `author` path already exists) |
| 5 | projection failure (HCLoader, XmlLanguageWriter, or an XAMPLE transform failed) |
| 6 | capture validation failure |
| 7 | author refusal (a grammar.xml construct is outside the supported subset -- see above; also `author.unconsumed-construct`, the reconciliation guard naming a parsed construct that never landed in `guidMap`) |
| 8 | parity mismatch (`verify-parity` found the first structural or HC-engine divergence from the fixture) |

## Build and run

```powershell
& .\tools\xample-projector\build.ps1 -Mode check   # restore + build only
& .\tools\xample-projector\build.ps1 -Mode test    # build, --validate-capture every checked-in
                                                    # testdata capture, and (whichever of these are
                                                    # reachable) a live 'project' run against a
                                                    # throwaway copy of Sena 3; two GrammarParser
                                                    # refusal-before-project-exists probes (needs
                                                    # FieldWorks only, no submodule); the
                                                    # AllomorphCoOccurrenceRule authoring + engine
                                                    # probe (also needs no submodule); PLUS a live
                                                    # 'author' -> 'project' -> 'verify-parity' run
                                                    # against the machine submodule's pilot fixture
                                                    # (guid-map binding, two guid-map corruption
                                                    # probes, normalized-.fwdata author determinism,
                                                    # and both construct-refusal probes too)
```
`build.ps1` locates MSBuild via `vswhere.exe` (preferring the Visual Studio toolchain this project
was built against) and falls back to `dotnet build` if MSBuild is unavailable.

FieldWorks install directory: `$env:PANGLOSS_FIELDWORKS_DIR`, default
`C:\Program Files\SIL\FieldWorks 9`. Sample-project directory for the Sena 3 live test:
`$env:PANGLOSS_FW_PROJECTS_DIR`, default `<FieldWorks source checkout>\DistFiles\Projects`. The
pilot-fixture `author`/`verify-parity` live tests instead read the `machine` git submodule at this
repo's own root (`machine\conformance\edge-cases\deep-optional-affix-nesting\grammar.xml` and two
fixtures under `machine\conformance\languages\` for the construct-refusal probes) and are skipped,
independently of the Sena 3 tests, if that submodule isn't initialized. The two
`GrammarParser`-refusal probes (unknown co-occurrence id; duplicate id) and
`testdata\allomorph-cooccurrence-probe.grammar.xml` (this tool's own fixture, not part of the
`machine` submodule) need no submodule at all, and run whenever FieldWorks itself is present --
independently of both the Sena 3 tests and the pilot-fixture tests.

## Pinned versions

This tool was built and verified against FieldWorks 9.3.10 (net48, x64). At startup (every mode
except `--validate-capture`) it checks the following file versions under the FieldWorks
directory and refuses (exit 3) on any mismatch, printing both versions:

| File | Expected file version |
|---|---|
| `ParserCore.dll` | 9.3.10.26161 |
| `SIL.LCModel.dll` | 11.0.0.55167 |
| `SIL.Machine.dll` | 3.8.2.0 |
| `SIL.Machine.Morphology.HermitCrab.dll` | 3.7.4.0 |
| `XAmpleManagedWrapper.dll` | 9.3.10.26161 |
| `xample64.dll` | 3.12.23.21 |

## Runtime probing

This tool references FieldWorks assemblies by `HintPath` with `Private=false` (it never copies
FieldWorks's 100+ DLLs into its own output). At runtime it widens the native DLL search path with
`SetDllDirectory` (so ICU, and in a later slice `xample.dll`, resolve) and registers an
`AppDomain.AssemblyResolve` handler that loads a missing managed assembly by simple name from the
FieldWorks directory. Both are set up in `Program.Main` before any FieldWorks type is touched.

## Provenance

`ProjectIdentifier.cs`, `NullFdoDirectories.cs`, `NullThreadedProgress.cs`, and
`DiagnosticLogger.cs` are adapted from FieldWorks's own headless tool,
`Src\GenerateHCConfig\{ProjectIdentifier,NullFdoDirectories,NullThreadedProgress,ConsoleLogger}.cs`.
`XampleProjection.cs` replicates the XSL/GAFAWS driving sequence from
`Src\LexText\ParserCore\M3ToXAmpleTransformer.cs` and `XAmpleParser.cs` (both drive an `internal`
class this tool cannot reference directly), parameterized to write into a caller-chosen directory
instead of the hardcoded `%TEMP%` the original always uses.

`GrammarParser.cs`/`GrammarModel.cs` (validation), `GrammarAuthor.cs`/`AuthorSession.cs`/
`FieldWorksEnvironmentSyntax.cs` (LibLCM object construction), `AuthorCommand.cs`, and
`VerifyParityCommand.cs` are this slice's own code, built against the factory-call shapes
`Src\LexText\ParserCore\ParserCoreTests\HCLoaderTests.cs` exercises (that file constructs LCM
objects programmatically the same way a real project does) and checked against `HCLoader.cs`
itself and its own `SIL.Machine.Morphology.HermitCrab` XML writer/loader for the two facts a test
file alone can't settle: the slot-reversal behavior (see "Slot ordering" above) and the mandatory
`"+"` character (see the supported-subset list above).

## Follow-ups

Not addressed in this pass: splitting `GrammarParser`/`GrammarModel` into a table-driven parser
(rather than the current sequence of hand-written element/attribute checks), and splitting the
larger files (`GrammarParser.cs`, `GrammarAuthor.cs`) along construct-kind boundaries.
