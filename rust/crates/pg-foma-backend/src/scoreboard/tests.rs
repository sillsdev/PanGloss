use super::*;

#[test]
fn compile_reason_is_none_for_full_hc_confirmed() {
    assert!(compile_reason(&Certification::FullHcConfirmed {
        words: 1,
        corpus_hash: "x".to_string()
    })
    .is_none());
}

#[test]
fn compile_reason_names_every_refusal_variant() {
    for cert in [
        Certification::CapabilityRejected {
            reason: "r".to_string(),
        },
        Certification::BuildFailed {
            reason: "r".to_string(),
        },
        Certification::Unsupported {
            reason: "r".to_string(),
        },
    ] {
        assert_eq!(compile_reason(&cert), Some("r".to_string()));
    }
}

fn cell(outcome: CellOutcome) -> CellMeasurement {
    CellMeasurement {
        strategy: EmissionStrategy::PlanComposed,
        outcome,
        certification_debug: "n/a".to_string(),
        divergence: None,
        legal_overgeneration: None,
        words_measured: None,
    }
}

#[test]
fn cell_compiled_and_exact_read_from_the_outcome_alone() {
    let refused = cell(CellOutcome::Refused {
        reason: "r".to_string(),
        predicates: Vec::new(),
    });
    assert!(!refused.compiled());
    assert!(!refused.exact());

    let exact = cell(CellOutcome::OracleExact);
    assert!(exact.compiled());
    assert!(exact.exact());
}

#[test]
fn unmeasurable_fills_every_strategy_with_the_same_reason() {
    let row = unmeasurable("some-label", "no grammar to measure");
    assert_eq!(row.exact_count, None);
    assert_eq!(row.cells.len(), ALL_STRATEGIES.len());
    for cell in &row.cells {
        assert_eq!(
            cell.outcome,
            CellOutcome::Unmeasurable {
                reason: "no grammar to measure".to_string()
            }
        );
    }
}
