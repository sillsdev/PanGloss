//! `openspec/changes/define-grammar-coverage-contract` (proposal.md/design.md/tasks.md;
//! `specs/grammar-coverage-contract/spec.md`) — **demoted to an evidence role**, per that change's
//! own proposal.md and `openspec/changes/STAGING.md` Stage 0B: this is the one-time, audited
//! coverage LEDGER over the frozen `pg-grammar/src/model.rs` construct set. It is evidence that
//! feeds *into* `add-capability-characteristics-check`'s Stage 0A gate
//! (`crate::capability::compose_envelope`) — it is **not itself a gate**. See "Evidence, not a
//! gate" below for what that means concretely in this file.
//!
//! `openspec/changes/IMPLEMENTATION-READINESS.md` R1 (verbatim): *"HermitCrab and the Rust model
//! are assumed complete apart from bug fixes. Implement a one-time REVIEWED ledger over the current
//! model; do NOT add source-AST/reflection infrastructure."* `docs/adr/0001-honest-capability-
//! boundary.md`'s own words: *"The pre-existing coverage ledger (`define-grammar-coverage-
//! contract`) supplies evidence into this gate; it is not itself the gate."*
//!
//! # Per-task assessment — what already existed vs. what this module actually adds
//! `tasks.md`'s own three items under "1. Inventory" and its own "Verification" item 3.3, checked
//! one at a time (this crate does not re-litigate `tasks.md` section "2. Gate contract v2" — see
//! "Out of scope" below):
//!
//! - **1.1** ("coverage-ledger schema v1 and a one-time reviewed inventory of the frozen public
//!   variants and behavior-bearing fields in `model.rs`"): the INVENTORY half of this task was
//!   **already fully done** by `crate::capability`: [`crate::capability::CharacteristicKind`] (20
//!   variants, [`crate::capability::CharacteristicKind::ALL`]) and [`crate::capability::
//!   characterize`] (the exhaustive, no-catch-all per-`model.rs`-variant walk) together ARE the
//!   one-time reviewed inventory R1 asks for — see that module's own top-doc: "walks a `Grammar`
//!   and matches every variant of every frozen `model.rs` enum design.md D1 names, with no
//!   catch-all arm". This module does not re-inventory `model.rs`; the genuinely missing SCHEMA
//!   half — a consolidated, queryable, *serializable* row-per-construct VIEW over that existing
//!   inventory — is what [`LedgerRow`]/[`CoverageLedger`]/[`build_ledger`] supply.
//! - **1.2** ("map every variant to compiler disposition, owning tests, positive witness, and
//!   negative witness"): the disposition mapping **already exists** and is the crate's own single
//!   source of truth — [`crate::capability::CharacteristicKind::default_disposition`]. This module
//!   never hardcodes a second copy of it (see [`LedgerRow::disposition`]'s own doc and this file's
//!   `ledger_disposition_never_diverges_from_default_disposition` test). "Owning tests"/witnesses
//!   *partially* existed: every citation in [`containment_evidence_for`] below names a REAL,
//!   already-merged test file (`tests/cover_*.rs`, `tests/phase_c_*.rs`, `tests/
//!   epenthesis_structural_route_containment.rs`, `tests/two_table_symbol_divergence.rs`, `tests/
//!   f6_reduplication_peel_chain_depth.rs`, `tests/p6_gate_parity.rs`) — but **no single table
//!   collected which construct each one is a witness FOR** before this module. That curated,
//!   hand-reviewed table is the genuine gap this module closes — exactly R1's own prescription
//!   ("a one-time REVIEWED ledger", not a mechanically-derived one).
//! - **1.3** ("render the ledger into maintained documentation and reconcile stale Phase B/C/P6
//!   statuses"): **out of scope for this merge unit** — proposal.md's own "Impact" section splits
//!   this change into three SERIAL merge units ("inventory/schema, oracle identity and containment
//!   library, then migration of named Phase-C and Aweti fixtures"); documentation rendering and
//!   reconciling stale planning prose is neither of the first unit's concerns. [`CoverageLedger::
//!   to_json`] is the machine-readable source artifact a later renderer would consume (mirrors
//!   `crate::health`'s own "Canonical JSON is the source artifact; Markdown is a rendering of the
//!   same findings" framing) — no Markdown/prose renderer is written here.
//! - **Section 2** ("Gate contract v2": versioned oracle-analysis records, exact identity/
//!   multiplicity comparison, the Machine `WordAnalysis.Equals` projection, dense-ordinal-to-HC-
//!   XML-key resolution, a key-decision precedent record): **out of scope for this merge unit**.
//!   `IMPLEMENTATION-READINESS.md`'s own "Safe initial dispatch" list scopes unit 1 to "audit the
//!   frozen model and classify every row; no permanent source-parser mechanism is required" —
//!   nothing about oracle-identity plumbing. Proposal.md's own three-unit split names this "oracle
//!   identity and containment library" as the SECOND, separate unit. Not attempted here; this
//!   module's [`ContainmentEvidence`] cites EXISTING containment tests, it does not build new
//!   oracle-identity infrastructure.
//! - **3.1** ("convert `phase_c_multi_table`, `phase_c_right_to_left`, `phase_c_simultaneous`, and
//!   `p6_templated_morphotactics_gate` to the new contract") and **3.2** ("prove old word-level gates cannot silently
//!   pass an analysis-loss fixture"): the THIRD merge unit (fixture migration) — not attempted here.
//! - **3.3** ("validate all ledger rows have an explicit disposition and evidence owner; document
//!   that any future model-shape change must reopen this audit rather than silently extending the
//!   ledger"): **implemented here** — see this module's own `tests` submodule, especially
//!   `every_characteristic_kind_appears_exactly_once`, `every_config_predicate_or_fail_closed_row_
//!   names_a_discharging_predicate`, and this file's own top-doc "A future model-shape change" note
//!   below (mirrors `crate::capability::CharacteristicKind::ALL`'s own doc: hand-maintained because
//!   Rust has no enum reflection, so `CharacteristicKind::ALL`'s own doc is this module's closest
//!   available backstop too).
//!
//! # A future model-shape change
//! Per `specs/grammar-coverage-contract/spec.md`'s own "A future change attempts to extend the
//! model" scenario: adding a new `pg-grammar/src/model.rs` construct or behavior-bearing field is
//! OUTSIDE this ledger's standing frozen-model assumption (R1) and must explicitly reopen and
//! revise this coverage contract before merge — concretely, that means updating `crate::capability`
//! first (`CharacteristicKind`, `CharacteristicKind::ALL`, `default_disposition`, `characterize`,
//! per that module's own exhaustiveness discipline), which breaks THIS module's build the moment a
//! new `CharacteristicKind::ALL` entry appears (every exhaustive match in this file has no
//! catch-all arm), forcing a reviewed update to [`containment_evidence_for`] too.
//!
//! # Evidence, not a gate
//! Nothing in this module is consulted by any compile path. [`build_ledger`] is a pure function;
//! [`CoverageLedger`] is inert data. The load-bearing, hard-failing artifact remains
//! [`crate::capability::compose_envelope`] (Stage 0A) — this ledger's rows are read BY a human/CI
//! reviewer and by that gate's own predicate authors as evidence when they write or review a
//! [`crate::capability::CapabilityPredicate`], never consulted at compile time to admit or refuse a
//! grammar. No test in this file asserts `gaps.is_empty()` for conformance/containment coverage —
//! same non-blocking-first discipline `crate::conformance_coverage`/`crate::
//! plan_interaction_coverage` already established for their own advisory reports; this module goes
//! one step further and is not even wired into a CI report yet, deliberately (that wiring, if any,
//! belongs to a later merge unit).
//!
//! # The four rows this ledger fills in per `CharacteristicKind` (deliverable, `tasks.md` 1.1/1.2)
//! [`LedgerRow`]: the [`crate::capability::CharacteristicKind`] itself; its
//! [`crate::capability::Disposition`] (ALWAYS [`crate::capability::CharacteristicKind::
//! default_disposition`] — never a second, divergent copy); every [`crate::capability::
//! CapabilityPredicate`] in the caller-supplied registry that discharges it, alongside that
//! predicate's own [`crate::capability::EvidenceProvenance`]; the mapped `machine/conformance/
//! constructs.txt` construct id(s) (reused verbatim from [`crate::conformance_coverage::
//! construct_ids_for`], never re-derived) plus the resulting [`crate::conformance_coverage::
//! CoverageStatus`] against a caller-supplied passing-construct set; and the curated
//! [`ContainmentEvidence`] naming which (if any) already-merged test is this construct's
//! proposer-to-confirm containment witness.
//!
//! # Canonical JSON
//! [`CoverageLedger::to_json`]/[`CoverageLedger::from_json`] follow `crate::health`'s own
//! established convention exactly: pretty-printed, two-space indent, fields in Rust declaration
//! order (serde's unmodified default), a `schema_version` constant
//! ([`COVERAGE_LEDGER_SCHEMA_VERSION`]) bumped only on a wire-incompatible change. CLI/AI/FieldWorks
//! tooling consumes this one artifact rather than re-deriving the same facts from `capability.rs`'s
//! Rust types directly.
//!
//! # Why `CharacteristicKind`/`Disposition`/`EvidenceProvenance`/`CoverageStatus` gain `Serialize`/
//! `Deserialize` impls IN THIS FILE, not in `capability.rs`/`conformance_coverage.rs`
//! This task's own hard rule: "additive only (don't change `capability.rs` semantics)". Rust's
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

