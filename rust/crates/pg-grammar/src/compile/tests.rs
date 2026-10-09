//! Unit tests for `pg_grammar::compile`, built entirely from code-constructed `Snapshot` values (no `.fwdata`/oracle files).

use std::collections::{BTreeMap, BTreeSet};

use pg_snapshot::feature::{
    ClosedFeature, ComplexFeature, FeatureStructure, FeatureSystem, FeatureValue, FeatureValueKind,
    FeatureValueSymbol,
};
use pg_snapshot::lexicon::{
    AffixProcess, Allomorph, EntryRef, LexEntry, Lexicon, Msa, RuleMapping, Sense,
};
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
use super::{CompiledAllomorphOrder, CompiledMapping};

/// The output key a compiled mapping names, by its 1-based output id.
fn key_of<'a>(output: &'a CompileOutput, mapping: &CompiledMapping) -> &'a str {
    &output.compiled_outputs[mapping.output_id as usize - 1].key
}

fn kind_of(output: &CompileOutput, mapping: &CompiledMapping) -> &'static str {
    output.compiled_outputs[mapping.output_id as usize - 1]
        .kind
        .as_str()
}

fn row_key<'a>(output: &'a CompileOutput, row: &CompiledAllomorphOrder) -> Option<&'a str> {
    row.output_id
        .map(|id| output.compiled_outputs[id as usize - 1].key.as_str())
}

fn bucket_of_row<'a>(output: &'a CompileOutput, row: &CompiledAllomorphOrder) -> &'a str {
    row.owner_output_id.map_or("", |owner| {
        output.compiled_outputs[owner as usize - 1].bucket.as_str()
    })
}
use super::{
    compile_project, compile_project_measured, compile_project_recording, compile_project_with,
    environment, CompileOptions, CompileOutput, SemanticLossPolicy,
};

fn warning_metadata(warning: &pg_snapshot::Warning) -> pg_snapshot::ImportWarningMetadata {
    let code = pg_snapshot::ImportWarningCode::from_wire_or_unregistered(&warning.code);
    pg_snapshot::import_warning_metadata(code)
}

#[test]
fn missing_natural_class_reports_info_and_drops_the_entire_environment() {
    let (mut snapshot, _) = fixture();
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "missing-class-environment".into(),
            name: "Missing class probe".into(),
            representation: "/ k [Absent] _".into(),
        });
    for entry in &mut snapshot.lexicon.entries {
        for allomorph in &mut entry.allomorphs {
            allomorph
                .environments
                .push("missing-class-environment".into());
        }
    }
    let compiled = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    let resolution = compiled
        .environment_resolutions
        .iter()
        .find(|resolution| resolution.environment_guid == "missing-class-environment")
        .unwrap();
    assert_eq!(
        resolution.status,
        super::EnvironmentResolutionStatus::Invalid
    );
    assert!(resolution.left.is_none() && resolution.right.is_none());
    assert_eq!(resolution.class_tokens[0].token_text, "[Absent]");
    assert!(resolution.class_tokens[0].natural_class_index.is_none());
    let findings = compiled
        .warnings
        .iter()
        .filter(|warning| warning.code == "grammar.environment.missing-natural-class")
        .collect::<Vec<_>>();
    assert_eq!(
        findings.len(),
        1,
        "one cached owner result must produce one Info finding"
    );
    let diagnostic =
        crate::grammar_health::GrammarHealthDiagnostic::from_import_warning(findings[0]);
    assert_eq!(diagnostic.level, pg_snapshot::DiagnosticLevel::Info);
    assert!(diagnostic.message.contains("[Absent]"));
    assert!(diagnostic.message.contains("entire environment"));
    assert_eq!(
        diagnostic.subjects[0].guid.as_deref(),
        Some("missing-class-environment")
    );
    let outcome = pg_parse::Morpher::new(&compiled.grammar, 100_000).parse_word("kumata");
    assert!(!outcome.capped && !outcome.timed_out && !outcome.invalid_shape);
    assert!(
        !outcome.analyses.is_empty(),
        "the surviving literal k restriction must also be dropped"
    );
    snapshot.phonology.environments[0].representation = "/ k _".into();
    let repaired = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert!(!repaired
        .warnings
        .iter()
        .any(|warning| warning.code == "grammar.environment.missing-natural-class"));
    assert!(
        pg_parse::Morpher::new(&repaired.grammar, 100_000)
            .parse_word("kumata")
            .analyses
            .is_empty(),
        "the valid literal environment must bind"
    );
}

#[test]
fn featureless_phoneme_info_requires_a_feature_condition() {
    for class_count in [0, 1, 2] {
        let (mut snapshot, _) = fixture();
        snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];
        add_front_feature_class_and_rewrite_rule(&mut snapshot);
        snapshot.phonology.rules.clear();
        if class_count == 0 {
            let SnapNaturalClass::Features { features, .. } =
                &mut snapshot.phonology.natural_classes[0]
            else {
                panic!("feature-class fixture");
            };
            features.values.clear();
        } else if class_count == 2 {
            let mut second = snapshot.phonology.natural_classes[0].clone();
            let SnapNaturalClass::Features { guid, name, .. } = &mut second else {
                panic!("feature-class fixture");
            };
            *guid = "nc-front-second".into();
            *name = "Second".into();
            snapshot.phonology.natural_classes.push(second);
        }
        let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        let report = crate::grammar_health::check_grammar_health(&output.grammar, None).unwrap();
        let findings: Vec<_> = report
            .diagnostics()
            .iter()
            .filter(|finding| finding.code.wire() == "provisional.phoneme-features")
            .collect();
        assert_eq!(findings.len(), if class_count == 0 { 0 } else { 7 });
        if class_count > 0 {
            for letter in ["k", "t", "m", "s", "a", "i", "u"] {
                assert_eq!(
                    findings
                        .iter()
                        .filter(|finding| finding.message.contains(&format!("phoneme '{letter}'")))
                        .count(),
                    1
                );
            }
            assert!(findings
                .iter()
                .all(|finding| finding.level == pg_snapshot::DiagnosticLevel::Info));
        }
        let encoded = serde_json::to_string(&report).unwrap();
        let decoded = crate::grammar_health::GrammarHealthReport::from_json(&encoded).unwrap();
        assert_eq!(decoded, report);
    }
}

#[test]
fn unconstrained_feature_classes_match_provisional_and_featureless_stems() {
    for authored in [false, true] {
        let (mut snapshot, _) = fixture();
        snapshot.morphology.parts_of_speech[0].affix_slots[0].optional = true;
        snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "kumaq")];
        if authored {
            add_explicit_featureless_segment(&mut snapshot, "q");
        }
        snapshot
            .phonology
            .natural_classes
            .push(SnapNaturalClass::Features {
                guid: "nc-wildcard".into(),
                name: "Wildcard".into(),
                display_name: Some("Wildcard".into()),
                features: FeatureStructure::default(),
            });
        snapshot
            .phonology
            .environments
            .push(pg_snapshot::phonology::Environment {
                guid: "env-wildcard".into(),
                name: String::new(),
                representation: "/[Wildcard]_".into(),
            });
        snapshot.lexicon.entries[1].allomorphs[0]
            .environments
            .push("env-wildcard".into());
        let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        let wildcard = output
            .grammar
            .natural_classes
            .iter()
            .find(|class| class.name.as_deref() == Some("Wildcard"))
            .unwrap();
        assert_eq!(
            crate::segment::nat_class_cd_set(&output.grammar.char_tables[0], wildcard),
            pg_shape::CdSet::Unrestricted
        );
        assert!(
            !pg_parse::Morpher::new(&output.grammar, usize::MAX)
                .parse_word("kumaqta")
                .analyses
                .is_empty(),
            "authored={authored}"
        );
    }
}

#[test]
fn provisional_letters_are_enabled_for_every_project_without_parser_policy() {
    for active_parser in [ActiveParser::Hc, ActiveParser::XAmple] {
        for accept in [false, true] {
            let (mut snapshot, _) = fixture();
            snapshot.morphology.parser_parameters.active_parser = active_parser;
            snapshot
                .morphology
                .parser_parameters
                .accept_unspecified_graphemes = accept;
            snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];
            let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
            assert_eq!(output.substrate.inferred_segments[0].representation, "q");
            assert_eq!(output.grammar.entries[0].allomorphs.len(), 1);
            let warning = output
                .warnings
                .iter()
                .find(|warning| warning.code == "provisional.letter")
                .unwrap();
            let finding =
                crate::grammar_health::GrammarHealthDiagnostic::from_import_warning(warning);
            assert_eq!(finding.level, pg_snapshot::DiagnosticLevel::Info);
            assert!(finding.message.contains("'q'"));
            assert!(finding.message.contains("no natural class"));
        }
    }
}

#[test]
fn provisional_multigraph_exemplars_preserve_root_and_prefix_analyses() {
    let (mut snapshot, fixture) = fixture();
    snapshot.project.exemplar_characters = vec!["ch".into()];
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "chuma")];
    let mut root = snapshot.lexicon.entries[0].clone();
    root.guid = "entry-huma".into();
    root.allomorphs[0].guid = "allo-huma".into();
    root.allomorphs[0].forms = vec![ws("sen", "huma")];
    if let Msa::Stem { guid, .. } = &mut root.msas[0] {
        *guid = "msa-huma".into();
    }
    root.senses[0].guid = "sense-huma".into();
    root.senses[0].msa = Some("msa-huma".into());
    snapshot.lexicon.entries.push(root);
    let prefix = &mut snapshot.lexicon.entries[1];
    prefix.lexeme_morph_type = MorphType::Prefix;
    prefix.allomorphs[0].morph_type = MorphType::Prefix;
    prefix.allomorphs[0].forms = vec![ws("sen", "c")];
    let noun = &mut snapshot.morphology.parts_of_speech[0];
    noun.affix_slots[0].optional = true;
    noun.affix_templates[0].suffix_slots.clear();
    noun.affix_templates[0].prefix_slots = vec![fixture.slot];
    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    let result = pg_parse::Morpher::new(&output.grammar, usize::MAX).parse_word("chuma");
    assert_eq!(result.structured.len(), 2, "{}", result.signature());
    assert!(output.grammar.char_tables[0].lookup_nfd("ch").is_none());
    assert!(output.grammar.char_tables[0].lookup_nfd("c").is_some());
    assert!(output.grammar.char_tables[0].lookup_nfd("h").is_some());
}

#[test]
fn provisional_letter_units_keep_a_composed_or_decomposed_grapheme_together() {
    for letter in ["ã", "a\u{0303}"] {
        let (mut snapshot, _) = fixture();
        snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", &format!("s{letter}m"))];
        snapshot.project.exemplar_characters = vec!["a".into()];
        let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        assert_eq!(output.substrate.inferred_segments.len(), 1);
        assert_eq!(
            output.substrate.inferred_segments[0].representation,
            "a\u{0303}"
        );
        assert_eq!(
            output.grammar.entries[0].allomorphs[0]
                .shape
                .shape
                .interior()
                .count(),
            3
        );
        assert!(output.grammar.char_tables[0]
            .lookup_nfd("\u{0303}")
            .is_none());
    }
}

#[test]
fn provisional_letter_control_refusal_names_the_owners_allomorph() {
    let (mut snapshot, _) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "ku\u{0001}ma")];
    let error = compile_project_with(&snapshot, CompileOptions::default()).unwrap_err();
    assert!(error.issues().iter().any(|issue| issue.code
        == pg_snapshot::ImportWarningCode::SubstrateClassificationAmbiguous
        && issue.fatal
        && issue
            .source
            .as_ref()
            .is_some_and(|source| source.id == "allo-stem")));
    assert!(error.issues().iter().any(|issue| issue.code
        == super::issue_codes::ALLOMORPH_UNSEGMENTABLE
        && issue
            .source
            .as_ref()
            .is_some_and(|source| source.id == "allo-stem")));
}

#[test]
fn provisional_letter_overlap_with_an_authored_multigraph_refuses_by_name() {
    let (mut snapshot, _) = fixture();
    snapshot.phonology.phonemes.push(phoneme("ph-ch", "ch"));
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "ch\u{0301}uma")];
    let error = compile_project_with(&snapshot, CompileOptions::default()).unwrap_err();
    assert!(error.issues().iter().any(|issue| issue.code
        == pg_snapshot::ImportWarningCode::SubstrateClassificationAmbiguous
        && issue.fatal
        && issue.message.contains("still cannot be segmented")
        && issue
            .source
            .as_ref()
            .is_some_and(|source| source.id == "allo-stem")));
}

#[test]
fn provisional_letter_has_no_membership_in_feature_or_segment_classes() {
    let (mut snapshot, _) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];
    add_front_feature_class_and_rewrite_rule(&mut snapshot);
    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    let table = &output.grammar.char_tables[0];
    let q = table.lookup_nfd("q").unwrap();
    let feature = output
        .grammar
        .natural_classes
        .iter()
        .find(|class| class.name.as_deref() == Some("Front"))
        .unwrap();
    let listed = crate::model::NaturalClass {
        xml_id: "listed-q".into(),
        name: Some("Listed q".into()),
        kind: crate::model::NaturalClassKind::Segments(vec![q]),
    };
    for class in [feature, &listed] {
        let pg_shape::CdSet::Members(bits) = crate::segment::nat_class_cd_set(table, class) else {
            panic!("a class containing no provisional letters cannot be unrestricted");
        };
        assert!(!bits.contains(q.0));
    }
}

