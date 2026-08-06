//! The wire schemas checked against real emitted artifacts, using a hand-written validator that implements only the JSON Schema subset these schemas use.
//! See `docs/research/schema-conformance-validator.md` for why the schemas are independent of the Rust types and why the validator is a declared subset rather than a full implementation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use pg_assess::{
    compare, golden_diff, investigate, AnalysisIdentity, AnalysisSet, BudgetDimension, CaseOutcome,
    CaseRecord, Diagnostic, Execution, HandoffRequest, IncompleteReason, NotAttemptedReason,
    Provenance, ReportDraft, Severity, SuiteRef, IDENTITY_PROFILE,
};

// --- The subset validator ---

struct Validator {
    /// `$defs` from the schema itself, merged over the shared `common.defs.json`.
    defs: Map<String, Value>,
}

#[derive(Debug)]
struct Failure {
    path: String,
    message: String,
}

impl Failure {
    fn at(path: &str, message: impl Into<String>) -> Self {
        Failure {
            path: path.to_string(),
            message: message.into(),
        }
    }
}

impl Validator {
    fn new(schema: &Value, common: &Value) -> Self {
        let mut defs = common
            .get("$defs")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if let Some(local) = schema.get("$defs").and_then(Value::as_object) {
            for (name, value) in local {
                defs.insert(name.clone(), value.clone());
            }
        }
        Validator { defs }
    }

    fn validate(&self, instance: &Value, schema: &Value, path: &str) -> Vec<Failure> {
        let Some(schema) = schema.as_object() else {
            return vec![Failure::at(path, "schema node is not an object")];
        };
        // `{}` accepts anything — the schemas use it for deliberately opaque caller data.
        if schema.is_empty() {
            return Vec::new();
        }

        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            let name = reference.strip_prefix("#/$defs/").unwrap_or_else(|| {
                panic!("{path}: only #/$defs/* refs are supported, got {reference}")
            });
            let target = self
                .defs
                .get(name)
                .unwrap_or_else(|| panic!("{path}: unknown $def {name}"));
            return self.validate(instance, target, path);
        }

        let mut failures = Vec::new();
        for keyword in schema.keys() {
            const SUPPORTED: &[&str] = &[
                "$schema",
                "$id",
                "title",
                "description",
                "$defs",
                "$ref",
                "type",
                "required",
                "properties",
                "additionalProperties",
                "enum",
                "const",
                "items",
                "oneOf",
                "minimum",
                "minLength",
                "maxLength",
                "minItems",
                "maxItems",
                "pattern",
            ];
            assert!(
                SUPPORTED.contains(&keyword.as_str()),
                "{path}: schema uses unsupported keyword {keyword:?}; extend the validator or the \
                 schema is unchecked"
            );
        }

        if let Some(expected) = schema.get("type") {
            if !type_matches(instance, expected) {
                failures.push(Failure::at(
                    path,
                    format!("expected type {expected}, got {}", kind_of(instance)),
                ));
                // Further keywords assume the type held.
                return failures;
            }
        }
        if let Some(expected) = schema.get("const") {
            if instance != expected {
                failures.push(Failure::at(path, format!("expected const {expected}")));
            }
        }
        if let Some(Value::Array(allowed)) = schema.get("enum") {
            if !allowed.contains(instance) {
                failures.push(Failure::at(
                    path,
                    format!("{instance} is not one of {}", Value::Array(allowed.clone())),
                ));
            }
        }
        if let Some(Value::Array(branches)) = schema.get("oneOf") {
            let matched = branches
                .iter()
                .filter(|branch| self.validate(instance, branch, path).is_empty())
                .count();
            if matched != 1 {
                failures.push(Failure::at(
                    path,
                    format!("matched {matched} oneOf branches, expected exactly 1"),
                ));
            }
        }

