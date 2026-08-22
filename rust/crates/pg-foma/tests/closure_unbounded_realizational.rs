//! Fail-closed gate for an unbounded realizational rule entering eager composite closure.

use std::path::PathBuf;

use pg_foma::analyzer::{FomaError, FomaProposer};
use pg_foma::emit::{self, ClosureFallbackBackend, ClosureRefusalCode, FomaTier};
use pg_foma::replace::SegAlphabet;
use pg_parse::{Morpher, ParseOptions};

const CONCATENATIVE_REALIZATIONAL_XML: &str = r#"<HermitCrabInput><Language><Name>LoopableRealizational</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>
    <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions></CharacterDefinitionTable>
  <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
  <Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="rr1"><Name>Main</Name>
    <MorphologicalRuleDefinitions><RealizationalRule id="rr1"><Name>repeatable</Name>
      <MorphologicalSubrules><MorphologicalSubrule id="sr1">
        <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
        <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>f</PhoneticShape></InsertSegments></MorphologicalOutput>
      </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>REAL</MorphemeId>
    </RealizationalRule></MorphologicalRuleDefinitions>
    <LexicalEntries><LexicalEntry id="e1" partOfSpeech="posV"><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs><MorphemeId>ROOT</MorphemeId></LexicalEntry></LexicalEntries>
  </Stratum></Strata>
</Language></HermitCrabInput>"#;

fn tagged_self_loops(lexc_source: &str) -> Vec<String> {
    let mut lexicon = "";
    let mut loops = Vec::new();
    for line in lexc_source.lines() {
        if let Some(name) = line.strip_prefix("LEXICON ") {
            lexicon = name;
            continue;
        }
        let Some(body) = line.strip_suffix(" ;") else {
            continue;
        };
        let Some((mapping, target)) = body.rsplit_once(' ') else {
            continue;
        };
        if target == lexicon && mapping.contains(':') {
            loops.push(format!("{lexicon}: {line}"));
        }
    }
    loops
}

#[test]
fn concatenative_realizational_rule_uses_a_regular_loop() {
    let grammar = pg_grammar::load(CONCATENATIVE_REALIZATIONAL_XML)
        .expect("concatenative realizational fixture must load");
    let oracle = Morpher::new(&grammar, 20_000).parse_word_opts("afffff", &ParseOptions::default());
    assert!(
        !oracle.structured.is_empty(),
        "oracle must apply rr1 five times"
    );
    let mut proposer = FomaProposer::new(&grammar).expect("regular rule must compile");
    assert!(
        !proposer.propose("afffff").is_empty(),
        "the FST loop must propose a surface requiring five applications"
    );
}

#[test]
fn concatenative_realizational_prefix_uses_a_regular_loop() {
    let xml = CONCATENATIVE_REALIZATIONAL_XML.replace(
        "<CopyFromInput index=\"stem\" /><InsertSegments><PhoneticShape>f</PhoneticShape></InsertSegments>",
        "<InsertSegments><PhoneticShape>f</PhoneticShape></InsertSegments><CopyFromInput index=\"stem\" />",
    );
    let grammar = pg_grammar::load(&xml).expect("concatenative prefix fixture must load");
    let oracle = Morpher::new(&grammar, 20_000).parse_word_opts("fffffa", &ParseOptions::default());
    assert!(
        !oracle.structured.is_empty(),
        "oracle must apply rr1 five times"
    );
    let mut proposer = FomaProposer::new(&grammar).expect("regular prefix rule must compile");
    assert!(
        !proposer.propose("fffffa").is_empty(),
        "the FST loop must propose a prefix surface requiring five applications"
    );
}

#[test]
fn boundary_only_realizational_output_does_not_form_an_epsilon_self_loop() {
    let xml = CONCATENATIVE_REALIZATIONAL_XML
        .replace(
            "</SegmentDefinitions>",
            "</SegmentDefinitions><BoundaryDefinitions><BoundaryDefinition id=\"bNull\"><Representations><Representation>^0</Representation></Representations></BoundaryDefinition></BoundaryDefinitions>",
        )
        .replace("<PhoneticShape>f</PhoneticShape>", "<PhoneticShape>^0</PhoneticShape>");
    let grammar = pg_grammar::load(&xml).expect("boundary-only fixture must load");
    let result = emit::emit(&grammar);
    assert!(!matches!(result.report.tier, FomaTier::Unsupported { .. }));
    assert!(
        tagged_self_loops(&result.lexc_source).is_empty(),
        "surface emission formed a tagged self-loop for boundary-only output: {:?}",
        tagged_self_loops(&result.lexc_source)
    );
    FomaProposer::new(&grammar).expect("boundary-only non-looping rule must still compile");

    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let underlying = emit::emit_underlying_templated(&grammar, &alphabet, None);
    assert!(
        tagged_self_loops(&underlying.lexc_source).is_empty(),
        "underlying emission formed a tagged self-loop whose boundary tokens clean up to epsilon: {:?}",
        tagged_self_loops(&underlying.lexc_source)
    );
}