#[test]
fn featureless_authored_phoneme_keeps_list_membership_but_no_feature_class_membership() {
    let (mut snapshot, _) = fixture();
    add_explicit_featureless_segment(&mut snapshot, "q");
    add_front_feature_class_and_rewrite_rule(&mut snapshot);
    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    let table = &output.grammar.char_tables[0];
    let q = table.lookup_nfd("q").unwrap();
    let feature = output
        .grammar
        .natural_classes
        .iter()
        .find(|class| class.name.as_deref() == Some("Front"))
        .unwrap();
    let pg_shape::CdSet::Members(bits) = crate::segment::nat_class_cd_set(table, feature) else {
        panic!("featureless phonemes must be excluded");
    };
    assert!(!bits.contains(q.0));
    let listed = crate::model::NaturalClass {
        xml_id: "listed-q".into(),
        name: Some("Listed q".into()),
        kind: crate::model::NaturalClassKind::Segments(vec![q]),
    };
    let pg_shape::CdSet::Members(bits) = crate::segment::nat_class_cd_set(table, &listed) else {
        panic!("listed membership is explicit");
    };
    assert!(bits.contains(q.0));
    let finding = crate::grammar_health::check_grammar_health_diagnostics(&output.grammar)
        .unwrap()
        .into_iter()
        .find(|finding| {
            finding.code.wire() == "provisional.phoneme-features" && finding.message.contains("'q'")
        })
        .unwrap();
    assert_eq!(finding.level, pg_snapshot::DiagnosticLevel::Info);
    assert!(finding.message.contains("segment-list"));
    assert!(finding.message.contains("FieldWorks"));
}

#[test]
fn provisional_and_featureless_letters_do_not_match_a_runtime_feature_environment() {
    for (authored, extra) in [(false, 0), (true, 0), (false, 70)] {
        let (mut snapshot, _) = fixture();
        snapshot.morphology.parts_of_speech[0].affix_slots[0].optional = true;
        snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "kumaq")];
        add_front_feature_class_and_rewrite_rule(&mut snapshot);
        snapshot.phonology.rules.clear();
        if authored {
            add_explicit_featureless_segment(&mut snapshot, "q");
        }
        for index in 0..extra {
            snapshot
                .phonology
                .phonemes
                .push(phoneme(&format!("extra-{index}"), &format!("z{index}")));
        }
        snapshot
            .phonology
            .environments
            .push(pg_snapshot::phonology::Environment {
                guid: "env-front-final".into(),
                name: String::new(),
                representation: "/[Front]_".into(),
            });
        snapshot.lexicon.entries[1].allomorphs[0]
            .environments
            .push("env-front-final".into());
        let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        let parser = pg_parse::Morpher::new(&output.grammar, usize::MAX);
        assert!(
            !parser.parse_word("kumaq").analyses.is_empty(),
            "bare stem: authored={authored}, extra={extra}"
        );
        assert!(
            parser.parse_word("kumaqta").analyses.is_empty(),
            "authored={authored}, extra={extra}"
        );
        let mut valued = snapshot;
        if authored {
            valued
                .phonology
                .phonemes
                .retain(|phoneme| phoneme.name != "q");
        }
        add_explicit_feature_valued_segment(&mut valued, "q", "feat-frontness");
        valued
            .phonology
            .phonemes
            .last_mut()
            .unwrap()
            .features
            .as_mut()
            .unwrap()
            .values[0]
            .value = FeatureValueKind::Closed {
            value: "val-front".into(),
        };
        let valued = compile_project_with(&valued, CompileOptions::default()).unwrap();
        assert!(!pg_parse::Morpher::new(&valued.grammar, usize::MAX)
            .parse_word("kumaqta")
            .analyses
            .is_empty());
    }
}

#[test]
fn boundary_inference_uses_only_the_owners_preferred_writing_system_forms() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.phonology.boundary_markers.push(BoundaryMarker {
        guid: "bd-multilingual".into(),
        name: "separator".into(),
        representations: vec![ws("en", "-"), ws("sen", "+")],
    });
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "kuma-")];

    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert!(output.substrate.inferred_boundaries.is_empty());
    let table = &output.grammar.char_tables[0];
    assert!(table.lookup_nfd("+").is_some());
    if let Some(id) = table.lookup_nfd("-") {
        assert_ne!(table.get(id).kind(), crate::chardef::CharDefKind::Boundary);
    }
}

#[test]
fn affix_validity_and_construction_use_the_same_preferred_writing_system_form() {
    for other in ["", "[C]"] {
        let (mut snapshot, _) = fixture();
        snapshot.lexicon.entries[1].allomorphs[0].forms = vec![ws("en", other), ws("sen", "ta")];
        let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        assert_eq!(
            output
                .grammar
                .mrules
                .iter()
                .filter(|rule| matches!(rule, MorphRuleDef::AffixProcess(_)))
                .count(),
            1,
            "other form {other:?}"
        );
        assert!(!output.issues.iter().any(|issue| {
            issue
                .source
                .as_ref()
                .is_some_and(|source| source.id == "allo-suffix")
        }));
    }
    for preferred in ["", "[C]"] {
        let (mut snapshot, _) = fixture();
        snapshot.lexicon.entries[1].allomorphs[0].forms =
            vec![ws("en", "ta"), ws("sen", preferred)];
        let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        assert!(
            output
                .grammar
                .mrules
                .iter()
                .all(|rule| !matches!(rule, MorphRuleDef::AffixProcess(_))),
            "preferred form {preferred:?}"
        );
        assert!(output.issues.iter().any(|issue| {
            issue
                .source
                .as_ref()
                .is_some_and(|source| source.id == "allo-suffix")
        }));
    }
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

#[test]
fn import_and_compile_warnings_deduplicate_at_the_compile_boundary() {
    let (mut snapshot, _) = fixture();
    let guid = "00000000-0000-0000-0000-000000000044";
    let code = pg_snapshot::ImportWarningCode::FwdataUnknownMorphTypeGuid;
    snapshot
        .conversion_provenance
        .import_issues
        .push(pg_snapshot::ConversionIssue {
            code: code.clone(),
            class: IssueClass::MigrationDifference,
            source: Some(pg_snapshot::SourceRef {
                kind: pg_snapshot::FwClass::MoForm,
                id: guid.to_string(),
            }),
            fatal: false,
            message: "compiler wording for the same warning".to_string(),
        });
    let importer_warning = pg_snapshot::Warning::new(code, "Importer wording for the same warning")
        .with_subject(
            pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::MoForm)
                .guid(guid)
                .source_class("MoAffixAllomorph")
                .field("MorphType"),
        );

    let (_, warnings) =
        super::compile_project_with_import_warnings(&snapshot, [importer_warning.clone()])
            .expect("snapshot compiles");
    let matching: Vec<_> = warnings
        .into_iter()
        .filter(|warning| warning.code == importer_warning.code)
        .collect();

    assert_eq!(matching, vec![importer_warning]);
}

#[test]
fn import_issue_projection_keeps_each_rich_owner_and_field() {
    let (snapshot, _) = fixture();
    let guid = "00000000-0000-0000-0000-0000000000ff";
    let code = pg_snapshot::ImportWarningCode::FwdataDanglingReference;
    let first = pg_snapshot::Warning::new(code.clone(), "Missing environment.")
        .with_subject(
            pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::MoForm)
                .guid("00000000-0000-0000-0000-000000000042")
                .field("PhoneEnv"),
        )
        .with_subject(
            pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::PhEnvironment)
                .guid(guid)
                .unresolved_reference()
                .field("PhoneEnv"),
        );
    let mut second = first.clone();
    second.subjects[0].field = Some("Position".to_string());
    second.subjects[1].field = Some("Position".to_string());
    let issue = ConversionIssue {
        code,
        class: IssueClass::InvalidSource,
        source: Some(pg_snapshot::SourceRef {
            kind: pg_snapshot::FwClass::PhEnvironment,
            id: guid.to_uppercase(),
        }),
        fatal: true,
        message: "Coarse imported issue.".to_string(),
    };
    let imported = vec![first.clone(), second.clone()];
    assert!(super::warnings::from_import_issues(
        &snapshot,
        std::slice::from_ref(&issue),
        &imported
    )
    .is_empty());
    assert_eq!(super::warnings::deduplicate(imported), vec![first, second]);
    assert_eq!(
        super::warnings::from_import_issues(&snapshot, &[issue], &[]).len(),
        1
    );
}

#[test]
fn compacted_natural_class_and_msa_without_usable_allomorphs_reach_grammar_health() {
    let (mut snapshot, _) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "?")];
    snapshot.phonology.natural_classes.extend([
        SnapNaturalClass::Segments {
            guid: "nc-unreferenced-compacted".to_string(),
            name: String::new(),
            display_name: None,
            phonemes: vec!["ph-k".to_string()],
        },
        SnapNaturalClass::Segments {
            guid: "nc-last-unnamed".to_string(),
            name: String::new(),
            display_name: None,
            phonemes: vec!["ph-t".to_string()],
        },
    ]);

    let (grammar, warnings) = compile_project(&snapshot).expect("snapshot compiles");
    assert_eq!(grammar.entries[0].allomorphs.len(), 1);
    let provisional = warnings
        .iter()
        .find(|warning| warning.code == "provisional.letter")
        .unwrap();
    assert!(provisional.message.contains("'?'"));
    assert_eq!(
        warning_metadata(provisional).level,
        pg_snapshot::DiagnosticLevel::Info
    );
    let report = crate::grammar_health::GrammarHealthReport::new(
        warnings
            .iter()
            .map(crate::grammar_health::GrammarHealthDiagnostic::from_import_warning)
            .collect(),
    )
    .expect("compiler warnings form a grammar-health report");

    let findings = report.diagnostics();
    assert!(findings
        .iter()
        .all(|finding| finding.code.wire() != "grammar.msa.no-allomorphs"));

    let compacted_natural_class = findings
        .iter()
        .find(|finding| finding.code.wire() == "grammar.natclass.unreferenced-compacted")
        .expect("a compacted natural class reaches grammar-health");
    assert_eq!(
        compacted_natural_class.level,
        pg_snapshot::DiagnosticLevel::Info
    );
    assert_eq!(
        compacted_natural_class.subjects[0].guid.as_deref(),
        Some("nc-unreferenced-compacted")
    );
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "\u{0001}")];
    let refused = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
        },
    )
    .unwrap();
    assert!(refused.issues.iter().any(|issue| issue.code
        == pg_snapshot::ImportWarningCode::SubstrateClassificationAmbiguous
        && issue.fatal));
    let msa_without_allomorphs = refused
        .warnings
        .iter()
        .find(|warning| warning.code == "grammar.msa.no-allomorphs")
        .unwrap();
    assert_eq!(
        warning_metadata(msa_without_allomorphs).level,
        pg_snapshot::DiagnosticLevel::Error
    );
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
    let (grammar, recorder, _substrate, substrate_issues, owner_warnings, _) =
        compile_project_recording(snapshot).expect("must compile");
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
    warnings.extend(super::warnings::from_import_issues(
        snapshot,
        &issues,
        &owner_warnings,
    ));
    warnings.extend(owner_warnings);
    warnings.extend(super::warnings::from_issues(snapshot, &substrate_issues));
    let warnings = super::warnings::deduplicate(warnings);
    (grammar, warnings, inventory, issues)
}

fn linguist_warnings(warnings: &[pg_snapshot::Warning]) -> Vec<&pg_snapshot::Warning> {
    warnings
        .iter()
        .filter(|warning| {
            let code = pg_snapshot::ImportWarningCode::from_wire_or_unregistered(&warning.code);
            pg_snapshot::warning_metadata::import_warning_metadata(code).level
                != pg_snapshot::DiagnosticLevel::Info
        })
        .collect()
}

#[test]
fn subjects_without_a_tool_of_their_own_open_where_fieldworks_edits_them() {
    use pg_snapshot::{FwClass, FwObjectRef, FwOpenTarget, Warning};
    let (mut snapshot, _f) = fixture();
    let set = "11111111-1111-1111-1111-111111111111";
    snapshot.phonology.phoneme_set = Some(set.to_string());
    let feature = |guid: &str| pg_snapshot::feature::ClosedFeature {
        guid: guid.to_string(),
        name: "feature".to_string(),
        abbreviation: "f".to_string(),
        values: Vec::new(),
    };
    let phonological = "22222222-2222-2222-2222-222222222222";
    let inflection = "33333333-3333-3333-3333-333333333333";
    let marker = "44444444-4444-4444-4444-444444444444";
    let natural_class = "55555555-5555-5555-5555-555555555555";
    snapshot
        .feature_systems
        .phonological
        .closed_features
        .push(feature(phonological));
    snapshot
        .feature_systems
        .morphosyntactic
        .closed_features
        .push(feature(inflection));
    let subject = |source: FwObjectRef| {
        Warning::new(super::issue_codes::STRATA_CUSTOM_UNSUPPORTED, "m").with_subject(source)
    };
    let mut warnings = vec![
        subject(FwObjectRef::new(FwClass::PhBdryMarker).guid(marker)),
        subject(FwObjectRef::new(FwClass::PhPhoneme).name("N")),
        subject(FwObjectRef::new(FwClass::FsClosedFeature).guid(phonological)),
        subject(FwObjectRef::new(FwClass::FsClosedFeature).guid(inflection)),
        subject(FwObjectRef::new(FwClass::PhNaturalClass).guid(natural_class)),
    ];

    super::warnings::add_open_targets(&snapshot, &mut warnings);

    let target = |tool: &str, guid: &str| {
        Some(FwOpenTarget {
            tool: tool.to_string(),
            guid: guid.to_string(),
        })
    };
    let opens: Vec<_> = warnings
        .iter()
        .map(|warning| warning.subjects[0].opens_in.clone())
        .collect();
    assert_eq!(
        opens,
        vec![
            target("phonemeEdit", set),
            target("phonemeEdit", set),
            target("phonologicalFeaturesAdvancedEdit", phonological),
            target("featuresAdvancedEdit", inflection),
            None,
        ],
        "only subjects whose class cannot pick a tool get an explicit target"
    );
}

