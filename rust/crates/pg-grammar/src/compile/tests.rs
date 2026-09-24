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
    BoundaryMarker, MetathesisRule, NaturalClass as SnapNaturalClass, PhonContext, Phoneme,
    PhonologicalRule, Phonology, RuleDirection,
};
use pg_snapshot::project::Project;
use pg_snapshot::{
    ActiveParser, ConversionIssue, FeatureSystems, InventoryKey, InventoryKind, IssueClass,
    Snapshot, SourceInventoryStatus, WsForm,
};

use crate::model::{MorphRuleDef, PartialMorphemeReason, TemplateSlotZone};
use crate::GrammarError;

use super::test_support::assert_grammars_equal;
use super::{
    compile_project, compile_project_measured, compile_project_recording, compile_project_with,
    environment, CompileOptions, CompileOutput, ResolvedSubstratePolicy, SemanticLossPolicy,
    SubstratePolicy,
};

fn warning_metadata(warning: &pg_snapshot::Warning) -> pg_snapshot::ImportWarningMetadata {
    let code = pg_snapshot::ImportWarningCode::from_wire_or_unregistered(&warning.code);
    pg_snapshot::import_warning_metadata(code)
}

fn warning_guidance(warning: &pg_snapshot::Warning) -> Option<String> {
    warning_metadata(warning).guidance_for_subject(
        warning
            .subjects
            .first()
            .and_then(|subject| subject.name.as_deref()),
        warning.subjects.first().map(|subject| subject.class),
    )
}

#[test]
fn warning_deduplication_keeps_the_first_linguist_wording_for_a_fact() {
    let subject = || {
        pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::MoForm)
            .guid("00000000-0000-0000-0000-000000000042")
    };
    let preferred = pg_snapshot::Warning::new(
        pg_snapshot::ImportWarningCode::AllomorphUnsegmentable,
        "Allomorph 'xyz' could not be segmented with this project's phonemes.",
    )
    .with_subject(subject());
    let engine_detail = pg_snapshot::Warning::new(
        pg_snapshot::ImportWarningCode::AllomorphUnsegmentable,
        "cannot segment \"xyz\": no character definition matches 'x' at position 0",
    )
    .with_subject(subject());

    let unique = super::warnings::deduplicate([preferred.clone(), engine_detail]);

    assert_eq!(unique, vec![preferred]);
}

/// Compiles `snapshot` through the recording seam and asserts the recorder's own invariants hold; returns everything a caller might want to inspect further.
fn compile_recording_ok(
    snapshot: &Snapshot,
) -> (
    crate::model::Grammar,
    Vec<pg_snapshot::Warning>,
    pg_snapshot::ConversionInventory,
    Vec<pg_snapshot::ConversionIssue>,
) {
    let (grammar, recorder, _substrate, substrate_issues, compile_warnings) =
        compile_project_recording(snapshot, SubstratePolicy::default()).expect("must compile");
    recorder
        .check_invariants()
        .expect("recorder invariants must hold");
    let (inventory, issues) = recorder.finish();
    let mut all_issues = snapshot.conversion_provenance.import_issues.clone();
    if snapshot.conversion_provenance.source_inventory_status == SourceInventoryStatus::Unknown {
        all_issues.push(pg_snapshot::ConversionIssue {
            code: super::issues::SOURCE_PROVENANCE_UNKNOWN,
            class: IssueClass::AmbiguousSource,
            source: None,
            fatal: true,
            message:
                "conversion provenance is unknown; cannot certify this conversion's completeness"
                    .to_string(),
        });
    }
    let mut warnings = super::warnings::from_issues(snapshot, &all_issues);
    warnings.extend(super::warnings::from_issues(snapshot, &substrate_issues));
    warnings.extend(compile_warnings);
    let warnings = super::warnings::deduplicate(warnings);
    (grammar, warnings, inventory, issues)
}

#[test]
fn import_warning_describes_failed_msa_without_internal_error_text() {
    let (snapshot, f) = fixture();
    let issues = [pg_snapshot::ConversionIssue {
        code: super::issue_codes::MSA_BUILD_FAILED,
        class: IssueClass::UnrepresentableForHc,
        source: Some(pg_snapshot::SourceRef {
            kind: pg_snapshot::FwClass::MoStemMsa,
            id: f.stem_msa,
        }),
        fatal: false,
        message: "internal decoder error: private feature identifier".to_string(),
    }];

    let warnings = super::warnings::from_issues(&snapshot, &issues);

    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].message,
        "Grammatical analysis for 'kuma' could not be imported; check its part of speech and features."
    );
    assert_eq!(warnings[0].subjects[0].name.as_deref(), Some("kuma"));
}

#[test]
fn import_warning_names_a_phonological_rule_without_internal_error_text() {
    let (mut snapshot, _) = fixture();
    let rule_guid = "10000000-0000-0000-0000-000000000001";
    snapshot.phonology.rules.push(PhonologicalRule::Rewrite(
        pg_snapshot::phonology::RewriteRule {
            guid: rule_guid.to_string(),
            name: "Vowel harmony".to_string(),
            direction: RuleDirection::LeftToRight,
            structural_description: Vec::new(),
            feature_constraint_variables: Vec::new(),
            right_hand_sides: Vec::new(),
        },
    ));
    let issues = [ConversionIssue {
        code: super::issue_codes::RULE_BUILD_FAILED,
        class: IssueClass::UnrepresentableForHc,
        source: Some(pg_snapshot::SourceRef {
            kind: pg_snapshot::FwClass::PhRegularRule,
            id: rule_guid.to_string(),
        }),
        fatal: false,
        message: "internal rewrite failure: private emitter detail".to_string(),
    }];

    let warnings = super::warnings::from_issues(&snapshot, &issues);

    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].message,
        "Phonological rule 'Vowel harmony' could not be loaded; check its structural description and change."
    );
    assert_eq!(
        warnings[0].subjects[0].name.as_deref(),
        Some("Vowel harmony")
    );
}

#[test]
fn import_warning_sanitizes_rule_form_failure_when_analysis_has_no_name() {
    let (snapshot, _) = fixture();
    let msa_guid = "10000000-0000-0000-0000-000000000002";
    let issues = [ConversionIssue {
        code: super::issue_codes::MSA_NO_RULE_FORM_ALLOMORPHS,
        class: IssueClass::UnrepresentableForHc,
        source: Some(pg_snapshot::SourceRef {
            kind: pg_snapshot::FwClass::MoDerivAffMsa,
            id: msa_guid.to_string(),
        }),
        fatal: false,
        message: "affix rule has zero loadable allomorphs".to_string(),
    }];

    let warnings = super::warnings::from_issues(&snapshot, &issues);

    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].message,
        "Grammatical analysis 'Unnamed grammatical analysis' has no affix form FieldWorks can use."
    );
    assert_eq!(warnings[0].subjects[0].guid.as_deref(), Some(msa_guid));
    assert_eq!(
        warnings[0].subjects[0].name.as_deref(),
        Some("Unnamed grammatical analysis")
    );
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
    let slot_rules = grammar.templates[0].slots[0].rules.clone();
    assert!(grammar
        .strata
        .iter()
        .flat_map(|stratum| stratum.mrules.iter())
        .all(|ordinary| !slot_rules.contains(ordinary)));
    grammar
        .final_template_prune_facts()
        .expect("post-compaction snapshot output must satisfy final-template ownership");
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

    let out =
        compile_project_with(&snapshot, CompileOptions::default()).expect("must still compile");
    assert!(
        out.issues
            .iter()
            .any(|i| i.code == super::issue_codes::ENVIRONMENT_INVALID && !i.fatal),
        "expected a non-fatal ENVIRONMENT_INVALID issue; got {:?}",
        out.issues
    );
    assert_eq!(
        out.grammar.entries.len(),
        1,
        "the stem entry must still compile"
    );
}

#[test]
fn unresolved_affix_environment_keeps_identifier_out_of_linguist_warning() {
    let (mut snapshot, _) = fixture();
    let dangling_guid = "dangling-env-guid";
    snapshot.lexicon.entries[1].allomorphs[0]
        .environments
        .push(dangling_guid.to_string());

    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("an unresolved affix environment is a non-fatal issue");
    let issue = output
        .issues
        .iter()
        .find(|issue| {
            issue.code == super::issue_codes::ENVIRONMENT_UNRESOLVED
                && issue.message.contains(dangling_guid)
        })
        .expect("the structured issue must retain the unresolved environment id");
    assert!(!issue.fatal);

    let warning = output
        .warnings
        .iter()
        .find(|warning| warning.code == super::issue_codes::ENVIRONMENT_UNRESOLVED.wire())
        .expect("the linguist warning must be emitted");
    assert!(
        !warning.message.contains(dangling_guid),
        "the linguist-facing description must not expose the environment id: {warning:?}"
    );
}

