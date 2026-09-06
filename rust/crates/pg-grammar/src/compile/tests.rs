//! Unit tests for `pg_grammar::compile`, built entirely from code-constructed `Snapshot` values (no `.fwdata`/oracle files).

use pg_snapshot::feature::{
    ClosedFeature, ComplexFeature, FeatureStructure, FeatureSystem, FeatureValue, FeatureValueKind,
    FeatureValueSymbol,
};
use pg_snapshot::lexicon::{Allomorph, EntryRef, LexEntry, Lexicon, Msa, Sense};
use pg_snapshot::morphology::{
    AdhocProhibition, Adjacency, AffixSlot, AffixTemplate, CompoundConstituentRequirement,
    CompoundOutcome, CompoundRule, InflectionClass, LexEntryInflType, MorphType, Morphology,
    PartOfSpeech,
};
use pg_snapshot::phonology::{
    BoundaryMarker, MetathesisRule, NaturalClass as SnapNaturalClass, Phoneme, PhonologicalRule,
    Phonology, RuleDirection,
};
use pg_snapshot::project::Project;
use pg_snapshot::{
    ActiveParser, ConversionIssue, FeatureSystems, InventoryKey, InventoryKind, IssueClass,
    SourceInventoryStatus, Snapshot, WsForm,
};

use crate::model::{MorphRuleDef, TemplateSlotZone};
use crate::GrammarError;

use super::test_support::assert_grammars_equal;
use super::{
    compile_project, compile_project_measured, compile_project_recording, compile_project_with,
    environment, CompileOptions, CompileOutput, ResolvedSubstratePolicy, SemanticLossPolicy,
    SubstratePolicy,
};

/// Compiles `snapshot` through the recording seam and asserts the recorder's own invariants hold; returns everything a caller might want to inspect further.
fn compile_recording_ok(
    snapshot: &Snapshot,
) -> (
    crate::model::Grammar,
    Vec<String>,
    pg_snapshot::ConversionInventory,
    Vec<pg_snapshot::ConversionIssue>,
) {
    let (grammar, warnings, recorder) =
        compile_project_recording(snapshot).expect("must compile");
    recorder
        .check_invariants()
        .expect("recorder invariants must hold");
    let (inventory, issues) = recorder.finish();
    (grammar, warnings, inventory, issues)
}

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
            exemplar_characters: Vec::new(),
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
    assert_eq!(
        grammar.templates[0].slots[0].zone,
        TemplateSlotZone::Suffix,
        "snapshot compilation must preserve the physical slot occurrence side"
    );
    let _ = f.template;
}

#[test]
fn template_slot_occurrences_preserve_suffix_then_reversed_prefix_layout() {
    let (mut snapshot, f) = fixture();
    let template = &mut snapshot.morphology.parts_of_speech[0].affix_templates[0];
    template.suffix_slots = vec![f.slot.clone()];
    template.prefix_slots = vec![f.slot.clone()];

    let (grammar, warnings) = compile_project(&snapshot).expect("fixture must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let zones: Vec<_> = grammar.templates[0]
        .slots
        .iter()
        .map(|slot| slot.zone)
        .collect();
    assert_eq!(
        zones,
        vec![TemplateSlotZone::Suffix, TemplateSlotZone::Prefix],
        "the side belongs to each physical template occurrence, even for one shared slot"
    );
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

/// `chardef::build`'s morph-boundary fallback (no authored `+` representation) must be recorded rejected, with the legacy warning text unchanged. Default compounding is suppressed: it unconditionally segments a literal `"+"` (`compounding::plus_join`), an unrelated pre-existing assumption this test must not trip.
#[test]
fn missing_morph_boundary_marker_is_recorded_rejected_with_the_legacy_warning_text() {
    let (mut snapshot, _f) = fixture();
    snapshot.phonology.boundary_markers.retain(|b| b.guid != "bd-plus");
    snapshot.morphology.parser_parameters.no_default_compounding = true;

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("no boundary marker representation '+' found")),
        "expected the legacy morph-boundary fallback warning to survive unchanged; got {warnings:?}"
    );
    let key = InventoryKey::setting(InventoryKind::BoundaryMarker, "morph-boundary".to_string());
    assert!(inventory.rejected.contains(&key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::BOUNDARY_MORPH_MARKER_UNRESOLVED));
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

/// A circumfix entry with `prefix_env`/`suffix_env` attached to its two halves (empty = unconditioned).
fn circumfix_snapshot(
    prefix_env: &[&str],
    suffix_env: &[&str],
) -> (pg_snapshot::Snapshot, Fixture) {
    let (mut snapshot, f) = fixture();
    // The conditioned cases below reference `[V]`; without the class, `parse_environment` fails and the environment is dropped as absent, which would make those tests unmeetable rather than meaningful.
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-vowel".to_string(),
            name: "V".to_string(),
            phonemes: vec!["ph-a".to_string(), "ph-i".to_string(), "ph-u".to_string()],
        });
    let mut prefix = simple_allomorph("allo-circ-prefix", MorphType::Prefix, "ka");
    let mut suffix = simple_allomorph("allo-circ-suffix", MorphType::Suffix, "ta");
    prefix.environments = prefix_env.iter().map(|s| s.to_string()).collect();
    suffix.environments = suffix_env.iter().map(|s| s.to_string()).collect();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-circumfix".to_string(),
        citation_form: vec![ws("sen", "ka-...-ta")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![prefix, suffix],
        msas: vec![Msa::Inflectional {
            guid: "msa-circumfix".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });
    (snapshot, f)
}

/// The `mrCross` shape: one allomorph per prefix-half x suffix-half pairing, inserting on both sides of one copy.
/// See docs/research/circumfix-cross-product-loading.md.
#[test]
fn an_unconditioned_circumfix_entry_builds_the_half_cross_product() {
    let (snapshot, _f) = circumfix_snapshot(&[], &[]);
    let (grammar, warnings) = compile_project(&snapshot).expect("circumfix must not be a hard error");
    assert!(
        !warnings.iter().any(|w| w.contains("circumfix")),
        "an unconditioned circumfix is representable, so nothing about it should be warned: {warnings:?}"
    );
    let built: Vec<&crate::model::AffixProcessRuleDef> = grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            crate::model::MorphRuleDef::AffixProcess(a) => Some(a),
            _ => None,
        })
        .filter(|a| {
            a.allomorphs.iter().any(|allo| {
                matches!(
                    allo.rhs.as_slice(),
                    [
                        crate::model::OutputAction::InsertSegments { .. },
                        crate::model::OutputAction::Copy(_),
                        crate::model::OutputAction::InsertSegments { .. }
                    ]
                )
            })
        })
        .collect();
    assert_eq!(
        built.len(),
        1,
        "expected exactly one rule whose allomorph inserts on both sides of a copy"
    );
    assert_eq!(
        built[0].allomorphs.len(),
        1,
        "one prefix half x one suffix half is a 1x1 cross-product"
    );
}

