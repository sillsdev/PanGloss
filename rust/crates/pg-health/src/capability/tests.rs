use super::*;
use serde_json::json;

#[test]
fn proposal_recall_cause_wire_names_are_stable() {
    for (gap, name) in [
        (ProposalRecallGap::TemplateReentry, "template_reentry"),
        (
            ProposalRecallGap::PhonologicalReduplication,
            "phonological_reduplication",
        ),
        (
            ProposalRecallGap::AmbiguousSegmentSpelling,
            "ambiguous_segment_spelling",
        ),
        (
            ProposalRecallGap::TruncationComposition,
            "truncation_composition",
        ),
        (
            ProposalRecallGap::NarrowingClassOutput,
            "narrowing_class_output",
        ),
        (ProposalRecallGap::SymbolicEpenthesis, "symbolic_epenthesis"),
        (
            ProposalRecallGap::RecursiveCompoundDepth,
            "recursive_compound_depth",
        ),
        (
            ProposalRecallGap::RecursiveCompoundSpelling,
            "recursive_compound_spelling",
        ),
        (
            ProposalRecallGap::CompoundBoundarySpelling,
            "compound_boundary_spelling",
        ),
        (
            ProposalRecallGap::CompoundJunctionTokenization,
            "compound_junction_tokenization",
        ),
        (
            ProposalRecallGap::SingletonRootBoundary,
            "singleton_root_boundary",
        ),
        (
            ProposalRecallGap::CompoundJunctionPhonology,
            "compound_junction_phonology",
        ),
        (
            ProposalRecallGap::RecursiveCompoundAsymmetricLicense,
            "recursive_compound_asymmetric_license",
        ),
        (
            ProposalRecallGap::RecursiveCompoundMprRestriction,
            "recursive_compound_mpr_restriction",
        ),
        (
            ProposalRecallGap::RecursiveCompoundMprOutput,
            "recursive_compound_mpr_output",
        ),
        (
            ProposalRecallGap::RecursiveCompoundAffixPlacement,
            "recursive_compound_affix_placement",
        ),
    ] {
        let cause = CapabilityRefusal::ProposalRecall(gap);
        let expected = json!({"kind": "proposal_recall", "details": name});
        assert_eq!(serde_json::to_value(cause).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<CapabilityRefusal>(expected).unwrap(),
            cause
        );
        assert_eq!(serde_json::to_value(gap).unwrap(), name);
    }
}

#[test]
fn other_capability_cause_payloads_round_trip_without_diagnostic_text() {
    for (cause, expected) in [
        (
            CapabilityRefusal::AlphaEnvironment,
            json!({"kind": "alpha_environment"}),
        ),
        (
            CapabilityRefusal::CircumfixNoRoute { rule: None },
            json!({"kind": "circumfix_no_route", "details": {"rule": null}}),
        ),
        (
            CapabilityRefusal::CircumfixNoRoute { rule: Some(3) },
            json!({"kind": "circumfix_no_route", "details": {"rule": 3}}),
        ),
        (
            CapabilityRefusal::ReduplicationNoRoute { rule: 4 },
            json!({"kind": "reduplication_no_route", "details": {"rule": 4}}),
        ),
        (
            CapabilityRefusal::PatternRootShape { entry: 5 },
            json!({"kind": "pattern_root_shape", "details": {"entry": 5}}),
        ),
        (
            CapabilityRefusal::RootSpellingBudget {
                entry: 6,
                bytes: 13,
                limit: 9,
            },
            json!({"kind": "root_spelling_budget", "details": {"entry": 6, "bytes": 13, "limit": 9}}),
        ),
        (
            CapabilityRefusal::StrategyCoverage,
            json!({"kind": "strategy_coverage"}),
        ),
    ] {
        assert_eq!(serde_json::to_value(cause).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<CapabilityRefusal>(expected).unwrap(),
            cause
        );
    }
    assert!(serde_json::from_value::<CapabilityRefusal>(json!({"kind": "unknown"})).is_err());
    assert!(serde_json::from_value::<CapabilityRefusal>(
        json!({"kind": "proposal_recall", "details": "unknown"})
    )
    .is_err());
}
