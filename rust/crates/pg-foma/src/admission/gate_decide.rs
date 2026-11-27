//! The gate's sequence: preconditions, warm-up, build, calibration, tiled stage.

use super::tiled::tiled_stage;
use super::{
    calibration_words, parse_cost, restricted_parse, staged, Decided, GateReport, Guess, Prefilter,
    RestrictedFn, StageState, Stages, ADMISSION_BUILD_SHARE, K_MAX, K_MIN, MIN_BATCH,
    SAVING_OVER_FILTER, WARMUP,
};
use crate::admission::{
    build_rule_admission_within, AdmissionRefusal, RuleAdmission, ADMISSION_UNIT_WEIGHT,
    ADMIT_STEP_WEIGHT,
};
use foma::options::FomaOptions;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use std::fmt;

/// Sample positions: `SAMPLE` evenly spaced indices of the batch, from its length alone.
const SAMPLE: usize = WARMUP + K_MAX;

/// The warm-up indices and the calibration candidates, each in index order, for a batch of `n >= MIN_BATCH` words.
pub(super) fn sample_positions(n: usize) -> (Vec<usize>, Vec<usize>) {
    let (mut warm, mut candidates) = (Vec::with_capacity(WARMUP), Vec::with_capacity(K_MAX));
    for m in 0..SAMPLE {
        let j = m * n / SAMPLE;
        if m % 3 == 1 {
            candidates.push(j);
        } else {
            warm.push(j);
        }
    }
    (warm, candidates)
}

/// `k` of the candidates, spread over the whole list so a short batch's calibration is not the head of it.
pub(super) fn calibration_candidates(candidates: &[usize], k: usize) -> Vec<usize> {
    (0..k)
        .map(|i| candidates[i * candidates.len() / k])
        .collect()
}

/// A result the gate must not see: it would be a soundness bug in a filter, not a gate outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateError {
    /// A restricted parse, or a tiled rejection, disagrees with the plain parse of the word.
    Disagreement { word: String, stage: &'static str },
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self::Disagreement { word, stage } = self;
        write!(
            f,
            "pre-filter soundness error: the {stage} result for {word:?} differs from the plain parse"
        )
    }
}

impl std::error::Error for GateError {}

pub(super) struct Ctx<'a, 'g> {
    pub(super) grammar: &'a Grammar,
    pub(super) morpher: &'a Morpher<'g>,
    pub(super) options: &'a ParseOptions,
    /// The caller's options with root guessing off, for the filters.
    pub(super) stage: ParseOptions,
}

/// Decides the stages for a batch of `words` and parses the leading words plainly on the way; the caller runs the rest through `Prefilter::parse`.
pub fn decide(
    grammar: &Grammar,
    morpher: &Morpher,
    options: &ParseOptions,
    words: &[String],
) -> Result<Decided, GateError> {
    decide_with(grammar, morpher, options, words, &restricted_parse)
}

/// The sequence with the restricted parse supplied, so a test can inject a filter that loses an analysis.
pub(super) fn decide_with(
    grammar: &Grammar,
    morpher: &Morpher,
    options: &ParseOptions,
    words: &[String],
    restricted: &RestrictedFn,
) -> Result<Decided, GateError> {
    let ctx = Ctx {
        grammar,
        morpher,
        options,
        stage: options.with_guess_root(false),
    };
    let n = words.len();
    let guess = Guess::of(morpher, options);
    if let Some(reason) = precondition(n, &ctx) {
        let mut report = GateReport::off(n, &reason);
        report.guess = guess;
        return Ok(off(Vec::new(), report));
    }
    let (warm, all_candidates) = sample_positions(n);
    let candidates = calibration_candidates(&all_candidates, calibration_words(n));
    let mut outcomes: Vec<(usize, ParseOutcome)> = warm
        .iter()
        .map(|&j| (j, morpher.parse_word_opts(&words[j], options)))
        .collect();
    let mut report = GateReport::off(n, "");
    report.guess = guess;
    report.warmup_words = WARMUP;
    let warmup_cost: u64 = outcomes.iter().map(|(_, o)| parse_cost(o)).sum();
    let projected_plain = (n - WARMUP) as u64 * (warmup_cost / WARMUP as u64);
    let budget_units = projected_plain / ADMISSION_BUILD_SHARE / ADMISSION_UNIT_WEIGHT;
    let built = build_rule_admission_within(&FomaOptions::default(), grammar, budget_units);
    let admission = match built {
        Ok(admission) => admission,
        Err(refusal) => {
            report.admission_units = match refusal {
                AdmissionRefusal::ConstructionBudget { spent, .. } => spent,
                _ => 0,
            };
            report.construction_cost = report.admission_units * ADMISSION_UNIT_WEIGHT;
            report.net = -(report.construction_cost as i64);
            report.admission = StageState::Off(refusal.reason());
            return Ok(off(outcomes, report));
        }
    };
    report.admission_units = admission.construction_units();
    report.construction_cost = report.admission_units * ADMISSION_UNIT_WEIGHT;
    let cal = calibrate(
        &ctx,
        &admission,
        (words, &candidates),
        &mut outcomes,
        restricted,
    )?;
    report.calibration_words = cal.sample.len();
    report.saving = cal.saving;
    report.filter_cost = cal.filter;
    report.calibration_cost = cal.extra;
    if !cal.on {
        report.admission = StageState::Off(format!(
            "calibration saving {} is not enough over filter cost {} in {} words",
            cal.saving,
            cal.filter,
            cal.sample.len()
        ));
        report.net = -((report.construction_cost + report.calibration_cost) as i64);
        return Ok(off(outcomes, report));
    }
    report.admission = StageState::On;
    let rest = (n - outcomes.len()) as i128;
    let admission_net =
        (i128::from(cal.saving) - i128::from(cal.filter)) * rest / cal.sample.len() as i128;
    let stage = tiled_stage(
        &ctx,
        &admission,
        (words, outcomes.as_slice()),
        (&cal, projected_plain),
    )?;
    report.tiled = stage.state;
    report.tiled_units = stage.units;
    report.construction_cost += stage.units * crate::admission::TILED_UNIT_WEIGHT;
    report.calibration_cost += stage.extra;
    let spent = report.construction_cost + report.calibration_cost;
    report.net = (admission_net + stage.net - i128::from(spent)) as i64;
    let prefilter = Prefilter {
        stages: Some(Stages {
            admission,
            tiled: stage.tiled,
        }),
    };
    Ok(Decided {
        prefilter,
        outcomes,
        report,
    })
}

