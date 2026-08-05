//! The `lexicon` snapshot section: entries, their allomorphs, morphosyntactic analyses (MSAs),
//! senses, and entry references (variants/complex forms).
//!
//! Reference: `HCLoader.cs` stems 626-731, variants 733-807, root allomorphs 809-845, affix
//! rules 847-1046, affix-process allomorphs incl. circumfix cross-product 1048-1332.

use serde::{Deserialize, Serialize};

use crate::common::{Guid, WsForm};
use crate::feature::FeatureStructure;
use crate::morphology::MorphType;
use crate::phonology::PhonContext;

/// The `lexicon` snapshot section.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lexicon {
    /// ← `LangProject.LexDbOA.Entries`.
    pub entries: Vec<LexEntry>,
}

/// A dictionary entry. ← `LexEntry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LexEntry {
    pub guid: Guid,
    /// ← `LexEntry.CitationForm`, one string per writing system.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citation_form: Vec<WsForm>,
    /// The morph type of this entry's lexeme form — a convenience cache of
    /// `allomorphs.last().morphType` (the lexeme form is always ordered last, see `allomorphs`
    /// below), *not* a distinct LCM property (LCM's `MorphType` lives on `MoForm`/allomorphs,
    /// not on `LexEntry` itself). Provided because `HCLoader` repeatedly branches on exactly
    /// this value at the entry level without needing a particular allomorph in hand — e.g.
    /// `ILexEntry.IsCircumfix()` and `HasValidRuleForm` (HCLoader.cs:517-534) both key off
    /// `entry.LexemeFormOA.MorphTypeRA`.
    pub lexeme_morph_type: MorphType,
    /// In HCLoader's own iteration order: `AlternateFormsOS` first, then `LexemeFormOA` last
    /// (HCLoader.cs:263, `entry.AlternateFormsOS.Concat(entry.LexemeFormOA)`). This order
    /// matters for allomorph/environment disjunctive-ordering semantics (`MasterLCModel.xml`
    /// `LexEntry.AlternateForms` doc: "ordering... is relevant if their environments of
    /// occurrence are taken to be disjunctively ordered").
    pub allomorphs: Vec<Allomorph>,
    /// ← `LexEntry.MorphoSyntaxAnalysesOC`. Unordered in LCM (a collection, not a sequence);
    /// order here is whatever `pg-fwdata` encountered them in (stable per import, not
    /// semantically meaningful).
    pub msas: Vec<Msa>,
    /// ← `LexEntry.SensesOS`, in order.
    pub senses: Vec<Sense>,
    /// ← `LexEntry.EntryRefsOS` — variant/complex-form links from this entry to other entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entry_refs: Vec<EntryRef>,
}

