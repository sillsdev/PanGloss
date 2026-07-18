//! The `morphology` snapshot section: parts of speech, inflection classes, stem names, affix
//! slots/templates, compound rules, ad-hoc co-occurrence prohibitions, irregular-inflection
//! types, morph types, and parser parameters.
//!
//! Reference: `HCLoader.cs` `LoadLanguage()` driver 164-357, rule-form validity 536-569,
//! templates 1673-1735, null-affix synthesis 1771-1806, compounding 1808-2001, ad-hoc rules
//! 2163-2258. LCM schema: `MasterLCModel.xml` `PartOfSpeech` (~3763), `MoInflClass` (~3458),
//! `MoStemName` (~3687), `MoInflAffixSlot`/`MoInflAffixTemplate` (~3310-3419),
//! `MoCompoundRule`/`MoEndoCompound`/`MoExoCompound` (~3073-3281), `MoAdhocProhib`/
//! `MoAlloAdhocProhib`/`MoMorphAdhocProhib` (~2964-2582), `LexEntryInflType` (~5176).

use serde::{Deserialize, Serialize};

use crate::common::Guid;
use crate::feature::FeatureStructure;

/// The `morphology` snapshot section.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Morphology {
    /// Root-level parts of speech; each [`PartOfSpeech`] carries its own `children` for the
    /// rest of the hierarchy. ← `LangProject.PartsOfSpeechOA` top level (`AllPartsOfSpeech` in
    /// `HCLoader.LoadLanguage`, HCLoader.cs:170, flattens the whole tree; this format keeps the
    /// tree shape instead since it is lossless and `AllPartsOfSpeech`-style flattening is a
    /// trivial, reversible compiler-side operation).
    pub parts_of_speech: Vec<PartOfSpeech>,
    /// ← `MorphologicalDataOA.CompoundRulesOS`, in declaration order, *including* `Disabled`
    /// ones (each rule carries its own `disabled` flag — `HCLoader` filters at load time,
    /// HCLoader.cs:241, but the snapshot keeps the full authored list; when this list is empty
    /// **and** `parser_parameters.no_default_compounding` is false, `HCLoader` synthesizes two
    /// default rules, `DefaultCompoundingRules`, HCLoader.cs:1808-1840 — that synthesis is
    /// compiler policy, not stored data, and is not represented here).
    pub compound_rules: Vec<CompoundRule>,
    /// ← `IMoAlloAdhocProhibRepository`/`IMoMorphAdhocProhibRepository.AllInstances()`
    /// (HCLoader.cs:341-351), *including* disabled/dangling ones (each carries its own
    /// `disabled` flag; a `primary`/`others` guid that fails to resolve to a real allomorph or
    /// MSA is exactly the "stale ad-hoc rule" tolerance case called out in
    /// `docs/fwdata-import-plan.md` §1 — a [`crate::validate`] warning, never an import error).
    pub adhoc_prohibitions: Vec<AdhocProhibition>,
    /// The registry of "exception feature" / productivity-restriction possibilities that MSAs,
    /// compound-rule constituent requirements, and rewrite-rule right-hand sides restrict
    /// productivity by. ← `MorphologicalDataOA.ProdRestrictOA.ReallyReallyAllPossibilities`
    /// (`HCLoader.LoadLanguage`, HCLoader.cs:180) *and* the `CmPossibility`-typed items of
    /// `PhonologicalDataOA.PhonRuleFeats` (HCLoader.cs:2619-2621) — both are flattened into this
    /// one registry since `HCLoader` treats them identically (an opaque, guid-identified
    /// `MprFeature`, `LoadMprFeature`, HCLoader.cs:571-577). Every `exceptionFeatures`/
    /// `fromExceptionFeatures`/`toExceptionFeatures`/`requiredRuleFeatures`/
    /// `excludedRuleFeatures` guid list elsewhere in this document either resolves here or is an
    /// [`InflectionClass`] guid (the other kind `HCLoader.LoadMprFeatures` accepts,
    /// HCLoader.cs:2610-2623).
    pub exception_features: Vec<ExceptionFeature>,
    /// ← `ILexEntryInflTypeRepository.AllInstances()`.
    pub lex_entry_infl_types: Vec<LexEntryInflType>,
    pub parser_parameters: ParserParameters,
}

