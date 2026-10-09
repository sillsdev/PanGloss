//! Versioned fixture and containment evidence for construct variant obligations.

use std::collections::HashSet;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::capability::{
    CharacteristicKind, ConstructVariant, Disposition, EvidenceProvenance, PredicateRegistry,
    VariantDisposition,
};
use crate::conformance_coverage::{construct_ids_for, coverage_status_for_ids, CoverageStatus};
use crate::enumerate::EmissionStrategy;
use crate::strategy_coverage::{representation_of, strategies_that_represent};

/// This schema's own version (mirrors `crate::health::HEALTH_SCHEMA_VERSION`'s convention).
pub const COVERAGE_LEDGER_SCHEMA_VERSION: u32 = 2;

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

/// Curated evidence with the compiler strategies actually exercised by its citation.
/// A strategy with no witness stays explicitly unwitnessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainmentEvidence {
    pub kind: ContainmentEvidenceKind,
    /// A crate-qualified `tests/<file>.rs` path plus the specific `#[test]` function name(s), e.g.
    /// `"pg-foma-backend/tests/cover_compounding.rs::head_a_word_over_propose_confirm_prune"`.
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
            "pg-foma/tests/f1_large_lexicon_gate.rs, pg-foma/tests/f2_junction_gate.rs, pg-foma/tests/f4_composite_gate.rs",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Ordinary AffixProcessRule prefixation/suffixation/infixation is the baseline every \
             full-grammar propose-confirm gate exercises continuously; Proven already licenses \
             unconditional admission-filtering, so no separate dedicated fixture is required.",
        ),
        RealizationalMorphology => ev(
            Dedicated,
            "pg-foma-backend/tests/cover_realizational_morphology_constraints.rs::\
             realizational_rule_presence_blocking_over_propose_confirm_prune",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Proposer-to-confirm containment for MorphRuleDef::Realizational's real_fs \
             head-wrapped presence-blocking.",
        ),
        Compounding => ev(
            Dedicated,
            "pg-foma-backend/tests/cover_compounding.rs::head_a_word_over_propose_confirm_prune (+ \
             subrule_group_gate_excludes_partial_match_like_confirm, \
             head_c_excluded_by_rule_level_gate_like_confirm)",
            &[EmissionStrategy::TunedSurfaceProbed],
            "License-gated head/non-head cross-product containment for the non-recursive case, \
             plus rule-level and subrule-level group-awareness witnesses.",
        ),
        OrderedMorphRuleApplication => ev(
            GeneralPervasive,
            "pg-foma/tests/phase_c_strata_depth.rs (multi-stratum cascade recall-parity), pg-foma/tests/f1_large_lexicon_gate.rs, pg-foma/tests/f4_composite_gate.rs",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Linear rule-application order is the default cascade shape exercised by every \
             general gate; Proven, no dedicated fixture required.",
        ),
        UnorderedMorphRuleApplication => ev(
            Dedicated,
            "pg-foma-backend/tests/cover_unordered_morph_rules.rs::non_document_order_analysis_is_proposed_and_\
             confirmed",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Chain-depth-bounded any-order proposal containment.",
        ),
        MprGroupAppend => ev(
            Dedicated,
            "pg-foma-backend/tests/cover_mpr_groups.rs::out_mpr_accumulation_then_gate_over_propose_confirm_prune \
             (+ append_output_is_order_invariant_overwrite_output_is_not)",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Non-tracking-baseline containment for MprGroupOutput::Append, plus the \
             order-invariance witness.",
        ),
        MprGroupOverwrite => ev(
            Dedicated,
            "pg-foma-backend/tests/cover_mpr_groups.rs::overwrite_group_composes_to_confirm_only",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Non-narrowing-baseline containment for MprGroupOutput::Overwrite: this witness proves \
             compose_envelope resolves an observed Overwrite group to ConfirmOnly, never Admit.",
        ),
        IterativeRewrite => ev(
            GeneralPervasive,
            "pg-foma/tests/f1_large_lexicon_gate.rs, pg-foma/tests/f2_junction_gate.rs, pg-foma/tests/phase_c_right_to_left.rs \
             (iterative baseline contrast)",
            &[EmissionStrategy::PlanComposed, EmissionStrategy::TunedSurfaceProbed],
            "The default RewriteMode every general gate's phonological rules use; Proven, no \
             dedicated fixture required.",
        ),
        SimultaneousRewrite => ev(
            Dedicated,
            "pg-foma/tests/phase_c_simultaneous.rs::sim_nonoverlap_env_now_compiles_and_matches_oracle_\
             exactly (+ sim_overlap_env_stays_honest_unsupported for the Refuse split)",
            &[EmissionStrategy::PlanComposed],
            "Containment for the pairwise-non-overlapping case the simultaneous.subrule-overlap \
             predicate Admits; the genuinely-overlapping case stays honestly unsupported.",
        ),
        LeftToRightRewrite => ev(
            GeneralPervasive,
            "pg-foma/tests/f2_junction_gate.rs, pg-foma/tests/phase_c_right_to_left.rs (LTR is the implicit \
             contrast baseline for every rtl_* case)",
            &[EmissionStrategy::PlanComposed, EmissionStrategy::TunedSurfaceProbed],
            "The default Dir every general gate's phonological rules use; Proven, no dedicated \
             fixture required.",
        ),
        RightToLeftRewrite => ev(
            Dedicated,
            "pg-foma/tests/phase_c_right_to_left.rs::rtl_plain_rule_now_compiles_and_matches_oracle (+ \
             rtl_feature_environment_swap_matches_oracle, rtl_deletion_matches_oracle, \
             rtl_cross_table_segments_environment_matches_oracle; \
             pg-foma-backend/tests/repeated_alpha_containment.rs::\
             repeated_alpha_propose_confirm_matches_recorded_oracle (+ \
             templated_repeated_alpha_matches_recorded_oracle); \
             pg-foma-backend/tests/ambiguous_alpha_containment.rs::\
             ambiguous_disagreement_propose_confirm_matches_recorded_oracle (+ \
             templated_ambiguous_disagreement_matches_recorded_oracle, mixed_agreeing_class_matches_recorded_oracle_after_confirmation, \
               repeated_minus_with_partial_focus_matches_recorded_oracle_after_confirmation, \
               overwritten_alpha_matches_recorded_oracle_after_confirmation))",
            &[EmissionStrategy::PlanComposed, EmissionStrategy::TemplatedUnderlyingTokens],
            "Reversal-plus-safety-net-union containment against the real oracle, including a \
             table-qualified cross-table Segments constraint, repeated alpha environments and ambiguous disagreement.",
        ),
        Metathesis => ev(
            Dedicated,
            "pg-foma/tests/phase_c_metathesis.rs::metathesis_adjacent_singleton_swap_matches_oracle_\
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
            "pg-foma-backend/tests/epenthesis_structural_route_containment.rs::\
             epenthesis_over_propose_confirm_prune_matches_oracle_exactly",
            &[EmissionStrategy::TunedSurfaceProbed],
            "End-to-end propose(over-generate)-then-confirm(prune) containment for an \
             obligatory-epenthesis grammar, matching the oracle's analysis set exactly.",
        ),
        SubruleGating => ev(
            Dedicated,
            "pg-foma/tests/p6_gate_parity.rs::synthetic_pos_gate_matches_oracle (+ \
             ungated_cascade_would_have_missed_the_noun_entry); scale: pg-foma/tests/phase_c_partition_k.rs::partition_k_recall_parity_via_generator_and_oracle",
            &[EmissionStrategy::PlanComposed],
            "Static MPR/POS subrule-gating containment against the real oracle, plus a \
             2^k-group scale gate.",
        ),
        CircumfixOutputAction => ev(
            Dedicated,
            "pg-foma/tests/phase_c_circumfix.rs::circumfix_recall_parity_via_generator_and_oracle (+ \
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
            "pg-foma-backend/tests/f6_reduplication_peel_chain_depth.rs::\
             kimbiakimbia_reduplication_is_recovered_with_oracle_containment (+ \
             deep_self_similar_chain_is_refused_deterministically for the chain-depth budget); \
             pg-foma/tests/f4_composite_gate.rs case (c)",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Peeler-to-confirm containment for true-reduplication allomorphs, plus the \
             deterministic deep-chain refusal witness.",
        ),
        CoOccurrenceConstraint => ev(
            Dedicated,
            "pg-foma-backend/tests/cover_realizational_morphology_constraints.rs::\
             morpheme_co_occurrence_exclude_anywhere_over_propose_confirm_prune",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Proposer-to-confirm containment for MorphemeCoOccurrenceRule adjacency exclusion.",
        ),
        NaturalClassDefinition => return None,
        MultiTable => ev(
            Dedicated,
            "pg-foma/tests/phase_c_multi_table.rs::\
             multi_table_rewrite_compiles_correctly_against_its_owning_table; stronger claim: \
             pg-foma-backend/tests/two_table_symbol_divergence.rs::\
             stratum_1_devoice_rewrite_proposer_confirm_matches_oracle",
            &[EmissionStrategy::PlanComposed],
            "Faithful per-stratum table threading, proven for one stratum's own rule and, more \
             strongly, for two strata whose tables disagree about the same symbol index.",
        ),
        CrossTableRespelling => ev(
            Dedicated,
            "pg-foma-backend/tests/cross_table_root_respelling_gate.rs::\
             every_respelling_fixture_is_oracle_exact_on_tsp_and_typed_elsewhere",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Propose-then-confirm containment for a root entered on an inner stratum and spelled \
             by the final stratum's table, over every conformance fixture that exhibits it; the \
             plan-composed backend is exact where it builds at all and the templated one refuses \
             typed rather than misses.",
        ),
        QuantifierPattern => ev(
            Dedicated,
            "pg-foma/tests/phase_c_quantifier.rs::quantifier_bounded_environment_compiles_and_matches_\
             oracle (+ quantifier_unbounded_environment_compiles_and_matches_oracle for the \
             genuinely-unbounded case); \
             pg-foma-backend/tests/repeated_alpha_containment.rs::\
             repeated_alpha_propose_confirm_matches_recorded_oracle (+ \
             templated_repeated_alpha_matches_recorded_oracle)",
            &[EmissionStrategy::PlanComposed, EmissionStrategy::TemplatedUnderlyingTokens],
            "Bounded- AND unbounded-quantifier containment against the real oracle, both at \
             min-boundary and zero counts, including repeated alpha agreement on both environment \
             sides and in both directions. Exact target lowering keeps its separate scope.",
        ),
        // `RootAllomorphDef::stem_name`, not `MorphRuleDef::required_stem_name` (folded into Affixation/RealizationalMorphology to avoid double-counting the same ModelLocation::MorphRule occurrence).
        StemName => ev(
            Dedicated,
            "pg-foma-backend/tests/cover_realizational_morphology_constraints.rs::\
             stem_name_gating_over_propose_confirm_prune",
            &[EmissionStrategy::TunedSurfaceProbed],
            "Proposer-to-confirm containment for RootAllomorphDef::stem_name's required- and \
             excluded-match gating (bare-restricted-allomorph rejection, plus the \
             default-allomorph-excluded-by-a-restricted-sibling case) -- the FST proposes every \
             stem-restricted allomorph unconditionally; confirm's stem_name_gate_reason prunes.",
        ),
        FreeFluctuation => ev(
            Dedicated,
            "pg-foma-backend/tests/free_fluctuation_containment.rs::disjunctive_recheck_proposes_and_confirms_free_fluctuating_allomorphs",
            &[EmissionStrategy::TunedSurfaceProbed],
            "TunedSurfaceProbed proposal and confirmation for the C#-recorded gray/grey free-\
             fluctuation pair, with disjunctive-rejection controls also checked against the same \
             oracle multiset.",
        ),
        ProcessMorphology => ev(
            Dedicated,
            "pg-foma-backend/tests/process_morphology_route_gate.rs::tsp_admits_and_certifies_the_pure_ablaut_rule",
            &[EmissionStrategy::TunedSurfaceProbed],
            "TunedSurfaceProbed's full-fixture proposal and confirmation for the pure ablaut \
             Process rule, certified oracle-exact with zero candidate-only identities.",
        ),
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