/// One allomorph (alternate form) of a `LexEntry`. Covers all three `MoForm` subclasses LCM
/// distinguishes (`MoStemAllomorph`, `MoAffixAllomorph`, `MoAffixProcess`) in a single shape:
/// stem-only fields (`stem_name`) and process-only fields (`process`) are simply absent/`None`
/// for the other two kinds; `forms`/`environments`/`inflection_classes`/`ms_env_features` are
/// shared by (subsets of) all three. See `docs/snapshot-format.md` for the exact field-to-kind
/// applicability table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Allomorph {
    pub guid: Guid,
    /// ← `MoForm.MorphTypeRA`.
    pub morph_type: MorphType,
    /// ← `MoForm.IsAbstract`.
    pub is_abstract: bool,
    /// ← `MoForm.Form`, one string per writing system. May contain FieldWorks' lexical-pattern
    /// bracket notation for reduplication/underspecified segments (e.g. `[C][V]d`, `-[...]`)
    /// left verbatim — `HCLoader.IsLexicalPattern`/`Segment` (HCLoader.cs:2532-2571) parse that
    /// notation at compile time, so T3 must do the same rather than treating brackets as
    /// literal characters. Empty for an `MoAffixProcess`-only allomorph (`process` is `Some`
    /// instead; `MasterLCModel.xml` `MoForm.Form` doc: "not clear... what is supposed to go
    /// into this attribute for process affixes").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forms: Vec<WsForm>,
    /// Phonetic conditioning environments. ← `MoStemAllomorph.PhoneEnvRC` /
    /// `MoAffixAllomorph.PhoneEnvRC`, each a guid into `phonology.environments`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environments: Vec<Guid>,
    /// Infix positioning environments (e.g. "after the first consonant"). ← `MoAffixAllomorph.
    /// PositionRS`, guids into `phonology.environments` (the same object kind as
    /// `environments` — `PositionRS` and `PhoneEnvRC` are both `PhEnvironment` sequences/
    /// collections; `HCLoader.GetAffixAllomorphEnvironments` concatenates the two,
    /// HCLoader.cs:1167-1170). Empty for stem allomorphs (`MoStemAllomorph` has no `Position`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub positions: Vec<Guid>,
    /// ← `MoStemAllomorph.StemNameRA`. `None` for affix allomorphs/processes (no such
    /// property).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stem_name: Option<Guid>,
    /// Inflection classes this allomorph is restricted to (an affix with several allomorphs
    /// differing only by inflection-class requirement). ← `MoAffixForm.InflectionClassesRC`
    /// (shared by `MoAffixAllomorph` and `MoAffixProcess`, HCLoader.cs:1057/1096/1115).
    /// Always empty for stem allomorphs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inflection_classes: Vec<Guid>,
    /// Morphosyntactic features required of the stem this allomorph attaches to (independent of
    /// the affix's own MSA-level requirement). ← `MoAffixAllomorph.MsEnvFeaturesOA`, when
    /// non-empty (HCLoader.cs:1124-1128). Resolves against `featureSystems.morphosyntactic`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms_env_features: Option<FeatureStructure>,
    /// A part-of-speech companion constraint to `ms_env_features`. ← `MoAffixAllomorph.
    /// MsEnvPartOfSpeechRA`. **Not currently read by `HCLoader`** (no reference to this
    /// property was found in `HCLoader.cs`); included for schema completeness since it is the
    /// direct sibling of `ms_env_features` in the LCM model and cheap to carry — T3 may ignore
    /// it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms_env_part_of_speech: Option<Guid>,
    /// Present only for `MoAffixProcess` allomorphs (rule-based, non-concatenative
    /// realizations — ablaut, templatic morphology, etc.); `None` for `MoStemAllomorph`/
    /// `MoAffixAllomorph` (whose shape is `forms` + `environments`/`positions` instead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<AffixProcess>,
}

/// A rule-based (`MoAffixProcess`) allomorph realization: an input pattern paired with an
/// ordered list of output actions. ← `MoAffixProcess.InputOS` / `.OutputOS`
/// (`HCLoader.LoadAffixProcessAllomorph`, HCLoader.cs:1334-1439).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffixProcess {
    /// Each entry is one numbered "part" of the input, referenced positionally (1-based, in
    /// list order) by `CopyFromInput`/`ModifyFromInput` in `output`. ← `MoAffixProcess.InputOS`
    /// (a `PhContextOrVar` sequence: either a `PhonContext::Variable` placeholder or a
    /// concrete phonological context).
    pub input: Vec<PhonContext>,
    /// ← `MoAffixProcess.OutputOS` (`MoRuleMapping` sequence), in order.
    pub output: Vec<RuleMapping>,
}

