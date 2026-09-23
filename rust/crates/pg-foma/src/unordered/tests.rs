use super::*;

/// Synthetic fixture generator: one stratum declaring `order` with `rule_count` trivial suffix rules; only order and rule count matter to this module's checks, so every rule is otherwise as bare as the loader accepts.
fn stratum_xml(order: &str, rule_count: u32) -> String {
    let mut rules = String::new();
    let mut segs = String::new();
    for i in 0..rule_count {
        segs.push_str(&format!(
            r#"<SegmentDefinition id="cx{i}"><Representations><Representation>x{i}</Representation></Representations></SegmentDefinition>"#
        ));
        rules.push_str(&format!(
            r#"<MorphologicalRule id="mr{i}" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
                     <Name>r{i}</Name>
                     <MorphologicalSubrules>
                       <MorphologicalSubrule id="sub{i}">
                         <MorphologicalInput><PhoneticSequence id="stem{i}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                         <MorphologicalOutput><InsertSegments><PhoneticShape>x{i}</PhoneticShape></InsertSegments><CopyFromInput index="stem{i}" /></MorphologicalOutput>
                       </MorphologicalSubrule>
                     </MorphologicalSubrules>
                     <MorphemeId>R{i}</MorphemeId>
                   </MorphologicalRule>"#
        ));
    }
    let rule_ids: Vec<String> = (0..rule_count).map(|i| format!("mr{i}")).collect();
    let rules_attr = if rule_ids.is_empty() {
        String::new()
    } else {
        format!(r#" morphologicalRules="{}""#, rule_ids.join(" "))
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>UnorderedBoundFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        {segs}
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="{order}"{rules_attr}>
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>{rules}</MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#,
    )
}

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

#[test]
fn linear_stratum_is_never_reported_regardless_of_rule_count() {
    let g = load(&stratum_xml("linear", 10));
    assert!(
        unordered_stratum_metrics(&g).is_empty(),
        "a Linear stratum must never appear in Unordered-only metrics"
    );
}

#[test]
fn unordered_stratum_reports_exact_rule_count() {
    let g = load(&stratum_xml("unordered", 3));
    let metrics = unordered_stratum_metrics(&g);
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].rule_count, 3);
}

#[test]
fn zero_rule_unordered_stratum_reports_zero_rules() {
    let g = load(&stratum_xml("unordered", 0));
    let metrics = unordered_stratum_metrics(&g);
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].rule_count, 0);
}
