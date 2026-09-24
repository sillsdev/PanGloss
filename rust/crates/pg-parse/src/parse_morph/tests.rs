use super::*;
use crate::{AnalysisProvenance, MorphOccurrence};
use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::{MorphemeInfo, MprSet, SourceMorphPlacement, StratumId};

fn grammar(msa: Option<&str>, infl_type: Option<&str>, forms: Vec<Vec<Option<&str>>>) -> Grammar {
    const XML: &str = r#"<HermitCrabInput><Language>
          <Name>ParseMorphProjection</Name>
          <PartsOfSpeech><PartOfSpeech id="pos"><Name>POS</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="table"><Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
        </Language></HermitCrabInput>"#;
    let mut grammar = pg_grammar::load(XML).expect("projection grammar loads");
    grammar.morphemes.push(MorphemeInfo {
        xml_key: "morpheme-key".to_string(),
        source_msa_guid: msa.map(str::to_string),
        source_msa_class: None,
        source_infl_type_guid: infl_type.map(str::to_string),
        morph_id: None,
        gloss: None,
        stratum: StratumId(0),
        properties: Vec::new(),
        co_occurrence: Vec::new(),
    });
    grammar.allomorph_sources = forms
        .into_iter()
        .map(|form_guids| pg_grammar_model::model::AllomorphSource {
            form_guids: form_guids
                .into_iter()
                .map(|guid| guid.map(str::to_string))
                .collect(),
            omitted: false,
            placement: SourceMorphPlacement::Append,
        })
        .collect();
    grammar
}

fn analysis(occurrences: Vec<MorphOccurrence>, guessed_string: Option<&str>) -> WordAnalysis {
    WordAnalysis {
        morpheme_ids: occurrences
            .iter()
            .map(|occurrence| occurrence.morpheme_id)
            .collect(),
        morph_occurrences: occurrences,
        root_morpheme_index: 0,
        pos_id: None,
        syn_fs: FeatureStruct::EMPTY,
        mpr: MprSet::EMPTY,
        guessed: guessed_string.is_some(),
        guessed_string: guessed_string.map(str::to_string),
        provenance: if guessed_string.is_some() {
            AnalysisProvenance::Guessed
        } else {
            AnalysisProvenance::Grammar
        },
        supplied_root: None,
        morpheme_roots: Vec::new(),
    }
}

fn inject_trusted_xml_fixture_metadata(
    grammar: &mut Grammar,
    root_entry_id: &str,
    root_form_guid: &str,
    root_msa_guid: &str,
    rules: &[(&str, &str, &[&str])],
) {
    for morpheme in &mut grammar.morphemes {
        if morpheme.xml_key == root_entry_id {
            morpheme.source_msa_guid = Some(root_msa_guid.to_string());
        } else if let Some((_, msa_guid, _)) = rules
            .iter()
            .find(|rule| morpheme.xml_key.as_str() == rule.0)
        {
            morpheme.source_msa_guid = Some((*msa_guid).to_string());
        }
    }
    for (allomorph_id, owner) in grammar.allomorph_owners.iter().copied().enumerate() {
        let form_guid = match owner {
            pg_grammar_model::model::AllomorphOwner::Root(_, _) => root_form_guid,
            pg_grammar_model::model::AllomorphOwner::Affix(rule, index) => {
                let morpheme = match &grammar.mrules[rule.0 as usize] {
                    pg_grammar_model::model::MorphRuleDef::AffixProcess(def) => def.morpheme,
                    pg_grammar_model::model::MorphRuleDef::Realizational(def) => def.morpheme,
                    pg_grammar_model::model::MorphRuleDef::Compounding(_) => {
                        panic!("fixture affix source owner points at a compounding rule")
                    }
                };
                let rule_id = &grammar.morphemes[morpheme.0 as usize].xml_key;
                let (_, _, forms) = rules
                    .iter()
                    .find(|rule| rule.0 == rule_id.as_str())
                    .expect("fixture rule metadata covers every affix owner");
                forms[index as usize]
            }
        };
        let source = &mut grammar.allomorph_sources[allomorph_id];
        source.form_guids = vec![Some(form_guid.to_string())];
    }
}