/// One step of an `AffixProcess`'s output construction. ← `MoRuleMapping`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RuleMapping {
    /// Insert a phoneme matching this natural class (no further feature constraints — LCM's
    /// `MoInsertNC.ContentRA` is a plain `IPhNaturalClass`, not a variable-constrained simple
    /// context). ← `MoInsertNC.ContentRA`, resolved against `phonology.naturalClasses`.
    InsertNaturalClass { natural_class: Guid },
    /// Copy the stretch of input matched by the numbered input part through unchanged.
    /// ← `MoCopyFromInput.ContentRA` (`HCLoader` resolves this to a 1-based index via
    /// `ContentRA.IndexInOwner + 1`, HCLoader.cs:1383).
    CopyFromInput { part: u32 },
    /// Insert this literal phoneme/boundary sequence (a plain surface string, re-segmented
    /// against the phoneme/boundary inventory by whatever consumes this snapshot — same
    /// "already-formatted text, tokenize downstream" philosophy as `Environment.
    /// representation`). ← `MoInsertPhones.ContentRS` (`IPhTerminalUnit` sequence, concatenated
    /// to a string by `HCLoader`, HCLoader.cs:1388-1406).
    InsertSegments { text: String },
    /// Replace the numbered input part's feature values with this natural class's feature
    /// values. ← `MoModifyFromInput.ContentRA` (the 1-based part index) / `.ModificationRA`
    /// (the natural class), HCLoader.cs:1409-1419.
    ModifyFromInput { part: u32, natural_class: Guid },
}

/// A morphosyntactic analysis — what an entry's morpheme *is*, grammatically: a stem/root, an
/// inflectional affix, a derivational affix, or an affix of unclassified function. Tagged union
/// over `MoMorphSynAnalysis`'s four concrete subclasses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Msa {
    /// A stem/root/clitic MSA. ← `MoStemMsa`. (The same LCM class also serves as a compound
    /// rule's constituent-requirement/outcome MSA and — when this `LexEntry`'s allomorphs are
    /// clitic-type morph types — as a clitic's own "morphological rule" MSA,
    /// `LoadCliticAffixProcessRule`, HCLoader.cs:1030-1046; hence `from_parts_of_speech` and
    /// `slots` below, which only apply in that clitic case.)
    Stem {
        guid: Guid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part_of_speech: Option<Guid>,
        /// ← `MoStemMsa.InflectionClassRA`. `None` means "use the owning POS's
        /// `default_inflection_class`" (`HCLoader.GetInflClass`, HCLoader.cs:2625-2632).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inflection_class: Option<Guid>,
        /// ← `MoStemMsa.MsFeaturesOA`, when non-empty.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        features: Option<FeatureStructure>,
        /// ← `MoStemMsa.ProdRestrictRC` ("exception features").
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exception_features: Vec<Guid>,
        /// Categories this (clitic) stem may attach to. ← `MoStemMsa.FromPartsOfSpeechRC`
        /// ("applies only to clitics", per `MasterLCModel.xml`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        from_parts_of_speech: Vec<Guid>,
        /// Inflectional-affix-template slots this (clitic) stem occupies. ← `MoStemMsa.
        /// SlotsRC` ("applies only to clitics").
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        slots: Vec<Guid>,
    },
    /// An inflectional affix MSA. ← `MoInflAffMsa`.
    Inflectional {
        guid: Guid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part_of_speech: Option<Guid>,
        /// ← `MoInflAffMsa.SlotsRC` — the affix-template slot(s) this affix can fill. Empty
        /// means "partial" (`HCLoader` sets `IsPartial = msa.SlotsRC.Count == 0`,
        /// HCLoader.cs:982) and the rule applies outside any template
        /// (HCLoader.cs:889-890, `stratum = null`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        slots: Vec<Guid>,
        /// ← `MoInflAffMsa.InflFeatsOA`, when non-empty.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        features: Option<FeatureStructure>,
        /// ← `MoInflAffMsa.FromProdRestrictRC` (restriction classes required of the stem this
        /// affix attaches to).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exception_features: Vec<Guid>,
    },
    /// A derivational affix MSA (changes category and/or features from input to output).
    /// ← `MoDerivAffMsa`.
    Derivational {
        guid: Guid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_part_of_speech: Option<Guid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_part_of_speech: Option<Guid>,
        /// ← `MoDerivAffMsa.FromMsFeaturesOA`, when non-empty.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_features: Option<FeatureStructure>,
        /// ← `MoDerivAffMsa.ToMsFeaturesOA`, when non-empty.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_features: Option<FeatureStructure>,
        /// ← `MoDerivAffMsa.FromInflectionClassRA`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_inflection_class: Option<Guid>,
        /// ← `MoDerivAffMsa.ToInflectionClassRA`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_inflection_class: Option<Guid>,
        /// ← `MoDerivAffMsa.FromProdRestrictRC`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        from_exception_features: Vec<Guid>,
        /// ← `MoDerivAffMsa.ToProdRestrictRC`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        to_exception_features: Vec<Guid>,
        /// The named stem-region this affix requires (e.g. attaches only to the "3-stem").
        /// ← `MoDerivAffMsa.FromStemNameRA`, resolved against the owning POS's `stemNames`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_stem_name: Option<Guid>,
    },
    /// An affix whose grammatical function has not been (or cannot be) classified as
    /// inflectional or derivational. ← `MoUnclassifiedAffixMsa`.
    Unclassified {
        guid: Guid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part_of_speech: Option<Guid>,
    },
}

