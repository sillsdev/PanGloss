//! Feature switches, refused constructs, character kinds and the width classes.

use std::collections::BTreeSet;

/// One independent generator switch; a word counts for it only when HC used a rule tagged with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    RewriteSubstitute,
    RewriteDelete,
    RewriteInsert,
    ModeSimultaneous,
    ModeIterative,
    DirLeftToRight,
    DirRightToLeft,
    FocusMultiToken,
    FocusQuantified,
    FocusAnchored,
    AlphaVariable,
    AlphaAcrossGap,
    ClassByFeature,
    ClassBySegments,
    BoundaryInAffixInsert,
    BoundaryAtJunction,
    BoundaryInRuleContext,
    AdjacentMetathesis,
    Prefix,
    Suffix,
    Circumfix,
    Infix,
    BoundedTruncation,
    Modify,
    InsertContext,
    PartialReduplication,
    FullReduplication,
    Reordering,
    NullAffix,
    TemplateOptionalSlot,
    TemplateRequiredSlot,
    RealizationalRule,
    RealizationalInRuleList,
    BlockCompounding,
    CompoundTwoAllomorphs,
    LinearStratum,
    UnorderedStratum,
    MultipleStrata,
    PhonologyInSeveralStrata,
    MaxAppsTwo,
    SurfaceTransparentInterior,
    TwoDerivationsOneIdentity,
    TwoNullAffixesOneZone,
    DropEnvironments,
    DropMprFeatures,
    DropStemNames,
    DropSyntacticFeatures,
    CliticsLayout,
    RootsInUpperStrata,
}

impl Feature {
    pub const ALL: [Feature; 49] = [
        Feature::RewriteSubstitute,
        Feature::RewriteDelete,
        Feature::RewriteInsert,
        Feature::ModeSimultaneous,
        Feature::ModeIterative,
        Feature::DirLeftToRight,
        Feature::DirRightToLeft,
        Feature::FocusMultiToken,
        Feature::FocusQuantified,
        Feature::FocusAnchored,
        Feature::AlphaVariable,
        Feature::AlphaAcrossGap,
        Feature::ClassByFeature,
        Feature::ClassBySegments,
        Feature::BoundaryInAffixInsert,
        Feature::BoundaryAtJunction,
        Feature::BoundaryInRuleContext,
        Feature::AdjacentMetathesis,
        Feature::Prefix,
        Feature::Suffix,
        Feature::Circumfix,
        Feature::Infix,
        Feature::BoundedTruncation,
        Feature::Modify,
        Feature::InsertContext,
        Feature::PartialReduplication,
        Feature::FullReduplication,
        Feature::Reordering,
        Feature::NullAffix,
        Feature::TemplateOptionalSlot,
        Feature::TemplateRequiredSlot,
        Feature::RealizationalRule,
        Feature::RealizationalInRuleList,
        Feature::BlockCompounding,
        Feature::CompoundTwoAllomorphs,
        Feature::LinearStratum,
        Feature::UnorderedStratum,
        Feature::MultipleStrata,
        Feature::PhonologyInSeveralStrata,
        Feature::MaxAppsTwo,
        Feature::SurfaceTransparentInterior,
        Feature::TwoDerivationsOneIdentity,
        Feature::TwoNullAffixesOneZone,
        Feature::DropEnvironments,
        Feature::DropMprFeatures,
        Feature::DropStemNames,
        Feature::DropSyntacticFeatures,
        Feature::CliticsLayout,
        Feature::RootsInUpperStrata,
    ];

    pub fn is_phonological(self) -> bool {
        use Feature::*;
        matches!(
            self,
            RewriteSubstitute
                | RewriteDelete
                | RewriteInsert
                | ModeSimultaneous
                | ModeIterative
                | DirLeftToRight
                | DirRightToLeft
                | FocusMultiToken
                | FocusQuantified
                | FocusAnchored
                | AlphaVariable
                | AlphaAcrossGap
                | ClassByFeature
                | ClassBySegments
                | BoundaryInRuleContext
                | AdjacentMetathesis
        )
    }

