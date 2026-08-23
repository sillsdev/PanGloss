use pg_foma::templated_compile::{compile_templated_morphotactics, TemplatedCompileError};

fn mixed_slot_xml() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "tests/fixtures/pangloss/fst-completeness/atomic-template-slot-carrier/grammar.xml",
        ),
    )
    .expect("atomic template-slot carrier fixture must be readable")
}

#[test]
fn one_slot_choice_stays_atomic_across_the_root() {
    let grammar = pg_grammar::load(&mixed_slot_xml()).expect("mixed-slot fixture must load");
    let compiled = compile_templated_morphotactics(&grammar)
        .expect("the atomic carrier must compile before any lexc artifact is accepted");
    let mut proposer = compiled.proposer;

    for surface in ["pqs", "pqS", "Pqs", "PqS", "qt", "uq", "q"] {
        assert!(
            !proposer.propose(surface).is_empty(),
            "authored template path {surface:?} must remain reachable"
        );
    }

    for surface in ["pqt", "Pqt", "uqt", "uqs", "uqS", "pq", "Pq", "qs", "qS"] {
        assert!(
            proposer.propose(surface).is_empty(),
            "unmatched or crossed slot path {surface:?} must not be manufactured"
        );
    }
}

#[test]
fn suffix_first_one_sided_choices_still_enter_the_carrier() {
    let xml = mixed_slot_xml().replace(
        r#"          <MorphologicalSubrule id="wrapper">
            <MorphologicalInput><PhoneticSequence id="whole"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments><CopyFromInput index="whole" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
"#,
        "",
    );
    let grammar = pg_grammar::load(&xml).expect("suffix-first mixed-slot fixture must load");
    let compiled = compile_templated_morphotactics(&grammar)
        .expect("suffix-first mixed slot must enter its atomic carrier");
    let mut proposer = compiled.proposer;

    for surface in ["qt", "uq", "q"] {
        assert!(!proposer.propose(surface).is_empty(), "{surface:?}");
    }
    assert!(
        proposer.propose("uqt").is_empty(),
        "prefix-only and suffix-only alternatives must not cross"
    );
}

#[test]
fn carrier_preserves_other_template_slots() {
    let xml = mixed_slot_xml()
        .replace(
            "</SegmentDefinitions>",
            r#"<SegmentDefinition id="ch"><Representations><Representation>h</Representation></Representations></SegmentDefinition><SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations></SegmentDefinition></SegmentDefinitions>"#,
        )
        .replace(
            "</MorphologicalRuleDefinitions>",
            r#"<MorphologicalRule id="head" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
              <Name>head</Name><MorphologicalSubrules><MorphologicalSubrule id="head-subrule">
                <MorphologicalInput><PhoneticSequence id="head-stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="head-stem" /></MorphologicalOutput>
              </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>HEAD</MorphemeId>
            </MorphologicalRule><MorphologicalRule id="tail" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
              <Name>tail</Name><MorphologicalSubrules><MorphologicalSubrule id="tail-subrule">
                <MorphologicalInput><PhoneticSequence id="tail-stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="tail-stem" /><InsertSegments><PhoneticShape>h</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>TAIL</MorphemeId>
            </MorphologicalRule></MorphologicalRuleDefinitions>"#,
        )
        .replace(
            "<Name>template</Name><Slot optional=\"true\" morphologicalRules=\"mixed\">",
            r#"<Name>template</Name><Slot optional="false" morphologicalRules="head"><Name>head</Name></Slot><Slot optional="true" morphologicalRules="mixed">"#,
        )
        .replace(
            "</AffixTemplate></AffixTemplates>",
            r#"<Slot optional="false" morphologicalRules="tail"><Name>tail</Name></Slot></AffixTemplate></AffixTemplates>"#,
        );
    let grammar = pg_grammar::load(&xml).expect("multi-slot carrier fixture must load");
    let compiled = compile_templated_morphotactics(&grammar)
        .expect("one mixed slot among ordinary template slots must compile");
    let mut proposer = compiled.proposer;

    for surface in ["pxqsh", "pxqSh", "Pxqsh", "PxqSh", "xqth", "uxqh", "xqh"] {
        assert!(
            !proposer.propose(surface).is_empty(),
            "authored multi-slot path {surface:?} must remain reachable"
        );
    }

    for surface in [
        "pxqth", "Pxqth", "uxqth", "uxqsh", "uxqSh", "pxqh", "Pxqh", "xqsh", "xqSh",
    ] {
        assert!(
            proposer.propose(surface).is_empty(),
            "crossed or unmatched multi-slot path {surface:?} must not be manufactured"
        );
    }
}

