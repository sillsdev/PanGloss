use super::super::*;

#[test]
fn display_is_exactly_the_message() {
    let w = Warning::new(
        ImportWarningCode::FwdataDanglingReference,
        "phoneme \"00...\" does not resolve",
    );
    assert_eq!(w.to_string(), "phoneme \"00...\" does not resolve");
}

#[test]
fn deref_supports_str_methods_like_contains() {
    let w = Warning::new(
        ImportWarningCode::FwdataDanglingReference,
        "dangling reference to Foo abc-123",
    );
    assert!(w.contains("abc-123"));
}

#[test]
fn code_is_independent_of_message_reword() {
    let original = Warning::new(
        ImportWarningCode::FwdataDanglingReference,
        "old wording of the same fact",
    );
    let reworded = Warning::new(
        ImportWarningCode::FwdataDanglingReference,
        "new wording, same situation",
    );
    assert_eq!(original.code, reworded.code);
    assert_ne!(original.message, reworded.message);
}

#[test]
fn different_situations_get_different_codes() {
    let dangling = Warning::new(
        ImportWarningCode::FwdataDanglingReference,
        "X does not resolve",
    );
    let unexpected_class = Warning::new(
        ImportWarningCode::FwdataUnexpectedClass,
        "X has unexpected class Y",
    );
    assert_ne!(dangling.code, unexpected_class.code);
}

#[test]
fn warning_equality_includes_code_and_subject() {
    let same_message_different_code = Warning::new(
        ImportWarningCode::Unregistered("warning.first".to_string()),
        "same prose",
    );
    let other_code = Warning::new(
        ImportWarningCode::Unregistered("warning.second".to_string()),
        "same prose",
    );
    assert_ne!(same_message_different_code, other_code);

    let subject_a = FwObjectRef::new(FwClass::PhEnvironment).name("first");
    let subject_b = FwObjectRef::new(FwClass::PhEnvironment).name("second");
    assert_ne!(
        Warning::new(
            ImportWarningCode::Unregistered("warning.same".to_string()),
            "same prose"
        )
        .with_subject(subject_a),
        Warning::new(
            ImportWarningCode::Unregistered("warning.same".to_string()),
            "same prose"
        )
        .with_subject(subject_b),
    );
}

#[test]
fn warning_deduplication_does_not_match_an_unidentified_warning_to_a_subject() {
    let unidentified = Warning::new(
        ImportWarningCode::Unregistered("warning.same".to_string()),
        "same prose",
    );
    let identified = Warning::new(
        ImportWarningCode::Unregistered("warning.same".to_string()),
        "same prose",
    )
    .with_subject(FwObjectRef::new(FwClass::MoForm).guid("00000000-0000-0000-0000-000000000042"));

    assert!(!unidentified.same_fact_as(&identified));
}

#[test]
fn warning_deduplication_does_not_merge_unidentified_facts() {
    let first = Warning::new(
        ImportWarningCode::Unregistered("warning.same".to_string()),
        "same prose",
    );
    let second = Warning::new(
        ImportWarningCode::Unregistered("warning.same".to_string()),
        "same prose",
    );

    assert!(!first.same_fact_as(&second));
}

#[test]
fn warning_deduplication_compares_subject_sets_without_order() {
    let natural_class =
        FwObjectRef::new(FwClass::PhNaturalClass).guid("00000000-0000-0000-0000-000000000001");
    let environment =
        FwObjectRef::new(FwClass::PhEnvironment).guid("00000000-0000-0000-0000-000000000002");
    let first = Warning::new(
        ImportWarningCode::Unregistered("warning.same".to_string()),
        "same prose",
    )
    .with_subject(natural_class.clone())
    .with_subject(environment.clone());
    let second = Warning::new(
        ImportWarningCode::Unregistered("warning.same".to_string()),
        "same prose",
    )
    .with_subject(environment)
    .with_subject(natural_class);

    assert!(first.same_fact_as(&second));
}

#[test]
fn conversion_issue_retains_unknown_code_and_noncanonical_guid() {
    let canonical_guid = "00000000-0000-0000-0000-000000000042";
    let issue = crate::ConversionIssue {
        code: ImportWarningCode::Unregistered("grammar.anything".to_string()),
        class: crate::IssueClass::MigrationDifference,
        source: Some(crate::SourceRef {
            kind: crate::FwClass::PhEnvironment,
            id: canonical_guid.to_string(),
        }),
        fatal: false,
        message: "internal notice".to_string(),
    };

    let warning = Warning::from_conversion_issue(&issue);
    assert!(warning.message.contains("grammar.anything"));
    assert_eq!(warning.subjects[0].guid.as_deref(), Some(canonical_guid));

    let issue = crate::ConversionIssue {
        source: Some(crate::SourceRef {
            kind: crate::FwClass::PhEnvironment,
            id: "env-bad".to_string(),
        }),
        ..issue
    };
    let warning = Warning::from_conversion_issue(&issue);
    assert_eq!(warning.subjects[0].guid.as_deref(), Some("env-bad"));
}

#[test]
fn fieldworks_path_table_uses_configured_tool_labels() {
    assert_eq!(
        crate::fieldworks_paths::LEXICON_EDIT,
        "Lexicon > Lexicon Edit"
    );
    assert_eq!(
        crate::fieldworks_paths::GRAMMAR_CATEGORY_AFFIX_TEMPLATES,
        "Grammar > Category Edit > the category's Affix Templates"
    );
    assert_eq!(
        crate::fieldworks_paths::GRAMMAR_AD_HOC_RULES,
        "Grammar > Ad hoc Rules"
    );
    assert_eq!(
        crate::fieldworks_paths::GRAMMAR_COMPOUND_RULES,
        "Grammar > Compound Rules"
    );
    assert_eq!(
        crate::fieldworks_paths::GRAMMAR_CATEGORY_EDIT,
        "Grammar > Category Edit"
    );
    assert_eq!(
        crate::fieldworks_paths::GRAMMAR_PHONEMES,
        "Grammar > Phonemes"
    );
    assert_eq!(
        crate::fieldworks_paths::GRAMMAR_PHONOLOGICAL_FEATURES,
        "Grammar > Phonological Features"
    );
    assert_eq!(
        crate::fieldworks_paths::GRAMMAR_PHONOLOGICAL_RULES,
        "Grammar > Phonological Rules"
    );
    assert_eq!(
        crate::fieldworks_paths::GRAMMAR_NATURAL_CLASSES,
        "Grammar > Natural Classes"
    );
    assert_eq!(
        crate::fieldworks_paths::GRAMMAR_ENVIRONMENTS,
        "Grammar > Environments"
    );
    assert_eq!(
        crate::fieldworks_paths::LISTS_VARIANT_TYPES,
        "Lists > Variant Types"
    );
    assert_eq!(
        crate::fieldworks_paths::WORDS_EDIT_PARSER_PARAMETERS,
        "Words > Edit Parser Parameters..."
    );
    assert_eq!(
        crate::fieldworks_paths::TOOLS_CONFIGURE_VERNACULAR_WRITING_SYSTEMS,
        "Tools > Configure > Set up Vernacular Writing Systems..."
    );
    assert_eq!(
        crate::fieldworks_paths::TOOLS_CONFIGURE_ANALYSIS_WRITING_SYSTEMS,
        "Tools > Configure > Set up Analysis Writing Systems..."
    );
    assert_eq!(
        crate::fieldworks_paths::FILE_RESTORE_PROJECT,
        "File > Restore a Project..."
    );
}
