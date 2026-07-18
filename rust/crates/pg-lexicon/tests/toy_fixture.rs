//! End-to-end tests against a small, hand-built, ORIGINAL HermitCrab XML fixture (not derived
//! from any real language project — same policy as the PanGloss-demo toy grammar): two noun
//! inflection classes (`C1`, `C2`), distinguished purely by an MPR feature
//! (`LexicalEntry@ruleFeatures`), whose plural affix differs (`+si` for `C1`, `+ta` for `C2`) via
//! two MPR-gated subrules of one `plural` morphological rule. Exercises every deliverable of
//! pg-lexicon's Sub-project 1 end to end: candidate-class enumeration, disambiguating-form
//! generation, shape validation, XML augmentation + reload + parse, and the `UserLexicon` JSON
//! round trip.

use pg_lexicon::{
    augment_xml, candidate_classes, disambiguating_forms, validate_shape, UserLexEntry, UserLexicon,
};

const TOY_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>LexiconToy</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech>
    </PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprC1">C1</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeature id="mprC2">C2</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cL"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cN"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cO"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll">
        <Name>All</Name>
        <Segment segment="cA" /><Segment segment="cI" /><Segment segment="cK" /><Segment segment="cL" />
        <Segment segment="cM" /><Segment segment="cN" /><Segment segment="cO" /><Segment segment="cP" />
        <Segment segment="cS" /><Segment segment="cT" /><Segment segment="cU" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrPl">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrPl" requiredPartsOfSpeech="posN" outputPartOfSpeech="posN">
            <Name>plural</Name>
            <MorphemeId>PL</MorphemeId>
            <Gloss>pl</Gloss>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subPlC1">
                <MorphologicalInput requiredMPRFeatures="mprC1">
                  <PhoneticSequence id="stem1">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem1" />
                  <InsertSegments><PhoneticShape>+si</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
              <MorphologicalSubrule id="subPlC2">
                <MorphologicalInput requiredMPRFeatures="mprC2">
                  <PhoneticSequence id="stem2">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem2" />
                  <InsertSegments><PhoneticShape>+ta</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eHouse" partOfSpeech="posN" ruleFeatures="mprC1">
            <Allomorphs><Allomorph id="aHouse"><PhoneticShape>milu</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>house</Gloss>
            <Properties><Property name="ID">101</Property></Properties>
          </LexicalEntry>
          <LexicalEntry id="eBook" partOfSpeech="posN" ruleFeatures="mprC1">
            <Allomorphs><Allomorph id="aBook"><PhoneticShape>kolo</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>book</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eStone" partOfSpeech="posN" ruleFeatures="mprC2">
            <Allomorphs><Allomorph id="aStone"><PhoneticShape>tanu</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>stone</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn candidate_classes_finds_both_noun_classes() {
    let g = pg_grammar::load(TOY_XML).expect("toy fixture loads");
    let classes = candidate_classes(&g);
    assert_eq!(
        classes.len(),
        2,
        "C1 and C2 must be distinct candidate classes: {classes:?}"
    );

    let c1 = classes
        .iter()
        .find(|c| c.mpr_names == vec!["C1".to_string()])
        .expect("a C1 class");
    assert_eq!(c1.pos.as_deref(), Some("n"));
    assert_eq!(c1.entry_count, 2, "eHouse + eBook are both C1");
    assert_eq!(
        c1.exemplar_xml_key, "eHouse",
        "first-seen entry in the class"
    );
    assert_eq!(c1.exemplar_morph_id.as_deref(), Some("101"));

    let c2 = classes
        .iter()
        .find(|c| c.mpr_names == vec!["C2".to_string()])
        .expect("a C2 class");
    assert_eq!(c2.entry_count, 1);
    assert_eq!(c2.exemplar_xml_key, "eStone");
    assert_eq!(
        c2.exemplar_morph_id, None,
        "eStone has no <Properties> block"
    );

    assert_ne!(c1.key, c2.key, "class keys must be distinct");
}

#[test]
fn disambiguating_forms_differ_between_the_two_classes() {
    let g = pg_grammar::load(TOY_XML).expect("toy fixture loads");
    let morpher = pg_parse::Morpher::new(&g, usize::MAX);
    let classes = candidate_classes(&g);
    assert_eq!(classes.len(), 2);

    let shape = "sato"; // a brand-new root, not in the lexicon, using only defined characters.
    validate_shape(&g, shape).expect("shape must validate against this grammar's alphabet");

    let forms = disambiguating_forms(&g, &morpher, shape, &classes, 4);
    assert_eq!(forms.len(), 2);

    let c1_key = classes
        .iter()
        .find(|c| c.mpr_names == vec!["C1".to_string()])
        .unwrap()
        .key
        .clone();
    let c2_key = classes
        .iter()
        .find(|c| c.mpr_names == vec!["C2".to_string()])
        .unwrap()
        .key
        .clone();

    let c1_forms = &forms.iter().find(|f| f.class_key == c1_key).unwrap().forms;
    let c2_forms = &forms.iter().find(|f| f.class_key == c2_key).unwrap().forms;

    assert!(
        !c1_forms.is_empty() && !c2_forms.is_empty(),
        "both classes must yield at least the bare stem"
    );
    assert_eq!(c1_forms[0], "sato", "bare stem comes first");
    assert_eq!(c2_forms[0], "sato");

    assert!(
        c1_forms.contains(&"satosi".to_string()),
        "C1's plural allomorph (+si) must appear: {c1_forms:?}"
    );
    assert!(
        c2_forms.contains(&"satota".to_string()),
        "C2's plural allomorph (+ta) must appear: {c2_forms:?}"
    );
    assert_ne!(c1_forms, c2_forms, "the two classes' form sets must differ");
}