#[test]
fn entry_headword_falls_back_to_the_lexeme_form_like_fieldworks() {
    let (snapshot, _f) = fixture();
    let mut entry = snapshot.lexicon.entries[0].clone();
    let lexeme_form = entry.allomorphs.last().unwrap().forms[0].form.clone();
    entry.citation_form = vec![ws("sen", " ")];
    assert_eq!(
        super::entry_headword(&entry, None),
        Some(lexeme_form.as_str())
    );
    entry.citation_form = vec![ws("sen", "cited")];
    assert_eq!(super::entry_headword(&entry, None), Some("cited"));
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
fn diagnostic_adapter_keeps_distinct_affix_and_template_causes() {
    let (snapshot, _) = fixture();
    for (code, class, id, cause) in [
        (
            pg_snapshot::ImportWarningCode::AllomorphNotRuleForm,
            pg_snapshot::FwClass::MoForm,
            snapshot.lexicon.entries[0].allomorphs[0].guid.clone(),
            "infix allomorph has no position environment",
        ),
        (
            pg_snapshot::ImportWarningCode::AllomorphNotRuleForm,
            pg_snapshot::FwClass::MoForm,
            snapshot.lexicon.entries[0].allomorphs[0].guid.clone(),
            "allomorph form is empty",
        ),
        (
            pg_snapshot::ImportWarningCode::TemplateNoSlots,
            pg_snapshot::FwClass::MoInflAffixTemplate,
            "10000000-0000-0000-0000-000000000003".to_string(),
            "affix template has no slots with any loaded affix rule",
        ),
        (
            pg_snapshot::ImportWarningCode::NullAffixSegmentFailed,
            pg_snapshot::FwClass::LexEntryInflType,
            "10000000-0000-0000-0000-000000000004".to_string(),
            "generated marker ^0+ could not be segmented",
        ),
        (
            pg_snapshot::ImportWarningCode::AllomorphUnsegmentable,
            pg_snapshot::FwClass::MoForm,
            snapshot.lexicon.entries[0].allomorphs[0].guid.clone(),
            "no character definition matches 'x' at position 0",
        ),
    ] {
        let issue = ConversionIssue {
            code,
            class: IssueClass::UnrepresentableForHc,
            source: Some(pg_snapshot::SourceRef { kind: class, id }),
            fatal: false,
            message: cause.to_string(),
        };
        let warning = super::warnings::from_issue(&snapshot, &issue);
        assert!(warning.message.contains(cause), "{warning:?}");
        assert!(!warning.message.contains("form FieldWorks can use"));
    }
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
        "Grammatical analysis 'Unnamed grammatical analysis' has no affix form PanGloss can load."
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

/// FieldWorks drops invalid root restrictions and emits one blank pass for literal affixes.
#[test]
fn sample_invalid_environments_compile_with_warnings_and_preserve_parses() {
    for (case, text) in ["/", "/", "_#", "[+ATR] (C)", "/[+ATR] _ #"]
        .into_iter()
        .enumerate()
    {
        for entry_index in [0, 1] {
            let (mut snapshot, _) = fixture();
            let baseline = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
            let env_guid = format!("env-bad-{case}");
            snapshot
                .phonology
                .environments
                .push(pg_snapshot::phonology::Environment {
                    guid: env_guid.clone(),
                    name: String::new(),
                    representation: text.into(),
                });
            let allo = &mut snapshot.lexicon.entries[entry_index].allomorphs[0];
            allo.guid = format!("seeded-allo-{entry_index}-{case}");
            allo.environments.push(env_guid.clone());
            let allo_guid = allo.guid.clone();
            let out = compile_project_with(&snapshot, CompileOptions::default())
                .unwrap_or_else(|error| panic!("{text:?}: {error}"));
            assert!(out
                .issues
                .iter()
                .any(|issue| issue.code == super::issue_codes::ENVIRONMENT_INVALID));
            assert!(out.issues.iter().all(|issue| !issue.fatal));
            let warning = out
                .warnings
                .iter()
                .find(|w| w.code == "grammar.environment.invalid")
                .unwrap();
            assert!(warning.message.contains(text), "{warning:?}");
            assert!(warning
                .subjects
                .iter()
                .any(|s| s.guid.as_deref() == Some(allo_guid.as_str())));
            assert!(warning_guidance(warning)
                .unwrap()
                .contains("Grammar > Environments"));
            assert_eq!(
                warning_metadata(warning).level,
                pg_snapshot::DiagnosticLevel::Warning
            );
            let expected = pg_parse::Morpher::new(&baseline.grammar, 100_000).parse_word("kumata");
            let actual = pg_parse::Morpher::new(&out.grammar, 100_000).parse_word("kumata");
            assert!(
                !actual.analyses.is_empty(),
                "seeded allomorph must remain usable: {text}"
            );
            assert!(!actual.capped && !actual.timed_out && !actual.invalid_shape);
            assert!(!expected.capped && !expected.timed_out && !expected.invalid_shape);
            assert_eq!(actual.signature(), expected.signature());
            assert!(pg_parse::Morpher::new(&out.grammar, 100_000)
                .parse_word("takuma")
                .analyses
                .is_empty());
        }
    }
}

#[test]
fn invalid_environment_does_not_widen_valid_root_restrictions() {
    for entry_index in [0, 1] {
        let (mut snapshot, _) = fixture();
        snapshot.phonology.environments.extend([
            pg_snapshot::phonology::Environment {
                guid: "valid-env".into(),
                name: String::new(),
                representation: "/t_".into(),
            },
            pg_snapshot::phonology::Environment {
                guid: "bad-env".into(),
                name: String::new(),
                representation: "/".into(),
            },
        ]);
        snapshot.lexicon.entries[entry_index].allomorphs[0].environments = vec!["valid-env".into()];
        let valid = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        assert!(pg_parse::Morpher::new(&valid.grammar, 100_000)
            .parse_word("kumata")
            .analyses
            .is_empty());
        snapshot.lexicon.entries[entry_index].allomorphs[0]
            .environments
            .push("bad-env".into());
        let mixed = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        let parses = pg_parse::Morpher::new(&mixed.grammar, 100_000).parse_word("kumata");
        assert_eq!(
            parses.analyses.is_empty(),
            entry_index == 0,
            "roots omit invalid restrictions; affixes add a blank pass"
        );
    }
}

#[test]
fn invalid_infix_positions_skip_the_allomorph_without_refusing_the_grammar() {
    for text in ["/", "_#", "[+ATR] (C)", "/[+ATR] _ #"] {
        let (mut snapshot, _) = fixture();
        snapshot
            .phonology
            .environments
            .push(pg_snapshot::phonology::Environment {
                guid: "bad-position".into(),
                name: String::new(),
                representation: text.into(),
            });
        let mut infix = snapshot.lexicon.entries[1].clone();
        infix.guid = "infix-entry".into();
        infix.lexeme_morph_type = MorphType::Infix;
        infix.senses[0].msa = Some("infix-msa".into());
        infix.senses[0].guid = "infix-sense".into();
        infix.allomorphs[0].guid = "infix-allo".into();
        infix.allomorphs[0].morph_type = MorphType::Infix;
        infix.allomorphs[0].positions = vec!["bad-position".into()];
        if let Msa::Inflectional { guid, .. } = &mut infix.msas[0] {
            *guid = "infix-msa".into();
        }
        snapshot.lexicon.entries.push(infix);
        let out = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        assert!(out
            .warnings
            .iter()
            .any(|w| w.code == "grammar.environment.invalid"));
        let morpher = pg_parse::Morpher::new(&out.grammar, 100_000);
        assert!(!morpher.parse_word("kumata").analyses.is_empty());
        assert!(
            morpher.parse_word("kutama").analyses.is_empty(),
            "invalid infix must not become freely insertable"
        );
        snapshot.phonology.environments[0].representation = "/ku_m".into();
        let repaired = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        let repaired = pg_parse::Morpher::new(&repaired.grammar, 100_000).parse_word("kutama");
        assert!(
            !repaired.analyses.is_empty(),
            "a repaired infix must become usable"
        );
        assert!(!repaired.capped && !repaired.timed_out && !repaired.invalid_shape);
    }
}

#[test]
fn unresolved_affix_environment_keeps_identifier_out_of_linguist_warning() {
    let (mut snapshot, _) = fixture();
    let dangling_guid = "dangling-env-guid";
    snapshot.lexicon.entries[1].allomorphs[0]
        .environments
        .push(dangling_guid.to_string());

    let production = compile_project_with(&snapshot, CompileOptions::default());
    let Err(GrammarError::Conversion(refused)) = production else {
        panic!("active restriction or rule must refuse if it cannot be preserved")
    };
    assert!(refused.issues.iter().any(|issue| issue.code
        == super::issue_codes::ENVIRONMENT_UNRESOLVED
        && issue.fatal
        && issue.source.is_some()));

    let output = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
        },
    )
    .expect("an unresolved affix environment remains inspectable in measurement");
    let issue = output
        .issues
        .iter()
        .find(|issue| {
            issue.code == super::issue_codes::ENVIRONMENT_UNRESOLVED
                && issue.message.contains(dangling_guid)
        })
        .expect("the structured issue must retain the unresolved environment id");
    assert!(issue.fatal);

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

    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("FieldWorks ignores invalid root restrictions with a warning");
    let resolution = output
        .environment_resolutions
        .iter()
        .find(|resolution| resolution.environment_guid == "env-bad")
        .expect("the owner-published whole-expression result is exposed");
    assert_eq!(
        resolution.status,
        super::EnvironmentResolutionStatus::Invalid
    );
    assert!(resolution
        .error
        .as_deref()
        .is_some_and(|error| error.contains("Nas")));
    let warnings = output.warnings;
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
    assert_eq!(
        warning_metadata(warning).level,
        pg_snapshot::DiagnosticLevel::Warning
    );
    assert!(warning.message.contains("/[Nas]_"));
    assert!(warning.message.contains("allomorph 'kuma' of entry 'kuma'"));
    assert_eq!(warning.subjects.len(), 3);
    assert_eq!(
        warning.subjects[0].class,
        pg_snapshot::FwClass::PhEnvironment
    );
    assert_eq!(warning.subjects[0].guid.as_deref(), Some("env-bad"));
    assert_eq!(warning.subjects[0].name.as_deref(), Some("bad environment"));
    assert_eq!(
        warning.subjects[0].field.as_deref(),
        Some("StringRepresentation")
    );
    assert_eq!(warning.subjects[1].class, pg_snapshot::FwClass::MoForm);
    assert_eq!(warning.subjects[1].guid.as_deref(), Some("allo-stem"));
    assert_eq!(warning.subjects[1].name.as_deref(), Some("kuma"));
    assert_eq!(warning.subjects[1].field.as_deref(), Some("PhoneEnv"));
    assert_eq!(warning.subjects[2].class, pg_snapshot::FwClass::LexEntry);
    assert_eq!(warning.subjects[2].guid.as_deref(), Some("entry-stem"));
    assert_eq!(warning.subjects[2].name.as_deref(), Some("kuma"));
    let guidance = warning_guidance(warning).expect("invalid environment has owned advice");
    assert!(guidance.contains(pg_snapshot::fieldworks_paths::GRAMMAR_ENVIRONMENTS));
    assert!(guidance.contains("bad environment"));
    assert!(guidance.contains(pg_snapshot::fieldworks_paths::LEXICON_EDIT));
    assert!(guidance.contains("Allomorphs > Environments"));
    assert!(guidance.contains("Roots ignore invalid restrictions"));
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
            display_name: None,
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

    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let resolution = output
        .environment_resolutions
        .iter()
        .find(|resolution| resolution.environment_guid == "env-v")
        .expect("the owner-published environment result is exposed");
    assert_eq!(resolution.status, super::EnvironmentResolutionStatus::Valid);
    assert_eq!(
        resolution.class_tokens[0].natural_class_guid.as_deref(),
        Some("nc-vowel")
    );
    let grammar = output.grammar;
    let warnings = output.warnings;
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
    let compiled_pattern = affix_rules[0].allomorphs[0].environments[0]
        .right
        .as_ref()
        .expect("the authored right side reaches the compiled environment");
    let compiled_class_guid = compiled_pattern
        .nodes
        .iter()
        .find_map(|node| match node {
            crate::model::PatternNode::Context(context) => grammar
                .natural_classes
                .get(context.nat_class.0 as usize)
                .map(|class| class.xml_id.as_str()),
            _ => None,
        })
        .expect("the compiled pattern contains its resolved natural class");
    assert_eq!(
        Some(compiled_class_guid),
        resolution.class_tokens[0].natural_class_guid.as_deref(),
        "published winner and compiled pattern must identify the same source class"
    );
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

    let (grammar, recorder, _, _, warnings, _) =
        compile_project_recording(&snapshot).expect("must compile");
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
    let (_, _, decisions) = recorder.finish_with_load_decisions();
    let msa_key = InventoryKey::object(InventoryKind::Msa, f.stem_msa.clone());
    assert!(decisions.iter().any(|decision| {
        decision.subject == msa_key
            && decision.pipeline_stage == pg_snapshot::LoadPipelineStage::Compile
            && decision.context_key == "ancestorDefaultInflectionClass"
            && decision.disposition == pg_snapshot::LoadDisposition::Defaulted
            && decision.loaded == Some(true)
            && decision.reason_code == pg_snapshot::LoadReasonCode::AncestorDefaultInflectionClass
            && decision.effective_value_json.as_deref() == Some(r#"{"classGuid":"class-default"}"#)
    }));
    assert!(
        decisions.iter().any(|decision| {
            decision.subject == msa_key
                && decision.pipeline_stage == pg_snapshot::LoadPipelineStage::Compile
                && decision.disposition == pg_snapshot::LoadDisposition::Represented
        }),
        "defaulting and final representation must coexist for the MSA"
    );
    let _ = f;
}

#[test]
fn unsupported_affix_process_arity_is_published_without_changing_compilation_warnings() {
    let (mut snapshot, f) = fixture();
    let allomorph = &mut snapshot.lexicon.entries[1].allomorphs[0];
    allomorph.process = Some(AffixProcess {
        input: vec![PhonContext::Variable],
        output: vec![RuleMapping::CopyFromInput { part: 1 }],
    });

    let (_, recorder, _, _, warnings, _) =
        compile_project_recording(&snapshot).expect("must compile");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let (_, _, decisions) = recorder.finish_with_load_decisions();
    let decision = decisions
        .iter()
        .find(|decision| {
            decision.subject
                == InventoryKey::object(InventoryKind::Allomorph, "allo-suffix".to_string())
                && decision.pipeline_stage == pg_snapshot::LoadPipelineStage::Compile
                && decision.reason_code
                    == pg_snapshot::LoadReasonCode::ConversionIssue(
                        "affixProcessUnsupportedArity".into(),
                    )
        })
        .expect("unsupported process arity must have a typed owner decision");
    assert_eq!(decision.context_key, "affixProcessArity");
    assert_eq!(decision.disposition, pg_snapshot::LoadDisposition::Rejected);
    assert_eq!(decision.loaded, Some(false));
    assert_eq!(
        decision.reason_code,
        pg_snapshot::LoadReasonCode::ConversionIssue("affixProcessUnsupportedArity".into())
    );
    assert_eq!(
        decision.effective_value_json.as_deref(),
        Some(r#"{"inputParts":1,"outputParts":1}"#)
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

    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let grammar = &output.grammar;
    assert!(
        output.warnings.is_empty(),
        "unexpected warnings: {:?}",
        output.warnings
    );
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

    let variant_entry_mapping = output
        .compiled_mappings
        .iter()
        .find(|mapping| {
            mapping.source_kind == "entry"
                && mapping.source_guid.as_deref() == Some("entry-variant")
                && kind_of(&output, mapping) == "lex_entry"
        })
        .expect("the variant entry must map to its final lexical entry");
    let msa_mapping = output
        .compiled_mappings
        .iter()
        .find(|mapping| {
            mapping.source_kind == "msa"
                && mapping.source_guid.as_deref() == Some(f.stem_msa.as_str())
                && kind_of(&output, mapping) == "lex_entry"
                && key_of(&output, mapping) == key_of(&output, variant_entry_mapping)
        })
        .expect("variant output must preserve its borrowed main-entry MSA association");
    assert_eq!(
        key_of(&output, msa_mapping),
        key_of(&output, variant_entry_mapping)
    );
    assert!(output.compiled_mappings.iter().any(|mapping| {
        mapping.source_kind == "allomorph"
            && mapping.source_guid.as_deref() == Some("allo-variant")
            && kind_of(&output, mapping) == "allomorph"
            && mapping.identity_quality == "structural"
    }));
    assert!(output.compiled_allomorph_order.iter().any(|row| {
        row.source_entry_guid.as_deref() == Some("entry-variant")
            && row.source_msa_guid.as_deref() == Some(f.stem_msa.as_str())
            && row.source_allomorph_guid.as_deref() == Some("allo-variant")
            && bucket_of_row(&output, row) == "Morphology"
            && row.compiled_order == Some(0)
    }));
}

#[test]
fn variant_affix_order_joins_variant_entry_to_the_main_entry_msa() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-variant-affix".to_string(),
        citation_form: vec![ws("sen", "-ku")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph(
            "allo-variant-affix",
            MorphType::Suffix,
            "ku",
        )],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![EntryRef::Variant {
            guid: "entryref-variant-affix".to_string(),
            component_lexemes: vec![f.suffix_entry],
            variant_entry_types: Vec::new(),
        }],
    });

    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("variant affix must compile through its main MSA");
    assert!(output.compiled_allomorph_order.iter().any(|row| {
        row.source_entry_guid.as_deref() == Some("entry-variant-affix")
            && row.source_msa_guid.as_deref() == Some("msa-suffix")
            && row.source_allomorph_guid.as_deref() == Some("allo-variant-affix")
            && bucket_of_row(&output, row) == "Morphology"
            && row.compiled_order == Some(0)
    }));
}