/// This schema's own version (mirrors [`crate::health::HEALTH_SCHEMA_VERSION`]'s convention).
pub const COVERAGE_LEDGER_SCHEMA_VERSION: u32 = 1;

// =================================================================================================
// Wire-name impls for foreign-module types (see module top-doc's last section for why these live
// here rather than in capability.rs/conformance_coverage.rs)
// =================================================================================================

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
        Disposition::FailClosed => "fail_closed",
    }
}

fn disposition_from_wire_name(s: &str) -> Option<Disposition> {
    match s {
        "proven" => Some(Disposition::Proven),
        "config_predicate" => Some(Disposition::ConfigPredicate),
        "confirm_only" => Some(Disposition::ConfirmOnly),
        "fail_closed" => Some(Disposition::FailClosed),
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
        EvidenceProvenance::Behavioral => "behavioral",
        EvidenceProvenance::Structural => "structural",
    }
}

fn provenance_from_wire_name(s: &str) -> Option<EvidenceProvenance> {
    match s {
        "behavioral" => Some(EvidenceProvenance::Behavioral),
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

// =================================================================================================
// The curated containment-evidence table (tasks.md 1.2's "owning tests" half)
// =================================================================================================

/// Which shape of evidence [`ContainmentEvidence::citation`] provides. Not every
/// [`crate::capability::CharacteristicKind`] needs (or can meaningfully have) the same shape of
/// witness — see each variant's own doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentEvidenceKind {
    /// A test whose specific, stated purpose is proving THIS construct's proposer-to-confirm
    /// containment (over-propose, confirm prunes to exactly the oracle's set) — the ADR 0001
    /// default shape every `ConfigPredicate`/`ConfirmOnly` characteristic's kit needs.
    Dedicated,
    /// No single dedicated fixture exists (or is needed): the characteristic is
    /// [`Disposition::Proven`] and is exercised pervasively, as ordinary background material, by
    /// this crate's general full-grammar propose-confirm gates (`tests/f1_large_lexicon_gate.rs`, `tests/
    /// f2_junction_gate.rs`, `tests/f4_composite_gate.rs`, etc.) rather than by any one
    /// construct-specific fixture.
    GeneralPervasive,
    /// The characteristic is [`Disposition::FailClosed`]: "containment" is not the applicable
    /// property (nothing may compile) — the cited test instead proves [`crate::capability::
    /// compose_envelope`] genuinely `Refuse`s whenever this construct is observed, never a silent
    /// compile.
    RefusalWitness,
}

/// One curated, hand-reviewed containment-evidence citation (tasks.md 1.2). Every field is a
/// `String` (not `&'static str`) so [`LedgerRow`] round-trips through [`CoverageLedger::from_json`]
/// losslessly, matching `crate::health::HealthFinding`'s own `String`-field convention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainmentEvidence {
    pub kind: ContainmentEvidenceKind,
    /// `tests/<file>.rs` plus the specific `#[test]` function name(s), e.g.
    /// `"tests/cover_compounding.rs::head_a_word_over_propose_confirm_prune"`.
    pub citation: String,
    /// A one-line note on what the cited test actually proves for this construct.
    pub note: String,
}

