//! The one-time, audited coverage LEDGER over the frozen `pg-grammar/src/model.rs` construct set.
//! It is evidence that feeds *into* the capability gate
//! (`crate::capability::compose_envelope`) — it is **not itself a gate**. See "Evidence, not a
//! gate" below for what that means concretely in this file.
//!
//! HermitCrab and the Rust model are assumed complete apart from bug fixes, so this module
//! implements a one-time REVIEWED ledger over the current model rather than source-AST/reflection
//! infrastructure that would try to stay in sync automatically. The pre-existing coverage ledger
//! supplies evidence into the capability gate; it is not itself the gate.
//!
//! # What this module owns, and what it reuses rather than re-deriving
//! - The INVENTORY of every frozen `model.rs` variant already exists as
//!   `crate::capability::CharacteristicKind` (its own `ALL` constant) and
//!   `crate::capability::characterize` (the exhaustive, no-catch-all per-`model.rs`-variant
//!   walk) — this module does not re-inventory `model.rs`. What it adds is the SCHEMA half: a
//!   consolidated, queryable, *serializable* row-per-construct VIEW over that existing inventory,
//!   via `LedgerRow`/`CoverageLedger`/`build_ledger`.
//! - The disposition mapping is not duplicated here either — `LedgerRow::disposition` always
//!   reads `crate::capability::CharacteristicKind::default_disposition`, never a second,
//!   hardcoded copy (pinned by this file's own
//!   `ledger_disposition_never_diverges_from_default_disposition` test).
//! - `containment_evidence_for` is a curated, hand-reviewed table naming, for every construct,
//!   which REAL already-merged test file (`tests/cover_*.rs`, `tests/phase_c_*.rs`, `tests/
//!   epenthesis_structural_route_containment.rs`, `tests/two_table_symbol_divergence.rs`, `tests/
//!   f6_reduplication_peel_chain_depth.rs`, `tests/p6_gate_parity.rs`) is a witness for it — a
//!   one-time REVIEWED table, not a mechanically-derived one.
//! - `CoverageLedger::to_json` is the machine-readable source artifact (mirrors `crate::health`'s
//!   own "canonical JSON is the source artifact" convention); no Markdown/prose renderer exists
//!   here.
//! - Every ledger row's disposition and evidence owner is validated exhaustively by this module's
//!   own `tests` submodule (`every_characteristic_kind_appears_exactly_once`,
//!   `every_config_predicate_row_names_a_discharging_predicate`, and this file's
//!   own "A future model-shape change" note below), hand-maintained because Rust has no enum
//!   reflection (the same reason `crate::capability::CharacteristicKind::ALL`'s own doc gives).
//!
//! # Coverage is claimed PER STRATEGY, never for "the compiler"
//! This crate has three compilers (`crate::enumerate::EmissionStrategy`), and until
//! `crate::strategy_coverage` existed this ledger's rows silently spoke for all of them at once.
//! The `Compounding` row cited `tests/cover_compounding.rs`, which exercises `FomaAnalyzer::new`
//! and therefore `crate::enumerate::EmissionStrategy::TunedSurfaceProbed` only -- while
//! `crate::uflexc`, the sole lexicon emitter
//! `crate::enumerate::EmissionStrategy::PlanComposed` has, could not propose a compound at all.
//! One compiler's coverage was read as three compilers' coverage. That is this repo's own
//! coverage-gate inheritance trap recurring on a per-strategy axis rather than a per-construct one.
//!
//! Three things follow, all enforced in this file's own `tests` submodule:
//! - Every `ContainmentEvidence` NAMES the strategies its citation was demonstrated on
//!   (`ContainmentEvidence::strategies`); `ev` panics on an unattributed one.
//! - A citation may not name a strategy `crate::strategy_coverage` says cannot represent the
//!   construct (`no_citation_claims_a_strategy_that_cannot_represent_the_construct`).
//! - Each row reports the strategies that CAN represent the construct but have no witness
//!   (`LedgerRow::strategies_unwitnessed`) and the ones that cannot represent it at all
//!   (`LedgerRow::strategies_cannot_represent`). Both are DERIVED from the strategy table, so a
//!   fourth compiler cannot inherit the incumbents' evidence by being added quietly.
//!
//! Reported, not gated -- consistent with this module's "Evidence, not a gate" section below. A
//! non-empty `strategies_unwitnessed` is today's honest reading of the test suite, not a failure;
//! notably no row names `crate::enumerate::EmissionStrategy::TemplatedUnderlyingTokens` at all.
//!
//! # A future model-shape change
//! Adding a new `pg-grammar/src/model.rs` construct or behavior-bearing field is OUTSIDE this
//! ledger's standing frozen-model assumption and must explicitly reopen and revise this coverage
//! contract before merge — concretely, that means updating `crate::capability`
//! first (`CharacteristicKind`, `CharacteristicKind::ALL`, `default_disposition`, `characterize`,
//! per that module's own exhaustiveness discipline), which breaks THIS module's build the moment a
//! new `CharacteristicKind::ALL` entry appears (every exhaustive match in this file has no
//! catch-all arm), forcing a reviewed update to `containment_evidence_for` too.
//!
//! # Evidence, not a gate
//! Nothing in this module is consulted by any compile path. `build_ledger` is a pure function;
//! `CoverageLedger` is inert data. The load-bearing, hard-failing artifact remains
//! `crate::capability::compose_envelope` — this ledger's rows are read BY a human/CI
//! reviewer and by that gate's own predicate authors as evidence when they write or review a
//! `crate::capability::CapabilityPredicate`, never consulted at compile time to admit or refuse a
//! grammar. No test in this file asserts `gaps.is_empty()` for conformance/containment coverage —
//! same non-blocking-first discipline `crate::conformance_coverage`/`crate::
//! plan_interaction_coverage` already established for their own advisory reports.
//!
//! # The four rows this ledger fills in per `CharacteristicKind`
//! `LedgerRow`: the `crate::capability::CharacteristicKind` itself; its
//! `crate::capability::Disposition` (ALWAYS [`crate::capability::CharacteristicKind::
//! default_disposition`] — never a second, divergent copy); every [`crate::capability::
//! CapabilityPredicate`] in the caller-supplied registry that discharges it, alongside that
//! predicate's own `crate::capability::EvidenceProvenance`; the mapped `machine/conformance/
//! constructs.txt` construct id(s) (reused verbatim from [`crate::conformance_coverage::
//! construct_ids_for`], never re-derived) plus the resulting [`crate::conformance_coverage::
//! CoverageStatus`] against a caller-supplied passing-construct set; and the curated
//! `ContainmentEvidence` naming which (if any) already-merged test is this construct's
//! proposer-to-confirm containment witness.
//!
//! # Canonical JSON
//! `CoverageLedger::to_json`/`CoverageLedger::from_json` follow `crate::health`'s own
//! established convention exactly: pretty-printed, two-space indent, fields in Rust declaration
//! order (serde's unmodified default), a `schema_version` constant
//! (`COVERAGE_LEDGER_SCHEMA_VERSION`) bumped only on a wire-incompatible change. CLI/AI/FieldWorks
//! tooling consumes this one artifact rather than re-deriving the same facts from `capability.rs`'s
//! Rust types directly.
//!
//! # Why `CharacteristicKind`/`Disposition`/`EvidenceProvenance`/`CoverageStatus` gain `Serialize`/
//! `Deserialize` impls IN THIS FILE, not in `capability.rs`/`conformance_coverage.rs`
//! This module's own hard rule: additive only, never changing `capability.rs` semantics. Rust's
//! orphan rule permits implementing a foreign trait (`serde::Serialize`/`Deserialize`) for a type
//! local to this crate from ANY module in the crate — so these four wire-format impls live here,
//! next to the one module that actually needs them, leaving `capability.rs`/`conformance_coverage.
//! rs` byte-for-byte untouched by this change. Each is an exhaustive, no-catch-all match over a
//! stable wire name (mirroring `crate::health::FindingCode`'s own hand-written `code()`/
//! `from_code()` pattern) rather than a derive, because none of those four types are defined in
//! this file.

