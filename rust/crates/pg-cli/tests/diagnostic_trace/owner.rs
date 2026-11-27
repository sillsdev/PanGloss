use super::{event, nonfinal_grammar, parse, rewrite_grammar, template_grammar, GRAMMAR};
use serde_json::{json, Value};

const LIMITS: &[&str] = &["--trace-max-paths", "64", "--trace-max-events", "512"];

fn grouped_mpr(xml: &str) -> String {
    xml.replace(
        "<StemNames>",
        r#"<MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprRoot">Root</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeature id="mprAlsoRoot">Other root feature</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeature id="mprOther">Other</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeature id="mprExtra">Extra</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeatureGroup features="mprOther mprExtra" matchType="any" outputType="append" />
            <MorphologicalPhonologicalRuleFeatureGroup features="mprRoot mprAlsoRoot" matchType="any" outputType="append" />
        </MorphologicalPhonologicalRuleFeatures><StemNames>"#,
    )
    .replace(
        r#"<LexicalEntry id="eKib" partOfSpeech="posV">"#,
        r#"<LexicalEntry id="eKib" partOfSpeech="posV" ruleFeatures="mprRoot mprAlsoRoot">"#,
    )
}

fn morphology_alternatives() -> String {
    let mut xml = grouped_mpr(GRAMMAR);
    let start = xml.find(r#"<MorphologicalSubrule id="subTense">"#).unwrap();
    let end = start
        + xml[start..].find("</MorphologicalSubrule>").unwrap()
        + "</MorphologicalSubrule>".len();
    let original = &xml[start..end];
    let alternative = |identity| {
        original
            .replace("subTense", &format!("subTense{identity}"))
            .replace("stemTense", &format!("stemTense{identity}"))
    };
    let required = alternative("Required").replace(
        "<MorphologicalInput>",
        r#"<MorphologicalInput requiredMPRFeatures="mprOther mprExtra">"#,
    );
    let excluded = alternative("Excluded").replace(
        "<MorphologicalInput>",
        r#"<MorphologicalInput excludedMPRFeatures="mprRoot mprAlsoRoot">"#,
    );
    let pattern = alternative("Pattern").replace(
        r#"<OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>"#,
        r#"<Segment segment="ct" />"#,
    );
    xml.insert_str(start, &format!("{required}{excluded}{pattern}"));
    xml
}

fn phonology_alternatives() -> String {
    const OUTPUT: &str = r#"<PhoneticOutput><PhoneticSequence><Segment segment="cd" /></PhoneticSequence></PhoneticOutput>"#;
    grouped_mpr(&rewrite_grammar()).replace(
        "<PhonologicalSubrule>",
        &format!(
            r#"<PhonologicalSubrule requiredMPRFeatures="mprOther mprExtra">{OUTPUT}</PhonologicalSubrule>
            <PhonologicalSubrule excludedMPRFeatures="mprRoot mprAlsoRoot">"#,
        ),
    )
}

fn environment_failure() -> String {
    GRAMMAR.replace(
        r#"<Allomorph id="aKib"><PhoneticShape>kib</PhoneticShape>"#,
        r#"<Allomorph id="aKib"><PhoneticShape>kib</PhoneticShape>
            <RequiredEnvironments><Environment id="envLeftA"><LeftEnvironment><PhoneticTemplate>
                <PhoneticSequence><Segment segment="ca" /></PhoneticSequence>
            </PhoneticTemplate></LeftEnvironment></Environment></RequiredEnvironments>"#,
    )
}

fn assert_grouped_checks(data: &[Value], stage: &str) {
    for (reason, polarity, group) in [
        ("required_mpr_features", "required", 0),
        ("excluded_mpr_features", "excluded", 1),
    ] {
        let observed = data
            .iter()
            .find(|row| {
                row["reason"] == reason && row["direction"] == "synthesis" && row["stage"] == stage
            })
            .unwrap_or_else(|| panic!("missing synthesis {stage} {reason}"));
        assert_eq!(observed["kind"], "observe");
        let operands = &observed["operands"];
        let predicates = operands["required"].as_array().unwrap();
        assert_eq!(predicates.len(), 2);
        for (order, predicate) in predicates.iter().enumerate() {
            assert_eq!(predicate["group"], group);
            assert_eq!(predicate["order"], order);
            assert_eq!(predicate["polarity"], polarity);
            let present = operands["actual"]
                .as_array()
                .unwrap()
                .contains(&predicate["feature"]);
            assert_eq!(present, polarity == "excluded");
        }
    }
}

pub(super) fn tuple_fixtures() -> Vec<Vec<Value>> {
    let optional = template_grammar().replace(
        r#"<Slot morphologicalRules="mrTense">"#,
        r#"<Slot morphologicalRules="mrTense" optional="true">"#,
    );
    let optional = parse(&optional, "kib", LIMITS);
    for direction in ["analysis", "synthesis"] {
        let skipped = event(&optional, "optional_slot_skipped", direction);
        assert_eq!(skipped["kind"], "observe");
        assert_eq!(skipped["operands"]["clause"], "optional_slot");
    }
    assert_eq!(optional[optional.len() - 2]["search"]["outputCount"], 1);

    let ineligible = template_grammar()
        .replace(
            "</PartsOfSpeech>",
            r#"<PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech></PartsOfSpeech>"#,
        )
        .replace(
            r#"<AffixTemplate requiredPartsOfSpeech="posV">"#,
            r#"<AffixTemplate requiredPartsOfSpeech="posN">"#,
        );
    let ineligible = parse(&ineligible, "kib", LIMITS);
    let skipped = event(&ineligible, "template_ineligible", "synthesis");
    assert_eq!(skipped["kind"], "observe");
    assert_eq!(skipped["operands"]["clause"], "syntactic");
    assert_eq!(ineligible[ineligible.len() - 2]["search"]["outputCount"], 1);

    let template = template_grammar().replace(
        r#"morphologicalRuleOrder="linear""#,
        r#"morphologicalRuleOrder="unordered""#,
    );
    // A separate partial rule lets analysis reach the synthesis policy. The pending plural rule is non-partial.
    let final_template = template.replace(r#"id="mrPast2""#, r#"id="mrPast2" partial="true""#);
    let final_template = parse(&final_template, "kibeson", LIMITS);
    let prohibited = event(
        &final_template,
        "nonpartial_rule_prohibited_after_final_template",
        "synthesis",
    );
    assert_eq!(prohibited["kind"], "reject");
    assert_eq!(prohibited["operands"]["clause"], "finality");

    let nonfinal_template =
        nonfinal_grammar(&template).replace(r#"id="mrPl""#, r#"id="mrPl" partial="true""#);
    let nonfinal_template = parse(&nonfinal_template, "kibeson", LIMITS);
    let required = event(
        &nonfinal_template,
        "nonpartial_rule_required_after_nonfinal_template",
        "synthesis",
    );
    assert_eq!(required["kind"], "reject");
    assert_eq!(required["operands"]["clause"], "finality");

    let morphology = parse(&morphology_alternatives(), "kibes", LIMITS);
    assert_grouped_checks(&morphology, "morphology");
    for direction in ["analysis", "synthesis"] {
        assert_eq!(
            event(&morphology, "pattern_no_match", direction)["kind"],
            "observe"
        );
    }
    assert_eq!(morphology[morphology.len() - 2]["search"]["outputCount"], 1);

    let phonology = parse(&phonology_alternatives(), "kib", LIMITS);
    assert_grouped_checks(&phonology, "phonology");
    for direction in ["analysis", "synthesis"] {
        assert_eq!(
            event(&phonology, "pattern_no_output", direction)["kind"],
            "observe"
        );
    }
    assert_eq!(phonology[phonology.len() - 2]["search"]["outputCount"], 1);

    let environment = parse(&environment_failure(), "kib", LIMITS);
    let failure = event(&environment, "environment", "synthesis");
    assert_eq!(failure["kind"], "reject");
    assert_eq!(
        failure["operands"]["span"],
        json!({"start": 0, "endExclusive": 3, "unit": "interior_segment"})
    );
    assert_eq!(failure["operands"]["polarity"], "required");
    assert_eq!(
        failure["operands"]["alternatives"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        environment[environment.len() - 2]["search"]["outputCount"],
        0
    );

    vec![
        optional,
        ineligible,
        final_template,
        nonfinal_template,
        morphology,
        phonology,
        environment,
    ]
}