#[test]
fn projects_selected_same_owner_allomorph_and_infl_type() {
    let grammar = grammar(
        Some("11111111-1111-4111-8111-111111111111"),
        Some("22222222-2222-4222-8222-222222222222"),
        vec![
            vec![Some("33333333-3333-4333-8333-333333333333")],
            vec![Some("44444444-4444-4444-8444-444444444444")],
        ],
    );
    let analysis = analysis(
        vec![MorphOccurrence {
            allomorph_id: 1,
            morpheme_id: 0,
            order: 0,
        }],
        None,
    );
    let projected = project_parse_analysis(&analysis, &grammar).expect("source identities");
    assert_eq!(
        projected.morphs[0].form.as_deref(),
        Some("44444444-4444-4444-8444-444444444444")
    );
    assert_eq!(
        projected.morphs[0].msa.as_deref(),
        Some("11111111-1111-4111-8111-111111111111")
    );
    assert_eq!(
        projected.morphs[0].infl_type.as_deref(),
        Some("22222222-2222-4222-8222-222222222222")
    );
}

#[test]
fn sorts_multiple_morphs_by_annotation_order() {
    let mut grammar = grammar(
        Some("11111111-1111-4111-8111-111111111111"),
        None,
        vec![
            vec![Some("33333333-3333-4333-8333-333333333333")],
            vec![Some("44444444-4444-4444-8444-444444444444")],
        ],
    );
    grammar.morphemes.push(MorphemeInfo {
        xml_key: "morpheme-key-2".to_string(),
        source_msa_guid: Some("11111111-1111-4111-8111-111111111111".to_string()),
        source_msa_class: None,
        source_infl_type_guid: None,
        morph_id: None,
        gloss: None,
        stratum: StratumId(0),
        properties: Vec::new(),
        co_occurrence: Vec::new(),
    });
    let analysis = analysis(
        vec![
            MorphOccurrence {
                allomorph_id: 0,
                morpheme_id: 0,
                order: 3,
            },
            MorphOccurrence {
                allomorph_id: 1,
                morpheme_id: 1,
                order: 1,
            },
        ],
        None,
    );
    let projected = project_parse_analysis(&analysis, &grammar).expect("ordered morphs");
    let forms: Vec<_> = projected
        .morphs
        .iter()
        .map(|morph| morph.form.as_deref())
        .collect();
    assert_eq!(
        forms,
        vec![
            Some("44444444-4444-4444-8444-444444444444"),
            Some("33333333-3333-4333-8333-333333333333")
        ]
    );
}

#[test]
fn interleaves_circumfix_halves_using_their_two_annotation_orders() {
    let grammar = grammar(
        Some("11111111-1111-4111-8111-111111111111"),
        None,
        vec![
            vec![
                Some("33333333-3333-4333-8333-333333333333"),
                Some("44444444-4444-4444-8444-444444444444"),
            ],
            vec![Some("55555555-5555-4555-8555-555555555555")],
        ],
    );
    let analysis = analysis(
        vec![
            MorphOccurrence {
                allomorph_id: 0,
                morpheme_id: 0,
                order: 1,
            },
            MorphOccurrence {
                allomorph_id: 1,
                morpheme_id: 0,
                order: 2,
            },
            MorphOccurrence {
                allomorph_id: 0,
                morpheme_id: 0,
                order: 3,
            },
        ],
        None,
    );
    let projected = project_parse_analysis(&analysis, &grammar).expect("circumfix order");
    let forms: Vec<_> = projected
        .morphs
        .iter()
        .map(|morph| morph.form.as_deref())
        .collect();
    assert_eq!(
        forms,
        vec![
            Some("33333333-3333-4333-8333-333333333333"),
            Some("55555555-5555-4555-8555-555555555555"),
            Some("44444444-4444-4444-8444-444444444444")
        ]
    );
}

