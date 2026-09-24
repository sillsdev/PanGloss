//! A partial-bearing grammar may be COMPILED under containment and may never be PUBLISHED.

use pg_foma::enumerate::EmissionStrategy;
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_foma_backend::health::{
    FindingClass, FindingCode, Metric, MetricValue, Severity, ValueProvenance,
};
use pg_foma_backend::production_admission::assess_completed_fst;
use pg_foma_backend::witnessed_coverage::compile_with_backend_for_measurement;
use pg_grammar::model::Grammar;

/// One char table, one entry, one suffixing affix-process rule; no partial marker anywhere.
const CONTROL_XML: &str = r#"<HermitCrabInput><Language><Name>PartialAdmissionControl</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cs" /></SegmentNaturalClass></NaturalClasses>
  <Strata><Stratum characterDefinitionTable="t1" morphologicalRules="rule-plain">
    <Name>S</Name>
    <MorphologicalRuleDefinitions>
      <MorphologicalRule id="rule-plain" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>plainSuffix</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="sub-plain">
          <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    </MorphologicalRuleDefinitions>
    <LexicalEntries><LexicalEntry id="entry-plain" partOfSpeech="posV"><Allomorphs>
      <Allomorph id="allo-plain"><PhoneticShape>a</PhoneticShape></Allomorph>
    </Allomorphs></LexicalEntry></LexicalEntries>
  </Stratum></Strata>
</Language></HermitCrabInput>"#;

/// `CONTROL_XML` with the LEXICAL ENTRY marked partial and nothing else changed.
fn partial_entry_xml() -> String {
    CONTROL_XML.replace(
        r#"<LexicalEntry id="entry-plain" partOfSpeech="posV">"#,
        r#"<LexicalEntry id="entry-partial" partOfSpeech="posV" partial="true">"#,
    )
}

/// `CONTROL_XML` with the AFFIX-PROCESS RULE marked partial and nothing else changed.
fn partial_rule_xml() -> String {
    CONTROL_XML
        .replace(
            r#"morphologicalRules="rule-plain""#,
            r#"morphologicalRules="rule-partial""#,
        )
        .replace(
            r#"<MorphologicalRule id="rule-plain" requiredPartsOfSpeech="posV""#,
            r#"<MorphologicalRule id="rule-partial" partial="true" requiredPartsOfSpeech="posV""#,
        )
}

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|error| panic!("fixture must load: {error}"))
}

/// A compiler contract violation panics rather than returning `Err`; a crash is not a completed measurement either way, so it is recorded as one more did-not-complete.
fn measure(grammar: &Grammar, strategy: EmissionStrategy) -> Result<(), String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compile_with_backend_for_measurement(grammar, strategy)
    })) {
        Ok(result) => result,
        Err(_) => Err("panicked".to_string()),
    }
}

/// The two partial model shapes, each named by the authored id its inventory must report.
fn partial_shapes() -> Vec<(&'static str, Grammar, &'static str)> {
    vec![
        (
            "partial lexical entry",
            load(&partial_entry_xml()),
            "entry-partial",
        ),
        (
            "partial affix-process rule",
            load(&partial_rule_xml()),
            "rule-partial",
        ),
    ]
}

#[test]
fn the_control_grammar_declares_no_partials_so_the_fixtures_isolate_one_variable() {
    let control = load(CONTROL_XML);
    let facts = control
        .partial_morpheme_facts()
        .expect("control grammar facts must be valid");
    assert!(
        !facts.has_partials(),
        "the control must have no partials or every assertion below is measuring two changes"
    );
    for (label, grammar, authored_id) in partial_shapes() {
        let facts = grammar
            .partial_morpheme_facts()
            .expect("partial fixture facts must be valid");
        assert_eq!(
            facts.total_count(),
            1,
            "{label}: exactly one partial expected"
        );
        assert!(
            facts.authored_ids().any(|id| id == authored_id),
            "{label}: inventory must name {authored_id}, got {:?}",
            facts.authored_ids().collect::<Vec<_>>()
        );
    }
}