/// Every allomorph across `grammar.mrules` shaped like a circumfix cross-product cell (leading+trailing insert around one copy).
fn circumfix_rule_allomorphs(
    grammar: &crate::model::Grammar,
) -> Vec<&crate::model::AffixAllomorphDef> {
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
                    crate::model::OutputAction::InsertSegments { .. },
                    crate::model::OutputAction::Copy(_),
                    crate::model::OutputAction::InsertSegments { .. }
                ]
            )
        })
        .collect()
}

/// Per-side conditioning is representable now, not refused; see docs/research/circumfix-cross-product-loading.md.
#[test]
fn a_circumfix_half_carrying_an_environment_builds_with_it_unioned_in() {
    let (mut snapshot, _f) = circumfix_snapshot(&["env-after-vowel"], &[]);
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-after-vowel".to_string(),
            name: "after vowel".to_string(),
            representation: "/[V]_".to_string(),
        });

    let (grammar, warnings) = compile_project(&snapshot).expect("must not be a hard error");
    assert!(
        !warnings
            .iter()
            .any(|w| w.contains("circumfix") && w.contains("environment")),
        "a conditioned circumfix half is representable, so nothing about it should be refused: {warnings:?}"
    );
    let built = circumfix_rule_allomorphs(&grammar);
    assert_eq!(
        built.len(),
        1,
        "one prefix half x one suffix half is still a 1x1 cross-product"
    );
    assert_eq!(
        built[0].environments.len(),
        1,
        "the prefix half's one environment must survive onto the combined allomorph: {:?}",
        built[0].environments
    );
}

/// Both halves conditioned: the combined allomorph must carry the UNION of both, not just one side.
#[test]
fn a_circumfix_with_environments_on_both_halves_unions_them() {
    let (mut snapshot, _f) = circumfix_snapshot(&["env-after-vowel"], &["env-before-vowel"]);
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-after-vowel".to_string(),
            name: "after vowel".to_string(),
            representation: "/[V]_".to_string(),
        });
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-before-vowel".to_string(),
            name: "before vowel".to_string(),
            representation: "/_[V]".to_string(),
        });

    let (grammar, warnings) = compile_project(&snapshot).expect("must not be a hard error");
    assert!(!warnings
        .iter()
        .any(|w| w.contains("circumfix") && w.contains("environment")));
    let built = circumfix_rule_allomorphs(&grammar);
    assert_eq!(built.len(), 1);
    assert_eq!(
        built[0].environments.len(),
        2,
        "both halves' environments must both survive, unioned onto one allomorph: {:?}",
        built[0].environments
    );
}

/// `positions` has the same C# analog as `environments` (see `combined_env_guids` below) and unions in the same way.
#[test]
fn a_circumfix_half_carrying_a_position_builds_with_it_unioned_in() {
    let (mut snapshot, f) = circumfix_snapshot(&[], &[]);
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-after-vowel".to_string(),
            name: "after vowel".to_string(),
            representation: "/[V]_".to_string(),
        });
    snapshot
        .lexicon
        .entries
        .iter_mut()
        .find(|e| e.guid == "entry-circumfix")
        .expect("circumfix_snapshot must have pushed entry-circumfix")
        .allomorphs[0]
        .positions
        .push("env-after-vowel".to_string());
    let _ = f;

    let (grammar, warnings) = compile_project(&snapshot).expect("must not be a hard error");
    assert!(!warnings
        .iter()
        .any(|w| w.contains("circumfix") && w.contains("environment")));
    let built = circumfix_rule_allomorphs(&grammar);
    assert_eq!(built.len(), 1);
    assert_eq!(
        built[0].environments.len(),
        1,
        "the prefix half's position must survive onto the combined allomorph like an environment would: {:?}",
        built[0].environments
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

// --- snapshot-to-grammar selection recording ---------------------------------------------------

/// Every authored key must be considered, and every selected key must reach represented or rejected -- the two cross-stage bookkeeping checks `SelectionRecorder::check_invariants` itself does not enforce (it only relates adjacent stages), so callers assert them directly.
fn assert_no_authored_or_selected_falls_through(inventory: &pg_snapshot::ConversionInventory) {
    let unaccounted_authored: Vec<_> = inventory
        .authored
        .difference(&inventory.considered)
        .collect();
    assert!(
        unaccounted_authored.is_empty(),
        "authored but never considered: {unaccounted_authored:?}"
    );

    let unaccounted_selected: Vec<_> = inventory
        .selected
        .iter()
        .filter(|k| !inventory.represented.contains(k) && !inventory.rejected.contains(k))
        .collect();
    assert!(
        unaccounted_selected.is_empty(),
        "selected but neither represented nor rejected: {unaccounted_selected:?}"
    );
}

#[test]
fn fixture_recording_authored_minus_considered_is_empty_and_selected_minus_represented_minus_rejected_is_empty(
) {
    let (snapshot, _f) = fixture();
    let (grammar, warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    assert_no_authored_or_selected_falls_through(&inventory);

    let represented_phonemes = inventory
        .represented
        .iter()
        .filter(|k| k.kind == InventoryKind::Phoneme)
        .count();
    assert_eq!(represented_phonemes, snapshot.phonology.phonemes.len());

    // Counts snapshot entries: one may yield a LexEntryDef, an mrule, both, or neither.
    let represented_entries = inventory
        .represented
        .iter()
        .filter(|k| k.kind == InventoryKind::Entry)
        .count();
    assert_eq!(represented_entries, snapshot.lexicon.entries.len());

    let represented_allomorphs = inventory
        .represented
        .iter()
        .filter(|k| k.kind == InventoryKind::Allomorph)
        .count();
    let grammar_allomorph_count = grammar
        .entries
        .iter()
        .map(|e| e.allomorphs.len())
        .sum::<usize>()
        + grammar
            .mrules
            .iter()
            .filter_map(|r| r.affix_allomorphs())
            .map(|a| a.len())
            .sum::<usize>();
    assert_eq!(represented_allomorphs, grammar_allomorph_count);

    // An `EntryRef::Variant` authors its own `EntryReference` atom (see `inventory::seed_authored_from_snapshot`), so the same check must hold once one is present.
    let (mut variant_snapshot, f) = fixture();
    variant_snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-variant".to_string(),
        citation_form: vec![ws("sen", "kumi")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-variant", MorphType::Stem, "kumi")],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![EntryRef::Variant {
            guid: "entryref-variant-authored-considered".to_string(),
            component_lexemes: vec![f.stem_entry.clone()],
            variant_entry_types: Vec::new(),
        }],
    });
    let (_grammar2, warnings2, inventory2, _issues2) = compile_recording_ok(&variant_snapshot);
    assert!(warnings2.is_empty(), "unexpected warnings: {warnings2:?}");
    assert_no_authored_or_selected_falls_through(&inventory2);
}