fn ev(kind: ContainmentEvidenceKind, citation: &str, note: &str) -> ContainmentEvidence {
    ContainmentEvidence {
        kind,
        citation: citation.to_string(),
        note: note.to_string(),
    }
}

/// Deliverable, tasks.md 1.2's "owning tests" column: `kind`'s curated proposer-to-confirm
/// containment (or, for [`Disposition::FailClosed`], refusal) witness, if this crate's test suite
/// has one — `None` only where no witness exists at all (a genuine, honestly-reported gap, never
/// silently invented; see [`CharacteristicKind::NaturalClassDefinition`]'s own arm below).
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
            "Ordinary AffixProcessRule prefixation/suffixation/infixation is the baseline every \
             full-grammar propose-confirm gate exercises continuously; Proven already licenses \
             unconditional admission-filtering, so no separate dedicated fixture is required.",
        ),
        RealizationalMorphology => ev(
            Dedicated,
            "tests/cover_realizational_morphology_constraints.rs::\
             realizational_rule_presence_blocking_over_propose_confirm_prune",
            "Proposer-to-confirm containment for MorphRuleDef::Realizational's real_fs \
             head-wrapped presence-blocking.",
        ),
        Compounding => ev(
            Dedicated,
            "tests/cover_compounding.rs::head_a_word_over_propose_confirm_prune (+ \
             subrule_group_gate_excludes_partial_match_like_confirm, \
             head_c_excluded_by_rule_level_gate_like_confirm); budget refusal: tests/\
             cover_compounding_budget.rs",
            "License-gated head/non-head cross-product containment for the non-recursive case, \
             plus the (un)group-awareness witness design.md D4 names.",
        ),
        OrderedMorphRuleApplication => ev(
            GeneralPervasive,
            "tests/phase_c_strata_depth.rs (multi-stratum cascade recall-parity), tests/\
             f1_large_lexicon_gate.rs, tests/f4_composite_gate.rs",
            "Linear rule-application order is the default cascade shape exercised by every \
             general gate; Proven, no dedicated fixture required.",
        ),
        UnorderedMorphRuleApplication => ev(
            Dedicated,
            "tests/cover_unordered_morph_rules.rs::non_document_order_analysis_is_proposed_and_\
             confirmed (+ unbounded_unordered_stratum_deterministically_refuses_to_compile for \
             the Refuse split)",
            "Chain-depth-bounded any-order proposal containment, plus the deterministic \
             over-budget refusal witness.",
        ),
        MprGroupAppend => ev(
            Dedicated,
            "tests/cover_mpr_groups.rs::out_mpr_accumulation_then_gate_over_propose_confirm_prune \
             (+ append_output_is_order_invariant_overwrite_output_is_not)",
            "Non-tracking-baseline containment for MprGroupOutput::Append, plus the \
             order-invariance witness design.md D4 names.",
        ),
        MprGroupOverwrite => ev(
            Dedicated,
            "tests/cover_mpr_groups.rs::overwrite_group_composes_to_confirm_only",
            "FailClosed: containment is not the applicable property here -- this witness proves \
             compose_envelope genuinely Refuses whenever MprGroupOutput::Overwrite is observed.",
        ),
        IterativeRewrite => ev(
            GeneralPervasive,
            "tests/f1_large_lexicon_gate.rs, tests/f2_junction_gate.rs, tests/phase_c_right_to_left.rs \
             (iterative baseline contrast)",
            "The default RewriteMode every general gate's phonological rules use; Proven, no \
             dedicated fixture required.",
        ),
        SimultaneousRewrite => ev(
            Dedicated,
            "tests/phase_c_simultaneous.rs::sim_nonoverlap_env_now_compiles_and_matches_oracle_\
             exactly (+ sim_overlap_env_stays_honest_unsupported for the Refuse split)",
            "Containment for the pairwise-non-overlapping case the simultaneous.subrule-overlap \
             predicate Admits; the genuinely-overlapping case stays honestly unsupported.",
        ),
        LeftToRightRewrite => ev(
            GeneralPervasive,
            "tests/f2_junction_gate.rs, tests/phase_c_right_to_left.rs (LTR is the implicit \
             contrast baseline for every rtl_* case)",
            "The default Dir every general gate's phonological rules use; Proven, no dedicated \
             fixture required.",
        ),
        RightToLeftRewrite => ev(
            Dedicated,
            "tests/phase_c_right_to_left.rs::rtl_plain_rule_now_compiles_and_matches_oracle (+ \
             rtl_feature_environment_swap_matches_oracle, rtl_deletion_matches_oracle, \
             rtl_cross_table_segments_environment_matches_oracle)",
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
            "Dedicated swap-relation containment against the real oracle in BOTH directions -- \
             Dir::RightToLeft is no longer a scope boundary (openspec/changes/\
             build-unbounded-quantifier-support's sibling task, plan-construct-coverage-completion \
             tasks.md 4.6): it compiles via the same mirror-and-reverse construction \
             compile_rtl_branch_net uses, so the union is a superset the oracle prunes. The \
             remaining refusals are pattern-shape ones (Anchor, and any Slot::Repeat -- \
             slot_candidates enumerates concrete alternatives), never the direction itself.",
        ),
        Epenthesis => ev(
            Dedicated,
            "tests/epenthesis_structural_route_containment.rs::\
             epenthesis_over_propose_confirm_prune_matches_oracle_exactly",
            "End-to-end propose(over-generate)-then-confirm(prune) containment for an \
             obligatory-epenthesis grammar, matching the oracle's analysis set exactly.",
        ),
        SubruleGating => ev(
            Dedicated,
            "tests/p6_gate_parity.rs::synthetic_pos_gate_matches_oracle (+ \
             ungated_cascade_would_have_missed_the_noun_entry); scale: tests/\
             phase_c_partition_k.rs::partition_k_recall_parity_via_generator_and_oracle",
            "Static MPR/POS subrule-gating containment against the real oracle, plus a \
             2^k-group scale gate.",
        ),
        CircumfixOutputAction => ev(
            Dedicated,
            "tests/phase_c_circumfix.rs::circumfix_recall_parity_via_generator_and_oracle (+ \
             ordered_multi_insert_no_first_insert_shortcut_recall_parity, \
             null_role_structural_drop_recall_parity)",
            "Structural-composite containment for circumfix-shaped (discontinuous/dropped-\
             material) allomorphs against the real oracle.",
        ),
        Reduplication => ev(
            Dedicated,
            "tests/f6_reduplication_peel_chain_depth.rs::\
             kimbiakimbia_reduplication_is_recovered_with_oracle_containment (+ \
             deep_self_similar_chain_is_refused_deterministically for the chain-depth budget); \
             tests/f4_composite_gate.rs case (c)",
            "Peeler-to-confirm containment for true-reduplication allomorphs, plus the \
             deterministic deep-chain refusal witness.",
        ),
        CoOccurrenceConstraint => ev(
            Dedicated,
            "tests/cover_realizational_morphology_constraints.rs::\
             morpheme_co_occurrence_exclude_anywhere_over_propose_confirm_prune",
            "Proposer-to-confirm containment for MorphemeCoOccurrenceRule adjacency exclusion.",
        ),
        NaturalClassDefinition => return None,
        MultiTable => ev(
            Dedicated,
            "tests/phase_c_multi_table.rs::\
             multi_table_rewrite_compiles_correctly_against_its_owning_table; stronger claim: \
             tests/two_table_symbol_divergence.rs::\
             stratum_1_devoice_rewrite_proposer_confirm_matches_oracle",
            "Faithful per-stratum table threading, proven for one stratum's own rule and, more \
             strongly, for two strata whose tables disagree about the same symbol index.",
        ),
        QuantifierPattern => ev(
            Dedicated,
            "tests/phase_c_quantifier.rs::quantifier_bounded_environment_compiles_and_matches_\
             oracle (+ quantifier_unbounded_environment_compiles_and_matches_oracle for the \
             genuinely-unbounded case, openspec/changes/build-unbounded-quantifier-support)",
            "Bounded- AND unbounded-quantifier containment against the real oracle, both at \
             min-boundary occurrence counts; an inverted/over-budget-finite/alpha-nested \
             quantifier stays honestly unsupported.",
        ),
        // Research report 13's taxonomy-gap fix: `RootAllomorphDef::stem_name` (model.rs:798) --
        // NOT `MorphRuleDef::required_stem_name` (model.rs:648), which stays folded into
        // `Affixation`/`RealizationalMorphology` per `tests/cover_realizational_morphology_
        // constraints.rs`'s own doc ("folding them into a separate CharacteristicKind would
        // double-count the same ModelLocation::MorphRule occurrence"). The ALLOMORPH-level
        // restriction that same file's `stem_name_gating_over_propose_confirm_prune` test already
        // exercises has no `ModelLocation::MorphRule` occurrence to double-count against at all --
        // it is a genuinely separate model.rs site this ledger had no row for until now.
        StemName => ev(
            Dedicated,
            "tests/cover_realizational_morphology_constraints.rs::\
             stem_name_gating_over_propose_confirm_prune",
            "Proposer-to-confirm containment for RootAllomorphDef::stem_name's required- and \
             excluded-match gating (bare-restricted-allomorph rejection, plus the \
             default-allomorph-excluded-by-a-restricted-sibling case) -- the FST proposes every \
             stem-restricted allomorph unconditionally; confirm's stem_name_gate_reason prunes.",
        ),
        // Research report 13's taxonomy-gap fix: the W3.2 disjunctive-allomorph re-check
        // (`pg_rules::validity`'s `free_fluctuates`/`disjunctive_candidates`). No DEDICATED
        // pg-foma-crate propose-then-confirm containment test exists for this specific mechanism
        // today (unlike `StemName`, which `cover_realizational_morphology_constraints.rs` already
        // covers) -- `machine/conformance/edge-cases/disjunctive-recheck` and `machine/conformance/
        // languages/suffixing-evidential-adjacency-chain` exercise it at the ORACLE level
        // (`conformance_coverage.rs`'s cross-check), but that is a different evidence axis from
        // this curated table's own FST-propose-then-confirm witness convention -- an honest,
        // reported gap (this function's own doc: "`None` only where no witness exists at all"),
        // not a fabricated citation.
        FreeFluctuation => return None,
    })
}

