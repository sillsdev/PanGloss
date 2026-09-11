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

// Generous enough for a slow corpus word (corpus-manifest.json documents several) without hanging forever.
const WORD_TIMEOUT: Duration = Duration::from_secs(10);

/// One corpus slice, memo on vs off: every non-timed-out word must match on analysis set and capped.
fn check_corpus(logical_name: &str, word_count: usize, step_cap: usize) -> usize {
    let grammar = load_fwdata_grammar(logical_name);
    let words = read_words(logical_name, word_count);
    assert!(
        !words.is_empty(),
        "{logical_name}: word list yielded zero words"
    );

    let memo_on = Morpher::new(&grammar, step_cap).with_word_timeout(Some(WORD_TIMEOUT));
    let memo_off = Morpher::new(&grammar, step_cap)
        .with_word_timeout(Some(WORD_TIMEOUT))
        .with_memo(false);
    let opts = ParseOptions::default();

    let mut checked = 0usize;
    for word in &words {
        let on = memo_on.parse_word_opts(word, &opts);
        let off = memo_off.parse_word_opts(word, &opts);
        // A wall-clock timeout is not a reproducible outcome, so it is excluded rather than compared.
        if on.timed_out || off.timed_out {
            continue;
        }
        assert_eq!(
            on.capped, off.capped,
            "{logical_name}: word {word:?}: memo=on capped={} but memo=off capped={}",
            on.capped, off.capped
        );
        // A capped word's surviving partial set legitimately differs by step order on each side (docs/research/analysis-memo-explosion.md); only completed words get the full set comparison.
        if on.capped {
            continue;
        }
        checked += 1;
        let on_set = identity_set(&on.structured, &grammar);
        let off_set = identity_set(&off.structured, &grammar);
        assert_eq!(
            on_set, off_set,
            "{logical_name}: word {word:?}: memo on/off analysis-identity sets differ"
        );
    }
    checked
}

#[test]
#[ignore = "needs local gitignored corpora under samples/data/; run through `pg.ps1 -Mode corpus-test`"]
fn memo_parity_survives_aweti_sena_mbugwe() {
    let mut total = 0usize;
    total += check_corpus("aweti", 44, 200_000);
    // 50_000_000: pg-cli's own DEFAULT_STEP_CAP (batch's unspecified --step-cap).
    total += check_corpus("sena", 300, 50_000_000);
    total += check_corpus("mbugwe", 60, 2_000_000);
    corpus::record_cases("memo_parity_survives_aweti_sena_mbugwe", total);
}
