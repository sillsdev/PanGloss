use super::*;
use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::TemplateId;

const XML: &str = r#"
<HermitCrabInput>
  <Language>
    <Name>TemplateAnalysisGate</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name>
        <Segment segment="cM" /><Segment segment="cI" /><Segment segment="cD" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrD" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN">
            <Name>d suffix</Name><MorphemeId>D</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subD">
                <MorphologicalInput>
                  <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>d</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <AffixTemplates>
          <AffixTemplate requiredPartsOfSpeech="posV">
            <Name>optional V template</Name>
            <Slot morphologicalRules="mrD" optional="true"><Name>suffix</Name></Slot>
          </AffixTemplate>
          <AffixTemplate requiredPartsOfSpeech="posN">
            <Name>consuming N template</Name>
            <Slot morphologicalRules="mrD"><Name>suffix</Name></Slot>
          </AffixTemplate>
        </AffixTemplates>
        <LexicalEntries>
          <LexicalEntry id="eN" partOfSpeech="posN"><MorphemeId>N</MorphemeId>
            <Allomorphs><Allomorph id="aN"><PhoneticShape>mi</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
          <LexicalEntry id="eV" partOfSpeech="posV"><MorphemeId>V</MorphemeId>
            <Allomorphs><Allomorph id="aV"><PhoneticShape>mi</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn grammar() -> Grammar {
    pg_grammar::load(XML).expect("template-analysis grammar loads")
}

fn word(g: &Grammar, text: &str) -> Word {
    let shape = pg_grammar_model::segment::segment(&g.char_tables[0], text).expect("word segments");
    Word::new(shape, StratumId(0))
}

#[test]
fn edge_index_falls_back_for_optional_and_unknown_intermediate_edges() {
    use pg_shape::{NodeFlags, NodeKind, ShapeBuilder, NO_CHAR_DEF};

    let g = grammar();
    let cache = RuleCache::build(&g);
    let id = MRuleId(0);
    let plain = word(&g, "mi").shape;
    let matching = word(&g, "mid").shape;
    let table = pg_grammar_model::model::TableId(0);
    let rejects = |shape: &pg_shape::Shape| {
        cache
            .edge_candidates(table, shape)
            .is_some_and(|c| c.rejects(id))
    };
    assert!(rejects(&plain));
    assert!(!rejects(&matching));
    let budget = StepBudget::new(100);
    let _scope = budget.enter();
    let analyzer = StratumAnalyzer::new(
        &g,
        StratumId(0),
        AnalyzerConfig::default(),
        None,
        None,
        Some(&cache),
        &budget,
        FinalTemplateAnalysisPolicy::default(),
        None,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    );
    let input = word(&g, "mi");
    let candidates = analyzer.edge_candidates(&input, [id].into_iter()).unwrap();
    assert!(analyzer
        .apply_one_mrule(id, &input, RuleInvocationRole::Ordinary, Some(&candidates))
        .is_empty());
    assert!(analyzer
        .apply_one_mrule(id, &input, RuleInvocationRole::Ordinary, Some(&candidates))
        .is_empty());
    assert!(analyzer
        .edge_candidates(&word(&g, "mid"), [id].into_iter())
        .is_none());
    assert_eq!(budget.work_steps(), 1);
    let (segments, _) = morph::segs_of(&g, pg_grammar_model::model::TableId(0), &plain, false);
    let allos = g.mrules[0].affix_allomorphs().unwrap();
    assert!(allos.iter().all(|allo| cache
        .allomorph(allo.id)
        .ana_lhs
        .as_ref()
        .unwrap()
        .1
        .edge_rejects(&segments)));

    let last = plain.len() - 2;
    let lanes = plain.node_lanes(last).to_vec();
    let mut optional = ShapeBuilder::from_shape(&plain);
    optional.delete(last);
    optional.insert(
        last,
        NodeKind::Segment,
        plain.char_def(last),
        NodeFlags(NodeFlags::OPTIONAL),
        &lanes,
    );
    assert!(!rejects(&optional.freeze()));

    let mut unknown = ShapeBuilder::from_shape(&plain);
    unknown.delete(last);
    unknown.insert(
        last,
        NodeKind::Segment,
        NO_CHAR_DEF,
        NodeFlags::EMPTY,
        &lanes,
    );
    assert!(!rejects(&unknown.freeze()));
}

