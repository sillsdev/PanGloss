//! `pangloss batch --prefilter auto|on|off`: the load-time gate of `pg_foma::admission::gate` behind the batch command.

#[cfg(test)]
mod tests;

use super::BatchParseCounter;
use pg_grammar::model::Grammar;
use pg_parse::{BatchWordOutcome, Morpher, ParseOptions, ParseOutcome};

/// What `--prefilter` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrefilterMode {
    Off,
    /// The calibrated gate decides whether each stage runs.
    Auto,
    /// Every stage that is valid for the grammar runs on every word: a verification and measurement mode.
    On,
}

impl PrefilterMode {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "off" => Ok(Self::Off),
            "auto" => Ok(Self::Auto),
            "on" => Ok(Self::On),
            other => Err(format!(
                "invalid --prefilter: {other} (expected auto, on or off)"
            )),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Auto => "auto",
            Self::On => "on",
        }
    }
}

/// The batch options whose output reads the full plain search; `--work-columns` and `--cost-column` are not among them, they give the work the selected pipeline spent.
pub(crate) struct SearchDependent {
    pub stats: bool,
    pub word_timeout: bool,
    pub per_word_diagnostics: bool,
}

/// Stops `--prefilter auto` and `--prefilter on` before any output is written when it cannot run, or cannot give the output that was asked for.
pub(crate) fn check(mode: PrefilterMode, options: &SearchDependent) -> Result<(), String> {
    if mode == PrefilterMode::Off {
        return Ok(());
    }
    if !cfg!(feature = "foma-tools") {
        return Err(format!("--prefilter {} needs a build with the `foma-tools` feature; this build has none (use --prefilter off)", mode.name()));
    }
    let refused = [
        (options.stats, "--stats", "it records the rules the full search attempts, and a restricted search attempts fewer"),
        (options.word_timeout, "--word-timeout-ms", "a wall-clock deadline makes the plain reference the gate compares against depend on the machine's load"),
        (options.per_word_diagnostics, "the HC_STEP_STATS, HC_FST_PROFILE, HC_FRONTIER_STATS, HC_WORD_STATS, HC_ALT_YIELD and HC_BRANCH_CENSUS diagnostics", "they profile the full search of each word, and a restricted search is a different search"),
    ];
    for (given, name, why) in refused {
        if given {
            return Err(format!(
                "--prefilter {} cannot be combined with {name}: {why}",
                mode.name()
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "foma-tools")]
pub(crate) use with_foma::BatchPrefilter;
#[cfg(not(feature = "foma-tools"))]
pub(crate) use without_foma::BatchPrefilter;

#[cfg(feature = "foma-tools")]
mod with_foma {
    use super::*;
    use pg_foma::admission::gate::{
        decide, decide_forced, total_line, Decided, GateReport, Prefilter, Tally,
    };
    use pg_parse::hc_parse_batch;
    use rayon::prelude::*;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[cfg(test)]
    thread_local! {
        /// The decision line the last `prepare` on this thread printed.
        pub(super) static LAST_REPORT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    }

    /// The gate's decision for one batch, the plain outcomes of the sampled words it parsed, and what the run has cost.
    pub(crate) struct BatchPrefilter {
        prefilter: Prefilter,
        /// Outcomes by index into `words[start..]`, taken out as they are written.
        sampled: HashMap<usize, ParseOutcome>,
        start: usize,
        report: GateReport,
        tally: Tally,
    }

    /// The unsampled words of a batch the gate left plain, dispatched exactly as `--prefilter off` dispatches them.
    fn plain_rest(
        morpher: &Morpher,
        (remaining, todo): (&[String], &[usize]),
        threads: usize,
        options: &ParseOptions,
        parse_counter: &BatchParseCounter,
    ) -> Vec<(BatchWordOutcome, Tally)> {
        let words: Vec<String> = todo.iter().map(|&at| remaining[at].clone()).collect();
        let done = if options.guess_root && !morpher.lexical_patterns().is_empty() {
            crate::parse_batch_with_opts(morpher, &words, threads, options, parse_counter)
        } else {
            hc_parse_batch(morpher, &words, threads)
        };
        done.into_iter()
            .map(|done| {
                let mut tally = Tally::default();
                tally.add_plain(&done.outcome);
                (done, tally)
            })
            .collect()
    }

    impl BatchPrefilter {
        /// Decides on an even sample of `words[start..]`, prints the report line on stderr and keeps the outcomes it parsed.
        pub(crate) fn prepare(
            mode: PrefilterMode,
            grammar: &Grammar,
            morpher: &Morpher,
            options: &ParseOptions,
            words: &[String],
            start: usize,
        ) -> Result<Self, String> {
            let prefilter = if mode == PrefilterMode::On {
                Ok(Self::from_decided(
                    decide_forced(grammar, morpher, options, words.len() - start),
                    start,
                ))
            } else {
                Self::decide(grammar, morpher, options, words, start)
            }?;
            #[cfg(test)]
            LAST_REPORT.with(|last| *last.borrow_mut() = Some(prefilter.report.to_string()));
            eprintln!("PREFILTER\t{}", prefilter.report);
            Ok(prefilter)
        }

        pub(crate) fn decide(
            grammar: &Grammar,
            morpher: &Morpher,
            options: &ParseOptions,
            words: &[String],
            start: usize,
        ) -> Result<Self, String> {
            let decided = decide(grammar, morpher, options, &words[start..])
                .map_err(|error| error.to_string())?;
            Ok(Self::from_decided(decided, start))
        }

        fn from_decided(decided: Decided, start: usize) -> Self {
            let mut tally = Tally::default();
            decided
                .outcomes
                .iter()
                .for_each(|(_, o)| tally.add_plain(o));
            Self {
                prefilter: decided.prefilter,
                sampled: decided.outcomes.into_iter().collect(),
                start,
                report: decided.report,
                tally,
            }
        }

        #[cfg(test)]
        pub(crate) fn report(&self) -> &GateReport {
            &self.report
        }

        /// The `PREFILTER-TOTAL` line for the words parsed so far.
        pub(crate) fn total_line(&self) -> String {
            total_line(&self.report, &self.tally)
        }

        /// The outcome of the word at `index` of the whole word list, one call per word.
        pub(crate) fn parse(
            &mut self,
            index: usize,
            morpher: &Morpher,
            word: &str,
            options: &ParseOptions,
        ) -> ParseOutcome {
            let sampled = index
                .checked_sub(self.start)
                .and_then(|at| self.sampled.remove(&at));
            sampled.unwrap_or_else(|| {
                self.prefilter
                    .parse_counted(morpher, word, options, &mut self.tally)
            })
        }

        /// The unsampled words (`todo`, indices into `remaining`) through the decided stages on a pool of `threads`, in `todo` order.
        fn staged_rest(
            &self,
            morpher: &Morpher,
            (remaining, todo): (&[String], &[usize]),
            threads: usize,
            options: &ParseOptions,
            parse_counter: &BatchParseCounter,
        ) -> Vec<(BatchWordOutcome, Tally)> {
            let mut builder = rayon::ThreadPoolBuilder::new().stack_size(1 << 30);
            if threads > 0 {
                builder = builder.num_threads(threads);
            }
            let pool = builder
                .build()
                .expect("build rayon pool for --prefilter batch dispatch");
            let prefilter = &self.prefilter;
            pool.install(|| {
                todo.par_iter()
                    .map(|&at| {
                        let start = Instant::now();
                        parse_counter.record();
                        let mut tally = Tally::default();
                        let outcome =
                            prefilter.parse_counted(morpher, &remaining[at], options, &mut tally);
                        let done = BatchWordOutcome {
                            outcome,
                            elapsed: start.elapsed(),
                        };
                        (done, tally)
                    })
                    .collect()
            })
        }

        /// Every word of `remaining` (`words[start..]`) on a pool of `threads`, in order; the sampled words keep the outcomes the gate parsed.
        pub(crate) fn parse_batch(
            &mut self,
            morpher: &Morpher,
            remaining: &[String],
            threads: usize,
            options: &ParseOptions,
            parse_counter: &BatchParseCounter,
        ) -> Vec<BatchWordOutcome> {
            let mut sampled = std::mem::take(&mut self.sampled);
            let todo: Vec<usize> = (0..remaining.len())
                .filter(|at| !sampled.contains_key(at))
                .collect();
            let rest = if self.prefilter.admission_on() {
                self.staged_rest(morpher, (remaining, &todo), threads, options, parse_counter)
            } else {
                plain_rest(morpher, (remaining, &todo), threads, options, parse_counter)
            };
            let mut rest = rest.into_iter();
            (0..remaining.len())
                .map(|at| match sampled.remove(&at) {
                    Some(outcome) => {
                        parse_counter.record();
                        BatchWordOutcome {
                            outcome,
                            elapsed: Duration::ZERO,
                        }
                    }
                    None => {
                        let (done, tally) = rest.next().expect("one outcome per unsampled word");
                        self.tally.add(&tally);
                        done
                    }
                })
                .collect()
        }
    }
}

#[cfg(not(feature = "foma-tools"))]
mod without_foma {
    use super::*;

    /// No value exists in a build without `foma-tools`.
    pub(crate) enum BatchPrefilter {}

    impl BatchPrefilter {
        pub(crate) fn prepare(
            mode: PrefilterMode,
            _grammar: &Grammar,
            _morpher: &Morpher,
            _options: &ParseOptions,
            _words: &[String],
            _start: usize,
        ) -> Result<Self, String> {
            Err(format!(
                "--prefilter {} needs a build with the `foma-tools` feature",
                mode.name()
            ))
        }

        pub(crate) fn total_line(&self) -> String {
            match *self {}
        }

        pub(crate) fn parse(
            &mut self,
            _index: usize,
            _morpher: &Morpher,
            _word: &str,
            _options: &ParseOptions,
        ) -> ParseOutcome {
            match *self {}
        }

        pub(crate) fn parse_batch(
            &mut self,
            _morpher: &Morpher,
            _remaining: &[String],
            _threads: usize,
            _options: &ParseOptions,
            _parse_counter: &BatchParseCounter,
        ) -> Vec<BatchWordOutcome> {
            match *self {}
        }
    }
}
