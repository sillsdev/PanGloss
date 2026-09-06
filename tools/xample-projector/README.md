# XampleProjector

A version-pinned managed helper that opens a FieldWorks 9 project via LibLCM and produces the
same two real FieldWorks projections of it that PanGloss needs to measure XAMPLE-to-HC migration
differences: the HC XML (`HCLoader` + `XmlLanguageWriter`) and the XAMPLE control/dictionary/
grammar files (the same XSL transforms and GAFAWS step `M3ToXAmpleTransformer`/`XAmpleParser`
drive internally, replicated here because that class is `internal`). Both sides read the same
opened `LcmCache`, so they can never diverge on which project state they saw.

The tool exposes six subcommands. `inspect` (read-only phonology survey), `project` (full HC +
XAMPLE projection), and `--validate-capture` (portable, FieldWorks-free schema check) are the core
projection path. `author` backs a HermitCrab conformance fixture (`grammar.xml`) out into a
brand-new FieldWorks project via LibLCM, refusing every construct outside a documented supported
subset; `verify-parity` is a structural + HC-engine proof that `project` on an authored project
reproduces the fixture it came from. `mutate` applies a scripted LibLCM phoneme-inventory
counterfactual to a CLONE of a project, never the source; `parse` drives the real XAmple engine
(`XAmpleManagedWrapper`/`xample.dll`) over a project's already-generated XAMPLE files.

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
  suffix -- a match against a suffix subrule is never silently reported as a confirmed pass: when
  ANY suffix subrule is checked, the response's `insertConventionConfirmed` is `false` (`true`
  when every checked subrule is a prefix) and a console line names the gap; the expected
  character-table representations are the fixture's own declared phoneme representations; the
  expected XAMPLE `lex.txt` `\lx ` record count is one per authored morph (every affix subrule
  plus every lexical entry allomorph);
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

`mutate --project <path-to-.fwdata> --request <request.json> --out-dir <dir>` applies a scripted
LibLCM phoneme-inventory counterfactual to a CLONE of the project, never the source. Request
(versioned):
```
{ "schemaVersion": 1, "caseId": "...", "baseSha256": "<64 hex of the source .fwdata>",
  "operations": [
    {"op":"remove_phoneme","guid":"...","assertRepresentations":["x"],"requireUnreferenced":true},
    {"op":"remove_all_phonemes","requireUnreferenced":true}
  ] }
```
In order:
1. Hashes the source `.fwdata` and compares it to `baseSha256` (mismatch: exit 10
   `mutation.source-digest-mismatch`).
2. Clones the WHOLE project directory (including `WritingSystemStore`) to a fresh directory under
   `--out-dir`; every later step touches only the clone. At the very end, the source is hashed
   again and compared to the first hash (mismatch: exit 10 `mutation.source-modified`).
3. Resolves each `remove_phoneme` guid exactly once against the clone's first phoneme set (missing:
   exit 9 `mutation.unknown-target`; the same guid named twice across operations: exit 9
   `mutation.duplicate-target`; `assertRepresentations` mismatched against the phoneme's own
   `CodesOS[*].Representation`, in order: exit 9 `mutation.representation-mismatch`).
   `remove_all_phonemes` targets every phoneme in that same set. Zero resolved targets: exit 9
   `mutation.no-targets`.
4. For every target whose operation set `requireUnreferenced`, walks `ICmObject.ReferringObjects`;
   any referrer (of any kind -- natural classes, environments, rules, ...) refuses the WHOLE
   request (exit 9 `mutation.referenced-phoneme`, naming each referrer's class, guid, and
   `ShortName`) and deletes nothing. A referrer on a target whose op did NOT set
   `requireUnreferenced` does not block, but is still recorded in the response's
   `inboundReferences` for transparency.
5. Deletes every target inside one `NonUndoableUnitOfWorkHelper.Do`, flushes
   (`IUndoStackManager.Save()`), disposes the cache, then REOPENS the clone and verifies by
   re-reading it: every target guid is gone and the phoneme-set count dropped by exactly the number
   deleted (else exit 10 `mutation.deletion-unverified`).