#[test]
fn ordinary_allomorph_is_emitted_once_for_duplicate_annotations() {
    let grammar = grammar(
        Some("11111111-1111-4111-8111-111111111111"),
        None,
        vec![vec![Some("33333333-3333-4333-8333-333333333333")]],
    );
    let analysis = analysis(
        vec![
            MorphOccurrence {
                allomorph_id: 0,
                morpheme_id: 0,
                order: 1,
            },
            MorphOccurrence {
                allomorph_id: 0,
                morpheme_id: 0,
                order: 3,
            },
        ],
        None,
    );
    let projected = project_parse_analysis(&analysis, &grammar).expect("ordinary morph");
    assert_eq!(projected.morphs.len(), 1);
    assert_eq!(
        projected.morphs[0].form.as_deref(),
        Some("33333333-3333-4333-8333-333333333333")
    );
}

#[test]
fn ordinary_projection_dedups_differing_allomorphs_by_morpheme() {
    let grammar = grammar(
        Some("11111111-1111-4111-8111-111111111111"),
        None,
        vec![
            vec![Some("33333333-3333-4333-8333-333333333333")],
            vec![Some("44444444-4444-4444-8444-444444444444")],
        ],
    );
    let analysis = analysis(
        vec![
            MorphOccurrence {
                allomorph_id: 0,
                morpheme_id: 0,
                order: 1,
            },
            MorphOccurrence {
                allomorph_id: 1,
                morpheme_id: 0,
                order: 2,
            },
        ],
        None,
    );
    let projected = project_parse_analysis(&analysis, &grammar).expect("ordinary morph");
    assert_eq!(projected.morphs.len(), 1);
    assert_eq!(
        projected.morphs[0].form.as_deref(),
        Some("33333333-3333-4333-8333-333333333333")
    );
}

#[test]
fn circumfix_can_emit_the_same_source_form_for_both_halves() {
    let grammar = grammar(
        Some("11111111-1111-4111-8111-111111111111"),
        None,
        vec![
            vec![
                Some("33333333-3333-4333-8333-333333333333"),
                Some("33333333-3333-4333-8333-333333333333"),
            ],
            vec![Some("55555555-5555-4555-8555-555555555555")],
        ],
    );
    let analysis = analysis(
        vec![
            MorphOccurrence {
                allomorph_id: 0,
                morpheme_id: 0,
                order: 1,
            },
            MorphOccurrence {
                allomorph_id: 1,
                morpheme_id: 0,
                order: 2,
            },
            MorphOccurrence {
                allomorph_id: 0,
                morpheme_id: 0,
                order: 3,
            },
        ],
        None,
    );
    let projected = project_parse_analysis(&analysis, &grammar).expect("circumfix identity");
    assert_eq!(
        projected
            .morphs
            .iter()
            .map(|morph| morph.form.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("33333333-3333-4333-8333-333333333333"),
            Some("55555555-5555-4555-8555-555555555555"),
            Some("33333333-3333-4333-8333-333333333333"),
        ]
    );
}

#[test]
fn intentional_null_affix_is_omitted_without_identity_error() {
    let mut grammar = grammar(None, None, vec![vec![]]);
    grammar.allomorph_sources[0].omitted = true;
    let analysis = analysis(
        vec![MorphOccurrence {
            allomorph_id: 0,
            morpheme_id: 0,
            order: 0,
        }],
        None,
    );
    let projected = project_parse_analysis(&analysis, &grammar).expect("null affix omission");
    assert!(projected.morphs.is_empty());
}

