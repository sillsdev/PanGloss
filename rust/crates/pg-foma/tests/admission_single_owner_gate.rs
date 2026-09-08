//! Differential measurement: the capability gate, the selector's own report, and the certified verdict must be reading the same admission fact, checked BEFORE any of the three is changed to share one owner.

use pg_conformance_fixtures::{discover_scoped, ConformanceScope};
use pg_foma::analyzer::FomaProposer;
use pg_foma::backend_selection::{select_backends, BackendReport};
use pg_foma::capability_gate::refuse_unless_admitted;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::readiness_policy::policy_v1;
use pg_foma::readiness_verdict::{certify_with_semantics, CapabilitySummary, TrustStatus};
use pg_foma::strategy_coverage::ALL_STRATEGIES;

/// One fixture x strategy observation over (a) the gate and (b) the selector's own report.
struct Row {
    label: String,
    strategy: EmissionStrategy,
    /// (a) `capability_gate::refuse_unless_admitted(...).is_ok()`.
    gate_admits: bool,
    /// (b) the selector's own report; `None` means no report was composed for this strategy at all.
    selector_report: Option<bool>,
}

/// One fixture's (c) observation, recorded once since `readiness_verdict` always certifies `FomaProposer::EMISSION_STRATEGY` rather than a caller-named strategy.
struct ReadinessRow {
    label: String,
    /// Whether the certified verdict's capability summary is a refusal.
    readiness_refuses: bool,
    /// (a)/(b) at the exact strategy `readiness_verdict` certifies, for direct comparison.
    gate_admits_at_certified_strategy: bool,
}

#[derive(Default)]
struct Counts {
    total: usize,
    none_reports: usize,
    /// (a) vs (b) disagreements, `None` scored per the gate's current fail-open handling.
    gate_selector_disagree: usize,
    /// (c) vs (a)/(b) disagreements at the strategy `readiness_verdict` actually certifies.
    readiness_disagree: usize,
}

fn sweep() -> (Vec<Row>, Vec<ReadinessRow>) {
    let mut rows = Vec::new();
    let mut readiness_rows = Vec::new();
    let policy = policy_v1();
    for fixture in discover_scoped(ConformanceScope::All) {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        let label = fixture.label();
        let semantics = GrammarSemantics::derive(&grammar);
        let selection = select_backends(&semantics);

        for &strategy in ALL_STRATEGIES {
            let gate_admits = refuse_unless_admitted(&grammar, strategy).is_ok();
            let selector_report = selection
                .report_for(strategy)
                .map(BackendReport::can_represent);
            rows.push(Row {
                label: label.clone(),
                strategy,
                gate_admits,
                selector_report,
            });
        }

        let readiness = certify_with_semantics(&semantics, &TrustStatus::Proven, None, &policy);
        let readiness_refuses = matches!(readiness.capability, CapabilitySummary::Refuse { .. });
        let gate_admits_at_certified_strategy =
            refuse_unless_admitted(&grammar, FomaProposer::EMISSION_STRATEGY).is_ok();
        readiness_rows.push(ReadinessRow {
            label,
            readiness_refuses,
            gate_admits_at_certified_strategy,
        });
    }
    (rows, readiness_rows)
}

fn counts(rows: &[Row], readiness_rows: &[ReadinessRow]) -> Counts {
    let mut counts = Counts {
        total: rows.len(),
        ..Counts::default()
    };
    for row in rows {
        match row.selector_report {
            None => {
                counts.none_reports += 1;
                // Current fail-open handling: a missing report reads as admitted.
                if !row.gate_admits {
                    counts.gate_selector_disagree += 1;
                }
            }
            Some(can_represent) => {
                if row.gate_admits != can_represent {
                    counts.gate_selector_disagree += 1;
                }
            }
        }
    }
    for row in readiness_rows {
        // Agreement is `readiness_refuses != gate_admits`; equal values means a disagreement.
        if row.readiness_refuses == row.gate_admits_at_certified_strategy {
            counts.readiness_disagree += 1;
        }
    }
    counts
}

/// The measurement itself: prints the full agreement/divergence table and pins today's counts.
#[test]
fn report_admission_owner_agreement() {
    let (rows, readiness_rows) = sweep();
    assert!(
        rows.len() > 100,
        "the sweep must actually observe the fixture set; got {} rows",
        rows.len()
    );
    let counts = counts(&rows, &readiness_rows);

    eprintln!(
        "admission-single-owner: {} (fixture x strategy) observations",
        counts.total
    );
    eprintln!("selector reports composed as None: {}", counts.none_reports);
    eprintln!(
        "gate (a) vs selector (b) disagreements: {}",
        counts.gate_selector_disagree
    );
    eprintln!(
        "readiness (c) vs gate/selector at the certified strategy, disagreements: {}",
        counts.readiness_disagree
    );
    eprintln!("fixtures certified for readiness: {}", readiness_rows.len());

    for row in &rows {
        if row.selector_report.is_none() {
            eprintln!("  NONE report: {} x {}", row.label, row.strategy.label());
        }
    }
    for row in &readiness_rows {
        if row.readiness_refuses == row.gate_admits_at_certified_strategy {
            eprintln!(
                "  readiness/gate divergence: {} (readiness_refuses={}, gate_admits={})",
                row.label, row.readiness_refuses, row.gate_admits_at_certified_strategy
            );
        }
    }

    // Ratchet: (a) and (b) must never disagree, before or after they become one call.
    assert_eq!(
        counts.gate_selector_disagree, 0,
        "the gate and the selector's own report disagree on {} observation(s)",
        counts.gate_selector_disagree
    );

    // Measures the claim rather than assuming it: see backend_selection.rs's ALL_STRATEGIES doc.
    assert_eq!(
        counts.none_reports, 0,
        "{} (fixture x strategy) observation(s) had no selector report at all",
        counts.none_reports
    );
}
