use pg_foma::structural_allomorph::{
    DerivationProjectionKey, DerivationRepeatPolicy, MarkerZone, MorphologyRelationError,
    MorphologyRelationPlan, SlotProjectionKey,
};
use pg_grammar::model::{AllomorphId, Grammar, MRuleId, MorphRuleDef, OutputAction, PartRef};

const PLAN_XML: &str = r#"
<HermitCrabInput><Language><Name>plan-projection</Name>
  <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions>
    <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions></CharacterDefinitionTable>
  <NaturalClasses>
    <SegmentNaturalClass id="ncAny"><Name>Any</Name><Segment segment="ca"/><Segment segment="cb"/></SegmentNaturalClass>
    <SegmentNaturalClass id="ncB"><Name>B</Name><Segment segment="cb"/></SegmentNaturalClass>
  </NaturalClasses>
  <Strata><Stratum characterDefinitionTable="t" morphologicalRuleOrder="linear" morphologicalRules="mr">
    <Name>plan</Name>
    <MorphologicalRuleDefinitions><MorphologicalRule id="mr" requiredPartsOfSpeech="p" outputPartOfSpeech="p">
      <Name>plan</Name><MorphologicalSubrules>
        <MorphologicalSubrule id="wrapper"><MorphologicalInput><PhoneticSequence id="whole0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="whole1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="whole0"/><CopyFromInput index="whole1"/><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        <MorphologicalSubrule id="drop"><MorphologicalInput><PhoneticSequence id="head"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="tail"><SimpleContext naturalClass="ncB"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="head"/><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
        <MorphologicalSubrule id="initial"><MorphologicalInput><PhoneticSequence id="v0"><Segment segment="ca"/></PhoneticSequence><PhoneticSequence id="v1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="v1"/></MorphologicalOutput></MorphologicalSubrule>
      </MorphologicalSubrules><MorphemeId>MORPH</MorphemeId>
    </MorphologicalRule></MorphologicalRuleDefinitions>
    <AffixTemplates><AffixTemplate id="tpl" final="true" requiredPartsOfSpeech="p"><Name>tpl</Name><Slot optional="true" morphologicalRules="mr"><Name>slot</Name></Slot></AffixTemplate></AffixTemplates>
  </Stratum></Strata>
</Language></HermitCrabInput>
"#;

fn load() -> Grammar {
    pg_grammar::load(PLAN_XML).unwrap_or_else(|error| panic!("plan fixture failed: {error}"))
}

fn load_with_ordinary_literal() -> Grammar {
    let ordinary = r#"<MorphologicalSubrule id="ordinary"><MorphologicalInput><PhoneticSequence id="o"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>"#;
    let xml = PLAN_XML.replace(
        "</MorphologicalSubrules><MorphemeId>MORPH</MorphemeId>",
        &format!("{ordinary}</MorphologicalSubrules><MorphemeId>MORPH</MorphemeId>"),
    );
    pg_grammar::load(&xml).unwrap_or_else(|error| panic!("ordinary fixture failed: {error}"))
}

fn load_with_realizational_structural_rule() -> Grammar {
    let realizational = r#"<RealizationalRule id="rr"><Name>realizational</Name><MorphologicalSubrules><MorphologicalSubrule id="rinitial"><MorphologicalInput><PhoneticSequence id="v0"><Segment segment="ca"/></PhoneticSequence><PhoneticSequence id="v1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="v1"/></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules></RealizationalRule>"#;
    let xml = PLAN_XML
        .replacen(
            "morphologicalRules=\"mr\"",
            "morphologicalRules=\"mr rr\"",
            1,
        )
        .replace(
            "</MorphologicalRuleDefinitions>",
            &format!("{realizational}</MorphologicalRuleDefinitions>"),
        );
    pg_grammar::load(&xml)
        .unwrap_or_else(|error| panic!("realizational fixture failed: {error}"))
}

fn load_without_template() -> Grammar {
    let xml = PLAN_XML.replace(
        r#"<AffixTemplates><AffixTemplate id="tpl" final="true" requiredPartsOfSpeech="p"><Name>tpl</Name><Slot optional="true" morphologicalRules="mr"><Name>slot</Name></Slot></AffixTemplate></AffixTemplates>"#,
        "",
    );
    pg_grammar::load(&xml)
        .unwrap_or_else(|error| panic!("template-less fixture failed: {error}"))
}

fn allomorph_ids(grammar: &Grammar) -> Vec<AllomorphId> {
    match &grammar.mrules[0] {
        pg_grammar::model::MorphRuleDef::AffixProcess(rule) => {
            rule.allomorphs.iter().map(|allomorph| allomorph.id).collect()
        }
        other => panic!("expected affix process, got {other:?}"),
    }
}