#[test]
fn compile_project_returns_structured_warnings() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-bad".to_string(),
            name: "bad environment".to_string(),
            representation: "/[Nas]_".to_string(),
        });
    snapshot.lexicon.entries[0].allomorphs[0]
        .environments
        .push("env-bad".to_string());

    let (_, warnings) = compile_project(&snapshot).expect("must still compile");
    let environment_warnings: Vec<_> = warnings
        .iter()
        .filter(|warning| warning.code == super::issue_codes::ENVIRONMENT_INVALID.wire())
        .collect();
    assert!(
        std::any::type_name_of_val(&warnings[0]).ends_with("pg_snapshot::warning::Warning"),
        "compile warnings must preserve their structured fields, got {warnings:?}"
    );
    assert_eq!(
        environment_warnings.len(),
        1,
        "duplicate environment reports must collapse"
    );
    let warning = environment_warnings[0];
    assert!(warning.message.contains("bad environment"));
    assert!(
        warning.message.contains("Nas"),
        "specific validation cause: {warning:?}"
    );
    assert_eq!(warning.subjects.len(), 1);
    assert_eq!(
        warning.subjects[0].class,
        pg_snapshot::FwClass::PhEnvironment
    );
    assert_eq!(warning.subjects[0].guid.as_deref(), Some("env-bad"));
    assert_eq!(warning.subjects[0].name.as_deref(), Some("bad environment"));
    assert_eq!(
        warning_guidance(warning).as_deref(),
        Some(
            format!(
                "In {}, correct the expression for phonological environment 'bad environment'.",
                pg_snapshot::fieldworks_paths::GRAMMAR_ENVIRONMENTS
            )
            .as_str()
        )
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
            variant_entry_types: vec![infl_type_guid.clone()],
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
    assert_eq!(
        grammar.morphemes[variant_morpheme_id]
            .source_msa_guid
            .as_deref(),
        Some(f.stem_msa.as_str())
    );
    assert_eq!(
        grammar.morphemes[variant_morpheme_id]
            .source_infl_type_guid
            .as_deref(),
        Some(infl_type_guid.as_str())
    );
    let variant_allomorph_id = variant.allomorphs[0].id.0 as usize;
    assert_eq!(
        grammar.allomorph_sources[variant_allomorph_id].form_guids,
        vec![Some("allo-variant".to_string())]
    );
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
        grammar.entries[0].is_partial(),
        "an MSA with no POS must be IsPartial"
    );
    assert_eq!(
        grammar.entries[0].partial_reason,
        Some(crate::model::PartialMorphemeReason::StemWithoutCategory)
    );
}

#[test]
fn empty_template_slot_is_recorded_without_a_linguist_warning() {
    let (mut snapshot, _f) = fixture();
    match &mut snapshot.lexicon.entries[1].msas[0] {
        Msa::Inflectional { slots, .. } => slots.clear(),
        _ => panic!("expected the fixture's inflectional MSA"),
    }

    let output =
        super::compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let grammar = output.grammar;
    let warnings = output.warnings;
    let slot_warnings: Vec<_> = warnings
        .iter()
        .filter(|warning| warning.code == super::issue_codes::TEMPLATE_SLOT_NO_RULES.wire())
        .collect();
    assert_eq!(
        slot_warnings.len(),
        0,
        "quietly dropped template slots are inventory issues, not linguist warnings"
    );
    let slot_issues: Vec<_> = output
        .issues
        .iter()
        .filter(|issue| issue.code == super::issue_codes::TEMPLATE_SLOT_NO_RULES)
        .collect();
    assert_eq!(
        slot_issues.len(),
        2,
        "both the slot and its template attachment must retain the slot identity"
    );
    assert!(slot_issues.iter().all(|issue| issue
        .source
        .as_ref()
        .is_some_and(
            |source| source.kind == pg_snapshot::FwClass::MoInflAffixSlot && source.id == "slot-pl"
        )));
    assert!(slot_issues.iter().all(
        |issue| issue.message == "Affix template slot 'Pl' has no loaded inflectional affixes."
    ));
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
        affix_rules[0].is_partial(),
        "an MoInflAffMsa with zero slots must be IsPartial"
    );
    assert_eq!(
        affix_rules[0].partial_reason,
        Some(crate::model::PartialMorphemeReason::InflectionalAffixWithoutTemplateSlot)
    );
    // With no slots referencing it, the template's one slot has no loaded affix and the whole template must be dropped.
    assert!(grammar.templates.is_empty());
}

/// `chardef::build`'s morph-boundary fallback (no authored `+` representation) must be recorded rejected. Default compounding is suppressed: it unconditionally segments a literal `"+"` (`compounding::plus_join`), an unrelated pre-existing assumption this test must not trip.
#[test]
fn missing_morph_boundary_marker_is_recorded_with_its_structured_warning() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .phonology
        .boundary_markers
        .retain(|b| b.guid != "bd-plus");
    snapshot.morphology.parser_parameters.no_default_compounding = true;

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(warnings.iter().any(|warning| {
        warning.code == super::issue_codes::BOUNDARY_MORPH_MARKER_UNRESOLVED.wire()
    }));
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

/// A table with no phonemes and no boundary cannot segment the default compounding rules' "+" join; this must refuse, never panic.
#[test]
fn compounding_over_a_table_with_no_phonemes_or_boundary_refuses_instead_of_panicking() {
    let (mut snapshot, _f) = fixture();
    snapshot.phonology.phonemes.clear();
    snapshot.phonology.boundary_markers.clear();

    let err = compile_project(&snapshot)
        .expect_err("a table that cannot segment '+' must be a typed refusal, not a panic");
    match err {
        crate::GrammarError::UnsegmentableBoundary(msg) => {
            assert!(msg.contains("main"), "message should name the table: {msg}");
            assert!(
                msg.contains('+'),
                "message should name the boundary marker: {msg}"
            );
        }
        other => panic!("expected UnsegmentableBoundary, got {other:?}"),
    }
}

