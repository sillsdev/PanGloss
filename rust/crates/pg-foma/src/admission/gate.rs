//! The load-time gate: whether the rule-admission filter and the tiled stage run for one batch.
//!
//! The gate decides by a short deterministic calibration in parser-work units, never by time. A batch of N words
//! runs, in order: the preconditions; a warm-up of plain parses that gives the mean plain cost; the admission
//! build under a share of the projected remaining plain cost; a calibration of plain and restricted parses of the
//! same words; and, when admission is on, the tiled stage on the same sample. The words sampled are spread evenly
//! over the batch, so an ordered word list does not bias the projection. Their plain outcomes are the batch's own
//! results at their indices.

#[path = "gate_decide.rs"]
mod decide;
#[path = "gate_forced.rs"]
mod forced;
#[path = "gate_report.rs"]
mod report;
#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "gate_tests_budget.rs"]
mod tests_budget;
#[cfg(test)]
#[path = "gate_tests_forced.rs"]
mod tests_forced;
#[cfg(test)]
#[path = "gate_tests_guess.rs"]
mod tests_guess;
#[cfg(test)]
#[path = "gate_tests_sample.rs"]
mod tests_sample;
#[path = "gate_tiled.rs"]
mod tiled;

use super::{RuleAdmission, Tiled, ADMIT_STEP_WEIGHT, TILED_STEP_WEIGHT};
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use pg_rules::stratum::RuleRef;

pub use decide::{decide, GateError};
pub use forced::decide_forced;
pub use report::{total_line, GateReport, StageState};

// The gate's constants, all in one place.

/// Evenly spaced words parsed plainly before anything is built.
pub const WARMUP: usize = 64;
/// Calibration words before the gate may stop with "on".
pub const K_MIN: usize = 8;
/// Calibration words at most.
pub const K_MAX: usize = 32;
/// A batch of N words calibrates on N / this words (clamped to K_MIN..=K_MAX): K restricted parses of N words.
pub const CALIBRATION_DIVISOR: usize = 64;
/// The fewest words in a gated batch: the batch length at which N / CALIBRATION_DIVISOR reaches K_MIN.
pub const MIN_BATCH: usize = K_MIN * CALIBRATION_DIVISOR;
/// The projected remaining plain cost divided by this is the most the admission build may cost.
///
/// A build that cannot fit spends its whole budget before anything can be learned, so this share is the nominal bound on what a refused build costs the batch; the stop is at a foma operation boundary, so a refused build can spend more.
pub const ADMISSION_BUILD_SHARE: u64 = 64;
/// The projected remaining plain cost divided by this is the most the tiled build may cost.
pub const TILED_BUILD_SHARE: u64 = 16;
/// The tiled build may also cost at most this fraction (as a divisor) of the projected potential of the tiled stage.
pub const TILED_POTENTIAL_SHARE: u64 = 2;
/// Admission stops calibrating early once the saving reaches this many times the filter cost.
pub const SAVING_OVER_FILTER: u64 = 4;

/// The calibration words for a batch of `n` words.
pub fn calibration_words(n: usize) -> usize {
    (n / CALIBRATION_DIVISOR).clamp(K_MIN, K_MAX)
}

/// The decided stages for one batch. Shared by reference across threads; it holds no mutable state.
pub struct Prefilter {
    stages: Option<Stages>,
}

struct Stages {
    admission: RuleAdmission,
    tiled: Option<Tiled>,
}

/// What `decide` leaves: the prefilter, the plain outcomes of the sampled words it parsed, and the report.
pub struct Decided {
    pub prefilter: Prefilter,
    /// The plain outcomes of the sampled words as (batch index, outcome): the warm-up words, then the calibration words, each in index order.
    pub outcomes: Vec<(usize, ParseOutcome)>,
    pub report: GateReport,
}

impl Prefilter {
    /// The plain parse for every word.
    pub fn off() -> Self {
        Self { stages: None }
    }

    pub fn admission_on(&self) -> bool {
        self.stages.is_some()
    }

    pub fn tiled_on(&self) -> bool {
        self.stages.as_ref().is_some_and(|s| s.tiled.is_some())
    }

    /// One word's outcome: plain when the gate is off; else the staged parse (tiled rejection, restricted parse, then one guess-only parse on a lexical miss).
    pub fn parse(&self, morpher: &Morpher, word: &str, options: &ParseOptions) -> ParseOutcome {
        self.parse_counted(morpher, word, options, &mut Tally::default())
    }

    /// [`Prefilter::parse`] that adds what the word cost to `tally`.
    pub fn parse_counted(
        &self,
        morpher: &Morpher,
        word: &str,
        options: &ParseOptions,
        tally: &mut Tally,
    ) -> ParseOutcome {
        tally.words += 1;
        let Some(stages) = &self.stages else {
            let outcome = morpher.parse_word_opts(word, options);
            tally.outcome_cost += parse_cost(&outcome);
            return outcome;
        };
        let admitted = stages.admission.admit_counted(word, &mut tally.admit_steps);
        let rejected = stages.tiled.as_ref().is_some_and(|tiled| {
            let base = options.with_guess_root(false);
            tiled.rejects_counted(word, &admitted, &base, &mut tally.tiled_steps)
        });
        let run = staged(
            morpher,
            word,
            (&admitted, rejected),
            options,
            &restricted_parse,
        );
        tally.restricted += u64::from(!rejected);
        tally.rejected += u64::from(rejected);
        tally.outcome_cost += parse_cost(&run.outcome);
        run.outcome
    }
}

