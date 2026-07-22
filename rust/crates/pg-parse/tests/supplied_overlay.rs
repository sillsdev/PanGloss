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
        realization_id: format!("{id}:sig0"),
        lexical_spelling: spelling.into(),
        gloss: gloss.into(),
        syn_fs: FeatureStruct::EMPTY,
        mpr: MprSet::EMPTY,
        stratum: StratumId(0),
        authority,
    }
}

#[test]
fn one_entry_with_multiple_signature_realizations_survives_dedup() {
    let g = grammar();
    let mut first = supplied("pgl_multi", "b", "multi", RootAuthority::Supplied);
    let mut second = first.clone();
    first.realization_id = "pgl_multi:sig-n".into();
    second.realization_id = "pgl_multi:sig-v".into();
    let overlay = SuppliedRootOverlay::build(&g, vec![first, second]).unwrap();
    let parsed = Morpher::new_with_overlay(&g, 10_000, &overlay).parse_word("b");
    assert_eq!(
        parsed
            .structured
            .iter()
            .filter(|a| matches!(a.provenance, AnalysisProvenance::Supplied { .. }))
            .count(),
        2
    );
}

#[test]
fn overlay_trie_shares_prefixes_at_scale() {
    let g = grammar();
    let roots = (0..1_000)
        .map(|i| supplied(&format!("pgl_{i}"), "b", "", RootAuthority::Supplied))
        .collect();
    let overlay = SuppliedRootOverlay::build(&g, roots).unwrap();
    assert_eq!(overlay.node_count(StratumId(0)), 2);
    assert_eq!(
        Morpher::new_with_overlay(&g, 100_000, &overlay)
            .parse_word("b")
            .structured
            .iter()
            .filter(|a| matches!(a.provenance, AnalysisProvenance::Supplied { .. }))
            .count(),
        1_000
    );
}

#[test]
fn overlay_trie_uses_char_def_closure_on_identity_miss() {
    let xml = r#"<HermitCrabInput><Language><Name>F</Name><PartsOfSpeech><PartOfSpeech id="n"><Name>n</Name></PartOfSpeech></PartsOfSpeech><PhonologicalFeatureSystem><SymbolicFeature id="voice"><Name>voice</Name><Symbols><Symbol id="plus">+</Symbol></Symbols></SymbolicFeature></PhonologicalFeatureSystem><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions><SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations><FeatureValue feature="voice" symbolValues="plus" /></SegmentDefinition><SegmentDefinition id="d"><Representations><Representation>d</Representation></Representations><FeatureValue feature="voice" symbolValues="plus" /></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries /></Stratum></Strata></Language></HermitCrabInput>"#;
    let g = pg_grammar::load(xml).unwrap();
    let overlay = SuppliedRootOverlay::build(
        &g,
        vec![supplied("pgl_closure", "b", "", RootAuthority::Supplied)],
    )
    .unwrap();
    let parsed = Morpher::new_with_overlay(&g, 10_000, &overlay).parse_word("d");
    assert!(parsed.structured.iter().any(|a| matches!(
        a.provenance,
        AnalysisProvenance::Supplied { ref entry_id } if entry_id == "pgl_closure"
    )));
}