#[test]
fn custom_strata_parser_parameter_warns_and_falls_back_to_the_default_layout() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.strata = Some("Morphology,(Clitics)".to_string());
    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(
        warnings.iter().any(|warning| {
            warning.code == super::issue_codes::STRATA_CUSTOM_UNSUPPORTED.wire()
        }),
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

    let out = compile_project_with(&snapshot, CompileOptions::default())
        .expect("metathesis must not be a hard error");
    assert!(
        out.issues
            .iter()
            .any(|i| i.code == super::issue_codes::RULE_METATHESIS_UNSUPPORTED && !i.fatal),
        "expected a non-fatal RULE_METATHESIS_UNSUPPORTED issue; got {:?}",
        out.issues
    );
    assert!(
        out.grammar.prules.is_empty(),
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
    let (grammar, warnings) =
        compile_project(&snapshot).expect("circumfix must not be a hard error");
    assert!(
        warnings.iter().all(|warning| {
            warning.code != super::issue_codes::CIRCUMFIX_ENVIRONMENT_COMBINATION_SKIPPED.wire()
                && warning.code != super::issue_codes::CIRCUMFIX_MISSING_HALF.wire()
        }),
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
    let built_morpheme = built[0].morpheme;
    assert_eq!(
        grammar.morphemes[built_morpheme.0 as usize]
            .source_msa_guid
            .as_deref(),
        Some("msa-circumfix")
    );
    let source = &grammar.allomorph_sources[built[0].allomorphs[0].id.0 as usize];
    assert_eq!(
        source.form_guids,
        vec![
            Some("allo-circ-prefix".to_string()),
            Some("allo-circ-suffix".to_string())
        ]
    );
}

#[test]
fn circumfix_missing_half_names_the_entry_and_the_missing_side_once() {
    let (mut snapshot, _) = circumfix_snapshot(&[], &[]);
    snapshot
        .lexicon
        .entries
        .iter_mut()
        .find(|entry| entry.guid == "entry-circumfix")
        .expect("circumfix entry exists")
        .allomorphs
        .pop();

    let (_, warnings) = compile_project(&snapshot).expect("a missing half is reported");
    let findings: Vec<_> = warnings
        .iter()
        .filter(|warning| warning.code == super::issue_codes::CIRCUMFIX_MISSING_HALF.wire())
        .collect();

    assert_eq!(findings.len(), 1, "one entry-level finding: {findings:?}");
    assert_eq!(findings[0].subjects.len(), 1);
    assert_eq!(
        findings[0].subjects[0].class,
        pg_snapshot::FwClass::LexEntry
    );
    assert_eq!(
        findings[0].subjects[0].guid.as_deref(),
        Some("entry-circumfix")
    );
    assert!(findings[0].message.contains("suffix"), "{findings:?}");
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
        warnings.iter().all(|warning| {
            warning.code != super::issue_codes::CIRCUMFIX_ENVIRONMENT_COMBINATION_SKIPPED.wire()
        }),
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

/// Both halves conditioned on their OUTER context (each env's inner side is empty here): the two outer contexts combine into one `AllomorphEnvironment`, per HCLoader.cs:1313-1322.
#[test]
fn a_circumfix_with_environments_on_both_halves_combines_the_outer_contexts_into_one() {
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
    assert!(warnings.iter().all(|warning| {
        warning.code != super::issue_codes::CIRCUMFIX_ENVIRONMENT_COMBINATION_SKIPPED.wire()
    }));
    let built = circumfix_rule_allomorphs(&grammar);
    assert_eq!(built.len(), 1);
    assert_eq!(
        built[0].environments.len(),
        1,
        "HCLoader calls Environments.Add at most once per allomorph: {:?}",
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
    assert!(warnings.iter().all(|warning| {
        warning.code != super::issue_codes::CIRCUMFIX_ENVIRONMENT_COMBINATION_SKIPPED.wire()
    }));
    let built = circumfix_rule_allomorphs(&grammar);
    assert_eq!(built.len(), 1);
    assert_eq!(
        built[0].environments.len(),
        1,
        "the prefix half's position must survive onto the combined allomorph like an environment would: {:?}",
        built[0].environments
    );
}

/// The structural half of docs/divergences/039's fix; parse behavior is pinned in `tests/circumfix_conditioning_parity.rs`.
#[test]
fn circumfix_cross_product_embeds_each_halfs_context_in_lhs_not_environment_union() {
    let (mut snapshot, f) = fixture();
    for (guid, rep) in [("ph-b", "b"), ("ph-p", "p"), ("ph-o", "o"), ("ph-z", "z")] {
        snapshot.phonology.phonemes.push(phoneme(guid, rep));
    }
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-v".to_string(),
            name: "V".to_string(),
            phonemes: vec!["ph-a".to_string()],
        });
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-c".to_string(),
            name: "C".to_string(),
            phonemes: vec!["ph-b".to_string()],
        });
    for (guid, rep) in [
        ("env-stem-starts-v", "/_[V]"),
        ("env-stem-starts-c", "/_[C]"),
        ("env-stem-ends-v", "/[V]_"),
        ("env-stem-ends-c", "/[C]_"),
    ] {
        snapshot
            .phonology
            .environments
            .push(pg_snapshot::phonology::Environment {
                guid: guid.to_string(),
                name: guid.to_string(),
                representation: rep.to_string(),
            });
    }

    let mut prefix_pu = simple_allomorph("allo-prefix-pu", MorphType::Prefix, "pu");
    prefix_pu.environments = vec!["env-stem-starts-v".to_string()];
    let mut prefix_ki = simple_allomorph("allo-prefix-ki", MorphType::Prefix, "ki");
    prefix_ki.environments = vec!["env-stem-starts-c".to_string()];
    let mut suffix_mo = simple_allomorph("allo-suffix-mo", MorphType::Suffix, "mo");
    suffix_mo.environments = vec!["env-stem-ends-v".to_string()];
    let mut suffix_zo = simple_allomorph("allo-suffix-zo", MorphType::Suffix, "zo");
    suffix_zo.environments = vec!["env-stem-ends-c".to_string()];

    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-circumfix".to_string(),
        citation_form: vec![ws("sen", "ki-...-zo")],
        lexeme_morph_type: MorphType::Circumfix,
        allomorphs: vec![prefix_pu, prefix_ki, suffix_mo, suffix_zo],
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

    let (grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(
        warnings.iter().all(|warning| {
            warning.code != super::issue_codes::CIRCUMFIX_ENVIRONMENT_COMBINATION_SKIPPED.wire()
                && warning.code != super::issue_codes::CIRCUMFIX_MISSING_HALF.wire()
        }),
        "unexpected circumfix warnings: {warnings:?}"
    );

    let built = circumfix_rule_allomorphs(&grammar);
    assert_eq!(
        built.len(),
        4,
        "2 prefix halves x 2 suffix halves is a 2x2 cross-product: {built:?}"
    );
    for allo in &built {
        assert!(
            allo.environments.is_empty(),
            "every environment here only conditions a stem edge already embedded in Lhs \
             (HCLoader.cs:1289-1290/:1298-1299), so none should survive as an AllomorphEnvironment: {:?}",
            allo.environments
        );
    }
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

    assert_eq!(
        grammar.allomorph_owners.len(),
        grammar.allomorph_sources.len(),
        "allomorph owner/source tables must stay parallel"
    );

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
fn unreachable_affix_before_live_rule_is_compacted_without_a_linguist_warning() {
    let (mut snapshot, f) = fixture();
    let orphan_slot = "slot-orphan".to_string();
    snapshot.morphology.parts_of_speech[0]
        .affix_slots
        .push(AffixSlot {
            guid: orphan_slot.clone(),
            name: "Orphan".to_string(),
            optional: false,
        });
    let orphan_entry = LexEntry {
        guid: "entry-orphan".to_string(),
        citation_form: vec![ws("sen", "-ta")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph("allo-orphan", MorphType::Suffix, "ta")],
        msas: vec![Msa::Inflectional {
            guid: "msa-orphan".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
            slots: vec![orphan_slot],
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    // An unused slot makes this earlier rule unreachable; its owner and source row must both drop.
    snapshot.lexicon.entries.insert(1, orphan_entry);

    let (grammar, warnings, _inventory, issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings.is_empty(),
        "internal compaction facts stay out of linguist output"
    );
    assert_eq!(
        issues
            .iter()
            .filter(|issue| issue.code == super::issue_codes::MRULE_UNREACHABLE_COMPACTED)
            .count(),
        2,
        "the two inventory keys retain their compaction issues"
    );
    assert_eq!(grammar.allomorph_owners.len(), 2);
    assert_eq!(grammar.allomorph_sources.len(), 2);
    assert_eq!(
        grammar.allomorph_sources[0].form_guids,
        vec![Some("allo-stem".to_string())]
    );
    assert_eq!(
        grammar.allomorph_sources[1].form_guids,
        vec![Some("allo-suffix".to_string())]
    );
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
        warnings.iter().all(|warning| {
            warning.code != super::issue_codes::ALLOMORPH_MORPH_TYPE_UNSUPPORTED_AS_RULE_FORM.wire()
        }),
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
    no_default_compounding
        .morphology
        .parser_parameters
        .no_default_compounding = true;
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

    // Both circumfix halves conditioned (mirrors a_circumfix_with_environments_on_both_halves_combines_the_outer_contexts_into_one).
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

/// `compile_project` returns the structured warnings produced by `compile_project_with`; the recorder helper reconstructs the same issue set for the same snapshot.
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
fn unsegmentable_allomorph_is_rejected_with_a_structured_warning() {
    let (mut snapshot, _f) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "xyz")];

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(warnings.iter().any(|warning| {
        warning.code == super::issue_codes::ALLOMORPH_UNSEGMENTABLE.wire()
            && warning
                .subjects
                .iter()
                .any(|subject| subject.guid.as_deref() == Some("allo-stem"))
    }));
    assert_eq!(
        grammar.entries.len(),
        0,
        "the unsegmentable stem entry must be dropped"
    );

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
fn unsegmentable_warning_names_the_fieldworks_form_and_action() {
    let (mut snapshot, _f) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "xyz")];

    let (_, warnings) =
        compile_project(&snapshot).expect("fixture must compile with a dropped form");
    let warning = warnings
        .iter()
        .find(|warning| warning.code == super::issue_codes::ALLOMORPH_UNSEGMENTABLE.wire())
        .expect("the unsegmentable allomorph must produce a warning");

    assert_eq!(
        warning.message,
        "Allomorph 'xyz' could not be segmented with this project's phonemes."
    );
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning.code == super::issue_codes::ALLOMORPH_UNSEGMENTABLE.wire())
            .count(),
        1,
        "the substrate and allomorph compiler reports describe the same fact"
    );
    assert_eq!(warning.subjects.len(), 1);
    assert_eq!(warning.subjects[0].class, pg_snapshot::FwClass::MoForm);
    assert_eq!(warning.subjects[0].guid.as_deref(), Some("allo-stem"));
    assert_eq!(warning.subjects[0].name.as_deref(), Some("xyz"));
    assert_eq!(
        warning_guidance(warning).as_deref(),
        Some(
            format!(
                "In {}, check the spelling and phonological environments for allomorph 'xyz'.",
                pg_snapshot::fieldworks_paths::LEXICON_EDIT
            )
            .as_str()
        )
    );
}

#[test]
fn a_disabled_compound_rule_is_considered_but_not_selected_with_no_issue() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .morphology
        .compound_rules
        .push(CompoundRule::Endocentric {
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
    assert!(
        !inventory.selected.contains(&key),
        "a disabled rule must never be selected"
    );
    assert!(
        !inventory.rejected.contains(&key),
        "a disabled rule is not a rejection"
    );
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

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
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
    assert!(
        issues
            .iter()
            .any(|i| i.code == super::issue_codes::ENVIRONMENT_UNRESOLVED && !i.fatal),
        "expected a non-fatal ENVIRONMENT_UNRESOLVED issue; got {issues:?}"
    );
}

#[test]
fn circumfix_cross_product_expansion_is_synthesized_and_represented() {
    let (snapshot, _f) = circumfix_snapshot(&[], &[]);
    let (_grammar, warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    assert!(warnings.iter().all(|warning| {
        warning.code != super::issue_codes::CIRCUMFIX_ENVIRONMENT_COMBINATION_SKIPPED.wire()
            && warning.code != super::issue_codes::CIRCUMFIX_MISSING_HALF.wire()
    }));

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
    let (_grammar2, _warnings2, inventory2, _issues2) =
        compile_recording_ok(&snapshot_with_authored);
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
    assert!(warnings
        .iter()
        .any(|warning| { warning.code == super::issue_codes::STRATA_CUSTOM_UNSUPPORTED.wire() }));
    let key = InventoryKey::setting(InventoryKind::StrataConfiguration, "Strata");
    assert!(inventory.rejected.contains(&key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::STRATA_CUSTOM_UNSUPPORTED && !i.fatal));
}

/// C# `HCLoader.LoadUnclassifiedAffixProcessRule` (HCLoader.cs:1013) always sets `IsPartial = true`.
#[test]
fn unclassified_affix_is_partial_like_hcloader() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-unclassified".to_string(),
        citation_form: vec![ws("sen", "ta")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph(
            "allo-unclassified",
            MorphType::Suffix,
            "ta",
        )],
        msas: vec![Msa::Unclassified {
            guid: "msa-unclassified".to_string(),
            part_of_speech: Some(f.noun_pos.clone()),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });

    let (grammar, _warnings, _inventory, _issues) = compile_recording_ok(&snapshot);

    let unclassified = grammar
        .mrules
        .iter()
        .filter_map(|rule| match rule {
            MorphRuleDef::AffixProcess(def) => Some(def),
            _ => None,
        })
        .filter(|def| def.partial_reason == Some(PartialMorphemeReason::UnclassifiedAffix))
        .count();
    assert_eq!(
        unclassified, 1,
        "the unclassified affix must compile as partial"
    );
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
        warnings.iter().any(|warning| {
            warning.code == super::issue_codes::ALLOMORPH_REDUPLICATION_UNSUPPORTED.wire()
                && warning
                    .subjects
                    .iter()
                    .any(|subject| subject.guid.as_deref() == Some("allo-bracket-form"))
        }),
        "expected a structured reduplication warning; got {warnings:?}"
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

    let bracket_key =
        InventoryKey::object(InventoryKind::Allomorph, "allo-bracket-form".to_string());
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
    assert!(
        inventory.selected.contains(&key),
        "must be selected before rejection"
    );
    assert!(inventory.rejected.contains(&key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::ALLOMORPH_MORPH_TYPE_UNSUPPORTED_AS_RULE_FORM));
}

/// `build_phon_features`'s complex-feature drop must select the feature before rejecting it (`rejected ⊆ selected`).
#[test]
fn complex_phonological_feature_is_recorded_selected_before_rejected() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .feature_systems
        .phonological
        .complex_features
        .push(ComplexFeature {
            guid: "cf-phon".to_string(),
            name: "PhonComplex".to_string(),
            abbreviation: "pc".to_string(),
            feature_type: None,
        });

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings.iter().any(|warning| {
            warning.code == super::issue_codes::PHON_COMPLEX_FEATURE_UNSUPPORTED.wire()
        }),
        "expected a named complex-feature warning; got {warnings:?}"
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

#[test]
fn complex_phonological_feature_warning_is_registered_and_names_the_feature() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .feature_systems
        .phonological
        .complex_features
        .push(ComplexFeature {
            guid: "cf-phon".to_string(),
            name: "PhonComplex".to_string(),
            abbreviation: "pc".to_string(),
            feature_type: None,
        });

    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");

    assert!(out
        .issues
        .iter()
        .all(|issue| !matches!(&issue.code, pg_snapshot::ImportWarningCode::Unregistered(_))));
    let warning = out
        .warnings
        .iter()
        .find(|warning| warning.code == super::issue_codes::PHON_COMPLEX_FEATURE_UNSUPPORTED.wire())
        .expect("complex feature warning");
    assert_eq!(
        warning.subjects[0].class,
        pg_snapshot::FwClass::FsComplexFeature
    );
    assert_eq!(warning.subjects[0].name.as_deref(), Some("PhonComplex"));
    assert!(warning_guidance(warning).is_some());
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
    assert!(
        warnings.is_empty(),
        "compaction revocation is silent: {warnings:?}"
    );

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

/// An affix (not root) allomorph whose literal text cannot be segmented is a recall gap for that one allomorph, matching the root case pinned elsewhere.
#[test]
fn affix_allomorph_unsegmentable_text_is_a_recall_gap_not_a_project_refusal() {
    let (mut snapshot, _f) = fixture();
    snapshot.lexicon.entries[1].allomorphs[0].forms = vec![ws("sen", "qa")]; // "q" is not declared anywhere in this fixture's phonology

    let out = compile_project_with(&snapshot, CompileOptions::default())
        .expect("one unrepresentable affix allomorph must not refuse the whole project");
    assert!(out
        .issues
        .iter()
        .any(|i| i.code == super::issue_codes::ALLOMORPH_UNSEGMENTABLE && !i.fatal));
    assert!(
        out.grammar.mrules.iter().all(|r| match r {
            MorphRuleDef::AffixProcess(d) => d.allomorphs.is_empty(),
            _ => true,
        }) || out.grammar.mrules.is_empty(),
        "the affix rule must end up with zero allomorphs (and be compacted away as unreachable)"
    );
}

/// A `Segments`-kind natural class referencing a phoneme guid that never resolves is a non-fatal, per-class recall gap.
#[test]
fn natclass_segments_member_unresolved_is_non_fatal() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-bad".to_string(),
            name: "Bad".to_string(),
            phonemes: vec!["ph-does-not-exist".to_string()],
        });

    let out =
        compile_project_with(&snapshot, CompileOptions::default()).expect("must still compile");
    assert!(
        out.issues
            .iter()
            .any(|i| i.code == super::issue_codes::NATCLASS_SEGMENTS_MEMBER_UNRESOLVED && !i.fatal),
        "expected a non-fatal NATCLASS_SEGMENTS_MEMBER_UNRESOLVED issue; got {:?}",
        out.issues
    );
}