fn variant_affix_sharing_main_msa(snapshot: &mut Snapshot, f: &Fixture) {
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-variant-affix".to_string(),
        citation_form: vec![ws("sen", "-ku")],
        lexeme_morph_type: MorphType::Suffix,
        allomorphs: vec![simple_allomorph(
            "allo-variant-affix",
            MorphType::Suffix,
            "ku",
        )],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![EntryRef::Variant {
            guid: "entryref-variant-affix".to_string(),
            component_lexemes: vec![f.suffix_entry.clone()],
            variant_entry_types: Vec::new(),
        }],
    });
}

#[test]
fn variant_affix_and_main_affix_sharing_an_msa_have_distinct_rule_keys() {
    let (mut snapshot, f) = fixture();
    variant_affix_sharing_main_msa(&mut snapshot, &f);
    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");

    let rule_keys: BTreeSet<&str> = output
        .compiled_mappings
        .iter()
        .filter(|m| {
            m.source_kind == "msa"
                && m.source_guid.as_deref() == Some(f.suffix_msa.as_str())
                && kind_of(&output, m) == "morph_rule"
        })
        .map(|m| key_of(&output, m))
        .collect();
    assert_eq!(rule_keys.len(), 2, "rule keys: {rule_keys:?}");
    assert!(rule_keys.iter().all(|key| key.starts_with("morph_rule:")));

    let allomorph_keys: BTreeSet<&str> = output
        .compiled_mappings
        .iter()
        .filter(|m| {
            m.source_kind == "allomorph"
                && kind_of(&output, m) == "allomorph"
                && matches!(
                    m.source_guid.as_deref(),
                    Some("allo-suffix" | "allo-variant-affix")
                )
        })
        .map(|m| key_of(&output, m))
        .collect();
    assert_eq!(
        allomorph_keys.len(),
        2,
        "allomorph keys: {allomorph_keys:?}"
    );
}

#[test]
fn variant_stem_with_two_inflection_types_has_two_entry_keys() {
    let (mut snapshot, f) = fixture();
    let infl_types = ["infl-plural", "infl-dual"];
    for guid in infl_types {
        snapshot
            .morphology
            .lex_entry_infl_types
            .push(LexEntryInflType {
                guid: guid.to_string(),
                name: guid.to_string(),
                abbreviation: guid.to_string(),
                gloss_prepend: String::new(),
                gloss_append: format!(".{guid}"),
                slots: Vec::new(),
                inflection_features: None,
            });
    }
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-variant".to_string(),
        citation_form: vec![ws("sen", "kumi")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-variant", MorphType::Stem, "kumi")],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![EntryRef::Variant {
            guid: "entryref-variant".to_string(),
            component_lexemes: vec![f.stem_entry.clone()],
            variant_entry_types: infl_types.iter().map(|g| g.to_string()).collect(),
        }],
    });
    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");

    let entry_keys: BTreeSet<&str> = output
        .compiled_mappings
        .iter()
        .filter(|m| {
            m.source_kind == "msa"
                && m.source_guid.as_deref() == Some(f.stem_msa.as_str())
                && kind_of(&output, m) == "lex_entry"
        })
        .map(|m| key_of(&output, m))
        .collect();
    assert_eq!(
        entry_keys.len(),
        3,
        "base stem plus two variants: {entry_keys:?}"
    );
    let variant_keys: BTreeSet<&str> = output
        .compiled_mappings
        .iter()
        .filter(|m| {
            m.source_kind == "entry"
                && m.source_guid.as_deref() == Some("entry-variant")
                && kind_of(&output, m) == "lex_entry"
        })
        .map(|m| key_of(&output, m))
        .collect();
    assert_eq!(
        variant_keys.len(),
        2,
        "variant entry keys: {variant_keys:?}"
    );
    assert!(variant_keys.iter().all(|key| key.starts_with("lex_entry:")));
}

#[test]
fn compiled_mapping_rows_equal_compiled_outputs() {
    let (mut snapshot, f) = fixture();
    variant_affix_sharing_main_msa(&mut snapshot, &f);
    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");

    let mut sources_by_key: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for row in &output.compiled_allomorph_order {
        let (Some(key), Some(source)) =
            (row_key(&output, row), row.source_allomorph_guid.as_deref())
        else {
            continue;
        };
        sources_by_key.entry(key).or_default().insert(source);
        assert!(
            output.compiled_mappings.iter().any(|m| {
                kind_of(&output, m) == "allomorph"
                    && key_of(&output, m) == key
                    && m.source_guid.as_deref() == Some(source)
            }),
            "order row {key} / {source} has no matching compiled mapping"
        );
    }
    assert!(
        sources_by_key.values().all(|sources| sources.len() == 1),
        "an output key names two source allomorphs: {sources_by_key:?}"
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
fn empty_template_slot_reaches_one_linguist_warning() {
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
        1,
        "the slot and its template attachment collapse to one linguist warning"
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

// --- 7. unsupported active phonological constructs refuse production ---------------------

#[test]
fn unsupported_active_metathesis_refuses_and_remains_measurable() {
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

    let production = compile_project_with(&snapshot, CompileOptions::default());
    let Err(GrammarError::Conversion(refused)) = production else {
        panic!("active restriction or rule must refuse if it cannot be preserved")
    };
    assert!(refused.issues.iter().any(|issue| issue.code
        == super::issue_codes::RULE_METATHESIS_UNSUPPORTED
        && issue.fatal
        && issue.source.is_some()));
    assert!(!refused.compiled_mappings.iter().any(|mapping| {
        mapping.source_kind == "phonologicalRule"
            && mapping.source_guid.as_deref() == Some("meta-1")
    }));

    let out = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
        },
    )
    .expect("measure-only records unsupported metathesis");
    assert!(
        out.issues
            .iter()
            .any(|i| i.code == super::issue_codes::RULE_METATHESIS_UNSUPPORTED && i.fatal),
        "expected a fatal RULE_METATHESIS_UNSUPPORTED issue; got {:?}",
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
            display_name: None,
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
            display_name: None,
            phonemes: vec!["ph-a".to_string()],
        });
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-c".to_string(),
            name: "C".to_string(),
            display_name: None,
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
        linguist_warnings(&warnings).is_empty(),
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

    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("clitic entries must not be a hard error");
    let grammar = &output.grammar;
    assert!(
        output.warnings.iter().all(|warning| {
            warning.code != super::issue_codes::ALLOMORPH_MORPH_TYPE_UNSUPPORTED_AS_RULE_FORM.wire()
        }),
        "clitics are implemented; no clitic warning expected, got {:?}",
        output.warnings
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
    let clitic_orders: Vec<_> = output
        .compiled_allomorph_order
        .iter()
        .filter(|row| {
            row.source_entry_guid.as_deref() == Some("entry-clitic")
                && row.source_msa_guid.as_deref() == Some("msa-clitic")
                && row.source_allomorph_guid.as_deref() == Some("allo-clitic")
                && bucket_of_row(&output, row) == "Clitics"
        })
        .collect();
    assert_eq!(clitic_orders.len(), 2);
    assert!(clitic_orders
        .iter()
        .all(|row| row.compiled_order == Some(0)));
    assert!(clitic_orders
        .iter()
        .any(|row| { row_key(&output, row).is_some_and(|key| key.starts_with("lex_entry:")) }));
    assert!(clitic_orders
        .iter()
        .any(|row| { row_key(&output, row).is_some_and(|key| key.starts_with("morph_rule:")) }));
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
                display_name: None,
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

    let (grammar, warnings, inventory, _) = compile_recording_ok(&snapshot);
    assert_eq!(grammar.entries[0].allomorphs.len(), 1);
    let key = InventoryKey::object(InventoryKind::Allomorph, "allo-stem");
    assert!(inventory.represented.contains(&key));
    assert!(!inventory.rejected.contains(&key));
    for letter in ["x", "y", "z"] {
        let warning = warnings
            .iter()
            .find(|warning| {
                warning.code == "provisional.letter"
                    && warning.message.contains(&format!("'{letter}'"))
            })
            .unwrap();
        assert_eq!(
            warning_metadata(warning).level,
            pg_snapshot::DiagnosticLevel::Info
        );
    }
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "xy\u{0001}z")];
    let error = compile_project_with(&snapshot, CompileOptions::default()).unwrap_err();
    assert!(error.issues().iter().any(|issue| issue.code
        == pg_snapshot::ImportWarningCode::SubstrateClassificationAmbiguous
        && issue.fatal
        && issue
            .source
            .as_ref()
            .is_some_and(|source| source.id == "allo-stem")));
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

    let (grammar, warnings) = compile_project(&snapshot).unwrap();
    assert_eq!(grammar.entries[0].allomorphs.len(), 1);
    assert!(warnings
        .iter()
        .any(|warning| warning.code == "provisional.letter"
            && warning_metadata(warning).level == pg_snapshot::DiagnosticLevel::Info));
    let form = "xy\u{0001}z";
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", form)];
    let (_, warnings, _, _) = compile_recording_ok(&snapshot);
    let warning = warnings
        .iter()
        .find(|warning| warning.code == super::issue_codes::ALLOMORPH_UNSEGMENTABLE.wire())
        .expect("the unsegmentable allomorph must produce a warning");

    assert!(warning.message.starts_with(&format!(
        "Allomorph '{form}' could not be segmented with this project's phonemes:"
    )));
    assert!(warning.message.contains("skipped"));
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
    assert_eq!(warning.subjects[0].name.as_deref(), Some(form));
    let advice = warning_guidance(warning).expect("owned advice");
    assert!(advice.contains(pg_snapshot::fieldworks_paths::LEXICON_EDIT));
    assert!(advice.contains("named in the finding"));
    assert!(advice.contains("unintended spelling"));
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
fn an_unresolved_environment_guid_on_a_root_allomorph_names_its_allomorph() {
    let (mut snapshot, _f) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0]
        .environments
        .push("dangling-env-guid".to_string());

    let (_grammar, warnings, inventory, issues) = compile_recording_ok(&snapshot);
    let environment_warnings: Vec<_> = warnings
        .iter()
        .filter(|w| w.code == super::issue_codes::ENVIRONMENT_UNRESOLVED.wire())
        .collect();
    assert_eq!(environment_warnings.len(), 1, "{warnings:?}");
    assert_eq!(
        environment_warnings[0]
            .subjects
            .iter()
            .map(|s| (s.class, s.guid.as_deref()))
            .collect::<Vec<_>>(),
        vec![(pg_snapshot::FwClass::MoForm, Some("allo-stem"))],
        "the linguist is pointed at the allomorph holding the dangling reference"
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
            .any(|i| i.code == super::issue_codes::ENVIRONMENT_UNRESOLVED && i.fatal),
        "expected a fatal ENVIRONMENT_UNRESOLVED issue; got {issues:?}"
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
    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("default compounds must compile");
    let synthesized_compound_rules = inventory
        .synthesized
        .iter()
        .filter(|k| k.kind == InventoryKind::CompoundRule)
        .count();
    assert_eq!(synthesized_compound_rules, 2);
    let default_mappings: Vec<_> = output
        .compiled_mappings
        .iter()
        .filter(|mapping| {
            mapping.source_kind == "synthetic"
                && kind_of(&output, mapping) == "compound_rule"
                && mapping.identity_quality == "synthetic"
        })
        .collect();
    assert_eq!(default_mappings.len(), 2);
    assert!(default_mappings
        .iter()
        .all(|mapping| mapping.source_guid.is_none()));
    for name in [
        "Default Left Head Compounding",
        "Default Right Head Compounding",
    ] {
        let prefix = format!("compound_rule:endo#{name}@");
        assert!(
            default_mappings
                .iter()
                .any(|mapping| key_of(&output, mapping).starts_with(&prefix)),
            "no output keyed {prefix}"
        );
    }

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
    let (grammar2, _warnings2, inventory2, _issues2) =
        compile_recording_ok(&snapshot_with_authored);
    let compound_sources: Vec<_> = grammar2
        .mrules
        .iter()
        .filter_map(|rule| match rule {
            MorphRuleDef::Compounding(def) => Some(def.source_guid.as_deref()),
            _ => None,
        })
        .collect();
    assert_eq!(
        compound_sources,
        vec![Some("cr-authored")],
        "an authored compound rule keeps its FieldWorks GUID for linking"
    );
    let synthesized_compound_rules_2 = inventory2
        .synthesized
        .iter()
        .filter(|k| k.kind == InventoryKind::CompoundRule)
        .count();
    assert_eq!(synthesized_compound_rules_2, 0);
}

