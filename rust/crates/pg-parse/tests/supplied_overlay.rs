use pg_featstruct::FeatureStruct;
use pg_grammar::model::{MprSet, StratumId};
use pg_parse::{AnalysisProvenance, Morpher, RootAuthority, SuppliedRoot, SuppliedRootOverlay};
mod csharp_port_common;

fn grammar() -> pg_grammar::model::Grammar {
    pg_grammar::load(r#"<HermitCrabInput><Language><Name>T</Name><PartsOfSpeech><PartOfSpeech id="n"><Name>n</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions><SegmentDefinition id="a"><Representations><Representation>a</Representation><Representation>á</Representation></Representations></SegmentDefinition><SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries><LexicalEntry id="official-a" partOfSpeech="n"><Gloss>official</Gloss><Allomorphs><Allomorph id="ao"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#).unwrap()
}

fn supplied(id: &str, spelling: &str, gloss: &str, authority: RootAuthority) -> SuppliedRoot {
    SuppliedRoot {
        entry_id: id.into(),
        lexical_spelling: spelling.into(),
        gloss: gloss.into(),
        syn_fs: FeatureStruct::EMPTY,
        mpr: MprSet::EMPTY,
        stratum: StratumId(0),
        authority,
    }
}

#[test]
fn supplied_homographs_survive_and_alternate_representations_are_equivalent() {
    let g = grammar();
    let overlay = SuppliedRootOverlay::build(
        &g,
        vec![
            supplied("pgl_one", "á", "one", RootAuthority::Supplied),
            supplied("pgl_two", "á", "two", RootAuthority::Supplied),
        ],
    )
    .unwrap();
    let parsed = Morpher::new_with_overlay(&g, 10_000, &overlay).parse_word("a");
    let mut ids: Vec<_> = parsed
        .structured
        .iter()
        .filter_map(|a| match &a.provenance {
            AnalysisProvenance::Supplied { entry_id } => Some(entry_id.as_str()),
            _ => None,
        })
        .collect();
    ids.sort();
    assert_eq!(ids, ["pgl_one", "pgl_two"]);
    let decomposed = Morpher::new_with_overlay(&g, 10_000, &overlay).parse_word("a\u{301}");
    assert_eq!(
        decomposed
            .structured
            .iter()
            .filter(|a| matches!(a.provenance, AnalysisProvenance::Supplied { .. }))
            .count(),
        2
    );
}

#[test]
fn supplied_root_participates_in_ordinary_inflection() {
    let rule = r#"<MorphologicalRule id="mrS" requiredPartsOfSpeech="posV"><Name>s_suffix</Name><MorphemeId>3SG</MorphemeId><MorphologicalSubrules><MorphologicalSubrule id="subS"><MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>s</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules></MorphologicalRule>"#;
    let g = csharp_port_common::build_morph_grammar(rule, "mrS");
    let exemplar = &g.entries[csharp_port_common::lex_entry_id(&g, "32").0 as usize];
    let overlay = SuppliedRootOverlay::build(
        &g,
        vec![SuppliedRoot {
            entry_id: "pgl_inflect".into(),
            lexical_spelling: "pap".into(),
            gloss: "new verb".into(),
            syn_fs: g.fs_interner.get(exemplar.syn_fs).clone(),
            mpr: exemplar.mpr,
            stratum: StratumId(0),
            authority: RootAuthority::Supplied,
        }],
    )
    .unwrap();
    let parsed = Morpher::new_with_overlay(&g, 100_000, &overlay).parse_word("paps");
    let analysis = parsed
        .structured
        .iter()
        .find(|analysis| {
            matches!(
                analysis.provenance,
                AnalysisProvenance::Supplied { ref entry_id } if entry_id == "pgl_inflect"
            )
        })
        .expect("supplied inflection analysis");
    assert!(Morpher::new_with_overlay(&g, 100_000, &overlay)
        .generate_words_from_analysis(analysis)
        .contains(&"paps".to_string()));
}

#[test]
fn supplied_roots_participate_as_compound_heads_and_non_heads() {
    let rule = r#"<CompoundingRule id="mrC"><Name>rule1</Name><CompoundingSubrules><CompoundingSubrule><HeadMorphologicalInput><PhoneticSequence id="head"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></HeadMorphologicalInput><NonHeadMorphologicalInput><PhoneticSequence id="nonHead"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></NonHeadMorphologicalInput><MorphologicalOutput><CopyFromInput index="head" /><InsertSegments><PhoneticShape>+</PhoneticShape></InsertSegments><CopyFromInput index="nonHead" /></MorphologicalOutput></CompoundingSubrule></CompoundingSubrules></CompoundingRule>"#;
    let g = csharp_port_common::build_morph_grammar(rule, "mrC");
    let head = &g.entries[csharp_port_common::lex_entry_id(&g, "5").0 as usize];
    let non_head = &g.entries[csharp_port_common::lex_entry_id(&g, "8").0 as usize];
    let overlay = SuppliedRootOverlay::build(
        &g,
        vec![
            SuppliedRoot {
                entry_id: "pgl_head".into(),
                lexical_spelling: "pap".into(),
                gloss: "head".into(),
                syn_fs: g.fs_interner.get(head.syn_fs).clone(),
                mpr: head.mpr,
                stratum: StratumId(0),
                authority: RootAuthority::Supplied,
            },
            SuppliedRoot {
                entry_id: "pgl_nonhead".into(),
                lexical_spelling: "das".into(),
                gloss: "non-head".into(),
                syn_fs: g.fs_interner.get(non_head.syn_fs).clone(),
                mpr: non_head.mpr,
                stratum: StratumId(0),
                authority: RootAuthority::Supplied,
            },
        ],
    )
    .unwrap();
    let m = Morpher::new_with_overlay(&g, 100_000, &overlay);
    assert!(m.parse_word("papdat").structured.iter().any(|a| matches!(
        a.provenance,
        AnalysisProvenance::Supplied { ref entry_id } if entry_id == "pgl_head"
    )));
    assert!(!m.parse_word("pʰutdas").structured.is_empty());
}

#[test]
fn override_suppresses_official_and_removal_restores_it() {
    let g = grammar();
    let overlay = SuppliedRootOverlay::build(
        &g,
        vec![supplied(
            "pgl_override",
            "a",
            "replacement",
            RootAuthority::SuppliedOverride {
                official_entry_id: "official-a".into(),
            },
        )],
    )
    .unwrap();
    let overridden = Morpher::new_with_overlay(&g, 10_000, &overlay).parse_word("a");
    assert!(overridden
        .structured
        .iter()
        .all(|a| matches!(a.provenance, AnalysisProvenance::SuppliedOverride { .. })));

    let restored =
        Morpher::new_with_overlay(&g, 10_000, &SuppliedRootOverlay::empty(&g)).parse_word("a");
    assert!(restored
        .structured
        .iter()
        .any(|a| a.provenance == AnalysisProvenance::Grammar));
}

#[test]
fn overlay_root_matches_the_same_root_compiled_into_the_grammar() {
    let base = grammar();
    let exemplar = &base.entries[0];
    let overlay = SuppliedRootOverlay::build(
        &base,
        vec![SuppliedRoot {
            entry_id: "pgl_b".into(),
            lexical_spelling: "b".into(),
            gloss: "compiled-equivalent".into(),
            syn_fs: base.fs_interner.get(exemplar.syn_fs).clone(),
            mpr: exemplar.mpr,
            stratum: StratumId(0),
            authority: RootAuthority::Supplied,
        }],
    )
    .unwrap();
    let overlay_out = Morpher::new_with_overlay(&base, 10_000, &overlay).parse_word("b");

    let compiled_xml = r#"<HermitCrabInput><Language><Name>T</Name><PartsOfSpeech><PartOfSpeech id="n"><Name>n</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions><SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition><SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries><LexicalEntry id="official-b" partOfSpeech="n"><Gloss>compiled-equivalent</Gloss><Allomorphs><Allomorph id="bo"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#;
    let compiled = pg_grammar::load(compiled_xml).unwrap();
    let compiled_out = Morpher::new(&compiled, 10_000).parse_word("b");
    assert_eq!(
        overlay_out
            .analyses
            .iter()
            .map(|a| &a.1)
            .collect::<Vec<_>>(),
        compiled_out
            .analyses
            .iter()
            .map(|a| &a.1)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        overlay_out.structured[0].pos_id,
        compiled_out.structured[0].pos_id
    );
}