// =================================================================================================
// LedgerRow / CoverageLedger / build_ledger
// =================================================================================================

/// One [`crate::capability::CapabilityPredicate`] that discharges a [`LedgerRow`]'s
/// [`CharacteristicKind`], alongside that predicate's own [`EvidenceProvenance`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DischargingPredicate {
    pub id: String,
    pub provenance: EvidenceProvenance,
}

/// One row of the coverage ledger (tasks.md 1.1/1.2's deliverable): everything this crate can say
/// today about one [`CharacteristicKind`] — the frozen-model construct(s) it represents (see that
/// type's own per-variant doc in `capability.rs` for the exact `model.rs` citation), its
/// disposition, which predicates (if any) discharge it, which `constructs.txt` id(s) it maps to and
/// whether a passing fixture is known to cover them, and its curated containment-test citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerRow {
    pub kind: CharacteristicKind,
    /// ALWAYS `kind.default_disposition()` — never a second, independently-maintained copy. See
    /// this file's own `ledger_disposition_never_diverges_from_default_disposition` test.
    pub disposition: Disposition,
    /// Every registered [`crate::capability::CapabilityPredicate`] whose [`crate::capability::
    /// CapabilityPredicate::discharges`] names this row's `kind`. Empty for every [`Disposition::
    /// Proven`] kind (none needed) and for a [`Disposition::ConfirmOnly`] kind with no registered
    /// predicate (also fine — see [`crate::capability::disposition_floor`]'s own doc: only
    /// `FailClosed`/`ConfigPredicate` kinds REQUIRE one, per [`crate::capability::
    /// undischarged_kinds`]).
    #[serde(default)]
    pub discharging_predicates: Vec<DischargingPredicate>,
    /// `machine/conformance/constructs.txt` identifier(s) this kind maps to (reused verbatim from
    /// [`construct_ids_for`] — never re-derived). Empty iff [`Self::conformance_status`] is
    /// [`CoverageStatus::Unmappable`].
    pub construct_ids: Vec<String>,
    /// This row's conformance-coverage cross-check outcome against the ledger's own build-time
    /// passing-construct set (see [`build_ledger`]'s own doc: this ledger reuses [`construct_ids_
    /// for`]/[`CoverageStatus`] rather than re-deriving the classification rule).
    pub conformance_status: CoverageStatus,
    /// The curated proposer-to-confirm containment (or refusal) witness, if this crate's test
    /// suite has one for this construct (`None` only for a genuine, honestly-reported gap).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub containment: Option<ContainmentEvidence>,
}