    /// Features drawn from the mode and direction axes of rewrite rules.
    pub fn is_rewrite_axis(self) -> bool {
        use Feature::*;
        matches!(
            self,
            RewriteSubstitute
                | RewriteDelete
                | RewriteInsert
                | ModeSimultaneous
                | ModeIterative
                | DirLeftToRight
                | DirRightToLeft
        )
    }
}

pub type FeatureSet = BTreeSet<Feature>;

/// A construct the cascade backend refuses by name; plain HC loads and parses each of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Refusal {
    BlockableFamily,
    SuppliedRoots,
    TwoCharacterTables,
    StemEditingCompounding,
    MetathesisMiddleNode,
    OverlappingSimultaneousFocus,
    ClassModifyWithoutFeatureLanes,
    UnboundedTruncation,
}

impl Refusal {
    pub const ALL: [Refusal; 8] = [
        Refusal::BlockableFamily,
        Refusal::SuppliedRoots,
        Refusal::TwoCharacterTables,
        Refusal::StemEditingCompounding,
        Refusal::MetathesisMiddleNode,
        Refusal::OverlappingSimultaneousFocus,
        Refusal::ClassModifyWithoutFeatureLanes,
        Refusal::UnboundedTruncation,
    ];

    /// The construct name a backend refusal must carry.
    pub fn named_construct(self) -> &'static str {
        match self {
            Refusal::BlockableFamily => "families with a blockable rule",
            Refusal::SuppliedRoots => "supplied roots",
            Refusal::TwoCharacterTables => "several character tables",
            Refusal::StemEditingCompounding => "stem-editing compounding",
            Refusal::MetathesisMiddleNode => "metathesis with a middle node",
            Refusal::OverlappingSimultaneousFocus => "overlapping simultaneous multi-token focus",
            Refusal::ClassModifyWithoutFeatureLanes => "class Modify without feature lanes",
            Refusal::UnboundedTruncation => "unbounded truncation",
        }
    }
}

/// A kind of character the speller assertions need the alphabet to be able to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CharKind {
    PrecomposedMarkedForm,
    BaseWithoutPrecomposedForm,
    MarkOnlySegment,
    ApostropheDigraph,
    HyphenSegment,
    LowerAndCapitalForms,
    CapitalOnlyForm,
    TwoMarksOnOneBase,
    NineMarksOnOneBase,
    WrittenBoundary,
    RootsDifferingInCase,
    BothCasesThenMark,
    ThreeMarksOfOneClass,
    TwoCharacterFolding,
}

impl CharKind {
    pub const ALL: [CharKind; 14] = [
        CharKind::PrecomposedMarkedForm,
        CharKind::BaseWithoutPrecomposedForm,
        CharKind::MarkOnlySegment,
        CharKind::ApostropheDigraph,
        CharKind::HyphenSegment,
        CharKind::LowerAndCapitalForms,
        CharKind::CapitalOnlyForm,
        CharKind::TwoMarksOnOneBase,
        CharKind::NineMarksOnOneBase,
        CharKind::WrittenBoundary,
        CharKind::RootsDifferingInCase,
        CharKind::BothCasesThenMark,
        CharKind::ThreeMarksOfOneClass,
        CharKind::TwoCharacterFolding,
    ];
}

/// Thresholds that decide whether the generator labels a grammar in-bound or wide.
pub mod width {
    pub const MAX_INTERIOR_RULES_PER_STRATUM: usize = 2;
    pub const MAX_PHONOLOGICAL_RULES: usize = 4;
    pub const MAX_RULE_AUTOMATON_STATES: usize = 6;
    pub const MAX_ALPHABET: usize = 8;
    pub const MAX_WORD_UNITS: usize = 6;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidthClass {
    InBound,
    Wide,
}
