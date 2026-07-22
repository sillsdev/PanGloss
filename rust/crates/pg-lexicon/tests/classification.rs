#[path = "toy_fixture.rs"]
mod fixture;

use pg_lexicon::{
    classify, ClassCatalog, ClassificationBudgets, ClassificationGuide, ClassificationRequest,
    Judgment, KnownFacts, TruncationReason,
};

fn setup() -> (pg_grammar::model::Grammar, ClassCatalog) {
    let grammar = pg_grammar::load(fixture::TOY_XML).unwrap();
    let catalog = ClassCatalog::from_grammar(&grammar).unwrap();
    (grammar, catalog)
}

#[test]
fn matrix_filters_known_facts_and_aggregates_real_rule_surfaces() {
    let (grammar, catalog) = setup();
    let matrix = classify(
        &grammar,
        &catalog,
        ClassificationRequest {
            stem: "sato".into(),
            known: KnownFacts {
                pos_id: Some("posN".into()),
                ..KnownFacts::default()
            },
            budgets: ClassificationBudgets::default(),
        },
    )
    .unwrap();
    assert_eq!(matrix.candidates.len(), 2);
    assert!(matrix.exhaustive);
    assert_eq!(matrix.truncation_reason, None);
    let si = matrix
        .forms
        .iter()
        .find(|form| form.surface == "satosi")
        .unwrap_or_else(|| panic!("{matrix:#?}"));
    let ta = matrix
        .forms
        .iter()
        .find(|form| form.surface == "satota")
        .unwrap();
    assert_eq!(si.predictions.len(), 1);
    assert_eq!(ta.predictions.len(), 1);
    assert_eq!(si.predictions[0].derivations[0][0].label, "plural");
    assert_ne!(
        si.predictions[0].signature_id,
        ta.predictions[0].signature_id
    );
}

#[test]
fn guide_can_consume_the_real_matrix_without_session_state_in_core() {
    let (grammar, catalog) = setup();
    let request = ClassificationRequest {
        stem: "sato".into(),
        known: KnownFacts::default(),
        budgets: ClassificationBudgets::default(),
    };
    let first = classify(&grammar, &catalog, request.clone()).unwrap();
    let second = classify(&grammar, &catalog, request).unwrap();
    assert_eq!(first, second);
    let mut guide = ClassificationGuide::new(first);
    let form = guide.next_form().unwrap();
    guide.answer(&form.id, Judgment::Yes).unwrap();
    assert_eq!(guide.remaining_signatures().len(), 1);
    assert!(guide.undo());
    assert_eq!(guide.remaining_signatures().len(), 2);
}

#[test]
fn precise_budgets_never_claim_truncated_work_is_exhaustive() {
    let (grammar, catalog) = setup();
    for (budgets, reason) in [
        (
            ClassificationBudgets {
                max_derivations: 1,
                ..ClassificationBudgets::default()
            },
            TruncationReason::DerivationLimit,
        ),
        (
            ClassificationBudgets {
                max_candidates: 1,
                ..ClassificationBudgets::default()
            },
            TruncationReason::CandidateLimit,
        ),
        (
            ClassificationBudgets {
                max_steps: 1,
                max_derivations: 100,
                ..ClassificationBudgets::default()
            },
            TruncationReason::StepLimit,
        ),
        (
            ClassificationBudgets {
                max_time_ms: 0,
                ..ClassificationBudgets::default()
            },
            TruncationReason::TimeLimit,
        ),
    ] {
        let matrix = classify(
            &grammar,
            &catalog,
            ClassificationRequest {
                stem: "sato".into(),
                known: KnownFacts::default(),
                budgets,
            },
        )
        .unwrap();
        assert!(!matrix.exhaustive, "{reason:?}: {matrix:#?}");
        assert_eq!(matrix.truncation_reason, Some(reason));
    }
    let err = classify(
        &grammar,
        &catalog,
        ClassificationRequest {
            stem: "sato".into(),
            known: KnownFacts::default(),
            budgets: ClassificationBudgets {
                max_forms: 129,
                ..ClassificationBudgets::default()
            },
        },
    )
    .unwrap_err();
    assert_eq!(err.code, "invalid_classification_budget");

    let matrix = classify(
        &grammar,
        &catalog,
        ClassificationRequest {
            stem: "sato".into(),
            known: KnownFacts::default(),
            budgets: ClassificationBudgets {
                max_forms: 1,
                ..ClassificationBudgets::default()
            },
        },
    )
    .unwrap();
    assert_eq!(matrix.forms.len(), 1);
    assert_eq!(matrix.truncation_reason, Some(TruncationReason::FormLimit));
    assert!(!matrix.exhaustive);
}
