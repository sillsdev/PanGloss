//! Corpus-gated memo on/off parity and survival check over slices of Aweti, Sena, and Mbugwe -- ignored unconditionally (needs gitignored `samples/data/`), run through `-Mode corpus-test` (`--run-ignored all`).

use std::collections::BTreeSet;
use std::time::Duration;

use pg_conformance_fixtures::corpus;
use pg_grammar::model::Grammar;
use pg_parse::identity::AnalysisIdentity;
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

fn load_fwdata_grammar(logical_name: &str) -> Grammar {
    let path = corpus::grammar_for(logical_name);
    let (snapshot, _) =
        pg_fwdata::import_file(&path).unwrap_or_else(|e| panic!("import {}: {e}", path.display()));
    pg_grammar::compile_project(&snapshot)
        .map(|(g, _)| g)
        .unwrap_or_else(|e| panic!("compile {logical_name}: {e:?}"))
}

fn read_words(logical_name: &str, count: usize) -> Vec<String> {
    let path = corpus::words_for(logical_name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .take(count)
        .map(str::to_owned)
        .collect()
}

fn identity_set(analyses: &[WordAnalysis], grammar: &Grammar) -> BTreeSet<AnalysisIdentity> {
    analyses
        .iter()
        .map(|a| AnalysisIdentity::project(a, grammar).expect("identity projection"))
        .collect()
}

/// The gate's only load-dependent input; raise it via `PANGLOSS_GATE_WORD_TIMEOUT_SECS` so the deterministic step cap is the only cap that fires.
fn word_timeout() -> Duration {
    let secs = std::env::var("PANGLOSS_GATE_WORD_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10);
    Duration::from_secs(secs)
}

/// Per-corpus tally: both-completed words get a hard analysis-set-equality check; a step cap firing on only one side is recorded, never failed (memoization changes step consumption, not recall).
#[derive(Default)]
struct CorpusTally {
    completed_both: usize,
    capped_both: usize,
    completed_only_on: usize,
    completed_only_off: usize,
    /// Counted and printed, not silently dropped: a silent exclusion reads as "no regression".
    timed_out: usize,
}

impl CorpusTally {
    fn add(&mut self, other: &CorpusTally) {
        self.completed_both += other.completed_both;
        self.capped_both += other.capped_both;
        self.completed_only_on += other.completed_only_on;
        self.completed_only_off += other.completed_only_off;
        self.timed_out += other.timed_out;
    }

    /// Timeouts included, so load moves words between printed buckets rather than out of the denominator.
    fn total(&self) -> usize {
        self.completed_both
            + self.capped_both
            + self.completed_only_on
            + self.completed_only_off
            + self.timed_out
    }
}

fn check_corpus(logical_name: &str, word_count: usize, step_cap: usize) -> CorpusTally {
    let grammar = load_fwdata_grammar(logical_name);
    let words = read_words(logical_name, word_count);
    assert!(
        !words.is_empty(),
        "{logical_name}: word list yielded zero words"
    );

    let timeout = word_timeout();
    let memo_on = Morpher::new(&grammar, step_cap).with_word_timeout(Some(timeout));
    let memo_off = Morpher::new(&grammar, step_cap)
        .with_word_timeout(Some(timeout))
        .with_memo(false);
    let opts = ParseOptions::default();

    let mut tally = CorpusTally::default();
    for word in &words {
        let on = memo_on.parse_word_opts(word, &opts);
        let off = memo_off.parse_word_opts(word, &opts);
        // A wall-clock timeout is not a reproducible outcome, so it is excluded rather than compared.
        if on.timed_out || off.timed_out {
            tally.timed_out += 1;
            continue;
        }
        // A cap firing on only one side reflects memoization's step-count effect, not a recall difference, and a capped run's partial set is never compared (not even as a subset) against a completed one -- recorded below, never failed.
        match (on.capped, off.capped) {
            (false, false) => {
                tally.completed_both += 1;
                let on_set = identity_set(&on.structured, &grammar);
                let off_set = identity_set(&off.structured, &grammar);
                assert_eq!(
                    on_set, off_set,
                    "{logical_name}: word {word:?}: memo on/off analysis-identity sets differ"
                );
            }
            (true, true) => tally.capped_both += 1,
            (false, true) => tally.completed_only_on += 1,
            (true, false) => tally.completed_only_off += 1,
        }
    }
    eprintln!(
        "{logical_name}: completed both={} capped both={} completed only on={} completed only off={} timed out={}",
        tally.completed_both,
        tally.capped_both,
        tally.completed_only_on,
        tally.completed_only_off,
        tally.timed_out
    );
    tally
}

#[test]
#[ignore = "needs local gitignored corpora under samples/data/; run through `pg.ps1 -Mode corpus-test`"]
fn memo_parity_survives_aweti_sena_mbugwe() {
    let mut total = CorpusTally::default();
    // 50_000_000: pg-cli's own DEFAULT_STEP_CAP (batch's unspecified --step-cap).
    for (name, count, cap) in [
        ("aweti", 44, 200_000),
        ("sena", 300, 50_000_000),
        ("mbugwe", 60, 2_000_000),
    ] {
        total.add(&check_corpus(name, count, cap));
    }
    eprintln!(
        "{} words completed only with memo on, {} only with memo off, {} excluded by wall-clock timeout",
        total.completed_only_on, total.completed_only_off, total.timed_out
    );
    corpus::record_cases("memo_parity_survives_aweti_sena_mbugwe", total.total());
}
