//! Suggests new fixed weights from per-word measurements, for tuning the code's constants.

use crate::{Counts, ALLOCATION_WEIGHT, BYTE_WEIGHT, REGISTER_WEIGHT, WEIGHT_SCALE, WORK_WEIGHT};

/// Work, bytes, registers copied, allocator calls.
const TERMS: usize = 4;

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
    pub allocation_factor: f64,
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

fn columns(counts: &Counts, weights: [f64; TERMS]) -> [f64; TERMS] {
    [
        counts.work as f64 * weights[0],
        counts.bytes as f64 * weights[1],
        counts.registers as f64 * weights[2],
        counts.allocations as f64 * weights[3],
    ]
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// The fixed weights as the fit's starting point.
pub fn current_weights() -> [f64; TERMS] {
    [WORK_WEIGHT, BYTE_WEIGHT, REGISTER_WEIGHT, ALLOCATION_WEIGHT]
        .map(|weight| weight as f64 / WEIGHT_SCALE as f64)
}

fn usable(samples: &[Sample]) -> Result<Vec<&Sample>, Refusal> {
    let usable: Vec<&Sample> = samples
        .iter()
        .filter(|sample| sample.counts.work >= MIN_WORK && sample.measured > 0.0)
        .collect();
    if usable.len() < MIN_WORDS {
        return Err(Refusal::TooFewWords {
            usable: usable.len(),
        });
    }
    Ok(usable)
}

/// Weights with the root-mean-square relative error they leave.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    pub work: f64,
    pub byte: f64,
    pub register: f64,
    pub allocation: f64,
    pub error: f64,
}

/// The exact fit of the weights to instruction counts, and what the words can tell apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solution {
    /// The non-negative least-squares fit on relative error, each weight free.
    pub free: Weights,
    /// The free fit's error when work, bytes, registers or allocations is left out and the rest are refitted.
    pub error_without: [f64; TERMS],
    pub words: usize,
}

/// Solves `a x = b` for up to three unknowns; `None` when the columns are not independent.
fn solve_linear<const N: usize>(
    mut a: [[f64; N]; N],
    mut b: [f64; N],
    n: usize,
) -> Option<[f64; N]> {
    for col in 0..n {
        let pivot = (col..n).max_by(|x, y| a[*x][col].abs().total_cmp(&a[*y][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let (top, top_b) = (a[col], b[col]);
        for row in col + 1..n {
            let factor = a[row][col] / top[col];
            for (cell, above) in a[row].iter_mut().zip(top) {
                *cell -= factor * above;
            }
            b[row] -= factor * top_b;
        }
    }
    let mut x = [0.0; N];
    for row in (0..n).rev() {
        let rest: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - rest) / a[row][row];
    }
    Some(x)
}

/// The normal matrix and right-hand side of `rows · x = 1` with each column scaled to unit length, and those lengths.
fn normal_equations<const N: usize>(rows: &[[f64; N]]) -> ([[f64; N]; N], [f64; N], [f64; N]) {
    let mut scale = [0.0; N];
    for row in rows {
        for (total, value) in scale.iter_mut().zip(row) {
            *total += value * value;
        }
    }
    let scale = scale.map(f64::sqrt);
    let unit = |row: &[f64; N]| {
        let mut out = *row;
        for (value, length) in out.iter_mut().zip(scale) {
            *value = if length > 0.0 { *value / length } else { 0.0 };
        }
        out
    };
    let (mut gram, mut rhs) = ([[0.0; N]; N], [0.0; N]);
    for row in rows.iter().map(unit) {
        for ((line, total), value) in gram.iter_mut().zip(rhs.iter_mut()).zip(row) {
            *total += value;
            for (cell, other) in line.iter_mut().zip(row) {
                *cell += value * other;
            }
        }
    }
    (gram, rhs, scale)
}

/// Non-negative least squares of `rows · x = 1` over the `allowed` columns and its error: the best plain fit over every column set with no negative weight.
fn least_squares<const N: usize>(rows: &[[f64; N]], allowed: [bool; N]) -> ([f64; N], f64) {
    let (gram, rhs, scale) = normal_equations(rows);
    let count = rows.len() as f64;
    let mut best = ([0.0; N], 1.0);
    for set in 1u32..(1 << N) {
        let columns: Vec<usize> = (0..N)
            .filter(|j| set & (1 << j) != 0 && allowed[*j] && scale[*j] > 0.0)
            .collect();
        if columns.len() != set.count_ones() as usize {
            continue;
        }
        let (mut a, mut b) = ([[0.0; N]; N], [0.0; N]);
        for (r, j) in columns.iter().enumerate() {
            b[r] = rhs[*j];
            for (c, k) in columns.iter().enumerate() {
                a[r][c] = gram[*j][*k];
            }
        }
        let Some(x) = solve_linear(a, b, columns.len()) else {
            continue;
        };
        if x[..columns.len()].iter().any(|value| *value < 0.0) {
            continue;
        }
        let mut squares = count;
        for (r, j) in columns.iter().enumerate() {
            squares -= 2.0 * rhs[*j] * x[r];
            for (c, k) in columns.iter().enumerate() {
                squares += x[r] * gram[*j][*k] * x[c];
            }
        }
        let error = (squares.max(0.0) / count).sqrt();
        if error < best.1 {
            let mut weights = [0.0; N];
            for (r, j) in columns.iter().enumerate() {
                weights[*j] = x[r] / scale[*j];
            }
            best = (weights, error);
        }
    }
    best
}

/// The exact fit for instruction counts, which load cannot disturb, so no bound is needed to steady it.
pub fn solve(samples: &[Sample]) -> Result<Solution, Refusal> {
    let usable = usable(samples)?;
    let rows: Vec<[f64; TERMS]> = usable
        .iter()
        .map(|sample| columns(&sample.counts, [1.0; TERMS]).map(|x| x / sample.measured))
        .collect();
    let (free, error) = least_squares(&rows, [true; TERMS]);
    let error_without = [0, 1, 2, 3].map(|left_out| {
        let mut allowed = [true; TERMS];
        allowed[left_out] = false;
        least_squares(&rows, allowed).1
    });
    Ok(Solution {
        free: Weights {
            work: free[0],
            byte: free[1],
            register: free[2],
            allocation: free[3],
            error,
        },
        error_without,
        words: usable.len(),
    })
}

/// Fits bounded multipliers on relative error. `cpu_share` is `None` for a count of instructions,
/// which load cannot disturb.
pub fn fit(samples: &[Sample], cpu_share: Option<f64>) -> Result<Fit, Refusal> {
    let weights = current_weights();
    if let Some(cpu_share) = cpu_share.filter(|share| *share < MIN_CPU_SHARE) {
        return Err(Refusal::MachineBusy { cpu_share });
    }
    let usable = usable(samples)?;
    let speed = median(
        usable
            .iter()
            .map(|sample| sample.measured / columns(&sample.counts, weights).iter().sum::<f64>())
            .collect(),
    );
    let rows: Vec<[f64; TERMS]> = usable
        .iter()
        .map(|sample| columns(&sample.counts, weights).map(|x| x * speed / sample.measured))
        .collect();
    let mut factors = [1.0f64; TERMS];
    for _ in 0..SWEEPS {
        for j in 0..TERMS {
            let (mut above, mut below) = (0.0, 0.0);
            for row in &rows {
                let others: f64 = (0..TERMS)
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
        allocation_factor: factors[3],
        speed,
        words: usable.len(),
    })
}

#[cfg(test)]
mod tests;