use std::collections::HashSet;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::capability::{CharacteristicKind, Disposition, EvidenceProvenance, PredicateRegistry};
use crate::conformance_coverage::{construct_ids_for, CoverageStatus};
use crate::enumerate::EmissionStrategy;
use crate::strategy_coverage::{representation_of, strategies_that_represent};

/// This schema's own version (mirrors `crate::health::HEALTH_SCHEMA_VERSION`'s convention).
pub const COVERAGE_LEDGER_SCHEMA_VERSION: u32 = 1;

// Wire-name impls for foreign-module types (see module top-doc's last section for why these live here).

fn kind_wire_name(kind: CharacteristicKind) -> &'static str {
    use CharacteristicKind::*;
    match kind {
        Affixation => "affixation",
        RealizationalMorphology => "realizational_morphology",
        Compounding => "compounding",
        OrderedMorphRuleApplication => "ordered_morph_rule_application",
        UnorderedMorphRuleApplication => "unordered_morph_rule_application",
        MprGroupAppend => "mpr_group_append",
        MprGroupOverwrite => "mpr_group_overwrite",
        IterativeRewrite => "iterative_rewrite",
        SimultaneousRewrite => "simultaneous_rewrite",
        LeftToRightRewrite => "left_to_right_rewrite",
        RightToLeftRewrite => "right_to_left_rewrite",
        Metathesis => "metathesis",
        Epenthesis => "epenthesis",
        SubruleGating => "subrule_gating",
        CircumfixOutputAction => "circumfix_output_action",
        Reduplication => "reduplication",
        CoOccurrenceConstraint => "co_occurrence_constraint",
        NaturalClassDefinition => "natural_class_definition",
        MultiTable => "multi_table",
        QuantifierPattern => "quantifier_pattern",
        StemName => "stem_name",
        FreeFluctuation => "free_fluctuation",
        ProcessMorphology => "process_morphology",
    }
}

fn kind_from_wire_name(s: &str) -> Option<CharacteristicKind> {
    CharacteristicKind::ALL
        .iter()
        .copied()
        .find(|k| kind_wire_name(*k) == s)
}

impl Serialize for CharacteristicKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(kind_wire_name(*self))
    }
}

impl<'de> Deserialize<'de> for CharacteristicKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        kind_from_wire_name(&s)
            .ok_or_else(|| D::Error::custom(format!("unknown CharacteristicKind wire name: {s}")))
    }
}

fn disposition_wire_name(disposition: Disposition) -> &'static str {
    match disposition {
        Disposition::Proven => "proven",
        Disposition::ConfigPredicate => "config_predicate",
        Disposition::ConfirmOnly => "confirm_only",
    }
}

fn disposition_from_wire_name(s: &str) -> Option<Disposition> {
    match s {
        "proven" => Some(Disposition::Proven),
        "config_predicate" => Some(Disposition::ConfigPredicate),
        "confirm_only" => Some(Disposition::ConfirmOnly),
        _ => None,
    }
}

impl Serialize for Disposition {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(disposition_wire_name(*self))
    }
}

impl<'de> Deserialize<'de> for Disposition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        disposition_from_wire_name(&s)
            .ok_or_else(|| D::Error::custom(format!("unknown Disposition wire name: {s}")))
    }
}

fn provenance_wire_name(provenance: EvidenceProvenance) -> &'static str {
    match provenance {
        EvidenceProvenance::Structural => "structural",
    }
}

fn provenance_from_wire_name(s: &str) -> Option<EvidenceProvenance> {
    match s {
        "structural" => Some(EvidenceProvenance::Structural),
        _ => None,
    }
}

