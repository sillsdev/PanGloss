use super::*;

#[test]
fn compiler_errors_are_json_and_keep_codes_subjects_fields_and_advice() {
    let guid = "00000000-0000-0000-0000-000000000042";
    let issue = pg_snapshot::ConversionIssue {
        code: ImportWarningCode::EnvironmentUnresolved,
        class: pg_snapshot::IssueClass::InvalidSource,
        source: Some(pg_snapshot::SourceRef {
            kind: FwClass::MoForm,
            id: guid.into(),
        }),
        fatal: true,
        message: "environment does not resolve".into(),
    };
    let mut warning = Warning::from_conversion_issue(&issue);
    warning.subjects[0] = warning.subjects[0]
        .clone()
        .name("seeded suffix")
        .field("PhoneEnv");
    let failure = CompileFailure::new(
        "sample.fwdata",
        pg_grammar::compile::issues::ConversionError {
            issues: vec![issue],
            warnings: vec![warning],
            substrate: Default::default(),
            inventory: pg_snapshot::InventoryDelta::from_stage(
                pg_snapshot::ConversionInventory::default(),
                Vec::new(),
            ),
            load_decisions: Vec::new(),
            environment_resolutions: Vec::new(),
            compiled_outputs: Vec::new(),
            compiled_mappings: Vec::new(),
            compiled_allomorph_order: Vec::new(),
            allomorph_gates: Vec::new(),
        }
        .into(),
        None,
    );
    let error = GrammarLoadError::Compile(Box::new(failure));
    let rendered = command_error("parse", &error.to_string());
    let wire: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(wire["schema_version"], 1);
    assert_eq!(wire["status"], "compile_error");
    let issue = &wire["issues"][0];
    assert_eq!(issue["code"], "grammar.environment.unresolved");
    assert_eq!(issue["kind"], "invalidSource");
    assert_eq!(issue["object_guid"], guid);
    assert_eq!(issue["object_kind"], "MoForm");
    assert_eq!(issue["field"], "PhoneEnv");
    assert_eq!(issue["text"], "environment does not resolve");
    assert!(issue["advice"]
        .as_str()
        .unwrap()
        .contains("Grammar > Environments"));
    assert_eq!(issue["fatal"], true);
    assert!(!rendered.contains("ConversionError {"));
}

#[test]
fn early_compiler_failures_form_valid_error_findings() {
    for error in [
        pg_grammar::GrammarError::Semantic("missing lexical source".into()),
        pg_grammar::GrammarError::Xml("bad XML".into()),
    ] {
        let report = CompileFailure::new("sample.xml", error, None);
        assert_eq!(report.issues.len(), 1);
        assert!(report.issues[0].fatal);
        let health =
            pg_grammar::grammar_health::GrammarHealthReport::new(report.diagnostics).unwrap();
        assert_eq!(health.diagnostics()[0].level, DiagnosticLevel::Error);
        assert_eq!(
            health.diagnostics()[0].code.wire(),
            "grammar.compile.failed"
        );
    }
}

#[test]
fn fatal_import_provenance_serializes_as_compile_error() {
    let provenance = pg_snapshot::ConversionProvenance {
        schema_version: pg_snapshot::CONVERSION_PROVENANCE_SCHEMA_VERSION,
        source_inventory_status: SourceInventoryStatus::ImportedWithFatalIssues,
        import_issues: vec![pg_snapshot::ConversionIssue {
            code: ImportWarningCode::InvalidSourceMissingGuid,
            class: IssueClass::InvalidSource,
            source: Some(pg_snapshot::SourceRef {
                kind: FwClass::MoStemMsa,
                id: "rt#2".into(),
            }),
            fatal: true,
            message: "LexDb record at rt#2 has no guid; dropped".into(),
        }],
        ..Default::default()
    };
    assert!(CompileFailure::import_has_fatal_issues(&provenance));
    let failure = CompileFailure::from_import_provenance("sample.fwdata", &provenance);
    let wire = serde_json::to_value(failure).unwrap();
    assert_eq!(wire["status"], "compile_error");
    assert_eq!(wire["path"], "sample.fwdata");
    assert_eq!(wire["issues"][0]["code"], "invalid-source.missing-guid");
    assert_eq!(wire["issues"][0]["object_kind"], "MoStemMsa");
    assert_eq!(wire["issues"][0]["fatal"], true);
}
