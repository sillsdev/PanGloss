//! The `phonology` snapshot section: phoneme inventory, boundary markers, natural classes,
//! environments, and phonological (rewrite/metathesis) rules.
//!
//! Reference: `HCLoader.cs` char-def table 2669-2743, natural classes 2788-2829, environment
//! tokenizer 2260-2457, rewrite rules 2003-2101, metathesis 2103-2161, feature structures
//! 2500-2530.

use serde::{Deserialize, Serialize};

use crate::common::{Guid, WsForm};
use crate::feature::FeatureStructure;

/// The `phonology` snapshot section.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Phonology {
    /// ← `PhPhonemeSet.PhonemesOC` of `PhonologicalDataOA.PhonemeSetsOS[0]` (HCLoader only ever
    /// loads the first phoneme set, HCLoader.cs:204/2669).
    pub phonemes: Vec<Phoneme>,
    /// ← `PhPhonemeSet.BoundaryMarkersOC`, excluding the special word-boundary marker
    /// (`LangProjectTags.kguidPhRuleWordBdry`, HCLoader.cs:2698), which is represented instead
    /// by [`PhonContext::WordBoundary`] wherever it appears in a rule/environment context.
    pub boundary_markers: Vec<BoundaryMarker>,
    /// ← `PhonologicalDataOA.NaturalClassesOS`.
    pub natural_classes: Vec<NaturalClass>,
    /// ← `PhonologicalDataOA.EnvironmentsOS` (reached indirectly, via every allomorph/rule that
    /// references one — `pg-fwdata` collects the closure of referenced environments).
    pub environments: Vec<Environment>,
    /// ← `PhonologicalDataOA.PhonRulesOS`, in `OrderNumber` order, skipping `Disabled` rules
    /// (HCLoader.cs:302).
    pub rules: Vec<PhonologicalRule>,
    /// ← `PhonologicalDataOA.FeatConstraints` (referenced by natural-class contexts inside rule
    /// patterns to express SPE-style alpha-variable agreement/disagreement).
    pub feature_constraints: Vec<FeatureConstraint>,
}

/// A single phoneme. ← `PhPhoneme`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Phoneme {
    pub guid: Guid,
    /// ← `PhPhoneme.Name` (best analysis alternative) — note this is distinct from the
    /// grapheme `representations` below; `HCLoader` itself has no notion of a phoneme "name"
    /// separate from its representations, but the LCM object has one and it is useful for
    /// diagnostics, so it is carried here.
    pub name: String,
    /// Grapheme spellings, one per writing system. ← `PhPhoneme.CodesOS[*].Representation`
    /// (`IPhCode.Representation`, a `MultiUnicode`), dotted-circle (U+25CC) stripped
    /// (`HCLoader.RemoveDottedCircles`, HCLoader.cs:2678-2680). A phoneme with zero
    /// representations after stripping is a load error in FieldWorks
    /// (`IHCLoadErrorLogger.InvalidPhoneme`); `pg-fwdata` reports the same condition as an
    /// import warning rather than omitting the phoneme.
    pub representations: Vec<WsForm>,
    /// ← `PhPhoneme.FeaturesOA`, when non-empty (HCLoader.cs:2675-2676). Feature guids resolve
    /// against `featureSystems.phonological`.
    pub features: Option<FeatureStructure>,
    /// ← `PhPhoneme.BasicIPASymbol`. Not read by `HCLoader` (it works purely from
    /// `representations`), but carried for completeness — a good fallback description of a
    /// phoneme when displaying diagnostics.
    pub basic_ipa_symbol: Option<String>,
}

/// A morpheme/word boundary marker (e.g. `+`), distinct from the phoneme inventory.
/// ← `PhBdryMarker`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryMarker {
    pub guid: Guid,
    /// ← `PhBdryMarker.Name` (best analysis alternative).
    pub name: String,
    /// ← `PhBdryMarker.CodesOS[*].Representation` (best vernacular alternative;
    /// HCLoader.cs:2700-2702).
    pub representations: Vec<WsForm>,
}

/// A natural class: either an explicit list of member phonemes (extensional, `PhNCSegments`) or
/// a feature-value description (intensional, `PhNCFeatures`). ← `PhNaturalClass`
/// (`HCLoader.TryLoadNaturalClass`, HCLoader.cs:2788-2829).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum NaturalClass {
    /// ← `PhNCSegments.SegmentsRC` — the class is exactly these phonemes.
    Segments {
        guid: Guid,
        /// ← `PhNaturalClass.Abbreviation` (best analysis alternative) — this, not `Name`, is
        /// what `HCLoader` uses (HCLoader.cs:2825) and what environment strings reference in
        /// `[Abbr]` bracket notation.
        name: String,
        phonemes: Vec<Guid>,
    },
    /// ← `PhNCFeatures.FeaturesOA` — the class is every phoneme whose feature structure
    /// includes these feature values.
    Features {
        guid: Guid,
        name: String,
        features: FeatureStructure,
    },
}

