use pg_lexicon::{ClassCatalog, SignatureId};

const PREFIX: &str = r#"<HermitCrabInput><Language><Name>T</Name>
<PartsOfSpeech><PartOfSpeech id="posN"><Name>noun</Name></PartOfSpeech></PartsOfSpeech>
<Families><Family id="fam">family</Family></Families>
<HeadFeatures><SymbolicFeature id="featNum"><Name>number</Name><Symbols>
<Symbol id="valSg">sg</Symbol><Symbol id="valPl">pl</Symbol>
</Symbols></SymbolicFeature><SymbolicFeature id="featCase"><Name>case</Name><Symbols>
<Symbol id="valNom">nom</Symbol><Symbol id="valAcc">acc</Symbol>
</Symbols></SymbolicFeature><ComplexFeature id="featAgr"><Name>agreement</Name></ComplexFeature></HeadFeatures>
<MorphologicalPhonologicalRuleFeatures>
<MorphologicalPhonologicalRuleFeature id="mprA">same</MorphologicalPhonologicalRuleFeature>
<MorphologicalPhonologicalRuleFeature id="mprB">same</MorphologicalPhonologicalRuleFeature>
</MorphologicalPhonologicalRuleFeatures>
<CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions>
<SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
</SegmentDefinitions></CharacterDefinitionTable><NaturalClasses><SegmentNaturalClass id="v"><Name>V</Name><Segment segment="a"/></SegmentNaturalClass></NaturalClasses>
<Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries>"#;
const SUFFIX: &str = "</LexicalEntries></Stratum></Strata></Language></HermitCrabInput>";

fn entry(id: &str, shape: &str, attrs: &str, allo_attrs: &str, fs: &str) -> String {
    let assigned = if fs.is_empty() {
        String::new()
    } else {
        format!("<AssignedHeadFeatures>{fs}</AssignedHeadFeatures>")
    };
    format!(
        r#"<LexicalEntry id="{id}" partOfSpeech="posN" {attrs}>{assigned}<Allomorphs><Allomorph id="a{id}" {allo_attrs}><PhoneticShape>{shape}</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>"#
    )
}

fn load(entries: String) -> pg_grammar::model::Grammar {
    pg_grammar::load(&format!("{PREFIX}{entries}{SUFFIX}")).expect("fixture loads")
}