/// Only a `Variant` ref's guid is authored as an `EntryReference` atom, never a `ComplexForm`'s.
#[test]
fn complex_form_ref_guid_is_never_authored_as_an_entry_reference() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-complex".to_string(),
        citation_form: vec![ws("sen", "kumita")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-complex", MorphType::Stem, "kumita")],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![EntryRef::ComplexForm {
            guid: "entryref-complex-unauthored".to_string(),
            component_lexemes: vec![f.stem_entry.clone()],
            complex_entry_types: Vec::new(),
        }],
    });

    let (_grammar, warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let complex_form_key = InventoryKey::object(
        InventoryKind::EntryReference,
        "entryref-complex-unauthored".to_string(),
    );
    assert!(
        !inventory.authored.contains(&complex_form_key),
        "a ComplexForm ref's guid must never be authored as an EntryReference atom"
    );
}

/// Every existing fixture-derived scenario elsewhere in this file must also leave the recorder's invariants intact; each snapshot here mirrors an existing test's mutation, checked through `compile_recording_ok` rather than duplicating that test's own assertions.
#[test]
fn every_existing_fixture_variant_leaves_recorder_invariants_intact() {
    let (mut template_variant, f) = fixture();
    let template = &mut template_variant.morphology.parts_of_speech[0].affix_templates[0];
    template.suffix_slots = vec![f.slot.clone()];
    template.prefix_slots = vec![f.slot.clone()];
    compile_recording_ok(&template_variant);

    let (mut infl_class_variant, _f) = fixture();
    infl_class_variant.morphology.parts_of_speech[0]
        .inflection_classes
        .push(InflectionClass {
            guid: "class-default".to_string(),
            name: "DefaultClass".to_string(),
            abbreviation: "def".to_string(),
            children: Vec::new(),
        });
    infl_class_variant.morphology.parts_of_speech[0].default_inflection_class =
        Some("class-default".to_string());
    match &mut infl_class_variant.lexicon.entries[0].msas[0] {
        Msa::Stem {
            inflection_class, ..
        } => *inflection_class = None,
        _ => panic!("expected the fixture's stem MSA"),
    }
    compile_recording_ok(&infl_class_variant);

    let (mut partial_stem, _f) = fixture();
    match &mut partial_stem.lexicon.entries[0].msas[0] {
        Msa::Stem { part_of_speech, .. } => *part_of_speech = None,
        _ => panic!("expected the fixture's stem MSA"),
    }
    compile_recording_ok(&partial_stem);

    let (mut partial_rule, _f) = fixture();
    match &mut partial_rule.lexicon.entries[1].msas[0] {
        Msa::Inflectional { slots, .. } => slots.clear(),
        _ => panic!("expected the fixture's inflectional MSA"),
    }
    compile_recording_ok(&partial_rule);

    let (mut no_default_compounding, _f) = fixture();
    no_default_compounding.morphology.parser_parameters.no_default_compounding = true;
    compile_recording_ok(&no_default_compounding);

    let (mut clitic_rules, _f) = fixture();
    clitic_rules.morphology.parser_parameters.not_on_clitics = false;
    clitic_rules.phonology.rules.push(PhonologicalRule::Rewrite(
        pg_snapshot::phonology::RewriteRule {
            guid: "prule-1".to_string(),
            name: "raise-a".to_string(),
            direction: RuleDirection::LeftToRight,
            structural_description: Vec::new(),
            feature_constraint_variables: Vec::new(),
            right_hand_sides: vec![pg_snapshot::phonology::RewriteRhs::default()],
        },
    ));
    compile_recording_ok(&clitic_rules);

    let (mut metathesis_variant, _f) = fixture();
    metathesis_variant
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
    compile_recording_ok(&metathesis_variant);

    let (bracket_env_variant, _f) = {
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
        (snapshot, f)
    };
    compile_recording_ok(&bracket_env_variant);

    let (invalid_env_variant, _f) = {
        let (mut snapshot, f) = fixture();
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
        (snapshot, f)
    };
    compile_recording_ok(&invalid_env_variant);

    compile_recording_ok(&circumfix_snapshot(&[], &[]).0);
    compile_recording_ok(&circumfix_snapshot(&["env-after-vowel"], &[]).0);

    // Variant entry (mirrors variant_entry_appends_infl_type_gloss_to_the_base_sense_gloss).
    let (mut variant_entry_variant, f) = fixture();
    let infl_type_guid = "infl-plural".to_string();
    variant_entry_variant
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
    variant_entry_variant.lexicon.entries.push(LexEntry {
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
    });
    compile_recording_ok(&variant_entry_variant);

    // Both circumfix halves conditioned (mirrors a_circumfix_with_environments_on_both_halves_unions_them).
    let (mut both_halves_circumfix, _f) =
        circumfix_snapshot(&["env-after-vowel"], &["env-before-vowel"]);
    both_halves_circumfix
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-after-vowel".to_string(),
            name: "after vowel".to_string(),
            representation: "/[V]_".to_string(),
        });
    both_halves_circumfix
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-before-vowel".to_string(),
            name: "before vowel".to_string(),
            representation: "/_[V]".to_string(),
        });
    compile_recording_ok(&both_halves_circumfix);

    // A circumfix half carrying a position (mirrors a_circumfix_half_carrying_a_position_builds_with_it_unioned_in).
    let (mut position_circumfix, _f) = circumfix_snapshot(&[], &[]);
    position_circumfix
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-after-vowel".to_string(),
            name: "after vowel".to_string(),
            representation: "/[V]_".to_string(),
        });
    position_circumfix
        .lexicon
        .entries
        .iter_mut()
        .find(|e| e.guid == "entry-circumfix")
        .expect("circumfix_snapshot must have pushed entry-circumfix")
        .allomorphs[0]
        .positions
        .push("env-after-vowel".to_string());
    compile_recording_ok(&position_circumfix);

    // Morphosyntactic closed feature (mirrors morphosyntactic_closed_feature_compiles_into_the_syntactic_feature_system).
    let (mut closed_feature_variant, _f) = fixture();
    let number_guid = "feat-number".to_string();
    let sg_guid = "val-sg".to_string();
    let pl_guid = "val-pl".to_string();
    closed_feature_variant.feature_systems.morphosyntactic = FeatureSystem {
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
    match &mut closed_feature_variant.lexicon.entries[0].msas[0] {
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
    compile_recording_ok(&closed_feature_variant);

    // Enclitic dual-stratum entry (mirrors enclitic_entry_compiles_to_clitic_stratum_lex_entry_and_affix_rule).
    let (mut enclitic_variant, f) = fixture();
    enclitic_variant.lexicon.entries.push(LexEntry {
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
    });
    compile_recording_ok(&enclitic_variant);
}

/// The variant-entry expansion atom (`expansion(Entry, variant_guid, [main_msa_guid], "variant")`) records the variant/main-MSA pairing itself, distinct from the entries/allomorphs/MSAs it draws from.
#[test]
fn variant_entry_expansion_atom_is_synthesized_and_represented() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-variant".to_string(),
        citation_form: vec![ws("sen", "kumi")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-variant", MorphType::Stem, "kumi")],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![EntryRef::Variant {
            guid: "entryref-variant-expansion".to_string(),
            component_lexemes: vec![f.stem_entry.clone()],
            variant_entry_types: Vec::new(),
        }],
    });

    let (_grammar, _warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    let expansion = InventoryKey::expansion(
        InventoryKind::Entry,
        "entry-variant".to_string(),
        vec![f.stem_msa.clone()],
        "variant",
    );
    assert!(
        inventory.synthesized.contains(&expansion),
        "expected the variant-entry expansion atom synthesized"
    );
    assert!(
        inventory.represented.contains(&expansion),
        "expected the variant-entry expansion atom represented"
    );
}

