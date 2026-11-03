//! Issue 148: affix forms written as reduplication bracket patterns (`[C^1][V^1]`) load as process allomorphs, as C# HermitCrab loads them (synthetic). The same words, accepted and rejected, are pinned for the HC-XML grammar by `conformance-staging/edge-cases/bracket-pattern-reduplication-affix`, whose accept set was replayed on hc.dll.

use pg_parse::Morpher;
use pg_snapshot::lexicon::{Allomorph, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{MorphType, Morphology, PartOfSpeech};
use pg_snapshot::phonology::{BoundaryMarker, Environment, NaturalClass, Phoneme, Phonology};
use pg_snapshot::project::Project;
use pg_snapshot::{FeatureSystems, Snapshot, WsForm};

fn ws(form: &str) -> WsForm {
    WsForm {
        ws: "sen".to_string(),
        form: form.to_string(),
    }
}

fn phoneme(rep: &str) -> Phoneme {
    Phoneme {
        guid: format!("ph-{rep}"),
        name: rep.to_string(),
        representations: vec![ws(rep)],
        features: None,
        basic_ipa_symbol: None,
    }
}

fn allomorph(guid: &str, morph_type: MorphType, form: &str, envs: &[&str]) -> Allomorph {
    Allomorph {
        guid: guid.to_string(),
        morph_type,
        is_abstract: false,
        forms: vec![ws(form)],
        environments: envs.iter().map(|e| e.to_string()).collect(),
        positions: Vec::new(),
        stem_name: None,
        inflection_classes: Vec::new(),
        ms_env_features: None,
        ms_env_part_of_speech: None,
        process: None,
    }
}

fn stem_entry(n: &str, form: &str) -> LexEntry {
    LexEntry {
        guid: format!("entry-{n}"),
        citation_form: vec![ws(form)],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![allomorph(&format!("allo-{n}"), MorphType::Stem, form, &[])],
        msas: vec![Msa::Stem {
            guid: format!("msa-{n}"),
            part_of_speech: Some("pos-verb".to_string()),
            inflection_class: None,
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: Vec::new(),
            slots: Vec::new(),
        }],
        senses: vec![Sense {
            guid: format!("sense-{n}"),
            gloss: vec![WsForm {
                ws: "en".to_string(),
                form: n.to_string(),
            }],
            definition: Vec::new(),
            msa: Some(format!("msa-{n}")),
        }],
        entry_refs: Vec::new(),
    }
}

fn affix_entry(n: &str, morph_type: MorphType, form: &str, env: &str) -> LexEntry {
    LexEntry {
        guid: format!("entry-{n}"),
        citation_form: vec![ws(form)],
        lexeme_morph_type: morph_type,
        allomorphs: vec![allomorph(&format!("allo-{n}"), morph_type, form, &[env])],
        msas: vec![Msa::Inflectional {
            guid: format!("msa-{n}"),
            part_of_speech: Some("pos-verb".to_string()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: vec![Sense {
            guid: format!("sense-{n}"),
            gloss: vec![WsForm {
                ws: "en".to_string(),
                form: n.to_string(),
            }],
            definition: Vec::new(),
            msa: Some(format!("msa-{n}")),
        }],
        entry_refs: Vec::new(),
    }
}

fn snapshot() -> Snapshot {
    let verb = PartOfSpeech {
        guid: "pos-verb".to_string(),
        name: "Verb".to_string(),
        abbreviation: "v".to_string(),
        children: Vec::new(),
        inflection_classes: Vec::new(),
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: Vec::new(),
        affix_templates: Vec::new(),
    };
    let mut phonology = Phonology {
        phonemes: ["k", "t", "m", "s", "a", "i", "u"]
            .into_iter()
            .map(phoneme)
            .collect(),
        boundary_markers: vec![BoundaryMarker {
            guid: "bd-plus".to_string(),
            name: "+".to_string(),
            representations: vec![ws("+")],
        }],
        ..Phonology::default()
    };
    for (guid, name, members) in [
        ("nc-c", "C", ["k", "t", "m", "s"].as_slice()),
        ("nc-v", "V", ["a", "i", "u"].as_slice()),
    ] {
        phonology.natural_classes.push(NaturalClass::Segments {
            guid: guid.to_string(),
            name: name.to_string(),
            display_name: None,
            phonemes: members.iter().map(|p| format!("ph-{p}")).collect(),
        });
    }
    for (guid, representation) in [
        ("env-prefix", "/ _ [C^1][V^1]"),
        ("env-suffix", "/ [C^1][V^1] _"),
    ] {
        phonology.environments.push(Environment {
            guid: guid.to_string(),
            name: String::new(),
            representation: representation.to_string(),
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
            parts_of_speech: vec![verb],
            ..Morphology::default()
        },
        Lexicon {
            msa_equality: None,
            entries: vec![
                stem_entry("kuma", "kuma"),
                stem_entry("timas", "timas"),
                affix_entry("pfx", MorphType::Prefix, "[C^1][V^1]", "env-prefix"),
                affix_entry("sfx", MorphType::Suffix, "[C^1][V^1]", "env-suffix"),
            ],
        },
    )
}

#[test]
fn bracket_pattern_affix_forms_load_and_copy_the_matched_segments() {
    let (grammar, warnings) = pg_grammar::compile_project(&snapshot()).expect("fixture compiles");
    assert!(
        !warnings
            .iter()
            .any(|w| w.code.contains("reduplication") || w.code.contains("unsupported")),
        "no reduplication warning: {warnings:?}"
    );
    let morpher = Morpher::new(&grammar, usize::MAX);
    let analyses = |word: &str| morpher.parse_word(word).analyses.len();
    for word in ["kuma", "kukuma", "kumama", "timas", "titimas"] {
        assert_eq!(analyses(word), 1, "{word}");
    }
    assert_eq!(analyses("kukumama"), 2, "either rule order, as in hc.dll");
    for word in ["tikuma", "kumata", "timasas", "kumaa"] {
        assert_eq!(analyses(word), 0, "{word}");
    }
}
