//! End-to-end pin for docs/divergences/039: all four cross-product cells of a conditioned-halves circumfix must parse (HCLoader.cs:1273-1332).

use pg_snapshot::lexicon::{Allomorph, LexEntry, Lexicon, Msa};
use pg_snapshot::morphology::{MorphType, Morphology, PartOfSpeech};
use pg_snapshot::phonology::{BoundaryMarker, Environment, NaturalClass, Phoneme, Phonology};
use pg_snapshot::project::Project;
use pg_snapshot::{FeatureSystems, Snapshot, WsForm};

use pg_grammar::model::{Grammar, MorphRuleDef, OutputAction};

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

fn simple_allomorph(guid: &str, morph_type: MorphType, form: &str) -> Allomorph {
    Allomorph {
        guid: guid.to_string(),
        morph_type,
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

/// A 2x2 conditioned-halves circumfix, mirroring `conformance-staging/edge-cases/circumfix-conditioned-halves`'s grammar.
fn snapshot() -> Snapshot {
    let noun_pos_guid = "pos-noun".to_string();
    let noun_pos = PartOfSpeech {
        guid: noun_pos_guid.clone(),
        name: "Noun".to_string(),
        abbreviation: "n".to_string(),
        children: Vec::new(),
        inflection_classes: Vec::new(),
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: Vec::new(),
        affix_templates: Vec::new(),
    };

    let phonology = Phonology {
        phonemes: vec![
            phoneme("ph-a", "a"),
            phoneme("ph-b", "b"),
            phoneme("ph-p", "p"),
            phoneme("ph-u", "u"),
            phoneme("ph-k", "k"),
            phoneme("ph-i", "i"),
            phoneme("ph-m", "m"),
            phoneme("ph-o", "o"),
            phoneme("ph-z", "z"),
        ],
        boundary_markers: vec![BoundaryMarker {
            guid: "bd-plus".to_string(),
            name: "+".to_string(),
            representations: vec![ws("sen", "+")],
        }],
        natural_classes: vec![
            NaturalClass::Segments {
                guid: "nc-v".to_string(),
                name: "V".to_string(),
                phonemes: vec!["ph-a".to_string()],
            },
            NaturalClass::Segments {
                guid: "nc-c".to_string(),
                name: "C".to_string(),
                phonemes: vec!["ph-b".to_string()],
            },
        ],
        environments: vec![
            Environment {
                guid: "env-stem-starts-v".to_string(),
                name: "env-stem-starts-v".to_string(),
                representation: "/_[V]".to_string(),
            },
            Environment {
                guid: "env-stem-starts-c".to_string(),
                name: "env-stem-starts-c".to_string(),
                representation: "/_[C]".to_string(),
            },
            Environment {
                guid: "env-stem-ends-v".to_string(),
                name: "env-stem-ends-v".to_string(),
                representation: "/[V]_".to_string(),
            },
            Environment {
                guid: "env-stem-ends-c".to_string(),
                name: "env-stem-ends-c".to_string(),
                representation: "/[C]_".to_string(),
            },
        ],
        ..Phonology::default()
    };

    let mut prefix_pu = simple_allomorph("allo-prefix-pu", MorphType::Prefix, "pu");
    prefix_pu.environments = vec!["env-stem-starts-v".to_string()];
    let mut prefix_ki = simple_allomorph("allo-prefix-ki", MorphType::Prefix, "ki");
    prefix_ki.environments = vec!["env-stem-starts-c".to_string()];
    let mut suffix_mo = simple_allomorph("allo-suffix-mo", MorphType::Suffix, "mo");
    suffix_mo.environments = vec!["env-stem-ends-v".to_string()];
    let mut suffix_zo = simple_allomorph("allo-suffix-zo", MorphType::Suffix, "zo");
    suffix_zo.environments = vec!["env-stem-ends-c".to_string()];

    let mut entries = vec![LexEntry {
        guid: "entry-circumfix".to_string(),
        citation_form: vec![ws("sen", "ki-...-zo")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![prefix_pu, prefix_ki, suffix_mo, suffix_zo],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix".to_string(),
            part_of_speech: Some(noun_pos_guid.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    }];
    // One root per VV/VC/CV/CC stem shape, matching the 2x2 cross-product's edge gates.
    for (guid, form) in [
        ("entry-root-vv", "aa"),
        ("entry-root-vc", "ab"),
        ("entry-root-cv", "ba"),
        ("entry-root-cc", "bb"),
    ] {
        entries.push(LexEntry {
            guid: guid.to_string(),
            citation_form: vec![ws("sen", form)],
            lexeme_morph_type: MorphType::Stem,
            allomorphs: vec![simple_allomorph(
                &format!("allo-{guid}"),
                MorphType::Stem,
                form,
            )],
            msas: vec![Msa::Stem {
                guid: format!("msa-{guid}"),
                part_of_speech: Some(noun_pos_guid.clone()),
                inflection_class: None,
                features: None,
                exception_features: Vec::new(),
                from_parts_of_speech: Vec::new(),
                slots: Vec::new(),
            }],
            senses: Vec::new(),
            entry_refs: Vec::new(),
        });
    }

    Snapshot::new(
        Project {
            name: "Test".to_string(),
            vernacular_writing_systems: vec!["sen".to_string()],
            analysis_writing_systems: vec!["en".to_string()],
            exemplar_characters: Vec::new(),
        },
        FeatureSystems::default(),
        phonology,
        Morphology {
            parts_of_speech: vec![noun_pos],
            ..Morphology::default()
        },
        Lexicon { entries },
    )
}

/// Every allomorph across `grammar.mrules` shaped like a circumfix cross-product cell (leading+trailing insert around one copy).
fn circumfix_rule_allomorph_count(grammar: &Grammar) -> usize {
    grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(a) => Some(a),
            _ => None,
        })
        .flat_map(|a| a.allomorphs.iter())
        .filter(|allo| {
            matches!(
                allo.rhs.as_slice(),
                [
                    OutputAction::InsertSegments { .. },
                    OutputAction::Copy(_),
                    OutputAction::InsertSegments { .. }
                ]
            )
        })
        .count()
}

#[test]
fn circumfix_cross_product_with_conditioned_halves_parses_all_four_cells() {
    let snapshot = snapshot();
    let (grammar, warnings) = pg_grammar::compile_project(&snapshot).expect("must compile");
    assert!(
        !warnings.iter().any(|w| w.contains("circumfix")),
        "unexpected circumfix warnings: {warnings:?}"
    );
    assert_eq!(
        circumfix_rule_allomorph_count(&grammar),
        4,
        "2 prefix halves x 2 suffix halves is a 2x2 cross-product"
    );

    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    for word in ["puaamo", "puabzo", "kibamo", "kibbzo"] {
        let outcome = morpher.parse_word(word);
        assert!(
            !outcome.analyses.is_empty(),
            "{word:?} is a matching cross-product cell and must parse under the HCLoader shape \
             (HCLoader.cs:1273-1332); the environment-union encoding parses only the first-declared cell"
        );
    }
    for word in ["puabmo", "pubbmo"] {
        let outcome = morpher.parse_word(word);
        assert!(
            outcome.analyses.is_empty(),
            "{word:?} wraps subVV's literal affixes (pu-...-mo) around a stem that fails its \
             embedded V/C gate and must not parse; got {:?}",
            outcome.analyses
        );
    }
}