#[test]
fn infix_source_is_inserted_before_the_last_fieldworks_morph() {
    let mut grammar = grammar(
        Some("11111111-1111-4111-8111-111111111111"),
        None,
        vec![
            vec![Some("33333333-3333-4333-8333-333333333333")],
            vec![Some("44444444-4444-4444-8444-444444444444")],
            vec![Some("55555555-5555-4555-8555-555555555555")],
        ],
    );
    for index in 1..3 {
        grammar.morphemes.push(MorphemeInfo {
            xml_key: format!("morpheme-key-{index}"),
            source_msa_guid: Some("11111111-1111-4111-8111-111111111111".to_string()),
            source_msa_class: None,
            source_infl_type_guid: None,
            morph_id: None,
            gloss: None,
            stratum: StratumId(0),
            properties: Vec::new(),
            co_occurrence: Vec::new(),
        });
    }
    grammar.allomorph_sources[0].placement = SourceMorphPlacement::InsertBeforeLast;
    let analysis = analysis(
        vec![
            MorphOccurrence {
                allomorph_id: 1,
                morpheme_id: 1,
                order: 1,
            },
            MorphOccurrence {
                allomorph_id: 2,
                morpheme_id: 2,
                order: 2,
            },
            MorphOccurrence {
                allomorph_id: 0,
                morpheme_id: 0,
                order: 3,
            },
        ],
        None,
    );
    let projected = project_parse_analysis(&analysis, &grammar).expect("infix placement");
    assert_eq!(
        projected
            .morphs
            .iter()
            .map(|morph| morph.form.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("44444444-4444-4444-8444-444444444444"),
            Some("33333333-3333-4333-8333-333333333333"),
            Some("55555555-5555-4555-8555-555555555555"),
        ]
    );
}

#[test]
fn guessed_morph_without_source_identity_is_unavailable() {
    let grammar = grammar(None, None, Vec::new());
    let analysis = analysis(
        vec![MorphOccurrence {
            allomorph_id: u32::MAX,
            morpheme_id: u32::MAX,
            order: 0,
        }],
        Some("guessed-root"),
    );
    assert!(matches!(
        project_parse_analysis(&analysis, &grammar),
        Err(ParseProjectionError::UnsupportedRuntimeRoot)
    ));
}

#[test]
fn missing_source_identity_is_an_explicit_error() {
    let grammar = grammar(
        Some("11111111-1111-4111-8111-111111111111"),
        None,
        vec![vec![None]],
    );
    let analysis = analysis(
        vec![MorphOccurrence {
            allomorph_id: 0,
            morpheme_id: 0,
            order: 0,
        }],
        None,
    );
    assert!(matches!(
        project_parse_analysis(&analysis, &grammar),
        Err(ParseProjectionError::MissingSourceForm { .. })
    ));
}

#[test]
fn rejects_empty_source_guid() {
    let grammar = grammar(
        Some("00000000-0000-0000-0000-000000000000"),
        None,
        vec![vec![Some("33333333-3333-4333-8333-333333333333")]],
    );
    let analysis = analysis(
        vec![MorphOccurrence {
            allomorph_id: 0,
            morpheme_id: 0,
            order: 0,
        }],
        None,
    );
    assert!(matches!(
        project_parse_analysis(&analysis, &grammar),
        Err(ParseProjectionError::NonCanonicalSourceGuid { kind: "MSA", .. })
    ));
}

