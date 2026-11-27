//! The forced gate: every stage that is valid for the grammar runs on every word, with no sample and no budget.

use super::{Decided, GateReport, Guess, Prefilter, StageState, Stages};
use crate::admission::{
    build_rule_admission_within, build_tiled_from, tiled_prospect, AdmissionRefusal,
    ADMISSION_UNIT_WEIGHT, TILED_UNIT_WEIGHT, UNLIMITED_UNITS,
};
use foma::options::FomaOptions;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

/// Why no stage may run for this parse at all, if so: the same soundness preconditions as `decide`.
fn unsound(morpher: &Morpher, options: &ParseOptions) -> Option<&'static str> {
    if options.guess_only {
        return Some("the parse is guess-only");
    }
    if morpher.has_supplied_roots() {
        return Some("a supplied-root overlay is present");
    }
    None
}

/// Builds both stages without a budget and keeps each one that is valid; nothing is parsed, so every word goes through `Prefilter::parse`.
pub fn decide_forced(
    grammar: &Grammar,
    morpher: &Morpher,
    options: &ParseOptions,
    words: usize,
) -> Decided {
    let mut report = GateReport::off(words, "");
    report.guess = Guess::of(morpher, options);
    let plain = |report| Decided {
        prefilter: Prefilter::off(),
        outcomes: Vec::new(),
        report,
    };
    if let Some(reason) = unsound(morpher, options) {
        report.admission = StageState::Off(reason.to_owned());
        report.tiled = StageState::Off(reason.to_owned());
        return plain(report);
    }
    let admission =
        match build_rule_admission_within(&FomaOptions::default(), grammar, UNLIMITED_UNITS) {
            Ok(admission) => admission,
            Err(refusal) => {
                report.admission_units = match refusal {
                    AdmissionRefusal::ConstructionBudget { spent, .. } => spent,
                    _ => 0,
                };
                report.construction_cost = report.admission_units * ADMISSION_UNIT_WEIGHT;
                report.admission = StageState::Off(refusal.reason());
                return plain(report);
            }
        };
    report.admission = StageState::On;
    report.admission_units = admission.construction_units();
    report.construction_cost = report.admission_units * ADMISSION_UNIT_WEIGHT;
    let prospect = tiled_prospect(grammar, &admission);
    let mut tiled = None;
    if let Some(reason) = prospect.cannot_be_active() {
        report.tiled = StageState::Off(reason.to_owned());
    } else {
        let built = build_tiled_from(&FomaOptions::default(), grammar, prospect, UNLIMITED_UNITS);
        report.tiled_units = built.construction_units();
        report.construction_cost += report.tiled_units * TILED_UNIT_WEIGHT;
        match built.inactive_reason() {
            Some(reason) => report.tiled = StageState::Off(reason.to_owned()),
            None => {
                report.tiled = StageState::On;
                tiled = Some(built);
            }
        }
    }
    report.net = -(report.construction_cost as i64);
    Decided {
        prefilter: Prefilter {
            stages: Some(Stages { admission, tiled }),
        },
        outcomes: Vec::new(),
        report,
    }
}
