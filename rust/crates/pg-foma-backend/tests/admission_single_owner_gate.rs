//! Differential measurement: the capability gate and the selector's own report must read the same admission fact.

use pg_conformance_fixtures::{discover_scoped, ConformanceScope};
use pg_foma::capability_gate::refuse_unless_admitted;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_foma_backend::backend_selection::{select_backends, BackendReport};

/// One fixture x strategy observation over (a) the gate and (b) the selector's own report.
struct Row {
    label: String,
    strategy: EmissionStrategy,
    /// (a) `capability_gate::refuse_unless_admitted(...).is_ok()`.
    gate_admits: bool,
    /// (b) the selector's own report; `None` means no report was composed for this strategy at all.
    selector_report: Option<bool>,
}

#[derive(Default)]
struct Counts {
    total: usize,
    none_reports: usize,
    /// (a) vs (b) disagreements, `None` scored per the gate's current fail-open handling.
    gate_selector_disagree: usize,
}

fn sweep() -> Vec<Row> {
    let mut rows = Vec::new();
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
    }
    rows
}

fn counts(rows: &[Row]) -> Counts {
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
    counts
}

/// The measurement itself: prints the full agreement/divergence table and pins today's counts.
#[test]
fn report_admission_owner_agreement() {
    let rows = sweep();
    assert!(
        rows.len() > 100,
        "the sweep must actually observe the fixture set; got {} rows",
        rows.len()
    );
    let counts = counts(&rows);

    eprintln!(
        "admission-single-owner: {} (fixture x strategy) observations",
        counts.total
    );
    eprintln!("selector reports composed as None: {}", counts.none_reports);
    eprintln!(
        "gate (a) vs selector (b) disagreements: {}",
        counts.gate_selector_disagree
    );

    for row in &rows {
        if row.selector_report.is_none() {
            eprintln!("  NONE report: {} x {}", row.label, row.strategy.label());
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