impl Msa {
    pub fn guid(&self) -> &str {
        match self {
            Msa::Stem { guid, .. }
            | Msa::Inflectional { guid, .. }
            | Msa::Derivational { guid, .. }
            | Msa::Unclassified { guid, .. } => guid,
        }
    }
}

/// A word sense. ← `LexSense`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sense {
    pub guid: Guid,
    /// ← `LexSense.Gloss`, one string per writing system.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gloss: Vec<WsForm>,
    /// ← `LexSense.Definition`, one string per writing system.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub definition: Vec<WsForm>,
    /// ← `LexSense.MorphoSyntaxAnalysisRA`, a guid into this same entry's `msas`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msa: Option<Guid>,
}

/// A link from a variant or complex-form entry to the entry/entries it relates to.
/// ← `LexEntryRef`, discriminated on whether `VariantEntryTypesRS` or `ComplexEntryTypesRS` was
/// populated (a `LexEntryRef` can in principle carry both, but FieldWorks UI treats them as
/// mutually exclusive per `MasterLCModel.xml`'s `LexEntryRef` doc; `pg-fwdata` must pick a
/// `kind` when both are non-empty, defaulting to `Variant`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum EntryRef {
    /// ← `LexEntryRef` with `VariantEntryTypesRS` populated. `HCLoader.LoadLexEntries`/
    /// `LoadMorphologicalRules` walk exactly this shape when `entry.SensesOS.Count == 0`
    /// (HCLoader.cs:628-651, 852-871) to attach this variant's allomorphs to the *main* entry's
    /// MSAs instead of its own (absent) senses.
    Variant {
        guid: Guid,
        /// ← `LexEntryRef.ComponentLexemesRS` — for variants, the main entry/sense this is a
        /// variant of. Each guid is a `LexEntry` or a `Sense` guid (LCM's `CmObject` union);
        /// resolvers must check both.
        component_lexemes: Vec<Guid>,
        /// ← `LexEntryRef.VariantEntryTypesRS`. Each guid may be a plain `LexEntryType` (no
        /// further data beyond identity) or a `crate::morphology::LexEntryInflType` guid (for
        /// irregularly-inflected-form variants) — resolve against
        /// `morphology.lexEntryInflTypes` first, falling back to "plain variant type, no
        /// further semantics" if absent (mirrors `HCLoader.GetInflTypes`'s `as ILexEntryInflType`
        /// downcast, HCLoader.cs:657-679).
        variant_entry_types: Vec<Guid>,
    },
    /// ← `LexEntryRef` with `ComplexEntryTypesRS` populated (compounds, idioms, derivatives
    /// recorded as their own headword pointing back at components). Not walked by `HCLoader`
    /// today (it only follows the `Variant` shape) — carried for completeness /
    /// forward-compatibility with a future compiler that reconstructs complex forms.
    ComplexForm {
        guid: Guid,
        /// ← `LexEntryRef.ComponentLexemesRS` — the components making up this complex form.
        component_lexemes: Vec<Guid>,
        /// ← `LexEntryRef.ComplexEntryTypesRS`.
        complex_entry_types: Vec<Guid>,
    },
}