/// `compile_project` is a thin wrapper that calls `compile_project_recording` and discards the recorder, so this can only prove that delegation is intact -- it cannot detect a regression in the recording seam itself, since `compile_project` has no independent implementation to diverge from it (the fixture's own warning-free pin lives on `stem_and_inflectional_affix_and_template_compile_into_expected_grammar`, via `compile_project` directly).
#[test]
fn compile_project_delegates_to_compile_project_recording_and_discards_the_recorder() {
    let (snapshot, _f) = fixture();
    let (_grammar_plain, warnings_plain) =
        compile_project(&snapshot).expect("fixture must compile");
    let (_grammar_recorded, warnings_recorded, _inventory, _issues) =
        compile_recording_ok(&snapshot);
    assert_eq!(warnings_plain, warnings_recorded);
}

#[test]
fn unsegmentable_allomorph_is_rejected_with_the_expected_code_and_the_legacy_warning_text() {
    let (mut snapshot, _f) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "xyz")];

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("cannot segment") && w.contains("allo-stem")),
        "expected the legacy 'cannot segment' warning to survive unchanged; got {warnings:?}"
    );
    assert_eq!(grammar.entries.len(), 0, "the unsegmentable stem entry must be dropped");

    let key = InventoryKey::object(InventoryKind::Allomorph, "allo-stem".to_string());
    assert!(
        inventory.rejected.contains(&key),
        "the unsegmentable allomorph must be recorded rejected"
    );
    assert!(
        issues
            .iter()
            .any(|i| i.code == super::issue_codes::ALLOMORPH_UNSEGMENTABLE),
        "expected an issue carrying the unsegmentable code; got {issues:?}"
    );
}

#[test]
fn a_disabled_compound_rule_is_considered_but_not_selected_with_no_issue() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.compound_rules.push(CompoundRule::Endocentric {
        guid: "cr-disabled".to_string(),
        name: "Disabled".to_string(),
        disabled: true,
        head_last: false,
        left: CompoundConstituentRequirement::default(),
        right: CompoundConstituentRequirement::default(),
        overriding: CompoundOutcome::default(),
    });

    let (_grammar, _warnings, inventory, issues) = compile_recording_ok(&snapshot);
    let key = InventoryKey::object(InventoryKind::CompoundRule, "cr-disabled".to_string());
    assert!(inventory.considered.contains(&key), "must be considered");
    assert!(!inventory.selected.contains(&key), "a disabled rule must never be selected");
    assert!(!inventory.rejected.contains(&key), "a disabled rule is not a rejection");
    assert!(
        issues.iter().all(|i| i.message != "cr-disabled"),
        "a disabled rule must not produce an issue"
    );
}

#[test]
fn an_unresolved_environment_guid_on_a_root_allomorph_is_a_quiet_attachment_rejection() {
    let (mut snapshot, _f) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0]
        .environments
        .push("dangling-env-guid".to_string());

    let (_grammar, warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings.is_empty(),
        "an unresolved environment guid is silently dropped, never warned: {warnings:?}"
    );
    let attachment = InventoryKey::attachment(
        InventoryKind::Environment,
        "allo-stem".to_string(),
        "dangling-env-guid".to_string(),
        "environment",
    );
    assert!(
        inventory.rejected.contains(&attachment),
        "the dangling environment attachment must still be recorded rejected"
    );
}