Response `mutation-response.json`:
```
{ "schemaVersion": 1, "mode": "mutate", "caseId": "...", "baseSha256": "<64 hex>",
  "materializedSha256": "<64 hex, of the clone's .fwdata after reopen>",
  "materializedProjectPath": "<relative to --out-dir>",
  "removed": [{"guid": "...", "representations": ["..."]}],
  "inboundReferences": [{"targetGuid": "...", "referrerGuid": "...", "referrerClass": "..."}],
  "reopened": true, "deletedCount": 0, "diagnostics": [] }
```
`--validate-capture` accepts `mode: "mutate"`.

### Comparing XAMPLE outputs across a mutation: never byte-identical, and that's expected

Projecting a base project and its phoneme-mutated clone (`project` run twice, once per project)
never produces byte-identical `adctl.txt`/`gram.txt`/`lex.txt`: every object the M3 dump writes is
named by its raw LibLCM **hvo**, and deleting one phoneme record shifts every later object's hvo
within that clone's own load session. This is baked in by a FieldWorks XSL parameter, not something
this tool introduces: `Src\Transforms\Application\FxtM3ParserToXAmpleADCtl.xsl:39` sets
`bMorphnameIsMsaId` to `y` by default, and every place that parameter is read
(`FxtM3ParserToXAmpleADCtl.xsl:217,268,284,305`) writes the referenced MSA's own hvo (`@dst`/`@Id`)
verbatim instead of its gloss text; `FxtM3ParserToXAmpleLex.xsl`'s `\lx`/`\wc`/`\a ... {}` fields and
`FxtM3ParserToToXAmpleGrammar.xsl`'s rule bodies do the same unconditionally. The hvo itself comes
from `SIL.LCModel.DomainServices.M3ModelExportServices.cs`, where every M3-dump `Id`/`dst` attribute
is written as `object.Hvo` -- a plain in-memory sequence number LibLCM assigns per load session, not
a stable identifier.