impl Serialize for EvidenceProvenance {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(provenance_wire_name(*self))
    }
}

impl<'de> Deserialize<'de> for EvidenceProvenance {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        provenance_from_wire_name(&s)
            .ok_or_else(|| D::Error::custom(format!("unknown EvidenceProvenance wire name: {s}")))
    }
}

fn coverage_status_wire_name(status: CoverageStatus) -> &'static str {
    match status {
        CoverageStatus::Covered => "covered",
        CoverageStatus::Uncovered => "uncovered",
        CoverageStatus::Unmappable => "unmappable",
    }
}

fn coverage_status_from_wire_name(s: &str) -> Option<CoverageStatus> {
    match s {
        "covered" => Some(CoverageStatus::Covered),
        "uncovered" => Some(CoverageStatus::Uncovered),
        "unmappable" => Some(CoverageStatus::Unmappable),
        _ => None,
    }
}

impl Serialize for CoverageStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(coverage_status_wire_name(*self))
    }
}

impl<'de> Deserialize<'de> for CoverageStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        coverage_status_from_wire_name(&s)
            .ok_or_else(|| D::Error::custom(format!("unknown CoverageStatus wire name: {s}")))
    }
}

// The curated containment-evidence table ("owning tests" per construct).

/// Which shape of evidence `ContainmentEvidence::citation` provides. Not every
/// `crate::capability::CharacteristicKind` needs (or can meaningfully have) the same shape of
/// witness — see each variant's own doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentEvidenceKind {
    /// A test whose specific, stated purpose is proving this construct's proposer-to-confirm containment.
    Dedicated,
    /// No dedicated fixture exists or is needed: `Disposition::Proven` and exercised pervasively by this crate's general full-grammar propose-confirm gates.
    GeneralPervasive,
}

/// One curated, hand-reviewed containment-evidence citation. Every field is a
/// `String` (not `&'static str`) so `LedgerRow` round-trips through `CoverageLedger::from_json`
/// losslessly, matching `crate::health::HealthFinding`'s own `String`-field convention.
///
/// # Evidence NAMES ITS STRATEGIES
/// Every citation must say which compiler(s) it was demonstrated on. Before that requirement, this
/// table's `Compounding` row cited `tests/cover_compounding.rs`, which exercises `FomaAnalyzer::new`
/// -- i.e. `crate::enumerate::EmissionStrategy::TunedSurfaceProbed` -- and nothing else, while the
/// row read as evidence that the construct was covered, full stop. It was then silently inherited by
/// `crate::enumerate::EmissionStrategy::PlanComposed`, whose emitter (`crate::uflexc`) could not
/// propose a compound at all. That is the coverage-gate inheritance trap on a per-STRATEGY axis, and
/// `ContainmentEvidence::strategies` is what makes it impossible to repeat silently: the ledger
/// now reports, per row, both the strategies a witness exists for and the ones that can represent
/// the construct but have NO witness (`LedgerRow::strategies_unwitnessed`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainmentEvidence {
    pub kind: ContainmentEvidenceKind,
    /// `tests/<file>.rs` plus the specific `#[test]` function name(s), e.g.
    /// `"tests/cover_compounding.rs::head_a_word_over_propose_confirm_prune"`.
    pub citation: String,
    /// Which `crate::enumerate::EmissionStrategy`s the cited test(s) actually exercise, as
    /// `crate::enumerate::EmissionStrategy::label` strings. NEVER empty (see `ev`) -- an
    /// unattributed citation is exactly the shape that let one compiler's coverage stand in for
    /// three.
    pub strategies: Vec<String>,
    /// A one-line note on what the cited test actually proves for this construct.
    pub note: String,
}

/// The strategies a citation demonstrates as `ContainmentEvidence::strategies` wire strings, so a citation cannot name a compiler that does not exist.
fn strategies_of(strategies: &[EmissionStrategy]) -> Vec<String> {
    strategies.iter().map(|s| s.label().to_string()).collect()
}

/// Panics if `strategies` is empty -- defaulting to "all strategies" would silently recreate the unattributed-coverage bug this table exists to close.
fn ev(
    kind: ContainmentEvidenceKind,
    citation: &str,
    strategies: &[EmissionStrategy],
    note: &str,
) -> ContainmentEvidence {
    assert!(
        !strategies.is_empty(),
        "containment evidence must name the strategies it was demonstrated on: {citation}"
    );
    ContainmentEvidence {
        kind,
        citation: citation.to_string(),
        strategies: strategies_of(strategies),
        note: note.to_string(),
    }
}

