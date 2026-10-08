//! `pangloss coverage [--json] [--grammar=<path>] [<out.json>]`: renders `pg_foma`'s own coverage/capability primitives (`coverage_ledger::build_ledger`, `conformance_coverage`, optionally `plan_interaction_coverage` when `--grammar` is given), never inventing a parallel data source or count.

use std::collections::HashSet;
use std::fs;

use pg_conformance_fixtures::{
    discover_scoped, producibility_census, ConformanceScope, ProducibilityCensus,
};
use pg_foma::capability::{default_registry, CharacteristicKind, Disposition, VariantDisposition};
use pg_foma_backend::conformance_coverage::CoverageStatus;
use pg_foma_backend::coverage_ledger::{build_ledger, obligation_met, CoverageLedger};
use pg_foma_backend::fixture_coverage::{passing_covered_constructs, FixtureLoadFailure};
use pg_foma_backend::plan_interaction_coverage::{
    compute_interaction_coverage, plan_and_profile, TupleStatus,
};
use pg_grammar::model::Grammar;
use serde::Serialize;

/// This CLI report's own schema version, independent of `pg_foma_backend::coverage_ledger::COVERAGE_LEDGER_SCHEMA_VERSION`, which the embedded `ledger` field carries in its own right.
pub const COVERAGE_CLI_SCHEMA_VERSION: u32 = 2;

#[derive(Serialize)]
struct DispositionCounts {
    proven: usize,
    confirm_only: usize,
    admit: usize,
    refuse: usize,
    total: usize,
}

fn compute_disposition_counts(ledger: &CoverageLedger) -> DispositionCounts {
    let mut c = DispositionCounts {
        proven: 0,
        confirm_only: 0,
        admit: 0,
        refuse: 0,
        total: ledger.rows.len(),
    };
    for row in &ledger.rows {
        match row.variant_disposition {
            Some(VariantDisposition::Admit) => c.admit += 1,
            Some(VariantDisposition::ConfirmOnly) => c.confirm_only += 1,
            Some(VariantDisposition::Refuse) => c.refuse += 1,
            None => match row.disposition {
                Disposition::Proven => c.proven += 1,
                Disposition::ConfirmOnly => c.confirm_only += 1,
                Disposition::ConfigPredicate => {
                    panic!("predicate obligation has no concrete variant")
                }
            },
        }
    }
    c
}

#[derive(Serialize)]
struct EvidenceCounts {
    rows_with_discharging_predicate: usize,
    rows_with_containment_evidence: usize,
    rows_mapped_to_conformance_construct: usize,
    rows_conformance_covered: usize,
    rows_unmappable: usize,
    total_rows: usize,
    variant_obligations_met: usize,
    variant_obligations_unmet: usize,
}

fn compute_evidence_counts(ledger: &CoverageLedger) -> EvidenceCounts {
    EvidenceCounts {
        rows_with_discharging_predicate: ledger
            .rows
            .iter()
            .filter(|r| !r.discharging_predicates.is_empty())
            .count(),
        rows_with_containment_evidence: ledger
            .rows
            .iter()
            .filter(|r| r.containment.is_some())
            .count(),
        rows_mapped_to_conformance_construct: ledger
            .rows
            .iter()
            .filter(|r| !r.construct_ids.is_empty())
            .count(),
        rows_conformance_covered: ledger
            .rows
            .iter()
            .filter(|r| r.conformance_status == CoverageStatus::Covered)
            .count(),
        rows_unmappable: ledger
            .rows
            .iter()
            .filter(|r| r.conformance_status == CoverageStatus::Unmappable)
            .count(),
        total_rows: ledger.rows.len(),
        variant_obligations_met: ledger.rows.iter().filter(|r| obligation_met(r)).count(),
        variant_obligations_unmet: ledger.rows.iter().filter(|r| !obligation_met(r)).count(),
    }
}

/// One row of the "supported (Proven) vs. conformance-covered" cross-check: a pure filter over the ledger's own `disposition == Proven` rows, so it cannot drift from the ledger.
#[derive(Serialize)]
struct SupportedConformanceRow {
    kind: CharacteristicKind,
    status: CoverageStatus,
    construct_ids: Vec<String>,
}

fn supported_conformance_cross_check(ledger: &CoverageLedger) -> Vec<SupportedConformanceRow> {
    ledger
        .rows
        .iter()
        .filter(|r| r.disposition == Disposition::Proven)
        .map(|r| SupportedConformanceRow {
            kind: r.kind,
            status: r.conformance_status,
            construct_ids: r.construct_ids.clone(),
        })
        .collect()
}

#[derive(Serialize)]
struct PlanInteractionRow {
    tuple: String,
    status: String,
    tags: Vec<String>,
}

#[derive(Serialize)]
struct PlanInteractionSummary {
    grammar_path: String,
    required_total: usize,
    covered: usize,
    uncovered: usize,
    retired: usize,
    unexpected_tuples: usize,
    rows: Vec<PlanInteractionRow>,
}