/// A compound rule side whose part-of-speech guid does not resolve is a non-fatal drop of that one attribution, not a project refusal.
#[test]
fn compound_rule_side_pos_unresolved_is_non_fatal() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.no_default_compounding = true;
    snapshot
        .morphology
        .compound_rules
        .push(CompoundRule::Endocentric {
            guid: "crule-bad-pos".to_string(),
            name: "bad".to_string(),
            disabled: false,
            head_last: true,
            left: CompoundConstituentRequirement {
                part_of_speech: Some("pos-does-not-exist".to_string()),
                exception_features: Vec::new(),
            },
            right: CompoundConstituentRequirement::default(),
            overriding: CompoundOutcome::default(),
        });

    let out =
        compile_project_with(&snapshot, CompileOptions::default()).expect("must still compile");
    assert!(
        out.issues
            .iter()
            .any(|i| i.code == super::issue_codes::COMPOUND_SIDE_POS_UNRESOLVED && !i.fatal),
        "expected a non-fatal COMPOUND_SIDE_POS_UNRESOLVED issue; got {:?}",
        out.issues
    );
}

/// A phonological rewrite rule whose right-hand side is malformed fails to build, non-fatally: the rule is dropped, not the project.
#[test]
fn phonological_rule_build_failure_is_non_fatal() {
    let (mut snapshot, _f) = fixture();
    snapshot.phonology.rules.push(PhonologicalRule::Rewrite(
        pg_snapshot::phonology::RewriteRule {
            guid: "prule-bad".to_string(),
            name: "bad".to_string(),
            direction: RuleDirection::LeftToRight,
            structural_description: vec![PhonContext::Segment {
                phoneme: "ph-does-not-exist".to_string(),
            }],
            feature_constraint_variables: Vec::new(),
            right_hand_sides: Vec::new(),
        },
    ));

    let out =
        compile_project_with(&snapshot, CompileOptions::default()).expect("must still compile");
    assert!(
        out.issues
            .iter()
            .any(|i| i.code == super::issue_codes::RULE_BUILD_FAILED && !i.fatal),
        "expected a non-fatal RULE_BUILD_FAILED issue; got {:?}",
        out.issues
    );
    assert!(
        out.grammar.prules.is_empty(),
        "the malformed rule itself must not appear in the grammar"
    );
}

