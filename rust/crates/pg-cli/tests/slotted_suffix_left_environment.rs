//! CEB-14: a left environment on a suffix allomorph holds on a featureless grammar with over 64 letters (synthetic).

use pg_parse::Morpher;
use pg_snapshot::lexicon::{Allomorph, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{AffixSlot, AffixTemplate, MorphType, Morphology, PartOfSpeech};
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

fn snapshot(slotted: bool, extra_letters: u32) -> Snapshot {
    let filler: String = (0..extra_letters)
        .filter_map(|i| char::from_u32(0x0430 + i))
        .collect();
    let verb = PartOfSpeech {
        guid: "pos-verb".to_string(),
        name: "Verb".to_string(),
        abbreviation: "v".to_string(),
        children: Vec::new(),
        inflection_classes: Vec::new(),
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: vec![AffixSlot {
            guid: "slot-pass".to_string(),
            name: "Passive suffix".to_string(),
            optional: true,
        }],
        affix_templates: vec![AffixTemplate {
            guid: "tmpl-verb".to_string(),
            name: "VerbTemplate".to_string(),
            disabled: false,
            prefix_slots: Vec::new(),
            suffix_slots: vec!["slot-pass".to_string()],
            is_final: true,
        }],
    };
    let suffix = LexEntry {
        guid: "entry-un".to_string(),
        citation_form: vec![ws("-un")],
        lexeme_morph_type: MorphType::Suffix,
        // Alternates first, lexeme form last, as the FieldWorks extract orders them.
        allomorphs: vec![
            allomorph("allo-hun", MorphType::Suffix, "hun", &["env-after-vowel"]),
            allomorph("allo-un", MorphType::Suffix, "un", &[]),
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-un".to_string(),
            part_of_speech: Some("pos-verb".to_string()),
            slots: if slotted {
                vec!["slot-pass".to_string()]
            } else {
                Vec::new()
            },
            features: None,
            exception_features: Vec::new(),
        }],
        senses: vec![Sense {
            guid: "sense-un".to_string(),
            gloss: vec![WsForm {
                ws: "en".to_string(),
                form: "PASS".to_string(),
            }],
            definition: Vec::new(),
            msa: Some("msa-un".to_string()),
        }],
        entry_refs: Vec::new(),
    };
    let mut phonology = Phonology {
        phonemes: ["k", "t", "m", "s", "h", "n", "a", "i", "u"]
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
    phonology.natural_classes.push(NaturalClass::Segments {
        guid: "nc-vowel".to_string(),
        name: "V".to_string(),
        display_name: None,
        phonemes: vec!["ph-a".to_string(), "ph-i".to_string(), "ph-u".to_string()],
    });
    phonology.environments.push(Environment {
        guid: "env-after-vowel".to_string(),
        name: String::new(),
        representation: "/ [V] _".to_string(),
    });
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
                stem_entry("vowelstem", "kuma"),
                stem_entry("consstem", "kam"),
                stem_entry("filler", &filler),
                suffix,
            ],
        },
    )
}

fn parses(morpher: &Morpher, word: &str) -> bool {
    !morpher.parse_word(word).analyses.is_empty()
}

fn check(slotted: bool, extra_letters: u32) {
    let snapshot = snapshot(slotted, extra_letters);
    let (grammar, _) = pg_grammar::compile_project(&snapshot).expect("fixture compiles");
    let morpher = Morpher::new(&grammar, usize::MAX);
    assert!(parses(&morpher, "kuma"), "bare vowel stem");
    assert!(parses(&morpher, "kumahun"), "hun after a vowel");
    assert!(
        !parses(&morpher, "kumaun"),
        "plain un after a vowel is blocked by hun"
    );
    assert!(
        parses(&morpher, "kamun"),
        "plain un after a consonant (slotted={slotted})"
    );
    assert!(
        !parses(&morpher, "kamhun"),
        "hun after a consonant violates its left environment (slotted={slotted})"
    );
}

/// The `-un` suffix rule: the only affix rule carrying the `hun`/`un` allomorph pair.
fn suffix_rule(grammar: &pg_grammar::model::Grammar) -> u32 {
    let ids: Vec<u32> = (0..grammar.mrules.len() as u32)
        .filter(|&i| {
            matches!(&grammar.mrules[i as usize],
                pg_grammar::model::MorphRuleDef::AffixProcess(r) if r.allomorphs.len() == 2)
        })
        .collect();
    assert_eq!(ids.len(), 1, "exactly one two-allomorph suffix rule");
    ids[0]
}

/// Words the suffix rule produces while unapplying from `word`, which parse results alone do not reveal.
fn analysis_outputs(morpher: &Morpher, rule: u32, word: &str) -> u64 {
    let (_, rows) = morpher.parse_word_with_stats(word, &pg_parse::ParseOptions::default());
    rows.iter()
        .filter(|r| {
            r.kind == pg_rules::stats::ObjectKind::MorphRule
                && r.direction == pg_rules::stats::Direction::Analysis
                && r.object_index == rule
        })
        .map(|r| r.counters.outputs)
        .sum()
}

fn check_analysis_literal(extra_letters: u32) {
    let snapshot = snapshot(false, extra_letters);
    let (grammar, _) = pg_grammar::compile_project(&snapshot).expect("fixture compiles");
    let morpher = Morpher::new(&grammar, usize::MAX);
    let rule = suffix_rule(&grammar);
    assert!(parses(&morpher, "kamun"), "kam + un");
    assert!(
        analysis_outputs(&morpher, rule, "kamun") > 0,
        "un unapplies from a word ending in un (extra_letters={extra_letters})"
    );
    assert!(!parses(&morpher, "kamat"), "no suffix ends in at");
    assert_eq!(
        analysis_outputs(&morpher, rule, "kamat"),
        0,
        "neither un nor hun unapplies from a word ending in at (extra_letters={extra_letters})"
    );
}

#[test]
fn analysis_suffix_literal_matches_only_itself() {
    check_analysis_literal(0);
}

#[test]
fn analysis_suffix_literal_matches_only_itself_on_a_large_letter_inventory() {
    check_analysis_literal(80);
}

#[test]
fn wide_featureless_suffix_requires_both_literal_segments() {
    let (grammar, _) = pg_grammar::compile_project(&snapshot(false, 80)).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX);
    let rule = suffix_rule(&grammar);
    assert!(analysis_outputs(&morpher, rule, "kamun") > 0);
    for word in ["kamum", "kaman"] {
        assert_eq!(analysis_outputs(&morpher, rule, word), 0, "{word}");
    }
}

#[test]
fn unslotted_inflectional_suffix_enforces_left_environment() {
    check(false, 0);
}

#[test]
fn slotted_inflectional_suffix_enforces_left_environment() {
    check(true, 0);
}

#[test]
fn slotted_left_environment_survives_a_large_letter_inventory() {
    check(true, 80);
}

#[test]
fn unslotted_left_environment_survives_a_large_letter_inventory() {
    check(false, 80);
}