/// A phonological environment: FieldWorks stores these as a hand-authored string like
/// `/_[UnVDent]` or `/#_C` (`PhEnvironment.StringRepresentation`) rather than as a structured
/// tree, and `HCLoader` tokenizes/validates that string at load time
/// (`TokenizeContext`/`IsValidEnvironment`, HCLoader.cs:2260-2457). This format keeps the raw
/// string as-is — re-tokenizing (and re-validating natural-class references inside it) is a
/// compiler (T3) concern, matching `HCLoader`'s own approach of parsing it lazily and tolerantly
/// (a malformed environment is a warning, not a hard failure, HCLoader.cs:1184-1197).
/// ← `PhEnvironment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub guid: Guid,
    /// ← `PhEnvironment.Name` (best analysis alternative); may be empty — FieldWorks users
    /// often leave environments unnamed, identifying them by `StringRepresentation` alone.
    pub name: String,
    /// ← `PhEnvironment.StringRepresentation`, e.g. `/_[UnVDent]` or `/#[C]_`.
    pub representation: String,
}

/// A single alpha-variable slot for expressing agreement/disagreement between a phonological
/// rule's structural description and its structural change (SPE-style Greek-letter variables:
/// α, β, γ, ...; `HCLoader.VariableNames`, HCLoader.cs:37-41). ← `PhFeatureConstraint`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureConstraint {
    pub guid: Guid,
    /// The feature this constraint ranges over. ← `PhFeatureConstraint.FeatureRA`, resolved
    /// against `featureSystems.phonological`.
    pub feature: Guid,
}

/// A phonological context: the recursive pattern-tree type used by rewrite-rule structural
/// descriptions/changes, environment left/right sides (in a fully structured world; this format
/// keeps environments as raw strings, see [`Environment`]), and affix-process input/output
/// (§ [`crate::lexicon::AffixProcess`]). Mirrors the LCM `PhPhonContext` hierarchy exactly as
/// `HCLoader.LoadPatternNode` consumes it (HCLoader.cs:2313-2389), plus `IPhVariable` (used only
/// inside `MoAffixProcess.InputOS`, HCLoader.cs:1338-1346) folded in as [`PhonContext::Variable`]
/// so callers do not need a second parallel type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PhonContext {
    /// ← `PhSequenceContext.MembersRS`.
    Sequence { members: Vec<PhonContext> },
    /// ← `PhIterationContext` (`Minimum`/`Maximum`, `MemberRA`). `max: -1` means unbounded
    /// (mirrors the LCM/HC convention, HCLoader.cs:2343).
    Iteration {
        min: i32,
        max: i32,
        member: Box<PhonContext>,
    },
    /// A single phoneme. ← `PhSimpleContextSeg.FeatureStructureRA` (a `PhPhoneme` guid).
    Segment { phoneme: Guid },
    /// A natural-class match, optionally constrained by alpha-variable agreement/disagreement.
    /// ← `PhSimpleContextNC.FeatureStructureRA` (the natural class), `.PlusConstrRS` /
    /// `.MinusConstrRS` (`FeatureConstraint` guids this occurrence must agree/disagree on with
    /// other occurrences of the same constraint elsewhere in the same rule; HCLoader.cs:2745-
    /// 2763).
    NaturalClass {
        natural_class: Guid,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        plus_variables: Vec<Guid>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        minus_variables: Vec<Guid>,
    },
    /// A user-defined boundary marker. ← `PhSimpleContextBdry.FeatureStructureRA`, when it is
    /// *not* the special word-boundary marker (see [`PhonContext::WordBoundary`]).
    Boundary { marker: Guid },
    /// The special word-initial/word-final anchor (`#` in environment notation).
    /// ← `PhSimpleContextBdry.FeatureStructureRA.Guid == LangProjectTags.kguidPhRuleWordBdry`
    /// (HCLoader.cs:2351/2489-2498) — deliberately *not* modeled as a `Boundary` referencing
    /// that well-known guid, since that guid does not correspond to any real entry in
    /// `boundary_markers` (`LoadCharacterDefinitionTable` explicitly excludes it,
    /// HCLoader.cs:2698).
    WordBoundary,
    /// "Match anything" — used only inside `MoAffixProcess.InputOS` as a placeholder for a
    /// stretch of input that is not itself constrained (`IPhVariable`, HCLoader.cs:1340-1346).
    /// Never appears inside a rewrite-rule/environment pattern tree in FieldWorks data (those
    /// only ever contain the other five variants); T3 should treat one appearing there as
    /// malformed input rather than crash.
    Variable,
}

