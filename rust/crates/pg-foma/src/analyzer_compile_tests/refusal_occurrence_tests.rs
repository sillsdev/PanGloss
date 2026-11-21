use super::*;
use serde_json::json;

fn aliased_fixture_xml() -> String {
    let xml = pg_conformance_fixtures::require_fixture("edge-cases", "featureless-literal-target")
        .load_grammar_xml();
    assert_eq!(xml.matches("<Representation>k</Representation>").count(), 1);
    xml.replace("<Representation>k</Representation>", "<Representation>k</Representation><Representation>x</Representation>")
}

#[test]
fn table_and_rewrite_witnesses_are_counted_before_deduplication() {
    let grammar = pg_grammar::load(&aliased_fixture_xml()).unwrap();
    let (result, profile) = compile_proposer_with_profile(&grammar);
    let Err(FomaError::CapabilityRefused(diagnostics)) = result else {
        panic!("expected capability refusal without an artifact");
    };
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].construct, "referenced character table lacks a singleton spelling closure");
    assert_eq!(profile.final_state_count, None);
    let report = serde_json::to_value(&profile.construction).unwrap();
    let cause = json!({"kind": "proposal_recall", "details": "ambiguous_segment_spelling"});
    assert_eq!(report["attempts"][0]["capability_causes"], json!([cause]));
    assert_eq!(report["attempts"][0]["proposal_recall_occurrences"], json!([{
        "strategy": "tuned-surface-probed", "cause": cause, "occurrences": 2
    }]));
}

#[test]
fn each_stratum_reference_counts_without_replacing_the_first_witness() {
    let mut grammar = pg_grammar::load(&aliased_fixture_xml()).unwrap();
    let (result, _) = compile_proposer_with_profile(&grammar);
    let Err(FomaError::CapabilityRefused(first)) = result else { panic!("expected refusal") };
    grammar.strata.push(pg_grammar::load(&aliased_fixture_xml()).unwrap().strata.remove(0));
    let (result, profile) = compile_proposer_with_profile(&grammar);
    let Err(FomaError::CapabilityRefused(repeated)) = result else { panic!("expected refusal") };
    assert_eq!(first, repeated);
    assert_eq!(profile.construction.attempts[0].proposal_recall_occurrences.as_ref().unwrap()[0].occurrences, 3);
    grammar.strata[0].prules.clear();
    let (_, profile) = compile_proposer_with_profile(&grammar);
    assert_eq!(profile.construction.attempts[0].proposal_recall_occurrences.as_ref().unwrap()[0].occurrences, 2);
    grammar.strata[1].prules.clear();
    let (result, profile) = compile_proposer_with_profile(&grammar);
    assert!(result.is_ok(), "a complete literal spelling plan now covers the table witness");
    assert!(profile.construction.attempts[0].proposal_recall_occurrences.as_ref().unwrap().is_empty());
}