fn off(outcomes: Vec<(usize, ParseOutcome)>, report: GateReport) -> Decided {
    Decided {
        prefilter: Prefilter::off(),
        outcomes,
        report,
    }
}

/// Why the gate is off before any work, if it is: a short batch, a guess-only parse or supplied roots.
fn precondition(n: usize, ctx: &Ctx<'_, '_>) -> Option<String> {
    if n < MIN_BATCH {
        return Some(format!("batch of {n} words is shorter than {MIN_BATCH}"));
    }
    if ctx.options.guess_only {
        return Some("the parse is guess-only".to_owned());
    }
    if ctx.morpher.has_supplied_roots() {
        return Some("a supplied-root overlay is present".to_owned());
    }
    None
}

pub(super) struct Calibration {
    pub(super) on: bool,
    /// The admitted rules of each calibration word.
    pub(super) sample: Vec<Vec<bool>>,
    pub(super) saving: u64,
    pub(super) filter: u64,
    /// Restricted parses and walks done only to decide.
    pub(super) extra: u64,
    pub(super) plain_total: u64,
    pub(super) restricted_total: u64,
}

/// Parses calibration words both ways until the saving settles the question; the plain outcome is each word's result.
pub(super) fn calibrate(
    ctx: &Ctx<'_, '_>,
    admission: &RuleAdmission,
    (words, candidates): (&[String], &[usize]),
    outcomes: &mut Vec<(usize, ParseOutcome)>,
    restricted: &RestrictedFn,
) -> Result<Calibration, GateError> {
    let mut cal = Calibration {
        on: false,
        sample: Vec::new(),
        saving: 0,
        filter: 0,
        extra: 0,
        plain_total: 0,
        restricted_total: 0,
    };
    for &j in candidates {
        let word = &words[j];
        let plain = ctx.morpher.parse_word_opts(word, ctx.options);
        let mut steps = 0;
        let admitted = admission.admit_counted(word, &mut steps);
        let filter = steps * ADMIT_STEP_WEIGHT;
        let run = staged(
            ctx.morpher,
            word,
            (&admitted, false),
            ctx.options,
            restricted,
        );
        let narrowed = run.outcome;
        if !agrees(&plain, &narrowed) {
            return Err(GateError::Disagreement {
                word: word.clone(),
                stage: "restricted parse",
            });
        }
        let plain_cost = parse_cost(&plain);
        cal.saving += plain_cost.saturating_sub(run.total_cost);
        cal.filter += filter;
        cal.extra += run.total_cost + filter;
        cal.plain_total += plain_cost;
        cal.restricted_total += run.first_cost;
        cal.sample.push(admitted);
        outcomes.push((j, plain));
        let k = cal.sample.len();
        if k >= K_MIN && cal.saving > 0 && cal.saving >= SAVING_OVER_FILTER * cal.filter {
            cal.on = true;
            return Ok(cal);
        }
    }
    cal.on = cal.sample.len() == candidates.len() && cal.saving > cal.filter;
    Ok(cal)
}

/// Whether a restricted outcome is the plain one: equal on a completed word, a superset on a word the plain parse could not finish.
fn agrees(plain: &ParseOutcome, narrowed: &ParseOutcome) -> bool {
    let incomplete = |o: &ParseOutcome| o.capped || o.timed_out || o.capture_capped;
    if plain.invalid_shape != narrowed.invalid_shape {
        return false;
    }
    let sorted = |o: &ParseOutcome| {
        let mut all: Vec<String> = o.structured.iter().map(|a| format!("{a:?}")).collect();
        all.sort();
        (all, o.guessed)
    };
    match (incomplete(plain), incomplete(narrowed)) {
        (false, false) => sorted(plain) == sorted(narrowed),
        (false, true) => false,
        (true, true) => true,
        (true, false) => plain.analyses.iter().all(|a| narrowed.analyses.contains(a)),
    }
}
