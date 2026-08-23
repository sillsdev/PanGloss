use pg_foma::structural_allomorph::{
    DerivationProjectionKey, MarkerZone, MorphologyRelationError, MorphologyRelationPlan,
    RepeatEligibility, SlotAlternativeRoute, SlotOwnership, SlotProjectionKey,
};
use pg_grammar::model::{
    AllomorphId, Grammar, MRuleId, MorphRuleDef, OutputAction, PartRef, StratumId,
    TemplateSlotZone,
};

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
  <Strata><Stratum characterDefinitionTable="t" morphologicalRuleOrder="linear" morphologicalRules="">
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

fn load_with_prefix_only_wrapper() -> Grammar {
    let xml = PLAN_XML.replacen(
        r#"<CopyFromInput index="whole1"/><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments>"#,
        r#"<CopyFromInput index="whole1"/>"#,
        1,
    );
    pg_grammar::load(&xml).unwrap_or_else(|error| panic!("prefix wrapper fixture failed: {error}"))
}

fn load_with_ordinary_literal() -> Grammar {
    let ordinary = r#"<MorphologicalSubrule id="ordinary"><MorphologicalInput><PhoneticSequence id="o"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny"/></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>"#;
    let xml = PLAN_XML.replace(
        "</MorphologicalSubrules><MorphemeId>MORPH</MorphemeId>",
        &format!("{ordinary}</MorphologicalSubrules><MorphemeId>MORPH</MorphemeId>"),
    );
    pg_grammar::load(&xml).unwrap_or_else(|error| panic!("ordinary fixture failed: {error}"))
}

fn remove_subrule(mut xml: String, id: &str) -> String {
    let start_tag = format!(r#"<MorphologicalSubrule id="{id}">"#);
    let start = xml.find(&start_tag).expect("subrule start");
    let close = "</MorphologicalSubrule>";
    let end = start + xml[start..].find(close).expect("subrule end") + close.len();
    xml.replace_range(start..end, "");
    xml
}

fn load_with_unique_suffix_caller() -> Grammar {
    let terminal = r#"<MorphologicalSubrule id="terminal"><MorphologicalInput><PhoneticSequence id="t0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence><PhoneticSequence id="t1"><Segment segment="ca"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="t0"/><ModifyFromInput index="t1"><SimpleContext naturalClass="ncAny"/></ModifyFromInput></MorphologicalOutput></MorphologicalSubrule>"#;
    let xml = remove_subrule(PLAN_XML.to_string(), "initial").replace(
        "</MorphologicalSubrules><MorphemeId>MORPH</MorphemeId>",
        &format!("{terminal}</MorphologicalSubrules><MorphemeId>MORPH</MorphemeId>"),
    );
    pg_grammar::load(&xml).unwrap_or_else(|error| panic!("caller fixture failed: {error}"))
}

fn load_with_realizational_structural_rule() -> Grammar {
    let realizational = r#"<RealizationalRule id="rr"><Name>realizational</Name><MorphologicalSubrules><MorphologicalSubrule id="rinitial"><MorphologicalInput><PhoneticSequence id="v0"><Segment segment="ca"/></PhoneticSequence><PhoneticSequence id="v1"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="v1"/></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules></RealizationalRule>"#;
    let xml = PLAN_XML
        .replacen(
            "morphologicalRules=\"\"",
            "morphologicalRules=\"rr\"",
            1,
        )
        .replace(
            "</MorphologicalRuleDefinitions>",
            &format!("{realizational}</MorphologicalRuleDefinitions>"),
        );
    pg_grammar::load(&xml)
        .unwrap_or_else(|error| panic!("realizational fixture failed: {error}"))
}

fn load_with_realizational_prefix_rule() -> Grammar {
    let realizational = r#"<RealizationalRule id="rr"><Name>realizational</Name><MorphologicalSubrules><MorphologicalSubrule id="rprefix"><MorphologicalInput><PhoneticSequence id="r0"><SimpleContext naturalClass="ncAny"/></PhoneticSequence></MorphologicalInput><MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="r0"/></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules></RealizationalRule>"#;
    let xml = PLAN_XML
        .replacen(
            "morphologicalRules=\"\"",
            "morphologicalRules=\"rr\"",
            1,
        )
        .replace(
            "</MorphologicalRuleDefinitions>",
            &format!("{realizational}</MorphologicalRuleDefinitions>"),
        );
    pg_grammar::load(&xml)
        .unwrap_or_else(|error| panic!("realizational prefix fixture failed: {error}"))
}

fn load_with_prefix_rule_at_both_sites() -> Grammar {
    let xml = PLAN_XML
        .replacen(
            r#"<CopyFromInput index="whole1"/><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments>"#,
            r#"<CopyFromInput index="whole1"/>"#,
            1,
        )
        .replacen(
            "morphologicalRules=\"\"",
            "morphologicalRules=\"mr\"",
            1,
        );
    pg_grammar::load(&xml).unwrap_or_else(|error| panic!("dual-site fixture failed: {error}"))
}

fn load_without_template() -> Grammar {
    let xml = PLAN_XML.replace(
        r#"<AffixTemplates><AffixTemplate id="tpl" final="true" requiredPartsOfSpeech="p"><Name>tpl</Name><Slot optional="true" morphologicalRules="mr"><Name>slot</Name></Slot></AffixTemplate></AffixTemplates>"#,
        "",
    )
    .replacen(
        "morphologicalRules=\"\"",
        "morphologicalRules=\"mr\"",
        1,
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
    assert_eq!(projection.ownership(), SlotOwnership::LegacyChoice);
    assert_eq!(projection.alternatives().len(), 3);

    let wrapper = projection
        .alternatives()
        .iter()
        .find(|alternative| alternative.allomorph() == ids[0])
        .expect("wrapper alternative");
    assert_eq!(wrapper.rule(), MRuleId(0));
    assert!(wrapper.prefix_binding().is_none());
    assert!(wrapper.suffix_binding().is_none());
    assert_eq!(wrapper.route(), SlotAlternativeRoute::Coupled);
    assert!(!wrapper.prefix_variants().is_empty());
    assert!(!wrapper.suffix_variants().is_empty());

    let drop = projection
        .alternatives()
        .iter()
        .find(|alternative| alternative.allomorph() == ids[1])
        .expect("drop alternative");
    assert!(drop.prefix_binding().is_none());
    assert_eq!(drop.route(), SlotAlternativeRoute::Suffix);
    assert_eq!(drop.suffix_binding().map(|binding| binding.zone), Some(MarkerZone::Suffix));

    let initial = projection
        .alternatives()
        .iter()
        .find(|alternative| alternative.allomorph() == ids[2])
        .expect("initial alternative");
    assert_eq!(initial.route(), SlotAlternativeRoute::Prefix);
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
    assert!(
        plan.derivation_projections().is_empty(),
        "a template-only rule must not gain a standalone derivation site"
    );
}

#[test]
fn fixed_snapshot_slot_refuses_an_intrinsic_alternative_on_the_wrong_side() {
    let mut grammar = load();
    grammar.templates[0].slots[0].zone = TemplateSlotZone::Prefix;
    let drop = allomorph_ids(&grammar)[1];

    assert!(matches!(
        MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0)),
        Err(MorphologyRelationError::ZoneMismatch {
            allomorph,
            required: MarkerZone::Suffix,
            actual: MarkerZone::Prefix,
        }) if allomorph == drop
    ));
}