#[test]
fn circumfix_cross_product_expansion_is_synthesized_and_represented() {
    let (snapshot, _f) = circumfix_snapshot(&[], &[]);
    let (_grammar, warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    assert!(warnings.iter().all(|w| !w.contains("circumfix")));

    let expansion = InventoryKey::expansion(
        InventoryKind::Allomorph,
        "allo-circ-prefix".to_string(),
        vec!["allo-circ-suffix".to_string()],
        "circumfix-cross-product",
    );
    assert!(inventory.synthesized.contains(&expansion));
    assert!(inventory.represented.contains(&expansion));
}

/// A circumfix's suffix half is built into the grammar too, so it must show up as represented, not merely selected.
#[test]
fn circumfix_suffix_half_is_not_silently_omitted() {
    for (label, prefix_env, suffix_env) in [
        ("circumfix_unconditioned", [].as_slice(), [].as_slice()),
        (
            "circumfix_dangling_env",
            ["dangling-env-guid"].as_slice(),
            [].as_slice(),
        ),
    ] {
        let (snapshot, _f) = circumfix_snapshot(prefix_env, suffix_env);
        let (_grammar, _warnings, delta) =
            compile_project_measured(&snapshot).expect("circumfix must compile");
        assert!(
            delta.silently_omitted.is_empty(),
            "{label}: silently_omitted must be empty, found {:?}",
            delta.silently_omitted
        );
        let suffix_key =
            InventoryKey::object(InventoryKind::Allomorph, "allo-circ-suffix".to_string());
        assert!(
            delta.inventory.represented.contains(&suffix_key),
            "{label}: the suffix half's own object key must be represented"
        );
    }
}

#[test]
fn default_compounding_synthesizes_exactly_two_compound_rule_atoms_only_when_none_are_authored() {
    let (snapshot, _f) = fixture();
    let (_grammar, _warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    let synthesized_compound_rules = inventory
        .synthesized
        .iter()
        .filter(|k| k.kind == InventoryKind::CompoundRule)
        .count();
    assert_eq!(synthesized_compound_rules, 2);

    let (mut snapshot_with_authored, _f2) = fixture();
    snapshot_with_authored
        .morphology
        .compound_rules
        .push(CompoundRule::Endocentric {
            guid: "cr-authored".to_string(),
            name: "Authored".to_string(),
            disabled: false,
            head_last: false,
            left: CompoundConstituentRequirement::default(),
            right: CompoundConstituentRequirement::default(),
            overriding: CompoundOutcome::default(),
        });
    let (_grammar2, _warnings2, inventory2, _issues2) = compile_recording_ok(&snapshot_with_authored);
    let synthesized_compound_rules_2 = inventory2
        .synthesized
        .iter()
        .filter(|k| k.kind == InventoryKind::CompoundRule)
        .count();
    assert_eq!(synthesized_compound_rules_2, 0);
}

#[test]
fn custom_strata_setting_is_recorded_rejected() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.strata = Some("Morphology,(Clitics)".to_string());

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(warnings.iter().any(|w| w.contains("Strata")));
    let key = InventoryKey::setting(InventoryKind::StrataConfiguration, "Strata");
    assert!(inventory.rejected.contains(&key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::STRATA_CUSTOM_UNSUPPORTED));
}

/// `is_valid_rule_form`'s three reject sites must select the allomorph before rejecting it (`rejected ⊆ selected`), for both the positionless-infix and the bracket-pattern (reduplication) routes.
#[test]
fn is_valid_rule_form_rejections_are_recorded_selected_before_rejected() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-invalid-forms".to_string(),
        citation_form: vec![ws("sen", "invalid")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![
            simple_allomorph("allo-infix-nopos", MorphType::Infix, "t"),
            simple_allomorph("allo-bracket-form", MorphType::Suffix, "[X]"),
        ],
        msas: vec![Msa::Unclassified {
            guid: "msa-invalid-forms".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);

    assert!(
        warnings
            .iter()
            .any(|w| w.contains("reduplication/bracket-pattern") && w.contains("allo-bracket-form")),
        "expected the legacy reduplication warning to survive unchanged; got {warnings:?}"
    );

    let infix_key = InventoryKey::object(InventoryKind::Allomorph, "allo-infix-nopos".to_string());
    assert!(
        inventory.selected.contains(&infix_key),
        "the positionless infix allomorph must be selected before rejection"
    );
    assert!(inventory.rejected.contains(&infix_key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::ALLOMORPH_NOT_RULE_FORM));

    let bracket_key = InventoryKey::object(InventoryKind::Allomorph, "allo-bracket-form".to_string());
    assert!(
        inventory.selected.contains(&bracket_key),
        "the bracket-pattern allomorph must be selected before rejection"
    );
    assert!(inventory.rejected.contains(&bracket_key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::ALLOMORPH_REDUPLICATION_UNSUPPORTED));
}

/// A bare `Circumfix`/`DiscontigPhrase`-typed allomorph must be selected then quietly rejected, not left dangling.
#[test]
fn circumfix_typed_allomorph_outside_a_cross_product_is_selected_before_quiet_rejection() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-bare-circumfix".to_string(),
        citation_form: vec![ws("sen", "bare")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph(
            "allo-bare-circumfix",
            MorphType::Circumfix,
            "x",
        )],
        msas: vec![Msa::Unclassified {
            guid: "msa-bare-circumfix".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "this rejection is quiet: {warnings:?}");

    let key = InventoryKey::object(InventoryKind::Allomorph, "allo-bare-circumfix".to_string());
    assert!(inventory.selected.contains(&key), "must be selected before rejection");
    assert!(inventory.rejected.contains(&key));
    assert!(issues.iter().any(
        |i| i.code == super::issue_codes::ALLOMORPH_MORPH_TYPE_UNSUPPORTED_AS_RULE_FORM
    ));
}

/// `build_phon_features`'s complex-feature drop must select the feature before rejecting it (`rejected ⊆ selected`).
#[test]
fn complex_phonological_feature_is_recorded_selected_before_rejected() {
    let (mut snapshot, _f) = fixture();
    snapshot.feature_systems.phonological.complex_features.push(ComplexFeature {
        guid: "cf-phon".to_string(),
        name: "PhonComplex".to_string(),
        abbreviation: "pc".to_string(),
        feature_type: None,
    });

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("phonological complex feature")),
        "expected the legacy complex-feature warning to survive unchanged; got {warnings:?}"
    );
    let key = InventoryKey::object(InventoryKind::FeatureDefinition, "cf-phon".to_string());
    assert!(
        inventory.selected.contains(&key),
        "the complex phonological feature must be selected before rejection"
    );
    assert!(inventory.rejected.contains(&key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::PHON_COMPLEX_FEATURE_UNSUPPORTED));
}

// --- finalizer revocation: a compacted-away mrule/natclass/co-occurrence rule is un-represented ---

/// A `template_only` mrule whose slot no template ever references is orphaned by compaction, so its MSA and allomorph keys end up rejected, not represented.
#[test]
fn template_only_mrule_orphaned_by_no_template_is_revoked_unreachable_after_compaction() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-orphan".to_string(),
        citation_form: vec![ws("sen", "-ka")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph("allo-orphan", MorphType::Suffix, "ka")],
        msas: vec![Msa::Inflectional {
            guid: "msa-orphan".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: vec!["slot-never-templated".to_string()],
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "compaction revocation is silent: {warnings:?}");

    let msa_key = InventoryKey::object(InventoryKind::Msa, "msa-orphan".to_string());
    assert!(inventory.rejected.contains(&msa_key));
    assert!(!inventory.represented.contains(&msa_key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::MRULE_UNREACHABLE_COMPACTED));

    let allo_key = InventoryKey::object(InventoryKind::Allomorph, "allo-orphan".to_string());
    assert!(inventory.rejected.contains(&allo_key));
    assert!(!inventory.represented.contains(&allo_key));

    assert!(
        grammar.mrules.iter().all(|r| match r {
            MorphRuleDef::AffixProcess(d) =>
                grammar.morphemes[d.morpheme.0 as usize].xml_key != "msa-orphan",
            _ => true,
        }),
        "the orphaned mrule must not survive compaction: {:?}",
        grammar.mrules
    );
}

