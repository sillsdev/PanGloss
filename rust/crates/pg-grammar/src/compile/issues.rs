//! Typed output of [`super::compile_project_with`]: the compiled `Grammar` alongside every
//! conversion issue and substrate inference the compile produced, plus the error returned when a
//! fatal issue is found under `Refuse`. `IssueClass`/`SourceRef`/`ConversionIssue` themselves live
//! in `pg_snapshot::conversion`; only the compiler-specific constructors and error live here.

use pg_snapshot::ImportWarningCode;
use pg_snapshot::{ConversionIssue, InventoryDelta, LoadDecision, SourceRef, Warning};

use super::EnvironmentResolution;
use crate::chardef::CharDefKind;
use crate::model::Grammar;

/// `source_inventory_status == Unknown`'s fatal-under-`Refuse` code.
pub(crate) const SOURCE_PROVENANCE_UNKNOWN: ImportWarningCode =
    ImportWarningCode::SourceProvenanceUnknown;

/// A text-use collector's code: the owner selected a construct (a bracket-pattern/reduplication
/// affix form) but that construct is not literal text, so it cannot publish a usage for it.
pub(crate) const UNSUPPORTED_CONSTRUCT: ImportWarningCode = ImportWarningCode::UnsupportedConstruct;

/// Provisional definitions supplied for selected usage, and usage that cannot be classified.
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
    GraphemeCluster,
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
    pub load_decisions: Vec<LoadDecision>,
    pub environment_resolutions: Vec<EnvironmentResolution>,
    /// Every compiled object's output identity, in compile order, after grammar compaction.
    pub compiled_outputs: Vec<super::lineage::CompiledOutput>,
    /// The output id of each allomorph, indexed by `AllomorphId`; parallel to `grammar.allomorph_owners`.
    pub allomorph_output_ids: Vec<Option<u32>>,
    /// Final source-to-output associations published after grammar compaction.
    pub compiled_mappings: Vec<CompiledMapping>,
    /// Final sibling allomorph order within each source MSA and stratum bucket.
    pub compiled_allomorph_order: Vec<CompiledAllomorphOrder>,
    /// The effect the compiler gave each authored allomorph gate; a gate with no entry was never read.
    pub allomorph_gates: Vec<super::AllomorphGateOutcome>,
}

/// A source identity attached to one compiled output, with the role the source plays for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledMapping {
    pub source_kind: String,
    pub source_guid: Option<String>,
    pub source_key: String,
    /// The [`super::CompiledOutput`] this source maps to.
    pub output_id: u32,
    pub relation_role: String,
    /// Position within the source, such as a circumfix half's surface order; 0 otherwise.
    pub source_ordinal: u32,
    pub identity_quality: String,
}

/// One compiler-ordered allomorph within its owner. Unrepresented rows carry no owner or output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledAllomorphOrder {
    pub owner_output_id: Option<u32>,
    pub source_entry_guid: Option<String>,
    pub source_msa_guid: Option<String>,
    pub source_allomorph_guid: Option<String>,
    pub output_id: Option<u32>,
    pub compiled_order: Option<u32>,
}

/// Returned by [`super::compile_project_with`] under `Refuse` when any collected issue is fatal:
/// an imported fatal issue, or `source_inventory_status == Unknown` (via
/// `SOURCE_PROVENANCE_UNKNOWN`).
#[derive(Debug, thiserror::Error)]
#[error("FieldWorks project cannot be converted to HC without semantic loss")]
pub struct ConversionError {
    pub issues: Vec<ConversionIssue>,
    /// Named diagnostics already projected by the compiler, including imported findings.
    pub warnings: Vec<Warning>,
    pub substrate: SubstrateReport,
    /// Compiler inventory retained when `Refuse` prevents a grammar from being returned.
    pub inventory: InventoryDelta,
    /// Owner-published load outcomes retained for refusal artifacts.
    pub load_decisions: Vec<LoadDecision>,
    pub environment_resolutions: Vec<EnvironmentResolution>,
    /// Output identities published before the refusal decision.
    pub compiled_outputs: Vec<super::lineage::CompiledOutput>,
    /// Final source-to-output associations published before the refusal decision.
    pub compiled_mappings: Vec<CompiledMapping>,
    /// Final sibling allomorph order published before the refusal decision.
    pub compiled_allomorph_order: Vec<CompiledAllomorphOrder>,
    /// Allomorph gate effects recorded before the refusal decision.
    pub allomorph_gates: Vec<super::AllomorphGateOutcome>,
}
