#[path = "toy_fixture.rs"]
mod fixture;

use pg_lexicon::{
    classify, ClassCatalog, ClassificationBudgets, ClassificationGuide, ClassificationRequest,
    Judgment, KnownFacts, TruncationReason,
};
use std::time::Duration;

fn setup() -> (pg_grammar::model::Grammar, ClassCatalog) {
    let grammar = pg_grammar::load(fixture::TOY_XML).unwrap();
    let catalog = ClassCatalog::from_grammar(&grammar).unwrap();
    (grammar, catalog)
}

fn two_rule_setup() -> (pg_grammar::model::Grammar, ClassCatalog) {
    let mut xml = fixture::TOY_XML
        .replace("<MorphologicalPhonologicalRuleFeature id=\"mprC2\">C2</MorphologicalPhonologicalRuleFeature>", "<MorphologicalPhonologicalRuleFeature id=\"mprC2\">C2</MorphologicalPhonologicalRuleFeature><MorphologicalPhonologicalRuleFeature id=\"mprStage\">Stage</MorphologicalPhonologicalRuleFeature>")
        .replace("morphologicalRules=\"mrPl\"", "morphologicalRules=\"mrPrep mrChoice\"");
    let start = xml.find("<MorphologicalRule id=\"mrPl\"").unwrap();
    let end =
        start + xml[start..].find("</MorphologicalRule>").unwrap() + "</MorphologicalRule>".len();
    let rules = r#"
      <MorphologicalRule id="mrPrep" requiredPartsOfSpeech="posN" outputPartOfSpeech="posN">
        <Name>prepare</Name><MorphemeId>PREP</MorphemeId><MorphologicalSubrules>
          <MorphologicalSubrule id="prepSub"><MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput MPRFeatures="mprStage"><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>+a</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrChoice" requiredPartsOfSpeech="posN" outputPartOfSpeech="posN">
        <Name>choose</Name><MorphemeId>CHOOSE</MorphemeId><MorphologicalSubrules>
          <MorphologicalSubrule id="choice1"><MorphologicalInput requiredMPRFeatures="mprStage mprC1"><PhoneticSequence id="stem1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="stem1" /><InsertSegments><PhoneticShape>+i</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
          <MorphologicalSubrule id="choice2"><MorphologicalInput requiredMPRFeatures="mprStage mprC2"><PhoneticSequence id="stem2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="stem2" /><InsertSegments><PhoneticShape>+o</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>"#;
    xml.replace_range(start..end, rules);
    let grammar = pg_grammar::load(&xml).unwrap();
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
fn bfs_finds_a_separator_that_requires_two_real_rules_in_replay_order() {
    let (grammar, catalog) = two_rule_setup();
    let matrix = classify(
        &grammar,
        &catalog,
        ClassificationRequest {
            stem: "sato".into(),
            known: KnownFacts::default(),
            budgets: ClassificationBudgets::default(),
        },
    )
    .unwrap();
    assert!(matrix.exhaustive, "{matrix:#?}");
    let form = matrix
        .forms
        .iter()
        .find(|form| {
            form.predictions
                .iter()
                .any(|prediction| prediction.derivations.iter().any(|path| path.len() == 2))
        })
        .expect("a two-rule separator");
    let path = form
        .predictions
        .iter()
        .flat_map(|prediction| &prediction.derivations)
        .find(|path| path.len() == 2)
        .unwrap();
    assert_eq!(
        path.iter()
            .map(|rule| rule.label.as_str())
            .collect::<Vec<_>>(),
        vec!["prepare", "choose"]
    );
    assert!(
        form.surface.ends_with("ai") || form.surface.ends_with("ao"),
        "{}",
        form.surface
    );
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

#[test]
fn parser_budget_instrumentation_is_shared_and_ordinary_synthesis_stays_unbounded() {
    let (grammar, catalog) = setup();
    let resolved = catalog.resolved(&catalog.signatures()[0].id).unwrap();
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    let ordinary = morpher.synthesize_resolved_stem(
        "sato",
        resolved.syn_fs,
        resolved.mpr,
        resolved.stratum,
        &[],
    );
    assert_eq!(ordinary, vec!["sato"]);

    let ample = pg_parse::SynthesisBudget::new(10_000, 10_000, Duration::from_secs(1));
    assert_eq!(
        morpher.synthesize_resolved_stem_bounded(
            "sato",
            resolved.syn_fs,
            resolved.mpr,
            resolved.stratum,
            &[],
            &ample
        ),
        ordinary
    );
    assert!(ample.steps() > 0);
    assert!(ample.candidates() > 0);

    let step = pg_parse::SynthesisBudget::new(1, 10_000, Duration::from_secs(1));
    let _ = morpher.synthesize_resolved_stem_bounded(
        "sato",
        resolved.syn_fs,
        resolved.mpr,
        resolved.stratum,
        &[],
        &step,
    );
    assert!(step.step_capped());
    assert_eq!(step.steps(), 1);

    let candidates = pg_parse::SynthesisBudget::new(10_000, 1, Duration::from_secs(1));
    let _ = morpher.synthesize_resolved_stem_bounded(
        "sato",
        resolved.syn_fs,
        resolved.mpr,
        resolved.stratum,
        &[],
        &candidates,
    );
    let _ = morpher.synthesize_resolved_stem_bounded(
        "sato",
        resolved.syn_fs,
        resolved.mpr,
        resolved.stratum,
        &[],
        &candidates,
    );
    assert!(candidates.candidate_capped());
    assert_eq!(candidates.candidates(), 1);

    let expired = pg_parse::SynthesisBudget::new(10_000, 10_000, Duration::ZERO);
    let _ = morpher.synthesize_resolved_stem_bounded(
        "sato",
        resolved.syn_fs,
        resolved.mpr,
        resolved.stratum,
        &[],
        &expired,
    );
    assert!(expired.timed_out());
}