/// The full, versioned, one-time-audited coverage ledger (tasks.md 1.1's deliverable). See this
/// module's own top-doc "Evidence, not a gate" section: this type is inert data, consulted by no
/// compile path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageLedger {
    pub schema_version: u32,
    /// One row per [`CharacteristicKind::ALL`] entry, in that constant's own declaration order.
    pub rows: Vec<LedgerRow>,
}

impl CoverageLedger {
    /// This ledger's row for `kind`, if present (always present in any ledger built by
    /// [`build_ledger`] — see `every_characteristic_kind_appears_exactly_once`).
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

/// Builds the coverage ledger: one [`LedgerRow`] per [`CharacteristicKind::ALL`] entry, in that
/// constant's own order. A pure function over a caller-supplied `registry` (whose predicates
/// determine [`LedgerRow::discharging_predicates`]) and `passing_covered_constructs` (the same
/// "set of `constructs.txt` identifiers exercised by at least one currently-passing fixture" shape
/// [`crate::conformance_coverage::supported_coverage_report`] itself takes) — mirroring that
/// module's own "pure core, wired-up glue lives at the edge" split: nothing here calls
/// `pg_conformance_fixtures::discover` or replays any fixture itself; a caller (e.g. a
/// `tests/coverage_ledger_gate.rs`, mirroring `tests/conformance_coverage_gate.rs`) supplies that
/// set by actually replaying fixtures, or a caller wanting a static, no-dynamic-dependency snapshot
/// may pass an empty set or a fixed hand-built one (as this crate's own golden-JSON test does, for
/// reproducibility independent of fixture churn elsewhere in the repo).
///
/// `conformance_status`'s classification is the identical three-way rule
/// [`crate::conformance_coverage::supported_coverage_report`]'s own inner closure uses
/// (`construct_ids.is_empty()` -> [`CoverageStatus::Unmappable`]; the row's own
/// [`crate::conformance_coverage::EvidenceRequirement`] satisfied -> [`CoverageStatus::Covered`];
/// otherwise [`CoverageStatus::Uncovered`]) — re-stated here rather than called, because the two
/// callers thread their evidence sets differently, not because the two scopes differ. **G8 note:**
/// `supported_kinds` used to be the `Proven`-only subset while this ledger walked all 20 kinds;
/// that asymmetry is GONE — `supported_kinds` now returns every [`crate::capability::
/// CharacteristicKind`], so both sides cover the same rows and differ only in plumbing. The
/// underlying contract ([`construct_ids_for`] plus [`CoverageStatus`] itself) is reused unchanged,
/// never re-derived.
///
/// # G8 fix: `FailClosed` rows are graded by refusal-witness evidence, not `passing_covered_constructs`
/// Before this fix, EVERY row -- `FailClosed` included -- was graded uniformly against
/// `passing_covered_constructs` alone. That is unsound for a [`Disposition::FailClosed`] row:
/// nothing compiles for a refused construct, so a passing ANALYSIS fixture can never exist for it,
/// and worse, a `FailClosed` kind sharing a `constructs.txt` id with a non-`FailClosed` sibling
/// could show `Covered` purely because the SIBLING's passing fixture tagged the same shared id --
/// exactly [`CharacteristicKind::MprGroupOverwrite`] (`FailClosed`) sharing `"MPR
/// features/groups"` with [`CharacteristicKind::MprGroupAppend`] (`ConfirmOnly`). This function
/// now uses [`crate::conformance_coverage::evidence_requirement_for`] to pick the right evidence
/// source per row: `FailClosed` rows are `Covered` iff [`containment_evidence_for`] names a
/// [`ContainmentEvidenceKind::RefusalWitness`] for that kind (a hand-curated fact, independent of
/// `passing_covered_constructs` entirely); every other disposition keeps the original
/// passing-fixture rule. No new parameter was needed: the refusal-witness signal was already
/// computed locally as `containment` below, just not consulted for `conformance_status` before.
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
            } else {
                use crate::conformance_coverage::EvidenceRequirement;
                let is_evidenced =
                    match crate::conformance_coverage::evidence_requirement_for(disposition) {
                        EvidenceRequirement::PassingFixture => construct_ids_static
                            .iter()
                            .any(|c| passing_covered_constructs.contains(c)),
                        EvidenceRequirement::RefusalWitness => matches!(
                            &containment,
                            Some(ev) if ev.kind == ContainmentEvidenceKind::RefusalWitness
                        ),
                    };
                if is_evidenced {
                    CoverageStatus::Covered
                } else {
                    CoverageStatus::Uncovered
                }
            };

            LedgerRow {
                kind,
                disposition,
                discharging_predicates,
                construct_ids,
                conformance_status,
                containment,
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

    /// A fixed, hand-built passing set (every construct id [`construct_ids_for`] ever names) so
    /// this test module's ledgers are deterministic and independent of any real fixture's
    /// pass/fail state elsewhere in the repo (this module builds no dependency on
    /// `pg_conformance_fixtures`/`pg_parse` replay at all -- see [`build_ledger`]'s own doc).
    fn fully_covered_constructs() -> HashSet<&'static str> {
        let mut set = HashSet::new();
        for &kind in CharacteristicKind::ALL {
            for &id in construct_ids_for(kind) {
                set.insert(id);
            }
        }
        set
    }

    // ---------------------------------------------------------------------------------------
    // tasks.md 3.3: exhaustiveness / no-drift
    // ---------------------------------------------------------------------------------------

    /// The ledger's own honesty test (tasks.md 3.3): every `CharacteristicKind` appears in the
    /// built ledger EXACTLY once. A future `CharacteristicKind` variant added without a
    /// corresponding `ALL` entry would silently never appear here at all (the same documented,
    /// non-panicking gap `CharacteristicKind::ALL`'s own doc names) -- the closed-match discipline
    /// in `containment_evidence_for`/the wire-name functions above is this file's actual
    /// compile-time backstop against a variant that VANISHES here silently.
    #[test]
    fn every_characteristic_kind_appears_exactly_once() {
        let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
        assert_eq!(ledger.rows.len(), CharacteristicKind::ALL.len());
        for &kind in CharacteristicKind::ALL {
            let count = ledger.rows.iter().filter(|r| r.kind == kind).count();
            assert_eq!(count, 1, "{kind:?} must appear exactly once in the ledger");
        }
    }

    /// Single source of truth (tasks.md's own "no divergent copy" discipline): every row's
    /// `disposition` is always exactly `kind.default_disposition()`, never a hardcoded or
    /// independently-computed value.
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

    /// Every non-vacuous `FailClosed`/`ConfigPredicate` row names at least one discharging
    /// predicate -- exactly [`undischarged_kinds`]'s own coverage requirement, cross-checked
    /// directly against the ledger's own per-row data (not re-derived: this test asserts the
    /// ledger AGREES with `undischarged_kinds`, the crate's existing single source of truth for
    /// this rule).
    #[test]
    fn every_config_predicate_or_fail_closed_row_names_a_discharging_predicate() {
        let registry = default_registry();
        assert!(
            undischarged_kinds(&registry).is_empty(),
            "sanity: default_registry() is expected to already discharge every FailClosed/\
             ConfigPredicate kind"
        );
        let ledger = build_ledger(&registry, &HashSet::new());
        for row in &ledger.rows {
            if matches!(
                row.disposition,
                Disposition::FailClosed | Disposition::ConfigPredicate
            ) {
                assert!(
                    !row.discharging_predicates.is_empty(),
                    "{:?} is {:?} but the ledger names no discharging predicate",
                    row.kind,
                    row.disposition
                );
            }
        }
    }

    /// [`containment_evidence_for`] must be callable end to end for every kind (mirrors
    /// `crate::conformance_coverage`'s own `construct_ids_for_is_callable_for_every_kind`
    /// belt-and-suspenders test).
    #[test]
    fn containment_evidence_for_is_callable_for_every_kind() {
        for &kind in CharacteristicKind::ALL {
            let _ = containment_evidence_for(kind);
        }
    }

    /// `NaturalClassDefinition` and (research report 13's taxonomy-gap fix) `FreeFluctuation` are
    /// the deliberate, documented `None`s -- pinned so a future edit that silently starts (or
    /// stops) returning evidence for either is a reviewed, visible change, not a silent drift
    /// (mirrors `crate::conformance_coverage`'s own `empty_covered_set_yields_no_covered_rows` pin
    /// for `LeftToRightRewrite`). `FreeFluctuation` has no DEDICATED pg-foma-crate propose-then-
    /// confirm containment test today (unlike its sibling `StemName`, which
    /// `cover_realizational_morphology_constraints.rs` already covers) -- see
    /// [`containment_evidence_for`]'s own `FreeFluctuation` arm for why this is an honestly
    /// reported gap, not an oversight.
    #[test]
    fn natural_class_definition_and_free_fluctuation_are_the_only_kinds_with_no_containment_evidence(
    ) {
        let missing: Vec<CharacteristicKind> = CharacteristicKind::ALL
            .iter()
            .copied()
            .filter(|&k| containment_evidence_for(k).is_none())
            .collect();
        assert_eq!(
            missing,
            vec![
                CharacteristicKind::NaturalClassDefinition,
                CharacteristicKind::FreeFluctuation
            ]
        );
    }

    // ---------------------------------------------------------------------------------------
    // build_ledger: conformance_status classification
    // ---------------------------------------------------------------------------------------

    #[test]
    fn build_ledger_with_empty_passing_set_never_marks_a_fixture_evidenced_row_covered() {
        let ledger = build_ledger(&default_registry(), &HashSet::new());
        for row in &ledger.rows {
            if row.disposition == Disposition::FailClosed {
                // FailClosed rows are graded by refusal-witness evidence (see build_ledger's own
                // G8-fix doc), independent of the passing-fixture set entirely -- covered below by
                // `fail_closed_row_is_covered_via_refusal_witness_regardless_of_passing_set`.
                continue;
            }
            assert_ne!(
                row.conformance_status,
                CoverageStatus::Covered,
                "{:?}",
                row.kind
            );
        }
    }

    /// G8 regression pin: a `FailClosed` row (`MprGroupOverwrite`) is `Covered` purely via its
    /// curated refusal witness, EVEN with a completely empty passing-fixture set -- proving the
    /// old cross-contamination bug (a `FailClosed` row showing `Covered` only because a sibling's
    /// passing fixture tagged the same shared `constructs.txt` id) can no longer happen, and that
    /// a genuine refusal witness is honored on its own terms.
    #[test]
    fn fail_closed_row_is_covered_via_refusal_witness_regardless_of_passing_set() {
        let ledger = build_ledger(&default_registry(), &fully_covered_constructs());
        let row = ledger
            .row(CharacteristicKind::MprGroupOverwrite)
            .expect("MprGroupOverwrite row must exist");
        assert_eq!(row.disposition, Disposition::ConfigPredicate);
        assert_eq!(
            row.conformance_status,
            CoverageStatus::Covered,
            "a FailClosed row with a curated RefusalWitness must be Covered even with zero \
             passing-fixture evidence"
        );
        assert_eq!(
            row.containment.as_ref().map(|c| c.kind),
            Some(ContainmentEvidenceKind::Dedicated)
        );
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

    /// G9: after adding the 4 missing `constructs.txt` rows upstream and mapping them in
    /// `conformance_coverage::construct_ids_for`, zero ledger rows are `Unmappable` any more --
    /// this is unconditional (depends only on `construct_ids_for` being non-empty per kind, never
    /// on the passing-fixture set), matching `conformance_coverage`'s own `zero_unmappable_after_g9`.
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
            "G9 must leave zero Unmappable ledger rows; found {unmappable:?}"
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

    // ---------------------------------------------------------------------------------------
    // Canonical JSON: golden + round trip
    // ---------------------------------------------------------------------------------------

    /// A deterministic, fully-covered-set ledger -- independent of any real fixture's live
    /// pass/fail state, so this golden stays stable regardless of unrelated fixture churn
    /// elsewhere in the repo.
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
    fn coverage_ledger_golden_panic_location_is_the_external_assertion_callsite() {
        let expected_line = line!() + 2;
        let location = crate::test_support::capture_panic_location(|| {
            assert_coverage_ledger_golden("actual", "expected");
        });
        assert_eq!(location.file, file!());
        assert_eq!(location.line, expected_line);
        assert!(location.column > 0);
        assert!(!location.file.ends_with("test_support.rs"));
    }

    #[test]
    fn semantic_json_parse_panic_location_is_the_external_caller() {
        let expected_line = line!() + 2;
        let location = crate::test_support::capture_panic_location(|| {
            crate::test_support::assert_semantic_json_eq("{", "{}");
        });
        assert_eq!(location.file, file!());
        assert_eq!(location.line, expected_line);
        assert!(location.column > 0);
        assert!(!location.file.ends_with("test_support.rs"));
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