#[test]
fn exocentric_compound_outputs_each_map_to_the_authored_rule() {
    let (mut snapshot, f) = fixture();
    snapshot
        .morphology
        .compound_rules
        .push(CompoundRule::Exocentric {
            guid: "compound-exocentric".to_string(),
            name: "Exocentric".to_string(),
            disabled: false,
            left: CompoundConstituentRequirement::default(),
            right: CompoundConstituentRequirement::default(),
            to: CompoundOutcome {
                part_of_speech: Some(f.noun_pos),
                ..CompoundOutcome::default()
            },
        });

    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("the exocentric compound must compile");
    let mappings: Vec<_> = output
        .compiled_mappings
        .iter()
        .filter(|mapping| {
            mapping.source_kind == "compoundRule"
                && mapping.source_guid.as_deref() == Some("compound-exocentric")
                && kind_of(&output, mapping) == "compound_rule"
        })
        .collect();

    assert_eq!(mappings.len(), 2);
    assert!(mappings
        .iter()
        .all(|mapping| mapping.identity_quality == "structural"));
    // Exocentric halves are two outputs by design, so the side names them and no collision suffix applies.
    let keys: BTreeSet<&str> = mappings.iter().map(|m| key_of(&output, m)).collect();
    assert!(keys
        .iter()
        .any(|key| key.starts_with("compound_rule:exo#compound-exocentric#left@")));
    assert!(keys
        .iter()
        .any(|key| key.starts_with("compound_rule:exo#compound-exocentric#right@")));
    assert!(
        keys.iter().all(|key| !key.contains('!')),
        "exocentric halves must not need a collision suffix: {keys:?}"
    );
}

#[test]
fn null_affix_output_has_synthetic_order_and_optional_slots_omit_it() {
    for optional in [false, true] {
        let (mut snapshot, f) = fixture();
        snapshot.morphology.parts_of_speech[0].affix_slots[0].optional = optional;
        snapshot
            .morphology
            .lex_entry_infl_types
            .push(LexEntryInflType {
                guid: "infl-null".to_string(),
                name: "Irregular plural".to_string(),
                abbreviation: "irr.pl".to_string(),
                gloss_prepend: String::new(),
                gloss_append: String::new(),
                slots: vec![f.slot],
                inflection_features: None,
            });

        let output = compile_project_with(&snapshot, CompileOptions::default())
            .expect("null-affix fixture must compile");
        let null_mappings: Vec<_> = output
            .compiled_mappings
            .iter()
            .filter(|mapping| {
                mapping.source_kind == "ruleFeature"
                    && mapping.source_guid.as_deref() == Some("infl-null")
                    && kind_of(&output, mapping) == "allomorph"
            })
            .collect();
        let null_orders: Vec<_> = output
            .compiled_allomorph_order
            .iter()
            .filter(|row| {
                row_key(&output, row).is_some_and(|key| key.contains("null-affix#infl-null"))
            })
            .collect();

        if optional {
            assert!(null_mappings.is_empty());
            assert!(null_orders.is_empty());
        } else {
            assert_eq!(null_mappings.len(), 1);
            assert_eq!(null_mappings[0].identity_quality, "structural");
            assert_eq!(null_orders.len(), 1);
            assert!(null_orders[0].source_entry_guid.is_none());
            assert!(null_orders[0].source_msa_guid.is_none());
            assert_eq!(null_orders[0].source_allomorph_guid, None);
            assert_eq!(bucket_of_row(&output, null_orders[0]), "Morphology");
            assert_eq!(null_orders[0].compiled_order, Some(0));
        }
    }
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

/// A bare `Circumfix`/`DiscontigPhrase`-typed allomorph must be selected then rejected, not left dangling.
#[test]
fn circumfix_typed_allomorph_outside_a_cross_product_is_selected_before_rejection() {
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
    assert!(
        linguist_warnings(&warnings).is_empty(),
        "a circumfix whole form is developer-only: {warnings:?}"
    );

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
        linguist_warnings(&warnings).is_empty(),
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
    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("the compacted rule fixture must compile");
    assert!(!output.compiled_mappings.iter().any(|mapping| {
        mapping.source_kind == "msa"
            && mapping.source_guid.as_deref() == Some("msa-orphan")
            && kind_of(&output, mapping) == "morph_rule"
    }));
    assert!(!output
        .compiled_allomorph_order
        .iter()
        .any(|row| { row.source_allomorph_guid.as_deref() == Some("allo-orphan") }));
}

/// An affix (not root) allomorph whose literal text cannot be segmented is a recall gap for that one allomorph, matching the root case pinned elsewhere.
#[test]
fn affix_allomorph_unsegmentable_text_is_a_recall_gap_not_a_project_refusal() {
    let (mut snapshot, _f) = fixture();
    snapshot.lexicon.entries[1].allomorphs[0].forms = vec![ws("sen", "qa")];

    let out = compile_project_with(&snapshot, CompileOptions::default())
        .expect("a provisional letter keeps the affix allomorph");
    assert!(out.grammar.mrules.iter().any(|rule| matches!(rule, MorphRuleDef::AffixProcess(definition) if !definition.allomorphs.is_empty())));
    let warning = out
        .warnings
        .iter()
        .find(|warning| warning.code == "provisional.letter" && warning.message.contains("'q'"))
        .unwrap();
    assert_eq!(
        warning_metadata(warning).level,
        pg_snapshot::DiagnosticLevel::Info
    );
    assert!(!out
        .inventory
        .inventory
        .rejected
        .contains(&InventoryKey::object(
            InventoryKind::Allomorph,
            "allo-suffix"
        )));
    snapshot.lexicon.entries[1].allomorphs[0].forms = vec![ws("sen", "q\u{0001}a")];
    let error = compile_project_with(&snapshot, CompileOptions::default()).unwrap_err();
    assert!(error.issues().iter().any(|issue| issue.code
        == pg_snapshot::ImportWarningCode::SubstrateClassificationAmbiguous
        && issue.fatal
        && issue
            .source
            .as_ref()
            .is_some_and(|source| source.id == "allo-suffix")));
    let out = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
        },
    )
    .unwrap();
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
            display_name: None,
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

/// A malformed active rewrite refuses production and remains visible in measurement.
#[test]
fn active_phonological_rule_build_failure_refuses_and_remains_measurable() {
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

    let production = compile_project_with(&snapshot, CompileOptions::default());
    let Err(GrammarError::Conversion(refused)) = production else {
        panic!("active restriction or rule must refuse if it cannot be preserved")
    };
    assert!(refused
        .issues
        .iter()
        .any(|issue| issue.code == super::issue_codes::RULE_BUILD_FAILED
            && issue.fatal
            && issue.source.is_some()));

    let out = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
        },
    )
    .expect("must still compile");
    assert!(
        out.issues
            .iter()
            .any(|i| i.code == super::issue_codes::RULE_BUILD_FAILED && i.fatal),
        "expected a fatal RULE_BUILD_FAILED issue; got {:?}",
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
            display_name: None,
            phonemes: vec!["ph-a".to_string()],
        });
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-last-unnamed".to_string(),
            name: String::new(),
            display_name: None,
            phonemes: vec!["ph-i".to_string()],
        });
    snapshot
        .phonology
        .natural_classes
        .push(SnapNaturalClass::Segments {
            guid: "nc-vowel".to_string(),
            name: "V".to_string(),
            display_name: None,
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
        linguist_warnings(&warnings).is_empty(),
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
    let warning = super::warnings::from_issues(&snapshot, std::slice::from_ref(compacted_issue));
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

    let any_key = InventoryKey::synthetic(InventoryKind::NaturalClass, "__any__");
    assert!(inventory.represented.contains(&any_key));

    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("the compacted class fixture must compile");
    assert!(!output.compiled_mappings.iter().any(|mapping| {
        mapping.source_kind == "naturalClass" && mapping.source_guid.as_deref() == Some("nc-orphan")
    }));
    assert!(output.compiled_mappings.iter().any(|mapping| {
        mapping.source_kind == "naturalClass"
            && mapping.source_guid.as_deref() == Some("nc-vowel")
            && mapping.identity_quality == "authored"
    }));
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
        linguist_warnings(&warnings).is_empty(),
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

/// Every represented `Msa`/`NaturalClass` key, synthetic ones too, matches a compiled atom.
#[test]
fn fixture_represented_msa_and_natural_class_atoms_match_the_final_grammar_exactly() {
    let (snapshot, _f) = fixture();
    let (grammar, warnings, inventory, _issues) = compile_recording_ok(&snapshot);
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    fn grammar_atom_key(k: &InventoryKey) -> Option<String> {
        match &k.identity {
            pg_snapshot::InventoryIdentity::Object { guid } => Some(guid.clone()),
            pg_snapshot::InventoryIdentity::Synthetic { key } => Some(key.clone()),
            _ => None,
        }
    }

    let represented_msas: std::collections::BTreeSet<String> = inventory
        .represented
        .iter()
        .filter(|k| k.kind == InventoryKind::Msa)
        .filter_map(grammar_atom_key)
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
        .filter_map(grammar_atom_key)
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
        linguist_warnings(&warnings).is_empty(),
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
        linguist_warnings(&warnings).is_empty(),
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
        linguist_warnings(&warnings).is_empty(),
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
    let CompileOptions { semantic_loss } = CompileOptions::default();
    assert_eq!(semantic_loss, SemanticLossPolicy::Refuse);

    let (snapshot, _f) = fixture();
    let out = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let CompileOutput {
        grammar,
        issues,
        warnings,
        substrate: _substrate,
        inventory,
        load_decisions: _,
        environment_resolutions: _,
        compiled_outputs: _,
        compiled_mappings: _,
        compiled_allomorph_order: _,
    } = out;
    assert_eq!(grammar.entries.len(), 1);
    assert!(issues.is_empty());
    assert!(warnings.is_empty());
    assert!(inventory.inventory.rejected.is_empty());
}

#[test]
fn compiled_lineage_and_order_follow_final_root_allomorphs() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries[0]
        .allomorphs
        .push(simple_allomorph("allo-stem-alt", MorphType::Stem, "kita"));

    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let entry = output
        .grammar
        .entries
        .iter()
        .find(|entry| entry.source_guid.as_deref() == Some(f.stem_entry.as_str()))
        .expect("the stem entry is represented");
    let order: Vec<_> = output
        .compiled_allomorph_order
        .iter()
        .filter(|row| {
            row.source_entry_guid.as_deref() == Some(f.stem_entry.as_str())
                && row.source_msa_guid.as_deref() == Some(f.stem_msa.as_str())
                && bucket_of_row(&output, row) == "Morphology"
        })
        .collect();

    assert_eq!(entry.allomorphs.len(), 2);
    assert_eq!(order.len(), 2);
    assert_eq!(order[0].compiled_order, Some(0));
    assert_eq!(order[1].compiled_order, Some(1));
    for guid in ["allo-stem", "allo-stem-alt"] {
        assert!(output.compiled_mappings.iter().any(|mapping| {
            mapping.source_kind == "allomorph"
                && mapping.source_guid.as_deref() == Some(guid)
                && kind_of(&output, mapping) == "allomorph"
        }));
    }
}

#[test]
fn circumfix_source_halves_map_only_to_final_compiled_products() {
    let (snapshot, _f) = circumfix_snapshot(&[], &[]);
    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let product_keys: std::collections::BTreeSet<_> = output
        .compiled_mappings
        .iter()
        .filter(|mapping| {
            mapping.source_kind == "allomorph"
                && matches!(
                    mapping.source_guid.as_deref(),
                    Some("allo-circ-prefix" | "allo-circ-suffix")
                )
                && kind_of(&output, mapping) == "allomorph"
        })
        .map(|mapping| key_of(&output, mapping))
        .collect();

    assert_eq!(
        product_keys.len(),
        1,
        "one prefix/suffix pair makes one product"
    );
    assert_eq!(
        output
            .compiled_mappings
            .iter()
            .filter(|mapping| {
                mapping.source_kind == "allomorph"
                    && matches!(
                        mapping.source_guid.as_deref(),
                        Some("allo-circ-prefix" | "allo-circ-suffix")
                    )
                    && kind_of(&output, mapping) == "allomorph"
            })
            .count(),
        2,
        "both source halves map to the final product"
    );
    for mapping in output.compiled_mappings.iter().filter(|mapping| {
        mapping.source_kind == "allomorph"
            && matches!(
                mapping.source_guid.as_deref(),
                Some("allo-circ-prefix" | "allo-circ-suffix")
            )
            && kind_of(&output, mapping) == "allomorph"
    }) {
        assert!(key_of(&output, mapping).contains("#allo"));
    }
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
    assert_eq!(
        out.issues.len(),
        warnings.len(),
        "issues {:?} warnings {warnings:?}",
        out.issues
    );
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
            && warning_metadata(warning).level == pg_snapshot::DiagnosticLevel::Warning
    }));

    let measured = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
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