#[test]
fn projects_ordered_morphs_from_a_real_parse() {
    const XML: &str = r#"<HermitCrabInput><Language>
          <Name>RealParseProjection</Name>
          <PartsOfSpeech><PartOfSpeech id="pos"><Name>POS</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="table"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
            <BoundaryDefinitions><BoundaryDefinition id="plus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="any"><Name>Any</Name><Segment segment="a" /><Segment segment="b" /></SegmentNaturalClass></NaturalClasses>
          <Strata><Stratum characterDefinitionTable="table" morphologicalRules="11111111-1111-4111-8111-111111111111">
            <Name>Morphology</Name>
            <MorphologicalRuleDefinitions>
              <MorphologicalRule id="11111111-1111-4111-8111-111111111111" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
                <MorphemeId>SUFFIX</MorphemeId>
                <MorphologicalSubrules><MorphologicalSubrule id="33333333-3333-4333-8333-333333333333">
                  <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                  <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>+b</PhoneticShape></InsertSegments></MorphologicalOutput>
                </MorphologicalSubrule></MorphologicalSubrules>
              </MorphologicalRule>
            </MorphologicalRuleDefinitions>
            <LexicalEntries><LexicalEntry id="22222222-2222-4222-8222-222222222222" partOfSpeech="pos"><Allomorphs><Allomorph id="44444444-4444-4444-8444-444444444444"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries>
          </Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let mut grammar = pg_grammar::load(XML).expect("real parse grammar loads");
    assert!(
        grammar
            .morphemes
            .iter()
            .all(|morpheme| morpheme.source_msa_guid.is_none()),
        "XML ids must not be treated as portable source MSA identities"
    );
    assert!(
        grammar
            .allomorph_sources
            .iter()
            .all(|source| source.form_guids.iter().all(Option::is_none)),
        "XML ids must not be treated as portable source MoForm identities"
    );
    inject_trusted_xml_fixture_metadata(
        &mut grammar,
        "22222222-2222-4222-8222-222222222222",
        "44444444-4444-4444-8444-444444444444",
        "22222222-2222-4222-8222-222222222222",
        &[(
            "11111111-1111-4111-8111-111111111111",
            "11111111-1111-4111-8111-111111111111",
            &["33333333-3333-4333-8333-333333333333"][..],
        )],
    );
    let outcome = crate::Morpher::new(&grammar, usize::MAX).parse_word("ab");
    let analysis = outcome
        .structured
        .iter()
        .find(|analysis| !analysis.guessed)
        .expect("real parse has an authored analysis");
    let projected = project_parse_analysis(analysis, &grammar).expect("source projection");
    assert_eq!(
        projected
            .morphs
            .iter()
            .map(|morph| morph.form.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("44444444-4444-4444-8444-444444444444"),
            Some("33333333-3333-4333-8333-333333333333")
        ]
    );
    assert_eq!(
        projected
            .morphs
            .iter()
            .map(|morph| morph.msa.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("22222222-2222-4222-8222-222222222222"),
            Some("11111111-1111-4111-8111-111111111111")
        ]
    );
}