#[test]
fn every_strategy_refuses_publication_for_both_partial_shapes() {
    for (label, grammar, authored_id) in partial_shapes() {
        for &strategy in ALL_STRATEGIES {
            let admission = assess_completed_fst(&grammar, strategy).expect("valid grammar facts");
            assert_eq!(admission.strategy(), strategy, "{label}");
            assert_eq!(
                admission.health().admission(),
                Severity::NotProductionReady,
                "{label} x {strategy:?}"
            );
            let by_class = admission.health().admission_by_class();
            assert_eq!(
                by_class.readiness,
                Severity::NotProductionReady,
                "{label} x {strategy:?}"
            );
            assert_eq!(
                by_class.representability,
                Severity::WithinLimits,
                "{label} x {strategy:?}"
            );
            assert_eq!(
                by_class.containment,
                Severity::WithinLimits,
                "{label} x {strategy:?}"
            );
            assert_eq!(
                by_class.process,
                Severity::WithinLimits,
                "{label} x {strategy:?}"
            );
            assert!(admission.blocks_publication(), "{label} x {strategy:?}");

            let finding = admission.health().findings.first().unwrap_or_else(|| {
                panic!("{label} x {strategy:?}: a blocked admission must carry a finding")
            });
            assert_eq!(finding.code, FindingCode::PartialMorphemeProductionPolicy);
            assert_eq!(finding.class(), FindingClass::Readiness);
            assert_eq!(finding.metric, Metric::PartialMorphemeCount);
            assert_eq!(finding.value, MetricValue::Count(1));
            assert_eq!(finding.provenance, ValueProvenance::Observed);
            assert!(
                finding.affected.iter().any(|id| id == authored_id),
                "{label}: the finding must name {authored_id}, got {:?}",
                finding.affected
            );
        }
    }
}

#[test]
fn every_strategy_admits_the_no_partial_control() {
    let grammar = load(CONTROL_XML);
    for &strategy in ALL_STRATEGIES {
        let admission = assess_completed_fst(&grammar, strategy).expect("valid grammar facts");
        assert_eq!(admission.strategy(), strategy);
        assert_eq!(
            admission.health().admission(),
            Severity::WithinLimits,
            "{strategy:?}: the control must stay production-admissible"
        );
        assert!(!admission.blocks_publication(), "{strategy:?}");
        assert!(admission.health().findings.is_empty(), "{strategy:?}");
    }
}

/// Per partial shape: every strategy refuses publication, and at least one COMPLETES the measurement first.
#[test]
fn all_three_strategies_measure_and_are_then_refused_publication() {
    for (label, grammar, _authored_id) in partial_shapes() {
        let mut completed = Vec::new();
        for &strategy in ALL_STRATEGIES {
            let measured = measure(&grammar, strategy);
            let admission = assess_completed_fst(&grammar, strategy).expect("valid grammar facts");
            eprintln!(
                "{label} x {}: measurement={} production_blocks={}",
                strategy.label(),
                match &measured {
                    Ok(()) => "completed".to_string(),
                    Err(reason) => format!("did-not-complete ({reason})"),
                },
                admission.blocks_publication(),
            );
            assert!(
                admission.blocks_publication(),
                "{label} x {strategy:?}: every strategy must refuse publication"
            );
            if measured.is_ok() {
                completed.push(strategy);
            }
        }
        assert!(
            !completed.is_empty(),
            "{label}: no strategy completed a measurement, so the refusals above cannot be \
             distinguished from compilers that simply cannot build this grammar"
        );
    }
}

/// The no-partial control must COMPLETE somewhere too, or the fixture proves nothing either way.
#[test]
fn the_control_measures_and_stays_publishable_on_every_strategy() {
    let grammar = load(CONTROL_XML);
    let mut completed = Vec::new();
    for &strategy in ALL_STRATEGIES {
        let measured = measure(&grammar, strategy);
        let admission = assess_completed_fst(&grammar, strategy).expect("valid grammar facts");
        eprintln!(
            "control x {}: measurement={} production_blocks={}",
            strategy.label(),
            if measured.is_ok() {
                "completed"
            } else {
                "did-not-complete"
            },
            admission.blocks_publication(),
        );
        assert!(!admission.blocks_publication(), "control x {strategy:?}");
        if measured.is_ok() {
            completed.push(strategy);
        }
    }
    assert!(
        !completed.is_empty(),
        "the control compiled nowhere, so it is not a control for anything"
    );
}

