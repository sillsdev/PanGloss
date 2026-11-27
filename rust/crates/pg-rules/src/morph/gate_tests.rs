//! Telemetry-only regressions; these synthetic probes assert observed checks, not C# parse results.

use super::*;
use crate::cache::RuleCache;
use crate::stats::{Direction as StatsDirection, GateFeature, StatsCollector};
use crate::trace::{TraceSink, TreeTraceSink};

const REAL_XML: &str = r#"<HermitCrabInput><Language><Name>PruneCounterFixture</Name>
  <HeadFeatures><SymbolicFeature id="tense"><Name>Tense</Name><Symbols><Symbol id="past">Past</Symbol><Symbol id="present">Present</Symbol></Symbols></SymbolicFeature></HeadFeatures><PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cs" /><Segment segment="ct" /></SegmentNaturalClass></NaturalClasses>
  <Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrOrd mrPresent">
    <Name>S</Name>
    <MorphologicalRuleDefinitions>
      <RealizationalRule id="mrOrd"><Name>ordSuffix</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="subOrd">
          <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      <RealizationalFeatures><FeatureValue feature="tense" symbolValues="past" /></RealizationalFeatures></RealizationalRule>

    <RealizationalRule id="mrPresent"><Name>ordSuffix</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="subPresent">
          <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      <RealizationalFeatures><FeatureValue feature="tense" symbolValues="present" /></RealizationalFeatures></RealizationalRule></MorphologicalRuleDefinitions>

    <LexicalEntries><LexicalEntry id="e1" partOfSpeech="posV"><Allomorphs>
      <Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph>
    </Allomorphs></LexicalEntry></LexicalEntries>
  </Stratum></Strata>
</Language></HermitCrabInput>"#;

const INVERSE_XML: &str = r#"<HermitCrabInput><Language><Name>PruneCounterFixture</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech><PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cs" /><Segment segment="ct" /></SegmentNaturalClass></NaturalClasses>
  <Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrOrd mrTpl">
    <Name>S</Name>
    <MorphologicalRuleDefinitions>
      <MorphologicalRule id="mrOrd" requiredPartsOfSpeech="posV" outputPartOfSpeech="posN"><Name>ordSuffix</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="subOrd">
          <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
      <MorphologicalRule id="mrTpl" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>tplSuffix</Name>
        <MorphologicalSubrules><MorphologicalSubrule id="subTpl">
          <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
          <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>t</PhoneticShape></InsertSegments></MorphologicalOutput>
        </MorphologicalSubrule></MorphologicalSubrules>
      </MorphologicalRule>
    </MorphologicalRuleDefinitions>

    <LexicalEntries><LexicalEntry id="e1" partOfSpeech="posV"><Allomorphs>
      <Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph>
    </Allomorphs></LexicalEntry></LexicalEntries>
  </Stratum></Strata>
</Language></HermitCrabInput>"#;

fn analyze_path(g: &Grammar, word: &Word, stats: &StatsCollector, path: usize, id: MRuleId) {
    let rule = &g.mrules[id.0 as usize];
    let ctx = Some(MRuleStatsCtx {
        stats,
        stratum: StratumId(0),
        id,
        direction: StatsDirection::Analysis,
    });
    let cache = RuleCache::build(g);
    let outputs = match path {
        0 => analyze_stats(g, word, rule, ctx, false),
        1 => analyze_cached(g, id, word, rule, &cache, ctx, false, None),
        2 => {
            let trace = TreeTraceSink::new();
            let parent = trace.analyze_word(word);
            let outputs =
                analyze_cached_traced(g, id, word, rule, &cache, ctx, &trace, parent, false, None);
            assert!(trace.is_tracing());
            outputs
        }
        _ => unreachable!(),
    };
    assert!(outputs.is_empty(), "the input has no matching suffix");
}

#[test]
fn realizational_analysis_observes_exact_pass_and_block_in_every_path() {
    let g = pg_grammar::load(REAL_XML).unwrap();
    let MorphRuleDef::Realizational(rule) = &g.mrules[0] else {
        panic!("real rule")
    };
    let MorphRuleDef::Realizational(conflict) = &g.mrules[1] else {
        panic!("conflict rule")
    };
    for path in 0..3 {
        for compatible in [false, true] {
            let mut word = Word::new(ShapeBuilder::new().freeze(), StratumId(0));
            word.real_fs = g
                .fs_interner
                .get(if compatible {
                    rule.real_fs
                } else {
                    conflict.real_fs
                })
                .clone();
            let stats = StatsCollector::new(&g);
            analyze_path(&g, &word, &stats, path, MRuleId(0));
            let rows = stats.gate_rows();
            assert_eq!(rows.len(), 1, "path={path}, compatible={compatible}");
            assert_eq!(
                (rows[0].passed, rows[0].blocked),
                (u64::from(compatible), u64::from(!compatible))
            );
        }
    }
}

