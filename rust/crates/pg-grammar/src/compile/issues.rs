//! Typed output of [`super::compile_project_with`]: the compiled `Grammar` alongside every
//! conversion issue and substrate inference the compile produced, plus the error returned when a
//! fatal issue is found under `Refuse`. `IssueClass`/`SourceRef`/`ConversionIssue` themselves live
//! in `pg_snapshot::conversion`; only the compiler-specific constructors and error live here.

use pg_snapshot::{ConversionIssue, InventoryDelta, SourceRef};

use crate::chardef::CharDefKind;
use crate::model::Grammar;

/// Shared code for a compile-stage warning not yet given its own per-site code/class.
pub(crate) const LEGACY_WARNING: &str = "grammar.legacy-warning";

/// `source_inventory_status == Unknown`'s fatal-under-`Refuse` code.
pub(crate) const SOURCE_PROVENANCE_UNKNOWN: &str = "conversion.source-provenance-unknown";

/// What the compiler had to infer about the phonological substrate rather than read off a closed
/// declaration, plus what it could not resolve at all. Empty in this task: no owner infers
/// anything yet (later tasks populate it under [`ResolvedSubstratePolicy::CompleteFromUsage`]).
///
/// [`ResolvedSubstratePolicy::CompleteFromUsage`]: super::options::ResolvedSubstratePolicy::CompleteFromUsage
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

/// [`super::compile_project_with`]'s successful result: the compiled `Grammar`, every conversion
/// issue collected across the import and compile stages, the substrate inference report, and the
/// same measurement [`super::compile_project_measured`] returns -- not merged with the import
/// stage's own inventory.
#[derive(Debug)]
pub struct CompileOutput {
    pub grammar: Grammar,
    pub issues: Vec<ConversionIssue>,
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