/// A co-occurrence prohibition whose PRIMARY is affix-owned and whose owning mrule reachability compaction later prunes as dead code (never referenced by any template slot) must not refuse the project: it would have been dropped regardless, so refusing it here is a false positive over code that was never going to survive anyway.
#[test]
fn cooccurrence_refusal_on_a_primary_whose_own_mrule_is_pruned_by_reachability_is_non_fatal() {
    let (mut snapshot, _f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-orphan".to_string(),
        citation_form: vec![ws("sen", "-ka")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph("allo-orphan", MorphType::Suffix, "ka")],
        msas: vec![Msa::Inflectional {
            guid: "msa-orphan".to_string(),
            part_of_speech: Some(_f.noun_pos.clone()),
            slots: vec!["slot-never-templated".to_string()],
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    });
    snapshot
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Allomorph {
            guid: "coocc-on-dead-code".to_string(),
            disabled: false,
            primary: "allo-orphan".to_string(),
            others: vec!["allo-does-not-exist".to_string()],
            adjacency: Adjacency::Anywhere,
        });

    let out = compile_project_with(&snapshot, CompileOptions::default()).expect(
        "a co-occurrence refusal over a primary that reachability prunes as dead code must not \
         refuse the whole project",
    );
    assert!(out
        .inventory
        .issues
        .iter()
        .any(|i| i.code == pg_snapshot::ImportWarningCode::AdhocProhibitionUnresolved && !i.fatal));
}

/// Paired control for the test above, same owner code path: a primary that STAYS reachable (the fixture's own template-filling suffix) still refuses over the identical dangling-others shape -- the deferral in `resolve_pending_cooccurrence_refusals` only changes the dead-code case, never the live one.
#[test]
fn cooccurrence_refusal_on_a_reachable_affix_owned_primary_still_refuses() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Allomorph {
            guid: "coocc-on-live-code".to_string(),
            disabled: false,
            primary: "allo-suffix".to_string(),
            others: vec!["allo-does-not-exist".to_string()],
            adjacency: Adjacency::Anywhere,
        });

    let err = compile_project_with(&snapshot, CompileOptions::default()).expect_err(
        "a co-occurrence refusal over a primary that survives reachability compaction must still refuse",
    );
    assert!(err
        .issues()
        .iter()
        .any(|i| i.code == pg_snapshot::ImportWarningCode::AdhocProhibitionUnresolved && i.fatal));
}

/// An unnamed, unreferenced, non-last natural class is revoked; a referenced one and `__any__` stay represented.
#[test]
fn unreferenced_unnamed_natural_class_is_revoked_but_referenced_and_any_survive() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-orphan".to_string(),
            name: String::new(),
            phonemes: vec!["ph-a".to_string()],
        });
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-last-unnamed".to_string(),
            name: String::new(),
            phonemes: vec!["ph-i".to_string()],
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
            guid: "env-v".to_string(),
            name: String::new(),
            representation: "/_[V]".to_string(),
        });
    snapshot.lexicon.entries[1].allomorphs[0]
        .environments
        .push("env-v".to_string());

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings.is_empty(),
        "compaction revocation is silent: {warnings:?}"
    );

    let orphan_key = InventoryKey::object(InventoryKind::NaturalClass, "nc-orphan".to_string());
    assert!(inventory.rejected.contains(&orphan_key));
    assert!(!inventory.represented.contains(&orphan_key));
    let compacted_issue = issues
        .iter()
        .find(|i| i.code == super::issue_codes::NATURAL_CLASS_UNREFERENCED_COMPACTED)
        .expect("compaction issue");
    assert_eq!(
        compacted_issue.source.as_ref().map(|source| source.kind),
        Some(pg_snapshot::FwClass::PhNaturalClass)
    );
    assert_eq!(
        compacted_issue
            .source
            .as_ref()
            .map(|source| source.id.as_str()),
        Some("nc-orphan")
    );
    let warning = super::warnings::from_issues(&snapshot, &[compacted_issue.clone()]);
    assert_eq!(
        warning[0].subjects[0].name.as_deref(),
        Some("Unnamed natural class")
    );
    assert!(!grammar
        .natural_classes
        .iter()
        .any(|d| d.xml_id == "nc-orphan"));

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
    snapshot
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Morpheme {
            guid: "coocc-dropped".to_string(),
            disabled: false,
            primary: f.stem_msa.clone(),
            others: vec!["msa-orphan".to_string()],
            adjacency: Adjacency::Anywhere,
        });
    snapshot
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Morpheme {
            guid: "coocc-survives".to_string(),
            disabled: false,
            primary: f.stem_msa.clone(),
            others: vec![f.suffix_msa.clone()],
            adjacency: Adjacency::Anywhere,
        });

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings.is_empty(),
        "compaction revocation is silent: {warnings:?}"
    );

    let dropped_key = InventoryKey::object(
        InventoryKind::MorphemeCoOccurrence,
        "coocc-dropped".to_string(),
    );
    assert!(inventory.rejected.contains(&dropped_key));
    assert!(!inventory.represented.contains(&dropped_key));
    assert!(issues
        .iter()
        .any(|i| i.code == super::issue_codes::COOCCURRENCE_TARGET_UNREACHABLE));

    let survives_key = InventoryKey::object(
        InventoryKind::MorphemeCoOccurrence,
        "coocc-survives".to_string(),
    );
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
    let grammar_msas: std::collections::BTreeSet<String> = grammar
        .morphemes
        .iter()
        .map(|m| m.xml_key.clone())
        .collect();
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
    let (snapshot, _) = fixture();
    let mut recorder = pg_snapshot::SelectionRecorder::default();
    let lineage = super::inventory::Lineage::default();
    super::inventory::finalize(
        &snapshot,
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
    let (snapshot, _) = fixture();
    let mut recorder = pg_snapshot::SelectionRecorder::default();
    let lineage = super::inventory::Lineage::default();
    super::inventory::finalize(
        &snapshot,
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
    let (snapshot, _) = fixture();
    let mut recorder = pg_snapshot::SelectionRecorder::default();
    let lineage = super::inventory::Lineage::default();
    super::inventory::finalize(
        &snapshot,
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
    let (snapshot, _) = fixture();
    let mut recorder = pg_snapshot::SelectionRecorder::default();
    let lineage = super::inventory::Lineage::default();
    super::inventory::finalize(
        &snapshot,
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
    snapshot
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Allomorph {
            guid: "coocc-owner-orphaned".to_string(),
            disabled: false,
            primary: "allo-orphan".to_string(),
            others: vec!["allo-stem".to_string()],
            adjacency: Adjacency::Anywhere,
        });

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings.is_empty(),
        "compaction revocation is silent: {warnings:?}"
    );

    let key = InventoryKey::object(
        InventoryKind::AllomorphCoOccurrence,
        "coocc-owner-orphaned".to_string(),
    );
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

/// An allomorph co-occurrence rule whose only `others` target is compacted away is revoked and reported through the typed issue.
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
    snapshot
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Allomorph {
            guid: "coocc-target-orphaned".to_string(),
            disabled: false,
            primary: "allo-suffix".to_string(),
            others: vec!["allo-orphan".to_string()],
            adjacency: Adjacency::Anywhere,
        });

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings.is_empty(),
        "developer compaction issues are typed: {warnings:?}"
    );

    let key = InventoryKey::object(
        InventoryKind::AllomorphCoOccurrence,
        "coocc-target-orphaned".to_string(),
    );
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
    snapshot
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Allomorph {
            guid: "coocc-survives".to_string(),
            disabled: false,
            primary: "allo-suffix".to_string(),
            others: vec!["allo-stem".to_string()],
            adjacency: Adjacency::Anywhere,
        });

    let (grammar, warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    let key = InventoryKey::object(
        InventoryKind::AllomorphCoOccurrence,
        "coocc-survives".to_string(),
    );
    assert!(inventory.represented.contains(&key));

    let found = grammar.mrules.iter().any(|r| match r {
        MorphRuleDef::AffixProcess(d) => d.allomorphs.iter().any(|a| !a.co_occurrence.is_empty()),
        _ => false,
    });
    assert!(
        found,
        "the surviving co-occurrence rule must remain on its owner's allomorph"
    );
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
    snapshot
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Allomorph {
            guid: "coocc-idx0-survives".to_string(),
            disabled: false,
            primary: "allo-suffix".to_string(),
            others: vec!["allo-stem".to_string()],
            adjacency: Adjacency::Anywhere,
        });
    // idx 1 on the SAME owner "allo-suffix": targets "allo-orphan", which is compacted away -- must be revoked.
    snapshot
        .morphology
        .adhoc_prohibitions
        .push(AdhocProhibition::Allomorph {
            guid: "coocc-idx1-revoked".to_string(),
            disabled: false,
            primary: "allo-suffix".to_string(),
            others: vec!["allo-orphan".to_string()],
            adjacency: Adjacency::Anywhere,
        });

    let (grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    assert!(
        warnings.is_empty(),
        "developer compaction issues are typed: {warnings:?}"
    );

    let survives = InventoryKey::object(
        InventoryKind::AllomorphCoOccurrence,
        "coocc-idx0-survives".to_string(),
    );
    let revoked = InventoryKey::object(
        InventoryKind::AllomorphCoOccurrence,
        "coocc-idx1-revoked".to_string(),
    );
    assert!(
        inventory.represented.contains(&survives),
        "idx 0's rule must stay represented"
    );
    assert!(
        !inventory.rejected.contains(&survives),
        "idx 0's rule must not be revoked"
    );
    assert!(
        inventory.rejected.contains(&revoked),
        "idx 1's rule must be revoked"
    );
    assert!(
        !inventory.represented.contains(&revoked),
        "idx 1's rule must not stay represented"
    );
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

// --- typed compile options/issues ---------------------------------------------------------------

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
        warnings,
        substrate: _substrate,
        inventory,
    } = out;
    assert_eq!(grammar.entries.len(), 1);
    assert!(issues.is_empty());
    assert!(warnings.is_empty());
    assert!(inventory.inventory.rejected.is_empty());
}