        match instance {
            Value::Object(object) => {
                if let Some(Value::Array(required)) = schema.get("required") {
                    for name in required.iter().filter_map(Value::as_str) {
                        if !object.contains_key(name) {
                            failures.push(Failure::at(path, format!("missing required {name:?}")));
                        }
                    }
                }
                let properties = schema.get("properties").and_then(Value::as_object);
                for (name, value) in object {
                    match properties.and_then(|p| p.get(name)) {
                        Some(sub) => {
                            failures.extend(self.validate(value, sub, &format!("{path}.{name}")))
                        }
                        None => match schema.get("additionalProperties") {
                            Some(Value::Bool(false)) => failures
                                .push(Failure::at(path, format!("unexpected property {name:?}"))),
                            Some(sub) if sub.is_object() => failures.extend(self.validate(
                                value,
                                sub,
                                &format!("{path}.{name}"),
                            )),
                            _ => {}
                        },
                    }
                }
            }
            Value::Array(items) => {
                if let Some(sub) = schema.get("items") {
                    for (index, item) in items.iter().enumerate() {
                        failures.extend(self.validate(item, sub, &format!("{path}[{index}]")));
                    }
                }
                if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
                    if (items.len() as u64) < min {
                        failures.push(Failure::at(path, format!("fewer than {min} items")));
                    }
                }
                if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
                    if (items.len() as u64) > max {
                        failures.push(Failure::at(path, format!("more than {max} items")));
                    }
                }
            }
            Value::String(text) => {
                if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
                    if (text.chars().count() as u64) < min {
                        failures.push(Failure::at(path, format!("shorter than {min} chars")));
                    }
                }
                if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
                    if (text.chars().count() as u64) > max {
                        failures.push(Failure::at(path, format!("longer than {max} chars")));
                    }
                }
                if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
                    if !matches_pattern(text, pattern) {
                        failures.push(Failure::at(path, format!("does not match {pattern}")));
                    }
                }
            }
            Value::Number(number) => {
                if let Some(min) = schema.get("minimum").and_then(Value::as_i64) {
                    if number.as_i64().is_some_and(|n| n < min) {
                        failures.push(Failure::at(path, format!("below minimum {min}")));
                    }
                }
            }
            _ => {}
        }
        failures
    }
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn type_matches(instance: &Value, expected: &Value) -> bool {
    let actual = kind_of(instance);
    let one = |name: &str| name == actual || (name == "number" && actual == "integer");
    match expected {
        Value::String(name) => one(name),
        Value::Array(names) => names.iter().filter_map(Value::as_str).any(one),
        _ => panic!("unsupported `type` form: {expected}"),
    }
}

/// The one pattern the schemas use, implemented literally rather than by pulling a regex engine; anything else is refused so the coverage claim stays true.
fn matches_pattern(text: &str, pattern: &str) -> bool {
    assert_eq!(
        pattern, "^sha256:[0-9a-f]{64}$",
        "unsupported pattern {pattern:?}; extend `matches_pattern` or it is unchecked"
    );
    text.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}

// --- Loading ---

fn schemas_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas")
}

fn load(path: &Path) -> Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn validator_for(name: &str) -> (Validator, Value) {
    let common = load(&schemas_dir().join("common.defs.json"));
    let schema = load(&schemas_dir().join(format!("{name}.schema.json")));
    (Validator::new(&schema, &common), schema)
}

