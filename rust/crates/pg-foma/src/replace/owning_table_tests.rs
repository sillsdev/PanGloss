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

#[test]
fn ambiguous_disagreement_matches_recorded_oracle_after_confirmation() {
    assert_confirmed_ambiguous_fixture("nullable-disagree-plain-ltr-right", 16);
}

#[test]
fn two_var_ambiguous_disagree_matches_recorded_oracle_after_confirmation() {
    assert_confirmed_ambiguous_fixture("alpha-variable-name-collision", 2);
}

fn assert_confirmed_ambiguous_fixture(name: &str, expected_words: usize) {
    use crate::analyzer::FomaProposer;
    use crate::compose_budget::ApplyBudget;
    use crate::composite::{FomaAnalyzer, ProfiledFomaApplyOutcome};
    use foma::lexcread::fsm_lexc_parse_string;
    use foma::minimize::fsm_minimize;

    let fixture = pg_conformance_fixtures::require_fixture("edge-cases", name);
    let g = pg_grammar::load(&fixture.load_grammar_xml()).unwrap();
    let rule = rewrite_rule_by_xml_id(&g, "prDoubleAlpha");
    let opts = FomaOptions::default();
    let (rewrite, _) = compile_rewrite_rule_subset(&opts, &g, rule, &|_| true)
        .expect("recorded ambiguous disagreement must compile");
    let table = owning_table(&g, rule).unwrap();
    let alphabet = SegAlphabet::new(table);
    let emitted = crate::uflexc::emit_underlying_filtered(&g, &alphabet, None).unwrap();
    assert!(emitted.skipped.is_empty());
    let lexicon = fsm_lexc_parse_string(&opts, None, &emitted.lexc_source).unwrap();
    let network = fsm_minimize(&opts, fsm_compose(&opts, lexicon, rewrite));
    let proposer = FomaProposer::from_precompiled_network_without_emit_report(&network)
        .with_segment_query_encoder(table);
    let mut analyzer = FomaAnalyzer::from_precompiled_proposer(&g, proposer);
    let budget = ApplyBudget::with_caps(Some(128), Some(32));
    let mut positives = 0;
    let mut pruned = 0;
    let mut checked = 0;
    for word in fixture.load_words_yaml().words {
        assert!(word.adapter_visible() && !word.expect_skip);
        let ProfiledFomaApplyOutcome::Complete(profiled) =
            analyzer.analyze_word_with_diagnostics_budgeted(&word.word, &budget)
        else {
            panic!("{} exceeded the apply budget", word.word)
        };
        let outcome = profiled.outcome;
        assert!(outcome.peel_chain_depth_error.is_none());
        if outcome.candidates_generated > 0 {
            assert!(
                profiled.diagnostics.confirmation_calls > 0,
                "{} bypassed HC confirmation",
                word.word
            );
        }
        assert_eq!(
            pg_parse::result_multiset(&outcome.analyses),
            word.expected_multiset(),
            "{name}: {}",
            word.word
        );
        positives += usize::from(!outcome.analyses.is_empty());
        pruned += usize::from(outcome.candidates_generated > 0 && outcome.confirmed == 0);
        checked += 1;
    }
    assert_eq!(checked, expected_words);
    assert_eq!(positives, 1);
    assert!(pruned > 0);
}

#[test]
fn ambiguous_disagreement_with_partial_class_matches_recorded_oracle_after_confirmation() {
    for name in [
        "partial-class-disagree-bounded-rtl-right",
        "partial-class-disagree-unbounded-rtl-left",
        "partial-class-disagree-bounded-ltr-left",
        "partial-class-disagree-bounded-rtl-left",
        "partial-class-disagree-unbounded-ltr-right",
        "partial-class-disagree-unbounded-rtl-right",
    ] {
        assert_confirmed_ambiguous_fixture(name, 16);
    }
}

#[test]
fn ambiguous_disagreement_still_refuses_unsupported_class_shapes() {
    let fixture = pg_conformance_fixtures::require_fixture(
        "edge-cases",
        "partial-class-disagree-bounded-rtl-right",
    );
    let xml = fixture.load_grammar_xml();
    let class_start = xml
        .find("<SegmentNaturalClass id=\"ncVowel\">")
        .expect("fixture has its explicit vowel class");
    let class_end = xml[class_start..]
        .find("</SegmentNaturalClass>")
        .map(|offset| class_start + offset + "</SegmentNaturalClass>".len())
        .expect("explicit vowel class closes");
    let feature_class = "<FeatureNaturalClass id=\"ncVowel\"><Name>vowels</Name>\
         <FeatureValue feature=\"featRound\" symbolValues=\"rdMinus\" /></FeatureNaturalClass>";
    let feature_xml = format!(
        "{}{}{}",
        &xml[..class_start],
        feature_class,
        &xml[class_end..]
    );

    let nonbinary_xml = xml.replacen(
        "</Symbols>",
        "<Symbol id=\"bkThird\">third</Symbol></Symbols>",
        1,
    );
    assert_ne!(xml, nonbinary_xml, "nonbinary control adds a feature value");

    let underspecified_xml = xml.replacen(
        "<FeatureValue feature=\"featRound\" symbolValues=\"rdMinus\" />",
        "",
        2,
    );
    assert_ne!(
        xml, underspecified_xml,
        "underspecified control removes governed values"
    );

    for (label, mutated) in [
        ("feature class", feature_xml),
        ("nonbinary governed feature", nonbinary_xml),
        ("underspecified governed value", underspecified_xml),
    ] {
        let grammar = pg_grammar::load(&mutated).unwrap_or_else(|error| {
            panic!("{label} control must remain a loadable grammar: {error}")
        });
        let rule = rewrite_rule_by_xml_id(&grammar, "prDoubleAlpha");
        assert!(!rewrite_rule_is_lowerable(&grammar, rule), "{label}");
        assert!(
            compile_rewrite_rule_subset(&FomaOptions::default(), &grammar, rule, &|_| true)
                .is_none(),
            "{label}"
        );
        assert_eq!(
            crate::lower::diagnose_unsupported(
                &grammar,
                owning_table(&grammar, rule).unwrap(),
                &rule.subrules[0].rhs,
                crate::lower::PatternLowerScope::RewriteRuleCompile,
            ),
            crate::lower::UnsupportedPatternNode::AlphaAmbiguousDisagree,
            "{label}"
        );
    }
}