/// Node-kind adjacency-tuple coverage for exactly one grammar's compiled plan; most tuples legitimately read `Uncovered` here, since the full-corpus picture is `tests/plan_interaction_coverage_gate.rs`'s job, not this command's.
fn plan_interaction_summary(grammar_path: &str, g: &Grammar) -> PlanInteractionSummary {
    let (plan, profile) = plan_and_profile(g);
    let refs = vec![(grammar_path, &plan, &profile)];
    let report = compute_interaction_coverage(&refs);

    let rows: Vec<PlanInteractionRow> = report
        .required
        .iter()
        .map(|r| PlanInteractionRow {
            tuple: format!("{:?}", r.tuple),
            status: format!("{:?}", r.status),
            tags: r.tags.iter().map(|k| format!("{k:?}")).collect(),
        })
        .collect();

    let covered = report
        .required
        .iter()
        .filter(|r| r.status == TupleStatus::Covered)
        .count();
    let uncovered = report
        .required
        .iter()
        .filter(|r| r.status == TupleStatus::Uncovered)
        .count();
    PlanInteractionSummary {
        grammar_path: grammar_path.to_string(),
        required_total: report.required.len(),
        covered,
        uncovered,
        retired: report.retired.len(),
        unexpected_tuples: report.unexpected_tuples.len(),
        rows,
    }
}

fn build_headline(ledger: &CoverageLedger) -> String {
    let met = ledger.rows.iter().filter(|r| obligation_met(r)).count();
    let total = ledger.rows.len();
    if total > 0 && met == total {
        format!("FULL HC coverage: all {total} reachable variant obligations meet their disposition-specific fixture and containment requirements or have a documented permanent refusal.")
    } else {
        format!("NOT full HC coverage: {met}/{total} reachable variant obligations met; {} lack required fixture or containment evidence or have an unresolved refusal. ConfirmOnly is a valid final disposition.", total - met)
    }
}

#[derive(Serialize)]
struct CoverageSummary {
    schema_version: u32,
    headline: String,
    fixture_load_failure_count: usize,
    fixture_load_failures: Vec<FixtureLoadFailure>,
    invalid_variant_tags: Vec<String>,
    /// Fixture population, not a claim about the ledger below: only `producible` names FieldWorks-facing coverage.
    producibility: ProducibilityCensus,
    disposition_counts: DispositionCounts,
    evidence_counts: EvidenceCounts,
    supported_conformance_cross_check: Vec<SupportedConformanceRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_interaction: Option<PlanInteractionSummary>,
    ledger: CoverageLedger,
}

fn build_summary(grammar: Option<(&str, &Grammar)>) -> CoverageSummary {
    let registry = default_registry();
    // Claims its scope: a user running the CLI has no environment claim to inherit.
    let fixtures = discover_scoped(ConformanceScope::All);
    let producibility = producibility_census(&fixtures);
    let replay = passing_covered_constructs(&fixtures);
    let covered_refs: HashSet<&str> = replay
        .passing_constructs
        .iter()
        .map(String::as_str)
        .collect();
    let ledger = build_ledger(&registry, &covered_refs);

    let disposition_counts = compute_disposition_counts(&ledger);
    let evidence_counts = compute_evidence_counts(&ledger);
    let supported_conformance_cross_check = supported_conformance_cross_check(&ledger);
    let headline = build_headline(&ledger);
    let plan_interaction = grammar.map(|(path, g)| plan_interaction_summary(path, g));

    CoverageSummary {
        schema_version: COVERAGE_CLI_SCHEMA_VERSION,
        headline,
        fixture_load_failure_count: replay.load_failures.len(),
        fixture_load_failures: replay.load_failures,
        invalid_variant_tags: replay.invalid_variant_tags,
        producibility,
        disposition_counts,
        evidence_counts,
        supported_conformance_cross_check,
        plan_interaction,
        ledger,
    }
}