#[test]
fn homophonous_affix_allomorphs_survive_structured_source_projection() {
    const XML: &str = r#"<HermitCrabInput><Language>
          <Name>HomophonousAffixProjection</Name>
          <PartsOfSpeech><PartOfSpeech id="pos"><Name>POS</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="table"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
            <BoundaryDefinitions><BoundaryDefinition id="plus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="any"><Name>Any</Name><Segment segment="a" /><Segment segment="b" /></SegmentNaturalClass></NaturalClasses>
          <Strata><Stratum characterDefinitionTable="table" morphologicalRules="11111111-1111-4111-8111-111111111111">
            <Name>Morphology</Name>
            <MorphologicalRuleDefinitions>
              <MorphologicalRule id="11111111-1111-4111-8111-111111111111" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
                <MorphemeId>SUFFIX</MorphemeId>
                <MorphologicalSubrules>
                  <MorphologicalSubrule id="33333333-3333-4333-8333-333333333333">
                    <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                    <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>+b</PhoneticShape></InsertSegments></MorphologicalOutput>
                  </MorphologicalSubrule>
                  <MorphologicalSubrule id="55555555-5555-4555-8555-555555555555">
                    <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                    <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>+b</PhoneticShape></InsertSegments></MorphologicalOutput>
                  </MorphologicalSubrule>
                </MorphologicalSubrules>
              </MorphologicalRule>
            </MorphologicalRuleDefinitions>
            <LexicalEntries><LexicalEntry id="22222222-2222-4222-8222-222222222222" partOfSpeech="pos"><Allomorphs><Allomorph id="44444444-4444-4444-8444-444444444444"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries>
          </Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let mut grammar = pg_grammar::load(XML).expect("homophonous grammar loads");
    assert!(
        grammar
            .morphemes
            .iter()
            .all(|morpheme| morpheme.source_msa_guid.is_none()),
        "XML ids must not be treated as portable source MSA identities"
    );
    assert!(
        grammar
            .allomorph_sources
            .iter()
            .all(|source| source.form_guids.iter().all(Option::is_none)),
        "XML ids must not be treated as portable source MoForm identities"
    );
    inject_trusted_xml_fixture_metadata(
        &mut grammar,
        "22222222-2222-4222-8222-222222222222",
        "44444444-4444-4444-8444-444444444444",
        "22222222-2222-4222-8222-222222222222",
        &[(
            "11111111-1111-4111-8111-111111111111",
            "11111111-1111-4111-8111-111111111111",
            &[
                "33333333-3333-4333-8333-333333333333",
                "55555555-5555-4555-8555-555555555555",
            ][..],
        )],
    );
    let outcome = crate::Morpher::new(&grammar, usize::MAX).parse_word("ab");
    let mut projected: Vec<Vec<(String, String)>> = outcome
        .structured
        .iter()
        .filter(|analysis| !analysis.guessed)
        .map(|analysis| {
            project_parse_analysis(analysis, &grammar)
                .expect("every authored homophonous reading must project")
                .morphs
                .into_iter()
                .map(|morph| {
                    (
                        morph.form.expect("fixture morph has a trusted source form"),
                        morph.msa.expect("fixture morph has a trusted source MSA"),
                    )
                })
                .collect()
        })
        .collect();
    projected.sort();
    assert_eq!(
        projected,
        vec![
            vec![
                (
                    "44444444-4444-4444-8444-444444444444".to_string(),
                    "22222222-2222-4222-8222-222222222222".to_string(),
                ),
                (
                    "33333333-3333-4333-8333-333333333333".to_string(),
                    "11111111-1111-4111-8111-111111111111".to_string(),
                ),
            ],
            vec![
                (
                    "44444444-4444-4444-8444-444444444444".to_string(),
                    "22222222-2222-4222-8222-222222222222".to_string(),
                ),
                (
                    "55555555-5555-4555-8555-555555555555".to_string(),
                    "11111111-1111-4111-8111-111111111111".to_string(),
                ),
            ],
        ]
    );
}