/// `compile_project_with` under default options must match `compile_project` message-for-message.
#[test]
fn compile_project_with_default_options_matches_compile_project() {
    let (snapshot, _f) = fixture();
    let (grammar_tuple, warnings_tuple) = compile_project(&snapshot).expect("must compile");
    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    assert_eq!(out.warnings, warnings_tuple);
    assert_grammars_equal(&grammar_tuple, &out.grammar);
}

/// Every compile-stage warning arrives as a non-fatal issue, on a snapshot that actually warns.
#[test]
fn every_compile_stage_warning_becomes_a_non_fatal_conversion_issue() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.strata = Some("Morphology,(Clitics)".to_string());
    let (_grammar, warnings) = compile_project(&snapshot).expect("must compile");
    assert!(
        warnings.iter().any(|warning| {
            warning.code == super::issue_codes::STRATA_CUSTOM_UNSUPPORTED.wire()
        }),
        "fixture must still produce the legacy Strata warning; got {warnings:?}"
    );

    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    assert_eq!(out.issues.len(), warnings.len());
    for issue in &out.issues {
        assert!(
            !issue.fatal,
            "adapted compile-stage issue must be non-fatal: {issue:?}"
        );
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
    snapshot
        .conversion_provenance
        .import_issues
        .push(ConversionIssue {
            code: pg_snapshot::ImportWarningCode::FwdataDanglingReference,
            class: IssueClass::InvalidSource,
            source: None,
            fatal: true,
            message: "A referenced FieldWorks object does not exist.".to_string(),
        });

    let err = compile_project_with(&snapshot, CompileOptions::default())
        .expect_err("a fatal imported issue must refuse under Refuse");
    assert!(matches!(err, GrammarError::Conversion(_)));
    assert!(err
        .issues()
        .iter()
        .any(|i| { i.code == pg_snapshot::ImportWarningCode::FwdataDanglingReference }));

    let (_, warnings, _) = compile_project_measured(&snapshot)
        .expect("inventory measurement must preserve the fatal issue without refusing");
    assert!(warnings.iter().any(|warning| {
        warning.code == pg_snapshot::ImportWarningCode::FwdataDanglingReference.wire()
            && warning_metadata(warning).audience == pg_snapshot::Audience::Linguist
    }));

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
        .any(|i| { i.code == pg_snapshot::ImportWarningCode::FwdataDanglingReference && i.fatal }));
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
        .any(|i| i.code == pg_snapshot::ImportWarningCode::SourceProvenanceUnknown && i.fatal));

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
        .any(|i| i.code == pg_snapshot::ImportWarningCode::SourceProvenanceUnknown && i.fatal));
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

// --- substrate completion from owner-published usage -------------------------------------------

#[test]
fn xample_authored_project_infers_missing_exemplar_segment() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];

    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("lossless compile");
    assert_eq!(out.substrate.inferred_segments.len(), 1);
    assert_eq!(out.substrate.inferred_segments[0].representation, "q");
    assert_eq!(out.grammar.entries[0].allomorphs.len(), 1);
    assert!(out.grammar.char_tables[0].lookup_nfd("q").is_some());
}

/// A single unsegmentable allomorph is a recall gap for that one entry, not a meaning change to the rest of the grammar -- see `substrate`'s module doc.
#[test]
fn strict_hc_project_drops_only_the_allomorph_with_the_missing_segment() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];

    let out = compile_project_with(
        &snapshot,
        CompileOptions {
            substrate: SubstratePolicy::Strict,
            ..CompileOptions::default()
        },
    )
    .expect("a single unrepresentable allomorph must not refuse the whole project");
    assert!(out.issues.iter().any(|i| {
        i.code == pg_snapshot::ImportWarningCode::SubstrateUnsegmentableForm
            && !i.fatal
            && i.source.as_ref().is_some_and(|s| s.id == "allo-stem")
    }));
    assert_eq!(
        out.grammar.entries.len(),
        0,
        "the fixture's only entry (the stem) has zero loadable allomorphs and is dropped -- the suffix, which never used this text, is unaffected (it just carries no LexEntryDef of its own)"
    );
}

/// As the segment-decl case above, but for a genuinely ambiguous character: still a recall gap, not a whole-project refusal.
#[test]
fn ambiguous_symbol_without_ldml_drops_only_that_allomorph() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "ku§ma")];

    let out = compile_project_with(&snapshot, CompileOptions::default())
        .expect("symbol role is not authoritative without LDML, but that drops one allomorph, not the project");
    assert!(out.issues.iter().any(|i| i.code
        == pg_snapshot::ImportWarningCode::SubstrateClassificationAmbiguous
        && !i.fatal));
    assert_eq!(
        out.grammar.entries.len(),
        0,
        "the fixture's only entry (the stem) is dropped"
    );
}

/// Pins the claim `substrate`'s module doc makes (rather than leaving it an unlinked prose claim): a substrate-unresolved literal and the real owner's independent segmentation failure land on the SAME allomorph, both non-fatal -- refusing at the substrate layer would duplicate, not add to, the owner's own decision.
#[test]
fn substrate_issue_and_the_real_owners_drop_agree_on_the_same_allomorph() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];

    let out = compile_project_with(
        &snapshot,
        CompileOptions {
            substrate: SubstratePolicy::Strict,
            ..CompileOptions::default()
        },
    )
    .expect("must compile");
    let substrate_hit = out.issues.iter().any(|i| {
        i.code == pg_snapshot::ImportWarningCode::SubstrateUnsegmentableForm
            && !i.fatal
            && i.source.as_ref().is_some_and(|s| s.id == "allo-stem")
    });
    let owner_key = InventoryKey::object(InventoryKind::Allomorph, "allo-stem".to_string());
    let owner_hit =
        out.inventory.inventory.rejected.contains(&owner_key)
            && out.inventory.issues.iter().any(|i| {
                i.code == pg_snapshot::ImportWarningCode::AllomorphUnsegmentable && !i.fatal
            });
    assert!(
        substrate_hit && owner_hit,
        "expected both the substrate issue and the owner's own drop on allo-stem; top-level={:?} inventory={:?}",
        out.issues,
        out.inventory.issues
    );
}

/// Regression pin for a probe/builder segmenter mismatch: `substrate::complete`'s probe used to consult `segment_phonemes_only` (built for environment-string validation, which deliberately SKIPS Boundary-kind char defs), while the real owner (`lexicon::build_root_allomorph`) uses `segment_with_patterns`, whose literal-match loop accepts Segment AND Boundary. A literal authored boundary marker inside an ordinary root form used to misfire a false `substrate.position-unmapped`; the probe now shares `segment` (both kinds, no patterns) with the owners.
#[test]
fn a_literal_authored_boundary_marker_inside_a_root_form_is_not_a_false_substrate_refusal() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "ku+ma")];

    let out = compile_project_with(
        &snapshot,
        CompileOptions {
            substrate: SubstratePolicy::Strict,
            ..CompileOptions::default()
        },
    )
    .expect("a literal authored boundary marker must segment, not misfire a substrate refusal");
    assert!(
        out.issues.iter().all(|i| i.code
            != pg_snapshot::ImportWarningCode::SubstratePositionUnmapped
            && i.code != pg_snapshot::ImportWarningCode::SubstrateUnsegmentableForm),
        "expected no substrate issue at all; got {:?}",
        out.issues
    );
    assert_eq!(
        out.grammar.entries.len(),
        1,
        "the stem allomorph must be fully represented, not dropped over a false positive"
    );
}

