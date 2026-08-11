//! Unit tests for `pg_grammar::compile`, built entirely from code-constructed `Snapshot` values (no `.fwdata`/oracle files).

use pg_snapshot::feature::{
    ClosedFeature, FeatureStructure, FeatureSystem, FeatureValue, FeatureValueKind,
    FeatureValueSymbol,
};
use pg_snapshot::lexicon::{Allomorph, EntryRef, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{
    AffixSlot, AffixTemplate, InflectionClass, LexEntryInflType, MorphType, Morphology,
    PartOfSpeech,
};
use pg_snapshot::phonology::{
    BoundaryMarker, MetathesisRule, NaturalClass as SnapNaturalClass, Phoneme, PhonologicalRule,
    Phonology, RuleDirection,
};
use pg_snapshot::project::Project;
use pg_snapshot::{FeatureSystems, Snapshot, WsForm};

use crate::model::{MorphRuleDef, OutputAction, PartRef};

use super::{compile_project, environment};

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

fn boundary(guid: &str, rep: &str) -> BoundaryMarker {
    BoundaryMarker {
        guid: guid.to_string(),
        name: rep.to_string(),
        representations: vec![ws("sen", rep)],
    }
}

fn base_phonology() -> Phonology {
    Phonology {
        phonemes: vec![
            phoneme("ph-k", "k"),
            phoneme("ph-t", "t"),
            phoneme("ph-m", "m"),
            phoneme("ph-s", "s"),
            phoneme("ph-a", "a"),
            phoneme("ph-i", "i"),
            phoneme("ph-u", "u"),
        ],
        boundary_markers: vec![boundary("bd-plus", "+")],
        ..Phonology::default()
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

/// One POS ("Noun") with one affix slot and one template using it, a stem entry, and an inflectional suffix entry filling that slot; every test below starts from this and mutates the parts it cares about.
struct Fixture {
    noun_pos: String,
    slot: String,
    template: String,
    stem_entry: String,
    stem_msa: String,
    suffix_entry: String,
    suffix_msa: String,
}

fn fixture() -> (Snapshot, Fixture) {
    let f = Fixture {
        noun_pos: "pos-noun".to_string(),
        slot: "slot-pl".to_string(),
        template: "tmpl-noun".to_string(),
        stem_entry: "entry-stem".to_string(),
        stem_msa: "msa-stem".to_string(),
        suffix_entry: "entry-suffix".to_string(),
        suffix_msa: "msa-suffix".to_string(),
    };

    let noun_pos = PartOfSpeech {
        guid: f.noun_pos.clone(),
        name: "Noun".to_string(),
        abbreviation: "n".to_string(),
        children: Vec::new(),
        inflection_classes: Vec::new(),
        default_inflection_class: None,
        inflectable_features: Vec::new(),
        stem_names: Vec::new(),
        affix_slots: vec![AffixSlot {
            guid: f.slot.clone(),
            name: "Pl".to_string(),
            optional: false,
        }],
        affix_templates: vec![AffixTemplate {
            guid: f.template.clone(),
            name: "NounTemplate".to_string(),
            disabled: false,
            prefix_slots: Vec::new(),
            suffix_slots: vec![f.slot.clone()],
            is_final: true,
        }],
    };

    let stem_entry = LexEntry {
        guid: f.stem_entry.clone(),
        citation_form: vec![ws("sen", "kuma")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-stem", MorphType::Stem, "kuma")],
        msas: vec![Msa::Stem {
            guid: f.stem_msa.clone(),
            part_of_speech: Some(f.noun_pos.clone()),
            inflection_class: None,
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: Vec::new(),
            slots: Vec::new(),
        }],
        senses: vec![Sense {
            guid: "sense-stem".to_string(),
            gloss: vec![ws("en", "dog")],
            definition: Vec::new(),
            msa: Some(f.stem_msa.clone()),
        }],
        entry_refs: Vec::new(),
    };

    let suffix_entry = LexEntry {
        guid: f.suffix_entry.clone(),
        citation_form: vec![ws("sen", "-ta")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph("allo-suffix", MorphType::Suffix, "ta")],
        msas: vec![Msa::Inflectional {
            guid: f.suffix_msa.clone(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: vec![f.slot.clone()],
            features: None,
            exception_features: Vec::new(),
        }],
        senses: vec![Sense {
            guid: "sense-suffix".to_string(),
            gloss: vec![ws("en", "PL")],
            definition: Vec::new(),
            msa: Some(f.suffix_msa.clone()),
        }],
        entry_refs: Vec::new(),
    };

    let snapshot = Snapshot::new(
        Project {
            name: "Test".to_string(),
            vernacular_writing_systems: vec!["sen".to_string()],
            analysis_writing_systems: vec!["en".to_string()],
        },
        FeatureSystems::default(),
        base_phonology(),
        Morphology {
            parts_of_speech: vec![noun_pos],
            ..Morphology::default()
        },
        Lexicon {
            entries: vec![stem_entry, suffix_entry],
        },
    );

    (snapshot, f)
}

// --- 1. stem + inflectional affix + template ------------------------------------------------

#[test]
fn stem_and_inflectional_affix_and_template_compile_into_expected_grammar() {
    let (snapshot, f) = fixture();
    let (grammar, warnings) = compile_project(&snapshot).expect("fixture must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    assert_eq!(grammar.entries.len(), 1, "one stem entry expected");
    match &grammar.syn_features.features[grammar.syn_features.pos.0 as usize].kind {
        crate::model::SynFeatureKind::Symbolic { symbols, .. } => {
            assert!(symbols.iter().any(|(id, _)| id == &f.noun_pos));
        }
        crate::model::SynFeatureKind::Complex => panic!("POS must be symbolic"),
    }
    let stem_morpheme = &grammar.morphemes[grammar.entries[0].morpheme.0 as usize];
    assert_eq!(stem_morpheme.gloss.as_deref(), Some("dog"));
    assert_eq!(stem_morpheme.xml_key, f.stem_msa);
    assert_eq!(grammar.entries[0].authored_id, f.stem_entry);

    let affix_rules: Vec<_> = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(affix_rules.len(), 1, "one inflectional affix rule expected");
    assert_eq!(affix_rules[0].allomorphs.len(), 1);
    let rule_morpheme = &grammar.morphemes[affix_rules[0].morpheme.0 as usize];
    assert_eq!(rule_morpheme.gloss.as_deref(), Some("PL"));
    assert_eq!(rule_morpheme.xml_key, f.suffix_msa);

    assert_eq!(grammar.templates.len(), 1, "one template expected");
    assert_eq!(grammar.templates[0].slots.len(), 1);
    assert_eq!(grammar.templates[0].slots[0].rules.len(), 1);
    let _ = f.template;
}

// --- 2. environment-string tokenization -------------------------------------------------------

#[test]
fn tokenize_splits_hash_bracket_and_optional_tokens() {
    let toks = environment::tokenize("#[Vowel](abc)").unwrap();
    assert_eq!(toks, vec!["#", "[Vowel]", "(abc)"]);
}

#[test]
fn tokenize_splits_plain_text_and_respects_spaces() {
    let toks = environment::tokenize("k a [C]").unwrap();
    assert_eq!(toks, vec!["k", "a", "[C]"]);
}

#[test]
fn tokenize_rejects_unclosed_bracket() {
    assert!(environment::tokenize("[Vowel").is_err());
}

#[test]
fn tokenize_rejects_unclosed_paren() {
    assert!(environment::tokenize("(abc").is_err());
}

/// An allomorph's environment guid pointing at a string that doesn't even start with `/` must not fail the whole compile -- it is a warning, and the allomorph still compiles with that one environment simply absent.
#[test]
fn invalid_environment_string_is_a_warning_not_an_error() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-bad".to_string(),
            name: String::new(),
            representation: "not-a-valid-environment".to_string(),
        });
    snapshot.lexicon.entries[0].allomorphs[0]
        .environments
        .push("env-bad".to_string());

    let (grammar, warnings) = compile_project(&snapshot).expect("must still compile");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("env-bad") || w.contains("must start with")),
        "expected a warning about the invalid environment; got {warnings:?}"
    );
    assert_eq!(
        grammar.entries.len(),
        1,
        "the stem entry must still compile"
    );
}

/// A well-formed environment (`[NC]` natural-class reference) parses into a real pattern and gates the allomorph, without any warning.
#[test]
fn valid_bracket_environment_compiles_without_warnings() {
    let (mut snapshot, f) = fixture();
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-vowel".to_string(),
            name: "V".to_string(),
            phonemes: vec!["ph-a".to_string(), "ph-i".to_string(), "ph-u".to_string()],
        });
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-v".to_string(),
            name: String::new(),
            representation: "/_[V]".to_string(),
        });
    snapshot.lexicon.entries[1].allomorphs[0]
        .environments
        .push("env-v".to_string());

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let affix_rules: Vec<_> = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(affix_rules.len(), 1);
    assert_eq!(affix_rules[0].allomorphs[0].environments.len(), 1);
    let _ = f;
}