/// What a run of words cost: counts of the paths taken and the filter walks, in table transitions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub words: u64,
    /// Words that went through the restricted parse.
    pub restricted: u64,
    /// Words the tiled stage rejected.
    pub rejected: u64,
    pub admit_steps: u64,
    pub tiled_steps: u64,
    /// Parser work of the outcomes, `parse_cost` summed.
    pub outcome_cost: u64,
}

impl Tally {
    /// One word whose outcome is a plain parse.
    pub fn add_plain(&mut self, outcome: &ParseOutcome) {
        self.words += 1;
        self.outcome_cost += parse_cost(outcome);
    }

    pub fn add(&mut self, other: &Tally) {
        self.words += other.words;
        self.restricted += other.restricted;
        self.rejected += other.rejected;
        self.admit_steps += other.admit_steps;
        self.tiled_steps += other.tiled_steps;
        self.outcome_cost += other.outcome_cost;
    }

    /// The admission and tiled walks in parser work.
    pub fn filter_cost(&self) -> u64 {
        self.admit_steps * ADMIT_STEP_WEIGHT + self.tiled_steps * TILED_STEP_WEIGHT
    }
}

/// How root guessing meets the stages: it is off, it cannot produce anything, or a guess-only parse follows a confirmed lexical miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guess {
    None,
    NoPatterns,
    Orchestrated,
}

impl Guess {
    pub fn of(morpher: &Morpher, options: &ParseOptions) -> Self {
        match (options.guess_root, morpher.lexical_patterns().is_empty()) {
            (false, _) => Self::None,
            (true, true) => Self::NoPatterns,
            (true, false) => Self::Orchestrated,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NoPatterns => "no-patterns",
            Self::Orchestrated => "orchestrated",
        }
    }
}

/// A word's outcome through the stages with what each step cost, in parser work.
pub(super) struct Staged {
    pub outcome: ParseOutcome,
    /// The tiled rejection or restricted parse alone.
    pub first_cost: u64,
    /// That plus the guess-only parse when one ran.
    pub total_cost: u64,
}

pub(super) type RestrictedFn<'a> =
    dyn Fn(&Morpher, &str, &[bool], &ParseOptions) -> ParseOutcome + 'a;

fn incomplete(outcome: &ParseOutcome) -> bool {
    outcome.capped || outcome.timed_out || outcome.capture_capped
}

/// One word through the stages with guessing off (`rejected`: the tiled stage proved no analysis), then one guess-only parse when guessing is on and the lexical miss is confirmed.
pub(super) fn staged(
    morpher: &Morpher,
    word: &str,
    (admitted, rejected): (&[bool], bool),
    options: &ParseOptions,
    restricted: &RestrictedFn,
) -> Staged {
    let base = options.with_guess_root(false);
    let guessing = Guess::of(morpher, options) == Guess::Orchestrated;
    let guess_only = options.with_guess_only(true);
    if rejected && guessing {
        let outcome = morpher.parse_word_opts(word, &guess_only);
        let cost = parse_cost(&outcome);
        return Staged {
            outcome,
            first_cost: 0,
            total_cost: cost,
        };
    }
    let first = if rejected {
        no_analysis(morpher, word, &base)
    } else {
        restricted(morpher, word, admitted, &base)
    };
    let first_cost = parse_cost(&first);
    let miss = first.analyses.is_empty() && !incomplete(&first) && !first.invalid_shape;
    if !(guessing && miss) {
        return Staged {
            outcome: first,
            first_cost,
            total_cost: first_cost,
        };
    }
    let mut outcome = morpher.parse_word_opts(word, &guess_only);
    let total_cost = first_cost + parse_cost(&outcome);
    outcome.work_steps += first.work_steps;
    outcome.cost = match (first.cost, outcome.cost) {
        (Some(a), Some(b)) => Some(pg_cost::Counts {
            work: a.work + b.work,
            bytes: a.bytes + b.bytes,
            registers: a.registers + b.registers,
            allocations: a.allocations + b.allocations,
        }),
        (_, b) => b,
    };
    Staged {
        outcome,
        first_cost,
        total_cost,
    }
}

/// One parse that tries only the affix rules `admitted` marks; strata and templates always run.
pub fn restricted_parse(
    morpher: &Morpher,
    word: &str,
    admitted: &[bool],
    options: &ParseOptions,
) -> ParseOutcome {
    let filter = |rule: RuleRef| match rule {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => admitted[id.0 as usize],
    };
    morpher.parse_word_selected(word, options, None, Some(&filter))
}

/// The outcome of a word known to have no analysis: segmentation runs, no stratum does.
fn no_analysis(morpher: &Morpher, word: &str, options: &ParseOptions) -> ParseOutcome {
    let none = |rule: RuleRef| !matches!(rule, RuleRef::Stratum(_));
    morpher.parse_word_selected(word, options, None, Some(&none))
}

/// A parse's cost in parser work: the measured cost (work, bytes and allocator calls) when the pg-cost meter ran.
///
/// A library caller with the meter off gets the work steps at the work weight, which leaves out bytes and
/// allocator calls and so understates the cost; `pangloss batch --prefilter auto` turns the meter on, so its gate
/// always prices the full parser work.
pub fn parse_cost(outcome: &ParseOutcome) -> u64 {
    outcome.cost.map_or(
        outcome.work_steps as u64 * pg_cost::WORK_WEIGHT / pg_cost::WEIGHT_SCALE,
        |c| c.cost(),
    )
}