#[test]
fn exact_signatures_deduplicate_but_subset_superset_and_authored_ids_do_not() {
    let sg = r#"<FeatureValue feature="featNum" symbolValues="valSg"/>"#;
    let grammar = load(
        [
            entry("e1", "a", r#"ruleFeatures="mprA""#, "", sg),
            entry("e2", "b", r#"ruleFeatures="mprA""#, "", sg),
            entry("e3", "a", r#"ruleFeatures="mprA mprB""#, "", sg),
            entry("e4", "b", r#"ruleFeatures="mprB""#, "", sg),
            entry("e5", "a", r#"ruleFeatures="mprA""#, "", ""),
        ]
        .concat(),
    );
    let catalog = ClassCatalog::from_grammar(&grammar).expect("catalog");
    assert_eq!(catalog.len(), 4, "{:#?}", catalog.signatures());
    assert_eq!(
        catalog
            .signatures()
            .iter()
            .map(|s| s.entry_count)
            .sum::<usize>(),
        5
    );
}

#[test]
fn excludes_partial_bound_pattern_and_restricted_entries() {
    let entries = [
        entry("ok", "a", "", "", ""),
        entry("partial", "b", r#"partial="true""#, "", ""),
        entry("bound", "a", "", r#"isBound="true""#, ""),
        entry("pattern", "[V]*", "", "", ""),
        entry("restricted", "b", "", "", "").replace(
            "</Allomorph>",
            "<RequiredEnvironments><Environment/></RequiredEnvironments></Allomorph>",
        ),
        entry("family", "a", r#"family="fam""#, "", ""),
        entry("mixed", "a", "", r#"isBound="true""#, "").replace(
            "</Allomorphs>",
            "<Allomorph id=\"amixedFree\"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>",
        ),
    ]
    .concat();
    let xml = format!("{PREFIX}{entries}</LexicalEntries></Stratum></Strata><MorphemeCoOccurrenceRules><MorphemeCoOccurrenceRule type=\"exclude\" primaryMorpheme=\"ok\" otherMorphemes=\"mixed\" adjacency=\"anywhere\"/></MorphemeCoOccurrenceRules></Language></HermitCrabInput>");
    let grammar = pg_grammar::load(&xml).expect("restricted fixture loads");
    let catalog = ClassCatalog::from_grammar(&grammar).expect("catalog");
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog.signatures()[0].entry_count, 1);
}

#[test]
fn recursive_authored_identity_changes_and_symbol_order_is_canonical() {
    let nested = r#"<FeatureValue feature="featAgr"><FeatureValue feature="featNum" symbolValues="valPl valSg"/></FeatureValue><FeatureValue feature="featCase" symbolValues="valNom valAcc"/>"#;
    let base = format!("{PREFIX}{}{SUFFIX}", entry("e", "a", "", "", nested));
    let id = |xml: &str| {
        ClassCatalog::from_grammar(&pg_grammar::load(xml).unwrap())
            .unwrap()
            .signatures()[0]
            .id
            .clone()
    };
    let base_id = id(&base);
    assert_ne!(base_id, id(&base.replace("posN", "posChanged")));
    assert_ne!(base_id, id(&base.replace("featCase", "featCaseChanged")));
    assert_ne!(base_id, id(&base.replace("valNom", "valNomChanged")));
    let reordered = base
        .replace(
            "<Symbol id=\"valSg\">sg</Symbol><Symbol id=\"valPl\">pl</Symbol>",
            "<Symbol id=\"valPl\">pl</Symbol><Symbol id=\"valSg\">sg</Symbol>",
        )
        .replace(
            "<SymbolicFeature id=\"featNum\"><Name>number</Name><Symbols>\n<Symbol id=\"valPl\">pl</Symbol><Symbol id=\"valSg\">sg</Symbol>\n</Symbols></SymbolicFeature><SymbolicFeature id=\"featCase\"><Name>case</Name><Symbols>\n<Symbol id=\"valNom\">nom</Symbol><Symbol id=\"valAcc\">acc</Symbol>\n</Symbols></SymbolicFeature>",
            "<SymbolicFeature id=\"featCase\"><Name>case</Name><Symbols>\n<Symbol id=\"valNom\">nom</Symbol><Symbol id=\"valAcc\">acc</Symbol>\n</Symbols></SymbolicFeature><SymbolicFeature id=\"featNum\"><Name>number</Name><Symbols>\n<Symbol id=\"valPl\">pl</Symbol><Symbol id=\"valSg\">sg</Symbol>\n</Symbols></SymbolicFeature>",
        )
        .replace(
            "symbolValues=\"valPl valSg\"",
            "symbolValues=\"valSg valPl\"",
        );
    let reordered_grammar = pg_grammar::load(&reordered).unwrap();
    assert!(
        reordered_grammar
            .syn_features
            .feature_by_xml_id("featCase")
            .unwrap()
            < reordered_grammar
                .syn_features
                .feature_by_xml_id("featNum")
                .unwrap()
    );
    assert_eq!(
        base_id,
        ClassCatalog::from_grammar(&reordered_grammar)
            .unwrap()
            .signatures()[0]
            .id
    );
}

#[test]
fn declaration_order_and_display_renames_do_not_change_identity() {
    let sg = r#"<FeatureValue feature="featNum" symbolValues="valSg"/>"#;
    let a = load(entry("e", "a", r#"ruleFeatures="mprA mprB""#, "", sg));
    let b_prefix = PREFIX.replace(
        "<MorphologicalPhonologicalRuleFeature id=\"mprA\">same</MorphologicalPhonologicalRuleFeature>\n<MorphologicalPhonologicalRuleFeature id=\"mprB\">same</MorphologicalPhonologicalRuleFeature>",
        "<MorphologicalPhonologicalRuleFeature id=\"mprB\">same</MorphologicalPhonologicalRuleFeature>\n<MorphologicalPhonologicalRuleFeature id=\"mprA\">same</MorphologicalPhonologicalRuleFeature>",
    );
    let b_xml = format!(
        "{b_prefix}{}{SUFFIX}",
        entry("e", "a", r#"ruleFeatures="mprB mprA""#, "", sg)
    )
    .replace(">noun<", ">renamed<")
    .replace(">same<", ">renamed-mpr<")
    .replace(">number<", ">renamed-feature<")
    .replace(">sg<", ">renamed-value<");
    let b = pg_grammar::load(&b_xml).expect("renamed fixture loads");
    assert_eq!(
        ClassCatalog::from_grammar(&a).unwrap().signatures()[0].id,
        ClassCatalog::from_grammar(&b).unwrap().signatures()[0].id
    );
}

#[test]
fn canonical_encoding_and_signature_id_are_golden() {
    let grammar = load(entry("e", "a", r#"ruleFeatures="mprA""#, "", ""));
    let catalog = ClassCatalog::from_grammar(&grammar).unwrap();
    let sig = &catalog.signatures()[0];
    assert_eq!(
        sig.canonical_encoding,
        r#"{"pos":"posN","features":[{"id":"__pos__","symbolic":["posN"]}],"mpr":["mprA"]}"#
    );
    assert_eq!(
        sig.id,
        SignatureId::parse("sig_2db1c5e06e900a88b0f2440b82a04be8172f36de6b32f25ad4fe64471c2c1f4c")
            .unwrap()
    );
}

#[test]
fn signature_id_parser_rejects_malformed_values() {
    assert!(SignatureId::parse("sig_short").is_err());
    assert!(SignatureId::parse(&format!("sig_{}", "G".repeat(64))).is_err());
}