/// A part-of-speech category. ← `PartOfSpeech` (a `CmPossibility` subclass; `Name`/
/// `Abbreviation`/`SubPossibilitiesOS` come from that base).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartOfSpeech {
    pub guid: Guid,
    /// ← `CmPossibility.Name` (best analysis alternative).
    pub name: String,
    /// ← `CmPossibility.Abbreviation` (best analysis alternative) — this is what `HCLoader`
    /// actually surfaces as the HC `FeatureSymbol` description (HCLoader.cs:172).
    pub abbreviation: String,
    /// ← `CmPossibility.SubPossibilitiesOS`, cast to `IPartOfSpeech`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PartOfSpeech>,
    /// Inflection classes owned directly by this POS (not inherited from an ancestor, not
    /// including subclasses of subclasses — each carries its own `children`).
    /// ← `PartOfSpeech.InflectionClassesOC`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inflection_classes: Vec<InflectionClass>,
    /// ← `PartOfSpeech.DefaultInflectionClassRA`. `HCLoader` walks this up the POS ownership
    /// chain when a stem's MSA doesn't specify one (`GetDefaultInflClass`, HCLoader.cs:2634-
    /// 2648); that walk is compiler logic over this same field on ancestor POSes, not separate
    /// data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_inflection_class: Option<Guid>,
    /// Morphosyntactic features this POS's words are inflected for. ← `PartOfSpeech.
    /// InflectableFeats` (`FsFeatDefn` guids; resolves against `featureSystems.morphosyntactic`
    /// `closedFeatures`/`complexFeatures`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inflectable_features: Vec<Guid>,
    /// ← `PartOfSpeech.StemNames`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stem_names: Vec<StemName>,
    /// ← `PartOfSpeech.AffixSlots`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affix_slots: Vec<AffixSlot>,
    /// ← `PartOfSpeech.AffixTemplates`, in declaration order; `HCLoader` skips `Disabled`
    /// templates and templates none of whose slots have any loaded affixes (HCLoader.cs:295-
    /// 300) — the snapshot keeps the full authored list, each template with its own `disabled`
    /// flag.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affix_templates: Vec<AffixTemplate>,
}

/// A user-authored "exception feature" / productivity-restriction possibility (also called a
/// "rule feature" or "ad hoc feature" in the LCM docs — an arbitrary, user-defined
/// `CmPossibility` list item with no further structure beyond identity/name). See
/// [`Morphology::exception_features`] for which two LCM sources feed this one registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionFeature {
    pub guid: Guid,
    /// ← `CmPossibility.Name` (best analysis alternative). `HCLoader` actually uses
    /// `CmObject.ShortName` for its internal `MprFeature.Name` (HCLoader.cs:573), which for a
    /// plain `CmPossibility` falls back to `Abbreviation` or `Name`; both are carried here so
    /// T3 can match that fallback if desired.
    pub name: String,
    pub abbreviation: String,
}

/// An inflection class (declension/conjugation class). ← `MoInflClass`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InflectionClass {
    pub guid: Guid,
    /// ← `MoInflClass.Name` (best analysis alternative).
    pub name: String,
    /// ← `MoInflClass.Abbreviation` (best analysis alternative). `HCLoader` represents each
    /// inflection class as an opaque `MprFeature` keyed by object identity, not by this string
    /// (`LoadMprFeature`, HCLoader.cs:571-577) — it is carried here for legibility/diagnostics.
    pub abbreviation: String,
    /// ← `MoInflClass.SubclassesOC` (`HCLoader.LoadInflClassMprFeature` recurses through these,
    /// HCLoader.cs:510-515).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<InflectionClass>,
}

