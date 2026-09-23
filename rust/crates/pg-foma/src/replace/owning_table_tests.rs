use super::*;
use pg_grammar::model::PhonRuleDef;

/// Table 0 (stratum "S0"): 2 segments. Table 1 (stratum "S1"): 3 segments -- deliberately different cardinalities so resolving against the wrong table gives a different, wrong `surviving` count.
const TWO_TABLE_ALPHA_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>TwoTableSymbolDivergenceAlphaFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featA">
        <Name>dummy</Name>
        <Symbols>
          <Symbol id="symA1">a</Symbol>
          <Symbol id="symA2">b</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t0">
      <Name>Table0</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0a"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c0b"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>Table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1a"><Representations><Representation>k</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c1b"><Representations><Representation>g</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
        <SegmentDefinition id="c1c"><Representations><Representation>x</Representation></Representations><FeatureValue feature="featA" symbolValues="symA1" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncBig"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule_alpha_t1">
        <Name>alpha rule on table 1</Name>
        <VariableFeatures>
          <VariableFeature id="var1" name="a" phonologicalFeature="featA" />
        </VariableFeatures>
        <PhoneticInput>
          <PhoneticSequence>
            <Segment segment="c1a" />
          </PhoneticSequence>
        </PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput>
              <PhoneticSequence>
                <SimpleContext naturalClass="ncBig">
                  <AlphaVariables>
                    <AlphaVariable variableFeature="var1" />
                  </AlphaVariables>
                </SimpleContext>
              </PhoneticSequence>
            </PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t0" morphologicalRuleOrder="unordered">
        <Name>S0</Name>
        <LexicalEntries>
          <LexicalEntry id="entry0" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>dummy0</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule_alpha_t1">
        <Name>S1</Name>
        <LexicalEntries>
          <LexicalEntry id="entry1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo1"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>dummy1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn two_table_alpha_grammar() -> Grammar {
    pg_grammar::load(TWO_TABLE_ALPHA_XML).unwrap_or_else(|e| {
        panic!(
            "failed to load two-table-symbol-divergence alpha fixture: {e}\n{TWO_TABLE_ALPHA_XML}"
        )
    })
}

fn rewrite_rule_by_xml_id<'g>(g: &'g Grammar, xml_id: &str) -> &'g RewriteRuleDef {
    for pr in &g.prules {
        if let PhonRuleDef::Rewrite(r) = pr {
            if r.xml_id == xml_id {
                return r;
            }
        }
    }
    panic!("prule {xml_id:?} not found in g.prules");
}

/// `owning_table` resolves `prule_alpha_t1` to table 1 (3 segments), never table 0 (2 segments).
#[test]
fn owning_table_resolves_to_the_rules_own_stratum_table_not_table_zero() {
    let g = two_table_alpha_grammar();
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    assert_eq!(
        g.char_tables[0].len(),
        2,
        "table 0 must have exactly 2 segments"
    );
    assert_eq!(
        g.char_tables[1].len(),
        3,
        "table 1 must have exactly 3 segments"
    );
    assert_eq!(g.strata.len(), 2, "fixture must declare exactly 2 strata");

    let rule = rewrite_rule_by_xml_id(&g, "prule_alpha_t1");
    let table = owning_table(&g, rule)
        .expect("prule_alpha_t1 is wired into stratum S1's own phonologicalRules cascade");
    assert_eq!(
        table.len(),
        3,
        "prule_alpha_t1 belongs to stratum S1 (table 1, 3 segments) -- owning_table must NOT \
         return table 0's 2-segment table"
    );
}

/// Full compile-level proof: `resolve_alpha_tuples`'s own `surviving` count for `prule_alpha_t1` is exactly 3 (table 1's cardinality), never table 0's 2.
#[test]
fn resolve_alpha_tuples_surviving_count_reflects_the_owning_table_not_table_zero() {
    let g = two_table_alpha_grammar();
    let rule = rewrite_rule_by_xml_id(&g, "prule_alpha_t1");
    let opts = FomaOptions::default();
    let (net, reports) = compile_rewrite_rule_subset(&opts, &g, rule, &|_| true)
        .expect("prule_alpha_t1 must compile");
    assert!(net.statecount > 0);
    assert_eq!(reports.len(), 1, "exactly one alpha-bearing subrule");
    assert_eq!(
        reports[0].surviving, 3,
        "surviving tuple count must equal table 1's own 3-member ncBig class -- 2 would mean \
         this rule wrongly resolved against table 0 instead of its own stratum's table"
    );
    assert_eq!(
        reports[0].raw_product, 3,
        "a single alpha occurrence's raw product equals its own candidate set size"
    );
}

