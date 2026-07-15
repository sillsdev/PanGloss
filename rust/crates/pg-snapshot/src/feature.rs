//! Feature systems and feature structures.
//!
//! FieldWorks has *two* independent feature systems — `LangProject.MsFeatureSystemOA`
//! (morphosyntactic: person/number/gender/tense/...) and `LangProject.PhFeatureSystemOA`
//! (phonological: voice/place/manner/...) — loaded identically by
//! `HCLoader.LoadFeatureSystem` (HCLoader.cs:2650-2667). Both are represented by the same
//! [`FeatureSystem`] shape here; [`crate::Snapshot`] carries one of each
//! (`featureSystems.phonological` / `featureSystems.morphosyntactic`).
//!
//! A feature is either *closed* (`FsClosedFeature`: a fixed enumeration of value symbols, e.g.
//! `number: {sg, pl}`) or *complex* (`FsComplexFeature`: its value is itself a nested feature
//! structure, e.g. `agreement: [person, number]`). [`FeatureStructure`] is the recursive value
//! type used everywhere a feature structure is attached to something else (phonemes, MSAs, stem
//! name regions, inflection types, natural classes, ...).

use serde::{Deserialize, Serialize};

use crate::common::Guid;

/// The `featureSystems` snapshot section: the two independent feature systems FieldWorks keeps
/// (phonological and morphosyntactic — see [`FeatureSystem`]'s doc for why there are two).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureSystems {
    /// ← `LangProject.PhFeatureSystemOA`.
    pub phonological: FeatureSystem,
    /// ← `LangProject.MsFeatureSystemOA`.
    pub morphosyntactic: FeatureSystem,
}

/// One of the two independent feature systems a project declares (`featureSystems.phonological`
/// / `featureSystems.morphosyntactic`). ← `FsFeatureSystem` (`LangProject.PhFeatureSystemOA` /
/// `MsFeatureSystemOA`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureSystem {
    /// ← `FsFeatureSystem.FeaturesOC` filtered to `IFsClosedFeature` (HCLoader.cs:2654-2660).
    pub closed_features: Vec<ClosedFeature>,
    /// ← `FsFeatureSystem.FeaturesOC` filtered to the complex (non-closed) case
    /// (HCLoader.cs:2661-2664).
    pub complex_features: Vec<ComplexFeature>,
}

/// A feature with a fixed, enumerated set of possible values (e.g. `number`, `gender`).
/// ← `FsClosedFeature`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedFeature {
    /// ← `FsClosedFeature.Guid`.
    pub guid: Guid,
    /// ← `FsFeatDefn.Name` (best analysis alternative).
    pub name: String,
    /// ← `FsFeatDefn.Abbreviation` (best analysis alternative) — HCLoader uses this as the
    /// HC `Description` (HCLoader.cs:2659).
    pub abbreviation: String,
    /// ← `FsClosedFeature.ValuesOC`, in declaration order (HCLoader.cs:2658).
    pub values: Vec<FeatureValueSymbol>,
}

/// One admissible value of a [`ClosedFeature`] (e.g. `number`'s `sg`/`pl`). ← `FsSymFeatVal`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureValueSymbol {
    /// ← `FsSymFeatVal.Guid`.
    pub guid: Guid,
    /// ← `FsSymFeatVal.Name` (best analysis alternative). Not read by `HCLoader` (which only
    /// consults `Abbreviation`, HCLoader.cs:2658) but carried for completeness/legibility.
    pub name: String,
    /// ← `FsSymFeatVal.Abbreviation` (best analysis alternative) — this is the string
    /// `HCLoader` actually uses as the HC `FeatureSymbol` description.
    pub abbreviation: String,
}

/// A feature whose value is itself a nested feature structure (e.g. `agreement`).
/// ← `FsComplexFeature`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplexFeature {
    /// ← `FsComplexFeature.Guid`.
    pub guid: Guid,
    /// ← `FsFeatDefn.Name` (best analysis alternative).
    pub name: String,
    /// ← `FsFeatDefn.Abbreviation` (best analysis alternative).
    pub abbreviation: String,
    /// ← `FsComplexFeature.TypeRA` (the `FsFeatStrucType` constraining which features may
    /// appear inside this feature's value). `HCLoader` never consults this constraint (it loads
    /// whatever `FeatureSpecsOC` a feature structure actually contains, HCLoader.cs:2500-2530),
    /// so it is carried here only for schema completeness / a future stricter compiler; T3 may
    /// ignore it. Not resolved further (no `FsFeatStrucType` section in this format).
    pub feature_type: Option<Guid>,
}

/// A feature structure: an ordered list of (feature → value) pairs. Recursive because a
/// [`ComplexFeature`]'s value is itself a `FeatureStructure`. ← `FsFeatStruc`
/// (`FeatureSpecsOC`, `HCLoader.LoadFeatureStruct`, HCLoader.cs:2500-2530).
///
/// Order is the LCM `FeatureSpecsOC` declaration order; it carries no semantic weight (feature
/// structures are unordered mathematically) but is preserved for deterministic output.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureStructure {
    pub values: Vec<FeatureValue>,
}

/// One (feature → value) pair inside a [`FeatureStructure`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureValue {
    /// The feature this value is for — a [`ClosedFeature`] or [`ComplexFeature`] guid, resolved
    /// against whichever [`FeatureSystem`] this feature structure lives under (phonological or
    /// morphosyntactic; the two are never mixed within one `FeatureStructure`).
    /// ← `IFsFeatureSpecification.FeatureRA`.
    pub feature: Guid,
    pub value: FeatureValueKind,
}

/// The value half of a [`FeatureValue`]: either a value symbol of a closed feature, or a nested
/// feature structure (when the feature is complex). ← `IFsClosedValue.ValueRA` /
/// `IFsComplexValue.ValueOA` (HCLoader.cs:2507-2526).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum FeatureValueKind {
    /// ← `IFsClosedValue.ValueRA` — the guid of a [`FeatureValueSymbol`] belonging to the
    /// referenced feature.
    Closed { value: Guid },
    /// ← `IFsComplexValue.ValueOA` — a nested feature structure.
    Complex { value: FeatureStructure },
}