/// `kind`'s curated proposer-to-confirm
/// containment witness, if this crate's test suite
/// has one — `None` only where no witness exists at all (a genuine, honestly-reported gap, never
/// silently invented; see `CharacteristicKind::NaturalClassDefinition`'s own arm below).
/// Exhaustively matched (no catch-all) — same discipline `crate::capability::characterize`/
/// `crate::conformance_coverage::construct_ids_for` already hold themselves to: adding a
/// `CharacteristicKind` variant breaks this file's build until it is given an explicit arm here.
pub fn containment_evidence_for(kind: CharacteristicKind) -> Option<ContainmentEvidence> {
    use CharacteristicKind::*;
    use ContainmentEvidenceKind::*;
    Some(match kind {
        Affixation => ev(
            GeneralPervasive,
            "tests/f1_large_lexicon_gate.rs, tests/f2_junction_gate.rs, tests/f4_composite_gate.rs",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Ordinary AffixProcessRule prefixation/suffixation/infixation is the baseline every \
             full-grammar propose-confirm gate exercises continuously; Proven already licenses \
             unconditional admission-filtering, so no separate dedicated fixture is required.",
        ),
        RealizationalMorphology => ev(
            Dedicated,
            "tests/cover_realizational_morphology_constraints.rs::\
             realizational_rule_presence_blocking_over_propose_confirm_prune",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Proposer-to-confirm containment for MorphRuleDef::Realizational's real_fs \
             head-wrapped presence-blocking.",
        ),
        Compounding => ev(
            Dedicated,
            "tests/cover_compounding.rs::head_a_word_over_propose_confirm_prune (+ \
             subrule_group_gate_excludes_partial_match_like_confirm, \
             head_c_excluded_by_rule_level_gate_like_confirm)",
            &[EmissionStrategy::TunedSurfaceProbed],
            "License-gated head/non-head cross-product containment for the non-recursive case, \
             plus the (un)group-awareness witness design.md D4 names.",
        ),
        OrderedMorphRuleApplication => ev(
            GeneralPervasive,
            "tests/phase_c_strata_depth.rs (multi-stratum cascade recall-parity), tests/\
             f1_large_lexicon_gate.rs, tests/f4_composite_gate.rs",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Linear rule-application order is the default cascade shape exercised by every \
             general gate; Proven, no dedicated fixture required.",
        ),
        UnorderedMorphRuleApplication => ev(
            Dedicated,
            "tests/cover_unordered_morph_rules.rs::non_document_order_analysis_is_proposed_and_\
             confirmed (+ unbounded_unordered_stratum_deterministically_refuses_to_compile for \
             the Refuse split)",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Chain-depth-bounded any-order proposal containment, plus the deterministic \
             over-budget refusal witness.",
        ),
        MprGroupAppend => ev(
            Dedicated,
            "tests/cover_mpr_groups.rs::out_mpr_accumulation_then_gate_over_propose_confirm_prune \
             (+ append_output_is_order_invariant_overwrite_output_is_not)",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Non-tracking-baseline containment for MprGroupOutput::Append, plus the \
             order-invariance witness design.md D4 names.",
        ),
        MprGroupOverwrite => ev(
            Dedicated,
            "tests/cover_mpr_groups.rs::overwrite_group_composes_to_confirm_only",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Non-narrowing-baseline containment for MprGroupOutput::Overwrite: this witness proves \
             compose_envelope resolves an observed Overwrite group to ConfirmOnly, never Admit.",
        ),
        IterativeRewrite => ev(
            GeneralPervasive,
            "tests/f1_large_lexicon_gate.rs, tests/f2_junction_gate.rs, tests/phase_c_right_to_left.rs \
             (iterative baseline contrast)",
            &[EmissionStrategy::PlanComposed, EmissionStrategy::TunedSurfaceProbed],
            "The default RewriteMode every general gate's phonological rules use; Proven, no \
             dedicated fixture required.",
        ),
        SimultaneousRewrite => ev(
            Dedicated,
            "tests/phase_c_simultaneous.rs::sim_nonoverlap_env_now_compiles_and_matches_oracle_\
             exactly (+ sim_overlap_env_stays_honest_unsupported for the Refuse split)",
            &[EmissionStrategy::PlanComposed],
            "Containment for the pairwise-non-overlapping case the simultaneous.subrule-overlap \
             predicate Admits; the genuinely-overlapping case stays honestly unsupported.",
        ),
        LeftToRightRewrite => ev(
            GeneralPervasive,
            "tests/f2_junction_gate.rs, tests/phase_c_right_to_left.rs (LTR is the implicit \
             contrast baseline for every rtl_* case)",
            &[EmissionStrategy::PlanComposed, EmissionStrategy::TunedSurfaceProbed],
            "The default Dir every general gate's phonological rules use; Proven, no dedicated \
             fixture required.",
        ),
        RightToLeftRewrite => ev(
            Dedicated,
            "tests/phase_c_right_to_left.rs::rtl_plain_rule_now_compiles_and_matches_oracle (+ \
             rtl_feature_environment_swap_matches_oracle, rtl_deletion_matches_oracle, \
             rtl_cross_table_segments_environment_matches_oracle)",
            &[EmissionStrategy::PlanComposed],
            "Reversal-plus-safety-net-union containment against the real oracle, including a \
             table-qualified cross-table Segments feature constraint.",
        ),
        Metathesis => ev(
            Dedicated,
            "tests/phase_c_metathesis.rs::metathesis_adjacent_singleton_swap_matches_oracle_\
             exactly (+ metathesis_right_to_left_reversal_matches_oracle_exactly for the \
             Dir::RightToLeft mirror construction, and \
             metathesis_right_to_left_differs_from_compiling_as_left_to_right for the \
             direction-blindness guard)",
            &[EmissionStrategy::PlanComposed],
            "Dedicated swap-relation containment against the real oracle in BOTH directions -- \
             Dir::RightToLeft is no longer a scope boundary: it compiles via the same \
             mirror-and-reverse construction \
             compile_rtl_branch_net uses, so the union is a superset the oracle prunes. The \
             remaining refusals are pattern-shape ones (Anchor, and any Slot::Repeat -- \
             slot_candidates enumerates concrete alternatives), never the direction itself.",
        ),
        Epenthesis => ev(
            Dedicated,
            "tests/epenthesis_structural_route_containment.rs::\
             epenthesis_over_propose_confirm_prune_matches_oracle_exactly",
            &[EmissionStrategy::TunedSurfaceProbed],
            "End-to-end propose(over-generate)-then-confirm(prune) containment for an \
             obligatory-epenthesis grammar, matching the oracle's analysis set exactly.",
        ),
        SubruleGating => ev(
            Dedicated,
            "tests/p6_gate_parity.rs::synthetic_pos_gate_matches_oracle (+ \
             ungated_cascade_would_have_missed_the_noun_entry); scale: tests/\
             phase_c_partition_k.rs::partition_k_recall_parity_via_generator_and_oracle",
            &[EmissionStrategy::PlanComposed],
            "Static MPR/POS subrule-gating containment against the real oracle, plus a \
             2^k-group scale gate.",
        ),
        CircumfixOutputAction => ev(
            Dedicated,
            "tests/phase_c_circumfix.rs::circumfix_recall_parity_via_generator_and_oracle (+ \
             ordered_multi_insert_no_first_insert_shortcut_recall_parity, \
             null_role_structural_drop_recall_parity, \
             infix_with_drop_structural_recall_parity, \
             redup_first_allomorph_then_dropping_prefix_allomorph_structural_recall_parity)",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Structural-composite containment for circumfix-shaped (discontinuous/dropped-\
             material) allomorphs against the real oracle, including a genuinely Infix-classified \
             allomorph that drops LHS material (census C4) and a dropping allomorph hidden behind \
             a Role::Reduplication-classified allomorph 0 (census C5).",
        ),
        Reduplication => ev(
            Dedicated,
            "tests/f6_reduplication_peel_chain_depth.rs::\
             kimbiakimbia_reduplication_is_recovered_with_oracle_containment (+ \
             deep_self_similar_chain_is_refused_deterministically for the chain-depth budget); \
             tests/f4_composite_gate.rs case (c)",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Peeler-to-confirm containment for true-reduplication allomorphs, plus the \
             deterministic deep-chain refusal witness.",
        ),
        CoOccurrenceConstraint => ev(
            Dedicated,
            "tests/cover_realizational_morphology_constraints.rs::\
             morpheme_co_occurrence_exclude_anywhere_over_propose_confirm_prune",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Proposer-to-confirm containment for MorphemeCoOccurrenceRule adjacency exclusion.",
        ),
        NaturalClassDefinition => return None,
        MultiTable => ev(
            Dedicated,
            "tests/phase_c_multi_table.rs::\
             multi_table_rewrite_compiles_correctly_against_its_owning_table; stronger claim: \
             tests/two_table_symbol_divergence.rs::\
             stratum_1_devoice_rewrite_proposer_confirm_matches_oracle",
            &[EmissionStrategy::PlanComposed],
            "Faithful per-stratum table threading, proven for one stratum's own rule and, more \
             strongly, for two strata whose tables disagree about the same symbol index.",
        ),
        QuantifierPattern => ev(
            Dedicated,
            "tests/phase_c_quantifier.rs::quantifier_bounded_environment_compiles_and_matches_\
             oracle (+ quantifier_unbounded_environment_compiles_and_matches_oracle for the \
             genuinely-unbounded case)",
            &[EmissionStrategy::PlanComposed],
            "Bounded- AND unbounded-quantifier containment against the real oracle, both at \
             min-boundary occurrence counts; an inverted/over-budget-finite/alpha-nested \
             quantifier stays honestly unsupported.",
        ),
        // `RootAllomorphDef::stem_name`, not `MorphRuleDef::required_stem_name` (folded into Affixation/RealizationalMorphology to avoid double-counting the same ModelLocation::MorphRule occurrence).
        StemName => ev(
            Dedicated,
            "tests/cover_realizational_morphology_constraints.rs::\
             stem_name_gating_over_propose_confirm_prune",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Proposer-to-confirm containment for RootAllomorphDef::stem_name's required- and \
             excluded-match gating (bare-restricted-allomorph rejection, plus the \
             default-allomorph-excluded-by-a-restricted-sibling case) -- the FST proposes every \
             stem-restricted allomorph unconditionally; confirm's stem_name_gate_reason prunes.",
        ),
        // No dedicated FST-propose-then-confirm witness exists for the disjunctive-allomorph re-check; only oracle-level conformance fixtures exercise it, a different evidence axis. Honest gap, not a fabricated citation.
        FreeFluctuation => return None,
        // No test drives an ablaut grammar through propose-then-confirm; None surfaces that as a gap.
        ProcessMorphology => return None,
    })
}

