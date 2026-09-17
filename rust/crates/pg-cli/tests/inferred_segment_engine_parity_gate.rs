//! An inferred substrate segment must behave exactly like an authored featureless one, and differently from an explicitly feature-valued one, on the real HC-Rust and foma-propose+HC-confirm engines -- not just in the compiled char-def artifact. Lives here (not in `pg-grammar`'s own tests) because `pg-parse`/`pg-foma` both depend on `pg-grammar`, so calling their APIs from inside `pg-grammar`'s own test binary splits `Grammar`'s type identity across the dev-dependency boundary (`error[E0308]: mismatched types ... multiple different versions of crate pg_grammar`); `pg-cli` already depends on all three normally, with no cycle.

use pg_grammar::compile::{CompileOptions, SubstratePolicy};
use pg_grammar::compile_project_with;
use pg_snapshot::feature::{
    ClosedFeature, FeatureStructure, FeatureValue, FeatureValueKind, FeatureValueSymbol,
};
use pg_snapshot::lexicon::{Allomorph, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{AffixSlot, AffixTemplate, MorphType, Morphology, PartOfSpeech};
use pg_snapshot::phonology::{BoundaryMarker, Environment, NaturalClass, Phoneme, Phonology};
use pg_snapshot::project::Project;
use pg_snapshot::{ActiveParser, FeatureSystems, Snapshot, WsForm};

fn ws(ws: &str, form: &str) -> WsForm {
    WsForm {
        ws: ws.to_string(),
        form: form.to_string(),
    }
}

fn phoneme(guid: &str, rep: &str) -> Phoneme {
    Phoneme {
        guid: guid.to_string(),
        name: rep.to_string(),
        representations: vec![ws("sen", rep)],
        features: None,
        basic_ipa_symbol: None,
    }
}

fn stem_allomorph(guid: &str, form: &str) -> Allomorph {
    Allomorph {
        guid: guid.to_string(),
        morph_type: MorphType::Stem,
        is_abstract: false,
        forms: vec![ws("sen", form)],
        environments: Vec::new(),
        positions: Vec::new(),
        stem_name: None,
        inflection_classes: Vec::new(),
        ms_env_features: None,
        ms_env_part_of_speech: None,
        process: None,
    }
}

/// One noun POS/slot/template, a `kuma` stem (no `q`), a `q` stem, and a `-ta` suffix gated by a `[Front]`-referencing environment.
fn base_snapshot() -> Snapshot {
    let noun_pos = "pos-noun".to_string();
    let slot = "slot-pl".to_string();
    let template = "tmpl-noun".to_string();

    let pos = PartOfSpeech {
        guid: noun_pos.clone(),
        name: "Noun".to_string(),
        abbreviation: "n".to_string(),
        children: Vec::new(),
        inflection_classes: Vec::new(),
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: vec![AffixSlot {
            guid: slot.clone(),
            name: "Pl".to_string(),
            optional: false,
        }],
        affix_templates: vec![AffixTemplate {
            guid: template,
            name: "NounTemplate".to_string(),
            disabled: false,
            prefix_slots: Vec::new(),
            suffix_slots: vec![slot.clone()],
            is_final: true,
        }],
    };

    let stem_kuma = LexEntry {
        guid: "entry-kuma".to_string(),
        citation_form: vec![ws("sen", "kuma")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![stem_allomorph("allo-kuma", "kuma")],
        msas: vec![Msa::Stem {
            guid: "msa-kuma".to_string(),
            part_of_speech: Some(noun_pos.clone()),
            inflection_class: None,
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: Vec::new(),
            slots: Vec::new(),
        }],
        senses: vec![Sense {
            guid: "sense-kuma".to_string(),
            gloss: vec![ws("en", "dog")],
            definition: Vec::new(),
            msa: Some("msa-kuma".to_string()),
        }],
        entry_refs: Vec::new(),
    };

    let stem_q = LexEntry {
        guid: "entry-q".to_string(),
        citation_form: vec![ws("sen", "q")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![stem_allomorph("allo-q", "q")],
        msas: vec![Msa::Stem {
            guid: "msa-q".to_string(),
            part_of_speech: Some(noun_pos.clone()),
            inflection_class: None,
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: Vec::new(),
            slots: Vec::new(),
        }],
        senses: vec![Sense {
            guid: "sense-q".to_string(),
            gloss: vec![ws("en", "thing")],
            definition: Vec::new(),
            msa: Some("msa-q".to_string()),
        }],
        entry_refs: Vec::new(),
    };

    let suffix = LexEntry {
        guid: "entry-suffix".to_string(),
        citation_form: vec![ws("sen", "-ta")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![Allomorph {
            guid: "allo-suffix".to_string(),
            morph_type: MorphType::Suffix,
            is_abstract: false,
            forms: vec![ws("sen", "ta")],
            environments: vec!["env-front".to_string()],
            positions: Vec::new(),
            stem_name: None,
            inflection_classes: Vec::new(),
            ms_env_features: None,
            ms_env_part_of_speech: None,
            process: None,
        }],
        msas: vec![Msa::Inflectional {
            guid: "msa-suffix".to_string(),
            part_of_speech: Some(noun_pos.clone()),
            slots: vec![slot],
            features: None,
            exception_features: Vec::new(),
        }],
        senses: vec![Sense {
            guid: "sense-suffix".to_string(),
            gloss: vec![ws("en", "PL")],
            definition: Vec::new(),
            msa: Some("msa-suffix".to_string()),
        }],
        entry_refs: Vec::new(),
    };

    let feature_guid = "feat-frontness".to_string();
    let front_guid = "val-front".to_string();
    let back_guid = "val-back".to_string();

    let mut snapshot = Snapshot::new(
        Project {
            name: "Test".to_string(),
            vernacular_writing_systems: vec!["sen".to_string()],
            analysis_writing_systems: vec!["en".to_string()],
            exemplar_characters: Vec::new(),
        },
        FeatureSystems::default(),
        Phonology {
            phonemes: vec![
                phoneme("ph-k", "k"),
                phoneme("ph-t", "t"),
                phoneme("ph-m", "m"),
                phoneme("ph-a", "a"),
                phoneme("ph-u", "u"),
            ],
            boundary_markers: vec![BoundaryMarker {
                guid: "bd-plus".to_string(),
                name: "+".to_string(),
                representations: vec![ws("sen", "+")],
            }],
            ..Phonology::default()
        },
        Morphology {
            parts_of_speech: vec![pos],
            ..Morphology::default()
        },
        Lexicon {
            entries: vec![stem_kuma, stem_q, suffix],
        },
    );

    snapshot
        .feature_systems
        .phonological
        .closed_features
        .push(ClosedFeature {
            guid: feature_guid,
            name: "Frontness".to_string(),
            abbreviation: "frnt".to_string(),
            values: vec![
                FeatureValueSymbol {
                    guid: front_guid.clone(),
                    name: "front".to_string(),
                    abbreviation: "fr".to_string(),
                },
                FeatureValueSymbol {
                    guid: back_guid,
                    name: "back".to_string(),
                    abbreviation: "bk".to_string(),
                },
            ],
        });
    snapshot
        .phonology
        .natural_classes
        .push(NaturalClass::Features {
            guid: "nc-front".to_string(),
            name: "Front".to_string(),
            features: FeatureStructure {
                values: vec![FeatureValue {
                    feature: "feat-frontness".to_string(),
                    value: FeatureValueKind::Closed { value: front_guid },
                }],
            },
        });
    snapshot.phonology.environments.push(Environment {
        guid: "env-front".to_string(),
        name: String::new(),
        representation: "/[Front]_".to_string(),
    });
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot
}

fn compile(snapshot: &Snapshot) -> pg_grammar::model::Grammar {
    compile_project_with(
        snapshot,
        CompileOptions {
            substrate: SubstratePolicy::Auto,
            ..CompileOptions::default()
        },
    )
    .expect("fixture must compile")
    .grammar
}

fn analyze_direct(grammar: &pg_grammar::model::Grammar, word: &str) -> String {
    pg_parse::Morpher::new(grammar, usize::MAX)
        .parse_word(word)
        .signature()
}

fn analyze_fst_confirm(grammar: &pg_grammar::model::Grammar, word: &str) -> String {
    let mut analyzer =
        pg_foma::composite::FomaAnalyzer::new(grammar).expect("fixture grammar must foma-compile");
    let outcome = analyzer.analyze_word(word);
    pg_parse::result_signature(&outcome.analyses)
}

#[test]
fn inferred_q_analyzes_like_an_authored_featureless_q_and_unlike_a_valued_one() {
    let inferred_snapshot = base_snapshot();
    let inferred = compile(&inferred_snapshot);
    assert!(
        inferred.char_tables[0].lookup_nfd("q").is_some(),
        "q must have been inferred into the char table"
    );

    let mut explicit_snapshot = inferred_snapshot.clone();
    explicit_snapshot
        .phonology
        .phonemes
        .push(phoneme("ph-explicit-q", "q"));
    let explicit = compile(&explicit_snapshot);

    let mut valued_snapshot = inferred_snapshot;
    valued_snapshot.phonology.phonemes.push(Phoneme {
        guid: "ph-valued-q".to_string(),
        name: "q".to_string(),
        representations: vec![ws("sen", "q")],
        features: Some(FeatureStructure {
            values: vec![FeatureValue {
                feature: "feat-frontness".to_string(),
                value: FeatureValueKind::Closed {
                    value: "val-back".to_string(),
                },
            }],
        }),
        basic_ipa_symbol: None,
    });
    let valued = compile(&valued_snapshot);

    for word in ["qta", "kumata"] {
        let inferred_direct = analyze_direct(&inferred, word);
        let explicit_direct = analyze_direct(&explicit, word);
        assert_eq!(
            inferred_direct, explicit_direct,
            "direct-HC analysis of {word:?} must match between the inferred and \
             explicit-featureless grammars"
        );
        assert_ne!(
            inferred_direct, "-",
            "expected {word:?} to have at least one analysis"
        );

        let inferred_fst = analyze_fst_confirm(&inferred, word);
        let explicit_fst = analyze_fst_confirm(&explicit, word);
        assert_eq!(
            inferred_fst, explicit_fst,
            "FST-confirm analysis of {word:?} must match between the inferred and \
             explicit-featureless grammars"
        );
        assert_ne!(
            inferred_fst, "-",
            "expected {word:?} to have at least one FST-confirmed analysis"
        );
    }

    let inferred_qta_direct = analyze_direct(&inferred, "qta");
    let valued_qta_direct = analyze_direct(&valued, "qta");
    assert_ne!(
        inferred_qta_direct, valued_qta_direct,
        "an explicitly feature-valued q must change qta's direct-HC analysis"
    );
    let inferred_qta_fst = analyze_fst_confirm(&inferred, "qta");
    let valued_qta_fst = analyze_fst_confirm(&valued, "qta");
    assert_ne!(
        inferred_qta_fst, valued_qta_fst,
        "an explicitly feature-valued q must change qta's FST-confirm analysis"
    );
}