#[test]
fn one_sided_wrapper_is_an_edge_route_not_a_coupled_choice() {
    let grammar = load_with_prefix_only_wrapper();
    let wrapper = allomorph_ids(&grammar)[0];
    let plan = MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0))
        .expect("a one-sided wrapper is an ordinary edge route");
    let alternative = plan
        .slot_projections()
        .iter()
        .flat_map(|projection| projection.alternatives())
        .find(|alternative| alternative.allomorph() == wrapper)
        .expect("prefix wrapper alternative");

    assert_eq!(alternative.route(), SlotAlternativeRoute::Prefix);
    assert!(alternative.prefix_variants().iter().any(|value| !value.is_empty()));
    assert!(alternative.suffix_variants().iter().all(String::is_empty));
}

#[test]
fn legacy_caller_zoned_alternative_inherits_the_only_sibling_edge() {
    let grammar = load_with_unique_suffix_caller();
    let caller = allomorph_ids(&grammar)[2];
    let plan = MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0))
        .expect("unique suffix evidence must resolve the caller-zoned alternative");
    let projection = plan
        .slot_projection(SlotProjectionKey {
            template_index: 0,
            slot_index: 0,
        })
        .expect("physical slot projection");
    let alternative = projection
        .alternatives()
        .iter()
        .find(|alternative| alternative.allomorph() == caller)
        .expect("caller-zoned alternative");

    assert_eq!(alternative.route(), SlotAlternativeRoute::Suffix);
    assert_eq!(
        alternative.suffix_binding().map(|binding| binding.zone),
        Some(MarkerZone::Suffix)
    );
    assert!(alternative.prefix_binding().is_none());
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
fn ordinary_realizational_derivation_is_an_unbounded_edge_alternative() {
    let grammar = load_with_realizational_prefix_rule();
    let plan = MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0))
        .expect("ordinary consuming realizational rule must be planned");
    let projection = plan
        .derivation_projection(DerivationProjectionKey {
            stratum: StratumId(0),
            site: 0,
            rule: MRuleId(1),
            route: SlotAlternativeRoute::Prefix,
        })
        .expect("realizational prefix projection");

    assert_eq!(projection.alternatives().len(), 1);
    let alternative = &projection.alternatives()[0];
    assert_eq!(alternative.rule(), MRuleId(1));
    assert_eq!(alternative.route(), SlotAlternativeRoute::Prefix);
    assert_eq!(alternative.repeat_eligibility(), RepeatEligibility::Unbounded);
}