fn render_human(summary: &CoverageSummary) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "pangloss coverage (schema v{})\n\n{}\n\n",
        summary.schema_version, summary.headline
    ));

    out.push_str(&format!(
        "Fixture grammar load failures: {}\n",
        summary.fixture_load_failure_count
    ));
    for failure in &summary.fixture_load_failures {
        out.push_str(&format!("  {}: {}\n", failure.fixture, failure.error));
    }
    for error in &summary.invalid_variant_tags {
        out.push_str(&format!("Invalid variant tag: {error}\n"));
    }
    out.push('\n');

    let p = &summary.producibility;
    out.push_str("FieldWorks producibility census (this is NOT the ledger below -- only the \"producible\" bucket is FieldWorks-facing coverage):\n");
    out.push_str(&format!("  producible:  {}\n", p.producible.len()));
    out.push_str(&format!("  engine-only: {}\n", p.engine_only.len()));
    out.push_str(&format!("  unmarked:    {}\n\n", p.unmarked.len()));

    let d = &summary.disposition_counts;
    out.push_str("Disposition counts:\n");
    out.push_str(&format!("  Proven:          {}\n", d.proven));
    out.push_str(&format!("  ConfirmOnly:     {}\n", d.confirm_only));
    out.push_str(&format!("  Admit:           {}\n", d.admit));
    out.push_str(&format!("  Refuse:          {}\n", d.refuse));
    out.push_str(&format!("  Total:           {}\n\n", d.total));

    let e = &summary.evidence_counts;
    out.push_str("Evidence counts:\n");
    out.push_str(&format!(
        "  Rows with a discharging predicate:           {}/{}\n",
        e.rows_with_discharging_predicate, e.total_rows
    ));
    out.push_str(&format!(
        "  Rows with curated containment-test evidence: {}/{}\n",
        e.rows_with_containment_evidence, e.total_rows
    ));
    out.push_str(&format!(
        "  Rows mapped to a conformance construct id:   {}/{}\n",
        e.rows_mapped_to_conformance_construct, e.total_rows
    ));
    out.push_str(&format!(
        "  Rows conformance-Covered by a passing fixture: {}/{}\n",
        e.rows_conformance_covered, e.total_rows
    ));
    out.push_str(&format!(
        "  Rows Unmappable (no constructs.txt id exists): {}\n\n",
        e.rows_unmappable
    ));

    out.push_str("Supported (Proven) constructs -- ADR 0001 conformance cross-check:\n");
    for row in &summary.supported_conformance_cross_check {
        out.push_str(&format!(
            "  {:?}: {:?} (construct ids: {:?})\n",
            row.kind, row.status, row.construct_ids
        ));
    }
    out.push('\n');

    out.push_str("Full variant obligation ledger:\n");
    for row in &summary.ledger.rows {
        let preds: Vec<&str> = row
            .discharging_predicates
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        let containment = row
            .containment
            .as_ref()
            .map(|c| c.citation.as_str())
            .unwrap_or("(none -- honest gap)");
        out.push_str(&format!(
            "  {:?} variant={:?}: disposition={:?} variant_disposition={:?} permanent_refusal={:?} predicates={:?} conformance={:?} construct_ids={:?}\n    containment: {}\n",
            row.kind,
            row.variant,
            row.disposition,
            row.variant_disposition,
            row.permanent_refusal,
            preds,
            row.conformance_status,
            row.construct_ids,
            containment
        ));
    }

    match &summary.plan_interaction {
        Some(pi) => {
            out.push_str(&format!(
                "\nPlan-node interaction coverage (grammar: {}):\n  required={} covered={} \
                 uncovered={} retired={} unexpected_tuples={}\n",
                pi.grammar_path,
                pi.required_total,
                pi.covered,
                pi.uncovered,
                pi.retired,
                pi.unexpected_tuples
            ));
            for row in &pi.rows {
                out.push_str(&format!(
                    "    {}: {} (tags: {:?})\n",
                    row.tuple, row.status, row.tags
                ));
            }
        }
        None => {
            out.push_str(
                "\nPlan-node interaction coverage: NOT COMPUTED -- no --grammar=<path> was given, \
                 so this section is honestly omitted rather than fabricated.\n",
            );
        }
    }

    out
}

/// `<out.json>` omitted and `--json` unset prints the human-readable summary to stdout; `--json` alone prints canonical JSON to stdout; `<out.json>` given always writes canonical JSON there regardless of `--json`.
pub fn run_coverage(args: &[String]) -> Result<(), String> {
    let mut json = false;
    let mut grammar_path: Option<String> = None;
    let mut out_path: Option<String> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--grammar" => {
                let v = it.next().ok_or("--grammar requires a value")?;
                grammar_path = Some(v.clone());
            }
            s if s.starts_with("--grammar=") => {
                grammar_path = Some(s["--grammar=".len()..].to_string());
            }
            s => {
                crate::reject_unknown_option("coverage", s)?;
                if out_path.is_some() {
                    return Err(format!(
                        "usage: coverage [--json] [--grammar=<path>] [<out.json>]; unexpected extra \
                         argument: {s}"
                    ));
                }
                out_path = Some(s.to_string());
            }
        }
    }

    let loaded_grammar = match &grammar_path {
        Some(path) => {
            let (grammar, warnings) = crate::load_grammar(path)?;
            crate::print_grammar_warnings(&warnings);
            Some((path.clone(), grammar))
        }
        None => None,
    };

    let summary = build_summary(loaded_grammar.as_ref().map(|(path, g)| (path.as_str(), g)));

    match &out_path {
        Some(path) => {
            let json_str = serde_json::to_string_pretty(&summary)
                .map_err(|e| format!("serialize coverage summary: {e}"))?;
            fs::write(path, &json_str).map_err(|e| format!("write {path}: {e}"))?;
            eprintln!(
                "coverage: wrote {path} ({} variant obligation rows, {} conformance-covered)",
                summary.disposition_counts.total, summary.evidence_counts.rows_conformance_covered
            );
        }
        None if json => {
            let json_str = serde_json::to_string_pretty(&summary)
                .map_err(|e| format!("serialize coverage summary: {e}"))?;
            println!("{json_str}");
        }
        None => {
            print!("{}", render_human(&summary));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