#[test]
fn one_physical_slot_owns_coupled_wrapper_drop_and_initial_choices() {
    let grammar = load();
    let ids = allomorph_ids(&grammar);
    let plan = MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0))
        .expect("fixture must produce one plan");
    let projection = plan
        .slot_projection(SlotProjectionKey {
            template_index: 0,
            slot_index: 0,
        })
        .expect("physical slot projection");
    assert!(projection.optional());
    assert_eq!(projection.alternatives().len(), 3);

    let wrapper = projection
        .alternatives()
        .iter()
        .find(|alternative| alternative.allomorph() == ids[0])
        .expect("wrapper alternative");
    assert_eq!(wrapper.rule(), MRuleId(0));
    assert!(wrapper.prefix_binding().is_none());
    assert!(wrapper.suffix_binding().is_none());
    assert!(!wrapper.prefix_variants().is_empty());
    assert!(!wrapper.suffix_variants().is_empty());

    let drop = projection
        .alternatives()
        .iter()
        .find(|alternative| alternative.allomorph() == ids[1])
        .expect("drop alternative");
    assert!(drop.prefix_binding().is_none());
    assert_eq!(drop.suffix_binding().map(|binding| binding.zone), Some(MarkerZone::Suffix));

    let initial = projection
        .alternatives()
        .iter()
        .find(|alternative| alternative.allomorph() == ids[2])
        .expect("initial alternative");
    assert_eq!(initial.prefix_binding().map(|binding| binding.zone), Some(MarkerZone::Prefix));
    assert!(initial.suffix_binding().is_none());

    assert_eq!(
        projection
            .alternatives()
            .iter()
            .filter(|alternative| alternative.allomorph() == ids[0])
            .count(),
        1,
        "one optional slot must not gain an independent second choice"
    );
}

#[test]
fn unsupported_slot_shape_returns_a_typed_failure_instead_of_panicking() {
    let mut grammar = load();
    let unsupported = match &mut grammar.mrules[0] {
        MorphRuleDef::AffixProcess(rule) => {
            let allomorph = &mut rule.allomorphs[0];
            allomorph.rhs = vec![
                OutputAction::Copy(PartRef::Input(0)),
                OutputAction::Copy(PartRef::Input(0)),
            ];
            allomorph.id
        }
        other => panic!("expected affix process, got {other:?}"),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0))
    }));
    let result = result.expect("plan construction must never panic on authored grammar shapes");
    assert!(matches!(
        result,
        Err(MorphologyRelationError::UnsupportedRewrite { allomorph, .. })
            if allomorph == unsupported
    ));
}

#[test]
fn ambiguous_unzoned_literal_in_a_mixed_slot_fails_closed() {
    let grammar = load_with_ordinary_literal();
    let ambiguous = allomorph_ids(&grammar)[3];

    assert!(matches!(
        MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0)),
        Err(MorphologyRelationError::UnsupportedRewrite {
            allomorph,
            reason_id: "ambiguous-slot-zone",
            ..
        }) if allomorph == ambiguous
    ));
}

#[test]
fn realizational_structural_rule_has_an_independent_derivation_projection() {
    let grammar = load_with_realizational_structural_rule();
    let plan = MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0))
        .expect("realizational structural rule must be planned");
    let projection = plan
        .derivation_projection(DerivationProjectionKey {
            rule: MRuleId(1),
            zone: MarkerZone::Prefix,
        })
        .expect("realizational prefix projection");

    assert_eq!(projection.repeat_policy(), DerivationRepeatPolicy::RealizationalLoop);
    assert_eq!(projection.alternatives().len(), 1);
    let alternative = &projection.alternatives()[0];
    assert_eq!(alternative.rule(), MRuleId(1));
    assert_eq!(
        alternative.prefix_binding().map(|binding| binding.zone),
        Some(MarkerZone::Prefix)
    );
    assert!(plan
        .relation()
        .marker_binding_for_zone(alternative.allomorph(), MarkerZone::Prefix)
        .is_some());
}

#[test]
fn standalone_wrapper_refuses_until_its_two_halves_have_one_coupled_choice() {
    let grammar = load_without_template();
    let wrapper = allomorph_ids(&grammar)[0];

    assert!(matches!(
        MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0)),
        Err(MorphologyRelationError::UnsupportedRewrite {
            allomorph,
            reason_id: "standalone-wrapper-requires-coupled-projection",
            ..
        }) if allomorph == wrapper
    ));
}