#[test]
fn provisional_letter_keeps_the_missing_segment_allomorph() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];
    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert_eq!(output.grammar.entries[0].allomorphs.len(), 1);
    assert_eq!(output.substrate.inferred_segments[0].representation, "q");
    let warning = output
        .warnings
        .iter()
        .find(|warning| warning.code == "provisional.letter")
        .unwrap();
    assert_eq!(
        warning_metadata(warning).level,
        pg_snapshot::DiagnosticLevel::Info
    );
    assert!(warning.message.contains("'q'"));
}

#[test]
fn provisional_grapheme_keeps_a_symbol_without_ldml() {
    let (mut snapshot, _) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "ku§ma")];
    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert_eq!(output.grammar.entries.len(), 1);
    assert_eq!(output.substrate.inferred_segments[0].representation, "§");
}

#[test]
fn provisional_letter_keeps_the_allomorph_and_control_refusal_agrees_with_owner() {
    let (mut snapshot, _) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];
    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    let owner = InventoryKey::object(InventoryKind::Allomorph, "allo-stem");
    assert!(output.inventory.inventory.represented.contains(&owner));
    assert!(!output.inventory.inventory.rejected.contains(&owner));
    assert!(output
        .warnings
        .iter()
        .any(|warning| warning.code == "provisional.letter" && warning.message.contains("'q'")));
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "ku\u{0001}ma")];
    let error = compile_project_with(&snapshot, CompileOptions::default()).unwrap_err();
    assert!(error.issues().iter().any(|issue| issue.code
        == pg_snapshot::ImportWarningCode::SubstrateClassificationAmbiguous
        && issue.fatal
        && issue
            .source
            .as_ref()
            .is_some_and(|source| source.id == "allo-stem")));
    assert!(error.issues().iter().any(|issue| issue.code
        == super::issue_codes::ALLOMORPH_UNSEGMENTABLE
        && issue
            .source
            .as_ref()
            .is_some_and(|source| source.id == "allo-stem")));
}

/// Regression pin for a probe/builder segmenter mismatch: `substrate::complete`'s probe used to consult `segment_phonemes_only` (built for environment-string validation, which deliberately SKIPS Boundary-kind char defs), while the real owner (`lexicon::build_root_allomorph`) uses `segment_with_patterns`, whose literal-match loop accepts Segment AND Boundary. A literal authored boundary marker inside an ordinary root form used to misfire a false `substrate.position-unmapped`; the probe now shares `segment` (both kinds, no patterns) with the owners.
#[test]
fn a_literal_authored_boundary_marker_inside_a_root_form_is_not_a_false_substrate_refusal() {
    let (mut snapshot, _f) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "ku+ma")];

    let out = compile_project_with(&snapshot, CompileOptions::default())
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
fn add_front_feature_class_and_rewrite_rule(snapshot: &mut Snapshot) -> String {
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
            display_name: None,
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

/// Adds an authored phoneme with no phonological values.
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
fn provisional_and_authored_featureless_segments_keep_unspecified_phonological_lanes() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];
    let feature_guid = add_front_feature_class_and_rewrite_rule(&mut snapshot);

    let inferred = compile_project_with(&snapshot, CompileOptions::default())
        .expect("ordinary HC unspecified-feature semantics is defined");
    assert!(inferred
        .issues
        .iter()
        .any(|i| i.code == pg_snapshot::ImportWarningCode::ProvisionalLetter));

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
fn provisional_letter_finding_names_the_letter_and_has_guidance() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".to_string());
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "quma")];
    add_front_feature_class_and_rewrite_rule(&mut snapshot);

    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("the inferred segment remains representable");
    let warning = output
        .warnings
        .iter()
        .find(|warning| warning.code == "provisional.letter")
        .expect("the provisional definition is reported");
    let finding = crate::grammar_health::GrammarHealthDiagnostic::from_import_warning(warning);
    assert_eq!(finding.level, pg_snapshot::DiagnosticLevel::Info);
    assert!(finding.message.contains("'q'"), "{}", finding.message);
    assert!(
        finding.message.contains("isn't defined"),
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
fn empty_stem_bucket_is_reported_with_a_linguist_warning() {
    let (mut snapshot, _) = fixture();
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "\u{0001}")];
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;

    let output = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
        },
    )
    .expect("an entry with no loadable allomorph is dropped with a warning");
    assert!(output
        .warnings
        .iter()
        .any(|warning| warning.code == super::issue_codes::MSA_NO_ALLOMORPHS.wire()));
    assert!(output
        .issues
        .iter()
        .any(|issue| issue.code == super::issue_codes::MSA_NO_ALLOMORPHS));
}

#[test]
fn empty_affix_form_reaches_a_linguist_warning() {
    let (mut snapshot, _) = fixture();
    snapshot.lexicon.entries[1].allomorphs[0].forms.clear();

    let output = compile_project_with(&snapshot, CompileOptions::default())
        .expect("an empty affix form is dropped with a warning");
    assert!(output
        .warnings
        .iter()
        .any(|warning| warning.code == super::issue_codes::ALLOMORPH_NOT_RULE_FORM.wire()));
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
            display_name: None,
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

#[test]
fn provisional_diacritic_mid_word_keeps_the_whole_grapheme() {
    let (mut snapshot, _) = fixture();
    snapshot.phonology.phonemes.push(phoneme("ph-b", "b"));
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "sáb")];
    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert_eq!(output.grammar.entries[0].allomorphs.len(), 1);
    assert_eq!(output.substrate.inferred_segments.len(), 1);
    assert_eq!(
        output.substrate.inferred_segments[0].representation,
        crate::nfd::nfd("á")
    );
    assert!(output.substrate.ambiguous_uses.is_empty());
    assert!(output
        .warnings
        .iter()
        .any(|warning| warning.code == "provisional.letter"));
}

#[test]
fn provisional_diacritic_word_final_keeps_the_whole_grapheme() {
    let (mut snapshot, _) = fixture();
    snapshot.phonology.phonemes.push(phoneme("ph-b", "b"));
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "sá")];
    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert_eq!(output.grammar.entries[0].allomorphs.len(), 1);
    assert_eq!(output.substrate.inferred_segments.len(), 1);
    assert_eq!(
        output.substrate.inferred_segments[0].representation,
        crate::nfd::nfd("á")
    );
    assert!(output.substrate.ambiguous_uses.is_empty());
    assert!(output
        .warnings
        .iter()
        .any(|warning| warning.code == "provisional.letter"));
}

#[test]
fn provisional_diacritic_word_final_is_enabled_for_hc_projects() {
    let (mut snapshot, _) = fixture();
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::Hc;
    snapshot.phonology.phonemes.push(phoneme("ph-b", "b"));
    snapshot.lexicon.entries[0].allomorphs[0].forms = vec![ws("sen", "sá")];
    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert_eq!(output.grammar.entries[0].allomorphs.len(), 1);
    assert_eq!(output.substrate.inferred_segments.len(), 1);
    assert_eq!(
        output.substrate.inferred_segments[0].representation,
        crate::nfd::nfd("á")
    );
    assert!(output.substrate.ambiguous_uses.is_empty());
    assert!(output
        .warnings
        .iter()
        .any(|warning| warning.code == "provisional.letter"));
}

/// Selected environment literals participate in substrate completion before pattern compilation.
#[test]
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
    let constrained = pg_parse::Morpher::new(&out.grammar, 10_000).parse_word("kumata");
    assert!(
        constrained.analyses.is_empty(),
        "a suffix requiring q must not apply after a"
    );
    assert!(!constrained.capped && !constrained.invalid_shape);
    snapshot.lexicon.entries[1].allomorphs[0]
        .environments
        .clear();
    let control = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    let unconstrained = pg_parse::Morpher::new(&control.grammar, 10_000).parse_word("kumata");
    assert!(
        !unconstrained.analyses.is_empty(),
        "removing the restriction must change the parse effect"
    );
}

#[test]
fn unsupported_provenance_never_claims_clean_conversion() {
    for (version, status) in [
        (3, SourceInventoryStatus::Synthetic),
        (0, SourceInventoryStatus::ImportedComplete),
        (1, SourceInventoryStatus::Unknown),
    ] {
        let (mut snapshot, _) = fixture();
        snapshot.conversion_provenance.schema_version = version;
        snapshot.conversion_provenance.source_inventory_status = status;
        let error = compile_project_with(&snapshot, CompileOptions::default())
            .expect_err("invalid provenance must refuse production conversion");
        let GrammarError::Conversion(error) = error else {
            panic!("expected conversion refusal")
        };
        assert!(error
            .issues
            .iter()
            .any(|issue| issue.fatal && issue.code == super::issues::SOURCE_PROVENANCE_UNKNOWN));
        let measured = compile_project_with(
            &snapshot,
            CompileOptions {
                semantic_loss: SemanticLossPolicy::MeasureOnly,
            },
        )
        .expect("measurement remains explicit");
        assert!(measured
            .issues
            .iter()
            .any(|issue| issue.fatal && issue.code == super::issues::SOURCE_PROVENANCE_UNKNOWN));
    }
}

#[test]
fn unreferenced_invalid_environment_does_not_refuse_production() {
    let (mut snapshot, _) = fixture();
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "unused-bad".into(),
            name: String::new(),
            representation: "invalid".into(),
        });
    let out = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert!(!out
        .issues
        .iter()
        .any(|issue| issue.code == super::issue_codes::ENVIRONMENT_INVALID));
}

#[test]
fn unreachable_affix_environment_is_nonfatal_and_root_positions_are_not_inferred() {
    let (mut snapshot, _) = fixture();
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "orphan-env".into(),
            name: String::new(),
            representation: "/_[Missing]".into(),
        });
    let mut orphan = snapshot.lexicon.entries[1].clone();
    orphan.guid = "entry-orphan".into();
    orphan.allomorphs[0].guid = "allo-orphan".into();
    orphan.allomorphs[0].environments = vec!["orphan-env".into()];
    if let Msa::Inflectional { guid, slots, .. } = &mut orphan.msas[0] {
        *guid = "msa-orphan".into();
        *slots = vec!["slot-never-templated".into()];
    }
    snapshot.lexicon.entries.push(orphan);
    let out = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert!(out.issues.iter().any(
        |issue| issue.code == super::issue_codes::ENVIRONMENT_INVALID
            && !issue.fatal
            && issue.class == IssueClass::UnreachableInGrammar
    ));
    snapshot.morphology.parser_parameters.active_parser = ActiveParser::XAmple;
    snapshot.project.exemplar_characters.push("q".into());
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "unused-position".into(),
            name: String::new(),
            representation: "/q_".into(),
        });
    snapshot.lexicon.entries[0].allomorphs[0]
        .positions
        .push("unused-position".into());
    let out = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert!(
        out.substrate.inferred_segments.is_empty(),
        "root positions are not an execution input"
    );
}

