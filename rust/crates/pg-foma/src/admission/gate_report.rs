//! What the gate decided and what it cost, as one line.

use super::{Guess, Tally};
use std::fmt;

/// One stage's state at the end of the gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageState {
    On,
    Off(String),
}

impl StageState {
    pub fn is_on(&self) -> bool {
        matches!(self, Self::On)
    }
}

impl fmt::Display for StageState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::On => f.write_str("on"),
            Self::Off(reason) => write!(f, "off({reason})"),
        }
    }
}

/// The gate's decision with every figure it rests on, in parser-work units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateReport {
    pub admission: StageState,
    pub tiled: StageState,
    /// How root guessing meets the stages.
    pub guess: Guess,
    /// Words in the batch.
    pub words: usize,
    /// Words parsed plainly to measure the mean plain cost.
    pub warmup_words: usize,
    /// Words parsed both plainly and restricted.
    pub calibration_words: usize,
    /// Construction units the admission build spent, a refused build's included.
    pub admission_units: u64,
    /// Construction units the tiled build spent, a refused build's included.
    pub tiled_units: u64,
    /// Both builds' units at their weights.
    pub construction_cost: u64,
    /// Cost of work done only to decide: restricted parses and walks on the sample.
    pub calibration_cost: u64,
    /// Plain cost minus restricted cost, summed over the calibration words.
    pub saving: u64,
    /// Admission walk cost over the calibration words.
    pub filter_cost: u64,
    /// Projected net over the batch: the stages' savings on the words still to run, minus construction and calibration. Negative when the gate spent more than it can win back; when both stages are off, what was spent.
    pub net: i64,
}

impl GateReport {
    pub(super) fn off(words: usize, reason: &str) -> Self {
        Self {
            admission: StageState::Off(reason.to_owned()),
            tiled: StageState::Off("admission is off".to_owned()),
            guess: Guess::None,
            words,
            warmup_words: 0,
            calibration_words: 0,
            admission_units: 0,
            tiled_units: 0,
            construction_cost: 0,
            calibration_cost: 0,
            saving: 0,
            filter_cost: 0,
            net: 0,
        }
    }
}

impl fmt::Display for GateReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "admission={}\ttiled={}\tguess={}\tsample=even\twords={}\twarmup={}\tcalibration={}\tadmission_units={}\ttiled_units={}\tconstruction_cost={}\tcalibration_cost={}\tsaving={}\tfilter_cost={}\tnet={}",
            self.admission,
            self.tiled,
            self.guess.label(),
            self.words,
            self.warmup_words,
            self.calibration_words,
            self.admission_units,
            self.tiled_units,
            self.construction_cost,
            self.calibration_cost,
            self.saving,
            self.filter_cost,
            self.net
        )
    }
}

/// The `PREFILTER-TOTAL` line: what a whole run cost in parser work, the outcomes, the filter walks, the builds and the calibration.
pub fn total_line(report: &GateReport, tally: &Tally) -> String {
    let filter = tally.filter_cost();
    let total = tally.outcome_cost + filter + report.construction_cost + report.calibration_cost;
    format!(
        "words={}\trestricted={}\trejected={}\tadmit_steps={}\ttiled_steps={}\tfilter_cost={}\toutcome_cost={}\tconstruction_cost={}\tcalibration_cost={}\ttotal_cost={}",
        tally.words,
        tally.restricted,
        tally.rejected,
        tally.admit_steps,
        tally.tiled_steps,
        filter,
        tally.outcome_cost,
        report.construction_cost,
        report.calibration_cost,
        total
    )
}