#[test]
fn all_proposal_causes_keep_independent_occurrences_in_first_witness_order() {
    use pg_health::capability::{CapabilityRefusal, ProposalRecallGap};
    let xml = aliased_fixture_xml()
        .replace("<NaturalClasses>", r#"<NaturalClasses><FeatureNaturalClass id="any"><Name>Any</Name></FeatureNaturalClass>"#)
        .replace("<PhonologicalRuleDefinitions>", r#"<PhonologicalRuleDefinitions>
        <PhonologicalRule id="epent"><Name>Insert</Name><PhoneticInput/>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence>
          <SimpleContext naturalClass="any"/>
          </PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>
        <PhonologicalRule id="narrow"><Name>Narrow</Name>
          <PhoneticInput><PhoneticSequence><Segment segment="cK"/><Segment segment="cS"/></PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence>
          <SimpleContext naturalClass="ncS"/>
          </PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
        </PhonologicalRule>"#)
        .replace(r#"phonologicalRules="prLen""#, r#"phonologicalRules="prLen epent narrow" morphologicalRules="red trunc""#)
        .replace("<LexicalEntries>", r#"<MorphologicalRuleDefinitions>
        <MorphologicalRule id="red"><Name>Copy</Name><MorphologicalSubrules><MorphologicalSubrule id="redSub">
          <MorphologicalInput><PhoneticSequence id="host"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="host"/><CopyFromInput index="host"/></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
        <MorphologicalRule id="trunc"><Name>Drop</Name><MorphologicalSubrules><MorphologicalSubrule id="truncSub">
          <MorphologicalInput><PhoneticSequence id="head"><Segment segment="cK"/></PhoneticSequence>
          <PhoneticSequence id="tail"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="tail"/></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
        </MorphologicalRuleDefinitions><AffixTemplates>
        <AffixTemplate final="false"><Name>Partial</Name><Slot optional="true" morphologicalRules="red"><Name>Copy</Name></Slot></AffixTemplate>
        <AffixTemplate><Name>Final</Name><Slot optional="true" morphologicalRules="trunc"><Name>Drop</Name></Slot></AffixTemplate>
        </AffixTemplates><LexicalEntries>"#);
    let mut grammar = pg_grammar::load(&xml).unwrap();
    grammar.strata.push(pg_grammar::load(&xml).unwrap().strata.remove(0));
    let (result, profile) = compile_proposer_with_profile(&grammar);
    let Err(FomaError::CapabilityRefused(diagnostics)) = result else { panic!("all causes must refuse") };
    let counts = profile.construction.attempts[0].proposal_recall_occurrences.as_ref().unwrap();
    let expected = [
        (ProposalRecallGap::AmbiguousSegmentSpelling, 7),
        (ProposalRecallGap::TemplateReentry, 1),
        (ProposalRecallGap::EmptiedMorphReorder, 1),
        (ProposalRecallGap::PhonologicalReduplication, 2),
        (ProposalRecallGap::TruncationComposition, 2),
        (ProposalRecallGap::SymbolicEpenthesis, 2),
        (ProposalRecallGap::NarrowingClassOutput, 2),
    ];
    assert_eq!(counts.iter().map(|c| (c.cause, c.occurrences)).collect::<Vec<_>>(), expected.map(|(gap, n)| (CapabilityRefusal::ProposalRecall(gap), n)));
    assert_eq!(diagnostics.iter().filter(|d| matches!(d.refusal, CapabilityRefusal::ProposalRecall(_))).count(), 7);
    assert!(counts.iter().all(|c| c.strategy == pg_health::strategy::EmissionStrategy::TunedSurfaceProbed));
    assert_eq!(profile.final_state_count, None);
    assert_eq!(profile.final_arc_count, None);
}

#[test]
fn unevaluated_attempts_and_empty_counts_remain_distinct_and_old_reports_decode() {
    use crate::construction::{ConstructionReport, ConstructionStrategy, FstCompileConfig, StrategySelection};
    let grammar = pg_conformance_fixtures::require_fixture("edge-cases", "rewrite-default-environment").load_grammar();
    let config = FstCompileConfig { strategy: StrategySelection::Auto, auto_order: vec![ConstructionStrategy::Factored, ConstructionStrategy::SurfaceEnum], ..Default::default() };
    let (result, profile) = compile_proposer_with_config(&grammar, &config);
    result.unwrap();
    assert_eq!(profile.construction.attempts[0].proposal_recall_occurrences, None);
    assert_eq!(profile.construction.attempts[1].proposal_recall_occurrences, Some(Vec::new()));
    assert!(profile.construction.attempts.iter().all(|a| a.capability_causes.is_empty()));
    let mut wire = serde_json::to_value(&profile.construction).unwrap();
    assert_eq!(wire["attempts"][0]["strategy"], "factored");
    assert_eq!(wire["attempts"][1]["strategy"], "surface-enum");
    assert_eq!(serde_json::from_value::<ConstructionReport>(wire.clone()).unwrap(), profile.construction);
    for attempt in wire["attempts"].as_array_mut().unwrap() {
        let attempt = attempt.as_object_mut().unwrap();
        attempt.remove("capability_causes");
        attempt.remove("proposal_recall_occurrences");
    }
    let old: ConstructionReport = serde_json::from_value(wire).unwrap();
    assert!(old.attempts.iter().all(|a| a.capability_causes.is_empty() && a.proposal_recall_occurrences.is_none()));
}