#[test]
fn syntactic_rejection_does_not_charge_the_edge_index() {
    let g = grammar();
    let cache = RuleCache::build(&g);
    let id = MRuleId(0);
    let mut input = word(&g, "mi");
    input.syn_fs = g.fs_interner.get(g.entries[1].syn_fs).clone();
    let MorphRuleDef::AffixProcess(rule) = &g.mrules[id.0 as usize] else {
        panic!("fixture must contain an affix rule");
    };
    assert!(morph::ana_syn_fs(&g, rule.required_syn_fs, rule.out_syn_fs, &input).is_none());
    let budget = StepBudget::new(100);
    let _scope = budget.enter();
    let analyzer = StratumAnalyzer::new(
        &g,
        StratumId(0),
        AnalyzerConfig::default(),
        None,
        None,
        Some(&cache),
        &budget,
        FinalTemplateAnalysisPolicy::default(),
        None,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    );
    let candidates = analyzer.edge_candidates(&input, [id].into_iter()).unwrap();
    assert!(candidates.rejects(id));
    assert!(analyzer
        .apply_one_mrule(id, &input, RuleInvocationRole::Ordinary, Some(&candidates))
        .is_empty());
    assert_eq!(budget.work_steps(), 1);
    assert_eq!(budget.steps(), 1);
    assert!(
        candidates.charge_needed(),
        "syntactic rejection must not charge the index"
    );
}

#[test]
fn an_undecidable_allomorph_keeps_the_whole_rule() {
    let second = r#"</MorphologicalSubrule>
      <MorphologicalSubrule id="subUnknown">
        <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
        <MorphologicalOutput><CopyFromInput index="stem" /></MorphologicalOutput>
      </MorphologicalSubrule>"#;
    let xml = XML.replacen("</MorphologicalSubrule>", second, 1);
    let g = pg_grammar::load(&xml).unwrap();
    let cache = RuleCache::build(&g);
    assert!(cache
        .edge_candidates(pg_grammar_model::model::TableId(0), &word(&g, "mi").shape)
        .is_none_or(|c| !c.rejects(MRuleId(0))));
}

fn analyze_template(g: &Grammar, template: TemplateId, input: &Word) -> Vec<Word> {
    let budget = StepBudget::new(usize::MAX);
    StratumAnalyzer::new(
        g,
        StratumId(0),
        AnalyzerConfig::default(),
        None,
        None,
        None,
        &budget,
        FinalTemplateAnalysisPolicy::default(),
        None,
        &crate::trace::NoopSink,
        TraceHandle::DUMMY,
    )
    .analyze_template(template, input)
}

#[test]
fn optional_template_absent_suffix_does_not_add_template_requirement() {
    let g = grammar();
    let input = word(&g, "mi");
    let output = analyze_template(&g, TemplateId(0), &input);

    assert_eq!(output.len(), 1);
    assert_eq!(output[0].shape, input.shape);
    assert_eq!(output[0].syn_fs, FeatureStruct::EMPTY);
}

#[test]
fn disjoint_template_input_is_rejected() {
    let g = grammar();
    let mut input = word(&g, "mi");
    input.syn_fs = g.fs_interner.get(g.entries[0].syn_fs).clone();

    let output = analyze_template(&g, TemplateId(0), &input);

    assert!(output.is_empty());
}

#[test]
fn consumed_slot_preserves_rule_analysis_features() {
    let g = grammar();
    let mut input = word(&g, "mid");
    input.syn_fs = g.fs_interner.get(g.entries[0].syn_fs).clone();
    let direct = morph::analyze(&g, &input, &g.mrules[0]);
    let output = analyze_template(&g, TemplateId(1), &input);

    assert_eq!(direct.len(), 1);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].shape, direct[0].shape);
    assert_eq!(
        direct[0].syn_fs,
        g.fs_interner.get(g.entries[1].syn_fs).clone()
    );
    assert_eq!(
        output[0].syn_fs,
        g.fs_interner.get(g.entries[1].syn_fs).clone()
    );
}