#[test]
fn unbounded_realizational_composite_route_returns_no_artifact() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pangloss/fst-completeness/late-structural-anchor-five-rule-chain/grammar.xml");
    let xml = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let realizational = r#"
      <RealizationalRule id="rrUnbounded">
        <Name>unbounded-realizational-circumfix</Name>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="srUnbounded">
            <MorphologicalInput><PhoneticSequence id="stemR"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>b</PhoneticShape></InsertSegments><CopyFromInput index="stemR" /><InsertSegments><PhoneticShape>c</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </RealizationalRule>
    </MorphologicalRuleDefinitions>"#;
    let xml = xml.replacen("</MorphologicalRuleDefinitions>", realizational, 1);
    let xml = xml.replacen(
        "morphologicalRules=\"mr1 mr2 mr3 mr4 mr5\"",
        "morphologicalRules=\"mr1 mr2 mr3 mr4 mr5 rrUnbounded\"",
        1,
    );
    let grammar = pg_grammar::load(&xml).expect("fixture with realizational rule must load");

    let result = emit::emit(&grammar);
    let reason = match &result.report.tier {
        FomaTier::Unsupported { reason } => reason,
        tier => panic!("unbounded composite closure must refuse, got {tier:?}"),
    };
    assert!(
        reason.contains("cannot prove finite closure") && reason.contains("RealizationalRule"),
        "refusal must identify the unbounded cause: {reason}"
    );
    assert!(
        result.lexc_source.is_empty(),
        "refusal must return no FST source"
    );
    assert!(result.report.enum_budget_exceeded.is_none());
    let refusal = result
        .report
        .closure_refusal
        .as_ref()
        .expect("unbounded closure refusal must be structured");
    assert_eq!(refusal.code, ClosureRefusalCode::UnboundedRuleApplication);
    assert_eq!(refusal.affected_rule_ordinals, vec![5]);
    assert_eq!(refusal.depth_limit, None);
    assert_eq!(refusal.pending_successors, None);
    assert_eq!(
        refusal.remedy_backend,
        ClosureFallbackBackend::FullMorphologicalParser
    );
    assert!(matches!(
        FomaProposer::new(&grammar),
        Err(FomaError::Unsupported(_))
    ));
}

#[test]
fn unreferenced_realizational_definition_does_not_refuse_closure() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pangloss/fst-completeness/late-structural-anchor-five-rule-chain/grammar.xml");
    let xml = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let dead_definition = r#"
      <RealizationalRule id="rrDead">
        <Name>unreferenced</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="srDead">
          <MorphologicalInput><PhoneticSequence id="stemDead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="stemDead" /><InsertSegments><PhoneticShape>b</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </RealizationalRule>
    </MorphologicalRuleDefinitions>"#;
    let grammar =
        pg_grammar::load(&xml.replacen("</MorphologicalRuleDefinitions>", dead_definition, 1))
            .expect("fixture with an unreferenced definition must load");

    let result = emit::emit(&grammar);
    assert!(
        !matches!(
            result.report.closure_refusal.as_ref().map(|r| r.code),
            Some(ClosureRefusalCode::UnboundedRuleApplication)
        ),
        "an unreferenced rule definition does not participate in eager closure"
    );
}

#[test]
fn excessive_bounded_chain_returns_no_partial_artifact() {
    let xml = r#"<HermitCrabInput><Language><Name>DeepFiniteClosure</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>
        <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cg"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions></CharacterDefinitionTable>
      <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
      <Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mr1"><Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mr1" multipleApplication="65" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>deep</Name>
            <MorphologicalSubrules><MorphologicalSubrule id="s1">
              <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
              <MorphologicalOutput><InsertSegments><PhoneticShape>f</PhoneticShape></InsertSegments><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>g</PhoneticShape></InsertSegments></MorphologicalOutput>
            </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>R1</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries><LexicalEntry id="e1" partOfSpeech="posV"><Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs><MorphemeId>ROOT</MorphemeId></LexicalEntry></LexicalEntries>
      </Stratum></Strata>
    </Language></HermitCrabInput>"#;
    let grammar = pg_grammar::load(xml).expect("deep finite fixture must load");

    let result = emit::emit(&grammar);
    let reason = match &result.report.tier {
        FomaTier::Unsupported { reason } => reason,
        tier => panic!("resource-envelope breach must refuse, got {tier:?}"),
    };
    assert!(
        reason.contains("resource envelope") && reason.contains("live successor"),
        "refusal must identify incomplete resource-bounded closure: {reason}"
    );
    assert!(
        result.lexc_source.is_empty(),
        "refusal must return no FST source"
    );
    assert!(result.report.enum_budget_exceeded.is_none());
    let refusal = result
        .report
        .closure_refusal
        .as_ref()
        .expect("depth refusal must be structured");
    assert_eq!(refusal.code, ClosureRefusalCode::DepthBudgetExceeded);
    assert_eq!(refusal.affected_rule_ordinals, vec![0]);
    assert_eq!(refusal.depth_limit, Some(64));
    assert!(refusal
        .pending_successors
        .is_some_and(|pending| pending > 0));
}