/// A named region of a paradigm using a non-default stem. ← `MoStemName`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StemName {
    pub guid: Guid,
    /// ← `MoStemName.Name` (best analysis alternative) — this is what `HCLoader` uses
    /// (HCLoader.cs:221).
    pub name: String,
    /// ← `MoStemName.Abbreviation` (best analysis alternative). Not read by `HCLoader`, carried
    /// for completeness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abbreviation: Option<String>,
    /// Each region is a (POS + head-feature) combination this stem name applies to.
    /// ← `MoStemName.RegionsOC`, filtered to non-empty feature structures (HCLoader.cs:210,
    /// `fs.Where(fs => !fs.IsEmpty)`); resolves against `featureSystems.morphosyntactic`. A
    /// `StemName` with zero non-empty regions is dropped entirely by `HCLoader` (HCLoader.cs:
    /// 219-224, never added to `m_stemNames`/`m_language.StemNames`) — `pg-fwdata` still emits
    /// it (possibly with an empty `regions`), leaving that filtering decision to T3.
    pub regions: Vec<FeatureStructure>,
}

/// A position class an inflectional affix can occupy. ← `MoInflAffixSlot`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffixSlot {
    pub guid: Guid,
    /// ← `MoInflAffixSlot.Name` (best analysis alternative).
    pub name: String,
    /// ← `MoInflAffixSlot.Optional`.
    pub optional: bool,
}

/// An ordered sequence of affix slots defining how inflection works for a part of speech.
/// ← `MoInflAffixTemplate`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffixTemplate {
    pub guid: Guid,
    /// ← `MoInflAffixTemplate.Name` (best analysis alternative).
    pub name: String,
    /// ← `MoInflAffixTemplate.Disabled`.
    pub disabled: bool,
    /// Innermost-to-outermost prefix slots (right-to-left order, matching LCM's own
    /// `PrefixSlotsRS` convention). ← `MoInflAffixTemplate.PrefixSlotsRS`, each a guid into the
    /// owning POS's `affixSlots`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefix_slots: Vec<Guid>,
    /// Innermost-to-outermost suffix slots (left-to-right order). ← `MoInflAffixTemplate.
    /// SuffixSlotsRS`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suffix_slots: Vec<Guid>,
    /// Whether a derivation must end with this template to be well-formed.
    /// ← `MoInflAffixTemplate.Final` (HCLoader.cs:1678, `IsFinal = template.Final`).
    pub is_final: bool,
}

/// The morphological classification of a lexical form — the closed set of well-known
/// `MoMorphType` possibilities FieldWorks ships with. `pg-fwdata` (T2) owns the guid→variant
/// mapping table (the well-known `MoMorphTypeTags.kguidMorph*` constants, referenced by
/// `HCLoader.cs` at e.g. lines 524-568 [rule-form validity], 591-624 [stem/clitic
/// classification], 837-841 [bound root/stem], 1423-1435 and 1464-1520 [reduplication hints /
/// affix-process morph-type switch]) — those GUIDs live in the compiled `SIL.LCModel` package,
/// not in this source-available slice of the FieldWorks repo, so they are not reproduced here;
/// this format only needs the closed, named enumeration.
///
/// Bound-ness ("is this allomorph a bound root/stem?") is *derived* from this enum
/// (`BoundRoot`/`BoundStem`) rather than stored as a separate flag on
/// [`crate::lexicon::Allomorph`], matching `HCLoader`'s own `IsBound` derivation
/// (HCLoader.cs:835-841).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MorphType {
    Stem,
    BoundStem,
    Root,
    BoundRoot,
    Prefix,
    Suffix,
    Infix,
    Circumfix,
    Proclitic,
    Enclitic,
    Clitic,
    Particle,
    Phrase,
    DiscontigPhrase,
    PrefixingInterfix,
    InfixingInterfix,
    SuffixingInterfix,
}