/// An unnamed, unreferenced, non-last natural class is revoked; a referenced one and `__any__` stay represented.
#[test]
fn unreferenced_unnamed_natural_class_is_revoked_but_referenced_and_any_survive() {
    let (mut snapshot, _f) = fixture();
    snapshot.phonology.natural_classes.push(SnapNaturalClass::Segments {
        guid: "nc-orphan".to_string(),
        name: String::new(),
        phonemes: vec!["ph-a".to_string()],
    });
    snapshot.phonology.natural_classes.push(SnapNaturalClass::Segments {
        guid: "nc-last-unnamed".to_string(),
        name: String::new(),
        phonemes: vec!["ph-i".to_string()],
    });
    snapshot.phonology.natural_classes.push(SnapNaturalClass::Segments {
        guid: "nc-vowel".to_string(),
        name: "V".to_string(),
        phonemes: vec!["ph-a".to_string(), "ph-i".to_string(), "ph-u".to_string()],
    });
    snapshot.phonology.environments.push(pg_snapshot::phonology::Environment {
        guid: "env-v".to_string(),
        name: String::new(),
        representation: "/_[V]".to_string(),
    });
    snapshot.lexicon.entries[1].allomorphs[0]
        .environments
        .push("env-v".to_string());

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "compaction revocation is silent: {warnings:?}");

    let orphan_key = InventoryKey::object(InventoryKind::NaturalClass, "nc-orphan".to_string());
    assert!(inventory.rejected.contains(&orphan_key));
    assert!(!inventory.represented.contains(&orphan_key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::NATURAL_CLASS_UNREFERENCED_COMPACTED));
    assert!(!grammar.natural_classes.iter().any(|d| d.xml_id == "nc-orphan"));

    let referenced_key = InventoryKey::object(InventoryKind::NaturalClass, "nc-vowel".to_string());
    assert!(inventory.represented.contains(&referenced_key));

    let any_key = InventoryKey::object(InventoryKind::NaturalClass, "__any__".to_string());
    assert!(inventory.represented.contains(&any_key));
}

/// A morpheme co-occurrence rule targeting an orphaned-away morpheme is revoked; one whose targets all survive stays represented, even sharing the same primary.
#[test]
fn morpheme_coocurrence_rule_targeting_a_compacted_away_morpheme_is_revoked_but_a_surviving_one_stays(
) {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-orphan".to_string(),
        citation_form: vec![ws("sen", "-ka")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph("allo-orphan", MorphType::Suffix, "ka")],
        msas: vec![Msa::Inflectional {
            guid: "msa-orphan".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: vec!["slot-never-templated".to_string()],
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });
    snapshot.morphology.adhoc_prohibitions.push(AdhocProhibition::Morpheme {
        guid: "coocc-dropped".to_string(),
        disabled: false,
        primary: f.stem_msa.clone(),
        others: vec!["msa-orphan".to_string()],
        adjacency: Adjacency::Anywhere,
    });
    snapshot.morphology.adhoc_prohibitions.push(AdhocProhibition::Morpheme {
        guid: "coocc-survives".to_string(),
        disabled: false,
        primary: f.stem_msa.clone(),
        others: vec![f.suffix_msa.clone()],
        adjacency: Adjacency::Anywhere,
    });

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "compaction revocation is silent: {warnings:?}");

    let dropped_key =
        InventoryKey::object(InventoryKind::MorphemeCoOccurrence, "coocc-dropped".to_string());
    assert!(inventory.rejected.contains(&dropped_key));
    assert!(!inventory.represented.contains(&dropped_key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::COOCCURRENCE_TARGET_UNREACHABLE));

    let survives_key =
        InventoryKey::object(InventoryKind::MorphemeCoOccurrence, "coocc-survives".to_string());
    assert!(inventory.represented.contains(&survives_key));
}

/// Every `represented` `Msa`/`NaturalClass` object atom must match an object in the compiled `Grammar`.
#[test]
fn fixture_represented_msa_and_natural_class_atoms_match_the_final_grammar_exactly() {
    let (snapshot, _f) = fixture();
    let (grammar, warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    fn object_guid(k: &InventoryKey) -> Option<String> {
        match &k.identity {
            pg_snapshot::InventoryIdentity::Object { guid } => Some(guid.clone()),
            _ => None,
        }
    }

    let represented_msas: std::collections::BTreeSet<String> = inventory
        .represented
        .iter()
        .filter(|k| k.kind == InventoryKind::Msa)
        .filter_map(object_guid)
        .collect();
    let grammar_msas: std::collections::BTreeSet<String> =
        grammar.morphemes.iter().map(|m| m.xml_key.clone()).collect();
    assert_eq!(represented_msas, grammar_msas);

    let represented_natclasses: std::collections::BTreeSet<String> = inventory
        .represented
        .iter()
        .filter(|k| k.kind == InventoryKind::NaturalClass)
        .filter_map(object_guid)
        .collect();
    let grammar_natclasses: std::collections::BTreeSet<String> = grammar
        .natural_classes
        .iter()
        .map(|d| d.xml_id.clone())
        .collect();
    assert_eq!(represented_natclasses, grammar_natclasses);
}

/// `inventory::finalize` must panic on a removed id whose owner never published lineage for it.
#[test]
#[should_panic(expected = "removed by reachability compaction but published no lineage")]
fn finalize_panics_on_a_removed_mrule_id_with_no_published_lineage() {
    let mut recorder = pg_snapshot::SelectionRecorder::default();
    let lineage = super::inventory::Lineage::default();
    super::inventory::finalize(
        &mut recorder,
        &lineage,
        vec![42],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
}

/// `inventory::finalize` must panic on a removed natural class whose owner never published lineage for it.
#[test]
#[should_panic(expected = "removed by reachability compaction but published no lineage")]
fn finalize_panics_on_a_removed_natural_class_id_with_no_published_lineage() {
    let mut recorder = pg_snapshot::SelectionRecorder::default();
    let lineage = super::inventory::Lineage::default();
    super::inventory::finalize(
        &mut recorder,
        &lineage,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![7],
    );
}

/// `inventory::finalize` must panic on a removed morpheme co-occurrence rule whose owner never published lineage for it.
#[test]
#[should_panic(expected = "removed by reachability compaction but published no lineage")]
fn finalize_panics_on_a_removed_morpheme_cooccurrence_id_with_no_published_lineage() {
    let mut recorder = pg_snapshot::SelectionRecorder::default();
    let lineage = super::inventory::Lineage::default();
    super::inventory::finalize(
        &mut recorder,
        &lineage,
        Vec::new(),
        Vec::new(),
        vec![(3, 0)],
        Vec::new(),
    );
}

/// `inventory::finalize` must panic on a removed allomorph co-occurrence rule whose owner never published lineage for it.
#[test]
#[should_panic(expected = "removed by reachability compaction but published no lineage")]
fn finalize_panics_on_a_removed_allomorph_cooccurrence_id_with_no_published_lineage() {
    let mut recorder = pg_snapshot::SelectionRecorder::default();
    let lineage = super::inventory::Lineage::default();
    super::inventory::finalize(
        &mut recorder,
        &lineage,
        Vec::new(),
        vec![(5, 0)],
        Vec::new(),
        Vec::new(),
    );
}

/// An allomorph co-occurrence rule whose OWNER allomorph is compacted away with its (template-only, unreferenced) mrule is revoked, not represented, and absent from every surviving `co_occurrence` Vec.
#[test]
fn allomorph_cooccurrence_rule_whose_owner_is_compacted_away_is_revoked() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-orphan".to_string(),
        citation_form: vec![ws("sen", "-ka")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph("allo-orphan", MorphType::Suffix, "ka")],
        msas: vec![Msa::Inflectional {
            guid: "msa-orphan".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: vec!["slot-never-templated".to_string()],
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });
    snapshot.morphology.adhoc_prohibitions.push(AdhocProhibition::Allomorph {
        guid: "coocc-owner-orphaned".to_string(),
        disabled: false,
        primary: "allo-orphan".to_string(),
        others: vec!["allo-stem".to_string()],
        adjacency: Adjacency::Anywhere,
    });

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "compaction revocation is silent: {warnings:?}");

    let key = InventoryKey::object(InventoryKind::AllomorphCoOccurrence, "coocc-owner-orphaned".to_string());
    assert!(inventory.rejected.contains(&key));
    assert!(!inventory.represented.contains(&key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::COOCCURRENCE_TARGET_UNREACHABLE));

    for e in &grammar.entries {
        for a in &e.allomorphs {
            assert!(a.co_occurrence.is_empty());
        }
    }
    for r in &grammar.mrules {
        let allos: &[crate::model::AffixAllomorphDef] = match r {
            MorphRuleDef::AffixProcess(d) => &d.allomorphs,
            MorphRuleDef::Realizational(d) => &d.allomorphs,
            MorphRuleDef::Compounding(_) => &[],
        };
        for a in allos {
            assert!(a.co_occurrence.is_empty());
        }
    }
}