#[track_caller]
fn assert_valid(name: &str, instance: &Value) {
    let (validator, schema) = validator_for(name);
    let failures = validator.validate(instance, &schema, "$");
    assert!(
        failures.is_empty(),
        "{name} rejected a real artifact:\n{}",
        failures
            .iter()
            .map(|f| format!("  {} — {}", f.path, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[track_caller]
fn assert_rejected(name: &str, instance: &Value, expected_path_fragment: &str) {
    let (validator, schema) = validator_for(name);
    let failures = validator.validate(instance, &schema, "$");
    assert!(!failures.is_empty(), "{name} accepted an invalid artifact");
    assert!(
        failures
            .iter()
            .any(|f| f.path.contains(expected_path_fragment)),
        // Without this a negative fixture can pass for the wrong reason and stop testing anything.
        "{name} rejected the artifact, but not at {expected_path_fragment:?}: {failures:?}"
    );
}

// --- Artifacts built the way production builds them ---

fn identity(morphemes: &[Option<&str>], category: Option<&str>) -> AnalysisIdentity {
    AnalysisIdentity {
        morphemes: morphemes.iter().map(|m| m.map(str::to_string)).collect(),
        root_index: 0,
        category: category.map(str::to_string),
    }
}

fn suite_ref() -> SuiteRef {
    SuiteRef {
        suite_id: "demo".into(),
        suite_revision: "r1".into(),
        semantic_digest: format!("sha256:{}", "a".repeat(64)),
        analysis_identity_profile: IDENTITY_PROFILE.into(),
    }
}

fn provenance() -> Provenance {
    Provenance {
        source_sha256: format!("sha256:{}", "b".repeat(64)),
        source_kind: "hc-xml".into(),
        model_fingerprint: format!("sha256:{}", "c".repeat(64)),
        importer_version: "0.1.0".into(),
        compiler_version: "0.1.0".into(),
    }
}

fn case(case_id: &str, outcome: CaseOutcome) -> CaseRecord {
    CaseRecord {
        case_id: case_id.into(),
        input: "walked".into(),
        outcome,
        supersedes: Vec::new(),
    }
}

/// A report exercising every per-case outcome, a guessed root, a duplicate, and a diagnostic, so the schema is checked against the full shape.
fn report(cases: Vec<CaseRecord>) -> pg_assess::AssessmentReport {
    let mut budgets = BTreeMap::new();
    budgets.insert("candidates".to_string(), 4096u64);
    ReportDraft {
        generated_at: "2026-07-29T00:00:00Z".into(),
        suite: suite_ref(),
        execution: Execution {
            pipeline: "foma-confirm".into(),
            budgets,
            wall_clock_limit_us: None,
        },
        provenance: provenance(),
        diagnostics: vec![Diagnostic {
            code: "importer.warning".into(),
            severity: Severity::Warning,
            message: "an entry referenced a missing MSA".into(),
        }],
        cases,
        failure: None,
        extensions: Some(json!({ "com.example.review": { "assignee": "sam" } })),
    }
    .finish()
    .expect("fixture report digests")
}

fn full_report() -> pg_assess::AssessmentReport {
    report(vec![
        case(
            "complete",
            CaseOutcome::Complete(AnalysisSet::from_annotated([
                (
                    identity(&[Some("guid-walk"), Some("guid-ed")], Some("guid-v")),
                    false,
                ),
                (
                    identity(&[Some("guid-walk"), Some("guid-ed")], Some("guid-v")),
                    false,
                ),
                (identity(&[None], None), true),
            ])),
        ),
        case(
            "empty",
            CaseOutcome::Complete(AnalysisSet::from_observed([])),
        ),
        case(
            "stopped",
            CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
                dimension: BudgetDimension::Candidates,
                value: 5000,
                limit: 4096,
            }),
        ),
        case(
            "clock",
            CaseOutcome::Incomplete(IncompleteReason::WallClockTimeout {
                elapsed_us: 2_000_000,
                limit_us: 1_000_000,
            }),
        ),
        case(
            "skipped",
            CaseOutcome::NotAttempted(NotAttemptedReason::BatchBudgetExhausted),
        ),
    ])
}

const SUITE_JSON: &str = r#"{
  "schema": "pangloss.assessment-suite",
  "schemaVersion": 1,
  "suiteId": "demo",
  "suiteRevision": "r1",
  "analysisIdentityProfile": "pangloss.machine-word-analysis/v1",
  "metadata": { "owner": "lcatom" },
  "extensions": { "com.example": { "note": "kept verbatim" } },
  "cases": [
    {
      "caseId": "bank-money",
      "input": "bank",
      "tags": ["finance"],
      "sourceReferences": [{ "kind": "opaque" }],
      "expectation": {
        "status": "adjudicated",
        "closedWorld": true,
        "required": [{ "morphemes": ["guid-bank"], "rootIndex": 0, "category": null }],
        "forbidden": [{ "morphemes": ["guid-bank-river"], "rootIndex": 0, "category": null }]
      }
    },
    {
      "caseId": "bank-river",
      "input": "bank",
      "supersedes": ["old-bank"],
      "expectation": { "status": "unresolved" }
    }
  ]
}"#;

// --- Positive: the real emitters produce schema-conformant artifacts ---

#[test]
fn every_schema_file_parses_and_declares_its_id() {
    for (file, id) in [
        ("assessment-suite", "pangloss.assessment-suite/v1"),
        ("assessment-report", "pangloss.assessment-report/v1"),
        ("grammar-delta", "pangloss.grammar-delta/v1"),
        ("golden-set-diff", "pangloss.golden-set-diff/v1"),
        ("investigation-handoff", "pangloss.investigation-handoff/v1"),
    ] {
        let schema = load(&schemas_dir().join(format!("{file}.schema.json")));
        assert_eq!(schema["$id"], json!(id), "{file}");
    }
}

#[test]
fn a_real_assessment_report_conforms() {
    assert_valid("assessment-report", &full_report().to_value());
}

#[test]
fn a_real_suite_conforms() {
    let parsed = pg_assess::parse_suite(SUITE_JSON).expect("fixture suite is valid to the code");
    // Validate the document the caller actually supplies: the schema and the validator must agree about what a suite is.
    assert_valid(
        "assessment-suite",
        &serde_json::from_str::<Value>(SUITE_JSON).unwrap(),
    );
    assert_eq!(parsed.cases().len(), 2);
}

#[test]
fn a_real_grammar_delta_conforms() {
    let baseline = full_report();
    let candidate = report(vec![
        case(
            "complete",
            CaseOutcome::Complete(AnalysisSet::from_annotated([(
                identity(&[Some("guid-walk"), Some("guid-ed")], Some("guid-v")),
                true,
            )])),
        ),
        case(
            "empty",
            CaseOutcome::Complete(AnalysisSet::from_observed([])),
        ),
        case(
            "stopped",
            CaseOutcome::Complete(AnalysisSet::from_observed([identity(
                &[Some("guid-walk")],
                None,
            )])),
        ),
        case(
            "clock",
            CaseOutcome::Incomplete(IncompleteReason::WallClockTimeout {
                elapsed_us: 1,
                limit_us: 2,
            }),
        ),
        case(
            "skipped",
            CaseOutcome::NotAttempted(NotAttemptedReason::BatchBudgetExhausted),
        ),
    ]);
    let delta = compare(&baseline, &candidate).expect("compare");
    assert_valid("grammar-delta", &delta.to_value());
}

#[test]
fn a_real_golden_set_diff_conforms() {
    let suite = pg_assess::parse_suite(SUITE_JSON).expect("fixture suite");
    let assessed = ReportDraft {
        generated_at: "2026-07-29T00:00:00Z".into(),
        suite: SuiteRef {
            suite_id: suite.suite().suite_id.clone(),
            suite_revision: suite.suite().suite_revision.clone(),
            semantic_digest: suite.semantic_digest().to_string(),
            analysis_identity_profile: IDENTITY_PROFILE.into(),
        },
        execution: Execution {
            pipeline: "hermitcrab".into(),
            ..Execution::default()
        },
        provenance: provenance(),
        diagnostics: Vec::new(),
        cases: vec![
            case(
                "bank-money",
                CaseOutcome::Complete(AnalysisSet::from_observed([identity(
                    &[Some("guid-bank")],
                    None,
                )])),
            ),
            case(
                "bank-river",
                CaseOutcome::Complete(AnalysisSet::from_observed([])),
            ),
        ],
        failure: None,
        extensions: None,
    }
    .finish()
    .expect("report digests");

    let diff = golden_diff(&assessed, &suite).expect("golden diff");
    assert_valid("golden-set-diff", &diff.to_value());
}

#[test]
fn a_real_investigation_handoff_conforms() {
    let report = full_report();
    let request = HandoffRequest {
        case_id: "complete".into(),
        asked_about: vec![identity(&[Some("guid-gone")], None)],
        constructs: vec![
            pg_assess::ConstructRef::source("lexicalEntry", "guid-walk", Some("walk".into())),
            pg_assess::ConstructRef::compiler_assigned("morphologicalRule", 7, None),
        ],
        narrative: vec![pg_assess::NarrativeStep {
            candidate: "walk + ed".into(),
            at: pg_assess::ConstructRef::compiler_assigned("phonologicalRule", 2, None),
            failure_reason: "Environments".into(),
            detail: "the rule's left environment did not match".into(),
        }],
        ..HandoffRequest::default()
    };
    let handoff = investigate(&report, &request).expect("handoff");
    assert_valid("investigation-handoff", &handoff.to_value());
}

// --- Negative: each fixture must be rejected, and rejected for its own reason ---

#[test]
fn a_successful_report_states_failure_as_null_rather_than_omitting_it() {
    // An explicit null says "did not fail"; a missing key would look identical to an older producer that never emitted the field.
    let value = full_report().to_value();
    assert!(value.as_object().unwrap().contains_key("failure"));
    assert_eq!(value["failure"], Value::Null);
    assert_valid("assessment-report", &value);
}

#[test]
fn a_failed_report_carries_a_typed_top_level_failure() {
    // A failed run carries a top-level status and a nullable typed failure.
    let mut draft = full_report().draft().clone();
    draft.cases = vec![case(
        "never-ran",
        CaseOutcome::NotAttempted(NotAttemptedReason::AssessmentSetupFailed),
    )];
    draft.failure = Some(pg_assess::AssessmentFailure {
        kind: pg_assess::FailureKind::AssessmentSetupFailed,
        message: "the grammar did not compile".into(),
    });
    let report = draft.finish().expect("digests");
    let value = report.to_value();

    assert_eq!(value["status"], "failed");
    assert_eq!(value["failure"]["kind"], "assessment_setup_failed");
    assert_valid("assessment-report", &value);

    // Survives a round trip, so a consumer reading the artifact later sees the same typed reason.
    let read = pg_assess::parse_report(&report.to_canonical_json().unwrap()).unwrap();
    assert_eq!(
        read.failure().map(|f| f.kind),
        Some(pg_assess::FailureKind::AssessmentSetupFailed)
    );
}

#[test]
fn a_report_with_an_untyped_failure_kind_is_rejected() {
    let mut value = full_report().to_value();
    value["failure"] = json!({ "kind": "something went wrong", "message": "prose" });
    assert_rejected("assessment-report", &value, "failure");
}

#[test]
fn a_report_that_omits_failure_entirely_is_rejected() {
    let mut value = full_report().to_value();
    value.as_object_mut().unwrap().remove("failure");
    assert_rejected("assessment-report", &value, "$");
}

#[test]
fn a_report_with_an_unknown_outcome_kind_is_rejected() {
    let mut value = full_report().to_value();
    value["cases"][0]["outcome"] = json!("mostly_complete");
    assert_rejected("assessment-report", &value, "cases[0].outcome");
}

#[test]
fn a_report_with_a_malformed_digest_is_rejected() {
    let mut value = full_report().to_value();
    value["reportId"] = json!("sha256:not-hex");
    assert_rejected("assessment-report", &value, "reportId");
}

#[test]
fn a_report_with_an_unprefixed_digest_is_rejected() {
    // The algorithm prefix is what stops a future digest being mistaken for this one.
    let mut value = full_report().to_value();
    value["semanticDigest"] = json!("d".repeat(64));
    assert_rejected("assessment-report", &value, "semanticDigest");
}

#[test]
fn a_report_missing_reproducible_is_rejected() {
    let mut value = full_report().to_value();
    value.as_object_mut().unwrap().remove("reproducible");
    assert_rejected("assessment-report", &value, "$");
}

#[test]
fn a_report_with_an_unknown_field_is_rejected() {
    // Closed schemas: a consumer must not silently ignore a field a newer producer added.
    let mut value = full_report().to_value();
    value["qualityScore"] = json!(0.92);
    assert_rejected("assessment-report", &value, "$");
}

#[test]
fn a_report_with_a_zero_duplicate_count_is_rejected() {
    let mut value = full_report().to_value();
    value["cases"][0]["analyses"][0]["duplicateCount"] = json!(0);
    assert_rejected("assessment-report", &value, "duplicateCount");
}

#[test]
fn a_report_from_an_unknown_pipeline_is_rejected() {
    let mut value = full_report().to_value();
    value["execution"]["pipeline"] = json!("xample");
    assert_rejected("assessment-report", &value, "execution.pipeline");
}

#[test]
fn a_suite_with_an_unknown_expectation_status_is_rejected() {
    let mut value: Value = serde_json::from_str(SUITE_JSON).unwrap();
    value["cases"][0]["expectation"]["status"] = json!("probably_fine");
    assert_rejected("assessment-suite", &value, "expectation.status");
}

#[test]
fn a_suite_from_another_identity_profile_is_rejected() {
    let mut value: Value = serde_json::from_str(SUITE_JSON).unwrap();
    value["analysisIdentityProfile"] = json!("pangloss.machine-word-analysis/v2");
    assert_rejected("assessment-suite", &value, "analysisIdentityProfile");
}

#[test]
fn a_delta_with_an_unknown_category_is_rejected() {
    let baseline = full_report();
    let mut value = compare(&baseline, &baseline).expect("compare").to_value();
    value["cases"][0]["category"] = json!("improved");
    assert_rejected("grammar-delta", &value, "cases[0].category");
}

#[test]
fn a_delta_with_an_untyped_not_comparable_reason_is_rejected() {
    let baseline = full_report();
    let mut value = compare(&baseline, &baseline).expect("compare").to_value();
    value["cases"][0]["notComparableReason"] = json!("the reports looked different");
    assert_rejected("grammar-delta", &value, "notComparableReason");
}

#[test]
fn a_handoff_that_omits_the_engine_caveat_is_rejected() {
    let report = full_report();
    let mut value = investigate(
        &report,
        &HandoffRequest {
            case_id: "complete".into(),
            ..HandoffRequest::default()
        },
    )
    .expect("handoff")
    .to_value();
    value.as_object_mut().unwrap().remove("caveat");
    assert_rejected("investigation-handoff", &value, "$");
}

#[test]
fn a_handoff_that_dresses_a_dense_ordinal_as_a_source_id_is_rejected() {
    let report = full_report();
    let mut value = investigate(
        &report,
        &HandoffRequest {
            case_id: "complete".into(),
            constructs: vec![pg_assess::ConstructRef::compiler_assigned(
                "morphologicalRule",
                7,
                None,
            )],
            ..HandoffRequest::default()
        },
    )
    .expect("handoff")
    .to_value();
    value["constructs"][0]["idKind"] = json!("fieldworksGuid");
    assert_rejected("investigation-handoff", &value, "constructs[0].idKind");
}

#[test]
fn the_validator_refuses_a_schema_keyword_it_does_not_implement() {
    // The claim "this subset is fully checked" is only true if an unknown keyword is loud.
    let common = json!({ "$defs": {} });
    let schema = json!({ "type": "object", "patternProperties": { "^x": { "type": "string" } } });
    let validator = Validator::new(&schema, &common);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        validator.validate(&json!({}), &schema, "$")
    }));
    assert!(
        outcome.is_err(),
        "an unimplemented keyword must not pass silently"
    );
}
