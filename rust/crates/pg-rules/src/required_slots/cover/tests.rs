//! Negative controls for the template cover proof: each case leaves the target unnamed.

use std::collections::BTreeSet;

use pg_grammar_model::model::{Grammar, MRuleId, MorphemeId, TemplateId};

use super::{hosting_templates, target_unreachable, unfillable_required_slot};

const XML: &str = r#"<HermitCrabInput><Language><Name>Cover</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>
<SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cC"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cF"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses><SegmentNaturalClass id="ncAny"><Name>Any</Name><Segment segment="cM" /><Segment segment="cA" /><Segment segment="cB" /><Segment segment="cC" /><Segment segment="cF" /></SegmentNaturalClass></NaturalClasses>
<Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered"><Name>S</Name>
<MorphologicalRuleDefinitions>
<MorphologicalRule id="mrA"><Name>A</Name><MorphemeId>A</MorphemeId>
<MorphologicalSubrules><MorphologicalSubrule id="subA">
<MorphologicalInput><PhoneticSequence id="stA"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
<MorphologicalOutput><CopyFromInput index="stA" /><InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments></MorphologicalOutput>
</MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
<MorphologicalRule id="mrB"><Name>B</Name><MorphemeId>B</MorphemeId>
<MorphologicalSubrules><MorphologicalSubrule id="subB">
<MorphologicalInput><PhoneticSequence id="stB"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
<MorphologicalOutput><CopyFromInput index="stB" /><InsertSegments><PhoneticShape>b</PhoneticShape></InsertSegments></MorphologicalOutput>
</MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
<MorphologicalRule id="mrC"><Name>C</Name><MorphemeId>C</MorphemeId>
<MorphologicalSubrules><MorphologicalSubrule id="subC">
<MorphologicalInput><PhoneticSequence id="stC"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
<MorphologicalOutput><CopyFromInput index="stC" /><InsertSegments><PhoneticShape>c</PhoneticShape></InsertSegments></MorphologicalOutput>
</MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
<MorphologicalRule id="mrF"><Name>F</Name><MorphemeId>F</MorphemeId>
<MorphologicalSubrules><MorphologicalSubrule id="subF">
<MorphologicalInput><PhoneticSequence id="stF"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
<MorphologicalOutput><CopyFromInput index="stF" /><InsertSegments><PhoneticShape>f</PhoneticShape></InsertSegments></MorphologicalOutput>
</MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>
<CompoundingRule id="crX" multipleApplication="1" headPartsOfSpeech="posV" nonHeadPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>X</Name>
<CompoundingSubrules><CompoundingSubrule>
<HeadMorphologicalInput><PhoneticSequence id="h0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput>
<NonHeadMorphologicalInput><PhoneticSequence id="n0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput>
<MorphologicalOutput><CopyFromInput index="h0" /><CopyFromInput index="n0" /></MorphologicalOutput>
</CompoundingSubrule></CompoundingSubrules></CompoundingRule>
</MorphologicalRuleDefinitions>
<AffixTemplates>
<AffixTemplate><Name>t0 A then B</Name><Slot morphologicalRules="mrA"><Name>a</Name></Slot><Slot morphologicalRules="mrB"><Name>b</Name></Slot></AffixTemplate>
<AffixTemplate><Name>t1 A alone</Name><Slot morphologicalRules="mrA"><Name>a</Name></Slot></AffixTemplate>
<AffixTemplate><Name>t2 compound</Name><Slot morphologicalRules="crX"><Name>x</Name></Slot></AffixTemplate>
<AffixTemplate><Name>t3 C only</Name><Slot morphologicalRules="mrC"><Name>c</Name></Slot></AffixTemplate>
<AffixTemplate><Name>t4 F then B</Name><Slot morphologicalRules="mrF"><Name>f</Name></Slot><Slot morphologicalRules="mrB"><Name>b</Name></Slot></AffixTemplate>
</AffixTemplates>
<LexicalEntries><LexicalEntry id="eV" partOfSpeech="posV"><MorphemeId>V</MorphemeId><Allomorphs><Allomorph id="aV"><PhoneticShape>m</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries>
</Stratum></Strata></Language></HermitCrabInput>"#;

fn grammar() -> Grammar {
    pg_grammar::load(XML).expect("cover grammar loads")
}

fn morpheme(g: &Grammar, key: &str) -> MorphemeId {
    let at = g
        .morphemes
        .iter()
        .position(|m| m.xml_key == key)
        .unwrap_or_else(|| panic!("{key}"));
    MorphemeId(at as u32)
}

fn set(g: &Grammar, keys: &[&str]) -> BTreeSet<MorphemeId> {
    keys.iter().map(|key| morpheme(g, key)).collect()
}

fn template(g: &Grammar, name: &str) -> TemplateId {
    let at = g
        .templates
        .iter()
        .position(|t| t.name.as_deref() == Some(name))
        .expect(name);
    TemplateId(at as u32)
}

#[test]
fn a_rule_listed_in_a_stratum_is_not_hosted_only_by_templates() {
    let mut g = grammar();
    let wanted = set(&g, &["mrB"]);
    let rule = g
        .mrules
        .iter()
        .position(|r| r.morpheme() == Some(morpheme(&g, "mrB")))
        .unwrap();
    assert!(
        target_unreachable(&g, &wanted),
        "control: B is hosted only by templates that need another filler"
    );
    g.strata[0].mrules.push(MRuleId(rule as u32));
    assert_eq!(hosting_templates(&g, &wanted), None);
    assert!(!target_unreachable(&g, &wanted));
}

#[test]
fn a_morpheme_with_no_rule_is_never_unreachable() {
    let g = grammar();
    let root = set(&g, &["eV"]);
    assert_eq!(hosting_templates(&g, &root), None);
    assert!(!target_unreachable(&g, &root));
    let mixed = set(&g, &["mrA", "eV"]);
    assert_eq!(hosting_templates(&g, &mixed), None);
    assert!(!target_unreachable(&g, &mixed));
}

#[test]
fn a_compounding_rule_in_a_slot_is_never_proven_unfillable() {
    let g = grammar();
    let compound = template(&g, "t2 compound");
    assert_eq!(
        unfillable_required_slot(&g, compound, &set(&g, &["mrA"])),
        None
    );
    assert_eq!(
        unfillable_required_slot(&g, compound, &set(&g, &["eV"])),
        None
    );
}

#[test]
fn a_rule_shared_by_two_templates_needs_both_uncoverable() {
    let g = grammar();
    let wanted = set(&g, &["mrA"]);
    let two_slots = template(&g, "t0 A then B");
    let alone = template(&g, "t1 A alone");
    assert_eq!(unfillable_required_slot(&g, two_slots, &wanted), Some(1));
    assert_eq!(unfillable_required_slot(&g, alone, &wanted), None);
    let hosts = hosting_templates(&g, &wanted).unwrap();
    assert_eq!(hosts, BTreeSet::from([two_slots, alone]));
    assert!(!target_unreachable(&g, &wanted));
}
