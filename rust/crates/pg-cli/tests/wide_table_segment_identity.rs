//! A literal segment in an allomorph environment or a rewrite-rule environment keeps its identity whatever the char-def count, featured or featureless (synthetic).

use pg_parse::Morpher;
use pg_snapshot::lexicon::{Allomorph, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{AffixSlot, AffixTemplate, MorphType, Morphology, PartOfSpeech};
use pg_snapshot::phonology::{
    BoundaryMarker, Environment, PhonContext, Phoneme, PhonologicalRule, Phonology, RewriteRhs,
    RewriteRule, RuleDirection,
};
use pg_snapshot::project::Project;
use pg_snapshot::{
    ClosedFeature, FeatureStructure, FeatureSystems, FeatureValue, FeatureValueKind,
    FeatureValueSymbol, Snapshot, WsForm,
};

fn ws(form: &str) -> WsForm {
    WsForm {
        ws: "sen".to_string(),
        form: form.to_string(),
    }
}

fn closed(feature: &str, value: &str) -> FeatureValue {
    FeatureValue {
        feature: feature.to_string(),
        value: FeatureValueKind::Closed {
            value: value.to_string(),
        },
    }
}

/// Every authored segment has its own feature bundle; the inferred letter `x` has none, so a feature-only match cannot tell it from the authored segment an environment names.
fn phoneme(rep: &str, featured: bool) -> Phoneme {
    let features = featured.then(|| {
        let mut values = Vec::new();
        if "aiuo".contains(rep) {
            values.push(closed("f-syll", "v-syll-plus"));
            values.push(closed(
                "f-round",
                if rep == "o" {
                    "v-round-plus"
                } else {
                    "v-round-minus"
                },
            ));
        } else {
            values.push(closed("f-syll", "v-syll-minus"));
        }
        FeatureStructure { values }
    });
    Phoneme {
        guid: format!("ph-{rep}"),
        name: rep.to_string(),
        representations: vec![ws(rep)],
        features,
        basic_ipa_symbol: None,
    }
}

fn feature_systems() -> FeatureSystems {
    let symbol = |guid: &str, abbreviation: &str| FeatureValueSymbol {
        guid: guid.to_string(),
        name: abbreviation.to_string(),
        abbreviation: abbreviation.to_string(),
    };
    let mut systems = FeatureSystems::default();
    systems.phonological.closed_features = vec![
        ClosedFeature {
            guid: "f-syll".to_string(),
            name: "syllabic".to_string(),
            abbreviation: "syll".to_string(),
            values: vec![symbol("v-syll-plus", "+"), symbol("v-syll-minus", "-")],
        },
        ClosedFeature {
            guid: "f-round".to_string(),
            name: "round".to_string(),
            abbreviation: "rnd".to_string(),
            values: vec![symbol("v-round-plus", "+"), symbol("v-round-minus", "-")],
        },
    ];
    systems
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

#[derive(Clone, Copy, Debug)]
enum Scenario {
    /// Suffix allomorph `hun` is licensed only after the literal `a`; plain `un` elsewhere.
    SuffixAfterA,
    /// Rewrite `a -> o / k _`.
    RewriteAfterK,
}

fn snapshot(scenario: Scenario, featured: bool, extra_letters: usize) -> Snapshot {
    let filler: String = (0..extra_letters)
        .filter_map(|i| char::from_u32(0x4E00 + i as u32))
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
        allomorphs: vec![
            allomorph("allo-hun", MorphType::Suffix, "hun", &["env-after-a"]),
            allomorph("allo-un", MorphType::Suffix, "un", &[]),
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-un".to_string(),
            part_of_speech: Some("pos-verb".to_string()),
            slots: vec!["slot-pass".to_string()],
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
        phonemes: ["k", "t", "m", "s", "h", "n", "a", "i", "u", "o"]
            .into_iter()
            .map(|rep| phoneme(rep, featured))
            .collect(),
        boundary_markers: vec![BoundaryMarker {
            guid: "bd-plus".to_string(),
            name: "+".to_string(),
            representations: vec![ws("+")],
        }],
        ..Phonology::default()
    };
    phonology.environments.push(Environment {
        guid: "env-after-a".to_string(),
        name: String::new(),
        representation: "/ a _".to_string(),
    });
    let mut entries = Vec::new();
    if !filler.is_empty() {
        entries.push(stem_entry("filler", &filler));
    }
    match scenario {
        Scenario::SuffixAfterA => {
            entries.push(stem_entry("astem", "kuma"));
            entries.push(stem_entry("xstem", "kumx"));
            entries.push(suffix);
        }
        Scenario::RewriteAfterK => {
            entries.push(stem_entry("kstem", "kat"));
            entries.push(stem_entry("xstem", "xat"));
            phonology.rules.push(PhonologicalRule::Rewrite(RewriteRule {
                guid: "rule-a-o".to_string(),
                name: "a to o after k".to_string(),
                direction: RuleDirection::LeftToRight,
                structural_description: vec![PhonContext::Segment {
                    phoneme: "ph-a".to_string(),
                }],
                feature_constraint_variables: Vec::new(),
                right_hand_sides: vec![RewriteRhs {
                    structural_change: vec![PhonContext::Segment {
                        phoneme: "ph-o".to_string(),
                    }],
                    left_context: Some(PhonContext::Segment {
                        phoneme: "ph-k".to_string(),
                    }),
                    ..RewriteRhs::default()
                }],
            }));
        }
    }
    Snapshot::new(
        Project {
            name: "Test".to_string(),
            vernacular_writing_systems: vec!["sen".to_string()],
            analysis_writing_systems: vec!["en".to_string()],
            exemplar_characters: Vec::new(),
        },
        if featured {
            feature_systems()
        } else {
            FeatureSystems::default()
        },
        phonology,
        Morphology {
            parts_of_speech: vec![verb],
            ..Morphology::default()
        },
        Lexicon { entries },
    )
}

fn parses(morpher: &Morpher, word: &str) -> bool {
    !morpher.parse_word(word).analyses.is_empty()
}

/// Build the scenario padded with filler letters until the char-def table holds exactly `target` entries (`None`: no padding).
fn compile(
    scenario: Scenario,
    featured: bool,
    target: Option<usize>,
) -> (pg_grammar::model::Grammar, usize) {
    let (base, _) =
        pg_grammar::compile_project(&snapshot(scenario, featured, 0)).expect("compiles");
    let base_len = base.char_tables[0].len();
    let extra = target.map_or(0, |t| t.checked_sub(base_len).expect("target above base"));
    let (grammar, _) =
        pg_grammar::compile_project(&snapshot(scenario, featured, extra)).expect("compiles");
    // The filler stem adds exactly its own letters, so a request for `target` defs must land on it.
    let len = grammar.char_tables[0].len();
    if let Some(t) = target {
        assert_eq!(
            len, t,
            "char-def table must hold exactly the requested count"
        );
    }
    (grammar, len)
}

fn check(scenario: Scenario, featured: bool, target: Option<usize>) {
    let (grammar, len) = compile(scenario, featured, target);
    let morpher = Morpher::new(&grammar, usize::MAX);
    let ctx = format!("{scenario:?} featured={featured} char-defs={len}");
    match scenario {
        Scenario::SuffixAfterA => {
            assert!(parses(&morpher, "kuma"), "bare a-stem ({ctx})");
            assert!(parses(&morpher, "kumahun"), "hun after a ({ctx})");
            assert!(!parses(&morpher, "kumaun"), "un after a is blocked ({ctx})");
            assert!(
                parses(&morpher, "kumxun"),
                "un after the featureless x ({ctx})"
            );
            assert!(
                !parses(&morpher, "kumxhun"),
                "hun after x violates its environment ({ctx})"
            );
        }
        Scenario::RewriteAfterK => {
            assert!(parses(&morpher, "kot"), "a becomes o after k ({ctx})");
            assert!(
                !parses(&morpher, "kat"),
                "a after k must have been rewritten ({ctx})"
            );
            assert!(parses(&morpher, "xat"), "a after x is untouched ({ctx})");
            assert!(
                !parses(&morpher, "xot"),
                "the featureless x is not k ({ctx})"
            );
        }
    }
}

#[test]
fn featured_suffix_environment_names_a_segment_on_a_wide_table() {
    check(Scenario::SuffixAfterA, true, Some(80));
}

#[test]
fn featured_rewrite_environment_names_a_segment_on_a_wide_table() {
    check(Scenario::RewriteAfterK, true, Some(80));
}

#[test]
fn featured_suffix_environment_names_a_segment_on_a_very_wide_table() {
    check(Scenario::SuffixAfterA, true, Some(420));
}

#[test]
fn featured_rewrite_environment_names_a_segment_on_a_very_wide_table() {
    check(Scenario::RewriteAfterK, true, Some(420));
}

#[test]
fn small_tables_keep_identity_featured_and_featureless() {
    for featured in [false, true] {
        check(Scenario::SuffixAfterA, featured, None);
        check(Scenario::RewriteAfterK, featured, None);
    }
}

#[test]
fn suffix_identity_holds_at_exactly_64_and_65_char_defs() {
    for featured in [false, true] {
        for target in [64, 65] {
            check(Scenario::SuffixAfterA, featured, Some(target));
        }
    }
}

#[test]
fn rewrite_identity_holds_at_exactly_64_and_65_char_defs() {
    for featured in [false, true] {
        for target in [64, 65] {
            check(Scenario::RewriteAfterK, featured, Some(target));
        }
    }
}