/// One reachable variant obligation, or the sole obligation of a kind without variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerRow {
    pub kind: CharacteristicKind,
    /// Stable concrete variant identity; absent only for a kind without predicate variants.
    pub variant: Option<String>,
    /// Concrete outcome from the owning predicate's variant inventory.
    pub variant_disposition: Option<VariantDisposition>,
    /// An explicit permanent refusal justification, when one has been ratified.
    pub permanent_refusal: Option<String>,
    /// The kind's default predicate requirement, distinct from this variant's concrete outcome.
    pub disposition: Disposition,
    /// Every registered `crate::capability::CapabilityPredicate` whose [`crate::capability::
    /// CapabilityPredicate::discharges`] names this row's `kind`. Empty for every [`Disposition::
    /// Proven`] kind (none needed) and for a `Disposition::ConfirmOnly` kind with no registered
    /// predicate (also fine — only `ConfigPredicate` kinds REQUIRE one, per
    /// `crate::capability::undischarged_kinds`).
    #[serde(default)]
    pub discharging_predicates: Vec<DischargingPredicate>,
    /// Fixture tags for this obligation: a concrete variant id, or generic ids for a kind without variants.
    pub construct_ids: Vec<String>,
    /// Coverage of this obligation's tags, classified by the shared coverage helper.
    pub conformance_status: CoverageStatus,
    /// The curated proposer-to-confirm containment witness, if this crate's test
    /// suite has one for this construct (`None` only for a genuine, honestly-reported gap).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub containment: Option<ContainmentEvidence>,
    /// Kind-level representation gaps, independent of this row's `variant_disposition`.
    /// Every `EmissionStrategy` whose proposer emits nothing at all for this kind
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

