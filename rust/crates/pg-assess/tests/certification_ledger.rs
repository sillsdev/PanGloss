use pg_assess::certification::{
    CaseEvidence, CaseStatus, CertificationLedger, DenominatorError, EvidenceError,
    ThreeLanguageDenominatorGate,
};
use pg_assess::{AnalysisIdentity, AnalysisSet};

fn id(key: &str) -> AnalysisIdentity {
    AnalysisIdentity {
        morphemes: vec![Some(key.into())],
        root_index: 0,
        category: Some("v".into()),
    }
}

fn complete(case_id: &str, line: usize, key: &str) -> CaseEvidence {
    CaseEvidence::complete(
        case_id,
        line,
        key,
        AnalysisSet::from_observed([id(key)]),
        AnalysisSet::from_observed([id(key)]),
    )
}

fn ledger(language: &str, count: usize) -> CertificationLedger {
    let cases = (0..count)
        .map(|index| complete(&format!("{language}-{index}"), index + 1, &format!("m{index}")))
        .collect();
    CertificationLedger::new(language, format!("{language}-v1"), cases)
        .expect("synthetic ledger is valid")
}

fn expected(entries: &[(&str, usize)]) -> Vec<(String, usize)> {
    entries
        .iter()
        .map(|(language, count)| ((*language).to_string(), *count))
        .collect()
}

#[test]
fn complete_empty_is_valid_but_every_noncomplete_status_is_noncertifying() {
    let empty = CaseEvidence::complete(
        "empty",
        1,
        "",
        AnalysisSet::from_observed([]),
        AnalysisSet::from_observed([]),
    );
    assert!(empty.is_complete());
    assert!(empty.exact_match());

    for status in [
        CaseStatus::LogicalBudget {
            dimension: "steps".into(),
            value: 10,
            limit: 9,
        },
        CaseStatus::WallClockTimeout {
            elapsed_us: 10,
            limit_us: 9,
        },
        CaseStatus::InvalidShape { side: "oracle".into() },
        CaseStatus::NotAttempted {
            reason: "setup".into(),
        },
        CaseStatus::CandidateBudget {
            dimension: "paths".into(),
            value: 10,
            limit: 9,
        },
        CaseStatus::IdentityProjection {
            side: "candidate".into(),
            reason: "guessed".into(),
        },
        CaseStatus::SetupFailure {
            reason: "compiler".into(),
        },
    ] {
        let evidence = CaseEvidence::noncomplete("case", 1, "x", status)
            .expect("non-complete status is valid");
        assert!(!evidence.is_complete());
        assert!(!evidence.exact_match());
    }
}

#[test]
fn noncomplete_constructor_rejects_complete_status_in_all_builds() {
    assert!(matches!(
        CaseEvidence::noncomplete("case", 1, "x", CaseStatus::Complete),
        Err(EvidenceError::CompleteStatus)
    ));
}

#[test]
fn duplicate_paths_do_not_change_exact_set_certification() {
    let oracle = AnalysisSet::from_observed([id("root"), id("root")]);
    let candidate = AnalysisSet::from_observed([id("root")]);
    let evidence = CaseEvidence::complete("case", 1, "word", oracle, candidate);
    assert!(evidence.exact_match());
}

#[test]
fn canonical_ledger_reconciles_denominator_and_rejects_mismatch() {
    let mut cases = vec![complete("a", 1, "a"), complete("b", 2, "b")];
    cases.push(
        CaseEvidence::noncomplete(
            "c",
            3,
            "c",
            CaseStatus::WallClockTimeout {
                elapsed_us: 20,
                limit_us: 10,
            },
        )
        .expect("timeout is a non-complete status"),
    );
    let ledger = CertificationLedger::new("indonesian", "ind-v1", cases)
        .expect("ledger shape is valid");
    let summary = ledger.reconcile();
    assert_eq!(summary.declared, 3);
    assert_eq!(summary.complete, 2);
    assert_eq!(summary.timeouts, 1);
    assert_eq!(summary.exact, 2);
    assert!(!ledger.can_certify());
    assert!(ledger.canonical_json().unwrap().contains("wallClockTimeout"));

    let malformed = CertificationLedger::new(
        "indonesian",
        "ind-v1",
        vec![complete("b", 2, "b"), complete("a", 1, "a")],
    );
    assert!(matches!(malformed, Err(DenominatorError::UnstableCaseOrder)));
}

#[test]
fn three_language_gate_requires_exact_declared_denominators_and_all_exact_cases() {
    let gate = ThreeLanguageDenominatorGate::new_with_expected(
        [
            ledger("indonesian", 2),
            ledger("amharic", 2),
            ledger("aweti", 2),
        ],
        expected(&[("indonesian", 2), ("amharic", 2), ("aweti", 2)]),
    )
    .expect("synthetic three-language report");
    assert!(gate.can_certify());
    assert_eq!(gate.reconcile().total_declared, 6);
    let report = gate.canonical_value();
    let language_order = report["languages"]
        .as_array()
        .expect("canonical languages array")
        .iter()
        .map(|ledger| ledger["language"].as_str().expect("language"))
        .collect::<Vec<_>>();
    assert_eq!(language_order, ["indonesian", "amharic", "aweti"]);
    let expected_order = report["expectedDenominators"]
        .as_array()
        .expect("canonical expected-denominator array")
        .iter()
        .map(|entry| entry["language"].as_str().expect("language"))
        .collect::<Vec<_>>();
    assert_eq!(expected_order, ["indonesian", "amharic", "aweti"]);

    let wrong = ThreeLanguageDenominatorGate::new_with_expected(
        [
            ledger("indonesian", 2),
            ledger("amharic", 1),
            ledger("aweti", 2),
        ],
        expected(&[("indonesian", 2), ("amharic", 2), ("aweti", 2)]),
    )
    .expect("shape is still valid");
    assert!(!wrong.can_certify());
    assert_eq!(wrong.reconcile().noncanonical_language_count, 1);
}

#[test]
fn gate_refuses_any_language_set_other_than_the_three_canonical_languages() {
    let canonical_expected = || {
        expected(&[("indonesian", 1), ("amharic", 1), ("aweti", 1)])
    };
    for expected_set in [
        expected(&[("indonesian", 1)]),
        expected(&[("indonesian", 1), ("amharic", 1)]),
        expected(&[
            ("indonesian", 1),
            ("amharic", 1),
            ("aweti", 1),
            ("spanish", 1),
        ]),
        expected(&[("spanish", 1), ("french", 1), ("latin", 1)]),
    ] {
        let ledgers = [
            ledger("indonesian", 1),
            ledger("amharic", 1),
            ledger("aweti", 1),
        ];
        assert!(
            ThreeLanguageDenominatorGate::new_with_expected(ledgers.clone(), expected_set)
                .is_err(),
            "non-canonical expected language sets must be rejected"
        );
    }

    for ledgers in [
        vec![ledger("indonesian", 1)],
        vec![ledger("indonesian", 1), ledger("amharic", 1)],
        vec![
            ledger("indonesian", 1),
            ledger("amharic", 1),
            ledger("aweti", 1),
            ledger("spanish", 1),
        ],
        vec![ledger("spanish", 1), ledger("french", 1), ledger("latin", 1)],
    ] {
        assert!(
            ThreeLanguageDenominatorGate::new_with_expected(ledgers, canonical_expected())
                .is_err(),
            "non-canonical ledger language sets must be rejected"
        );
    }
}