#[test]
fn rule_explicitly_present_in_template_and_stratum_keeps_both_sites() {
    let grammar = load_with_prefix_rule_at_both_sites();
    let plan = MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0))
        .expect("the same rule may have distinct template and derivation sites");

    assert!(plan
        .slot_projection(SlotProjectionKey {
            template_index: 0,
            slot_index: 0,
        })
        .is_some());
    let derivation = plan
        .derivation_projection(DerivationProjectionKey {
            stratum: StratumId(0),
            site: 0,
            rule: MRuleId(0),
            route: SlotAlternativeRoute::Prefix,
        })
        .expect("explicit loose occurrence must not be suppressed by is_template_rule");
    assert!(derivation
        .alternatives()
        .iter()
        .all(|alternative| alternative.repeat_eligibility() == RepeatEligibility::Once));
}

#[test]
fn duplicate_authored_derivation_sites_remain_distinct() {
    let mut grammar = load_with_prefix_rule_at_both_sites();
    grammar.strata[0].mrules.push(MRuleId(0));
    let plan = MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0))
        .expect("duplicate authored sites must remain addressable");

    for site in 0..=1 {
        assert!(plan
            .derivation_projection(DerivationProjectionKey {
                stratum: StratumId(0),
                site,
                rule: MRuleId(0),
                route: SlotAlternativeRoute::Prefix,
            })
            .is_some());
    }
}

#[test]
fn ordinary_affix_derivation_retains_its_authored_application_bound() {
    let mut grammar = load_with_prefix_rule_at_both_sites();
    let MorphRuleDef::AffixProcess(rule) = &mut grammar.mrules[0] else {
        panic!("expected ordinary affix rule");
    };
    rule.max_apps = 2;
    let plan = MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0))
        .expect("bounded ordinary affix rule must be planned");
    let projection = plan
        .derivation_projection(DerivationProjectionKey {
            stratum: StratumId(0),
            site: 0,
            rule: MRuleId(0),
            route: SlotAlternativeRoute::Prefix,
        })
        .expect("bounded prefix derivation");

    assert!(projection.alternatives().iter().all(|alternative| {
        alternative.repeat_eligibility() == RepeatEligibility::Bounded { max_apps: 2 }
    }));
}

#[test]
fn zero_application_affix_retains_its_zero_bound() {
    let mut grammar = load_with_prefix_rule_at_both_sites();
    let MorphRuleDef::AffixProcess(rule) = &mut grammar.mrules[0] else {
        panic!("expected ordinary affix rule");
    };
    rule.max_apps = 0;
    let plan = MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0))
        .expect("a zero-bound rule remains a classified, confirmable proposal site");
    let projection = plan
        .derivation_projection(DerivationProjectionKey {
            stratum: StratumId(0),
            site: 0,
            rule: MRuleId(0),
            route: SlotAlternativeRoute::Prefix,
        })
        .expect("zero-bound prefix derivation");

    assert!(projection.alternatives().iter().all(|alternative| {
        alternative.repeat_eligibility() == RepeatEligibility::Bounded { max_apps: 0 }
    }));
}

#[test]
fn unbounded_structural_derivation_fails_closed() {
    let grammar = load_with_realizational_structural_rule();
    let structural = match &grammar.mrules[1] {
        MorphRuleDef::Realizational(rule) => rule.allomorphs[0].id,
        other => panic!("expected realizational rule, got {other:?}"),
    };

    assert!(matches!(
        MorphologyRelationPlan::build(&grammar, pg_grammar::model::TableId(0)),
        Err(MorphologyRelationError::UnsupportedRewrite {
            allomorph,
            reason_id: "unsupported-structural-repetition",
            ..
        }) if allomorph == structural
    ));
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
