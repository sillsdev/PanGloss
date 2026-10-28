//! The lookup-time stem-name and MPR replay drops alternatives that synthesis would reject, and changes nothing else.

#[path = "csharp_port_common/mod.rs"]
mod csharp_port_common;

use pg_parse::Morpher;

fn suffix(id: &str, gate: &str, input_attrs: &str, shape: &str, pers: &str) -> String {
    format!(
        r#"<MorphologicalRule id="{id}" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV" {gate}><Name>{id}</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="{id}Sub">
          <MorphologicalInput {input_attrs}><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>{shape}</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
        <OutputHeadFeatures><FeatureValue feature="featPers" symbolValues="{pers}" /></OutputHeadFeatures>
        <MorphemeId>{id}M</MorphemeId></MorphologicalRule>"#
    )
}

fn grammar() -> pg_grammar_model::model::Grammar {
    let stem_names = r#"<StemNames>
        <StemName id="sn1" partsOfSpeech="posV"><Name>sn1</Name><Regions>
          <Region><AssignedHeadFeatures><FeatureValue feature="featPers" symbolValues="symP1" /></AssignedHeadFeatures></Region>
          <Region><AssignedHeadFeatures><FeatureValue feature="featPers" symbolValues="symP2" /></AssignedHeadFeatures></Region>
        </Regions></StemName>
        <StemName id="sn2" partsOfSpeech="posV"><Name>sn2</Name><Regions>
          <Region><AssignedHeadFeatures><FeatureValue feature="featPers" symbolValues="symP1" /></AssignedHeadFeatures></Region>
          <Region><AssignedHeadFeatures><FeatureValue feature="featPers" symbolValues="symP3" /></AssignedHeadFeatures></Region>
        </Regions></StemName></StemNames>"#;
    let rules = [
        suffix("stemGated", r#"requiredStemName="sn1""#, "", "+ɯd", "symP1"),
        suffix(
            "mprRequired",
            "",
            r#"requiredMPRFeatures="mprLatinate""#,
            "+t",
            "symP2",
        ),
        suffix(
            "mprExcluded",
            "",
            r#"excludedMPRFeatures="mprGermanic""#,
            "+s",
            "symP3",
        ),
        suffix("plain", "", "", "+n", "symP1"),
    ]
    .concat();
    let lexicon = r#"<LexicalEntry id="eStem" partOfSpeech="posV" ruleFeatures="mprGermanic">
        <Allomorphs>
          <Allomorph id="aSan"><PhoneticShape>san</PhoneticShape></Allomorph>
          <Allomorph id="aSad" stemName="sn1"><PhoneticShape>sad</PhoneticShape></Allomorph>
          <Allomorph id="aSap" stemName="sn2"><PhoneticShape>sap</PhoneticShape></Allomorph>
        </Allomorphs>
        <MorphemeId>stem</MorphemeId></LexicalEntry>"#;
    csharp_port_common::build_grammar_w5(
        stem_names,
        "",
        &rules,
        "stemGated mprRequired mprExcluded plain",
        "",
        lexicon,
    )
}

const WORDS: [&str; 10] = [
    "san", "sanɯd", "sant", "sans", "sadɯd", "sapɯd", "sadt", "saps", "sadn", "sann",
];

#[test]
fn the_replay_keeps_every_analysis_and_saves_synthesis_work() {
    let g = grammar();
    let on = Morpher::new(&g, usize::MAX).with_lookup_replay(true);
    let off = Morpher::new(&g, usize::MAX).with_lookup_replay(false);
    let (mut steps_on, mut steps_off, mut parsed) = (0, 0, 0);
    for word in WORDS {
        let (a, b) = (on.parse_word(word), off.parse_word(word));
        let (mut a_an, mut b_an) = (a.analyses.clone(), b.analyses.clone());
        a_an.sort();
        b_an.sort();
        assert_eq!(a_an, b_an, "{word}");
        parsed += usize::from(!a_an.is_empty());
        steps_on += a.work_steps;
        steps_off += b.work_steps;
    }
    assert!(
        parsed >= 2,
        "control: the fixture must still parse some words"
    );
    assert!(steps_on < steps_off, "on={steps_on} off={steps_off}");
    assert!(on.lookup_replay_work() > 0);
    assert_eq!(off.lookup_replay_work(), 0);
}