/// Versioned evidence data for coverage claims, independent of compile-time admission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageLedger {
    pub schema_version: u32,
    /// Kind order, then the owning predicates' declared variant order.
    pub rows: Vec<LedgerRow>,
}

impl CoverageLedger {
    /// Looks up one obligation by kind and concrete variant identity.
    pub fn row(&self, kind: CharacteristicKind, variant: Option<&str>) -> Option<&LedgerRow> {
        self.rows
            .iter()
            .find(|r| r.kind == kind && r.variant.as_deref() == variant)
    }

    /// Canonical machine-readable form (mirrors `crate::health::HealthReport::to_json` exactly:
    /// pretty-printed, two-space indent, Rust declaration field order).
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Parses a ledger from its canonical JSON form.
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        let ledger: Self = serde_json::from_str(json)?;
        if ledger.schema_version != COVERAGE_LEDGER_SCHEMA_VERSION {
            return Err(serde_json::Error::custom(format!(
                "unsupported coverage ledger schema version: {}",
                ledger.schema_version
            )));
        }
        Ok(ledger)
    }
}

/// Builds one evidence obligation per registered variant, retaining one row for other kinds.
/// Generic kind tags cannot satisfy a concrete variant's fixture obligation.
pub fn build_ledger(
    registry: &PredicateRegistry,
    passing_covered_constructs: &HashSet<&str>,
) -> CoverageLedger {
    let rows = CharacteristicKind::ALL
        .iter()
        .copied()
        .flat_map(|kind| {
            let variants: Vec<_> = registry
                .predicates()
                .iter()
                .filter(|p| p.discharges().contains(&kind))
                .flat_map(|p| p.variants().iter().copied())
                .collect();
            assert!(
                variants.iter().all(|v| v.kind() == kind),
                "predicate catalog names a variant of another kind: {kind:?}"
            );
            let unique: HashSet<_> = variants.iter().copied().collect();
            assert_eq!(
                unique.len(),
                variants.len(),
                "duplicate variant obligations for {kind:?}"
            );
            assert!(
                kind.default_disposition() != Disposition::ConfigPredicate || !variants.is_empty(),
                "coverage cannot enumerate variants for {kind:?}"
            );
            let obligations: Vec<Option<ConstructVariant>> = if variants.is_empty() {
                vec![None]
            } else {
                variants.into_iter().map(Some).collect()
            };
            obligations.into_iter().map(move |variant| {
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

                let ids: Vec<&str> = match variant {
                    Some(v) => vec![v.id()],
                    None => construct_ids_for(kind).to_vec(),
                };
                let conformance_status = coverage_status_for_ids(&ids, passing_covered_constructs);
                let construct_ids = ids.into_iter().map(str::to_string).collect();
                let containment = match variant {
                    Some(v) => containment_evidence_for_variant(v),
                    None => containment_evidence_for(kind),
                };

                // Both lists are derived from crate::strategy_coverage, never hand-maintained.
                let strategies_cannot_represent: Vec<String> =
                    crate::strategy_coverage::ALL_STRATEGIES
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
                    variant: variant.map(|v| v.id().to_string()),
                    variant_disposition: variant.map(ConstructVariant::disposition),
                    permanent_refusal: variant
                        .and_then(ConstructVariant::permanent_refusal_reason)
                        .map(str::to_string),
                    disposition,
                    discharging_predicates,
                    construct_ids,
                    conformance_status,
                    containment,
                    strategies_cannot_represent,
                    strategies_unwitnessed,
                }
            })
        })
        .collect();

    CoverageLedger {
        schema_version: COVERAGE_LEDGER_SCHEMA_VERSION,
        rows,
    }
}

