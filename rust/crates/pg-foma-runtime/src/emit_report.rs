use serde::{Deserialize, Serialize};

/// Why a closure walk did not reach an exhausted worklist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClosureStopReason {
    WorkBudgetReached,
    DepthBudgetReached,
    ResourceBudgetReached,
    UnboundedTransition,
    UnsupportedTransition,
    InternalConstructionFault,
}

/// The total terminal state of a closure characterization or production trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClosureTerminal {
    Complete,
    Incomplete(ClosureStopReason),
    Refused(ClosureStopReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosureEvidence {
    pub rule_pairs_visited: usize,
    pub synthesized_successors: usize,
    pub maximum_depth: usize,
    pub per_depth_counts: Vec<usize>,
    pub pending_successor_count: usize,
    pub pending_rule_ordinals: Vec<u32>,
    pub worklist_empty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterizationResult {
    pub terminal: ClosureTerminal,
    pub evidence: ClosureEvidence,
}

/// One construct this emitter could not represent as literal lexc — never silently dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncoveredItem {
    /// A short machine-stable category: "pattern-allomorph", "process-morph", "unsegmentable-root",
    /// "unsegmentable-affix", "infix", "reduplication", "circumfix-prefix", "circumfix-suffix",
    /// "process".
    pub kind: String,
    /// The rule/entry this was found on (e.g. `"mrule37#allo0"`, `"entry482(morpheme=...)#allo0"`).
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmitCounts {
    /// `Grammar::entries.len()` (grammar-wide lexical entry count).
    pub entries: usize,
    /// `Grammar::mrules.len()` (grammar-wide morphological rule count).
    pub rules: usize,
    /// Total `<AffixTemplate><Slot>` count across the grammar (structural, not entry-count).
    pub slots: usize,
    /// Category groups this emitter collapsed the grammar's templates into (superset item 1 in
    /// the module doc). Does not count the template-less section.
    pub groups: usize,
    /// Root/affix allomorph OCCURRENCES that produced at least one lexc entry line (an allomorph
    /// emitted in several slots/levels counts each time — this is an emission-volume number, not
    /// a distinct-allomorph census).
    pub allomorphs_emitted: usize,
    /// Allomorph occurrences routed to `uncovered` instead (pattern shapes, process morphs, zone
    /// mismatches, unsegmentable text) — pre-dedup, same counting convention.
    pub allomorphs_skipped: usize,
    /// Total lexc entry lines written — the number that most directly predicts foma compile cost.
    pub lexc_lines: usize,
    /// `crate::preexpand`: (root allomorph, candidate rule) pairs actually attempted for the
    /// rule-application/fusion composite mechanisms, after the cheap required-FS pre-filter — the
    /// module's own scale-bridge number (`crate::preexpand`'s module doc).
    pub composite_pairs_probed: usize,
    /// Composite lexc entries emitted for `Role::Infix` rules (interdigitation — e.g. Amharic's
    /// `-pfv-`/`-conv-`).
    pub composite_interdigitation_entries: usize,
    /// Composite lexc entries emitted for `Role::Prefix`/`Role::Suffix` rules whose fused surface
    /// differs from what the ordinary two-entry emission already reaches (Ge'ez boundary fusion).
    pub composite_fusion_entries: usize,
    /// Composite lexc entries emitted by `build_structural_composites`
    /// (`edge-cases/truncate-morphotactic`/`languages/suffixing-vowel-harmony`): rules `crate::preexpand`
    /// cannot represent at all — `Role::None`/multi-part-LHS truncation, or (when
    /// `probe_would_refuse`) an ordinary `Prefix`/`Suffix`/`Infix` rule in a grammar whose own
    /// phonological cascade defeats `crate::preexpand`'s probe-based fusion mechanism entirely.
    pub composite_structural_entries: usize,
    /// Bare-root (`"#"`-continuation) lexc entry lines OMITTED because `RootRec::never_valid_bare`
    /// proved them dead weight. Counts entry
    /// LINES (one per surface variant), not distinct roots, matching `lexc_lines`'/`allomorphs_
    /// emitted`'s own convention; zero for any grammar with no bound single-allomorph root entries.
    pub bare_root_arcs_pruned: usize,
    /// `(root allomorph, ordinary edge rule)` pairs omitted from exact structural closure after
    /// feature-state reachability proved no non-edge structural anchor reachable from that root.
    pub structural_candidate_pairs_pruned: usize,
}

/// Overall verdict for this grammar's foma path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FomaTier {
    /// Every construct the grammar uses was representable (no `uncovered` entries).
    Full,
    /// Emitted lexc material with known uncovered constructs. This is development evidence, not a
    /// trusted proposer: confirmation cannot restore candidates that the emission omitted.
    Partial { uncovered: usize },
    /// Could not emit a usable network at all; caller should fall back to the full engine for this grammar.
    Unsupported { reason: String },
}

/// Structured detail for a bounded emitter refusal. The field is retained for compatibility with
/// the compound-chain-depth refusal, whose measured value and configured limit are useful to
/// callers and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumBudgetExceeded {
    /// Human-readable label for the bounded condition.
    pub measure: &'static str,
    /// The measured value when the refusal was produced.
    pub value: usize,
    /// The configured threshold.
    pub limit: usize,
}

/// Machine-readable cause for refusing an incomplete eager-composite construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosureRefusalCode {
    /// At least one participating rule has no authored finite application bound.
    UnboundedRuleApplication,
    /// A legal successor remained after the configured closure-depth limit.
    DepthBudgetExceeded,
}

/// Backend that can consume the grammar without relying on this eager FST closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosureFallbackBackend {
    FullMorphologicalParser,
}

/// Structured evidence retained alongside the human-readable unsupported reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureRefusal {
    pub code: ClosureRefusalCode,
    pub affected_rule_ordinals: Vec<u32>,
    pub depth_limit: Option<usize>,
    pub pending_successors: Option<usize>,
    pub remedy_backend: ClosureFallbackBackend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitReport {
    pub uncovered: Vec<UncoveredItem>,
    pub counts: EmitCounts,
    pub tier: FomaTier,
    /// Structured detail for a bounded emitter refusal, when one applies. `None` for ordinary
    /// successful or unrelated failure reports.
    pub enum_budget_exceeded: Option<EnumBudgetExceeded>,
    /// Typed closure-refusal evidence; `None` for successful emission and unrelated failures.
    pub closure_refusal: Option<ClosureRefusal>,
    /// Exact transition evidence retained when the test-support emitter is traced.
    pub closure_evidence: Option<crate::emit_report::CharacterizationResult>,
}

pub struct EmitResult {
    pub lexc_source: String,
    pub report: EmitReport,
}

// --- Affix role classification (ported from hc-hybrid/src/token.rs, no dependency) --------------
