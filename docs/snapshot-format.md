# PanGloss project snapshot format (v1)

**Crate:** `rust/crates/pg-snapshot`. **Status:** implemented (T1 of `docs/fwdata-import-plan.md`
§6). This document is the field-by-field specification; see that plan for the surrounding
architecture (`.fwdata → pg-fwdata → Snapshot → hc_grammar::compile → Grammar`).

## 0. Conventions

- **Envelope.** Every document is `{ "format": "pangloss-project", "version": 1, ... }`.
  `Snapshot::from_json` rejects any other format tag or version with a specific error rather
  than a generic parse failure.
- **Naming.** PanGloss-owned camelCase field names — *not* a mirror of LCM/M3Dump class or
  property names. Each field below documents which LCM property it originates from ("←").
- **Cross-references.** Every reference to another object in the snapshot is a FieldWorks GUID,
  rendered as a lowercase-hyphenated string (`Guid.ToString()`'s default format) — never an
  `Hvo` (FieldWorks' in-session integer id, which drifts across loads/sessions and is therefore
  unusable as a durable interchange key).
- **Determinism.** `to_json` is always pretty-printed (two-space indent), fields in Rust
  declaration order (serde's default). Every collection is a plain JSON array; **construction
  order is preserved** — nothing is sorted or reordered by this crate. Where the source LCM
  property is a sequence (`card="seq"`) the order is semantically meaningful (rule order, slot
  order, allomorph disjunctive order, ...); where the source is a collection (`card="col"`) the
  order is simply whatever order the producer (`pg-fwdata`) encountered it in, which is itself
  stable across repeated imports of the same `.fwdata` file.
- **Optional vs. absent.** `Option<T>` fields are omitted from the JSON entirely when `None`
  (`#[serde(skip_serializing_if = "Option::is_none")]`); empty `Vec<T>` fields are likewise
  omitted rather than emitted as `[]`, except where a field is expected on essentially every
  instance (e.g. `LexEntry.allomorphs`), in which case it's always present, even if empty.
- **Validation philosophy.** `Snapshot::validate()` returns `Vec<String>` **warnings** for
  dangling GUID references. It is not exhaustive (see §8) and never turns a reference problem
  into a hard error — real FieldWorks projects contain stale references (the motivating example
  in `docs/fwdata-import-plan.md` §1 is a stale `MoMorphAdhocProhib` that crashes the legacy C#
  exporter), and this pipeline must tolerate them.
- **Source references.** "HCLoader.cs:N" below means `Src/LexText/ParserCore/HCLoader.cs` in the
  FieldWorks repo (line numbers as of FieldWorks HEAD 2026-07-14). "MasterLCModel.xml" means
  `Localizations/LCM/src/SIL.LCModel/MasterLCModel.xml` in the same repo — the authoritative LCM
  schema (property names/cardinality); the compiled `SIL.LCModel` package (not vendored as source
  in that checkout) is where the actual GUID constants for well-known list items (morph types,
  etc.) live — see §5.9.

## 1. Top level (`Snapshot`)

| Field | Type | Notes |
|---|---|---|
| `format` | string | Always `"pangloss-project"`. |
| `version` | integer | Always `1` in this build. |
| `project` | [`Project`](#2-project) | |
| `featureSystems` | [`FeatureSystems`](#3-featuresystems) | |
| `phonology` | [`Phonology`](#4-phonology) | |
| `morphology` | [`Morphology`](#5-morphology) | |
| `lexicon` | [`Lexicon`](#6-lexicon) | |

## 2. `project`

| Field | Type | LCM origin |
|---|---|---|
| `name` | string | `LcmCache.ProjectId.Name` (`HCLoader.LoadLanguage`, HCLoader.cs:166). |
| `vernacularWritingSystems` | string[] | `LangProject.CurrentVernacularWritingSystems` (ICU tags), default writing system first. |
| `analysisWritingSystems` | string[] | `LangProject.CurrentAnalysisWritingSystems`, default first. |

Every writing-system tag used as a key in a `WsForm` anywhere else in the document is expected to
appear in one of these two lists.

## 3. `featureSystems`

FieldWorks has two independent feature systems, loaded identically by
`HCLoader.LoadFeatureSystem` (HCLoader.cs:2650-2667):

| Field | LCM origin |
|---|---|
| `featureSystems.phonological` | `LangProject.PhFeatureSystemOA` |
| `featureSystems.morphosyntactic` | `LangProject.MsFeatureSystemOA` |

Each is a `FeatureSystem`:

| Field | Type | LCM origin |
|---|---|---|
| `closedFeatures` | `ClosedFeature[]` | `FsFeatureSystem.FeaturesOC` filtered to `IFsClosedFeature` (HCLoader.cs:2654-2660). |
| `complexFeatures` | `ComplexFeature[]` | The non-closed remainder of `FeaturesOC` (HCLoader.cs:2661-2664). |

### `ClosedFeature`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `FsClosedFeature.Guid` |
| `name` | string | `FsFeatDefn.Name` (best analysis alternative) |
| `abbreviation` | string | `FsFeatDefn.Abbreviation` (best analysis alternative) — this is what `HCLoader` actually uses as the HC feature's description (HCLoader.cs:2659). |
| `values` | `FeatureValueSymbol[]` | `FsClosedFeature.ValuesOC`, in declaration order. |

### `FeatureValueSymbol`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `FsSymFeatVal.Guid` |
| `name` | string | `FsSymFeatVal.Name` (best analysis alternative). Not read by `HCLoader` (which only consults `Abbreviation`); carried for completeness/legibility. |
| `abbreviation` | string | `FsSymFeatVal.Abbreviation` (best analysis alternative) — the string `HCLoader` actually uses. |

### `ComplexFeature`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `FsComplexFeature.Guid` |
| `name` | string | `FsFeatDefn.Name` (best analysis alternative) |
| `abbreviation` | string | `FsFeatDefn.Abbreviation` (best analysis alternative) |
| `featureType` | guid, optional | `FsComplexFeature.TypeRA` (the `FsFeatStrucType` constraining what may appear inside this feature's value). `HCLoader` never consults this constraint; carried for schema completeness only — not resolved further (no `FsFeatStrucType` section exists in this format), T3 may ignore it. |

### `FeatureStructure` (recursive value type)

Used everywhere a feature structure is attached to something: phoneme features, MSA features,
stem-name regions, inflection-type features, natural-class features, ...

```json
{ "values": [ { "feature": "<guid>", "value": { "kind": "closed", "value": "<guid>" } } ] }
```

| Field | Type | LCM origin |
|---|---|---|
| `values` | `FeatureValue[]` | `FsFeatStruc.FeatureSpecsOC`, in declaration order (`HCLoader.LoadFeatureStruct`, HCLoader.cs:2500-2530). Order carries no semantic weight (feature structures are unordered mathematically) but is preserved for determinism. |

Each `FeatureValue`:

| Field | Type | LCM origin |
|---|---|---|
| `feature` | guid | `IFsFeatureSpecification.FeatureRA` — a `ClosedFeature`/`ComplexFeature` guid, resolved against whichever `FeatureSystem` this structure lives under (phonological or morphosyntactic; the two are never mixed within one structure). |
| `value` | `FeatureValueKind` | tagged: `{"kind":"closed","value":"<guid>"}` (← `IFsClosedValue.ValueRA`, a `FeatureValueSymbol` guid) or `{"kind":"complex","value":{...}}` (← `IFsComplexValue.ValueOA`, a nested `FeatureStructure`). |

## 4. `phonology`

| Field | Type | LCM origin |
|---|---|---|
| `phonemes` | `Phoneme[]` | `PhonologicalDataOA.PhonemeSetsOS[0].PhonemesOC` — `HCLoader` only ever loads the first phoneme set (HCLoader.cs:204). |
| `boundaryMarkers` | `BoundaryMarker[]` | `PhonemeSetsOS[0].BoundaryMarkersOC`, excluding the special word-boundary marker (see `PhonContext.wordBoundary` below). |
| `naturalClasses` | `NaturalClass[]` | `PhonologicalDataOA.NaturalClassesOS`. |
| `environments` | `Environment[]` | `PhonologicalDataOA.EnvironmentsOS`, reached via the closure of every allomorph/rule that references one. |
| `rules` | `PhonologicalRule[]` | `PhonologicalDataOA.PhonRulesOS`, in `OrderNumber` order, `Disabled` rules skipped (HCLoader.cs:302). |
| `featureConstraints` | `FeatureConstraint[]` | `PhonologicalDataOA.FeatConstraints`. |

### `Phoneme`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `PhPhoneme.Guid` |
| `name` | string | `PhPhoneme.Name` (best analysis alternative) — distinct from `representations`; not read by `HCLoader` itself but useful for diagnostics. |
| `representations` | `WsForm[]` | `PhPhoneme.CodesOS[*].Representation` per writing system, dotted-circle (U+25CC) stripped (`HCLoader.RemoveDottedCircles`, HCLoader.cs:2678-2680). A phoneme with zero representations after stripping is `IHCLoadErrorLogger.InvalidPhoneme` in FieldWorks; `pg-fwdata` reports the same as an import warning rather than dropping the phoneme. |
| `features` | `FeatureStructure`, optional | `PhPhoneme.FeaturesOA`, when non-empty (HCLoader.cs:2675-2676); resolves against `featureSystems.phonological`. |
| `basicIpaSymbol` | string, optional | `PhPhoneme.BasicIPASymbol`. Not read by `HCLoader` (works purely from `representations`); carried as a diagnostic fallback description. |

### `BoundaryMarker`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `PhBdryMarker.Guid` |
| `name` | string | `PhBdryMarker.Name` (best analysis alternative) |
| `representations` | `WsForm[]` | `PhBdryMarker.CodesOS[*].Representation` (best vernacular alternative; HCLoader.cs:2700-2702) |

### `NaturalClass` (tagged: `kind: "segments" | "features"`)

| Variant | Fields | LCM origin |
|---|---|---|
| `segments` | `guid`, `name`, `phonemes: guid[]` | `PhNCSegments.SegmentsRC` — extensional definition (list of member phonemes). `name` ← `PhNaturalClass.Abbreviation` (best analysis alternative) — this, not `Name`, is what `HCLoader` uses (HCLoader.cs:2825) and what environment strings reference in `[Abbr]` notation. |
| `features` | `guid`, `name`, `features: FeatureStructure` | `PhNCFeatures.FeaturesOA` — intensional definition (feature values every member phoneme must include). |

### `Environment`

FieldWorks stores environments as a hand-authored string (e.g. `/_[UnVDent]`, `/#[C]_`) rather
than a structured tree, and `HCLoader` tokenizes/validates that string lazily at load time
(`TokenizeContext`/`IsValidEnvironment`, HCLoader.cs:2260-2457), tolerating malformed strings as
warnings, not hard failures (HCLoader.cs:1184-1197). This format keeps the raw string as-is —
re-tokenizing it is a compiler (T3) concern.

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `PhEnvironment.Guid` |
| `name` | string | `PhEnvironment.Name` (best analysis alternative); often empty. |
| `representation` | string | `PhEnvironment.StringRepresentation` |

### `FeatureConstraint`

One alpha-variable slot (SPE-style Greek-letter agreement/disagreement variables: α, β, γ, ...;
`HCLoader.VariableNames`, HCLoader.cs:37-41).

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `PhFeatureConstraint.Guid` |
| `feature` | guid | `PhFeatureConstraint.FeatureRA`, resolved against `featureSystems.phonological`. |

### `PhonContext` (recursive, tagged: `kind`)

The pattern-tree type used by rewrite-rule structural descriptions/changes, metathesis
descriptions, and affix-process input (`AffixProcess.input`). Mirrors the LCM `PhPhonContext`
hierarchy exactly as `HCLoader.LoadPatternNode` consumes it (HCLoader.cs:2313-2389), plus
`IPhVariable` (used only inside `MoAffixProcess.InputOS`, HCLoader.cs:1338-1346) folded in as
`variable` so callers don't need a second parallel type.

| Variant | Fields | LCM origin |
|---|---|---|
| `sequence` | `members: PhonContext[]` | `PhSequenceContext.MembersRS` |
| `iteration` | `min: int`, `max: int` (`-1` = unbounded), `member: PhonContext` | `PhIterationContext` (`Minimum`/`Maximum`/`MemberRA`) |
| `segment` | `phoneme: guid` | `PhSimpleContextSeg.FeatureStructureRA` (a `PhPhoneme`) |
| `naturalClass` | `naturalClass: guid`, `plusVariables: guid[]`, `minusVariables: guid[]` | `PhSimpleContextNC.FeatureStructureRA` + `.PlusConstrRS`/`.MinusConstrRS` (`FeatureConstraint` guids this occurrence must agree/disagree on; HCLoader.cs:2745-2763) |
| `boundary` | `marker: guid` | `PhSimpleContextBdry.FeatureStructureRA`, when *not* the special word-boundary marker |
| `wordBoundary` | (none) | `PhSimpleContextBdry.FeatureStructureRA.Guid == LangProjectTags.kguidPhRuleWordBdry` (HCLoader.cs:2351, 2489-2498) — the `#` anchor. Deliberately not a `boundary` referencing that well-known guid, since it corresponds to no entry in `boundaryMarkers` (`LoadCharacterDefinitionTable` explicitly excludes it, HCLoader.cs:2698). |
| `variable` | (none) | `IPhVariable` — "match anything"; only ever appears inside `MoAffixProcess.InputOS` in real FieldWorks data. |

### `PhonologicalRule` (tagged: `kind: "rewrite" | "metathesis"`)

#### `rewrite` → `RewriteRule` (← `PhRegularRule`)

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `PhRegularRule.Guid` |
| `name` | string | `PhSegmentRule.Name` (best analysis alternative) |
| `direction` | `"leftToRight" \| "rightToLeft" \| "simultaneous"` | `PhRegularRule.Direction` (int enum 0/1/2; HCLoader.cs:2015-2031). `HCLoader` additionally derives an `ApplicationMode` (iterative vs. simultaneous) from this value at compile time — that derivation is not separately stored here; T3 re-derives it the same way (0/1 → iterative, 2 → simultaneous). |
| `structuralDescription` | `PhonContext[]` | `PhRegularRule.StrucDescOS` (HCLoader.cs:2033-2044) |
| `featureConstraintVariables` | `guid[]` | `PhRegularRule.FeatureConstraints`, in the order `HCLoader` assigns Greek letters (HCLoader.cs:2005-2011); each is a `FeatureConstraint` guid also present in `phonology.featureConstraints`. |
| `rightHandSides` | `RewriteRhs[]` | `PhRegularRule.RightHandSidesOS` |

`RewriteRhs` (← `PhSegRuleRHS`):

| Field | Type | LCM origin |
|---|---|---|
| `structuralChange` | `PhonContext[]` | `PhSegRuleRHS.StrucChangeOS` (HCLoader.cs:2060-2071) |
| `leftContext` | `PhonContext`, optional | `PhSegRuleRHS.LeftContextOA` |
| `rightContext` | `PhonContext`, optional | `PhSegRuleRHS.RightContextOA` |
| `requiredPartsOfSpeech` | `guid[]` | `PhSegRuleRHS.InputPOSesRC`, resolved against `morphology.partsOfSpeech`. |
| `requiredRuleFeatures` | `guid[]` | `PhSegRuleRHS.ReqRuleFeatsRC` (`IPhPhonRuleFeat.ItemRA`: an `MoInflClass` or a `CmPossibility`; HCLoader.cs:2610-2623). Each guid is the referenced item itself, **not** expanded to its subclass closure (`HCLoader.LoadAllInflClasses` does that at compile time, HCLoader.cs:2593-2608) — T3 must perform the same expansion using `morphology`'s inflection-class hierarchy. |
| `excludedRuleFeatures` | `guid[]` | `PhSegRuleRHS.ExclRuleFeatsRC`, same shape. |

#### `metathesis` → `MetathesisRule` (← `PhMetathesisRule`)

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `PhMetathesisRule.Guid` |
| `name` | string | `PhSegmentRule.Name` (best analysis alternative) |
| `direction` | `RuleDirection` | `PhMetathesisRule.Direction` (0/1/2, where 2 also behaves as left-to-right for metathesis; HCLoader.cs:2107-2117) |
| `structuralDescription` | `PhonContext[]` | `PhMetathesisRule.StrucDescOS`, left to right |
| `leftSwitchIndex` | integer | `PhMetathesisRule.LeftSwitchIndex` — 0-based index into `structuralDescription` |
| `rightSwitchIndex` | integer | `PhMetathesisRule.RightSwitchIndex` |

`HCLoader` additionally computes derived "middle"/"left env"/"right env" part indices from these
two (`GetStrucChangeIndices`, HCLoader.cs:2119-2120) under an assumed canonical layout; that
derivation is compiler policy over already-complete data and is not duplicated here.

## 5. `morphology`

| Field | Type | LCM origin |
|---|---|---|
| `partsOfSpeech` | `PartOfSpeech[]` | Root-level parts of speech (each carries its own `children`). ← `LangProject.PartsOfSpeechOA` top level. `HCLoader.LoadLanguage`'s `AllPartsOfSpeech` (HCLoader.cs:170) flattens the whole tree at load time — this format keeps the tree shape since flattening it is a trivial, reversible compiler-side step. |
| `compoundRules` | `CompoundRule[]` | `MorphologicalDataOA.CompoundRulesOS`, in declaration order, **including** disabled ones (each carries its own `disabled` flag). When this list is empty *and* `parserParameters.noDefaultCompounding` is false, `HCLoader` synthesizes two default rules (`DefaultCompoundingRules`, HCLoader.cs:1808-1840) — that synthesis is compiler policy, not stored data. |
| `adhocProhibitions` | `AdhocProhibition[]` | `IMoAlloAdhocProhibRepository`/`IMoMorphAdhocProhibRepository.AllInstances()` (HCLoader.cs:341-351), including disabled/dangling ones. |
| `lexEntryInflTypes` | `LexEntryInflType[]` | `ILexEntryInflTypeRepository.AllInstances()` |
| `parserParameters` | `ParserParameters` | `MorphologicalDataOA.ParserParameters` (raw `<ParserParameters><HC>` XML block, parsed by `HCLoader`'s constructor, HCLoader.cs:92-112) |

### `PartOfSpeech`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `PartOfSpeech.Guid` |
| `name` | string | `CmPossibility.Name` (best analysis alternative) |
| `abbreviation` | string | `CmPossibility.Abbreviation` (best analysis alternative) — what `HCLoader` surfaces as the HC feature-symbol description (HCLoader.cs:172) |
| `children` | `PartOfSpeech[]` | `CmPossibility.SubPossibilitiesOS`, cast to `IPartOfSpeech` |
| `inflectionClasses` | `InflectionClass[]` | `PartOfSpeech.InflectionClassesOC` — owned directly by this POS, not inherited |
| `defaultInflectionClass` | guid, optional | `PartOfSpeech.DefaultInflectionClassRA`. `HCLoader` walks this up the POS ownership chain when a stem's MSA doesn't specify one (`GetDefaultInflClass`, HCLoader.cs:2634-2648). |
| `inflectableFeatures` | `guid[]` | `PartOfSpeech.InflectableFeats` (`FsFeatDefn` guids; resolves against `featureSystems.morphosyntactic`) |
| `stemNames` | `StemName[]` | `PartOfSpeech.StemNames` |
| `affixSlots` | `AffixSlot[]` | `PartOfSpeech.AffixSlots` |
| `affixTemplates` | `AffixTemplate[]` | `PartOfSpeech.AffixTemplates`, in declaration order; `HCLoader` skips `Disabled` templates and templates with no loaded affixes in any slot (HCLoader.cs:295-300) — the snapshot keeps the full authored list. |

### `InflectionClass`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `MoInflClass.Guid` |
| `name` | string | `MoInflClass.Name` (best analysis alternative) |
| `abbreviation` | string | `MoInflClass.Abbreviation` (best analysis alternative). `HCLoader` represents each inflection class as an opaque `MprFeature` keyed by object identity, not by this string (HCLoader.cs:571-577); carried for legibility. |
| `children` | `InflectionClass[]` | `MoInflClass.SubclassesOC` (`HCLoader.LoadInflClassMprFeature` recurses through these, HCLoader.cs:510-515) |

### `StemName`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `MoStemName.Guid` |
| `name` | string | `MoStemName.Name` (best analysis alternative) — what `HCLoader` uses (HCLoader.cs:221) |
| `abbreviation` | string, optional | `MoStemName.Abbreviation` (best analysis alternative). Not read by `HCLoader`; carried for completeness. |
| `regions` | `FeatureStructure[]` | `MoStemName.RegionsOC`, filtered to non-empty feature structures (HCLoader.cs:210: `fs.Where(fs => !fs.IsEmpty)`); resolves against `featureSystems.morphosyntactic`. A stem name with zero non-empty regions is dropped entirely by `HCLoader` (HCLoader.cs:219-224) — `pg-fwdata` still emits it, leaving that filtering decision to T3. |

### `AffixSlot`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `MoInflAffixSlot.Guid` |
| `name` | string | `MoInflAffixSlot.Name` (best analysis alternative) |
| `optional` | boolean | `MoInflAffixSlot.Optional` |

### `AffixTemplate`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `MoInflAffixTemplate.Guid` |
| `name` | string | `MoInflAffixTemplate.Name` (best analysis alternative) |
| `disabled` | boolean | `MoInflAffixTemplate.Disabled` |
| `prefixSlots` | `guid[]` | `MoInflAffixTemplate.PrefixSlotsRS`, innermost-to-outermost (right-to-left), guids into the owning POS's `affixSlots` |
| `suffixSlots` | `guid[]` | `MoInflAffixTemplate.SuffixSlotsRS`, innermost-to-outermost (left-to-right) |
| `isFinal` | boolean | `MoInflAffixTemplate.Final` (HCLoader.cs:1678) |

Note: `MoInflAffixTemplate` also has `ProcliticSlots`/`EncliticSlots` in the LCM schema; `HCLoader`
never reads them (only `PrefixSlotsRS`/`SuffixSlotsRS`, HCLoader.cs:297), so they are not carried.

### `CompoundRule` (tagged: `kind: "endocentric" | "exocentric"`)

| Variant | Fields | LCM origin |
|---|---|---|
| `endocentric` | `guid`, `name`, `disabled`, `headLast: bool`, `left`/`right: CompoundConstituentRequirement`, `overriding: CompoundOutcome` | `MoEndoCompound` — `headLast` ← `HeadLast`; `left`/`right` ← `LeftMsaOA`/`RightMsaOA`; `overriding` ← `OverridingMsaOA` |
| `exocentric` | `guid`, `name`, `disabled`, `left`/`right: CompoundConstituentRequirement`, `to: CompoundOutcome` | `MoExoCompound` — `to` ← `ToMsaOA` |

`CompoundConstituentRequirement` (← the `PartOfSpeechRA`/`ProdRestrictRC` pair read off a compound
side's `MoStemMsa`; HCLoader.cs:1848-1941 only ever reads these two properties of that MSA):

| Field | Type | LCM origin |
|---|---|---|
| `partOfSpeech` | guid, optional | `MoStemMsa.PartOfSpeechRA` |
| `exceptionFeatures` | `guid[]` | `MoStemMsa.ProdRestrictRC` |

`CompoundOutcome` (the output MSA's shape):

| Field | Type | LCM origin |
|---|---|---|
| `partOfSpeech` | guid, optional | `MoStemMsa.PartOfSpeechRA` |
| `inflectionClass` | guid, optional | `MoStemMsa.InflectionClassRA` |

### `MorphType` (closed enum)

`stem | boundStem | root | boundRoot | prefix | suffix | infix | circumfix | proclitic | enclitic
| clitic | particle | phrase | discontigPhrase | prefixingInterfix | infixingInterfix |
suffixingInterfix`

← FieldWorks' well-known `MoMorphType` possibilities. `pg-fwdata` (T2) owns the guid→variant
mapping table (the `MoMorphTypeTags.kguidMorph*` constants referenced throughout `HCLoader.cs`,
e.g. lines 524-568 [rule-form validity], 591-624 [stem/clitic classification], 837-841 [bound
root/stem], 1423-1435 and 1464-1520 [reduplication hints / affix-process morph-type switch]).
Those GUID literals live in the compiled `SIL.LCModel` NuGet package, not in the source-available
slice of the FieldWorks repo checked out alongside this project — see §5.9 for detail. This
format only needs the closed, named enumeration; the actual guid↔variant table is `pg-fwdata`'s
responsibility, out of scope for this crate.

Bound-ness ("is this allomorph a bound root/stem?") is *derived* from this enum
(`boundRoot`/`boundStem`) rather than a separate flag, matching `HCLoader`'s own `IsBound`
derivation (HCLoader.cs:835-841).

### `AdhocProhibition` (tagged: `kind: "allomorph" | "morpheme"`)

| Variant | Fields | LCM origin |
|---|---|---|
| `allomorph` | `guid`, `disabled`, `primary: guid`, `others: guid[]`, `adjacency` | `MoAlloAdhocProhib` — `primary` ← `FirstAllomorphRA` (an `MoForm` guid, resolves against `lexicon.entries[*].allomorphs`); `others` ← `RestOfAllosRS`, in order |
| `morpheme` | `guid`, `disabled`, `primary: guid`, `others: guid[]`, `adjacency` | `MoMorphAdhocProhib` — `primary` ← `FirstMorphemeRA` (an `MoMorphSynAnalysis` guid, resolves against `lexicon.entries[*].msas`); `others` ← `RestOfMorphsRS` |

`adjacency`: `anywhere | somewhereToLeft | somewhereToRight | adjacentToLeft | adjacentToRight` ←
`MoAdhocProhib.Adjacency` (int enum 0-4; `HCLoader.GetAdjacency`, HCLoader.cs:2241-2258).

### `LexEntryInflType`

Tags a variant `LexEntry` as realizing a specific inflectional cell without an overt inflectional
affix (e.g. English "went" as the past-tense type of "go"). ← `LexEntryInflType`.

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `LexEntryInflType.Guid` |
| `name` | string | `CmPossibility.Name` (best analysis alternative) |
| `abbreviation` | string | `CmPossibility.Abbreviation` (best analysis alternative) |
| `glossPrepend` | string | `LexEntryInflType.GlossPrepend` (best analysis alternative). `HCLoader` omits it when it equals the literal sentinel `"***"` (HCLoader.cs:751-753) — `pg-fwdata` still carries the raw value; T3 must apply the same sentinel check. |
| `glossAppend` | string | `LexEntryInflType.GlossAppend`, same `"***"` sentinel convention |
| `slots` | `guid[]` | `LexEntryInflType.SlotsRC` — affix-template slots this irregular form should be treated as filling |
| `inflectionFeatures` | `FeatureStructure`, optional | `LexEntryInflType.InflFeatsOA`, when non-empty |

### `ParserParameters`

← the `<ParserParameters><HC>` XML block FieldWorks stores as a string on
`MorphologicalDataOA.ParserParameters`, parsed by `HCLoader`'s constructor (HCLoader.cs:92-112).

| Field | Type | LCM origin / default |
|---|---|---|
| `notOnClitics` | boolean | `<NotOnClitics>`. Default **`true`** when the `<HC>` element or this sub-element is absent (HCLoader.cs:95 — note the default-true polarity). |
| `acceptUnspecifiedGraphemes` | boolean | `<AcceptUnspecifiedGraphemes>`, default `false` |
| `noDefaultCompounding` | boolean | `<NoDefaultCompounding>`, default `false` |
| `strata` | string, optional | The raw, unparsed `<Strata>` string (e.g. `"Morphology,(Clitics,Templates),Phonology"`). `HCLoader.ParseStrataString`'s comma/parenthesis tokenizer (HCLoader.cs:120-151) is compiler policy over this string; T3 re-parses it the same way. `None`/absent means the default stratum layout (`Morphology`, `Clitics`, `Surface`) applies. |
| `compoundRuleMaxApplications` | `{compoundRule: guid, maxApplications: integer}[]` | `<CompoundRules>`'s per-rule `maxApps` attribute, keyed by `MoCompoundRule` guid (HCLoader.cs:103-112). A compound rule with no entry here defaults to `maxApplications = 1` (HCLoader.cs:1894). |

## 6. `lexicon`

| Field | Type | LCM origin |
|---|---|---|
| `entries` | `LexEntry[]` | `LangProject.LexDbOA.Entries` |

### `LexEntry`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `LexEntry.Guid` |
| `citationForm` | `WsForm[]` | `LexEntry.CitationForm` |
| `lexemeMorphType` | `MorphType` | A convenience cache of `allomorphs[last].morphType` (the lexeme form is always ordered last — see `allomorphs` below). **Not** a distinct LCM property (LCM's `MorphType` lives on `MoForm`/allomorphs, not on `LexEntry`); provided because `HCLoader` repeatedly branches on exactly this value at the entry level (e.g. `ILexEntry.IsCircumfix()`, `HasValidRuleForm`, HCLoader.cs:517-534, both keyed off `entry.LexemeFormOA.MorphTypeRA`). |
| `allomorphs` | `Allomorph[]` | `AlternateFormsOS` **then** `LexemeFormOA` (HCLoader.cs:263: `entry.AlternateFormsOS.Concat(entry.LexemeFormOA)`) — this order matters for disjunctive-ordering semantics. |
| `msas` | `Msa[]` | `LexEntry.MorphoSyntaxAnalysesOC` (unordered in LCM; snapshot order is import-stable but not semantically meaningful) |
| `senses` | `Sense[]` | `LexEntry.SensesOS`, in order |
| `entryRefs` | `EntryRef[]` | `LexEntry.EntryRefsOS` |

### `Allomorph`

Covers all three `MoForm` subclasses (`MoStemAllomorph`, `MoAffixAllomorph`, `MoAffixProcess`) in
one shape; fields specific to one kind are simply absent for the others.

| Field | Type | Applies to | LCM origin |
|---|---|---|---|
| `guid` | guid | all | `MoForm.Guid` |
| `morphType` | `MorphType` | all | `MoForm.MorphTypeRA` |
| `isAbstract` | boolean | all | `MoForm.IsAbstract` |
| `forms` | `WsForm[]` | stem, affix (empty for process-only) | `MoForm.Form`. May contain FieldWorks' lexical-pattern bracket notation for reduplication/underspecified segments (e.g. `[C][V]d`, `-[...]`) left verbatim — `HCLoader.IsLexicalPattern`/`Segment` (HCLoader.cs:2532-2571) parse that notation at compile time; T3 must do the same. |
| `environments` | `guid[]` | stem, affix | `MoStemAllomorph.PhoneEnvRC` / `MoAffixAllomorph.PhoneEnvRC`, guids into `phonology.environments` |
| `positions` | `guid[]` | affix (infixes) | `MoAffixAllomorph.PositionRS`, same object kind as `environments` (`HCLoader.GetAffixAllomorphEnvironments` concatenates the two, HCLoader.cs:1167-1170) |
| `stemName` | guid, optional | stem only | `MoStemAllomorph.StemNameRA` |
| `inflectionClasses` | `guid[]` | affix (both `MoAffixAllomorph` and `MoAffixProcess`) | `MoAffixForm.InflectionClassesRC` |
| `msEnvFeatures` | `FeatureStructure`, optional | affix (`MoAffixAllomorph`) | `MoAffixAllomorph.MsEnvFeaturesOA`, when non-empty (HCLoader.cs:1124-1128); resolves against `featureSystems.morphosyntactic` |
| `msEnvPartOfSpeech` | guid, optional | affix (`MoAffixAllomorph`) | `MoAffixAllomorph.MsEnvPartOfSpeechRA`. **Not currently read by `HCLoader`** — no reference found in `HCLoader.cs`; included for schema completeness since it's `msEnvFeatures`' direct sibling in the LCM model. T3 may ignore it. |
| `process` | `AffixProcess`, optional | `MoAffixProcess` only | see below |

### `AffixProcess`

Rule-based (non-concatenative) allomorph realization — ablaut, templatic morphology, etc.
← `MoAffixProcess.InputOS` / `.OutputOS` (`HCLoader.LoadAffixProcessAllomorph`, HCLoader.cs:
1334-1439).

| Field | Type | LCM origin |
|---|---|---|
| `input` | `PhonContext[]` | `MoAffixProcess.InputOS`. Each entry is one numbered "part" of the input, referenced positionally (1-based, list order) by `copyFromInput`/`modifyFromInput` in `output`. |
| `output` | `RuleMapping[]` | `MoAffixProcess.OutputOS`, in order |

`RuleMapping` (tagged: `kind`) — ← `MoRuleMapping`:

| Variant | Fields | LCM origin |
|---|---|---|
| `insertNaturalClass` | `naturalClass: guid` | `MoInsertNC.ContentRA` (a plain `IPhNaturalClass`, no variable constraints) |
| `copyFromInput` | `part: integer` (1-based) | `MoCopyFromInput.ContentRA` (`HCLoader` resolves via `ContentRA.IndexInOwner + 1`, HCLoader.cs:1383) |
| `insertSegments` | `text: string` | `MoInsertPhones.ContentRS` (`IPhTerminalUnit` sequence, concatenated to a string by `HCLoader`, HCLoader.cs:1388-1406) — a literal surface string, re-segmented downstream, same philosophy as `Environment.representation` |
| `modifyFromInput` | `part: integer`, `naturalClass: guid` | `MoModifyFromInput.ContentRA` (part index) / `.ModificationRA` (natural class), HCLoader.cs:1409-1419 |

### `Msa` (tagged: `kind: "stem" | "inflectional" | "derivational" | "unclassified"`)

| Variant | Fields | LCM origin |
|---|---|---|
| `stem` | `guid`, `partOfSpeech?`, `inflectionClass?`, `features?`, `exceptionFeatures[]`, `fromPartsOfSpeech[]`, `slots[]` | `MoStemMsa`. `inflectionClass` ← `InflectionClassRA` (`None` = use owning POS's `defaultInflectionClass`, HCLoader.cs:2625-2632); `exceptionFeatures` ← `ProdRestrictRC`; `fromPartsOfSpeech`/`slots` ← `FromPartsOfSpeechRC`/`SlotsRC` (only meaningful when this stem's allomorphs are clitic-type morph types — the same LCM class doubles as a clitic's "rule" MSA, `LoadCliticAffixProcessRule`, HCLoader.cs:1030-1046) |
| `inflectional` | `guid`, `partOfSpeech?`, `slots[]`, `features?`, `exceptionFeatures[]` | `MoInflAffMsa`. `slots` ← `SlotsRC` (empty ⇒ "partial", `HCLoader` sets `IsPartial = SlotsRC.Count == 0`, HCLoader.cs:982, and the rule applies outside any template, HCLoader.cs:889-890); `features` ← `InflFeatsOA`; `exceptionFeatures` ← `FromProdRestrictRC` |
| `derivational` | `guid`, `fromPartOfSpeech?`, `toPartOfSpeech?`, `fromFeatures?`, `toFeatures?`, `fromInflectionClass?`, `toInflectionClass?`, `fromExceptionFeatures[]`, `toExceptionFeatures[]`, `fromStemName?` | `MoDerivAffMsa` — direct field-for-field mapping of `FromPartOfSpeechRA`/`ToPartOfSpeechRA`/`FromMsFeaturesOA`/`ToMsFeaturesOA`/`FromInflectionClassRA`/`ToInflectionClassRA`/`FromProdRestrictRC`/`ToProdRestrictRC`/`FromStemNameRA` |
| `unclassified` | `guid`, `partOfSpeech?` | `MoUnclassifiedAffixMsa` |

### `Sense`

| Field | Type | LCM origin |
|---|---|---|
| `guid` | guid | `LexSense.Guid` |
| `gloss` | `WsForm[]` | `LexSense.Gloss` |
| `definition` | `WsForm[]` | `LexSense.Definition` |
| `msa` | guid, optional | `LexSense.MorphoSyntaxAnalysisRA` — a guid into **this same entry's** `msas` |

### `EntryRef` (tagged: `kind: "variant" | "complexForm"`)

← `LexEntryRef`, discriminated on whether `VariantEntryTypesRS` or `ComplexEntryTypesRS` was
populated (`pg-fwdata` picks `variant` when both are somehow non-empty, matching FieldWorks UI's
treatment of the two as mutually exclusive in practice).

| Variant | Fields | LCM origin |
|---|---|---|
| `variant` | `guid`, `componentLexemes: guid[]`, `variantEntryTypes: guid[]` | `LexEntryRef` with `VariantEntryTypesRS` populated. `componentLexemes` ← `ComponentLexemesRS` (the main entry/sense this is a variant of — each guid may be a `LexEntry` **or** a `LexSense`, LCM's `CmObject` union; resolvers must check both `lexicon.entries` and each entry's `senses`). `variantEntryTypes` ← `VariantEntryTypesRS` — each guid may be a plain `LexEntryType` or a `morphology.lexEntryInflTypes` guid (`HCLoader.GetInflTypes`'s `as ILexEntryInflType` downcast, HCLoader.cs:657-679); `HCLoader.LoadLexEntries`/`LoadMorphologicalRules` walk exactly this shape when `entry.SensesOS.Count == 0` (HCLoader.cs:628-651, 852-871) to attach the variant's allomorphs to the *main* entry's MSAs. |
| `complexForm` | `guid`, `componentLexemes: guid[]`, `complexEntryTypes: guid[]` | `LexEntryRef` with `ComplexEntryTypesRS` populated. Not walked by `HCLoader` today; carried for completeness/forward-compatibility. |

## 7. `WsForm` (shared primitive)

```json
{ "ws": "en", "form": "dog" }
```

`ws` is a writing-system tag (ICU locale id), not a GUID — FieldWorks writing systems are
identified by tag, not object GUID, in the raw `.fwdata` XML itself (`AUni ws="..."` /
`AStr ws="..."` attributes). Used for every LCM `MultiUnicode`/`MultiString` field this format
carries.

## 8. What `Snapshot::validate()` checks (and doesn't)

`validate()` is a **light** structural check, not an exhaustive schema validator. It resolves:

- Feature-structure `feature`/`value` guids against the relevant `FeatureSystem`.
- Natural-class member-phoneme guids, and phoneme/natural-class/boundary/feature-constraint guids
  inside every `PhonContext` tree (rewrite/metathesis rules, affix-process input/output).
- Part-of-speech and inflection-class guids everywhere they're referenced (rewrite-rule RHS POS
  requirements, compound rules, MSAs, allomorphs).
- Environment/position/stem-name/affix-slot guids on allomorphs, MSAs, and affix templates.
- Ad-hoc-prohibition `primary`/`others` guids against the full set of allomorph/MSA guids across
  every entry.
- `Sense.msa` against the *same entry's* own `msas`.
- `EntryRef.componentLexemes` against every entry and sense guid in the document.
- Compound-rule `maxApps` keys in `parserParameters` against `morphology.compoundRules`.

It deliberately does **not** check (because this format has no canonical registry to check
against):

- `requiredRuleFeatures`/`excludedRuleFeatures` on a rewrite rule's RHS, and `exceptionFeatures`/
  `fromExceptionFeatures`/`toExceptionFeatures` on MSAs/allomorphs/compound sides — these may
  reference either an inflection class (checked) or an arbitrary `CmPossibility` "exception
  feature"/"production restriction" list item, which is never enumerated as its own top-level
  snapshot section (it's an open-ended, user-authored possibility list).
- `EntryRef.variantEntryTypes`/`complexEntryTypes` — may reference either a `LexEntryInflType`
  or a plain `LexEntryType` possibility, the latter not enumerated anywhere in this format.
- The raw text inside `Environment.representation` and `RuleMapping::InsertSegments.text` — these
  are opaque strings re-tokenized by the compiler (T3), not structured references this crate can
  resolve.

## 9. Deferred / out of scope for this crate

- **The `MoMorphType` guid→`MorphType` mapping table.** The actual well-known GUID constants
  (`MoMorphTypeTags.kguidMorph*`) live in the compiled `SIL.LCModel` NuGet package, which is not
  vendored as source anywhere in the FieldWorks checkout available while writing this crate (only
  usage sites were found, e.g. `Src/LexText/ParserCore/HCLoader.cs` and
  `Src/LexText/ParserCore/HCParser.cs`; the constants' *definitions* are compiled, not source).
  Building and maintaining that mapping table is `pg-fwdata`'s (T2's) job — it's the component
  that actually reads raw `.fwdata` XML `class="MoMorphType"` records and their GUIDs.
- **Parsing/validating `Environment.representation` strings and reduplication bracket notation in
  `Allomorph.forms`.** Both are kept as opaque, FieldWorks-native syntax; `hc_grammar::compile`
  (T3) is the intended consumer and owns the tokenizer/parser for them, mirroring `HCLoader`'s own
  lazy, tolerant parsing (HCLoader.cs:2260-2457, 2532-2571).
- **`.fwdata` XML parsing itself, and everything in `HCLoader.cs` that constructs a HermitCrab
  runtime object graph (`Language`, `Stratum`, `Pattern<Word, ShapeNode>`, ...).** That belongs to
  `pg-fwdata` (T2, the extractor) and `hc_grammar::compile` (T3, the compiler) respectively; this
  crate only defines and (de)serializes the data contract between them.
