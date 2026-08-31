//! Static backend reports for the five private reference grammars.

use pg_conformance_fixtures::corpus;
use pg_foma::backend_selection::{select_backends_for_grammar, BackendSelection, BackendStatus};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::health::{FindingCode, Severity};
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_grammar::model::Grammar;

/// The named corpus' grammar, whatever file the manifest says backs it.
fn load(logical_name: &str) -> Grammar {
    let path = corpus::grammar_for(logical_name);
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

/// Fails when a backend is added without extending the per-grammar verdicts below to cover it.
fn assert_every_backend_reported(name: &str, selection: &BackendSelection) {
    assert_eq!(
        selection.reports().len(),
        ALL_STRATEGIES.len(),
        "{name}: a backend produced no compatibility report; an absent one reads as a pass it did \
         not earn, and the per-backend assertions below would silently stop covering it"
    );
    for &strategy in ALL_STRATEGIES {
        let report = selection
            .report_for(strategy)
            .unwrap_or_else(|| panic!("{name}: no report for {strategy:?}"));
        assert_ne!(
            report.status(),
            BackendStatus::Missing,
            "{name}: {strategy:?}"
        );
    }
}

fn assert_backend(
    name: &str,
    selection: &BackendSelection,
    strategy: EmissionStrategy,
    status: BackendStatus,
    severity: Severity,
    finding: Option<FindingCode>,
    shape: Option<&str>,
) {
    let report = selection
        .report_for(strategy)
        .unwrap_or_else(|| panic!("{name}: missing {strategy:?} report"));
    assert_eq!(
        report.status(),
        status,
        "{name}: {strategy:?} status: {report:?}"
    );
    assert_eq!(
        report.worst_severity(),
        severity,
        "{name}: {strategy:?} severity: {report:?}"
    );
    assert_eq!(
        report.findings().first().map(|item| item.code),
        finding,
        "{name}: {strategy:?} finding: {report:?}"
    );
    assert_eq!(
        report.shapes().first().map(String::as_str),
        shape,
        "{name}: {strategy:?} shape: {report:?}"
    );
}

/// The verdict every reference grammar but Sena measures to: the surface-probed backend is the only path.
fn assert_only_tuned_surface_accepts(name: &str, selection: &BackendSelection) {
    assert_every_backend_reported(name, selection);
    assert_backend(
        name,
        selection,
        EmissionStrategy::TunedSurfaceProbed,
        BackendStatus::Accepted,
        Severity::WithinLimits,
        None,
        None,
    );
    assert_backend(
        name,
        selection,
        EmissionStrategy::TemplatedUnderlyingTokens,
        BackendStatus::Refused,
        Severity::CannotRepresent,
        Some(FindingCode::BackendCoverageIncomplete),
        Some("nonregular-process-morphology"),
    );
    assert_backend(
        name,
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
fn sena_backend_reports_are_complete() {
    let selection = characterize("sena", &load("sena"));
    assert_every_backend_reported("sena", &selection);
    assert_backend(
        "sena",
        &selection,
        EmissionStrategy::TunedSurfaceProbed,
        BackendStatus::Accepted,
        Severity::WithinLimits,
        None,
        None,
    );
    assert_backend(
        "sena",
        &selection,
        EmissionStrategy::TemplatedUnderlyingTokens,
        BackendStatus::Refused,
        Severity::CannotRepresent,
        Some(FindingCode::BackendCoverageIncomplete),
        Some("nonregular-process-morphology"),
    );
    // Sena is the one reference grammar whose plan `build_controllable` can build outright, so PlanComposed is admitted rather than refused for a missing subtree.
    assert_backend(
        "sena",
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
fn indonesian_backend_reports_are_complete() {
    let selection = characterize("indonesian", &load("indonesian"));
    assert_only_tuned_surface_accepts("indonesian", &selection);
}

#[test]
#[ignore = "needs local gitignored corpus data; run with --include-ignored"]
fn amharic_backend_reports_are_complete() {
    let selection = characterize("amharic", &load("amharic"));
    assert_only_tuned_surface_accepts("amharic", &selection);
}

/// Both were refused on `"repeated-application"` until the variant CAP became an advisory threshold; they now enumerate completely and match Indonesian/Amharic.
#[test]
#[ignore = "needs local gitignored corpus data; run with --include-ignored"]
fn aweti_backend_reports_are_complete() {
    let selection = characterize("aweti", &load("aweti"));
    assert_only_tuned_surface_accepts("aweti", &selection);
}

#[test]
#[ignore = "needs local gitignored corpus data; run with --include-ignored"]
fn mbugwe_backend_reports_are_complete() {
    let selection = characterize("mbugwe", &load("mbugwe"));
    assert_only_tuned_surface_accepts("mbugwe", &selection);
}

/// All five reference grammars keep an accepted backend; which one, and why, is pinned above.
#[test]
#[ignore = "needs local gitignored corpus data; run with --include-ignored"]
fn every_reference_grammar_has_an_accepted_backend() {
    for name in ["sena", "indonesian", "amharic", "aweti", "mbugwe"] {
        let selection = characterize(name, &load(name));
        assert!(
            selection
                .reports()
                .iter()
                .any(|report| report.status() == BackendStatus::Accepted),
            "{name}: no backend accepts this grammar, so selection has no path and would write no \
             trusted FST"
        );
    }
}
