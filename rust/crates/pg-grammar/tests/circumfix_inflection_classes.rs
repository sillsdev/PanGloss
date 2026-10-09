//! HCLoader applies only the prefix half's classes, and only under an inflectional MSA (HCLoader.cs:1055-1069).

use pg_snapshot::lexicon::{Allomorph, LexEntry, Lexicon, Msa};
use pg_snapshot::morphology::{InflectionClass, MorphType, Morphology, PartOfSpeech};
use pg_snapshot::phonology::{BoundaryMarker, Phoneme, Phonology};
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

fn allomorph(guid: &str, morph_type: MorphType, form: &str, classes: &[&str]) -> Allomorph {
    Allomorph {
        guid: guid.to_string(),
        morph_type,
        is_abstract: false,
        forms: vec![ws("sen", form)],
        environments: Vec::new(),
        positions: Vec::new(),
        stem_name: None,
        inflection_classes: classes.iter().map(|c| c.to_string()).collect(),
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
        allomorphs: vec![allomorph(
            &format!("allo-{guid}"),
            MorphType::Stem,
            form,
            &[],
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

/// A `ma-...-u` circumfix under an inflectional MSA; `prefix_class` and `suffix_class` restrict either half.
fn snapshot(prefix_class: &[&str], suffix_class: &[&str]) -> Snapshot {
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
        phonemes: ["m", "k", "a", "t", "p", "i", "u"]
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
    let circumfix = LexEntry {
        guid: "entry-circumfix".to_string(),
        citation_form: vec![ws("sen", "ma-...-u")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![
            allomorph("allo-prefix", MorphType::Prefix, "ma", prefix_class),
            allomorph("allo-suffix", MorphType::Suffix, "u", suffix_class),
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix".to_string(),
            part_of_speech: Some("pos-noun".to_string()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
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
                circumfix,
            ],
        },
    )
}

fn parse_count(prefix_class: &[&str], suffix_class: &[&str], word: &str) -> usize {
    let (grammar, _warnings) =
        pg_grammar::compile_project(&snapshot(prefix_class, suffix_class)).expect("must compile");
    let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
    morpher.parse_word(word).analyses.len()
}

#[test]
fn prefix_half_classes_restrict_the_circumfix() {
    assert_eq!(
        parse_count(&["ic-b"], &[], "mapitu"),
        1,
        "class-B stem takes the class-B prefix half"
    );
    assert_eq!(
        parse_count(&["ic-b"], &[], "makatu"),
        0,
        "class-A stem must not take a circumfix whose prefix half requires class B (HCLoader.cs:1055-1069)"
    );
}

#[test]
fn suffix_half_classes_are_ignored() {
    assert_eq!(
        parse_count(&[], &["ic-b"], "makatu"),
        1,
        "suffix-half classes are never read, so the class-A stem parses (HCLoader.cs:1055-1069)"
    );
    assert_eq!(
        parse_count(&[], &["ic-b"], "mapitu"),
        1,
        "class-B stem parses under the same circumfix"
    );
}
