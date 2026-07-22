//! Unit tests for `pg_grammar::compile`, built entirely from code-constructed `Snapshot` values
//! (no `.fwdata`/oracle files — those are T4's job). See `pg-snapshot/src/lib.rs`'s own tests for
//! the construction style this mirrors.

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

use crate::model::MorphRuleDef;

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

/// One POS ("Noun") with one affix slot and one template using it; a stem entry ("kuma", gloss
/// "dog") and an inflectional suffix entry ("-ta", gloss "PL") filling that slot. Every test below
/// starts from this and mutates the parts it cares about.
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

/// An allomorph's environment guid pointing at a string that doesn't even start with `/` (an
/// `IsValidEnvironment`-rejected string, HCLoader.cs:1205-1271) must not fail the whole compile —
/// it is a warning, and the allomorph still compiles with that one environment simply absent.
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

/// A well-formed environment (`[NC]` natural-class reference) parses into a real pattern and
/// gates the allomorph, without any warning.
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
    assert_eq!(grammar.mpr_features[class_bit].xml_id, class_guid);
    assert_eq!(grammar.mpr_features[class_bit].name, "DefaultClass");
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
    let variant_morpheme = grammar
        .morphemes
        .iter()
        .find(|m| m.gloss.as_deref() == Some("dog.IRR"))
        .expect("expected a morpheme with the prepend/append-combined gloss \"dog.IRR\"");
    let _ = variant_morpheme;
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
    // With no slots referencing it, the template's one slot has no loaded affix and the whole
    // template must be dropped (HCLoader.cs:297-300).
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

/// A circumfix entry is Phase B too (cross-product allomorphs, HCLoader.cs:1048-1332): it warns
/// and contributes no rule, rather than crashing the whole compile.
#[test]
fn circumfix_entry_is_unsupported_and_warns_rather_than_erroring() {
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
            slots: Vec::new(),
            features: None,
            exception_features: Vec::new(),
        }],
        senses: Vec::new(),
        entry_refs: Vec::new(),
    };
    snapshot.lexicon.entries.push(circumfix_entry);

    let (_grammar, warnings) =
        compile_project(&snapshot).expect("circumfix must not be a hard error");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("unsupported") && w.contains("circumfix")),
        "expected an 'unsupported: circumfix ...' warning; got {warnings:?}"
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

/// Walks every dense-id cross-reference in a compiled `Grammar` and panics on the first
/// inconsistency. Every test above only asserts *counts and shapes* (`entries.len() == 1`, etc.) —
/// none of them would catch an off-by-one in `allomorph_owners`, a `MorphemeId` pointing at the
/// wrong row, or an out-of-range `MRuleId` in a stratum/slot list, since a wrong index is just
/// another `usize` and every field involved still "has a value". This walks the full fixture grammar
/// (which — via `absent_compound_rules_synthesize_the_two_defaults_when_not_suppressed` — already
/// carries 2 synthesized compounding rules ahead of the 1 affix rule in `mrules`, exactly the kind of
/// index offset that would hide an off-by-one) and checks every link resolves to the record it claims
/// to.
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
    // Sanity: the fixture really does carry the 2 default compounding rules ahead of the 1 affix
    // rule in `mrules`, so this test exercises a non-trivial index offset, not just the identity case.
    assert_eq!(grammar.mrules.len(), 3);
    assert_grammar_ids_are_internally_consistent(&grammar);
}

#[test]
fn variant_entry_grammar_dense_ids_are_internally_consistent() {
    // Reuse the variant-entry scenario (stem + variant + affix + template) to also exercise the
    // multi-entry, multi-allomorph-owner case through the same consistency walk.
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

// --- clitic morph types are Phase B (no dedicated Clitics-stratum handling) ---------------------

#[test]
fn enclitic_entry_compiles_to_clitic_stratum_lex_entry_and_affix_rule() {
    // HCLoader.cs:256-293's form partition: an enclitic `MoStemAllomorph` is BOTH a valid clitic
    // lex-entry form (`IsValidLexEntryForm` + `IsCliticType`) and a valid rule form
    // (`IsValidRuleForm`, HCLoader.cs:550-552), so the entry appears on the Clitics stratum twice
    // over: as a `LexEntry` (stem role) and as a clitic affix-process rule
    // (`LoadCliticAffixProcessRule`, HCLoader.cs:1030-1046, suffix-shaped per
    // `LoadFormAffixProcessAllomorph`'s shared enclitic/suffix arm).
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