// --- 3. inflection-class defaulting -----------------------------------------------------------

#[test]
fn stem_msa_without_its_own_inflection_class_defaults_up_the_pos_chain() {
    let (mut snapshot, f) = fixture();
    let class_guid = "class-default".to_string();
    snapshot.morphology.parts_of_speech[0]
        .inflection_classes
        .push(InflectionClass {
            guid: class_guid.clone(),
            name: "DefaultClass".to_string(),
            abbreviation: "def".to_string(),
            children: Vec::new(),
        });
    let same_name_class_guid = "class-same-display-name".to_string();
    snapshot.morphology.parts_of_speech[0]
        .inflection_classes
        .push(InflectionClass {
            guid: same_name_class_guid.clone(),
            name: "DefaultClass".to_string(),
            abbreviation: "other".to_string(),
            children: Vec::new(),
        });
    snapshot.morphology.parts_of_speech[0].default_inflection_class = Some(class_guid.clone());
    // The stem MSA declares no inflection class of its own -- GetDefaultInflClass must supply it.
    match &mut snapshot.lexicon.entries[0].msas[0] {
        Msa::Stem {
            inflection_class, ..
        } => *inflection_class = None,
        _ => panic!("expected the fixture's stem MSA"),
    }

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let class_bit = grammar
        .mpr_names
        .iter()
        .position(|n| n == "DefaultClass")
        .expect("the default inflection class must be registered in the MPR table");
    assert_eq!(grammar.mpr_names.len(), grammar.mpr_features.len());
    for (id, feature) in grammar.mpr_features.iter().enumerate() {
        assert_eq!(grammar.mpr_names[id], feature.name);
    }
    let first = grammar
        .mpr_feature(crate::model::MprId(class_bit as u8))
        .expect("default class bit must resolve");
    assert_eq!(first.xml_id, class_guid);
    let second = grammar
        .mpr_features
        .iter()
        .position(|feature| feature.xml_id == same_name_class_guid)
        .expect("same-named authored class must have a distinct row");
    assert_ne!(class_bit, second);
    assert_eq!(grammar.mpr_names[class_bit], grammar.mpr_names[second]);
    assert!(
        grammar.entries[0]
            .mpr
            .contains(crate::model::MprId(class_bit as u8)),
        "the stem entry's MPR set must carry the POS's defaulted inflection class"
    );
    let _ = f;
}

// --- 4. variant entry with gloss append -------------------------------------------------------