// LedgerRow / CoverageLedger / build_ledger

/// One `crate::capability::CapabilityPredicate` that discharges a `LedgerRow`'s
/// `CharacteristicKind`, alongside that predicate's own `EvidenceProvenance`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DischargingPredicate {
    pub id: String,
    pub provenance: EvidenceProvenance,
}

/// One row of the coverage ledger: everything this crate can say
/// today about one `CharacteristicKind` — the frozen-model construct(s) it represents (see that
/// type's own per-variant doc in `capability.rs` for the exact `model.rs` citation), its
/// disposition, which predicates (if any) discharge it, which `constructs.txt` id(s) it maps to and
/// whether a passing fixture is known to cover them, and its curated containment-test citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerRow {
    pub kind: CharacteristicKind,
    /// ALWAYS `kind.default_disposition()` — never a second, independently-maintained copy. See
    /// this file's own `ledger_disposition_never_diverges_from_default_disposition` test.
    pub disposition: Disposition,
    /// Every registered `crate::capability::CapabilityPredicate` whose [`crate::capability::
    /// CapabilityPredicate::discharges`] names this row's `kind`. Empty for every [`Disposition::
    /// Proven`] kind (none needed) and for a `Disposition::ConfirmOnly` kind with no registered
    /// predicate (also fine — only `ConfigPredicate` kinds REQUIRE one, per
    /// `crate::capability::undischarged_kinds`).
    #[serde(default)]
    pub discharging_predicates: Vec<DischargingPredicate>,
    /// `machine/conformance/constructs.txt` identifier(s) this kind maps to (reused verbatim from
    /// `construct_ids_for` — never re-derived). Empty iff `Self::conformance_status` is
    /// `CoverageStatus::Unmappable`.
    pub construct_ids: Vec<String>,
    /// This row's conformance-coverage cross-check outcome against the ledger's own build-time
    /// passing-construct set (see `build_ledger`'s own doc: this ledger reuses [`construct_ids_
    /// for`]/`CoverageStatus` rather than re-deriving the classification rule).
    pub conformance_status: CoverageStatus,
    /// The curated proposer-to-confirm containment witness, if this crate's test
    /// suite has one for this construct (`None` only for a genuine, honestly-reported gap).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub containment: Option<ContainmentEvidence>,
    /// Every `EmissionStrategy` whose proposer emits nothing at all for this construct
    /// (`crate::strategy_coverage::StrategyRepresentation::CannotRepresent`), as
    /// `EmissionStrategy::label` strings. A whole-construct recall hole for that compiler --
    /// a candidate realized by one is a typed refusal, pinned by
    /// `a_construct_the_adapter_cannot_represent_is_a_typed_refusal_never_a_substitution`.
    #[serde(default)]
    pub strategies_cannot_represent: Vec<String>,
    /// Every strategy that CAN represent this construct but which no citation in
    /// `containment_evidence_for` names -- i.e. coverage this row would be INHERITING rather than
    /// demonstrating. Non-empty is not a failure; it is the honest reading of the evidence, and the
    /// thing that was invisible before. See `ContainmentEvidence`'s own doc for the incident.
    #[serde(default)]
    pub strategies_unwitnessed: Vec<String>,
}