/// An allomorph co-occurrence rule whose owner survives but whose only `others` target is compacted away is revoked; the legacy warning is unchanged from before this change.
#[test]
fn allomorph_cooccurrence_rule_whose_only_target_is_compacted_away_is_revoked() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-orphan".to_string(),
        citation_form: vec![ws("sen", "-ka")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph("allo-orphan", MorphType::Suffix, "ka")],
        msas: vec![Msa::Inflectional {
            guid: "msa-orphan".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: vec!["slot-never-templated".to_string()],
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });
    snapshot.morphology.adhoc_prohibitions.push(AdhocProhibition::Allomorph {
        guid: "coocc-target-orphaned".to_string(),
        disabled: false,
        primary: "allo-suffix".to_string(),
        others: vec!["allo-orphan".to_string()],
        adjacency: Adjacency::Anywhere,
    });

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert_eq!(
        warnings,
        vec![
            "allomorph co-occurrence rule: an 'others' target was dropped by mrule reachability \
             compaction; reference removed"
                .to_string()
        ],
        "the legacy warning text/order must stay byte-identical"
    );

    let key = InventoryKey::object(InventoryKind::AllomorphCoOccurrence, "coocc-target-orphaned".to_string());
    assert!(inventory.rejected.contains(&key));
    assert!(!inventory.represented.contains(&key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::COOCCURRENCE_TARGET_UNREACHABLE));

    for e in &grammar.entries {
        for a in &e.allomorphs {
            assert!(a.co_occurrence.is_empty());
        }
    }
    for r in &grammar.mrules {
        if let MorphRuleDef::AffixProcess(d) = r {
            for a in &d.allomorphs {
                assert!(a.co_occurrence.is_empty());
            }
        }
    }
}

/// An allomorph co-occurrence rule whose owner and every `others` target survive stays represented.
#[test]
fn allomorph_cooccurrence_rule_whose_owner_and_targets_survive_stays_represented() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.adhoc_prohibitions.push(AdhocProhibition::Allomorph {
        guid: "coocc-survives".to_string(),
        disabled: false,
        primary: "allo-suffix".to_string(),
        others: vec!["allo-stem".to_string()],
        adjacency: Adjacency::Anywhere,
    });

    let (grammar, warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    let key = InventoryKey::object(InventoryKind::AllomorphCoOccurrence, "coocc-survives".to_string());
    assert!(inventory.represented.contains(&key));

    let found = grammar.mrules.iter().any(|r| match r {
        MorphRuleDef::AffixProcess(d) => d.allomorphs.iter().any(|a| !a.co_occurrence.is_empty()),
        _ => false,
    });
    assert!(found, "the surviving co-occurrence rule must remain on its owner's allomorph");
}

/// One owner allomorph with TWO co-occurrence rules: pins the per-rule `idx`, never exercised at index >= 1 before this test.
#[test]
fn owner_with_two_cooccurrence_rules_revokes_only_the_one_whose_target_is_compacted_away() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-orphan".to_string(),
        citation_form: vec![ws("sen", "-ka")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph("allo-orphan", MorphType::Suffix, "ka")],
        msas: vec![Msa::Inflectional {
            guid: "msa-orphan".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: vec!["slot-never-templated".to_string()],
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });
    // idx 0 on "allo-suffix": targets "allo-stem", which survives compaction -- must stay represented.
    snapshot.morphology.adhoc_prohibitions.push(AdhocProhibition::Allomorph {
        guid: "coocc-idx0-survives".to_string(),
        disabled: false,
        primary: "allo-suffix".to_string(),
        others: vec!["allo-stem".to_string()],
        adjacency: Adjacency::Anywhere,
    });
    // idx 1 on the SAME owner "allo-suffix": targets "allo-orphan", which is compacted away -- must be revoked.
    snapshot.morphology.adhoc_prohibitions.push(AdhocProhibition::Allomorph {
        guid: "coocc-idx1-revoked".to_string(),
        disabled: false,
        primary: "allo-suffix".to_string(),
        others: vec!["allo-orphan".to_string()],
        adjacency: Adjacency::Anywhere,
    });

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert_eq!(
        warnings,
        vec![
            "allomorph co-occurrence rule: an 'others' target was dropped by mrule reachability \
             compaction; reference removed"
                .to_string()
        ],
        "only the idx-1 rule's target compaction produces a warning"
    );

    let survives =
        InventoryKey::object(InventoryKind::AllomorphCoOccurrence, "coocc-idx0-survives".to_string());
    let revoked =
        InventoryKey::object(InventoryKind::AllomorphCoOccurrence, "coocc-idx1-revoked".to_string());
    assert!(inventory.represented.contains(&survives), "idx 0's rule must stay represented");
    assert!(!inventory.rejected.contains(&survives), "idx 0's rule must not be revoked");
    assert!(inventory.rejected.contains(&revoked), "idx 1's rule must be revoked");
    assert!(!inventory.represented.contains(&revoked), "idx 1's rule must not stay represented");
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::COOCCURRENCE_TARGET_UNREACHABLE));

    let mut surviving_coocc_count = 0usize;
    for r in &grammar.mrules {
        if let MorphRuleDef::AffixProcess(d) = r {
            for a in &d.allomorphs {
                surviving_coocc_count += a.co_occurrence.len();
            }
        }
    }
    assert_eq!(
        surviving_coocc_count, 1,
        "exactly the idx-0 rule must remain on the compiled allomorph"
    );
}