/// Left-to-right/right-to-left/simultaneous rule application direction.
/// ← `PhSegmentRule.Direction` / `PhRegularRule.Direction` (an LCM `int` enum: 0/1/2;
/// HCLoader.cs:2015-2031) and `PhMetathesisRule.Direction` (0/1/2, where 2 also behaves as
/// left-to-right for metathesis, HCLoader.cs:2107-2117). `HCLoader` additionally derives an
/// `ApplicationMode` (`Iterative` vs `Simultaneous`) from a regular rule's direction (0/1 →
/// iterative, 2 → simultaneous); that is compiler policy, not stored data, so it is not
/// represented here — T3 re-derives it from `Simultaneous` the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleDirection {
    LeftToRight,
    RightToLeft,
    Simultaneous,
}

/// A phonological rule: either a regular rewrite rule or a metathesis rule.
/// ← `PhSegmentRule` (`PhRegularRule` / `PhMetathesisRule`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PhonologicalRule {
    Rewrite(RewriteRule),
    Metathesis(MetathesisRule),
}

/// A structural-change (`A -> B / C _ D`) rewrite rule. ← `PhRegularRule`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteRule {
    pub guid: Guid,
    /// ← `PhSegmentRule.Name` (best analysis alternative).
    pub name: String,
    pub direction: RuleDirection,
    /// The rule's structural description (the general shape it matches, before any right-hand
    /// side's more specific structural change/environment is applied). ← `PhRegularRule.
    /// StrucDescOS` (a list of `IPhSimpleContext`, HCLoader.cs:2033-2044).
    pub structural_description: Vec<PhonContext>,
    /// The alpha-variables available to `structural_description`/`right_hand_sides`' natural-
    /// class contexts, in the order `HCLoader` assigns Greek letters to them (α, β, γ, ...).
    /// ← `PhRegularRule.FeatureConstraints` (HCLoader.cs:2005-2011); each entry is a
    /// [`FeatureConstraint`] guid also present in `Phonology.feature_constraints`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feature_constraint_variables: Vec<Guid>,
    /// ← `PhRegularRule.RightHandSidesOS` — a rewrite rule may have several right-hand sides,
    /// each with its own change/environment/POS-and-rule-feature conditions.
    pub right_hand_sides: Vec<RewriteRhs>,
}

/// One right-hand side of a [`RewriteRule`]. ← `PhSegRuleRHS`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteRhs {
    /// ← `PhSegRuleRHS.StrucChangeOS` (a list of `IPhSimpleContext`, HCLoader.cs:2060-2071).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structural_change: Vec<PhonContext>,
    /// ← `PhSegRuleRHS.LeftContextOA`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_context: Option<PhonContext>,
    /// ← `PhSegRuleRHS.RightContextOA`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_context: Option<PhonContext>,
    /// Parts of speech the rule requires of the word it applies within.
    /// ← `PhSegRuleRHS.InputPOSesRC`, resolved against `morphology.partsOfSpeech`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_parts_of_speech: Vec<Guid>,
    /// "Rule features" the rule requires be present. ← `PhSegRuleRHS.ReqRuleFeatsRC`
    /// (`IPhPhonRuleFeat.ItemRA`, an `MoInflClass` or `CmPossibility`; HCLoader.cs:2610-2623).
    /// Each guid is the referenced `MoInflClass`/`CmPossibility` itself, *not* expanded to its
    /// subclass closure (`HCLoader.LoadAllInflClasses` does that expansion at compile time,
    /// HCLoader.cs:2593-2608) — T3 must perform the same expansion using
    /// `morphology.inflectionClasses`' hierarchy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_rule_features: Vec<Guid>,
    /// ← `PhSegRuleRHS.ExclRuleFeatsRC`, same shape as `required_rule_features`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_rule_features: Vec<Guid>,
}

/// A metathesis rule (swaps two parts of a matched sequence). ← `PhMetathesisRule`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetathesisRule {
    pub guid: Guid,
    pub name: String,
    pub direction: RuleDirection,
    /// The full matched pattern, left to right. ← `PhMetathesisRule.StrucDescOS`.
    pub structural_description: Vec<PhonContext>,
    /// 0-based index into `structural_description` of the part that moves right of
    /// `right_switch_index`. ← `PhMetathesisRule.LeftSwitchIndex`.
    pub left_switch_index: i32,
    /// 0-based index into `structural_description` of the part that moves left of
    /// `left_switch_index`. ← `PhMetathesisRule.RightSwitchIndex`.
    ///
    /// `HCLoader` additionally computes derived "middle"/"left env"/"right env" part indices
    /// from these two (`GetStrucChangeIndices`, HCLoader.cs:2119-2120) under an assumed
    /// canonical layout; that derivation is compiler policy over already-complete data, not
    /// separately stored LCM data, so it is not duplicated here — T3 re-derives it the same way.
    pub right_switch_index: i32,
}