#[test]
fn variant_entry_appends_infl_type_gloss_to_the_base_sense_gloss() {
    let (mut snapshot, f) = fixture();
    let infl_type_guid = "infl-plural".to_string();
    snapshot
        .morphology
        .lex_entry_infl_types
        .push(LexEntryInflType {
            guid: infl_type_guid.clone(),
            name: "Irregular Plural".to_string(),
            abbreviation: "irr.pl".to_string(),
            gloss_prepend: String::new(),
            gloss_append: ".IRR".to_string(),
            slots: Vec::new(),
            inflection_features: None,
        });

    let variant_entry = LexEntry {
        guid: "entry-variant".to_string(),
        citation_form: vec![ws("sen", "kumi")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-variant", MorphType::Stem, "kumi")],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![EntryRef::Variant {
            guid: "entryref-variant".to_string(),
            component_lexemes: vec![f.stem_entry.clone()],
            variant_entry_types: vec![infl_type_guid],
        }],
    };
    snapshot.lexicon.entries.push(variant_entry);

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        grammar.entries.len(),
        2,
        "the base stem entry plus the variant"
    );
    let variant_morpheme_id = grammar
        .morphemes
        .iter()
        .position(|m| m.gloss.as_deref() == Some("dog.IRR"))
        .expect("expected a morpheme with the prepend/append-combined gloss \"dog.IRR\"");
    let variant = grammar
        .entries
        .iter()
        .find(|entry| entry.morpheme.0 as usize == variant_morpheme_id)
        .expect("variant morpheme must belong to a lexical entry");
    assert_eq!(variant.authored_id, "entry-variant");
}

// --- 5. partial entry (MSA without POS) -------------------------------------------------------

#[test]
fn stem_msa_without_a_part_of_speech_is_partial() {
    let (mut snapshot, _f) = fixture();
    match &mut snapshot.lexicon.entries[0].msas[0] {
        Msa::Stem { part_of_speech, .. } => *part_of_speech = None,
        _ => panic!("expected the fixture's stem MSA"),
    }

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(grammar.entries.len(), 1);
    assert!(
        grammar.entries[0].partial,
        "an MSA with no POS must be IsPartial"
    );
}

#[test]
fn inflectional_msa_with_no_slots_is_a_partial_rule() {
    let (mut snapshot, _f) = fixture();
    match &mut snapshot.lexicon.entries[1].msas[0] {
        Msa::Inflectional { slots, .. } => slots.clear(),
        _ => panic!("expected the fixture's inflectional MSA"),
    }

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let affix_rules: Vec<_> = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(affix_rules.len(), 1);
    assert!(
        affix_rules[0].partial,
        "an MoInflAffMsa with zero slots must be IsPartial"
    );
    // With no slots referencing it, the template's one slot has no loaded affix and the whole template must be dropped.
    assert!(grammar.templates.is_empty());
}

// --- 6. parser-parameter handling ---------------------------------------------------------------

#[test]
fn no_default_compounding_suppresses_the_synthesized_default_rules() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.no_default_compounding = true;
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let compound_count = grammar
        .mrules
        .iter()
        .filter(|r| matches!(r, MorphRuleDef::Compounding(_)))
        .count();
    assert_eq!(compound_count, 0);
}

#[test]
fn absent_compound_rules_synthesize_the_two_defaults_when_not_suppressed() {
    let (snapshot, _f) = fixture();
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let compound_count = grammar
        .mrules
        .iter()
        .filter(|r| matches!(r, MorphRuleDef::Compounding(_)))
        .count();
    assert_eq!(
        compound_count, 2,
        "DefaultCompoundingRules synthesizes exactly two rules"
    );
}

#[test]
fn custom_strata_parser_parameter_warns_and_falls_back_to_the_default_layout() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.strata = Some("Morphology,(Clitics)".to_string());
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(
        warnings.iter().any(|w| w.contains("Strata")),
        "expected a warning about unsupported custom Strata reorganization; got {warnings:?}"
    );
    assert_eq!(
        grammar.strata.len(),
        3,
        "default Morphology/Clitics/Surface layout still used"
    );
}

#[test]
fn not_on_clitics_false_places_rewrite_rules_on_the_clitic_stratum() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.not_on_clitics = false;
    snapshot.phonology.rules.push(PhonologicalRule::Rewrite(
        pg_snapshot::phonology::RewriteRule {
            guid: "prule-1".to_string(),
            name: "raise-a".to_string(),
            direction: RuleDirection::LeftToRight,
            structural_description: Vec::new(),
            feature_constraint_variables: Vec::new(),
            right_hand_sides: vec![pg_snapshot::phonology::RewriteRhs::default()],
        },
    ));

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(grammar.prules.len(), 1);
    assert!(
        grammar.strata[0].prules.is_empty(),
        "Morphology stratum must not carry the rule"
    );
    assert_eq!(
        grammar.strata[1].prules.len(),
        1,
        "Clitics stratum must carry the rule"
    );
}

// --- 7. unsupported Phase-B construct: a warning, not an error ---------------------------------

#[test]
fn metathesis_rule_is_unsupported_and_warns_rather_than_erroring() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .phonology
        .rules
        .push(PhonologicalRule::Metathesis(MetathesisRule {
            guid: "meta-1".to_string(),
            name: "swap".to_string(),
            direction: RuleDirection::LeftToRight,
            structural_description: Vec::new(),
            left_switch_index: 0,
            right_switch_index: 1,
        }));

    let (grammar, warnings) =
        compile_project(&snapshot).expect("metathesis must not be a hard error");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("unsupported") && w.contains("metathesis")),
        "expected an 'unsupported: metathesis ...' warning; got {warnings:?}"
    );
    assert!(
        grammar.prules.is_empty(),
        "the metathesis rule itself must not appear in the grammar"
    );
}

// --- 7b. circumfix cross-product (HCLoader.cs:1048-1332) --------------------------------------

