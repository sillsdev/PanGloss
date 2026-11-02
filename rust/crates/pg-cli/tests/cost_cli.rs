//! `batch --cost-column`, `cost-compare` and `cost-calibrate` through the real binary.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_pangloss");
const WORDS: &str = "milu\nmilusi\nzzz\n";

fn grammar() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/fixtures/supplied-lexicon-host-smoke.xml")
}

fn scratch(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "pangloss-cost-cli-{tag}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn pangloss(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

/// Runs `batch` over `WORDS` and returns the TSV rows as fields, sorted by word index.
fn batch(tag: &str, extra: &[&str]) -> (PathBuf, Vec<Vec<String>>) {
    let dir = scratch(tag);
    let words = dir.join("words.txt");
    let out = dir.join("out.tsv");
    std::fs::write(&words, WORDS).unwrap();
    let grammar = grammar();
    let mut args = vec![
        "batch",
        grammar.to_str().unwrap(),
        words.to_str().unwrap(),
        out.to_str().unwrap(),
        "--threads",
        "1",
    ];
    args.extend_from_slice(extra);
    let output = pangloss(&args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (out.clone(), read_rows(&out))
}

fn read_rows(path: &Path) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| line.split('\t').map(str::to_string).collect::<Vec<_>>())
        .filter(|fields| fields[2] != "STARTED")
        .collect();
    rows.sort_by_key(|fields| fields[0].parse::<usize>().unwrap());
    rows
}

fn cost_of(row: &[String]) -> u64 {
    row.last()
        .unwrap()
        .parse()
        .expect("the last field is a number")
}

#[test]
fn the_cost_column_is_the_last_numeric_field_and_leaves_the_rest_of_the_row_alone() {
    let (_, plain) = batch("plain", &[]);
    let (_, costed) = batch("costed", &["--cost-column"]);
    assert_eq!(plain.len(), 3);
    for (plain, costed) in plain.iter().zip(&costed) {
        assert_eq!(plain.len(), 5, "{plain:?}");
        assert_eq!(costed.len(), 6, "{costed:?}");
        assert_eq!(costed[..2], plain[..2]);
        assert_eq!(costed[3..5], plain[3..5]);
        assert!(cost_of(costed) > 0, "{costed:?}");
    }
}

#[test]
fn work_columns_stay_right_before_the_cost() {
    let (_, work) = batch("work", &["--work-columns"]);
    let (_, both) = batch("work-cost", &["--work-columns", "--cost-column"]);
    for (work, both) in work.iter().zip(&both) {
        assert_eq!(both.len(), work.len() + 1, "{both:?}");
        assert_eq!(
            both[3..work.len()],
            work[3..],
            "steps and work_steps unchanged"
        );
        assert_eq!(both[both.len() - 3..both.len() - 1], work[work.len() - 2..]);
        assert!(cost_of(both) > 0);
    }
}

#[test]
fn a_word_that_does_more_work_costs_more() {
    let (_, rows) = batch("grow", &["--cost-column", "--work-columns"]);
    let (root, suffixed) = (&rows[0], &rows[1]);
    assert_eq!((root[1].as_str(), suffixed[1].as_str()), ("milu", "milusi"));
    assert!(
        cost_of(suffixed) > cost_of(root),
        "{root:?} vs {suffixed:?}"
    );
}

#[test]
fn the_cost_of_a_word_barely_depends_on_the_thread_count() {
    let (_, one) = batch("cost-one", &["--cost-column"]);
    let (_, two) = batch("cost-two", &["--cost-column", "--threads", "2"]);
    let costs = |rows: &[Vec<String>]| rows.iter().map(|row| cost_of(row)).collect::<Vec<_>>();
    for (one, two) in costs(&one).into_iter().zip(costs(&two)) {
        let gap = one.abs_diff(two) as f64 / one as f64;
        assert!(gap < 0.005, "{one} vs {two}");
    }
}

