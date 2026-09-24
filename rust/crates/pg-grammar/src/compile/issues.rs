//! Typed output of [`super::compile_project_with`]: the compiled `Grammar` alongside every
//! conversion issue and substrate inference the compile produced, plus the error returned when a
//! fatal issue is found under `Refuse`. `IssueClass`/`SourceRef`/`ConversionIssue` themselves live
//! in `pg_snapshot::conversion`; only the compiler-specific constructors and error live here.

use pg_snapshot::ImportWarningCode;
use pg_snapshot::{ConversionIssue, InventoryDelta, SourceRef, Warning};

use crate::chardef::CharDefKind;
use crate::model::Grammar;

/// `source_inventory_status == Unknown`'s fatal-under-`Refuse` code.
pub(crate) const SOURCE_PROVENANCE_UNKNOWN: &str =
    ImportWarningCode::SourceProvenanceUnknown.wire();

/// `substrate::complete`'s `Strict`-policy code: a recorded usage cannot segment and the project
/// declared no closed-inventory-completion policy to fix it. Non-fatal and per-allomorph -- see
/// `substrate`'s module doc.
pub(crate) const SUBSTRATE_UNSEGMENTABLE_FORM: &str =
    ImportWarningCode::SubstrateUnsegmentableForm.wire();

/// `substrate::complete`'s ambiguous-classification code: a failing character is neither an
/// exemplar, an authored boundary, nor in the versioned safe-boundary table. Non-fatal and
/// per-allomorph -- see `substrate`'s module doc.
pub(crate) const SUBSTRATE_CLASSIFICATION_AMBIGUOUS: &str =
    ImportWarningCode::SubstrateClassificationAmbiguous.wire();

/// `substrate::feature_rule_migration_issues`'s code: an inferred (featureless) segment satisfies
/// a `Feature`-kind natural class purely via HC's unspecified-lane-matches-anything default.
pub(crate) const SUBSTRATE_INFERRED_SEGMENT_WITH_FEATURE_RULE: &str =
    ImportWarningCode::MigrationInferredSegmentWithFeatureRule.wire();

/// A text-use collector's code: the owner selected a construct (a bracket-pattern/reduplication
/// affix form) but that construct is not literal text, so it cannot publish a usage for it.
pub(crate) const UNSUPPORTED_CONSTRUCT: &str = ImportWarningCode::UnsupportedConstruct.wire();

/// `substrate::complete`'s code when a failure position remaps to an already-registered character
/// (a decomposed-diacritic artifact of `segment::remap_error_position`'s own documented heuristic).
/// Non-fatal and per-allomorph -- see `substrate`'s module doc.
pub(crate) const SUBSTRATE_POSITION_UNMAPPED: &str =
    ImportWarningCode::SubstratePositionUnmapped.wire();

/// What the compiler had to infer about the phonological substrate rather than read off a closed
/// declaration, plus what it could not resolve at all. Populated by `substrate::complete` under
/// [`ResolvedSubstratePolicy::CompleteFromUsage`]; always empty under
/// [`ResolvedSubstratePolicy::Strict`], which never infers.
///
/// [`ResolvedSubstratePolicy::CompleteFromUsage`]: super::options::ResolvedSubstratePolicy::CompleteFromUsage
/// [`ResolvedSubstratePolicy::Strict`]: super::options::ResolvedSubstratePolicy::Strict
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubstrateReport {
    pub inferred_segments: Vec<InferredChar>,
    pub inferred_boundaries: Vec<InferredChar>,
    pub unresolved_uses: Vec<SourceRef>,
    pub ambiguous_uses: Vec<SourceRef>,
}

/// One character definition inferred from usage rather than read off an authored declaration.
/// Deliberately carries no feature-value field: the `RawCharDef` an inferred char becomes must
/// have an empty `feature_values`, since a feature system unifies segments by declared features
/// and an inference has none to declare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredChar {
    pub representation: String,
    pub kind: CharDefKind,
    pub evidence: InferenceEvidence,
}

/// What justified inferring one [`InferredChar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceEvidence {
    LdmlExemplar,
    AuthoredBoundary,
    SafeBoundaryTable { version: u16 },
}

/// [`super::compile_project_with`]'s successful result: the compiled `Grammar`, the substrate
/// inference report, and the same measurement [`super::compile_project_measured`] returns (`inventory`,
/// not merged with the import stage's own inventory). `issues` is every conversion issue collected
/// across the import and compile stages -- import-stage issues, every owner's own recorder issue,
/// and substrate issues; `inventory.issues` carries the recorder's own subset again, unmerged with
/// the other two.
#[derive(Debug)]
pub struct CompileOutput {
    pub grammar: Grammar,
    pub issues: Vec<ConversionIssue>,
    pub warnings: Vec<Warning>,
    pub substrate: SubstrateReport,
    pub inventory: InventoryDelta,
}

/// Returned by [`super::compile_project_with`] under `Refuse` when any collected issue is fatal:
/// an imported fatal issue, or `source_inventory_status == Unknown` (via
/// `SOURCE_PROVENANCE_UNKNOWN`).
#[derive(Debug, thiserror::Error)]
#[error("FieldWorks project cannot be converted to HC without semantic loss")]
pub struct ConversionError {
    pub issues: Vec<ConversionIssue>,
    pub substrate: SubstrateReport,
}
