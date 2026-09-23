use super::*;
use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::MprSet;

/// Two tables/strata with deliberately misaligned raw indices: `t0`'s segment "z" and `t1`'s "q" both sit at index 0 but carry opposite feature values, so a wrongly-table-0-resolved `ncQ` can never match a real `t1` "q".
const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OwningTableProbe</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featF">
        <Name>f</Name>
        <Symbols><Symbol id="fp">+</Symbol><Symbol id="fm">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t0">
      <Name>T0</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0z">
          <Representations><Representation>z</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fp" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>T1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1q">
          <Representations><Representation>q</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fm" />
        </SegmentDefinition>
        <SegmentDefinition id="c1p">
          <Representations><Representation>p</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fp" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="c1q" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="c1p" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prQtoP">
        <Name>qtop</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncQ" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncP" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t0" morphologicalRuleOrder="unordered">
        <Name>S0</Name>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prQtoP">
        <Name>S1</Name>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn load() -> Grammar {
    pg_grammar::load(XML).unwrap_or_else(|e| panic!("owning-table probe grammar loads: {e}"))
}

#[test]
fn owning_table_for_prule_resolves_the_rules_own_stratum_not_table_zero() {
    let g = load();
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    assert_eq!(g.strata.len(), 2, "fixture must declare exactly 2 strata");
    assert_eq!(
        g.prules.len(),
        1,
        "fixture declares exactly 1 phonological rule"
    );

    let table = owning_table_for_prule(&g, PRuleId(0))
        .expect("prQtoP is wired into stratum S1's own phonologicalRules cascade");
    assert_eq!(
        table,
        TableId(1),
        "prQtoP belongs to stratum S1 (table 1) -- owning_table_for_prule must NOT return \
         table 0"
    );
}

/// Runs the rule through the real cached production path on a `t1`-native "q" segment: if `ncQ` is ever wrongly compiled against table 0 again, synthesis finds nothing and this test fails.
#[test]
fn cached_synthesis_resolves_natural_classes_against_the_rules_own_table_not_table_zero() {
    let g = load();
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("prQtoP must load as a PhonRuleDef::Rewrite");
    };
    let cache = RuleCache::build(&g);

    let t1 = &g.char_tables[1];
    let input =
        crate::shape_feat::segment_with_features(&g, t1, "q").expect("\"q\" segments against t1");

    let out = rewrite::synthesize_with_mpr_cached(
        &g,
        PRuleId(0),
        rule,
        &input,
        &FeatureStruct::EMPTY,
        MprSet::EMPTY,
        &cache,
    );
    assert_eq!(
        out.len(),
        1,
        "prQtoP must fire on a genuine t1 \"q\" segment when its own natural classes are \
         resolved against t1 (its real owning table); an empty result here means ncQ/ncP were \
         wrongly compiled against table 0 instead"
    );

    // The interior node's lanes must match "p"'s (f=+), not "z"'s -- an independent check from the match-at-all proof above, since both happen to share f=+.
    let p_lanes = t1
        .get(pg_grammar_model::chardef::CharDefId(1))
        .feature_lanes()
        .to_vec();
    let interior: Vec<usize> = (0..out[0].len())
        .filter(|&i| matches!(out[0].kind(i), pg_shape::NodeKind::Segment))
        .collect();
    assert_eq!(interior.len(), 1, "exactly one segment node");
    assert_eq!(
        out[0].node_lanes(interior[0]).to_vec(),
        p_lanes,
        "the rewritten node must carry ncP's (t1's \"p\") own lanes"
    );
}

/// Same regression as the two tests above, but for an affix allomorph's own LHS/RHS pattern rather than a phonological rule's environment.
#[test]
fn cached_affix_synthesis_resolves_the_allomorphs_own_lhs_rhs_pattern_against_its_own_table_not_table_zero(
) {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OwningTableAffixProbe</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures />
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featF">
        <Name>f</Name>
        <Symbols><Symbol id="fp">+</Symbol><Symbol id="fm">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t0">
      <Name>T0</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0z">
          <Representations><Representation>z</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fp" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>T1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1q">
          <Representations><Representation>q</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fm" />
        </SegmentDefinition>
        <SegmentDefinition id="c1p">
          <Representations><Representation>p</Representation></Representations>
          <FeatureValue feature="featF" symbolValues="fp" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="c1q" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t0" morphologicalRuleOrder="unordered">
        <Name>S0</Name>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrQtoQP">
        <Name>S1</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrQtoQP" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>plus-p</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subQP">
                <MorphologicalInput>
                  <PhoneticSequence id="stem"><SimpleContext naturalClass="ncQ" /></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eQ" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aQ"><PhoneticShape>q</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>root</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = pg_grammar::load(XML)
        .unwrap_or_else(|e| panic!("owning-table affix probe grammar loads: {e}"));
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    assert_eq!(g.strata.len(), 2, "fixture must declare exactly 2 strata");
    assert_eq!(
        g.mrules.len(),
        1,
        "fixture declares exactly 1 morphological rule"
    );
    assert_eq!(
        g.entries.len(),
        1,
        "fixture declares exactly 1 lexical entry"
    );

    let cache = RuleCache::build(&g);
    let mrid = MRuleId(0);
    let rule = &g.mrules[0];

    let word = morph::seed_from_entry(
        &g,
        pg_grammar_model::model::LexEntryId(0),
        FeatureStruct::EMPTY,
    );
    assert_eq!(
        word.stratum,
        pg_grammar_model::model::StratumId(1),
        "the root entry must load onto S1 (table t1), not S0"
    );

    let out = morph::synthesize_cached(&g, mrid, &word, rule, &cache);
    assert_eq!(
        out.len(),
        1,
        "mrQtoQP must fire on a genuine t1 \"q\" root when ncQ (its allomorph's own LHS \
         pattern) is resolved against t1 (its real owning table); an empty result here means \
         ncQ was wrongly compiled against table 0 instead"
    );

    let w = &out[0];
    let interior: Vec<usize> = (0..w.shape.len())
        .filter(|&i| matches!(w.shape.kind(i), pg_shape::NodeKind::Segment))
        .collect();
    assert_eq!(interior.len(), 2, "the root \"q\" plus the inserted \"p\"");

    let t1 = &g.char_tables[1];
    let q_lanes = t1
        .get(pg_grammar_model::chardef::CharDefId(0))
        .feature_lanes()
        .to_vec();
    let p_lanes = t1
        .get(pg_grammar_model::chardef::CharDefId(1))
        .feature_lanes()
        .to_vec();
    assert_eq!(
        w.shape.node_lanes(interior[0]).to_vec(),
        q_lanes,
        "the copied root node must keep t1's own \"q\" lanes"
    );
    assert_eq!(
        w.shape.node_lanes(interior[1]).to_vec(),
        p_lanes,
        "the inserted node must carry t1's own \"p\" lanes (cd_lanes resolved against t1), \
         not t0's"
    );
}
