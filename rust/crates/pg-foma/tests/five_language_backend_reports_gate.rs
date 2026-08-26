//! Pins complete static backend reports for the five private reference grammars.

use pg_conformance_fixtures::corpus;
use pg_foma::backend_selection::{
    select_backends_for_grammar, BackendSelection, BackendStatus,
};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::health::{FindingCode, Severity};
use pg_grammar::model::Grammar;

fn load_xml(name: &str) -> Grammar {
    let path = corpus::require(name);
    let xml = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|error| panic!("load {}: {error}", path.display()))
}

fn load_snapshot(name: &str) -> Grammar {
    let path = corpus::require(name);
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    pg_grammar::compile_project(&snapshot)
        .map(|(grammar, _)| grammar)
        .unwrap_or_else(|error| panic!("compile {}: {error:?}", path.display()))
}

fn load_fwdata(name: &str) -> Grammar {
    let path = corpus::require(name);
    let (snapshot, _) = pg_fwdata::import_file(&path)
        .unwrap_or_else(|error| panic!("import {}: {error}", path.display()));
    pg_grammar::compile_project(&snapshot)
        .map(|(grammar, _)| grammar)
        .unwrap_or_else(|error| panic!("compile {}: {error:?}", path.display()))
}

fn characterize(name: &str, grammar: &Grammar) -> BackendSelection {
    let selection = select_backends_for_grammar(grammar);
    for report in selection.reports() {
        eprintln!(
            "{name}: backend={:?} status={:?} severity={:?} findings={:?} predicates={} shapes={:?} advice={}",
            report.strategy(),
            report.status(),
            report.worst_severity(),
            report
                .findings()
                .iter()
                .map(|finding| finding.code)
                .collect::<Vec<_>>(),
            report.failed_predicates().len(),
            report.shapes(),
            report.advice_references().len(),
        );
    }
    corpus::record_cases(&format!("{name}_backend_reports"), 1);
    selection
}

fn assert_backend(
    selection: &BackendSelection,
    strategy: EmissionStrategy,
    status: BackendStatus,
    severity: Severity,
    finding: Option<FindingCode>,
    shape: Option<&str>,
) {
    let report = selection
        .report_for(strategy)
        .unwrap_or_else(|| panic!("missing {strategy:?} report"));
    assert_eq!(report.status(), status, "{strategy:?} status: {report:?}");
    assert_eq!(
        report.worst_severity(),
        severity,
        "{strategy:?} severity: {report:?}"
    );
    assert_eq!(
        report.findings().first().map(|item| item.code),
        finding,
        "{strategy:?} finding: {report:?}"
    );
    assert_eq!(
        report.shapes().first().map(String::as_str),
        shape,
        "{strategy:?} shape: {report:?}"
    );
}

fn assert_default_closure_budget_no_path(selection: &BackendSelection) {
    assert_backend(
        selection,
        EmissionStrategy::TunedSurfaceProbed,
        BackendStatus::Accepted,
        Severity::NotProductionReady,
        Some(FindingCode::ProvenBoundExceedsBudget),
        Some("tuned-surface-closure-budget"),
    );
    assert_backend(
        selection,
        EmissionStrategy::TemplatedUnderlyingTokens,
        BackendStatus::Refused,
        Severity::CannotRepresent,
        Some(FindingCode::BackendCoverageIncomplete),
        Some("nonregular-process-morphology"),
    );
    assert_backend(
        selection,
        EmissionStrategy::PlanComposed,
        BackendStatus::Refused,
        Severity::CannotRepresent,
        Some(FindingCode::BackendCoverageIncomplete),
        Some("plan-composed-missing-subtrees"),
    );
}

#[test]
#[ignore = "needs local gitignored corpus data; run with --include-ignored"]
fn indonesian_backend_reports_are_complete() {
    let grammar = load_xml("indonesian-hc.xml");
    let selection = characterize("indonesian", &grammar);
    assert_default_closure_budget_no_path(&selection);
}

#[test]
#[ignore = "needs local gitignored corpus data; run with --include-ignored"]
fn sena_backend_reports_are_complete() {
    let selection = characterize("sena", &load_xml("sena-hc.xml"));
    assert_backend(
        &selection,
        EmissionStrategy::TunedSurfaceProbed,
        BackendStatus::Accepted,
        Severity::WithinLimits,
        None,
        None,
    );
    assert_backend(
        &selection,
        EmissionStrategy::TemplatedUnderlyingTokens,
        BackendStatus::Refused,
        Severity::CannotRepresent,
        Some(FindingCode::BackendCoverageIncomplete),
        Some("nonregular-process-morphology"),
    );
    assert_backend(
        &selection,
        EmissionStrategy::PlanComposed,
        BackendStatus::Accepted,
        Severity::WithinLimits,
        None,
        None,
    );
}

#[test]
#[ignore = "needs local gitignored corpus data; run with --include-ignored"]
fn amharic_backend_reports_are_complete() {
    let selection = characterize("amharic", &load_xml("amharic-hc.xml"));
    assert_default_closure_budget_no_path(&selection);
}

#[test]
#[ignore = "needs local gitignored corpus data; run with --include-ignored"]
fn aweti_backend_reports_are_complete() {
    let selection = characterize("aweti", &load_snapshot("aweti.json"));
    assert_default_closure_budget_no_path(&selection);
}

#[test]
#[ignore = "needs local gitignored corpus data; run with --include-ignored"]
fn mbugwe_backend_reports_are_complete() {
    let selection = characterize("mbugwe", &load_fwdata("mbugwe.fwdata"));
    assert_default_closure_budget_no_path(&selection);
}