#[test]
fn cost_column_with_stats_is_refused() {
    let dir = scratch("stats");
    let words = dir.join("words.txt");
    std::fs::write(&words, WORDS).unwrap();
    let grammar = grammar();
    let output = pangloss(&[
        "batch",
        grammar.to_str().unwrap(),
        words.to_str().unwrap(),
        dir.join("out.tsv").to_str().unwrap(),
        "--cost-column",
        "--stats",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--cost-column") && stderr.contains("--stats"),
        "{stderr}"
    );
}

/// A copy of `source` with every row's cost scaled by `num / den`.
fn scaled(source: &Path, tag: &str, num: u64, den: u64) -> PathBuf {
    let path = scratch(tag).join("scaled.tsv");
    let text: String = std::fs::read_to_string(source)
        .unwrap()
        .lines()
        .map(|line| {
            let mut fields: Vec<String> = line.split('\t').map(str::to_string).collect();
            if fields.len() > 5 {
                let cost: u64 = fields.pop().unwrap().parse().unwrap();
                fields.push((cost * num / den).to_string());
            }
            fields.join("\t") + "\n"
        })
        .collect();
    std::fs::write(&path, text).unwrap();
    path
}

fn verdict(before: &Path, after: &Path) -> Value {
    let output = pangloss(&[
        "cost-compare",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn cost_compare_reads_two_cost_runs_into_a_verdict() {
    let (run, _) = batch("compare", &["--cost-column"]);
    let same = verdict(&run, &run);
    assert_eq!(same["verdict"], "no clear change");
    assert_eq!(same["words"], 3);
    assert_eq!(same["parseChanges"], 0);
    assert_eq!(same["atLeast"], false);

    let doubled = scaled(&run, "doubled", 2, 1);
    assert_eq!(verdict(&run, &doubled)["verdict"], "worse");
    let halved = scaled(&run, "halved", 1, 2);
    assert_eq!(verdict(&run, &halved)["verdict"], "better");
}

#[test]
fn cost_compare_refuses_a_run_without_the_cost_column() {
    let (plain, _) = batch("no-cost", &[]);
    let (work, _) = batch("no-cost-work", &["--work-columns"]);
    for run in [plain, work] {
        let output = pangloss(&["cost-compare", run.to_str().unwrap(), run.to_str().unwrap()]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--cost-column"), "{stderr}");
    }
}

#[cfg(not(feature = "cost-calibration"))]
#[test]
fn a_default_build_has_no_cost_calibrate_and_describes_cost_compare() {
    let output = pangloss(&["cost-calibrate", "a", "b"]);
    assert!(
        !output.status.success(),
        "an unknown command prints usage and fails"
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("cost-calibrate"));

    let doc: Value = serde_json::from_slice(&pangloss(&["--describe"]).stdout).unwrap();
    assert_eq!(doc["interfaces"]["batchTsv"], 4);
    let names: Vec<&str> = doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|command| command["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"cost-compare"));
    assert!(!names.contains(&"cost-calibrate"));
}

#[cfg(feature = "cost-calibration")]
mod calibrate {
    use super::*;

    #[test]
    fn a_missing_grammar_is_refused() {
        let words = scratch("calibrate-missing").join("words.txt");
        std::fs::write(&words, WORDS).unwrap();
        let output = pangloss(&[
            "cost-calibrate",
            "no-such-grammar.xml",
            words.to_str().unwrap(),
        ]);
        assert!(!output.status.success());
        assert!(!output.stderr.is_empty());
    }

    #[test]
    fn a_missing_word_list_is_refused() {
        let grammar = grammar();
        let output = pangloss(&[
            "cost-calibrate",
            grammar.to_str().unwrap(),
            "no-such-words.txt",
        ]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("no-such-words.txt"));
    }

    #[test]
    fn too_few_words_are_refused_rather_than_fitted() {
        let words = scratch("calibrate-few").join("words.txt");
        std::fs::write(&words, WORDS).unwrap();
        let grammar = grammar();
        let output = pangloss(&[
            "cost-calibrate",
            grammar.to_str().unwrap(),
            words.to_str().unwrap(),
        ]);
        assert!(!output.status.success());
        assert!(!output.stderr.is_empty());
    }
}