So **a base-vs-mutant XAMPLE-file comparison must be hvo-blind, or must go through `parse`'s own
`msaGuid`** (a real LCM guid, stable across a file-copy clone even though the hvo it is resolved
from is not -- see `parse`'s own doc below), never raw byte-identity:
- `build.ps1`'s `Get-HvoBlindLines`/`Test-XampleFilesEquivalentIgnoringHvoRenumbering` blind only
  the token shapes measured to actually carry an hvo (an identifier+hvo token from the closed
  `XAmpleTemplateVariables.xsl` prefix set, an `(hvo_slotIndex)` pair, `\lx`/`\wc`/`\a ... {hvo}` in
  `lex.txt`, and the like) -- never every digit, which would also erase a real regression in a
  `\maxp`/`\maxs`/`\maxi`/`\maxr`/`\maxn`/`\maxnull` cap or a count.
- `parse`'s own multiset comparison (`Get-AnalysisSignatures`, keyed on `msaGuid`) is the
  guid-based cross-check: it is verified for both `mutate` live-proof cases
  (`remove-k-only`, `empty-phoneme-inventory`).

`parse --project <path-to-.fwdata> --project-dir <dir with <db>adctl.txt/gram.txt/lex.txt>
--database <name> --words <file, one per line> --out <response.json> [--max-analyses N]
[--max-prefixes N] [--max-suffixes N] [--max-infixes N] [--max-roots N] [--max-interfixes N]
[--max-nulls N]` drives the real XAmple engine (`XAmpleManagedWrapper.XAmpleWrapper` ->
`xample.dll`, resolved via the `SetDllDirectory` search path `Program.Main` already sets up) over a
project's already-generated XAMPLE files. `--project` is required, not merely accepted: parsing a
`Morph`'s `MoForm`/`MSI` `DbRef` hvo the way
`SIL.FieldWorks.WordWorks.Parser.XAmpleParser.ProcessParseResults`/`TryCreateParseMorph` do
(`Src\LexText\ParserCore\XAmpleParser.cs:178-331`) needs a live `ICmObjectRepository`, and an hvo is
only stable within the ONE `LcmCache` session that assigned it -- so every morph is reported by its
LCM **guid** (`msaGuid`), not the bare hvo the XML carries, and that guid is what makes a parse
against one project comparable to a parse against a cloned/mutated copy of it.

**Every `<XAmple>` cap except `MaxAnalysesToReturn` is baked into `adctl.txt`'s own `\maxp`/`\maxi`/
`\maxs`/`\maxr`/`\maxn`/`\maxnull` control lines at author/project time**
(`FxtM3ParserToXAmpleADCtl.xsl:130-134` for `\maxp`/`\maxi`/`\maxs`/`\maxr`/`\maxn`; `\maxnull` is
written separately, at line 117) -- confirmed by reading
`XAmpleManagedWrapper.XAmpleDLLWrapper.SetParameter(name, value)`: it special-cases ONLY
`"MaxAnalysesToReturn"` and silently drops every other name, so the native engine truly has no
runtime knob for the rest. `--max-prefixes`/`--max-suffixes`/`--max-infixes`/`--max-roots`/
`--max-interfixes`/`--max-nulls`, when given, therefore patch a COPY of `<db>adctl.txt` (never the
caller's own file) before `LoadFiles` -- measured live: dropping `--max-prefixes` from 12 to 3
against the pilot fixture changes `xxxxxxk`'s analysis count from 924 to 1, proving the patch is
read, not merely accepted. `--max-analyses` is the one genuine runtime override
(`SetParameter("MaxAnalysesToReturn", ...)`); its default is `1000` when not given (this tool's own
convention, matching `author`'s default), since XAmple's own unset default (20) exists to serve
interactive FLEx, not a batch tool.

The response's `parameters` keeps these two provenances apart rather than flattening them into
sibling keys: `runtime` is the one cap `SetParameter` actually honours; `adctl` is read back from
whichever `adctl.txt` was actually loaded (the caller's own file, or a patched temp copy -- see
above); `adctlPatched` says which; `adctlSource` names that file's path, relative to
`--project-dir` (so a patched run's path runs through the temp directory named above, and an
unpatched run's does not).

Response:
```
{ "schemaVersion": 1, "mode": "parse", "database": "...",
  "engineVersion": "<xample64.dll file version -- AmpleReportVersion is never exposed by the public managed wrapper surface>",
  "parameters": {
    "runtime": { "maxAnalysesToReturn": 1000 },
    "adctl": { "maxPrefixes": 5, "maxSuffixes": 5, "maxInfixes": 0, "maxRoots": 1, "maxInterfixes": 0, "maxNulls": 0 },
    "adctlPatched": false,
    "adctlSource": "MPBaseadctl.txt"
  },
  "words": [ { "word": "...", "analyses": [ { "morphemes": [ { "form": "...", "msaGuid": "...", "morphnameOrGloss": "...", "type": "..." } ], "categoryId": null, "surfaceNfd": "..." } ], "reachedMaxAnalyses": false, "engineError": null } ] }
```
`analyses` is a MULTISET -- duplicate-looking entries (same surface text, same morph list) are two
genuinely distinct analyses (e.g. two different subsets of optional slots that happen to fire the
same rules) and are never deduplicated. `reachedMaxAnalyses` is read from the engine's own
`<Exception code="ReachedMaxAnalyses">`; any other engine exception or `<Error>` element is
recorded verbatim in `engineError` and parsing continues with the next word. `LoadFiles`/`Init`
failure (a missing `cd.tab`/`adctl.txt`/`gram.txt`/`lex.txt`, or a native load error) exits 11,
naming the missing file.

`--validate-capture <response.json>`: schema validation only, no FieldWorks install required.
Checks required fields are present for the response's own `mode` (`inspect`, `project`, `author`,
`mutate`, or `parse`), `schemaVersion == 1`, every `sha256`-shaped field is 64 lowercase hex,
`assemblyVersions`'s keys are exactly the pinned set (when present), and no path field is absolute.
This is the portable-CI path.

## `author`'s supported subset

Refusal is always loud: exit 7, naming the offending element/attribute and its fixture id, decided
by a full validation pass over `grammar.xml` (`GrammarParser.cs`) **before** any LibLCM project is
created -- a refused grammar leaves no partial project on disk. This includes every check that once
lived inside `GrammarAuthor` and could only fire after `AuthorSession.Run` had already created and
locked a real `.fwdata` (a slot mixing prefix and suffix rules; a `MorphemeCoOccurrenceRule`/
`AllomorphCoOccurrenceRule` referencing an unknown id; an unsupported `adjacency` value):
`GrammarAuthor` itself refuses nothing (`AuthorResult.Note`'s duplicate-key check throws
`InvalidOperationException("unreachable: ...")`, a defect signal, never a
`GrammarAuthorException` -- see `build.ps1`'s duplicate-id and duplicate-slot-name refusal probes
below). `GrammarParser.Parse` tracks every DTD `id` attribute, every `AffixTemplate`/`Slot` `Name`
text, and every synthetic `morphemeCoOccurrence[i]`/`allomorphCoOccurrence[i]` key in one
document-global dictionary, and refuses the first one reused across kinds (naming both) -- every
string `AuthorResult.GuidMap` (a plain dictionary keyed by fixture id/Name/synthetic key) will
ever be keyed by is proven unique here before any project exists. Two `AffixTemplate`s each
declaring a `Slot` with the same `Name` (legal by the DTD, since slot names are template-local in
HermitCrab but `GuidMap` flattens them into one namespace) is refused the same way as a duplicate
`id`.

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
| 9 | mutation refusal (`mutate` cannot proceed with this request/target shape -- unknown/duplicate target, representation mismatch, no targets, or a still-referenced phoneme; nothing is deleted) |
| 10 | mutation integrity failure (the source digest didn't match, the source changed, or the reopened clone didn't verify the deletion) |
| 11 | parse engine load failure (`LoadFiles`/`Init` failed, or a required XAMPLE/`cd.tab` file is missing -- names the file) |

## Build and run

```powershell
& .\tools\xample-projector\build.ps1 -Mode check   # restore + build only
& .\tools\xample-projector\build.ps1 -Mode test    # build, --validate-capture every checked-in
                                                    # testdata capture, and (whichever of these are
                                                    # reachable) a live 'project' run against a
                                                    # throwaway copy of Sena 3; three GrammarParser
                                                    # refusal-before-project-exists probes (unknown
                                                    # co-occurrence id; duplicate id; duplicate
                                                    # slot name across two AffixTemplates -- needs
                                                    # FieldWorks only, no submodule); the
                                                    # AllomorphCoOccurrenceRule authoring + engine
                                                    # probe (also needs no submodule); PLUS a live
                                                    # 'author' -> 'project' -> 'verify-parity' run
                                                    # against the machine submodule's pilot fixture
                                                    # (guid-map binding, two guid-map corruption
                                                    # probes, normalized-.fwdata author determinism,
                                                    # and both construct-refusal probes too); PLUS a
                                                    # live 'mutate' (remove-k-only, empty-phoneme-
                                                    # inventory, a referenced-phoneme refusal probe,
                                                    # and a corrupted-copy negative probe proving the
                                                    # hvo-only blind still catches a corrupted cap)
                                                    # + 'parse' (real XAmple engine; adctl-untouched
                                                    # + adctlPatched true/false + msaGuid multiset
                                                    # checks for both mutate cases) run against a
                                                    # freshly authored pilot project (a separate
                                                    # 'C:\Users\johnm\...\machine' checkout, not the
                                                    # submodule above -- see below)
```
`build.ps1` locates MSBuild via `vswhere.exe` (preferring the Visual Studio toolchain this project
was built against) and falls back to `dotnet build` if MSBuild is unavailable.

FieldWorks install directory: `$env:PANGLOSS_FIELDWORKS_DIR`, default
`C:\Program Files\SIL\FieldWorks 9`. Sample-project directory for the Sena 3 live test:
`$env:PANGLOSS_FW_PROJECTS_DIR`, default `<FieldWorks source checkout>\DistFiles\Projects`. The
pilot-fixture `author`/`verify-parity` live tests instead read the `machine` git submodule at this
repo's own root (`machine\conformance\edge-cases\deep-optional-affix-nesting\grammar.xml` and two
fixtures under `machine\conformance\languages\` for the construct-refusal probes) and are skipped,
independently of the Sena 3 tests, if that submodule isn't initialized. The three
`GrammarParser`-refusal probes (unknown co-occurrence id; duplicate id; duplicate slot name across
two `AffixTemplate`s) and `testdata\allomorph-cooccurrence-probe.grammar.xml` (this tool's own fixture, not part of the
`machine` submodule) need no submodule at all, and run whenever FieldWorks itself is present --
independently of both the Sena 3 tests and the pilot-fixture tests.

The `mutate`/`parse` live proof reads a THIRD, independent `machine` checkout --
`$env:PANGLOSS_MACHINE_DIR`, default `C:\Users\johnm\Documents\repos\machine` -- rather than this
repo's own submodule (another agent commits to that checkout concurrently; this tool only ever
reads its working tree). Skipped (with reason) only when FieldWorks itself is absent, or when
`conformance\edge-cases\deep-optional-affix-nesting\grammar.xml` isn't found at that path; it
separately tries a short list of `conformance\edge-cases\*` fixtures with a `SegmentNaturalClass`
(`disjunctive-recheck`, `free-fluctuating-allomorph-pair`, `strrep-identity`, `diacritic-segments`,
`loader-pattern-shapes`, in that order) for its referenced-phoneme refusal probe, using the first
one that authors successfully and reporting which.

**Base project: the checked-in FieldWorks witness, when present, else a fresh `author` run.**
A real, oracle-adjacent FieldWorks project is committed alongside the pilot fixture itself --
`machine\conformance\edge-cases\deep-optional-affix-nesting\fieldworks\project.fwdata` (+
`fieldworks\WritingSystemStore\*.ldml`) and `fieldworks\phonology-mutations.yaml`, a small versioned
manifest of the same two phoneme-removal cases this proof runs (see
`machine\conformance\PROTOCOL.md` section 10 for the manifest's own vocabulary and eligibility
rules; `build.ps1`'s `ConvertFrom-PhonologyMutationsYaml` refuses any `version:` other than `1`,
naming the version found and the version supported). When that file exists, `build.ps1`:
1. Reads `phonology-mutations.yaml` (a bespoke, fixed-shape parser -- `ConvertFrom-
   PhonologyMutationsYaml` -- not a general YAML reader; the Rust side owns YAML later) and verifies
   its `base_sha256` against the ACTUAL sha256 of `project.fwdata`. A mismatch fails the run loudly,
   naming both hashes, rather than silently trusting a manifest that no longer describes the project
   beside it.
2. Copies `fieldworks\` (never the machine checkout's own copy -- opening a project can leave
   session artifacts beside it even read-only, as this same witness's own first `inspect` did) into
   a scratch directory and uses that copy as the base project for every later step, in place of a
   freshly authored one.
3. Builds each case's `mutate` request JSON directly from the manifest's own operations (so a
   manifest edit changes what this proof runs, rather than the request staying hardcoded beside a
   manifest nobody reads) and additionally asserts `expect.inferred_segments` against the mutation's
   own `removed[]` -- exercising that field rather than leaving it as dead documentation. `mutate`
   itself refuses a request whose `schemaVersion` isn't `1`.
4. Runs ONE drift probe: freshly `author`s the SAME `grammar.xml` and asserts it normalizes
   identically to the checked-in witness -- the SAME `Get-NormalizedFwdataText` the determinism
   harness above uses, but with each side's guid-to-label map built independently from that side's
   OWN `.fwdata` content (`Get-ContentDerivedGuidLabelMap`), since the witness's own `guidMap` was
   never committed (it isn't part of the checked-in file set, see PROTOCOL.md section 10) and a
   shared or empty map on both sides cannot: a record's own `Name`/`Gloss`/`Form`/`Representation`/
   `StringRepresentation`/`Abbreviation` text becomes its label (e.g. a `MoInflAffixSlot` renders as
   `MoInflAffixSlot:slot3`, not a blind `GUID`), so which slot a rule targets, which class a phoneme
   belongs to, which POS a rule requires, and which LexEntry (via its own `LexSense` gloss) an
   allomorph belongs to all survive normalization -- a wiring-only difference between two otherwise
   identical fragments (e.g. two affix rules' target slots swapped) makes the normalized text
   differ, where blanking everything to `GUID` could not. A record with none of those fields (e.g.
   `MoInflAffMsa`, `MoAffixAllomorph`) instead borrows its resolved owner's label (via the owning
   `LexEntry`), which is enough to distinguish two allomorphs on the same entry by content instead of
   refusing outright.

   Two record classes GrammarAuthor DOES create several of, but whose shared owner
   (`MoMorphData`/`MorphologicalDataOA`, a project-global singleton GrammarAuthor never names) has no
   label of its own to borrow, get a THIRD fallback: `MoMorphAdhocProhib`/`MoAlloAdhocProhib`
   (co-occurrence rules) derive a candidate from their OWN referenced content (which morpheme or
   allomorph they exclude), computed once every reference they contain has itself settled. Without
   this, two structurally-identical co-occurrence rules differing only in WHICH morpheme/allomorph
   they target normalized to the same multiset -- neither record had its own identity, so
   `Get-NormalizedFwdataText`'s final sort could not tell "rule A excludes X, rule B excludes Y" from
   "rule A excludes Y, rule B excludes X". This fallback is a narrow, explicit allowlist
   (`$script:OwnContentLabelClasses`), not a blanket rule for every unowned record: applying it
   generally to FieldWorks' OWN scaffold objects (below) turned their harmless, expected variation
   between independently created projects into a false witness-drift failure, which is exactly the
   failure mode a scoped allowlist avoids.

   Label uniqueness is PROVEN, not assumed: `XampleProjector.exe check-label-uniqueness` refuses (the
   same `IdRegistry.Register` guarded insert `GrammarParser` uses for grammar.xml id/Name uniqueness)
   if two guids would render identically. When two records land on the identical candidate label
   (e.g. two allomorphs on one entry, or two co-occurrence rules sharing the same unlabeled owner),
   the tie is broken by each record's own content signature (`Get-RecordSignature`) and, for a
   GENUINE content tie (two records with byte-identical own content -- interchangeable by
   construction), by that record's position in the project's own `.fwdata` document order -- never by
   which guid happens to enumerate first out of a Hashtable, which depends on the guid VALUES
   themselves and need not agree between two projects whose equivalent records carry different real
   guids.

   **Content-derived labeling: known residual blind spot.** A record whose class is not on the
   `$script:OwnContentLabelClasses` allowlist AND whose owner also has no label of its own is left
   unlabeled and falls through to the existing blanket `GUID` blind. Measured against the pilot
   fixture's own checked-in witness (151 records total): 37 stay unlabeled, and every one is
   FieldWorks' own scaffold -- objects GrammarAuthor never creates and never gives any distinguishing
   content -- `CmPossibilityList` (19), `CmAnnotationDefn` (4), `CmPossibility` (3), `CmAgent` (3),
   `FsFeatureSystem` (2), and one each of `PhPhonemeSet`, `PhPhonData`, `MoMorphData`, `RnResearchNbk`,
   `LexDb`, `LangProject` (the last five are project-global singletons; there is only ever one of
   each, so nothing needs telling apart). This is harmless PRECISELY because GrammarAuthor never
   distinguishes between two instances of any of these classes -- there is no wiring GrammarAuthor
   could ever assign differently between them for a swap to make visible.

   `build.ps1 -Mode test` runs three permanent negative probes proving each of the above by effect,
   not by message:
   - `Test-ContentDerivedLabelCatchesWiringSwap` swaps two affix rules' target-slot guids in a copy
     of a genuine project and asserts the comparison now reports a difference (with a control
     confirming the OLD blanket-`GUID` blind stays blind to the identical perturbation).
   - `Test-ContentDerivedLabelCatchesCoOccurrenceSwap` does the same for two `MoAlloAdhocProhib`
     records' excluded-allomorph reference (`testdata\allomorph-cooccurrence-probe.grammar.xml` was
     extended with a second, independent rule pair so the fixture has two such records to swap
     between).
   - A guid-rename probe (needs no FieldWorks project at all) constructs a genuine content tie
     between two synthetic records and asserts that renaming every guid in the file to a fresh value,
     without moving any element's on-disk position, still resolves the tie to the same `#0`/`#1`
     assignment -- run against the PRE-fix tie-break, the identical rename measurably flips that
     assignment about a third of the time (Hashtable enumeration order tracks the guid strings, which
     the rename changes; document-order tracks the file, which it does not).

   So the witness cannot silently drift from what `author` actually produces, a wiring-only
   regression cannot silently pass as a literal-text match, and the map's own tie-break cannot
   silently disagree between two projects that differ only in which random guids LCM happened to
   assign.

When the witness file is absent (an older `machine` checkout, or a partial one), `build.ps1` prints
`SKIPPED (checked-in FieldWorks witness): ...` naming the expected path and falls back to authoring
`conformance\edge-cases\deep-optional-affix-nesting\grammar.xml` fresh, as before.

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
`SetDllDirectory` (so ICU and `xample.dll`, loaded by `parse` via `XAmpleManagedWrapper`, both
resolve) and registers an `AppDomain.AssemblyResolve` handler that loads a missing managed assembly
by simple name from the FieldWorks directory. Both are set up in `Program.Main` before any
FieldWorks type is touched.

`FwRegistryHelper.Initialize`/`FwUtils.InitializeIcu`/`Sldr.Initialize` are process-global, one-shot
calls -- `Sldr.Initialize` itself throws on a second call in the same process. Every prior command
opened at most one `LcmCache` per process, so this was never exercised until `mutate`, which opens
the clone once to mutate it and once more to reopen-and-verify. `FieldWorksBootstrap.EnsureInitialized`
guards all three behind a static flag; `FieldWorksSession.Run` and `AuthorSession.Run` both call it
instead of the three calls directly.

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

`MutateCommand.cs`/`MutationException.cs` (LibLCM phoneme deletion, built against the
`ReferringObjects`/`NonUndoableUnitOfWorkHelper`/`IUndoStackManager.Save` shapes documented in
`SIL.LCModel.DomainServices.PhonologyServices.DeletePhonology`) and `ParseCommand.cs` (drives
`XAmpleManagedWrapper.XAmpleWrapper`, built against the real usage pattern in
`Src\LexText\ParserCore\XAmpleParser.cs`) are this slice's own code. `FieldWorksBootstrap.cs`
factors the one-shot `FwRegistryHelper`/`FwUtils`/`Sldr` initialization out of `FieldWorksSession.cs`
and `AuthorSession.cs` so a single process can open more than one `LcmCache` (see "Runtime probing").

## Follow-ups

Not addressed in this pass: splitting `GrammarParser`/`GrammarModel` into a table-driven parser
(rather than the current sequence of hand-written element/attribute checks), and splitting the
larger files (`GrammarParser.cs`, `GrammarAuthor.cs`) along construct-kind boundaries. `parse`'s
`categoryId` extraction is a best-effort read of a `Category`/`category` attribute on
`<WfiAnalysis>` -- no fixture in this slice's live proof exercises a non-null value, so it is
unverified against a real category-bearing result. Part 2 added the Machine fixture data proper (the
checked-in `fieldworks/` witness described above) and `build.ps1`'s consumption of it;
`ConvertFrom-PhonologyMutationsYaml` is a bespoke parser for that one manifest shape, not a general
YAML reader -- a real YAML dependency, if this grows past one manifest per fixture, is follow-on
work, not something this pass reaches for.