#[test]
fn discarded_affix_environment_does_not_refuse_a_surviving_sibling() {
    let (mut snapshot, _) = fixture();
    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "discarded-env".into(),
            name: String::new(),
            representation: "/_[Missing]".into(),
        });
    let mut discarded = snapshot.lexicon.entries[1].allomorphs[0].clone();
    discarded.guid = "allo-unsegmentable".into();
    discarded.forms = vec![ws("sen", "qa")];
    discarded.environments = vec!["discarded-env".into()];
    snapshot.lexicon.entries[1].allomorphs.push(discarded);
    let out = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    assert!(out.issues.iter().any(
        |issue| issue.code == super::issue_codes::ENVIRONMENT_INVALID
            && !issue.fatal
            && issue.class == IssueClass::InvalidSource
    ));
    let warning = out
        .warnings
        .iter()
        .find(|warning| warning.code == "provisional.letter" && warning.message.contains("'q'"))
        .unwrap();
    assert_eq!(
        warning_metadata(warning).level,
        pg_snapshot::DiagnosticLevel::Info
    );
    let affix = out
        .grammar
        .mrules
        .iter()
        .find_map(|rule| match rule {
            MorphRuleDef::AffixProcess(definition) => Some(definition),
            _ => None,
        })
        .unwrap();
    assert_eq!(affix.allomorphs.len(), 2);
    assert!(affix
        .allomorphs
        .iter()
        .all(|allomorph| allomorph.environments.is_empty()));
    let parsed = pg_parse::Morpher::new(&out.grammar, 100_000).parse_word("kumata");
    assert!(
        !parsed.analyses.is_empty(),
        "valid sibling must remain executable"
    );
    snapshot.lexicon.entries[1].allomorphs[1].forms = vec![ws("sen", "q\u{0001}a")];
    let refused = compile_project_with(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
        },
    )
    .unwrap();
    assert!(refused.issues.iter().any(|issue| issue.code
        == super::issue_codes::ENVIRONMENT_INVALID
        && !issue.fatal
        && issue.class == IssueClass::UnreachableInGrammar));
    assert!(refused.issues.iter().any(|issue| issue.code
        == pg_snapshot::ImportWarningCode::SubstrateClassificationAmbiguous
        && issue.fatal
        && issue
            .source
            .as_ref()
            .is_some_and(|source| source.id == "allo-unsegmentable")));
}

#[test]
fn imported_validated_and_compiled_fixture_warnings_have_reportable_subjects() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pg-fwdata/tests/data/fixture.fwdata");
    let (snapshot, imported) = pg_fwdata::import_file(&path).expect("fixture imports");
    let mut inputs = imported.warnings;
    inputs.extend(pg_snapshot::validate::validate(&snapshot));
    let output = super::compile_project_with_options_and_import_warnings(
        &snapshot,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
        },
        inputs,
    )
    .expect("fixture compiles permissively");
    assert!(
        !output.warnings.is_empty(),
        "fixture must exercise warnings"
    );
    for warning in &output.warnings {
        assert!(
            !warning.subjects.is_empty(),
            "{}: {}",
            warning.code,
            warning.message
        );
        for subject in &warning.subjects {
            assert_eq!(
                subject.class == pg_snapshot::FwClass::Project,
                subject.status == pg_snapshot::FwSubjectStatus::ProjectSettings,
                "{warning:?}"
            );
            if subject.status == pg_snapshot::FwSubjectStatus::UnresolvedReference {
                assert!(
                    subject
                        .guid
                        .as_deref()
                        .is_some_and(|id| !id.trim().is_empty()),
                    "{warning:?}"
                );
                assert!(subject.opens_in.is_none(), "{warning:?}");
            }
        }
    }
    let report = crate::grammar_health::GrammarHealthReport::new(
        output
            .warnings
            .iter()
            .map(crate::grammar_health::GrammarHealthDiagnostic::from_import_warning)
            .collect(),
    )
    .expect("all producer warnings are reportable")
    .with_fieldworks_project(crate::grammar_health::FieldWorksProject {
        name: Some(snapshot.project.name.clone()),
        source: Some(crate::grammar_health::FieldWorksProjectSource::Argument),
    });
    assert_eq!(
        crate::grammar_health::GrammarHealthReport::from_json(&report.to_json().unwrap()).unwrap(),
        report
    );
}

#[test]
fn parser_setting_projection_preserves_distinct_named_settings() {
    let (snapshot, _) = fixture();
    let code = pg_snapshot::ImportWarningCode::FwdataInvalidParserParameter;
    let imported: Vec<_> = ["XAmple.MaxNulls", "XAmple.MaxPrefixes"]
        .into_iter()
        .map(|field| {
            pg_snapshot::Warning::new(code.clone(), "Malformed setting.").with_subject(
                pg_snapshot::FwObjectRef::new(pg_snapshot::FwClass::Project)
                    .project_settings()
                    .name("Demo")
                    .field(field),
            )
        })
        .collect();
    let issue = ConversionIssue {
        code,
        class: IssueClass::MalformedSource,
        source: None,
        fatal: false,
        message: "Malformed setting projection.".to_string(),
    };
    assert!(super::warnings::from_import_issues(
        &snapshot,
        std::slice::from_ref(&issue),
        &imported
    )
    .is_empty());
    assert_eq!(super::warnings::deduplicate(imported.clone()), imported);
    let projected = super::warnings::from_import_issues(&snapshot, &[issue], &[]);
    assert_eq!(
        projected[0].subjects[0].status,
        pg_snapshot::FwSubjectStatus::ProjectSettings
    );
}

#[test]
fn rejected_compound_attachment_names_its_owner_and_is_reportable() {
    let (mut snapshot, _) = fixture();
    let rule_guid = "66666666-6666-6666-6666-666666666666";
    snapshot
        .morphology
        .compound_rules
        .push(CompoundRule::Exocentric {
            guid: rule_guid.to_string(),
            name: "Owner title".to_string(),
            disabled: false,
            left: CompoundConstituentRequirement::default(),
            right: CompoundConstituentRequirement::default(),
            to: CompoundOutcome {
                part_of_speech: Some("missing-pos-guid".to_string()),
                ..CompoundOutcome::default()
            },
        });

    let (_grammar, warnings, _inventory, _issues) = compile_recording_ok(&snapshot);
    let warning = warnings
        .iter()
        .find(|warning| warning.code == super::issue_codes::COMPOUND_SIDE_POS_UNRESOLVED.wire())
        .expect("the unresolved compound output POS must be diagnosed");
    let owner = warning
        .subjects
        .first()
        .expect("attachment warning has owner");
    assert_eq!(owner.class, pg_snapshot::FwClass::MoCompoundRule);
    assert_eq!(owner.guid.as_deref(), Some(rule_guid));
    assert_eq!(owner.name.as_deref(), Some("Owner title"));

    let report = crate::grammar_health::GrammarHealthReport::new(vec![
        crate::grammar_health::GrammarHealthDiagnostic::from_import_warning(warning),
    ])
    .expect("owner-backed attachment warning is reportable");
    assert_eq!(
        report.diagnostics()[0].subjects[0].guid.as_deref(),
        Some(rule_guid)
    );
}

#[test]
fn unresolved_feature_constraint_retains_its_guid_in_a_reportable_subject() {
    let (snapshot, _) = fixture();
    let guid = "77777777-7777-7777-7777-777777777777";
    let key = InventoryKey::object(InventoryKind::FeatureConstraint, guid);
    let source = super::warnings::source_for_key(&snapshot, &key)
        .expect("feature-constraint identity is retained");
    assert_eq!(source.kind, pg_snapshot::FwClass::Unknown);
    assert_eq!(source.id, guid);

    let issue = ConversionIssue {
        code: pg_snapshot::ImportWarningCode::FeatureConstraintUnresolved,
        class: IssueClass::InvalidSource,
        source: Some(source),
        fatal: true,
        message: "feature constraint does not resolve".to_string(),
    };
    let warning = super::warnings::from_issue(&snapshot, &issue);
    let report = crate::grammar_health::GrammarHealthReport::new(vec![
        crate::grammar_health::GrammarHealthDiagnostic::from_import_warning(&warning),
    ])
    .expect("unresolved-reference subject with the original GUID is reportable");
    let subject = &report.diagnostics()[0].subjects[0];
    assert_eq!(
        subject.status,
        pg_snapshot::FwSubjectStatus::UnresolvedReference
    );
    assert_eq!(subject.guid.as_deref(), Some(guid));
}

#[test]
fn feature_constraint_feature_failure_names_the_live_constraint_and_field() {
    let (snapshot, _) = fixture();
    let issue = ConversionIssue {
        code: pg_snapshot::ImportWarningCode::FeatureConstraintPhonFeatureUnresolved,
        class: IssueClass::InvalidSource,
        source: Some(pg_snapshot::SourceRef {
            kind: pg_snapshot::FwClass::Unknown,
            id: "77777777-7777-7777-7777-777777777777".to_string(),
        }),
        fatal: true,
        message: "constraint's Feature does not resolve".to_string(),
    };
    let warning = super::warnings::from_issue(&snapshot, &issue);
    let subject = &warning.subjects[0];
    assert_eq!(subject.status, pg_snapshot::FwSubjectStatus::Object);
    assert_eq!(subject.source_class.as_deref(), Some("PhFeatureConstraint"));
    assert_eq!(subject.field.as_deref(), Some("Feature"));
    assert!(crate::grammar_health::GrammarHealthReport::new(vec![
        crate::grammar_health::GrammarHealthDiagnostic::from_import_warning(&warning),
    ])
    .is_ok());
}

#[test]
fn missing_source_guid_does_not_publish_the_importers_synthetic_id() {
    let (snapshot, _) = fixture();
    let issue = ConversionIssue {
        code: pg_snapshot::ImportWarningCode::InvalidSourceMissingGuid,
        class: IssueClass::InvalidSource,
        source: Some(pg_snapshot::SourceRef {
            kind: pg_snapshot::FwClass::LexEntry,
            id: "rt#7".to_string(),
        }),
        fatal: true,
        message: "rt#7 has no GUID".to_string(),
    };
    let warning = super::warnings::from_issue(&snapshot, &issue);
    assert_eq!(warning.subjects[0].guid, None);
    assert!(warning.message.contains("rt#7"));
    let report = crate::grammar_health::GrammarHealthReport::new(vec![
        crate::grammar_health::GrammarHealthDiagnostic::from_import_warning(&warning),
    ])
    .unwrap();
    let value = serde_json::to_value(report).unwrap();
    assert_eq!(
        value["diagnostics"][0]["subjects"][0]["fieldworks"]["reason"],
        "guid_not_recorded"
    );
}

#[test]
fn rejected_entry_expansion_retains_its_owner_in_a_reportable_subject() {
    let (snapshot, _) = fixture();
    let entry = &snapshot.lexicon.entries[0];
    let key = InventoryKey::expansion(
        InventoryKind::Entry,
        entry.guid.clone(),
        vec!["generated-variant".to_string()],
        "variant",
    );
    let source = super::warnings::source_for_key(&snapshot, &key).unwrap();
    assert_eq!(source.kind, pg_snapshot::FwClass::LexEntry);
    assert_eq!(source.id, entry.guid);
    let issue = ConversionIssue {
        code: pg_snapshot::ImportWarningCode::MsaBuildFailed,
        class: IssueClass::InvalidSource,
        source: Some(source),
        fatal: false,
        message: "variant feature structure failed".to_string(),
    };
    let warning = super::warnings::from_issue(&snapshot, &issue);
    let report = crate::grammar_health::GrammarHealthReport::new(vec![
        crate::grammar_health::GrammarHealthDiagnostic::from_import_warning(&warning),
    ])
    .expect("expanded objects retain their source owner");
    assert_eq!(
        report.diagnostics()[0].subjects[0].guid.as_deref(),
        Some(entry.guid.as_str())
    );
}

#[test]
fn prerelease_missing_template_slots_have_owner_fields_and_unavailable_targets() {
    for field in ["PrefixSlots", "SuffixSlots"] {
        let mut snapshot: Snapshot = serde_json::from_str(include_str!(
            "../../../../../docs/formats/examples/trace-details-v2-sample.snapshot.json"
        ))
        .unwrap();
        let baseline = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        let missing = "11111111-1111-1111-1111-111111111111";
        let template = &mut snapshot.morphology.parts_of_speech[0].affix_templates[0];
        let owner_guid = template.guid.clone();
        if field == "PrefixSlots" {
            template.prefix_slots.push(missing.into());
        } else {
            template.suffix_slots.push(missing.into());
        }
        let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
        assert_grammars_equal(&baseline.grammar, &output.grammar);
        let missing_object = InventoryKey::object(InventoryKind::TemplateSlot, missing);
        assert!(!output
            .inventory
            .inventory
            .authored
            .contains(&missing_object));
        let warnings: Vec<_> = output
            .warnings
            .iter()
            .filter(|w| w.code == super::issue_codes::TEMPLATE_SLOT_UNRESOLVED.wire())
            .collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].subjects.len(), 2);
        let report = crate::grammar_health::GrammarHealthReport::new(
            warnings
                .iter()
                .map(|w| crate::grammar_health::GrammarHealthDiagnostic::from_import_warning(w))
                .collect(),
        )
        .unwrap()
        .with_fieldworks_project(crate::grammar_health::FieldWorksProject {
            name: Some("Demo".into()),
            source: Some(crate::grammar_health::FieldWorksProjectSource::Argument),
        });
        let subjects: Vec<_> = report
            .diagnostics()
            .iter()
            .flat_map(|d| &d.subjects)
            .collect();
        assert!(subjects
            .iter()
            .any(|s| s.guid.as_deref() == Some(owner_guid.as_str())
                && s.field.as_deref() == Some(field)
                && s.status == pg_snapshot::FwSubjectStatus::Object));
        let targets: Vec<_> = subjects
            .iter()
            .filter(|s| s.guid.as_deref() == Some(missing))
            .collect();
        assert!(!targets.is_empty());
        assert!(targets
            .iter()
            .all(|s| s.status == pg_snapshot::FwSubjectStatus::UnresolvedReference));
        let json: serde_json::Value = serde_json::from_str(&report.to_json().unwrap()).unwrap();
        for diagnostic in json["diagnostics"].as_array().unwrap() {
            for subject in diagnostic["subjects"].as_array().unwrap() {
                if subject["guid"] == missing {
                    assert_eq!(subject["fieldworks"]["status"], "unavailable");
                    assert!(subject["fieldworks"].get("url").is_none());
                }
            }
        }
    }
}