/// Whether an obligation meets its disposition's evidence requirements.
/// Proven kinds require a passing fixture; concrete supported variants and ConfirmOnly kinds
/// also require containment. An unresolved Refuse stays open even if detection is witnessed.
pub fn obligation_met(row: &LedgerRow) -> bool {
    if row.variant_disposition == Some(VariantDisposition::Refuse) {
        return row
            .permanent_refusal
            .as_ref()
            .is_some_and(|reason| !reason.trim().is_empty());
    }
    row.conformance_status == CoverageStatus::Covered
        && (row.disposition == Disposition::Proven || row.containment.is_some())
}

/// Variant-specific citations never inherit a supported sibling's evidence for a refusal.
pub fn containment_evidence_for_variant(variant: ConstructVariant) -> Option<ContainmentEvidence> {
    use ConstructVariant::*;
    use ContainmentEvidenceKind::Dedicated;
    use EmissionStrategy::{PlanComposed, TemplatedUnderlyingTokens, TunedSurfaceProbed};
    let specific = match variant {
        CompoundingRecursive => Some((
            "pg-foma-backend/tests/cover_compounding_recursive_depth_bound.rs::depth_budgeted_compound_loop_contains_the_raised_cap_oracle_analysis",
            TunedSurfaceProbed,
        )),
        MprOverwrite => Some((
            "pg-foma-backend/tests/templated_conformance_proposal_pins.rs::mpr_overwrite_order_dependence_proposes_both_relative_orders",
            TemplatedUnderlyingTokens,
        )),
        MetathesisLtrSwap => Some((
            "pg-foma/tests/phase_c_metathesis.rs::metathesis_adjacent_singleton_swap_matches_oracle_exactly",
            PlanComposed,
        )),
        MetathesisRtlSwap => Some((
            "pg-foma/tests/phase_c_metathesis.rs::metathesis_right_to_left_reversal_matches_oracle_exactly",
            PlanComposed,
        )),
        QuantifierBounded => Some((
            "pg-foma/tests/phase_c_quantifier.rs::quantifier_bounded_environment_compiles_and_matches_oracle; \
             pg-foma-backend/tests/repeated_alpha_containment.rs::repeated_alpha_propose_confirm_matches_recorded_oracle (+ templated_repeated_alpha_matches_recorded_oracle); \
             pg-foma-backend/tests/ambiguous_alpha_containment.rs::ambiguous_disagreement_propose_confirm_matches_recorded_oracle (+ templated_ambiguous_disagreement_matches_recorded_oracle, mixed_agreeing_class_matches_recorded_oracle_after_confirmation, \
               repeated_minus_with_partial_focus_matches_recorded_oracle_after_confirmation, \
               overwritten_alpha_matches_recorded_oracle_after_confirmation)",
            PlanComposed,
        )),
        QuantifierUnbounded => Some((
            "pg-foma/tests/phase_c_quantifier.rs::quantifier_unbounded_environment_compiles_and_matches_oracle; \
             pg-foma-backend/tests/repeated_alpha_containment.rs::repeated_alpha_propose_confirm_matches_recorded_oracle (+ templated_repeated_alpha_matches_recorded_oracle); \
             pg-foma-backend/tests/ambiguous_alpha_containment.rs::ambiguous_disagreement_propose_confirm_matches_recorded_oracle (+ templated_ambiguous_disagreement_matches_recorded_oracle, mixed_agreeing_class_matches_recorded_oracle_after_confirmation, \
               repeated_minus_with_partial_focus_matches_recorded_oracle_after_confirmation, \
               overwritten_alpha_matches_recorded_oracle_after_confirmation)",
            PlanComposed,
        )),
        ReduplicationStructural => Some((
            "pg-foma-backend/tests/circumfix_candidate_selection.rs::circumfix_reduplication_recall_parity",
            TunedSurfaceProbed,
        )),
        MultiTableShared => Some((
            "pg-foma-backend/tests/two_table_shared_representation_recall.rs::fst_propose_confirm_matches_oracle_across_the_table_boundary",
            PlanComposed,
        )),
        SimultaneousUnproven | RtlUnlowerable | MetathesisLtrUnlowerable
        | MetathesisRtlUnlowerable | CircumfixUnrouted | ReduplicationUnrouted
        | QuantifierBoundedUnlowerable | QuantifierUnboundedUnlowerable => return None,
        CompoundingNonRecursive | UnorderedOrderUnion | SimultaneousDisjoint
        | RtlReversal | EpenthesisStructural | CircumfixStructural | ReduplicationPeel
        | MultiTableDisjoint => None,
    };
    match specific {
        Some((citation, strategy)) => {
            let strategies = match variant {
                QuantifierBounded | QuantifierUnbounded => {
                    vec![strategy, TemplatedUnderlyingTokens]
                }
                _ => vec![strategy],
            };
            Some(ev(Dedicated, citation, &strategies, variant.id()))
        }
        None => containment_evidence_for(variant.kind()),
    }
}

#[cfg(test)]
mod tests;