#[test]
fn overlay_and_provenance_have_canonical_tagged_wire_shapes() {
    let root = supplied(
        "pgl_wire",
        "b",
        "wire",
        RootAuthority::SuppliedOverride {
            official_entry_id: "official-b".into(),
        },
    );
    let json = serde_json::to_value(&root).unwrap();
    assert_eq!(json["entryId"], "pgl_wire");
    assert_eq!(json["realizationId"], "pgl_wire:sig0");
    assert_eq!(json["authority"]["kind"], "suppliedOverride");
    assert_eq!(serde_json::from_value::<SuppliedRoot>(json).unwrap(), root);

    let provenance = AnalysisProvenance::SuppliedOverride {
        entry_id: "pgl_wire".into(),
        overridden_grammar_entry_id: "official-b".into(),
    };
    let json = serde_json::to_value(&provenance).unwrap();
    assert_eq!(json["kind"], "suppliedOverride");
    assert_eq!(json["entryId"], "pgl_wire");
    assert_eq!(
        serde_json::from_value::<AnalysisProvenance>(json).unwrap(),
        provenance
    );
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
            realization_id: "pgl_inflect:sig-v".into(),
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

    let compiled = csharp_port_common::build_grammar_custom_lexicon(
        rule,
        "mrS",
        r#"<LexicalEntry id="compiled-pap" partOfSpeech="posV"><MorphemeId>ROOT</MorphemeId><Allomorphs><Allomorph id="compiled-pap-a"><PhoneticShape>pap</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>"#,
    );
    let compiled_out = Morpher::new(&compiled, 100_000).parse_word("paps");
    let compiled_analysis = &compiled_out.structured[0];
    assert_eq!(
        analysis.root_morpheme_index,
        compiled_analysis.root_morpheme_index
    );
    assert_eq!(
        analysis.morpheme_ids.len(),
        compiled_analysis.morpheme_ids.len()
    );
    assert_eq!(analysis.pos_id, compiled_analysis.pos_id);
    assert_eq!(analysis.syn_fs, compiled_analysis.syn_fs);
    assert_eq!(analysis.mpr, compiled_analysis.mpr);
    assert_eq!(parsed.analyses[0].1, compiled_out.analyses[0].1);
    assert!(Morpher::new(&compiled, 100_000)
        .generate_words_from_analysis(compiled_analysis)
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
                realization_id: "pgl_head:sig-n".into(),
                lexical_spelling: "pap".into(),
                gloss: "head".into(),
                syn_fs: g.fs_interner.get(head.syn_fs).clone(),
                mpr: head.mpr,
                stratum: StratumId(0),
                authority: RootAuthority::Supplied,
            },
            SuppliedRoot {
                entry_id: "pgl_nonhead".into(),
                realization_id: "pgl_nonhead:sig-n".into(),
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
    let official_head = m.parse_word("pʰutdas");
    let compound = official_head.structured.first().expect("compound analysis");
    assert!(compound
        .morpheme_roots
        .iter()
        .flatten()
        .any(|root| root.entry_id == "pgl_nonhead"));
    assert!(m
        .generate_words_from_analysis(compound)
        .contains(&"pʰutdas".to_string()));

    let supplied_both = m.parse_word("papdas");
    let compound = supplied_both
        .structured
        .iter()
        .find(|a| matches!(a.provenance, AnalysisProvenance::Supplied { .. }))
        .expect("supplied-head compound analysis");
    assert_eq!(compound.morpheme_roots.iter().flatten().count(), 2);
    assert!(m
        .generate_words_from_analysis(compound)
        .contains(&"papdas".to_string()));

    let compiled = csharp_port_common::build_grammar_custom_lexicon(
        rule,
        "mrC",
        r#"<LexicalEntry id="compiled-pap" partOfSpeech="posN"><MorphemeId>HEAD</MorphemeId><Allomorphs><Allomorph id="compiled-pap-a"><PhoneticShape>pap</PhoneticShape></Allomorph></Allomorphs></LexicalEntry><LexicalEntry id="compiled-das" partOfSpeech="posN"><MorphemeId>NONHEAD</MorphemeId><Allomorphs><Allomorph id="compiled-das-a"><PhoneticShape>das</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>"#,
    );
    let compiled_m = Morpher::new(&compiled, 100_000);
    let compiled_out = compiled_m.parse_word("papdas");
    let compiled_analysis = &compiled_out.structured[0];
    assert_eq!(
        compound.root_morpheme_index,
        compiled_analysis.root_morpheme_index
    );
    assert_eq!(
        compound.morpheme_ids.len(),
        compiled_analysis.morpheme_ids.len()
    );
    assert_eq!(compound.pos_id, compiled_analysis.pos_id);
    assert_eq!(compound.syn_fs, compiled_analysis.syn_fs);
    assert_eq!(compound.mpr, compiled_analysis.mpr);
    assert_eq!(supplied_both.analyses[0].1, compiled_out.analyses[0].1);
    assert!(compiled_m
        .generate_words_from_analysis(compiled_analysis)
        .contains(&"papdas".to_string()));
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
            realization_id: "pgl_b:sig-n".into(),
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