#[test]
fn carrier_preserves_derivation_chains_around_the_root() {
    let xml = mixed_slot_xml()
        .replace(
            "</SegmentDefinitions>",
            r#"<SegmentDefinition id="cv"><Representations><Representation>v</Representation></Representations></SegmentDefinition><SegmentDefinition id="cw"><Representations><Representation>w</Representation></Representations></SegmentDefinition></SegmentDefinitions>"#,
        )
        .replace(
            "morphologicalRuleOrder=\"linear\" morphologicalRules=\"\"",
            "morphologicalRuleOrder=\"linear\" morphologicalRules=\"pre post\"",
        )
        .replace(
            "</MorphologicalRuleDefinitions>",
            r#"<MorphologicalRule id="pre" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
              <Name>derivational prefix</Name><MorphologicalSubrules><MorphologicalSubrule id="pre-subrule">
                <MorphologicalInput><PhoneticSequence id="pre-stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><InsertSegments><PhoneticShape>v</PhoneticShape></InsertSegments><CopyFromInput index="pre-stem" /></MorphologicalOutput>
              </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>PRE</MorphemeId>
            </MorphologicalRule>
            <MorphologicalRule id="post" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
              <Name>derivational suffix</Name><MorphologicalSubrules><MorphologicalSubrule id="post-subrule">
                <MorphologicalInput><PhoneticSequence id="post-stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="post-stem" /><InsertSegments><PhoneticShape>w</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>POST</MorphemeId>
            </MorphologicalRule></MorphologicalRuleDefinitions>"#,
        );
    let grammar = pg_grammar::load(&xml).expect("carrier plus derivation fixture must load");
    let compiled = compile_templated_morphotactics(&grammar)
        .expect("carrier must preserve the normal derivation topology");
    let mut proposer = compiled.proposer;

    for surface in ["pvqws", "uvqw", "vqwt", "vqw"] {
        assert!(
            !proposer.propose(surface).is_empty(),
            "authored carrier plus derivation path {surface:?} must remain reachable"
        );
    }
    for surface in ["pvqwt", "uvqwt", "uvqws"] {
        assert!(
            proposer.propose(surface).is_empty(),
            "carrier must not cross alternatives while preserving derivation: {surface:?}"
        );
    }
}

fn assert_typed_unsupported(xml: &str, context: &str) {
    let grammar = pg_grammar::load(xml).unwrap_or_else(|error| panic!("{context}: {error}"));
    match compile_templated_morphotactics(&grammar) {
        Err(TemplatedCompileError::Unsupported(_)) => {}
        Err(other) => panic!("{context}: expected typed Unsupported, got {other}"),
        Ok(_) => panic!("{context}: unsupported carrier topology compiled"),
    }
}

#[test]
fn two_cross_root_slots_fail_closed() {
    let xml = mixed_slot_xml().replace(
        "</AffixTemplate></AffixTemplates>",
        r#"<Slot optional="true" morphologicalRules="mixed"><Name>second mixed</Name></Slot></AffixTemplate></AffixTemplates>"#,
    );
    assert_typed_unsupported(&xml, "two cross-root slots");
}

#[test]
fn cross_root_slot_with_compounding_fails_closed() {
    let xml = mixed_slot_xml()
        .replace(
            "morphologicalRuleOrder=\"linear\" morphologicalRules=\"\"",
            "morphologicalRuleOrder=\"linear\" morphologicalRules=\"compound\"",
        )
        .replace(
            "</MorphologicalRuleDefinitions>",
            r#"<CompoundingRule id="compound" nonHeadPartsOfSpeech="pos">
              <Name>compound</Name><CompoundingSubrules><CompoundingSubrule>
                <HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
                <NonHeadMorphologicalInput><PhoneticSequence id="nonhead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="head" /><CopyFromInput index="nonhead" /></MorphologicalOutput>
              </CompoundingSubrule></CompoundingSubrules>
            </CompoundingRule></MorphologicalRuleDefinitions>"#,
        );
    assert_typed_unsupported(&xml, "cross-root slot with compounding");
}

#[test]
fn dual_authored_cross_root_rule_fails_closed_without_a_derivation_carrier() {
    let xml = mixed_slot_xml().replace(
        "morphologicalRuleOrder=\"linear\" morphologicalRules=\"\"",
        "morphologicalRuleOrder=\"linear\" morphologicalRules=\"mixed\"",
    );
    assert_typed_unsupported(&xml, "dual-authored cross-root rule");
}