#[test]
fn prerelease_missing_template_slot_in_both_fields_keeps_distinct_facts() {
    let mut snapshot: Snapshot = serde_json::from_str(include_str!(
        "../../../../../docs/formats/examples/trace-details-v2-sample.snapshot.json"
    ))
    .unwrap();
    let template = &mut snapshot.morphology.parts_of_speech[0].affix_templates[0];
    let missing = "11111111-1111-1111-1111-111111111111";
    template.prefix_slots.push(missing.into());
    template.suffix_slots.push(missing.into());
    let output = compile_project_with(&snapshot, CompileOptions::default()).unwrap();
    let warnings: Vec<_> = output
        .warnings
        .iter()
        .filter(|w| w.code == super::issue_codes::TEMPLATE_SLOT_UNRESOLVED.wire())
        .collect();
    assert_eq!(warnings.len(), 2);
    let mut fields: Vec<_> = warnings
        .iter()
        .map(|w| w.subjects[0].field.as_deref().unwrap())
        .collect();
    fields.sort();
    assert_eq!(fields, ["PrefixSlots", "SuffixSlots"]);
    assert!(warnings.iter().all(|w| w.subjects.len() == 2
        && w.subjects[1].guid.as_deref() == Some(missing)
        && w.subjects[1].status == pg_snapshot::FwSubjectStatus::UnresolvedReference));
}

/// Every compiled output names a distinct key, the one spelling every consumer joins on.
fn assert_output_keys_unique(output: &CompileOutput) {
    let keys: BTreeSet<&str> = output
        .compiled_outputs
        .iter()
        .map(|out| out.key.as_str())
        .collect();
    assert_eq!(
        keys.len(),
        output.compiled_outputs.len(),
        "compiled output keys must be unique"
    );
}

/// The lexical-entry output keys a source entry maps to.
fn entry_output_keys<'a>(output: &'a CompileOutput, entry_guid: &str) -> BTreeSet<&'a str> {
    output
        .compiled_mappings
        .iter()
        .filter(|m| {
            m.source_kind == "entry"
                && m.source_guid.as_deref() == Some(entry_guid)
                && kind_of(output, m) == "lex_entry"
        })
        .map(|m| key_of(output, m))
        .collect()
}

#[test]
fn duplicate_variant_inflection_types_keep_unique_output_keys() {
    let (mut snapshot, f) = fixture();
    snapshot
        .morphology
        .lex_entry_infl_types
        .push(LexEntryInflType {
            guid: "infl-plural".to_string(),
            name: "plural".to_string(),
            abbreviation: "pl".to_string(),
            gloss_prepend: String::new(),
            gloss_append: ".pl".to_string(),
            slots: Vec::new(),
            inflection_features: None,
        });
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-variant".to_string(),
        citation_form: vec![ws("sen", "kumi")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-variant", MorphType::Stem, "kumi")],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![EntryRef::Variant {
            guid: "entryref-variant".to_string(),
            component_lexemes: vec![f.stem_entry.clone()],
            variant_entry_types: vec!["infl-plural".to_string(), "infl-plural".to_string()],
        }],
    });
    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");

    assert_output_keys_unique(&output);
    assert_eq!(
        entry_output_keys(&output, "entry-variant").len(),
        2,
        "each repeated inflection type is its own lexical entry output"
    );
}

#[test]
fn repeated_variant_component_keeps_unique_output_keys() {
    let (mut snapshot, f) = fixture();
    snapshot.lexicon.entries.push(LexEntry {
        guid: "entry-variant".to_string(),
        citation_form: vec![ws("sen", "kumi")],
        lexeme_morph_type: MorphType::Stem,
        allomorphs: vec![simple_allomorph("allo-variant", MorphType::Stem, "kumi")],
        msas: Vec::new(),
        senses: Vec::new(),
        entry_refs: vec![EntryRef::Variant {
            guid: "entryref-variant".to_string(),
            component_lexemes: vec![f.stem_entry.clone(), f.stem_entry.clone()],
            variant_entry_types: Vec::new(),
        }],
    });
    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");

    assert_output_keys_unique(&output);
    assert_eq!(
        entry_output_keys(&output, "entry-variant").len(),
        2,
        "a component named twice is two lexical entry outputs"
    );
}

fn endocentric_rule(guid: &str, name: &str) -> CompoundRule {
    CompoundRule::Endocentric {
        guid: guid.to_string(),
        name: name.to_string(),
        disabled: false,
        head_last: false,
        left: CompoundConstituentRequirement::default(),
        right: CompoundConstituentRequirement::default(),
        overriding: CompoundOutcome::default(),
    }
}

fn compound_keys_for(output: &CompileOutput, rule_guid: &str) -> Vec<String> {
    let prefix = format!("compound_rule:endo#{rule_guid}@");
    output
        .compiled_outputs
        .iter()
        .filter(|out| out.key.starts_with(&prefix))
        .map(|out| out.key.clone())
        .collect()
}

#[test]
fn same_named_compound_rules_have_distinct_output_keys() {
    let (mut snapshot, _f) = fixture();
    snapshot
        .morphology
        .compound_rules
        .push(endocentric_rule("cr-one", "Same"));
    snapshot
        .morphology
        .compound_rules
        .push(endocentric_rule("cr-two", "Same"));
    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");

    assert_output_keys_unique(&output);
    assert_eq!(compound_keys_for(&output, "cr-one").len(), 1);
    assert_eq!(compound_keys_for(&output, "cr-two").len(), 1);
}

#[test]
fn renaming_a_compound_rule_keeps_its_output_key() {
    let key_under = |name: &str| {
        let (mut snapshot, _f) = fixture();
        snapshot
            .morphology
            .compound_rules
            .push(endocentric_rule("cr-stable", name));
        let output =
            compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
        let keys = compound_keys_for(&output, "cr-stable");
        assert_eq!(keys.len(), 1, "one output for the rule under {name}");
        keys[0].clone()
    };
    assert_eq!(key_under("Before"), key_under("After"));
}

#[test]
fn allomorph_conditioning_flags() {
    let (mut snapshot, _f) = fixture();
    // A twin with the same gates (none) shares the signature; an environment changes conditioning, not gates.
    snapshot
        .lexicon
        .entries
        .iter_mut()
        .find(|entry| entry.allomorphs.iter().any(|a| a.guid == "allo-suffix"))
        .expect("the fixture's suffix entry owns allo-suffix")
        .allomorphs
        .push(simple_allomorph(
            "allo-suffix-twin",
            MorphType::Suffix,
            "ku",
        ));
    let allomorph_output = |output: &CompileOutput, guid: &str| {
        output
            .compiled_mappings
            .iter()
            .find(|m| {
                m.source_kind == "allomorph"
                    && m.source_guid.as_deref() == Some(guid)
                    && kind_of(output, m) == "allomorph"
            })
            .map(|m| output.compiled_outputs[m.output_id as usize - 1].clone())
            .expect("the allomorph must have a compiled output")
    };

    let plain = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let plain_suffix = allomorph_output(&plain, "allo-suffix");
    let plain_flags = plain_suffix
        .conditioning
        .expect("an allomorph output carries its conditioning");
    assert!(
        plain_flags.is_unconditioned,
        "a bare suffix is unconditioned"
    );
    assert!(!plain_flags.has_phone_condition);
    assert_eq!(plain_flags.realization_kind, "segments");
    let twin_flags = allomorph_output(&plain, "allo-suffix-twin")
        .conditioning
        .expect("the twin carries conditioning");
    assert_eq!(twin_flags.gate_signature, plain_flags.gate_signature);

    snapshot
        .phonology
        .environments
        .push(pg_snapshot::phonology::Environment {
            guid: "env-after-t".into(),
            name: String::new(),
            representation: "/t_".into(),
        });
    let suffix_entry = snapshot
        .lexicon
        .entries
        .iter_mut()
        .find(|entry| entry.allomorphs.iter().any(|a| a.guid == "allo-suffix"))
        .expect("the fixture's suffix entry owns allo-suffix");
    suffix_entry
        .allomorphs
        .iter_mut()
        .find(|a| a.guid == "allo-suffix")
        .expect("allo-suffix")
        .environments
        .push("env-after-t".into());
    let conditioned =
        compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let flags = allomorph_output(&conditioned, "allo-suffix")
        .conditioning
        .expect("conditioning");
    assert!(
        flags.has_phone_condition,
        "an environment conditions the allomorph"
    );
    assert!(!flags.is_unconditioned);
    assert_eq!(flags.gate_signature, plain_flags.gate_signature);
}

fn allomorph_output_for(output: &CompileOutput, guid: &str) -> crate::compile::CompiledOutput {
    output
        .compiled_mappings
        .iter()
        .find(|m| {
            m.source_kind == "allomorph"
                && m.source_guid.as_deref() == Some(guid)
                && kind_of(output, m) == "allomorph"
        })
        .map(|m| output.compiled_outputs[m.output_id as usize - 1].clone())
        .expect("the allomorph must have a compiled output")
}

#[test]
fn gated_allomorph_has_morph_gate_and_a_canonical_gate_signature() {
    let (mut snapshot, _f) = fixture();
    for guid in ["class-a", "class-b"] {
        snapshot.morphology.parts_of_speech[0]
            .inflection_classes
            .push(InflectionClass {
                guid: guid.to_string(),
                name: guid.to_string(),
                abbreviation: guid.to_string(),
                children: Vec::new(),
            });
    }
    let suffix_entry = snapshot
        .lexicon
        .entries
        .iter_mut()
        .find(|entry| entry.allomorphs.iter().any(|a| a.guid == "allo-suffix"))
        .expect("the fixture's suffix entry owns allo-suffix");
    let gated = |guid: &str, class: &str| {
        let mut allo = simple_allomorph(guid, MorphType::Suffix, "ku");
        allo.inflection_classes = vec![class.to_string()];
        allo
    };
    suffix_entry
        .allomorphs
        .iter_mut()
        .find(|a| a.guid == "allo-suffix")
        .expect("allo-suffix")
        .inflection_classes = vec!["class-a".to_string()];
    suffix_entry
        .allomorphs
        .push(gated("allo-gate-twin", "class-a"));
    suffix_entry
        .allomorphs
        .push(gated("allo-gate-other", "class-b"));

    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let gated_flags = allomorph_output_for(&output, "allo-suffix")
        .conditioning
        .expect("conditioning");
    assert!(
        gated_flags.has_morph_gate,
        "an inflection class gates the allomorph"
    );
    assert!(!gated_flags.is_unconditioned);
    assert!(
        gated_flags
            .gate_signature
            .strip_prefix("mpr=")
            .and_then(|rest| rest.split(';').next())
            .is_some_and(|ids| !ids.is_empty() && ids.split(',').all(|id| id.parse::<u8>().is_ok())),
        "the gate names its MPR: {}",
        gated_flags.gate_signature
    );
    let twin = allomorph_output_for(&output, "allo-gate-twin")
        .conditioning
        .expect("conditioning");
    let other = allomorph_output_for(&output, "allo-gate-other")
        .conditioning
        .expect("conditioning");
    assert_eq!(gated_flags.gate_signature, twin.gate_signature);
    assert_ne!(gated_flags.gate_signature, other.gate_signature);
}

#[test]
fn template_slot_surface_ordinals_count_outward_from_the_stem() {
    let (mut snapshot, f) = fixture();
    let pos = &mut snapshot.morphology.parts_of_speech[0];
    for guid in ["slot-s2", "slot-p1", "slot-p2"] {
        pos.affix_slots.push(AffixSlot {
            guid: guid.to_string(),
            name: guid.to_string(),
            optional: false,
        });
    }
    // Slot lists run innermost-to-outermost, so ordinal 0 sits nearest the stem on each side.
    pos.affix_templates[0].prefix_slots = vec!["slot-p1".to_string(), "slot-p2".to_string()];
    pos.affix_templates[0].suffix_slots = vec![f.slot.clone(), "slot-s2".to_string()];
    for (entry, morph, slot, form) in [
        ("entry-s2", MorphType::Suffix, "slot-s2", "ku"),
        ("entry-p1", MorphType::Prefix, "slot-p1", "ma"),
        ("entry-p2", MorphType::Prefix, "slot-p2", "ku"),
    ] {
        let msa = format!("msa-{entry}");
        snapshot.lexicon.entries.push(LexEntry {
            guid: entry.to_string(),
            citation_form: vec![ws("sen", form)],
            lexeme_morph_type: morph,
            allomorphs: vec![simple_allomorph(&format!("allo-{entry}"), morph, form)],
            msas: vec![Msa::Inflectional {
                guid: msa.clone(),
                part_of_speech: Some(f.noun_pos.clone()),
                slots: vec![slot.to_string()],
                features: None,
                exception_features: Vec::new(),
            }],
            senses: vec![Sense {
                guid: format!("sense-{entry}"),
                gloss: vec![ws("en", form)],
                definition: Vec::new(),
                msa: Some(msa),
            }],
            entry_refs: Vec::new(),
        });
    }

    let output = compile_project_with(&snapshot, CompileOptions::default()).expect("must compile");
    let mut surface_by_slot = BTreeMap::new();
    for decision in &output.load_decisions {
        let Some(value) = &decision.effective_value_json else {
            continue;
        };
        if decision.subject.kind != pg_snapshot::InventoryKind::TemplateSlot {
            continue;
        }
        let placement: serde_json::Value = serde_json::from_str(value).unwrap();
        let slot = placement["slotGuid"].as_str().unwrap().to_string();
        surface_by_slot.insert(slot, placement["surfaceOrdinal"].as_i64().unwrap());
    }
    let expected: BTreeMap<String, i64> = [
        ("slot-s2".to_string(), 2),
        (f.slot.clone(), 1),
        ("slot-p1".to_string(), -1),
        ("slot-p2".to_string(), -2),
    ]
    .into_iter()
    .collect();
    assert_eq!(surface_by_slot, expected);
}