/// The morphosyntactic-feature-and-inflection-class "shape" a compound rule requires of one of
/// its constituents. ← the `PartOfSpeechRA`/`ProdRestrictRC` pair read off an
/// `MoStemMsa` used as a compound-rule side (`LoadEndoCompoundingRule`/
/// `LoadExoCompoundingRule`, HCLoader.cs:1848-1941 — only these two properties of that
/// `MoStemMsa` are ever read for a compound side).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompoundConstituentRequirement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_of_speech: Option<Guid>,
    /// ← `MoStemMsa.ProdRestrictRC` ("exception features" / productivity restrictions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exception_features: Vec<Guid>,
}

/// The morphosyntactic outcome a compound rule assigns to the whole compound.
/// ← the `PartOfSpeechRA`/`InflectionClassRA` pair of the `MoStemMsa` used as a compound rule's
/// output MSA (`OverridingMsaOA` for endocentric, `ToMsaOA` for exocentric).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompoundOutcome {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_of_speech: Option<Guid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inflection_class: Option<Guid>,
}

/// A compounding rule. ← `MoCompoundRule` (`MoEndoCompound` / `MoExoCompound`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CompoundRule {
    /// The compound's morphosyntax comes from one of its own constituents (the "head"), with
    /// `overriding` able to override individual properties of that head. ← `MoEndoCompound`.
    Endocentric {
        guid: Guid,
        name: String,
        disabled: bool,
        /// ← `MoEndoCompound.HeadLast` (true = right-hand constituent is the head).
        head_last: bool,
        /// ← `MoEndoCompound.LeftMsaOA` (as a compound-side requirement).
        left: CompoundConstituentRequirement,
        /// ← `MoEndoCompound.RightMsaOA`.
        right: CompoundConstituentRequirement,
        /// ← `MoEndoCompound.OverridingMsaOA`.
        overriding: CompoundOutcome,
    },
    /// The compound's morphosyntax is stipulated directly, independent of either constituent.
    /// ← `MoExoCompound`.
    Exocentric {
        guid: Guid,
        name: String,
        disabled: bool,
        left: CompoundConstituentRequirement,
        right: CompoundConstituentRequirement,
        /// ← `MoExoCompound.ToMsaOA`.
        to: CompoundOutcome,
    },
}

impl CompoundRule {
    pub fn guid(&self) -> &str {
        match self {
            CompoundRule::Endocentric { guid, .. } | CompoundRule::Exocentric { guid, .. } => guid,
        }
    }

    pub fn disabled(&self) -> bool {
        match self {
            CompoundRule::Endocentric { disabled, .. }
            | CompoundRule::Exocentric { disabled, .. } => *disabled,
        }
    }
}

/// How close together two co-occurrence-prohibited items must be for the prohibition to fire.
/// ← `MoAdhocProhib.Adjacency` (an LCM `int` enum 0-4; `HCLoader.GetAdjacency`, HCLoader.cs:
/// 2241-2258).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Adjacency {
    Anywhere,
    SomewhereToLeft,
    SomewhereToRight,
    AdjacentToLeft,
    AdjacentToRight,
}

/// A co-occurrence prohibition: the `primary` item and every combination of one item from each
/// of `others`' original groups may never all appear together (subject to `adjacency`).
/// ← `MoAdhocProhib` (`MoAlloAdhocProhib` / `MoMorphAdhocProhib`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AdhocProhibition {
    /// ← `MoAlloAdhocProhib`. `primary` ← `FirstAllomorphRA` (an `MoForm` guid, resolved
    /// against `lexicon.entries[*].allomorphs`); `others` ← `RestOfAllosRS`, in order.
    Allomorph {
        guid: Guid,
        disabled: bool,
        primary: Guid,
        others: Vec<Guid>,
        adjacency: Adjacency,
    },
    /// ← `MoMorphAdhocProhib`. `primary` ← `FirstMorphemeRA` (an `MoMorphSynAnalysis` guid,
    /// resolved against `lexicon.entries[*].msas`); `others` ← `RestOfMorphsRS`, in order.
    Morpheme {
        guid: Guid,
        disabled: bool,
        primary: Guid,
        others: Vec<Guid>,
        adjacency: Adjacency,
    },
}