#[test]
fn validate_shape_rejects_out_of_alphabet_characters() {
    let g = pg_grammar::load(TOY_XML).expect("toy fixture loads");
    assert!(validate_shape(&g, "milu").is_ok());
    let err = validate_shape(&g, "milux").unwrap_err();
    assert!(
        err.contains('x'),
        "message should name the offending character: {err}"
    );
}

#[test]
fn user_lexicon_json_round_trips() {
    let lexicon = UserLexicon {
        entries: vec![UserLexEntry {
            id: "abc-123".to_string(),
            shape: "sato".to_string(),
            gloss: "rock".to_string(),
            class_key: "n|C2".to_string(),
            added_at: "2026-07-14T00:00:00Z".to_string(),
        }],
    };
    let json = serde_json::to_string(&lexicon).expect("serializes");
    let round_tripped: UserLexicon = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(lexicon, round_tripped);
}

#[test]
fn augment_xml_splices_a_new_entry_that_reloads_and_parses() {
    let g = pg_grammar::load(TOY_XML).expect("toy fixture loads");
    let classes = candidate_classes(&g);
    let c2_key = classes
        .iter()
        .find(|c| c.mpr_names == vec!["C2".to_string()])
        .unwrap()
        .key
        .clone();

    let lexicon = UserLexicon {
        entries: vec![UserLexEntry {
            id: "user-1".to_string(),
            shape: "sato".to_string(),
            gloss: "rock".to_string(),
            class_key: c2_key,
            added_at: "2026-07-14T00:00:00Z".to_string(),
        }],
    };

    let (augmented_xml, report) =
        augment_xml(TOY_XML, &g, &lexicon, &classes).expect("augmentation succeeds");
    assert!(
        report.skipped.is_empty(),
        "nothing should be skipped: {:?}",
        report.skipped
    );
    assert!(
        augmented_xml.contains("user:user-1"),
        "the user marker must be present"
    );
    assert!(
        augmented_xml.contains("sato"),
        "the new shape must be present"
    );

    // Reload through the NORMAL loader (not the in-memory `g`) and confirm the original grammar's
    // own words still parse (the splice must not corrupt anything already there)...
    let g2 = pg_grammar::load(&augmented_xml).expect("augmented XML reloads cleanly");
    let m2 = pg_parse::Morpher::new(&g2, usize::MAX);
    assert!(
        !m2.parse_word("milu").analyses.is_empty(),
        "pre-existing entry must still parse"
    );
    assert!(
        m2.parse_word("tanusi").analyses.is_empty(),
        "tanu is C2 (pre-existing eStone), so it must not accept C1's plural allomorph either"
    );

    // ...and that the NEW word now parses too, including its class-appropriate plural (+ta, C2).
    let bare = m2.parse_word("sato");
    assert!(
        !bare.analyses.is_empty(),
        "the newly added root must parse bare: {:?}",
        bare.analyses
    );
    let plural = m2.parse_word("satota");
    assert!(
        !plural.analyses.is_empty(),
        "the new C2 word's plural (+ta) must parse: {:?}",
        plural.analyses
    );
    // The C1-suffixed form must NOT parse for this new (C2) word -- confirms the augmented entry
    // really carries C2's own `ruleFeatures`, not some default/blank MPR set.
    let wrong_plural = m2.parse_word("satosi");
    assert!(
        wrong_plural.analyses.is_empty(),
        "a C2 word must not accept C1's plural allomorph"
    );
}

#[test]
fn augment_xml_skips_and_reports_an_unresolvable_class_key() {
    let g = pg_grammar::load(TOY_XML).expect("toy fixture loads");
    let classes = candidate_classes(&g);

    let lexicon = UserLexicon {
        entries: vec![UserLexEntry {
            id: "user-2".to_string(),
            shape: "sato".to_string(),
            gloss: "rock".to_string(),
            class_key: "n|does-not-exist".to_string(),
            added_at: "2026-07-14T00:00:00Z".to_string(),
        }],
    };

    let (augmented_xml, report) =
        augment_xml(TOY_XML, &g, &lexicon, &classes).expect("augmentation still succeeds overall");
    assert_eq!(
        report.skipped.len(),
        1,
        "the unresolvable entry must be reported: {:?}",
        report.skipped
    );
    assert!(report.skipped[0].contains("user-2"));
    assert!(
        !augmented_xml.contains("user:user-2"),
        "a skipped entry must not be spliced in"
    );
    // The rest of the document must be untouched (byte-identical) when nothing was spliced in.
    assert_eq!(augmented_xml, TOY_XML);
}