/// READINESS claims recall, so it is measured: every backend must contain the oracle's analyses for the partial-dependent words.
#[test]
fn every_backend_proposes_the_analyses_that_exist_only_because_a_rule_is_partial() {
    let fixture = pg_conformance_fixtures::discover()
        .into_iter()
        .find(|f| f.name == "final-template-partial-discriminators")
        .expect("the partial-discriminator fixture must be discoverable");
    let label = fixture.label();
    let grammar = load(&fixture.load_grammar_xml());
    let words: Vec<String> = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|entry| entry.word)
        .collect();
    assert!(!words.is_empty(), "{label}: fixture must have words");

    // Structural, not hardcoded: clearing every partial flag must cost these words their analyses.
    let mut cleared = load(&fixture.load_grammar_xml());
    for rule in &mut cleared.mrules {
        if let pg_grammar::model::MorphRuleDef::AffixProcess(def) = rule {
            def.partial_reason = None;
        }
    }
    for entry in &mut cleared.entries {
        entry.partial_reason = None;
    }

    let oracle = pg_parse::Morpher::new(&grammar, usize::MAX);
    let without_partials = pg_parse::Morpher::new(&cleared, usize::MAX);
    let mut partial_dependent = Vec::new();
    for word in &words {
        let with = oracle.parse_word(word).structured.len();
        let without = without_partials.parse_word(word).structured.len();
        if with > 0 && without == 0 {
            partial_dependent.push((word.clone(), with));
        }
    }
    for (word, count) in &partial_dependent {
        eprintln!("{label}: {word:?} has {count} analysis/analyses ONLY because a rule is partial");
    }
    assert!(
        partial_dependent.len() >= 2,
        "{label}: this gate needs at least two words whose parse DEPENDS on a partial rule, or a \
         backend that silently under-proposes on partials would pass it; found {:?}",
        partial_dependent
    );

    // Partial-dependent words only: a miss elsewhere in the fixture is `faithfulness_coverage_gate`'s question.
    let only_partial_dependent: Vec<String> = partial_dependent
        .iter()
        .map(|(word, _)| word.clone())
        .collect();
    let observation = pg_foma_backend::faithfulness_coverage::observe_fixture_containment(
        &label,
        &grammar,
        &only_partial_dependent,
    );
    let whole_fixture = pg_foma_backend::faithfulness_coverage::observe_fixture_containment(
        &label, &grammar, &words,
    );

    for &strategy in ALL_STRATEGIES {
        let outcome_for =
            |observation: &pg_foma_backend::faithfulness_coverage::FixtureContainmentObservation| {
                observation
                    .outcomes
                    .iter()
                    .find(|(observed, _)| *observed == strategy)
                    .map(|(_, outcome)| outcome.clone())
                    .unwrap_or_else(|| panic!("{label}: no containment outcome for {strategy:?}"))
            };
        let partial_outcome = outcome_for(&observation);
        eprintln!(
            "{label} x {}: partial-dependent containment {} | whole-fixture containment {}",
            strategy.label(),
            partial_outcome.label(),
            outcome_for(&whole_fixture).label(),
        );
        match partial_outcome {
            pg_foma_backend::faithfulness_coverage::ContainmentOutcome::Held => {}
            other => panic!(
                "{label} x {strategy:?}: partial-dependent recall is {}, so this backend cannot \
                 keep the READINESS classification -- its partial refusal must become \
                 CannotRepresent (representability) and refuse the build, not just publication. \
                 Outcome: {other:?}",
                other.label()
            ),
        }
    }
}

/// A readiness verdict must never be reachable from the representability axis, whatever the tier.
#[test]
fn the_partial_policy_code_answers_readiness_and_nothing_else() {
    assert_eq!(
        FindingCode::PartialMorphemeProductionPolicy.class(),
        FindingClass::Readiness
    );
    assert_ne!(
        FindingCode::PartialMorphemeProductionPolicy.class(),
        FindingClass::Representability
    );
}