/// Flips the circumfix-drop warning into a positive lowering pin: with no environment on either half, the LHS is a single flat `AnyPlus()`.
#[test]
fn circumfix_entry_lowers_to_a_cross_product_allomorph_and_registers_its_slot() {
    let (mut snapshot, f) = fixture();
    let circumfix_entry = LexEntry {
        guid: "entry-circumfix".to_string(),
        citation_form: vec![ws("sen", "ka-...-ta")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![
            simple_allomorph("allo-circ-prefix", MorphType::Prefix, "ka"),
            simple_allomorph("allo-circ-suffix", MorphType::Suffix, "ta"),
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: vec![f.slot.clone()],
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);

    let (grammar, warnings) = compile_project(&snapshot).expect("circumfix must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    let affix_rules: Vec<_> = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .collect();
    // The fixture's ordinary suffix rule plus the new circumfix rule.
    assert_eq!(affix_rules.len(), 2);
    let circumfix_rule = affix_rules
        .iter()
        .find(|r| r.allomorphs.first().is_some_and(|a| a.rhs.len() == 3))
        .expect("expected the circumfix rule (3-action RHS) among the affix rules");
    assert_eq!(
        circumfix_rule.allomorphs.len(),
        1,
        "1 prefix x 1 suffix x 1 (blank) prefix-env x 1 (blank) suffix-env = 1 allomorph"
    );
    let allo = &circumfix_rule.allomorphs[0];
    assert!(
        allo.environments.is_empty(),
        "no environment authored on either half -> no EnvironmentDef"
    );
    assert_eq!(allo.lhs.len(), 1, "one flat LHS pattern for a circumfix");
    assert_eq!(
        allo.lhs[0].nodes.len(),
        3,
        "AnyPlus() is PrefixNull + one-or-more-Any + SuffixNull"
    );
    assert!(
        allo.lhs[0]
            .nodes
            .iter()
            .all(|n| matches!(n, crate::model::PatternNode::Quantifier { .. })),
        "all three AnyPlus nodes are quantifiers"
    );
    match &allo.rhs[0] {
        OutputAction::InsertSegments { shape, .. } => assert_eq!(shape.text, "ka+"),
        other => panic!("expected InsertSegments; got {other:?}"),
    }
    assert_eq!(allo.rhs[1], OutputAction::Copy(PartRef::Input(0)));
    match &allo.rhs[2] {
        OutputAction::InsertSegments { shape, .. } => assert_eq!(shape.text, "+ta"),
        other => panic!("expected InsertSegments; got {other:?}"),
    }

    // The user-visible point of the fix: the owning template's slot gains the circumfix rule too.
    let circumfix_mrule_id = grammar
        .mrules
        .iter()
        .position(|r| {
            matches!(r, MorphRuleDef::AffixProcess(d) if d.allomorphs.first().is_some_and(|a| a.rhs.len() == 3))
        })
        .map(|i| crate::model::MRuleId(i as u32))
        .expect("circumfix rule must be registered in grammar.mrules");
    assert_eq!(grammar.templates[0].slots[0].rules.len(), 2);
    assert!(grammar.templates[0].slots[0]
        .rules
        .contains(&circumfix_mrule_id));
}

/// MPR asymmetry (HCLoader.cs:1055-1057): a circumfix allomorph's required inflection-class MPR bit comes from the PREFIX half only.
#[test]
fn circumfix_required_mpr_comes_from_the_prefix_half_only() {
    let (mut snapshot, f) = fixture();
    let prefix_class = "class-prefix-only".to_string();
    let suffix_class = "class-suffix-only".to_string();
    snapshot.morphology.parts_of_speech[0]
        .inflection_classes
        .push(InflectionClass {
            guid: prefix_class.clone(),
            name: "PrefixClass".to_string(),
            abbreviation: "pfx".to_string(),
            children: Vec::new(),
        });
    snapshot.morphology.parts_of_speech[0]
        .inflection_classes
        .push(InflectionClass {
            guid: suffix_class.clone(),
            name: "SuffixClass".to_string(),
            abbreviation: "sfx".to_string(),
            children: Vec::new(),
        });
    let mut prefix_allo = simple_allomorph("allo-mpr-prefix", MorphType::Prefix, "ka");
    prefix_allo.inflection_classes.push(prefix_class.clone());
    let mut suffix_allo = simple_allomorph("allo-mpr-suffix", MorphType::Suffix, "ta");
    suffix_allo.inflection_classes.push(suffix_class.clone());
    let circumfix_entry = LexEntry {
        guid: "entry-circumfix-mpr".to_string(),
        citation_form: vec![ws("sen", "ka-...-ta")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![prefix_allo, suffix_allo],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix-mpr".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let rule = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .find(|d| d.allomorphs.first().is_some_and(|a| a.rhs.len() == 3))
        .expect("expected the circumfix rule");
    let prefix_bit = grammar
        .mpr_features
        .iter()
        .position(|feat| feat.xml_id == prefix_class)
        .expect("prefix inflection class must be registered");
    let suffix_bit = grammar
        .mpr_features
        .iter()
        .position(|feat| feat.xml_id == suffix_class)
        .expect("suffix inflection class must be registered");
    let allo = &rule.allomorphs[0];
    assert!(
        allo.required_mpr
            .contains(crate::model::MprId(prefix_bit as u8)),
        "the prefix half's inflection class must gate the allomorph"
    );
    assert!(
        !allo
            .required_mpr
            .contains(crate::model::MprId(suffix_bit as u8)),
        "the suffix half's inflection class must be ignored (the HCLoader.cs:1055-1057 asymmetry)"
    );
}

/// A prefix-only environment inlines its right-context text after `PrefixNull()`; a non-empty left-context text becomes the one external `EnvironmentDef`.
#[test]
fn circumfix_prefix_only_environment_inlines_right_context_and_externalizes_left() {
    let (mut snapshot, f) = fixture();
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-cons".to_string(),
            name: "C".to_string(),
            phonemes: vec![
                "ph-k".to_string(),
                "ph-t".to_string(),
                "ph-m".to_string(),
                "ph-s".to_string(),
            ],
        });
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-vowel".to_string(),
            name: "V".to_string(),
            phonemes: vec!["ph-a".to_string(), "ph-i".to_string(), "ph-u".to_string()],
        });
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-prefix-b".to_string(),
            name: String::new(),
            representation: "/[C]_[V]".to_string(),
        });
    let mut prefix_allo = simple_allomorph("allo-b-prefix", MorphType::Prefix, "ka");
    prefix_allo.environments.push("env-prefix-b".to_string());
    let circumfix_entry = LexEntry {
        guid: "entry-circumfix-b".to_string(),
        citation_form: vec![ws("sen", "ka-...-ta")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![
            prefix_allo,
            simple_allomorph("allo-b-suffix", MorphType::Suffix, "ta"),
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix-b".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let rule = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .find(|d| d.allomorphs.first().is_some_and(|a| a.rhs.len() == 3))
        .expect("expected the circumfix rule");
    assert_eq!(rule.allomorphs.len(), 1);
    let allo = &rule.allomorphs[0];
    assert_eq!(
        allo.environments.len(),
        1,
        "the prefix's non-empty left context is the one external EnvironmentDef"
    );
    assert!(allo.environments[0].left.is_some());
    assert!(allo.environments[0].right.is_none());
}

/// Mirror of the prefix-only case for the suffix half: left-context text inlines before `SuffixNull()`; a non-empty right-context text externalizes.
#[test]
fn circumfix_suffix_only_environment_inlines_left_context_and_externalizes_right() {
    let (mut snapshot, f) = fixture();
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-cons".to_string(),
            name: "C".to_string(),
            phonemes: vec![
                "ph-k".to_string(),
                "ph-t".to_string(),
                "ph-m".to_string(),
                "ph-s".to_string(),
            ],
        });
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-vowel".to_string(),
            name: "V".to_string(),
            phonemes: vec!["ph-a".to_string(), "ph-i".to_string(), "ph-u".to_string()],
        });
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-suffix-c".to_string(),
            name: String::new(),
            representation: "/[V]_[C]".to_string(),
        });
    let mut suffix_allo = simple_allomorph("allo-c-suffix", MorphType::Suffix, "ta");
    suffix_allo.environments.push("env-suffix-c".to_string());
    let circumfix_entry = LexEntry {
        guid: "entry-circumfix-c".to_string(),
        citation_form: vec![ws("sen", "ka-...-ta")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![
            simple_allomorph("allo-c-prefix", MorphType::Prefix, "ka"),
            suffix_allo,
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix-c".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let rule = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .find(|d| d.allomorphs.first().is_some_and(|a| a.rhs.len() == 3))
        .expect("expected the circumfix rule");
    assert_eq!(rule.allomorphs.len(), 1);
    let allo = &rule.allomorphs[0];
    assert_eq!(
        allo.environments.len(),
        1,
        "the suffix's non-empty right context is the one external EnvironmentDef"
    );
    assert!(allo.environments[0].left.is_none());
    assert!(allo.environments[0].right.is_some());
}

/// When both halves carry an environment with an external (left/right) context, they merge into exactly ONE `EnvironmentDef`, never two.
#[test]
fn circumfix_both_side_environments_merge_into_one_environment_def() {
    let (mut snapshot, f) = fixture();
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-cons".to_string(),
            name: "C".to_string(),
            phonemes: vec![
                "ph-k".to_string(),
                "ph-t".to_string(),
                "ph-m".to_string(),
                "ph-s".to_string(),
            ],
        });
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-vowel".to_string(),
            name: "V".to_string(),
            phonemes: vec!["ph-a".to_string(), "ph-i".to_string(), "ph-u".to_string()],
        });
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-prefix-d".to_string(),
            name: String::new(),
            representation: "/[C]_[V]".to_string(),
        });
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-suffix-d".to_string(),
            name: String::new(),
            representation: "/[V]_[C]".to_string(),
        });
    let mut prefix_allo = simple_allomorph("allo-d-prefix", MorphType::Prefix, "ka");
    prefix_allo.environments.push("env-prefix-d".to_string());
    let mut suffix_allo = simple_allomorph("allo-d-suffix", MorphType::Suffix, "ta");
    suffix_allo.environments.push("env-suffix-d".to_string());
    let circumfix_entry = LexEntry {
        guid: "entry-circumfix-d".to_string(),
        citation_form: vec![ws("sen", "ka-...-ta")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![prefix_allo, suffix_allo],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix-d".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let rule = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .find(|d| d.allomorphs.first().is_some_and(|a| a.rhs.len() == 3))
        .expect("expected the circumfix rule");
    assert_eq!(rule.allomorphs.len(), 1);
    let allo = &rule.allomorphs[0];
    assert_eq!(
        allo.environments.len(),
        1,
        "prefix-left and suffix-right external contexts merge into ONE EnvironmentDef"
    );
    assert!(allo.environments[0].left.is_some());
    assert!(allo.environments[0].right.is_some());
}

/// 2 prefix alternates x 2 suffix alternates build 4 allomorphs, in HCLoader's exact nesting order (prefix outer, suffix inner) -- load-bearing for disjunctive-ordering semantics.
#[test]
fn circumfix_two_by_two_cross_product_builds_four_allomorphs_in_hcloader_nesting_order() {
    let (mut snapshot, f) = fixture();
    let circumfix_entry = LexEntry {
        guid: "entry-circumfix-e".to_string(),
        citation_form: vec![ws("sen", "ka/ku-...-ta/tu")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![
            simple_allomorph("allo-e-prefix0", MorphType::Prefix, "ka"),
            simple_allomorph("allo-e-prefix1", MorphType::Prefix, "ku"),
            simple_allomorph("allo-e-suffix0", MorphType::Suffix, "ta"),
            simple_allomorph("allo-e-suffix1", MorphType::Suffix, "tu"),
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix-e".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let rule = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .find(|d| d.allomorphs.len() == 4)
        .expect("expected the 4-allomorph circumfix rule");
    let inserted: Vec<(String, String)> = rule
        .allomorphs
        .iter()
        .map(|a| {
            let pfx = match &a.rhs[0] {
                OutputAction::InsertSegments { shape, .. } => shape.text.clone(),
                other => panic!("expected InsertSegments; got {other:?}"),
            };
            let sfx = match &a.rhs[2] {
                OutputAction::InsertSegments { shape, .. } => shape.text.clone(),
                other => panic!("expected InsertSegments; got {other:?}"),
            };
            (pfx, sfx)
        })
        .collect();
    assert_eq!(
        inserted,
        vec![
            ("ka+".to_string(), "+ta".to_string()),
            ("ka+".to_string(), "+tu".to_string()),
            ("ku+".to_string(), "+ta".to_string()),
            ("ku+".to_string(), "+tu".to_string()),
        ],
        "prefix outer, suffix inner nesting order"
    );
}

/// An entry declared `Circumfix` but missing an entire half warns and drops the whole entry, mirroring HCLoader's zero-yielded-allomorphs outcome for the same malformed data.
#[test]
fn circumfix_missing_suffix_half_warns_and_drops_the_entry() {
    let (mut snapshot, f) = fixture();
    let circumfix_entry = LexEntry {
        guid: "entry-circumfix-f".to_string(),
        citation_form: vec![ws("sen", "ka-...-ku")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![
            simple_allomorph("allo-f-prefix0", MorphType::Prefix, "ka"),
            simple_allomorph("allo-f-prefix1", MorphType::Prefix, "ku"),
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix-f".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);

    let (grammar, warnings) =
        compile_project(&snapshot).expect("a malformed circumfix must not be a hard error");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("circumfix") && w.contains("empty")),
        "expected an empty-half warning; got {warnings:?}"
    );
    let circumfix_rule_count = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .filter(|d| d.allomorphs.first().is_some_and(|a| a.rhs.len() == 3))
        .count();
    assert_eq!(circumfix_rule_count, 0, "no rule built with an empty half");
}

/// U+25CC stripping is `pg-fwdata`'s job (`node.rs:187-189`), done before the snapshot exists; the compiled RHS text must carry no residual dotted circle.
#[test]
fn circumfix_forms_already_stripped_of_dotted_circles_by_fwdata_round_trip_cleanly() {
    let (mut snapshot, f) = fixture();
    let circumfix_entry = LexEntry {
        guid: "entry-circumfix-g".to_string(),
        citation_form: vec![ws("sen", "ka-...-ta")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![
            simple_allomorph("allo-g-prefix", MorphType::Prefix, "ka"),
            simple_allomorph("allo-g-suffix", MorphType::Suffix, "ta"),
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix-g".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let rule = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .find(|d| d.allomorphs.first().is_some_and(|a| a.rhs.len() == 3))
        .expect("expected the circumfix rule");
    match &rule.allomorphs[0].rhs[0] {
        OutputAction::InsertSegments { shape, .. } => {
            assert!(!shape.text.contains('\u{25CC}'));
            assert_eq!(shape.text, "ka+");
        }
        other => panic!("expected InsertSegments; got {other:?}"),
    }
}

/// If the fwdata-side strip were ever removed, a raw U+25CC reaching this compiler must fail loudly (a segmentation warning), never silently compile as if absent.
#[test]
fn circumfix_form_with_an_unstripped_dotted_circle_is_a_loud_warning_not_a_silent_pass() {
    let (mut snapshot, f) = fixture();
    let circumfix_entry = LexEntry {
        guid: "entry-circumfix-g2".to_string(),
        citation_form: vec![ws("sen", "ka-...-ta")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![
            simple_allomorph("allo-g2-prefix", MorphType::Prefix, "\u{25CC}ka"),
            simple_allomorph("allo-g2-suffix", MorphType::Suffix, "ta"),
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix-g2".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);
    let (grammar, warnings) =
        compile_project(&snapshot).expect("an unresolvable phoneme must not be a hard error");
    assert!(
        warnings.iter().any(|w| w.contains("cannot segment")),
        "expected a segmentation warning for the un-stripped dotted circle; got {warnings:?}"
    );
    let circumfix_rule_count = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .filter(|d| d.allomorphs.first().is_some_and(|a| a.rhs.len() == 3))
        .count();
    assert_eq!(circumfix_rule_count, 0);
}

/// The same semantic circumfix built through `compile_project` and through the legacy HC-XML `crate::load` must agree on RHS action sequence and environment count; LHS node shapes are not compared since `compile_project`'s no-environment case synthesizes `PrefixNull()`/`AnyStar()`/`SuffixNull()` wildcards the DTD has no equivalent for.
#[test]
fn circumfix_cross_product_matches_the_xml_loaders_generic_affix_process_rhs() {
    let (mut snapshot, f) = fixture();
    let circumfix_entry = LexEntry {
        guid: "entry-circumfix-parity".to_string(),
        citation_form: vec![ws("sen", "ka-...-ta")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![
            simple_allomorph("allo-parity-prefix", MorphType::Prefix, "ka"),
            simple_allomorph("allo-parity-suffix", MorphType::Suffix, "ta"),
        ],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix-parity".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let compiled = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .find(|d| d.allomorphs.first().is_some_and(|a| a.rhs.len() == 3))
        .expect("expected the circumfix rule");
    assert_eq!(compiled.allomorphs.len(), 1);

    const XML: &str = r#"<HermitCrabInput><Language><Name>Parity</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
        <BoundaryDefinitions>
          <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
        </BoundaryDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses>
        <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="cK" /><Segment segment="cA" /><Segment segment="cT" /></SegmentNaturalClass>
      </NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrCirc">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mrCirc" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
              <Name>circ</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="subCirc">
                  <MorphologicalInput>
                    <PhoneticSequence id="stem">
                      <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                    </PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput>
                    <InsertSegments><PhoneticShape>ka+</PhoneticShape></InsertSegments>
                    <CopyFromInput index="stem" />
                    <InsertSegments><PhoneticShape>+ta</PhoneticShape></InsertSegments>
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;
    let xml_grammar = crate::load::load(XML).expect("hand-authored XML must load");
    let xml_rule = xml_grammar
        .mrules
        .iter()
        .find_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .expect("expected the XML-loaded circumfix rule");
    assert_eq!(xml_rule.allomorphs.len(), 1);

    let compiled_rhs = &compiled.allomorphs[0].rhs;
    let xml_rhs = &xml_rule.allomorphs[0].rhs;
    assert_eq!(compiled_rhs.len(), xml_rhs.len());
    for (c, x) in compiled_rhs.iter().zip(xml_rhs.iter()) {
        match (c, x) {
            (
                OutputAction::InsertSegments { shape: cs, .. },
                OutputAction::InsertSegments { shape: xs, .. },
            ) => assert_eq!(cs.text, xs.text),
            (OutputAction::Copy(cp), OutputAction::Copy(xp)) => assert_eq!(cp, xp),
            other => panic!("action-kind mismatch: {other:?}"),
        }
    }
    assert_eq!(
        compiled.allomorphs[0].environments.len(),
        xml_rule.allomorphs[0].environments.len()
    );
}

// --- feature-structure / basic feature-system sanity --------------------------------------------

#[test]
fn morphosyntactic_closed_feature_compiles_into_the_syntactic_feature_system() {
    let (mut snapshot, _f) = fixture();
    let number_guid = "feat-number".to_string();
    let sg_guid = "val-sg".to_string();
    let pl_guid = "val-pl".to_string();
    snapshot.feature_systems.morphosyntactic = FeatureSystem {
        closed_features: vec![ClosedFeature {
            guid: number_guid.clone(),
            name: "Number".to_string(),
            abbreviation: "num".to_string(),
            values: vec![
                FeatureValueSymbol {
                    guid: sg_guid.clone(),
                    name: "singular".to_string(),
                    abbreviation: "sg".to_string(),
                },
                FeatureValueSymbol {
                    guid: pl_guid.clone(),
                    name: "plural".to_string(),
                    abbreviation: "pl".to_string(),
                },
            ],
        }],
        complex_features: Vec::new(),
    };
    match &mut snapshot.lexicon.entries[0].msas[0] {
        Msa::Stem { features, .. } => {
            *features = Some(FeatureStructure {
                values: vec![FeatureValue {
                    feature: number_guid.clone(),
                    value: FeatureValueKind::Closed {
                        value: sg_guid.clone(),
                    },
                }],
            })
        }
        _ => panic!("expected the fixture's stem MSA"),
    }

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let feature_id = grammar
        .syn_features
        .feature_by_xml_id(&number_guid)
        .expect("the morphosyntactic feature must retain its authored identity");
    match &grammar.syn_features.features[feature_id.0 as usize].kind {
        crate::model::SynFeatureKind::Symbolic { symbols, .. } => {
            assert_eq!(symbols[0].0, sg_guid);
            assert_eq!(symbols[1].0, pl_guid);
        }
        crate::model::SynFeatureKind::Complex => panic!("number must be symbolic"),
    }
}

// --- cross-reference (dense-id) integrity -------------------------------------------------------

/// Walks every dense-id cross-reference in a compiled `Grammar` and panics on the first inconsistency; catches what count/shape assertions alone would miss, such as an off-by-one in `allomorph_owners` or an out-of-range `MRuleId`.
fn assert_grammar_ids_are_internally_consistent(grammar: &crate::model::Grammar) {
    use crate::model::AllomorphOwner;

    // allomorph_owners[i] must round-trip: the owner it names must itself carry `id == i`.
    for (i, owner) in grammar.allomorph_owners.iter().enumerate() {
        let want = crate::model::AllomorphId(i as u32);
        match owner {
            AllomorphOwner::Root(le, k) => {
                let entry = grammar.entries.get(le.0 as usize).unwrap_or_else(|| {
                    panic!("allomorph_owners[{i}] = Root({le:?}, {k}): entry index out of range")
                });
                let allo = entry.allomorphs.get(*k as usize).unwrap_or_else(|| {
                    panic!(
                        "allomorph_owners[{i}] = Root({le:?}, {k}): allomorph index out of range"
                    )
                });
                assert_eq!(
                    allo.id, want,
                    "allomorph_owners[{i}] = Root({le:?}, {k}) does not round-trip to itself (found {:?})",
                    allo.id
                );
            }
            AllomorphOwner::Affix(mr, k) => {
                let rule = grammar.mrules.get(mr.0 as usize).unwrap_or_else(|| {
                    panic!("allomorph_owners[{i}] = Affix({mr:?}, {k}): mrule index out of range")
                });
                let allos = rule.affix_allomorphs().unwrap_or_else(|| {
                    panic!("allomorph_owners[{i}] = Affix({mr:?}, {k}): mrule {mr:?} is not an AffixProcess/Realizational rule")
                });
                let allo = allos.get(*k as usize).unwrap_or_else(|| {
                    panic!(
                        "allomorph_owners[{i}] = Affix({mr:?}, {k}): allomorph index out of range"
                    )
                });
                assert_eq!(
                    allo.id, want,
                    "allomorph_owners[{i}] = Affix({mr:?}, {k}) does not round-trip to itself (found {:?})",
                    allo.id
                );
            }
        }
    }

    // Every entry's morpheme id must resolve.
    for (i, entry) in grammar.entries.iter().enumerate() {
        assert!(
            (entry.morpheme.0 as usize) < grammar.morphemes.len(),
            "entries[{i}].morpheme = {:?} is out of range (morphemes.len() == {})",
            entry.morpheme,
            grammar.morphemes.len()
        );
    }

    // Every AffixProcess/Realizational rule's morpheme id must resolve (Compounding rules have none).
    for (i, rule) in grammar.mrules.iter().enumerate() {
        let morpheme = match rule {
            crate::model::MorphRuleDef::AffixProcess(d) => Some(d.morpheme),
            crate::model::MorphRuleDef::Realizational(d) => Some(d.morpheme),
            crate::model::MorphRuleDef::Compounding(_) => None,
        };
        if let Some(m) = morpheme {
            assert!(
                (m.0 as usize) < grammar.morphemes.len(),
                "mrules[{i}].morpheme = {m:?} is out of range (morphemes.len() == {})",
                grammar.morphemes.len()
            );
        }
    }

    // Every MRuleId referenced by a stratum or a template slot must resolve.
    for (i, stratum) in grammar.strata.iter().enumerate() {
        for r in &stratum.mrules {
            assert!(
                (r.0 as usize) < grammar.mrules.len(),
                "strata[{i}].mrules contains {r:?}, out of range (mrules.len() == {})",
                grammar.mrules.len()
            );
        }
        for e in &stratum.entries {
            assert!(
                (e.0 as usize) < grammar.entries.len(),
                "strata[{i}].entries contains {e:?}, out of range (entries.len() == {})",
                grammar.entries.len()
            );
        }
    }
    for (i, template) in grammar.templates.iter().enumerate() {
        for (j, slot) in template.slots.iter().enumerate() {
            for r in &slot.rules {
                assert!(
                    (r.0 as usize) < grammar.mrules.len(),
                    "templates[{i}].slots[{j}].rules contains {r:?}, out of range (mrules.len() == {})",
                    grammar.mrules.len()
                );
            }
        }
    }
}

#[test]
fn fixture_grammar_dense_ids_are_internally_consistent() {
    let (snapshot, _f) = fixture();
    let (grammar, warnings) = compile_project(&snapshot).expect("fixture must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    // Sanity: the fixture carries the 2 default compounding rules ahead of the 1 affix rule in `mrules`, so this exercises a non-trivial index offset, not just the identity case.
    assert_eq!(grammar.mrules.len(), 3);
    assert_grammar_ids_are_internally_consistent(&grammar);
}

#[test]
fn variant_entry_grammar_dense_ids_are_internally_consistent() {
    // Reuse the variant-entry scenario to also exercise the multi-entry, multi-allomorph-owner case through the same consistency walk.
    let (mut snapshot, f) = fixture();
    let variant_entry = LexEntry {
        guid: "entry-variant".to_string(),
        citation_form: vec![ws("sen", "kumi")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-variant", MorphType::Stem, "kumi")],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![EntryRef::Variant {
            guid: "entryref-variant-consistency".to_string(),
            component_lexemes: vec![f.stem_entry.clone()],
            variant_entry_types: Vec::new(),
        }],
    };
    snapshot.lexicon.entries.push(variant_entry);

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        grammar.entries.len(),
        2,
        "stem entry + variant entry expected"
    );
    assert_grammar_ids_are_internally_consistent(&grammar);
}

// --- clitic morph types -------------------------------------------------------------------------

#[test]
fn enclitic_entry_compiles_to_clitic_stratum_lex_entry_and_affix_rule() {
    // An enclitic allomorph is both a valid clitic lex-entry form and a valid rule form, so the entry appears on the Clitics stratum twice: as a stem-role `LexEntry` and as a clitic affix-process rule.
    let (mut snapshot, f) = fixture();
    let clitic_entry = LexEntry {
        guid: "entry-clitic".to_string(),
        citation_form: vec![ws("sen", "=si")],
        lexeme_morph_type: MorphType::Enclitic,
        allomorphs: vec![simple_allomorph("allo-clitic", MorphType::Enclitic, "si")],
        msas: vec![Msa::Stem {
            guid: "msa-clitic".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            inflection_class: None,
            features: None,
            exception_features: Vec::new(),
            from_parts_of_speech: vec![f.noun_pos.clone()],
            slots: Vec::new(),
        }],
        senses: vec![Sense {
            guid: "sense-clitic".to_string(),
            gloss: vec![ws("en", "TOP")],
            definition: Vec::new(),
            msa: Some("msa-clitic".to_string()),
        }],
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(clitic_entry);

    let (grammar, warnings) =
        compile_project(&snapshot).expect("clitic entries must not be a hard error");
    assert!(
        !warnings.iter().any(|w| w.contains("clitic")),
        "clitics are implemented; no clitic warning expected, got {warnings:?}"
    );
    // The fixture's own stem entry + the clitic entry's stem role.
    assert_eq!(grammar.entries.len(), 2);
    let clitics = &grammar.strata[1];
    assert_eq!(
        clitics.entries.len(),
        1,
        "the enclitic's stem role lands on the Clitics stratum"
    );
    assert_eq!(
        clitics.mrules.len(),
        1,
        "the enclitic's rule role (LoadCliticAffixProcessRule) lands on the Clitics stratum"
    );
    // The morphology stratum keeps only the fixture's own entries/rules.
    assert!(grammar.strata[0].entries.len() == 1);
    // The clitic morpheme records live on the Clitics stratum.
    let clitic_rule_morpheme = match &grammar.mrules[clitics.mrules[0].0 as usize] {
        crate::model::MorphRuleDef::AffixProcess(d) => d.morpheme,
        other => panic!("expected an affix-process rule, got {other:?}"),
    };
    assert_eq!(
        grammar.morphemes[clitic_rule_morpheme.0 as usize].stratum.0,
        1
    );
    assert_eq!(
        grammar.morphemes[clitic_rule_morpheme.0 as usize]
            .gloss
            .as_deref(),
        Some("TOP")
    );
}