/// `compile_project_measured` must change no behaviour versus `compile_project`: same `Grammar`, same warnings.
#[test]
fn compile_project_measured_changes_no_behaviour_versus_compile_project() {
    let (snapshot, _f) = fixture();
    let (grammar_plain, warnings_plain) = compile_project(&snapshot).expect("fixture must compile");
    let (grammar_measured, warnings_measured, _delta) =
        compile_project_measured(&snapshot).expect("fixture must compile");

    assert_eq!(
        warnings_plain.len(),
        warnings_measured.len(),
        "warning count must match"
    );
    for (a, b) in warnings_plain.iter().zip(warnings_measured.iter()) {
        assert_eq!(a, b, "warnings must be byte-identical element by element");
    }
    assert_grammars_equal(&grammar_plain, &grammar_measured);
}

// --- Task 4: typed compile options/issues -----------------------------------------------------

/// No `..` rest pattern: a new field on either type fails to compile until named here too.
#[test]
fn compile_options_and_output_carry_exactly_their_declared_fields() {
    let CompileOptions {
        substrate,
        semantic_loss,
    } = CompileOptions::default();
    assert_eq!(substrate, SubstratePolicy::Auto);
    assert_eq!(semantic_loss, SemanticLossPolicy::Refuse);

    let (snapshot, _f) = fixture();
    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let CompileOutput {
        grammar,
        issues,
        substrate,
        inventory,
    } = out;
    assert_eq!(grammar.entries.len(), 1);
    assert!(issues.is_empty());
    assert!(substrate.inferred_segments.is_empty());
    assert!(inventory.inventory.rejected.is_empty());
}

/// `compile_project_with` under default options must match `compile_project` message-for-message.
#[test]
fn compile_project_with_default_options_matches_compile_project() {
    let (snapshot, _f) = fixture();
    let (grammar_tuple, warnings_tuple) = compile_project(&snapshot).expect("must compile");
    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let messages: Vec<String> = out.issues.iter().map(|i| i.message.clone()).collect();
    assert_eq!(messages, warnings_tuple);
    assert_grammars_equal(&grammar_tuple, &out.grammar);
}

/// Every compile-stage warning arrives as a non-fatal issue, on a snapshot that actually warns.
#[test]
fn every_compile_stage_warning_becomes_a_non_fatal_conversion_issue() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.strata = Some("Morphology,(Clitics)".to_string());
    let (_grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(
        warnings.iter().any(|w| w.contains("Strata")),
        "fixture must still produce the legacy Strata warning; got {warnings:?}"
    );

    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    assert_eq!(out.issues.len(), warnings.len());
    for issue in &out.issues {
        assert!(!issue.fatal, "adapted compile-stage issue must be non-fatal: {issue:?}");
    }
    assert!(out
        .issues
        .iter()
        .any(|i| i.message.contains("Strata") && !i.fatal));
}

/// `Refuse` rejects a fatal imported issue; `MeasureOnly` on the same snapshot retains it instead.
#[test]
fn refuse_rejects_a_fatal_imported_issue_but_measure_only_retains_it() {
    let (mut snapshot, _f) = fixture();
    snapshot.conversion_provenance.import_issues.push(ConversionIssue {
        code: "test.imported-fatal".to_string(),
        class: IssueClass::InvalidSource,
        source: None,
        fatal: true,
        message: "test: a fatal import-stage issue".to_string(),
    });

    let err = compile_project_with(&snapshot, CompileOptions::default())
        .expect_err("a fatal imported issue must refuse under Refuse");
    assert!(matches!(err, GrammarError::Conversion(_)));
    assert!(err.issues().iter().any(|i| i.code == "test.imported-fatal"));

    let measured = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
            ..CompileOptions::default()
        },
    )
    .expect("MeasureOnly must never refuse");
    assert!(measured.issues.iter().any(|i| i.code == "test.imported-fatal" && i.fatal));
}

/// Unknown source provenance is fatal under `Refuse` too; `MeasureOnly` retains it instead.
#[test]
fn refuse_rejects_unknown_source_provenance_but_measure_only_retains_it() {
    let (mut snapshot, _f) = fixture();
    snapshot.conversion_provenance.source_inventory_status = SourceInventoryStatus::Unknown;

    let err = compile_project_with(&snapshot, CompileOptions::default())
        .expect_err("unknown source provenance must refuse under Refuse");
    assert!(err
        .issues()
        .iter()
        .any(|i| i.code == "conversion.source-provenance-unknown" && i.fatal));

    let measured = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
            ..CompileOptions::default()
        },
    )
    .expect("MeasureOnly must never refuse");
    assert!(measured
        .issues
        .iter()
        .any(|i| i.code == "conversion.source-provenance-unknown" && i.fatal));
}

/// `GrammarError::issues()` returns `&[]` for every non-`Conversion` variant.
#[test]
fn grammar_error_issues_is_empty_for_non_conversion_variants() {
    let err = GrammarError::Semantic("test".to_string());
    assert!(err.issues().is_empty());
}

/// `resolve` takes only `ActiveParser`; no `ParserProfile`/XAMPLE cap type is even in scope here.
#[test]
fn options_and_output_types_never_carry_parser_profile_or_xample_cap_state() {
    let _ = ActiveParser::XAmple;
    let resolved = SubstratePolicy::Auto.resolve(ActiveParser::XAmple, false);
    assert_eq!(resolved, ResolvedSubstratePolicy::CompleteFromUsage);
}