#[test]
fn unordered_two_stage_homophonous_parse_keeps_both_ordered_source_trails() {
    const XML: &str = r#"<HermitCrabInput><Language>
          <Name>TwoStageHomophonousProjection</Name>
          <PartsOfSpeech><PartOfSpeech id="pos"><Name>POS</Name></PartOfSpeech></PartsOfSpeech>
          <CharacterDefinitionTable id="table"><Name>Main</Name>
            <SegmentDefinitions>
              <SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
              <SegmentDefinition id="c"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
            </SegmentDefinitions>
            <BoundaryDefinitions><BoundaryDefinition id="plus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="any"><Name>Any</Name><Segment segment="a" /><Segment segment="b" /><Segment segment="c" /></SegmentNaturalClass></NaturalClasses>
          <Strata><Stratum characterDefinitionTable="table" morphologicalRuleOrder="unordered" morphologicalRules="11111111-1111-4111-8111-111111111111 77777777-7777-4777-8777-777777777777">
            <Name>Morphology</Name>
            <MorphologicalRuleDefinitions>
              <MorphologicalRule id="11111111-1111-4111-8111-111111111111" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
                <MorphemeId>INNER</MorphemeId>
                <MorphologicalSubrules><MorphologicalSubrule id="88888888-8888-4888-8888-888888888888">
                  <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                  <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>+b</PhoneticShape></InsertSegments></MorphologicalOutput>
                </MorphologicalSubrule></MorphologicalSubrules>
              </MorphologicalRule>
              <MorphologicalRule id="77777777-7777-4777-8777-777777777777" requiredPartsOfSpeech="pos" outputPartOfSpeech="pos">
                <MorphemeId>OUTER</MorphemeId>
                <MorphologicalSubrules>
                  <MorphologicalSubrule id="33333333-3333-4333-8333-333333333333">
                    <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                    <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>+c</PhoneticShape></InsertSegments></MorphologicalOutput>
                  </MorphologicalSubrule>
                  <MorphologicalSubrule id="55555555-5555-4555-8555-555555555555">
                    <MorphologicalInput><PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                    <MorphologicalOutput><CopyFromInput index="stem" /><InsertSegments><PhoneticShape>+c</PhoneticShape></InsertSegments></MorphologicalOutput>
                  </MorphologicalSubrule>
                </MorphologicalSubrules>
              </MorphologicalRule>
            </MorphologicalRuleDefinitions>
            <LexicalEntries><LexicalEntry id="22222222-2222-4222-8222-222222222222" partOfSpeech="pos"><Allomorphs><Allomorph id="44444444-4444-4444-8444-444444444444"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries>
          </Stratum></Strata>
        </Language></HermitCrabInput>"#;
    let mut grammar = pg_grammar::load(XML).expect("two-stage grammar loads");
    assert!(grammar
        .morphemes
        .iter()
        .all(|morpheme| morpheme.source_msa_guid.is_none()));
    assert!(grammar
        .allomorph_sources
        .iter()
        .all(|source| source.form_guids.iter().all(Option::is_none)));
    inject_trusted_xml_fixture_metadata(
        &mut grammar,
        "22222222-2222-4222-8222-222222222222",
        "44444444-4444-4444-8444-444444444444",
        "22222222-2222-4222-8222-222222222222",
        &[
            (
                "11111111-1111-4111-8111-111111111111",
                "11111111-1111-4111-8111-111111111111",
                &["88888888-8888-4888-8888-888888888888"][..],
            ),
            (
                "77777777-7777-4777-8777-777777777777",
                "77777777-7777-4777-8777-777777777777",
                &[
                    "33333333-3333-4333-8333-333333333333",
                    "55555555-5555-4555-8555-555555555555",
                ][..],
            ),
        ],
    );

    fn projected(grammar: &Grammar, outcome: &crate::ParseOutcome) -> Vec<Vec<(String, String)>> {
        let mut result: Vec<Vec<(String, String)>> = outcome
            .structured
            .iter()
            .filter(|analysis| !analysis.guessed)
            .map(|analysis| {
                project_parse_analysis(analysis, grammar)
                    .expect("every authored two-stage reading must project")
                    .morphs
                    .into_iter()
                    .map(|morph| {
                        (
                            morph.form.expect("trusted fixture form"),
                            morph.msa.expect("trusted fixture MSA"),
                        )
                    })
                    .collect()
            })
            .collect();
        result.sort();
        result
    }

    let parsed = crate::Morpher::new(&grammar, usize::MAX).parse_word("abc");
    assert_eq!(
        projected(&grammar, &parsed),
        vec![
            vec![
                (
                    "44444444-4444-4444-8444-444444444444".to_string(),
                    "22222222-2222-4222-8222-222222222222".to_string(),
                ),
                (
                    "88888888-8888-4888-8888-888888888888".to_string(),
                    "11111111-1111-4111-8111-111111111111".to_string(),
                ),
                (
                    "33333333-3333-4333-8333-333333333333".to_string(),
                    "77777777-7777-4777-8777-777777777777".to_string(),
                ),
            ],
            vec![
                (
                    "44444444-4444-4444-8444-444444444444".to_string(),
                    "22222222-2222-4222-8222-222222222222".to_string(),
                ),
                (
                    "88888888-8888-4888-8888-888888888888".to_string(),
                    "11111111-1111-4111-8111-111111111111".to_string(),
                ),
                (
                    "55555555-5555-4555-8555-555555555555".to_string(),
                    "77777777-7777-4777-8777-777777777777".to_string(),
                ),
            ],
        ]
    );
}