#[test]
fn accept_unspecified_graphemes_changes_the_effect_not_just_the_message() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot
        .morphology
        .parser_parameters
        .accept_unspecified_graphemes = true;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];

    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("flag must act");
    assert_eq!(out.grammar.entries[0].allomorphs.len(), 1);
    assert!(out.grammar.char_tables[0].lookup_nfd("q").is_some());
}

/// A closed feature, a `Feature`-kind natural class over it, and a rewrite rule referencing that class; returns the feature's guid.
fn add_feature_based_rule_that_can_match_unspecified_q(snapshot: &mut Snapshot) -> String {
    let feature_guid = "feat-frontness".to_string();
    let front_guid = "val-front".to_string();
    let back_guid = "val-back".to_string();
    snapshot
        .feature_systems
        .phonological
        .closed_features
        .push(ClosedFeature {
            guid: feature_guid.clone(),
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
    let nc_guid = "nc-front".to_string();
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Features {
            guid: nc_guid.clone(),
            name: "Front".to_string(),
            features: FeatureStructure {
                values: vec![FeatureValue {
                    feature: feature_guid.clone(),
                    value: FeatureValueKind::Closed { value: front_guid },
                }],
            },
        });
    let class_context = || pg_snapshot::phonology::PhonContext::NaturalClass {
        natural_class: nc_guid.clone(),
        plus_variables: Vec::new(),
        minus_variables: Vec::new(),
    };
    snapshot.phonology.rules.push(PhonologicalRule::Rewrite(
        pg_snapshot::phonology::RewriteRule {
            guid: "prule-front-raise".to_string(),
            name: "front-raise".to_string(),
            direction: RuleDirection::LeftToRight,
            structural_description: vec![class_context()],
            feature_constraint_variables: Vec::new(),
            right_hand_sides: vec![pg_snapshot::phonology::RewriteRhs {
                structural_change: vec![class_context()],
                ..pg_snapshot::phonology::RewriteRhs::default()
            }],
        },
    ));
    feature_guid
}

/// The ordinary HC "featureless segment" shape an inferred segment's `RawCharDef` must compile identically to.
fn add_explicit_featureless_segment(snapshot: &mut Snapshot, rep: &str) {
    snapshot.phonology.phonemes.push(Phoneme {
        guid: format!("ph-explicit-{rep}"),
        name: rep.to_string(),
        representations: vec![ws("sen", rep)],
        features: None,
        basic_ipa_symbol: None,
    });
}

/// Pinned to the class's non-matching value (`Frontness = back`), so its lanes must differ from a featureless segment's.
fn add_explicit_feature_valued_segment(snapshot: &mut Snapshot, rep: &str, feature_guid: &str) {
    snapshot.phonology.phonemes.push(Phoneme {
        guid: format!("ph-valued-{rep}"),
        name: rep.to_string(),
        representations: vec![ws("sen", rep)],
        features: Some(FeatureStructure {
            values: vec![FeatureValue {
                feature: feature_guid.to_string(),
                value: FeatureValueKind::Closed {
                    value: "val-back".to_string(),
                },
            }],
        }),
        basic_ipa_symbol: None,
    });
}

/// Checks the fact `pg-grammar` itself owns -- compiled `feature_lanes` -- rather than running a parser or FST engine, since both live in crates that depend on `pg-grammar` itself.
#[test]
fn inferred_segment_uses_the_same_semantics_as_an_authored_featureless_segment() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];
    let feature_guid = add_feature_based_rule_that_can_match_unspecified_q(&mut snapshot);

    let inferred = compile_project_with(&snapshot, CompileOptions::default())
        .expect("ordinary HC unspecified-feature semantics is defined");
    assert!(
        inferred
            .issues
            .iter()
            .any(|i| i.code
                == pg_snapshot::ImportWarningCode::MigrationInferredSegmentWithFeatureRule)
    );

    let q_id = inferred.grammar.char_tables[0]
        .lookup_nfd("q")
        .expect("q must be in the compiled table");
    let inferred_lanes = inferred.grammar.char_tables[0]
        .get(q_id)
        .feature_lanes()
        .to_vec();

    let mut explicit_snapshot = snapshot.clone();
    add_explicit_featureless_segment(&mut explicit_snapshot, "q");
    let explicit = compile_project_with(&explicit_snapshot, CompileOptions::default()).unwrap();
    let explicit_id = explicit.grammar.char_tables[0].lookup_nfd("q").unwrap();
    let explicit_lanes = explicit.grammar.char_tables[0]
        .get(explicit_id)
        .feature_lanes();
    assert_eq!(
        inferred_lanes, explicit_lanes,
        "an inferred segment must carry the exact same unspecified-feature semantics as an \
         authored featureless one"
    );

    let mut valued_snapshot = snapshot;
    add_explicit_feature_valued_segment(&mut valued_snapshot, "q", &feature_guid);
    let valued = compile_project_with(&valued_snapshot, CompileOptions::default()).unwrap();
    let valued_id = valued.grammar.char_tables[0].lookup_nfd("q").unwrap();
    let valued_lanes = valued.grammar.char_tables[0].get(valued_id).feature_lanes();
    assert_ne!(
        inferred_lanes, valued_lanes,
        "an explicitly feature-valued segment must NOT share the inferred segment's wildcard lanes"
    );
}

#[test]
fn import_warning_migration_names_the_inferred_phoneme_and_has_guidance() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];
    add_feature_based_rule_that_can_match_unspecified_q(&mut snapshot);

    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("the inferred segment remains representable");
    let warning = output
        .warnings
        .iter()
        .find(|warning| warning.code == "migration.inferred-segment-with-feature-rule")
        .expect("the migration difference is reported");
    let finding = crate::grammar_health::GrammarHealthCheckFinding::from_import_warning(warning);

    assert!(finding.message.contains("'q'"), "{}", finding.message);
    assert!(
        finding.message.contains("not a project phoneme"),
        "{}",
        finding.message
    );
    assert!(
        !finding.message.contains("inferred segment"),
        "{}",
        finding.message
    );
    assert_eq!(finding.subjects.len(), 1);
    assert_eq!(finding.subjects[0].kind, pg_snapshot::FwClass::PhPhoneme);
    assert_eq!(finding.subjects[0].title, "q");
    assert!(finding.subjects[0].guid.is_none());
    assert!(finding.guidance.as_deref().is_some_and(|guidance| {
        guidance.contains(pg_snapshot::fieldworks_paths::GRAMMAR_PHONEMES)
    }));
}

#[test]
fn phoneme_collision_warning_names_the_other_phoneme() {
    let (mut snapshot, _) = fixture();
    snapshot.phonology.phonemes[0].name = "first phoneme".to_string();
    snapshot.phonology.phonemes.push(Phoneme {
        guid: "ph-k-duplicate".to_string(),
        name: "second phoneme".to_string(),
        representations: vec![ws("sen", "k")],
        features: None,
        basic_ipa_symbol: None,
    });

    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    let warning = output
        .warnings
        .iter()
        .find(|warning| warning.code == pg_snapshot::ImportWarningCode::PhonemeNfdCollision.wire())
        .expect("the duplicate phoneme is reported");

    assert!(warning.message.contains("second phoneme"), "{warning:?}");
    assert!(warning.message.contains("first phoneme"), "{warning:?}");
}

#[test]
fn empty_stem_bucket_is_recorded_without_a_linguist_warning() {
    let (mut snapshot, _) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "?")];
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;

    let output = compile_project_with(
        &snapshot,
        CompileOptions {
            substrate: SubstratePolicy::Strict,
            ..CompileOptions::default()
        },
    )
    .expect("an entry with no loadable allomorph is dropped with a warning");
    assert!(output
        .warnings
        .iter()
        .all(|warning| warning.code != super::issue_codes::MSA_NO_ALLOMORPHS.wire()));
    assert!(output
        .issues
        .iter()
        .any(|issue| issue.code == super::issue_codes::MSA_NO_ALLOMORPHS));
}

#[test]
fn empty_affix_form_is_recorded_without_a_linguist_warning() {
    let (mut snapshot, _) = fixture();
    snapshot.lexicon.entries[1].allomorphs[0].forms.clear();

    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("an empty affix form is dropped with a warning");
    assert!(output
        .warnings
        .iter()
        .all(|warning| warning.code != super::issue_codes::ALLOMORPH_NOT_RULE_FORM.wire()));
    assert!(output
        .issues
        .iter()
        .any(|issue| issue.code == super::issue_codes::ALLOMORPH_NOT_RULE_FORM));
}