/// The full, versioned, one-time-audited coverage ledger. See this module's own top-doc "Evidence,
/// not a gate" section: this type is inert data, consulted by no compile path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageLedger {
    pub schema_version: u32,
    /// One row per `CharacteristicKind::ALL` entry, in that constant's own declaration order.
    pub rows: Vec<LedgerRow>,
}

impl CoverageLedger {
    /// This ledger's row for `kind`, if present (always present in any ledger built by
    /// `build_ledger` — see `every_characteristic_kind_appears_exactly_once`).
    pub fn row(&self, kind: CharacteristicKind) -> Option<&LedgerRow> {
        self.rows.iter().find(|r| r.kind == kind)
    }

    /// Canonical machine-readable form (mirrors `crate::health::HealthReport::to_json` exactly:
    /// pretty-printed, two-space indent, Rust declaration field order).
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Parses a ledger from its canonical JSON form.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }
}

/// Builds the coverage ledger: one `LedgerRow` per `CharacteristicKind::ALL` entry, in that
/// constant's own order. A pure function over a caller-supplied `registry` (whose predicates
/// determine `LedgerRow::discharging_predicates`) and `passing_covered_constructs` (the same
/// "set of `constructs.txt` identifiers exercised by at least one currently-passing fixture" shape
/// `crate::conformance_coverage::supported_coverage_report` itself takes) — mirroring that
/// module's own "pure core, wired-up glue lives at the edge" split: nothing here calls
/// `pg_conformance_fixtures::discover` or replays any fixture itself; a caller (e.g. a
/// `tests/coverage_ledger_gate.rs`, mirroring `tests/conformance_coverage_gate.rs`) supplies that
/// set by actually replaying fixtures, or a caller wanting a static, no-dynamic-dependency snapshot
/// may pass an empty set or a fixed hand-built one (as this crate's own golden-JSON test does, for
/// reproducibility independent of fixture churn elsewhere in the repo).
///
/// `conformance_status`'s classification is the identical three-way rule
/// `crate::conformance_coverage::supported_coverage_report`'s own inner closure uses
/// (`construct_ids.is_empty()` -> `CoverageStatus::Unmappable`; at least one of the row's
/// construct ids in `passing_covered_constructs` -> `CoverageStatus::Covered`; otherwise
/// `CoverageStatus::Uncovered`) — re-stated here rather than called, because the two callers
/// thread their evidence sets differently, not because the two scopes differ. The underlying
/// contract (`construct_ids_for` plus `CoverageStatus` itself) is reused unchanged, never
/// re-derived.
pub fn build_ledger(
    registry: &PredicateRegistry,
    passing_covered_constructs: &HashSet<&str>,
) -> CoverageLedger {
    let rows = CharacteristicKind::ALL
        .iter()
        .copied()
        .map(|kind| {
            let disposition = kind.default_disposition();

            let discharging_predicates: Vec<DischargingPredicate> = registry
                .predicates()
                .iter()
                .filter(|p| p.discharges().contains(&kind))
                .map(|p| DischargingPredicate {
                    id: p.id().to_string(),
                    provenance: p.provenance(),
                })
                .collect();

            let construct_ids_static = construct_ids_for(kind);
            let construct_ids: Vec<String> =
                construct_ids_static.iter().map(|s| s.to_string()).collect();
            let containment = containment_evidence_for(kind);

            let conformance_status = if construct_ids_static.is_empty() {
                CoverageStatus::Unmappable
            } else if construct_ids_static
                .iter()
                .any(|c| passing_covered_constructs.contains(c))
            {
                CoverageStatus::Covered
            } else {
                CoverageStatus::Uncovered
            };

            // Both lists are derived from crate::strategy_coverage, never hand-maintained.
            let strategies_cannot_represent: Vec<String> = crate::strategy_coverage::ALL_STRATEGIES
                .iter()
                .copied()
                .filter(|&s| {
                    representation_of(s, kind).representation
                        == crate::strategy_coverage::StrategyRepresentation::CannotRepresent
                })
                .map(|s| s.label().to_string())
                .collect();

            let witnessed: Vec<String> = containment
                .as_ref()
                .map(|ev| ev.strategies.clone())
                .unwrap_or_default();
            let strategies_unwitnessed: Vec<String> = strategies_that_represent(kind)
                .into_iter()
                .map(|s| s.label().to_string())
                .filter(|label| !witnessed.contains(label))
                .collect();

            LedgerRow {
                kind,
                disposition,
                discharging_predicates,
                construct_ids,
                conformance_status,
                containment,
                strategies_cannot_represent,
                strategies_unwitnessed,
            }
        })
        .collect();

    CoverageLedger {
        schema_version: COVERAGE_LEDGER_SCHEMA_VERSION,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{default_registry, undischarged_kinds};

    /// A fixed, hand-built passing set so these ledgers are deterministic and independent of any real fixture's pass/fail state.
    fn fully_covered_constructs() -> HashSet<&'static str> {
        let mut set = HashSet::new();
        for &kind in CharacteristicKind::ALL {
            for &id in construct_ids_for(kind) {
                set.insert(id);
            }
        }
        set
    }

    // Exhaustiveness / no-drift

    /// Every `CharacteristicKind` appears in the built ledger exactly once.
    #[test]
    fn every_characteristic_kind_appears_exactly_once() {
        let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
        assert_eq!(ledger.rows.len(), CharacteristicKind::ALL.len());
        for &kind in CharacteristicKind::ALL {
            let count = ledger.rows.iter().filter(|r| r.kind == kind).count();
            assert_eq!(count, 1, "{kind:?} must appear exactly once in the ledger");
        }
    }

    /// Every row's `disposition` is always exactly `kind.default_disposition()`, never a hardcoded copy.
    #[test]
    fn ledger_disposition_never_diverges_from_default_disposition() {
        let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
        for row in &ledger.rows {
            assert_eq!(
                row.disposition,
                row.kind.default_disposition(),
                "{:?}'s ledger disposition diverged from default_disposition()",
                row.kind
            );
        }
    }

    /// Every `ConfigPredicate` row names at least one discharging predicate, agreeing with `undischarged_kinds`.
    #[test]
    fn every_config_predicate_row_names_a_discharging_predicate() {
        let registry = default_registry();
        assert!(
            undischarged_kinds(&registry).is_empty(),
            "sanity: default_registry() is expected to already discharge every ConfigPredicate kind"
        );
        let ledger = build_ledger(&registry, &HashSet::new());
        for row in &ledger.rows {
            if row.disposition == Disposition::ConfigPredicate {
                assert!(
                    !row.discharging_predicates.is_empty(),
                    "{:?} is {:?} but the ledger names no discharging predicate",
                    row.kind,
                    row.disposition
                );
            }
        }
    }

    /// `containment_evidence_for` must be callable end to end for every kind.
    #[test]
    fn containment_evidence_for_is_callable_for_every_kind() {
        for &kind in CharacteristicKind::ALL {
            let _ = containment_evidence_for(kind);
        }
    }

    // Evidence names its strategies (the coverage-gate inheritance trap, per-strategy axis)

    /// A construct claimed covered must name the strategies it was demonstrated on; no citation may be unattributed or name a compiler that does not exist.
    #[test]
    fn every_containment_citation_names_at_least_one_real_strategy() {
        let valid: HashSet<&str> = crate::strategy_coverage::ALL_STRATEGIES
            .iter()
            .map(|s| s.label())
            .collect();
        for &kind in CharacteristicKind::ALL {
            let Some(evidence) = containment_evidence_for(kind) else {
                continue;
            };
            assert!(
                !evidence.strategies.is_empty(),
                "{kind:?}'s containment citation names no strategy: {}",
                evidence.citation
            );
            for named in &evidence.strategies {
                assert!(
                    valid.contains(named.as_str()),
                    "{kind:?}'s citation names an unknown strategy {named:?}"
                );
            }
        }
    }

    /// A citation may never name a strategy the per-strategy account says cannot represent the construct -- either the citation or the table row is wrong.
    #[test]
    fn no_citation_claims_a_strategy_that_cannot_represent_the_construct() {
        for &kind in CharacteristicKind::ALL {
            let Some(evidence) = containment_evidence_for(kind) else {
                continue;
            };
            for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
                if !evidence.strategies.iter().any(|s| s == strategy.label()) {
                    continue;
                }
                assert_ne!(
                    representation_of(strategy, kind).representation,
                    crate::strategy_coverage::StrategyRepresentation::CannotRepresent,
                    "{kind:?}'s citation claims {strategy:?}, which the strategy account says \
                     cannot represent the construct at all"
                );
            }
        }
    }

    /// `strategies_unwitnessed` must be derived (representing set minus evidence set), not hand-maintained, and genuinely non-empty somewhere.
    #[test]
    fn unwitnessed_strategies_are_derived_and_the_gap_is_reported_not_hidden() {
        let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
        let mut any_gap = false;
        for row in &ledger.rows {
            let witnessed: Vec<String> = row
                .containment
                .as_ref()
                .map(|e| e.strategies.clone())
                .unwrap_or_default();
            let expected: Vec<String> = strategies_that_represent(row.kind)
                .into_iter()
                .map(|s| s.label().to_string())
                .filter(|l| !witnessed.contains(l))
                .collect();
            assert_eq!(
                row.strategies_unwitnessed, expected,
                "{:?}'s unwitnessed set must be derived, never hand-written",
                row.kind
            );
            any_gap |= !row.strategies_unwitnessed.is_empty();
        }
        assert!(
            any_gap,
            "no row reports an unwitnessed strategy -- either every construct now has a witness on \
             every compiler that can represent it (check before believing it), or the derivation \
             silently collapsed"
        );
    }

    /// The per-row `CannotRepresent` list makes the live `PlanComposed` x `RealizationalMorphology` hole visible in the ledger, not only in the selection path.
    #[test]
    fn the_ledger_reports_the_live_whole_construct_hole() {
        let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
        let row = ledger
            .row(CharacteristicKind::RealizationalMorphology)
            .expect("row must exist");
        assert_eq!(
            row.strategies_cannot_represent,
            vec![EmissionStrategy::PlanComposed.label().to_string()]
        );
        assert!(
            ledger
                .row(CharacteristicKind::Affixation)
                .expect("row must exist")
                .strategies_cannot_represent
                .is_empty(),
            "no strategy fails to represent ordinary affixation"
        );
    }

    /// `NaturalClassDefinition` and `FreeFluctuation` are the deliberate, documented `None`s; a future edit that starts or stops returning evidence for either must be a reviewed, visible change.
    #[test]
    fn every_kind_without_a_containment_witness_is_named_and_justified() {
        let missing: Vec<CharacteristicKind> = CharacteristicKind::ALL
            .iter()
            .copied()
            .filter(|&k| containment_evidence_for(k).is_none())
            .collect();
        assert_eq!(
            missing,
            vec![
                CharacteristicKind::NaturalClassDefinition,
                CharacteristicKind::FreeFluctuation,
                // No test drives an ablaut grammar through propose-then-confirm on ANY backend.
                CharacteristicKind::ProcessMorphology
            ],
            "every kind without a containment witness must be named here with a reason -- an \
             unexplained addition means somebody added a construct and skipped its witness"
        );
    }

    // build_ledger: conformance_status classification

    #[test]
    fn build_ledger_with_empty_passing_set_never_marks_a_fixture_evidenced_row_covered() {
        let ledger = build_ledger(&default_registry(), &HashSet::new());
        for row in &ledger.rows {
            assert_ne!(
                row.conformance_status,
                CoverageStatus::Covered,
                "{:?}",
                row.kind
            );
        }
    }

    #[test]
    fn build_ledger_with_fully_covered_set_covers_every_mappable_row() {
        let covered = fully_covered_constructs();
        let ledger = build_ledger(&default_registry(), &covered);
        for row in &ledger.rows {
            if row.construct_ids.is_empty() {
                assert_eq!(
                    row.conformance_status,
                    CoverageStatus::Unmappable,
                    "{:?}",
                    row.kind
                );
            } else {
                assert_eq!(
                    row.conformance_status,
                    CoverageStatus::Covered,
                    "{:?}",
                    row.kind
                );
            }
        }
    }

    /// Zero ledger rows are `Unmappable`, unconditionally -- depends only on `construct_ids_for` being non-empty per kind, never on the passing-fixture set.
    #[test]
    fn zero_unmappable_rows_after_g9() {
        let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
        let unmappable: Vec<CharacteristicKind> = ledger
            .rows
            .iter()
            .filter(|r| r.conformance_status == CoverageStatus::Unmappable)
            .map(|r| r.kind)
            .collect();
        assert!(
            unmappable.is_empty(),
            "expected zero Unmappable ledger rows; found {unmappable:?}"
        );
        for row in &ledger.rows {
            assert!(
                !row.construct_ids.is_empty(),
                "{:?} still has no constructs.txt mapping after G9",
                row.kind
            );
        }
    }

    #[test]
    fn row_accessor_finds_every_kind_exactly_once() {
        let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
        for &kind in CharacteristicKind::ALL {
            assert!(
                ledger.row(kind).is_some(),
                "{kind:?} must be findable via row()"
            );
        }
    }

    // Canonical JSON: golden + round trip

    /// A deterministic, fully-covered-set ledger, so this golden stays stable regardless of unrelated fixture churn.
    fn golden_ledger() -> CoverageLedger {
        build_ledger(&default_registry(), &fully_covered_constructs())
    }

    #[track_caller]
    fn assert_coverage_ledger_golden(actual: &str, expected: &str) {
        crate::test_support::assert_canonical_lf_text_eq(actual, expected);
    }

    #[test]
    fn coverage_ledger_golden_boundary_accepts_lf_actual_against_crlf_expected() {
        let actual = "{\n  \"schema_version\": 1\n}\n";
        let expected = actual.replace('\n', "\r\n");
        assert_ne!(actual, expected);
        assert_coverage_ledger_golden(actual, &expected);
    }

    #[test]
    fn coverage_ledger_golden_boundary_rejects_crlf_actual() {
        let actual = "{\n  \"schema_version\": 1\n}\n";
        let expected = "{\n  \"schema_version\": 1\n}\n";
        let crlf_actual = actual.replace('\n', "\r\n");
        assert_ne!(crlf_actual, expected);
        let panic = std::panic::catch_unwind(|| {
            assert_coverage_ledger_golden(&crlf_actual, expected);
        });
        assert!(panic.is_err());
    }

    #[test]
    fn coverage_ledger_golden_boundary_rejects_ordering_and_trailing_newline_drift() {
        let ordering = std::panic::catch_unwind(|| {
            assert_coverage_ledger_golden(
                "{\n  \"a\": 1,\n  \"b\": 2\n}\n",
                "{\n  \"b\": 2,\n  \"a\": 1\n}\n",
            );
        });
        assert!(ordering.is_err());

        let trailing_newline = std::panic::catch_unwind(|| {
            assert_coverage_ledger_golden(
                "{\n  \"schema_version\": 1\n}",
                "{\n  \"schema_version\": 1\n}\n",
            );
        });
        assert!(trailing_newline.is_err());
    }

    #[test]
    fn coverage_ledger_round_trip() {
        let ledger = golden_ledger();
        let json = ledger.to_json().expect("serialization must succeed");
        let parsed = CoverageLedger::from_json(&json).expect("deserialization must succeed");
        assert_eq!(
            parsed, ledger,
            "round trip through canonical JSON must be lossless"
        );
    }

    #[test]
    fn coverage_ledger_schema_version_is_stamped() {
        assert_eq!(
            golden_ledger().schema_version,
            COVERAGE_LEDGER_SCHEMA_VERSION
        );
    }

    #[test]
    #[ignore = "regeneration helper, not a gate: run with --ignored to rewrite the golden from this \
                test's own computation after a reviewed citation/predicate change"]
    fn regenerate_coverage_ledger_golden_json() {
        let json = golden_ledger()
            .to_json()
            .expect("serialization must succeed");
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/coverage_ledger_golden.json"
            ),
            json,
        )
        .expect("golden must be writable");
    }

    #[test]
    fn coverage_ledger_golden_json() {
        let ledger = golden_ledger();
        let json = ledger.to_json().expect("serialization must succeed");
        assert_coverage_ledger_golden(&json, GOLDEN_JSON);
    }

    const GOLDEN_JSON: &str = include_str!("coverage_ledger_golden.json");
}