fn inverse_grammar(output_compatible: bool, compound: bool, output_only: bool) -> Grammar {
    let mut xml = INVERSE_XML.to_string();
    if output_compatible {
        xml = xml.replace("outputPartOfSpeech=\"posN\"", "outputPartOfSpeech=\"posV\"");
    }
    if output_only {
        xml = xml.replace("requiredPartsOfSpeech=\"posV\"", "");
    }
    let mut g = pg_grammar::load(&xml).unwrap();
    if compound {
        let MorphRuleDef::AffixProcess(rule) = &g.mrules[0] else {
            panic!("affix rule")
        };
        g.mrules.push(MorphRuleDef::Compounding(CompoundingRuleDef {
            xml_id: "compound".into(),
            source_guid: None,
            name: None,
            blockable: false,
            max_apps: 1,
            head_required_syn_fs: rule.required_syn_fs,
            non_head_required_syn_fs: pg_featstruct::FsId(0),
            out_syn_fs: rule.out_syn_fs,
            head_prod_restrictions_mpr: pg_grammar_model::model::MprSet::EMPTY,
            non_head_prod_restrictions_mpr: pg_grammar_model::model::MprSet::EMPTY,
            output_prod_restrictions_mpr: pg_grammar_model::model::MprSet::EMPTY,
            obligatory_features: vec![],
            subrules: vec![],
        }));
        g.strata[0].mrules.push(MRuleId(2));
    }
    g
}

#[test]
fn inverse_checks_identify_output_contribution_in_affix_and_compound_paths() {
    for path in 0..3 {
        for compound in [false, true] {
            for output_only in [false, true] {
                let mut identities = Vec::new();
                for compatible in [false, true] {
                    let g = inverse_grammar(compatible, compound, output_only);
                    let mut word = Word::new(ShapeBuilder::new().freeze(), StratumId(0));
                    let MorphRuleDef::AffixProcess(outer) = &g.mrules[1] else {
                        panic!("outer rule")
                    };
                    word.syn_fs = g.fs_interner.get(outer.out_syn_fs).clone();
                    let stats = StatsCollector::new(&g);
                    analyze_path(
                        &g,
                        &word,
                        &stats,
                        path,
                        MRuleId(if compound { 2 } else { 0 }),
                    );
                    let rows = stats.gate_rows();
                    assert_eq!(
                        rows.len(),
                        1,
                        "path={path}, compound={compound}, output_only={output_only}"
                    );
                    assert_eq!(
                        (rows[0].passed, rows[0].blocked),
                        (u64::from(compatible), u64::from(!compatible))
                    );
                    let feature = |id| g.fs_interner.get(pg_featstruct::FsId(id)).clone();
                    identities.push(match rows[0].feature {
                        GateFeature::InverseSyntactic(required, output) => {
                            vec![feature(required), feature(output)]
                        }
                        GateFeature::Syntactic(required) => vec![feature(required)],
                        other => panic!("unexpected inverse predicate: {other:?}"),
                    });
                }
                assert_ne!(
                    identities[0], identities[1],
                    "output changes must change predicate identity"
                );
            }
        }
    }
}

#[test]
fn forward_and_inverse_comparisons_are_not_aggregated_as_one_predicate() {
    let g = inverse_grammar(false, false, false);
    let MorphRuleDef::AffixProcess(outer) = &g.mrules[1] else {
        panic!("outer rule")
    };
    let mut word = Word::new(ShapeBuilder::new().freeze(), StratumId(0));
    word.syn_fs = g.fs_interner.get(outer.out_syn_fs).clone();
    let stats = StatsCollector::new(&g);
    analyze_path(&g, &word, &stats, 1, MRuleId(0));
    let ctx = Some(MRuleStatsCtx {
        stats: &stats,
        stratum: StratumId(0),
        id: MRuleId(0),
        direction: StatsDirection::Synthesis,
    });
    synthesize_stats(&g, &word, &g.mrules[0], ctx);
    let rows = stats.gate_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().map(|r| r.passed).sum::<u64>(), 1);
    assert_eq!(rows.iter().map(|r| r.blocked).sum::<u64>(), 1);
}

#[test]
fn realizational_unification_and_subsumption_remain_distinct() {
    let g = pg_grammar::load(REAL_XML).unwrap();
    let MorphRuleDef::Realizational(rule) = &g.mrules[0] else {
        panic!("real rule")
    };
    let mut word = Word::new(ShapeBuilder::new().freeze(), StratumId(0));
    word.real_fs = g.fs_interner.get(rule.real_fs).clone();
    let stats = StatsCollector::new(&g);
    analyze_path(&g, &word, &stats, 0, MRuleId(0));
    let ctx = Some(MRuleStatsCtx {
        stats: &stats,
        stratum: StratumId(0),
        id: MRuleId(0),
        direction: StatsDirection::Synthesis,
    });
    synthesize_stats(&g, &word, &g.mrules[0], ctx);
    let rows = stats.gate_rows();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| (row.passed, row.blocked) == (1, 0)));
    assert_ne!(rows[0].feature, rows[1].feature);
}