/// Two VariableFeatures disagreeing over a 4-member class varying on both features -- `featBack` alone does not uniquely determine a member (`cI`/`cY` share `bkMinus`).
const TWO_VAR_AMBIGUOUS_DISAGREE_XML: &str = r#"<HermitCrabInput><Language><Name>AmbiguousDisagree</Name>
      <PartsOfSpeech><PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech></PartsOfSpeech>
      <PhonologicalFeatureSystem>
        <SymbolicFeature id="featBack"><Name>back</Name><Symbols><Symbol id="bkPlus">+bk</Symbol><Symbol id="bkMinus">-bk</Symbol></Symbols></SymbolicFeature>
        <SymbolicFeature id="featRound"><Name>round</Name><Symbols><Symbol id="rdMinus">-rd</Symbol><Symbol id="rdPlus">+rd</Symbol></Symbols></SymbolicFeature>
      </PhonologicalFeatureSystem>
      <CharacterDefinitionTable id="tbl"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations><FeatureValue feature="featBack" symbolValues="bkMinus" /><FeatureValue feature="featRound" symbolValues="rdMinus" /></SegmentDefinition>
          <SegmentDefinition id="cY"><Representations><Representation>y</Representation></Representations><FeatureValue feature="featBack" symbolValues="bkMinus" /><FeatureValue feature="featRound" symbolValues="rdPlus" /></SegmentDefinition>
          <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featBack" symbolValues="bkPlus" /><FeatureValue feature="featRound" symbolValues="rdMinus" /></SegmentDefinition>
          <SegmentDefinition id="cU"><Representations><Representation>u</Representation></Representations><FeatureValue feature="featBack" symbolValues="bkPlus" /><FeatureValue feature="featRound" symbolValues="rdPlus" /></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncVowel"><Name>vowels</Name><Segment segment="cI" /><Segment segment="cY" /><Segment segment="cA" /><Segment segment="cU" /></SegmentNaturalClass></NaturalClasses>
      <PhonologicalRuleDefinitions>
        <PhonologicalRule id="prDoubleAlpha">
          <Name>doubleAlphaFlip</Name>
          <VariableFeatures>
            <VariableFeature id="varBack" name="a" phonologicalFeature="featBack" />
            <VariableFeature id="varRound" name="b" phonologicalFeature="featRound" />
          </VariableFeatures>
          <PhoneticInput><PhoneticSequence>
            <SimpleContext naturalClass="ncVowel"><AlphaVariables><AlphaVariable variableFeature="varBack" polarity="plus" /></AlphaVariables></SimpleContext>
            <SimpleContext naturalClass="ncVowel"><AlphaVariables><AlphaVariable variableFeature="varRound" polarity="plus" /></AlphaVariables></SimpleContext>
          </PhoneticSequence></PhoneticInput>
          <PhonologicalSubrules>
            <PhonologicalSubrule>
              <PhoneticOutput><PhoneticSequence>
                <SimpleContext naturalClass="ncVowel"><AlphaVariables><AlphaVariable variableFeature="varBack" polarity="minus" /></AlphaVariables></SimpleContext>
                <SimpleContext naturalClass="ncVowel"><AlphaVariables><AlphaVariable variableFeature="varRound" polarity="minus" /></AlphaVariables></SimpleContext>
              </PhoneticSequence></PhoneticOutput>
            </PhonologicalSubrule>
          </PhonologicalSubrules>
        </PhonologicalRule>
      </PhonologicalRuleDefinitions>
      <Strata><Stratum characterDefinitionTable="tbl" phonologicalRules="prDoubleAlpha"><Name>Main</Name>
        <LexicalEntries><LexicalEntry id="eAu" partOfSpeech="posN"><Allomorphs><Allomorph id="aAu"><PhoneticShape>au</PhoneticShape></Allomorph></Allomorphs><MorphemeId>AU</MorphemeId><Gloss>au</Gloss></LexicalEntry></LexicalEntries>
      </Stratum></Strata>
    </Language></HermitCrabInput>"#;

/// FALSIFICATION: unguarded, this shape's 64 surviving tuples collapse to one wrong branch (`down("au")` produced `"ii"`, never the oracle-facing set) -- must stay refused.
#[test]
fn two_var_ambiguous_disagree_stays_refused() {
    let g = pg_grammar::load(TWO_VAR_AMBIGUOUS_DISAGREE_XML).unwrap_or_else(|e| panic!("{e}"));
    let rule = rewrite_rule_by_xml_id(&g, "prDoubleAlpha");
    let opts = FomaOptions::default();
    assert!(
        compile_rewrite_rule_subset(&opts, &g, rule, &|_| true).is_none(),
        "an ambiguous disagree-polarity class must stay refused, not silently miscompile"
    );
    let table = owning_table(&g, rule).expect("rule resolves to a real owning table");
    assert_eq!(
        crate::lower::diagnose_unsupported(
            &g,
            table,
            &rule.subrules[0].rhs,
            crate::lower::PatternLowerScope::RewriteRuleCompile,
        ),
        crate::lower::UnsupportedPatternNode::AlphaAmbiguousDisagree,
        "the witness must name the ambiguous-disagree shape specifically"
    );
}