/// A root form carrying `[C]`-style pattern syntax must compile exactly as it does without substrate completion -- `[`/`]` must never be checked as an undeclared literal.
#[test]
fn pattern_bearing_root_form_is_not_treated_as_an_undeclared_literal() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-vowel".to_string(),
            name: "V".to_string(),
            phonemes: vec!["ph-a".to_string(), "ph-i".to_string(), "ph-u".to_string()],
        });
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "k[V]t")];

    let out = compile_project_with(&snapshot, CompileOptions::default())
        .expect("a pattern-bearing root form must still compile");
    assert!(out.substrate.ambiguous_uses.is_empty());
    assert!(out.substrate.inferred_segments.is_empty());
    assert!(out.substrate.inferred_boundaries.is_empty());
    assert_eq!(out.grammar.entries[0].allomorphs.len(), 1);
}

// --- affix-form substrate collection (review R1/R3) ---------------------------------------------

/// The blocking review repro: a SUFFIX allomorph's form (not the stem) carries the missing exemplar.
#[test]
fn xample_authored_project_infers_a_missing_exemplar_segment_from_a_suffix_form() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot.lexicon.entries[1].allomorphs[0].forms = vec![ws("sen", "qta")];

    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("lossless compile");
    assert_eq!(out.substrate.inferred_segments.len(), 1);
    assert_eq!(out.substrate.inferred_segments[0].representation, "q");
    assert!(out.grammar.char_tables[0].lookup_nfd("q").is_some());
    let affix_rules: Vec<_> = out
        .grammar
        .mrules
        .iter()
        .filter_map(|r| match r {
            MorphRuleDef::AffixProcess(d) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(
        affix_rules.len(),
        1,
        "the suffix rule must not have vanished into a warning"
    );
    assert_eq!(affix_rules[0].allomorphs.len(), 1);
}

/// A bracket-pattern/reduplication affix form must compile exactly as it did without substrate completion, and now also publish `conversion.unsupported-construct`.
#[test]
fn bracket_pattern_affix_form_still_compiles_unaffected_and_publishes_unsupported_construct() {
    let (mut snapshot, f) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.lexicon.entries[1].allomorphs[0].forms = vec![ws("sen", "[X]")];

    let out =
        compile_project_with(&snapshot, CompileOptions::default()).expect("must still compile");
    assert!(
        out.issues
            .iter()
            .any(|i| i.message.contains("reduplication/bracket-pattern") && !i.fatal),
        "the legacy reduplication warning must survive unchanged: {:?}",
        out.issues
    );
    assert!(
        out.issues
            .iter()
            .any(|i| i.code == pg_snapshot::ImportWarningCode::UnsupportedConstruct && !i.fatal),
        "expected a non-fatal conversion.unsupported-construct issue; got {:?}",
        out.issues
    );
    assert!(out.substrate.inferred_segments.is_empty());
    assert!(out.substrate.inferred_boundaries.is_empty());
    assert!(out.substrate.ambiguous_uses.is_empty());
    assert!(
        out.grammar.char_tables[0].lookup_nfd("X").is_none(),
        "bracket-pattern text must never be treated as a literal character"
    );
    let _ = f;
}

/// Every allomorph the compiler actually represents must have had its text published by one of the two collectors, or they have drifted.
#[test]
fn text_use_collection_covers_every_represented_allomorph() {
    let (snapshot, _f) = fixture();
    let mut recorder = pg_snapshot::SelectionRecorder::default();
    super::lexicon::collect_text_uses(&snapshot, &mut recorder);
    let mut collection_issues = Vec::new();
    super::affixes::collect_text_uses(&snapshot, &mut recorder, &mut collection_issues);
    assert!(collection_issues.is_empty());
    let collected: std::collections::BTreeSet<String> = recorder
        .text_uses()
        .iter()
        .map(|(source, _)| source.id.clone())
        .collect();

    let (_grammar, _warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    let represented_allomorphs: Vec<String> = inventory
        .represented
        .iter()
        .filter(|k| k.kind == InventoryKind::Allomorph)
        .filter_map(|k| match &k.identity {
            pg_snapshot::InventoryIdentity::Object { guid } => Some(guid.clone()),
            _ => None,
        })
        .collect();
    assert!(!represented_allomorphs.is_empty());
    let missing: Vec<_> = represented_allomorphs
        .iter()
        .filter(|guid| !collected.contains(*guid))
        .collect();
    assert!(
        missing.is_empty(),
        "represented allomorph(s) with no recorded text use: {missing:?}"
    );
}

// --- position-remap mismap regression (a real corpus went from compiling to refusing) -----------

/// Mid-word sub-case: the mismapped position lands on the NEXT, already-registered character -- the real-Sena-3 "b" duplicate-representation panic, reproduced synthetically.
#[test]
fn precomposed_diacritic_mid_word_never_reselects_the_next_already_registered_character() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.phonology.phonemes.push(phoneme("ph-b", "b"));
    snapshot.project.exemplar_characters.push("b".to_string());
    // "a"/"b" are registered, precomposed "\u{e1}" is not: greedy matching stalls on its own mark.
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "s\u{e1}b")];

    let out = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
            ..CompileOptions::default()
        },
    )
    .expect("must not panic; the mismap must be reported as an issue, never as a duplicate registration");
    assert!(
        out.issues.iter().any(|i| i.code == pg_snapshot::ImportWarningCode::SubstratePositionUnmapped && !i.fatal),
        "expected a non-fatal substrate.position-unmapped issue (a recall gap for one allomorph); got {:?}",
        out.issues
    );
    assert_eq!(
        out.substrate.inferred_segments.len(),
        0,
        "\"b\" must not be re-inferred"
    );
    assert_eq!(out.substrate.ambiguous_uses.len(), 1);

    let refused = compile_project_with(&snapshot, CompileOptions::default());
    assert!(
        refused.is_ok(),
        "production Refuse must accept this: one unmapped allomorph is a recall gap, not a meaning change"
    );
}

/// Word-final sub-case: the mismapped position lands PAST THE END of the word, with no "next" character to land on at all -- previously an unconditional panic in `failing_char` itself, not caught by any duplicate-registration guard.
#[test]
fn precomposed_diacritic_word_final_refuses_instead_of_panicking_past_the_end() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    // "s"/"a" are registered, "\u{e1}" is not, and the word ends right after it: no next character.
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "s\u{e1}")];

    let out = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
            ..CompileOptions::default()
        },
    )
    .expect("must not panic; a mismapped word-final position must be reported as an issue");
    assert!(
        out.issues.iter().any(|i| i.code == pg_snapshot::ImportWarningCode::SubstratePositionUnmapped && !i.fatal),
        "expected a non-fatal substrate.position-unmapped issue (a recall gap for one allomorph); got {:?}",
        out.issues
    );
    assert_eq!(out.substrate.inferred_segments.len(), 0);
    assert_eq!(out.substrate.ambiguous_uses.len(), 1);

    let refused = compile_project_with(&snapshot, CompileOptions::default());
    assert!(
        refused.is_ok(),
        "production Refuse must accept this: one unmapped allomorph is a recall gap, not a meaning change"
    );
}

/// The same word-final mismap under `Strict` -- `Strict`'s single pass and `CompleteFromUsage`'s loop share `position_mismap`, so both call sites must refuse, never panic.
#[test]
fn precomposed_diacritic_word_final_refuses_under_strict_too() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "s\u{e1}")];

    let out = compile_project_with(
        &snapshot,
        CompileOptions {
            substrate: SubstratePolicy::Strict,
            semantic_loss: SemanticLossPolicy::MeasureOnly,
        },
    )
    .expect("must not panic under Strict either");
    assert!(
        out.issues.iter().any(|i| i.code == pg_snapshot::ImportWarningCode::SubstratePositionUnmapped && !i.fatal),
        "expected a non-fatal substrate.position-unmapped issue (a recall gap for one allomorph); got {:?}",
        out.issues
    );

    let refused = compile_project_with(
        &snapshot,
        CompileOptions {
            substrate: SubstratePolicy::Strict,
            ..CompileOptions::default()
        },
    );
    assert!(
        refused.is_ok(),
        "production Refuse must accept this: one unmapped allomorph is a recall gap, not a meaning change"
    );
}

/// Target behavior, not current: no owner yet publishes environment-string text into substrate completion, so this stays `#[ignore]`d (visible) rather than silently absent, until one does.
#[test]
#[ignore = "environment-sourced substrate completion is not wired; see literal_text_elements"]
fn environment_only_undeclared_exemplar_is_completed_from_usage() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-q".to_string(),
            name: String::new(),
            representation: "/q_".to_string(),
        });
    snapshot.lexicon.entries[1].allomorphs[0]
        .environments
        .push("env-q".to_string());

    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    assert_eq!(
        out.substrate.inferred_segments.len(),
        1,
        "an exemplar character used only inside an environment must still be inferred"
    );
    assert_eq!(out.substrate.inferred_segments[0].representation, "q");
}