#[test]
fn inverse_admission_matches_materialization_with_and_without_observation() {
    let g = pg_grammar::load(INVERSE_XML).unwrap();
    let mut outcomes = [0, 0];
    for req in 0..g.fs_interner.len() {
        for out in 0..g.fs_interner.len() {
            for have in 0..g.fs_interner.len() {
                let req = pg_featstruct::FsId(req as u32);
                let out = pg_featstruct::FsId(out as u32);
                let mut word = Word::new(ShapeBuilder::new().freeze(), StratumId(0));
                word.syn_fs = g.fs_interner.get(pg_featstruct::FsId(have as u32)).clone();
                let admitted = ana_syn_gate(&g, req, out, &word, None);
                assert_eq!(admitted, ana_syn_fs(&g, req, out, &word, None).is_some());
                outcomes[usize::from(admitted)] += 1;
                let gate_stats = StatsCollector::new(&g);
                let full_stats = StatsCollector::new(&g);
                let ctx = |stats| {
                    Some(MRuleStatsCtx {
                        stats,
                        stratum: StratumId(0),
                        id: MRuleId(0),
                        direction: StatsDirection::Analysis,
                    })
                };
                assert_eq!(
                    admitted,
                    ana_syn_gate(&g, req, out, &word, ctx(&gate_stats))
                );
                assert_eq!(
                    admitted,
                    ana_syn_fs(&g, req, out, &word, ctx(&full_stats)).is_some()
                );
                assert_eq!(gate_stats.gate_rows(), full_stats.gate_rows());
                let expected = u64::from(
                    !g.fs_interner.get(req).is_empty() || !g.fs_interner.get(out).is_empty(),
                );
                assert_eq!(
                    gate_stats
                        .gate_rows()
                        .iter()
                        .map(|r| r.passed + r.blocked)
                        .sum::<u64>(),
                    expected
                );
            }
        }
    }
    assert!(outcomes.iter().all(|&count| count > 0));
}

mod left_edge_copy_guard {
    use super::*;
    use pg_grammar_model::model::NatClassId;

    fn grammar() -> Grammar {
        pg_grammar::load(REAL_XML).expect("fixture loads")
    }

    fn ctx() -> SimpleContext {
        SimpleContext {
            nat_class: NatClassId(0),
            vars: Vec::new(),
        }
    }

    fn one_or_more() -> Pattern {
        Pattern {
            nodes: vec![PatternNode::Quantifier {
                min: 1,
                max: None,
                children: vec![PatternNode::Context(ctx())],
            }],
        }
    }

    fn zero_or_more() -> Pattern {
        Pattern {
            nodes: vec![PatternNode::Quantifier {
                min: 0,
                max: None,
                children: vec![PatternNode::Context(ctx())],
            }],
        }
    }

    fn subrule(non_head_lhs: Vec<Pattern>, rhs: Vec<OutputAction>) -> CompoundingSubruleDef {
        CompoundingSubruleDef {
            vars: Default::default(),
            required_mpr: Default::default(),
            excluded_mpr: Default::default(),
            out_mpr: Default::default(),
            head_lhs: vec![one_or_more()],
            non_head_lhs,
            rhs,
        }
    }

    fn guard(sr: &CompoundingSubruleDef) -> bool {
        let g = grammar();
        let table = TableId(0);
        let (_, lhs) = build_ana_compound_lhs(&g, table, sr).expect("analysis lhs builds");
        non_head_is_left_edge_copy(sr, &lhs, &g, table)
    }

    fn nh_copy() -> OutputAction {
        OutputAction::Copy(PartRef::NonHead(0))
    }

    fn head_copy() -> OutputAction {
        OutputAction::Copy(PartRef::Head(0))
    }

    #[test]
    fn a_non_head_copied_first_qualifies() {
        assert!(guard(&subrule(
            vec![one_or_more()],
            vec![nh_copy(), head_copy()]
        )));
    }

    #[test]
    fn a_head_first_subrule_does_not() {
        assert!(!guard(&subrule(
            vec![one_or_more()],
            vec![head_copy(), nh_copy()]
        )));
    }

    #[test]
    fn a_modified_non_head_does_not() {
        assert!(!guard(&subrule(
            vec![one_or_more()],
            vec![
                OutputAction::Modify(PartRef::NonHead(0), ctx()),
                head_copy()
            ]
        )));
    }

    #[test]
    fn a_multi_part_non_head_does_not() {
        assert!(!guard(&subrule(
            vec![one_or_more(), one_or_more()],
            vec![nh_copy(), head_copy()]
        )));
    }

    #[test]
    fn a_non_head_that_can_match_nothing_does_not() {
        assert!(!guard(&subrule(
            vec![zero_or_more()],
            vec![nh_copy(), head_copy()]
        )));
    }
}
