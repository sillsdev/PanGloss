//! HCLoader applies a process allomorph's classes only under an inflectional MSA (HCLoader.cs:1094-1097).

use pg_snapshot::lexicon::{AffixProcess, Allomorph, LexEntry, Lexicon, Msa, RuleMapping};
use pg_snapshot::morphology::{InflectionClass, MorphType, Morphology, PartOfSpeech};
use pg_snapshot::phonology::{BoundaryMarker, PhonContext, Phoneme, Phonology};
use pg_snapshot::project::Project;
use pg_snapshot::{FeatureSystems, Snapshot, WsForm};

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

fn ic(guid: &str) -> InflectionClass {
    InflectionClass {
        guid: guid.to_string(),
        name: guid.to_string(),
        abbreviation: guid.to_string(),
        children: Vec::new(),
    }
}

fn stem(guid: &str, form: &str, class: &str) -> LexEntry {
    LexEntry {
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
            part_of_speech: Some("pos-noun".to_string()),
            inflection_class: Some(class.to_string()),
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: Vec::new(),
            slots: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    }
}

/// Suffix -u restricted to class B; `process` picks MoAffixProcess, `derivational` the MSA kind.
fn snapshot(process: bool, derivational: bool) -> Snapshot {
    let noun = PartOfSpeech {
        guid: "pos-noun".to_string(),
        name: "Noun".to_string(),
        abbreviation: "n".to_string(),
        children: Vec::new(),
        inflection_classes: vec![ic("ic-a"), ic("ic-b")],
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: Vec::new(),
        affix_templates: Vec::new(),
    };
    let phonology = Phonology {
        phonemes: ["k", "a", "t", "p", "i", "u"]
            .iter()
            .map(|p| phoneme(&format!("ph-{p}"), p))
            .collect(),
        boundary_markers: vec![BoundaryMarker {
            guid: "bd-plus".to_string(),
            name: "+".to_string(),
            representations: vec![ws("sen", "+")],
        }],
        ..Phonology::default()
    };
    let mut suffix = simple_allomorph("allo-suf", MorphType::Suffix, "u");
    suffix.inflection_classes = vec!["ic-b".to_string()];
    if process {
        suffix.forms = Vec::new();
        suffix.process = Some(AffixProcess {
            input: vec![PhonContext::Variable],
            output: vec![
                RuleMapping::CopyFromInput { part: 1 },
                RuleMapping::InsertSegments {
                    text: "u".to_string(),
                },
            ],
        });
    }
    let msa = if derivational {
        Msa::Derivational {
            guid: "msa-suf".to_string(),
            from_part_of_speech: Some("pos-noun".to_string()),
            to_part_of_speech: Some("pos-noun".to_string()),
            from_features: None,
            to_features: None,
            from_inflection_class: None,
            to_inflection_class: None,
            from_exception_features: Vec::new(),
            to_exception_features: Vec::new(),
            from_stem_name: None,
        }
    } else {
        Msa::Inflectional {
            guid: "msa-suf".to_string(),
            part_of_speech: Some("pos-noun".to_string()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }
    };
    let suf = LexEntry {
        guid: "entry-suf".to_string(),
        citation_form: vec![ws("sen", "u")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![suffix],
        msas: vec![msa],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
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
            parts_of_speech: vec![noun],
            ..Morphology::default()
        },
        Lexicon {
            entries: vec![
                stem("root-a", "kat", "ic-a"),
                stem("root-b", "pit", "ic-b"),
                suf,
            ],
        },
    )
}

fn parse_count(process: bool, derivational: bool, word: &str) -> usize {
    let (grammar, _warnings) =
        pg_grammar::compile_project(&snapshot(process, derivational)).expect("must compile");
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    morpher.parse_word(word).analyses.len()
}

#[test]
fn ordinary_allomorph_respects_inflection_class() {
    assert_eq!(
        parse_count(false, false, "pitu"),
        1,
        "class-B stem takes the class-B suffix"
    );
    assert_eq!(
        parse_count(false, false, "katu"),
        0,
        "class-A stem must not take a class-B suffix"
    );
}

#[test]
fn process_allomorph_respects_inflection_class() {
    assert_eq!(
        parse_count(true, false, "pitu"),
        1,
        "class-B stem takes the class-B suffix"
    );
    assert_eq!(
        parse_count(true, false, "katu"),
        0,
        "class-A stem must not take a class-B suffix (HCLoader.cs:1094-1097)"
    );
}

#[test]
fn derivational_process_allomorph_ignores_allomorph_inflection_classes() {
    assert_eq!(
        parse_count(true, true, "pitu"),
        1,
        "derivational MSA: class-B stem still parses"
    );
    assert_eq!(
        parse_count(true, true, "katu"),
        1,
        "derivational MSA: allomorph classes are not applied (HCLoader.cs:1094-1097)"
    );
}