/// An irregular-inflected-form variant type: tags a variant `LexEntry` as realizing a specific
/// inflectional cell without an overt inflectional affix (e.g. English "went" as the past-tense
/// `LexEntryInflType` of "go"). ← `LexEntryInflType`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LexEntryInflType {
    pub guid: Guid,
    /// ← `CmPossibility.Name` (best analysis alternative; `LexEntryInflType`'s base class).
    pub name: String,
    /// ← `CmPossibility.Abbreviation` (best analysis alternative).
    pub abbreviation: String,
    /// Gloss text prepended to the base gloss unless it is the literal placeholder `"***"`
    /// (`HCLoader` checks for that sentinel and omits it, HCLoader.cs:751-753).
    /// ← `LexEntryInflType.GlossPrepend` (best analysis alternative).
    pub gloss_prepend: String,
    /// ← `LexEntryInflType.GlossAppend` (best analysis alternative), same `"***"` sentinel
    /// convention as `gloss_prepend`.
    pub gloss_append: String,
    /// Slots this irregular form should be treated as filling, for template-scope purposes.
    /// ← `LexEntryInflType.SlotsRC`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<Guid>,
    /// ← `LexEntryInflType.InflFeatsOA`, when non-empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inflection_features: Option<FeatureStructure>,
}

/// The `<ParserParameters><HC>` block plus per-compound-rule `maxApps`.
/// ← `MorphologicalDataOA.ParserParameters` (a raw XML string LCM stores verbatim; parsed by
/// `HCLoader`'s constructor, HCLoader.cs:92-112).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserParameters {
    /// Whether phonological rules should apply on the clitic stratum instead of the
    /// morphophonemic one. ← `<NotOnClitics>`, default **`true`** when the `<HC>` element is
    /// absent or the sub-element is absent (HCLoader.cs:95 — note the unusual default-true
    /// polarity, preserved here rather than normalized away).
    pub not_on_clitics: bool,
    /// ← `<AcceptUnspecifiedGraphemes>`, default `false`.
    pub accept_unspecified_graphemes: bool,
    /// ← `<NoDefaultCompounding>`, default `false`.
    pub no_default_compounding: bool,
    /// The raw, unparsed `<Strata>` string (e.g. `"Morphology,(Clitics,Templates),Phonology"`)
    /// — `HCLoader.ParseStrataString`'s comma/parenthesis tokenizer (HCLoader.cs:120-151) is
    /// compiler policy over this string, not separate data, so T3 re-parses it the same way.
    /// `None` when the project has no `<Strata>` element (the common case: default stratum
    /// layout applies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strata: Option<String>,
    /// ← `<CompoundRules>`'s per-rule `maxApps` attribute, keyed by the `MoCompoundRule` guid
    /// (HCLoader.cs:103-112). A compound rule with no entry here defaults to `maxApps = 1`
    /// (HCLoader.cs:1894, `int maxApps = 1;`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compound_rule_max_applications: Vec<CompoundRuleMaxApplications>,
}

impl Default for ParserParameters {
    /// Mirrors `HCLoader`'s constructor defaults when no `<ParserParameters><HC>` block is
    /// present at all (HCLoader.cs:93-96: `hcElem == null`).
    fn default() -> Self {
        ParserParameters {
            not_on_clitics: true,
            accept_unspecified_graphemes: false,
            no_default_compounding: false,
            strata: None,
            compound_rule_max_applications: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompoundRuleMaxApplications {
    pub compound_rule: Guid,
    pub max_applications: u32,
}
