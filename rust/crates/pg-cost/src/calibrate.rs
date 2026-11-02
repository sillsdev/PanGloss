//! Suggests new fixed weights from per-word measurements, for tuning the code's constants.

use crate::{Counts, BYTE_WEIGHT, REGISTER_WEIGHT, WORK_WEIGHT};

/// The furthest one run may pull a weight from its current value.
pub const MIN_FACTOR: f64 = 0.75;
pub const MAX_FACTOR: f64 = 1.5;
/// Below this share of wall time on the CPU, a timing run measured the machine's load instead.
pub const MIN_CPU_SHARE: f64 = 0.95;
/// Words under this many work units are dominated by fixed start-up cost.
const MIN_WORK: u64 = 1000;
const MIN_WORDS: usize = 8;
const SWEEPS: usize = 500;

/// One word's counts and what it was measured to cost, in any single unit.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub counts: Counts,
    pub measured: f64,
}

/// Bounded multipliers for the fixed weights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub work_factor: f64,
    pub byte_factor: f64,
    pub register_factor: f64,
    /// Measured units per fixed-cost unit: the machine's speed, which is not part of a weight.
    pub speed: f64,
    pub words: usize,
}

/// Why a run must not be used to change weights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Refusal {
    TooFewWords { usable: usize },
    MachineBusy { cpu_share: f64 },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooFewWords { usable } => {
                write!(f, "{usable} usable words; at least {MIN_WORDS} are needed")
            }
            Self::MachineBusy { cpu_share } => write!(
                f,
                "the parse held the CPU {:.1}% of wall time; {:.1}% is required",
                cpu_share * 100.0,
                MIN_CPU_SHARE * 100.0
            ),
        }
    }
}

fn columns(counts: &Counts, weights: [f64; 3]) -> [f64; 3] {
    [
        counts.work as f64 * weights[0],
        counts.bytes as f64 * weights[1],
        counts.registers as f64 * weights[2],
    ]
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// Fits bounded multipliers on relative error. `cpu_share` is `None` for a count of instructions,
/// which load cannot disturb.
pub fn fit(samples: &[Sample], cpu_share: Option<f64>) -> Result<Fit, Refusal> {
    fit_from(samples, cpu_share, current_weights())
}

/// The fixed weights as the fit's starting point.
pub fn current_weights() -> [f64; 3] {
    [
        WORK_WEIGHT as f64,
        BYTE_WEIGHT as f64,
        REGISTER_WEIGHT as f64,
    ]
}

/// `fit` starting from other weights, so a fit that hit a bound can be continued without editing the constants.
pub fn fit_from(
    samples: &[Sample],
    cpu_share: Option<f64>,
    weights: [f64; 3],
) -> Result<Fit, Refusal> {
    if let Some(cpu_share) = cpu_share.filter(|share| *share < MIN_CPU_SHARE) {
        return Err(Refusal::MachineBusy { cpu_share });
    }
    let usable: Vec<&Sample> = samples
        .iter()
        .filter(|sample| sample.counts.work >= MIN_WORK && sample.measured > 0.0)
        .collect();
    if usable.len() < MIN_WORDS {
        return Err(Refusal::TooFewWords {
            usable: usable.len(),
        });
    }
    let speed = median(
        usable
            .iter()
            .map(|sample| sample.measured / columns(&sample.counts, weights).iter().sum::<f64>())
            .collect(),
    );
    let rows: Vec<[f64; 3]> = usable
        .iter()
        .map(|sample| columns(&sample.counts, weights).map(|x| x * speed / sample.measured))
        .collect();
    let mut factors = [1.0f64; 3];
    for _ in 0..SWEEPS {
        for j in 0..3 {
            let (mut above, mut below) = (0.0, 0.0);
            for row in &rows {
                let others: f64 = (0..3)
                    .filter(|k| *k != j)
                    .map(|k| factors[k] * row[k])
                    .sum();
                above += row[j] * (1.0 - others);
                below += row[j] * row[j];
            }
            if below > 0.0 {
                factors[j] = (above / below).clamp(MIN_FACTOR, MAX_FACTOR);
            }
        }
    }
    Ok(Fit {
        work_factor: factors[0],
        byte_factor: factors[1],
        register_factor: factors[2],
        speed,
        words: usable.len(),
    })
}

#[cfg(test)]
mod tests;
