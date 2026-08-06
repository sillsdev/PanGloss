//! Per-word timing over the synthetic-language conformance suite, grouped per construct/typology;
//! see `docs/research/pg-foma-typology-speedup.md` for the design rationale.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pg_conformance_fixtures::{discover, FixtureRef};
use pg_foma::capability::{CapabilityDiagnostic, CompileDecision};
use pg_foma::capability_entry::evaluate_capability;
use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;
use pg_parse::Morpher;

/// Timed samples per word per engine, after one discarded warmup call (env override `PG_TYPOLOGY_REPEATS`, minimum 1, default 7).
fn repeats() -> u32 {
    std::env::var("PG_TYPOLOGY_REPEATS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(7)
}

/// Calibrates this process's real tick granularity: spins until two consecutive reads differ, several times, keeping the minimum observed delta. Never assumes a platform constant; the result is at least 1.
fn measure_timer_floor_ns() -> u64 {
    let mut floor = u64::MAX;
    let mut prev = Instant::now();
    for _ in 0..16 {
        loop {
            let now = Instant::now();
            if now > prev {
                floor = floor.min((now - prev).as_nanos() as u64);
                prev = now;
                break;
            }
        }
    }
    floor.max(1)
}

/// One word's timing distribution: the calling convention every timed row in this harness reports.
#[derive(Debug, Clone, Copy)]
struct Timing {
    median_ns: u64,
    min_ns: u64,
    max_ns: u64,
    samples: u32,
}

/// Runs `f` once (discarded warmup), then `n` timed times, returning the median/min/max in nanoseconds.
fn time_repeated<F: FnMut()>(mut f: F, n: u32) -> Timing {
    f();
    let mut samples: Vec<u64> = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let start = Instant::now();
        f();
        samples.push(start.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    let median_ns = samples[samples.len() / 2];
    Timing {
        median_ns,
        min_ns: samples[0],
        max_ns: samples[samples.len() - 1],
        samples: n,
    }
}

/// One CSV row: a per-word timed row (`word` non-empty), or a fixture-level refused/compile-error row (`word` empty, since that outcome belongs to the whole grammar).
#[derive(Debug, Clone)]
struct Row {
    root: &'static str,
    category: String,
    fixture: String,
    word: String,
    engine: &'static str,
    outcome: &'static str,
    /// `None` whenever the raw value is below the calibrated floor -- never `Some(0)`.
    median_ns: Option<u64>,
    min_ns: Option<u64>,
    max_ns: Option<u64>,
    samples: Option<u32>,
    below_floor: bool,
    floor_ns: u64,
    nonempty: Option<bool>,
    predicate: Option<String>,
    construct: Option<String>,
    witness: Option<String>,
    note: Option<String>,
}

impl Row {
    #[allow(clippy::too_many_arguments)]
    fn timed(
        root: &'static str,
        category: &str,
        fixture: &str,
        word: &str,
        engine: &'static str,
        t: Timing,
        floor_ns: u64,
        nonempty: bool,
    ) -> Row {
        let field = |v: u64| -> Option<u64> {
            if v < floor_ns {
                None
            } else {
                Some(v)
            }
        };
        let median_ns = field(t.median_ns);
        Row {
            root,
            category: category.to_string(),
            fixture: fixture.to_string(),
            word: word.to_string(),
            engine,
            outcome: "ok",
            below_floor: median_ns.is_none(),
            median_ns,
            min_ns: field(t.min_ns),
            max_ns: field(t.max_ns),
            samples: Some(t.samples),
            floor_ns,
            nonempty: Some(nonempty),
            predicate: None,
            construct: None,
            witness: None,
            note: None,
        }
    }

    fn refused(
        root: &'static str,
        category: &str,
        fixture: &str,
        predicate: &str,
        construct: &str,
        witness: &str,
        floor_ns: u64,
    ) -> Row {
        Row {
            root,
            category: category.to_string(),
            fixture: fixture.to_string(),
            word: String::new(),
            engine: "compiled",
            outcome: "refused",
            median_ns: None,
            min_ns: None,
            max_ns: None,
            samples: None,
            below_floor: false,
            floor_ns,
            nonempty: None,
            predicate: Some(predicate.to_string()),
            construct: Some(construct.to_string()),
            witness: Some(witness.to_string()),
            note: None,
        }
    }

    fn compile_error(
        root: &'static str,
        category: &str,
        fixture: &str,
        message: &str,
        floor_ns: u64,
    ) -> Row {
        Row {
            root,
            category: category.to_string(),
            fixture: fixture.to_string(),
            word: String::new(),
            engine: "compiled",
            outcome: "compile_error",
            median_ns: None,
            min_ns: None,
            max_ns: None,
            samples: None,
            below_floor: false,
            floor_ns,
            nonempty: None,
            predicate: None,
            construct: None,
            witness: None,
            note: Some(message.to_string()),
        }
    }

    /// A genuine Rust panic inside one engine's construction/timing for this fixture, distinct from `compile_error`'s clean `Result::Err` and from `refused`'s capability-gate verdict.
    fn engine_panic(
        root: &'static str,
        category: &str,
        fixture: &str,
        engine: &'static str,
        message: &str,
        floor_ns: u64,
    ) -> Row {
        Row {
            root,
            category: category.to_string(),
            fixture: fixture.to_string(),
            word: String::new(),
            engine,
            outcome: "engine_panic",
            median_ns: None,
            min_ns: None,
            max_ns: None,
            samples: None,
            below_floor: false,
            floor_ns,
            nonempty: None,
            predicate: None,
            construct: None,
            witness: None,
            note: Some(message.to_string()),
        }
    }
}

/// Renders a caught `catch_unwind` payload as a display string; a payload other than `&'static str`/`String` prints a fixed fallback rather than failing.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

const CSV_HEADER: &str = "root,category,fixture,word,engine,outcome,median_ns,min_ns,max_ns,samples,below_floor,floor_ns,nonempty,predicate,construct,witness,note";

/// Minimal RFC4180-shaped escaping: quote a field iff it contains a comma, quote, or newline, doubling embedded quotes.
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn opt_u64(v: Option<u64>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn opt_u32(v: Option<u32>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn opt_bool(v: Option<bool>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn opt_str(v: &Option<String>) -> String {
    v.as_deref().map(csv_field).unwrap_or_default()
}

/// Renders `rows` as CSV text (header + one line per row), the canonical source artifact.
fn render_csv(rows: &[Row]) -> String {
    let mut out = String::new();
    writeln!(out, "{CSV_HEADER}").unwrap();
    for r in rows {
        writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            r.root,
            csv_field(&r.category),
            csv_field(&r.fixture),
            csv_field(&r.word),
            r.engine,
            r.outcome,
            opt_u64(r.median_ns),
            opt_u64(r.min_ns),
            opt_u64(r.max_ns),
            opt_u32(r.samples),
            r.below_floor,
            r.floor_ns,
            opt_bool(r.nonempty),
            opt_str(&r.predicate),
            opt_str(&r.construct),
            opt_str(&r.witness),
            opt_str(&r.note),
        )
        .unwrap();
    }
    out
}

/// Times every word in `words` against `morpher`, the complete engine -- never capability-gated, since `pg_parse::Morpher` never relies on the FST proposer.
fn time_complete_engine(
    morpher: &Morpher,
    words: &[String],
    root: &'static str,
    category: &str,
    fixture: &str,
    floor_ns: u64,
    n: u32,
) -> Vec<Row> {
    words
        .iter()
        .map(|w| {
            let mut nonempty = false;
            let t = time_repeated(
                || {
                    let outcome = morpher.parse_word(w);
                    nonempty = !outcome.structured.is_empty();
                },
                n,
            );
            Row::timed(
                root, category, fixture, w, "complete", t, floor_ns, nonempty,
            )
        })
        .collect()
}

/// Evaluates `evaluate_capability` before ever attempting `FomaAnalyzer::new`; `Refuse` becomes one row per diagnostic, naming the refusing predicate/construct/witness, never force-compiled.
fn refusal_rows(
    diags: &[CapabilityDiagnostic],
    root: &'static str,
    category: &str,
    fixture: &str,
    floor_ns: u64,
) -> Vec<Row> {
    diags
        .iter()
        .map(|d| {
            Row::refused(
                root,
                category,
                fixture,
                d.predicate,
                &d.construct,
                &d.witness,
                floor_ns,
            )
        })
        .collect()
}
fn time_compiled_engine(
    g: &Grammar,
    words: &[String],
    root: &'static str,
    category: &str,
    fixture: &str,
    floor_ns: u64,
    n: u32,
) -> Vec<Row> {
    match evaluate_capability(g) {
        CompileDecision::Refuse(diags) => refusal_rows(&diags, root, category, fixture, floor_ns),
        CompileDecision::Admit | CompileDecision::ConfirmOnly => match FomaAnalyzer::new(g) {
            Err(e) => vec![Row::compile_error(
                root,
                category,
                fixture,
                &e.to_string(),
                floor_ns,
            )],
            Ok(mut analyzer) => words
                .iter()
                .map(|w| {
                    let mut nonempty = false;
                    let t = time_repeated(
                        || {
                            let outcome = analyzer.analyze_word(w);
                            nonempty = outcome.confirmed > 0;
                        },
                        n,
                    );
                    Row::timed(
                        root, category, fixture, w, "compiled", t, floor_ns, nonempty,
                    )
                })
                .collect(),
        },
    }
}

/// One fixture, both engines, each independently panic-guarded; see
/// `docs/research/pg-foma-typology-speedup.md` for the real bug that guard caught.
fn process_fixture(f: &FixtureRef, floor_ns: u64, n: u32) -> Vec<Row> {
    let words_yaml = f.load_words_yaml();
    let xml = f.load_grammar_xml();
    let g = pg_grammar::load(&xml)
        .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", f.label()));
    let words: Vec<String> = words_yaml.words.iter().map(|w| w.word.clone()).collect();
    let root = f.root.label();
    let category = f.category.as_str();
    let fixture = f.name.as_str();

    let mut rows = Vec::new();

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
        time_complete_engine(&morpher, &words, root, category, fixture, floor_ns, n)
    })) {
        Ok(r) => rows.extend(r),
        Err(payload) => rows.push(Row::engine_panic(
            root,
            category,
            fixture,
            "complete",
            &panic_message(&*payload),
            floor_ns,
        )),
    }

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        time_compiled_engine(&g, &words, root, category, fixture, floor_ns, n)
    })) {
        Ok(r) => rows.extend(r),
        Err(payload) => rows.push(Row::engine_panic(
            root,
            category,
            fixture,
            "compiled",
            &panic_message(&*payload),
            floor_ns,
        )),
    }

    rows
}

struct FixtureSummary {
    root: &'static str,
    category: String,
    fixture: String,
    word_count: usize,
    floor_ns: u64,
    complete: EngineSummary,
    compiled: EngineSummary,
    noisy: Option<&'static str>,
}

/// One engine's outcome for one fixture, shared by both `complete` and `compiled` since either can panic even though only `compiled` can be refused.
enum EngineSummary {
    Timed {
        agg_ns: Option<u64>,
        below_floor: bool,
    },
    Refused(Vec<String>),
    CompileError(String),
    EnginePanic(String),
    /// No rows at all for this engine on this fixture; should not happen, but degrades to "not assessed" rather than panicking.
    Unassessed,
}

/// Deliberately simple, judgment-call thresholds: flags a fixture noisy if either engine's aggregate sits within 20x of the timer floor, or any word's repeat spread exceeds 3x.
fn fixture_is_noisy(rows: &[Row], floor_ns: u64, agg_ns: &[Option<u64>]) -> Option<&'static str> {
    const NOISE_FLOOR_MULTIPLE: u64 = 20;
    const REPEAT_SPREAD_RATIO: u64 = 3;

    for a in agg_ns.iter().flatten() {
        if *a < floor_ns.saturating_mul(NOISE_FLOOR_MULTIPLE) {
            return Some("near timer-resolution floor");
        }
    }
    for r in rows {
        if let (Some(min), Some(max)) = (r.min_ns, r.max_ns) {
            if min > 0 && max > min * REPEAT_SPREAD_RATIO {
                return Some("high repeat-to-repeat spread");
            }
        }
    }
    None
}

fn median_of(mut values: Vec<u64>) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[values.len() / 2])
}

/// Summarizes one engine's rows for one fixture into an `EngineSummary`, shared logic for both `complete` and `compiled`.
fn summarize_engine(engine_rows: &[&Row]) -> EngineSummary {
    let refused: Vec<&&Row> = engine_rows
        .iter()
        .filter(|r| r.outcome == "refused")
        .collect();
    if !refused.is_empty() {
        let mut predicates: Vec<String> = refused
            .iter()
            .map(|r| {
                format!(
                    "{} ({})",
                    r.predicate.as_deref().unwrap_or("?"),
                    r.construct.as_deref().unwrap_or("?")
                )
            })
            .collect();
        predicates.sort();
        predicates.dedup();
        return EngineSummary::Refused(predicates);
    }
    if let Some(e) = engine_rows.iter().find(|r| r.outcome == "compile_error") {
        return EngineSummary::CompileError(e.note.clone().unwrap_or_default());
    }
    if let Some(e) = engine_rows.iter().find(|r| r.outcome == "engine_panic") {
        return EngineSummary::EnginePanic(e.note.clone().unwrap_or_default());
    }
    let timed: Vec<&&Row> = engine_rows.iter().filter(|r| r.outcome == "ok").collect();
    if !timed.is_empty() {
        let below_floor = timed.iter().any(|r| r.median_ns.is_none());
        let agg_ns = if below_floor {
            None
        } else {
            median_of(timed.iter().filter_map(|r| r.median_ns).collect())
        };
        return EngineSummary::Timed {
            agg_ns,
            below_floor,
        };
    }
    EngineSummary::Unassessed
}

fn summarize_fixture(
    root: &'static str,
    category: &str,
    fixture: &str,
    rows: &[Row],
) -> FixtureSummary {
    let mine: Vec<&Row> = rows
        .iter()
        .filter(|r| r.root == root && r.category == category && r.fixture == fixture)
        .collect();

    // Word count is distinct non-empty `word` values seen for this fixture, meaningful even when one engine panicked/refused and contributed zero per-word rows.
    let mut words: Vec<&str> = mine
        .iter()
        .map(|r| r.word.as_str())
        .filter(|w| !w.is_empty())
        .collect();
    words.sort_unstable();
    words.dedup();
    let word_count = words.len();

    let complete_rows: Vec<&Row> = mine
        .iter()
        .copied()
        .filter(|r| r.engine == "complete")
        .collect();
    let compiled_rows: Vec<&Row> = mine
        .iter()
        .copied()
        .filter(|r| r.engine == "compiled")
        .collect();
    let complete = summarize_engine(&complete_rows);
    let compiled = summarize_engine(&compiled_rows);

    let mut agg_for_noise = Vec::new();
    if let EngineSummary::Timed { agg_ns, .. } = &complete {
        agg_for_noise.push(*agg_ns);
    }
    if let EngineSummary::Timed { agg_ns, .. } = &compiled {
        agg_for_noise.push(*agg_ns);
    }
    let floor_ns = mine.first().map(|r| r.floor_ns).unwrap_or(1);
    let owned_rows: Vec<Row> = mine.iter().map(|r| (*r).clone()).collect();
    let noisy = fixture_is_noisy(&owned_rows, floor_ns, &agg_for_noise);

    FixtureSummary {
        root,
        category: category.to_string(),
        fixture: fixture.to_string(),
        word_count,
        floor_ns,
        complete,
        compiled,
        noisy,
    }
}

/// One `(root, category, fixture)` key per group, first-seen order.
fn fixture_keys(rows: &[Row]) -> Vec<(&'static str, String, String)> {
    let mut keys = Vec::new();
    for r in rows {
        let key = (r.root, r.category.clone(), r.fixture.clone());
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

fn format_ns_cell(agg_ns: Option<u64>, below_floor: bool, floor_ns: u64) -> String {
    match agg_ns {
        Some(ns) => format!("{:.3} ms", ns as f64 / 1_000_000.0),
        None if below_floor => format!("<{floor_ns}ns"),
        None => "n/a".to_string(),
    }
}

/// Renders one `EngineSummary` as its table cell text, never a bare `0`/`0ms` (see `format_ns_cell`).
fn render_engine_cell(e: &EngineSummary, floor_ns: u64) -> String {
    match e {
        EngineSummary::Refused(preds) => format!("REFUSED: {}", preds.join("; ")),
        EngineSummary::CompileError(msg) => format!("COMPILE ERROR: {msg}"),
        EngineSummary::EnginePanic(msg) => format!("ENGINE PANIC: {msg}"),
        EngineSummary::Unassessed => "not assessed".to_string(),
        EngineSummary::Timed {
            agg_ns,
            below_floor,
        } => format_ns_cell(*agg_ns, *below_floor, floor_ns),
    }
}

fn render_fixture_table_row(s: &FixtureSummary) -> String {
    let complete_cell = render_engine_cell(&s.complete, s.floor_ns);
    let compiled_cell = render_engine_cell(&s.compiled, s.floor_ns);
    let speedup_cell = match (&s.complete, &s.compiled) {
        (
            EngineSummary::Timed {
                agg_ns: Some(c), ..
            },
            EngineSummary::Timed {
                agg_ns: Some(f), ..
            },
        ) if *f > 0 => format!("{:.2}x", *c as f64 / *f as f64),
        (EngineSummary::Refused(_), _) => "n/a (refused)".to_string(),
        (_, EngineSummary::Refused(_)) => "n/a (refused)".to_string(),
        (EngineSummary::CompileError(_), _) | (_, EngineSummary::CompileError(_)) => {
            "n/a (compile error)".to_string()
        }
        (EngineSummary::EnginePanic(_), _) | (_, EngineSummary::EnginePanic(_)) => {
            "n/a (engine panic)".to_string()
        }
        _ => "n/a (below floor)".to_string(),
    };
    let noise_note = s
        .noisy
        .map(|n| format!(" _(noisy: {n})_"))
        .unwrap_or_default();
    format!(
        "| {}:{} | {} | {} | {} | {} | {}{} |",
        s.root,
        s.category,
        s.fixture,
        s.word_count,
        complete_cell,
        compiled_cell,
        speedup_cell,
        noise_note
    )
}

/// Renders the per-fixture Markdown table, a view over `rows` computing nothing `render_csv` didn't already write.
fn render_markdown(rows: &[Row], floor_ns: u64, repeats: u32) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "# Typology speedup: complete engine vs compiled proposer+confirm"
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "Per-fixture median-of-medians over {repeats} timed samples per word (1 discarded warmup \
         call each). Timer floor calibrated this run at **{floor_ns}ns**; any aggregate at or below \
         it is shown as `<{floor_ns}ns`, never `0`. Grouped per fixture, which is per \
         construct/typology (fixtures are named by construct under `edge-cases/`, by typology under \
         `languages/`)."
    )
    .unwrap();
    writeln!(out).unwrap();

    for (label, category_filter) in [
        ("Languages (typology)", "languages"),
        ("Edge cases (construct)", "edge-cases"),
    ] {
        let keys: Vec<_> = fixture_keys(rows)
            .into_iter()
            .filter(|(_, cat, _)| cat == category_filter)
            .collect();
        if keys.is_empty() {
            continue;
        }
        writeln!(out, "## {label}").unwrap();
        writeln!(out).unwrap();
        // Six columns matching `render_fixture_table_row`'s cells exactly; `markdown_header_matches_row_cell_count` pins that a mismatch here silently misaligns the table (see docs/research/pg-foma-typology-speedup.md).
        writeln!(
            out,
            "| source | fixture | words | complete engine | compiled (foma) | speedup |"
        )
        .unwrap();
        writeln!(out, "|---|---|---|---|---|---|").unwrap();
        for (root, category, fixture) in keys {
            let s = summarize_fixture(root, &category, &fixture, rows);
            writeln!(out, "{}", render_fixture_table_row(&s)).unwrap();
        }
        writeln!(out).unwrap();
    }

    out
}

fn out_dir() -> PathBuf {
    std::env::var("PG_TYPOLOGY_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/typology-speedup")
        })
}

fn write_artifacts(rows: &[Row], floor_ns: u64, n: u32, dir: &Path) -> (PathBuf, PathBuf) {
    fs::create_dir_all(dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    let csv_path = dir.join("typology-speedup.csv");
    let md_path = dir.join("typology-speedup.md");
    fs::write(&csv_path, render_csv(rows))
        .unwrap_or_else(|e| panic!("write {}: {e}", csv_path.display()));
    fs::write(&md_path, render_markdown(rows, floor_ns, n))
        .unwrap_or_else(|e| panic!("write {}: {e}", md_path.display()));
    (csv_path, md_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_floor_calibrates_to_a_positive_finite_value() {
        let floor = measure_timer_floor_ns();
        assert!(floor >= 1, "floor must be at least 1ns, got {floor}");
        assert!(
            floor < 1_000_000_000,
            "floor of {floor}ns is implausibly coarse (>= 1s) -- calibration likely broken"
        );
    }

    #[test]
    fn time_repeated_reports_a_full_distribution() {
        let mut calls = 0u32;
        let t = time_repeated(
            || {
                calls += 1;
            },
            5,
        );
        assert_eq!(t.samples, 5);
        assert!(t.min_ns <= t.median_ns && t.median_ns <= t.max_ns);
        // 1 warmup + 5 timed = 6 total invocations of the closure.
        assert_eq!(calls, 6);
    }

    /// Gate: below-floor is never rendered as a literal `0`, in either artifact. Constructs a synthetic below-floor row directly, a deterministic proof rather than hoping a real word measures as exactly zero.
    #[test]
    fn below_floor_never_renders_as_zero_in_csv_or_markdown() {
        let floor_ns = 1_000; // pretend the calibrated floor is 1us for this test.
        let t = Timing {
            median_ns: 0, // the pathological "measured exactly zero" case.
            min_ns: 0,
            max_ns: 0,
            samples: 7,
        };
        let row = Row::timed(
            "staging",
            "edge-cases",
            "fast-fixture",
            "w",
            "complete",
            t,
            floor_ns,
            true,
        );
        assert!(
            row.below_floor,
            "a zero-duration measurement must be flagged below_floor"
        );
        assert_eq!(
            row.median_ns, None,
            "a below-floor value must be None, never Some(0)"
        );

        let csv = render_csv(std::slice::from_ref(&row));
        for line in csv.lines().skip(1) {
            let cells: Vec<&str> = line.split(',').collect();
            // median_ns/min_ns/max_ns are columns 6,7,8 (0-indexed) -- must be blank, never "0".
            for idx in [6, 7, 8] {
                assert_ne!(
                    cells[idx], "0",
                    "CSV column {idx} must never be the literal '0': {line}"
                );
            }
        }

        let rows = vec![row];
        let md = render_markdown(&rows, floor_ns, 7);
        assert!(
            md.contains(&format!("<{floor_ns}ns")),
            "markdown must show the below-floor indicator with the stated floor:\n{md}"
        );
        // The table's data rows must not contain a bare "0" latency cell either.
        for line in md.lines().filter(|l| l.starts_with("| staging")) {
            assert!(
                !line.contains("| 0 |") && !line.contains(" 0ms") && !line.contains(" 0 ms"),
                "markdown row must never show a bare zero latency: {line}"
            );
        }
    }

    /// Gate: every rendered Markdown table's header declares exactly as many columns as its rows
    /// emit; a real defect this caught is in `docs/research/pg-foma-typology-speedup.md`.
    #[test]
    fn markdown_header_matches_row_cell_count() {
        // Two fixtures in different roots/categories exercise both rendered tables; both engines per fixture so a speedup cell is actually computed.
        let t = |ns: u64| Timing {
            median_ns: ns,
            min_ns: ns,
            max_ns: ns,
            samples: 7,
        };
        let rows = vec![
            Row::timed(
                "machine",
                "languages",
                "typ-a",
                "w1",
                "complete",
                t(9_000),
                100,
                true,
            ),
            Row::timed(
                "machine",
                "languages",
                "typ-a",
                "w1",
                "compiled",
                t(3_000),
                100,
                true,
            ),
            Row::timed(
                "staging",
                "edge-cases",
                "con-b",
                "w2",
                "complete",
                t(5_000),
                100,
                true,
            ),
            Row::timed(
                "staging",
                "edge-cases",
                "con-b",
                "w2",
                "compiled",
                t(5_000),
                100,
                true,
            ),
        ];
        let md = render_markdown(&rows, 100, 7);

        let cells = |line: &str| line.trim().trim_matches('|').split('|').count();

        let mut tables_checked = 0usize;
        let mut current: Option<usize> = None;
        for line in md.lines() {
            let t = line.trim();
            if !t.starts_with('|') {
                current = None; // left the table body
                continue;
            }
            match current {
                None => current = Some(cells(t)), // header
                Some(expected) => {
                    if t.contains("---") {
                        assert_eq!(
                            cells(t),
                            expected,
                            "separator column count must match the header it follows:\n{t}"
                        );
                        tables_checked += 1;
                    } else {
                        assert_eq!(
                            cells(t),
                            expected,
                            "data row emits a different number of cells than its table header \
                             declares -- markdown will render this misaligned rather than \
                             failing:\n{t}"
                        );
                    }
                }
            }
        }
        assert!(
            tables_checked > 0,
            "no markdown tables found in the rendered document -- this check went vacuous"
        );
    }

    /// Gate: typed refusal diagnostics produce distinct, named rows, never a zero time or an omitted row; tests the conversion directly since no discovered fixture is currently refused.
    #[test]
    fn refusal_diagnostics_produce_named_rows_not_zero_not_omitted() {
        let diags = vec![CapabilityDiagnostic {
            predicate: "mpr-group.overwrite-output",
            construct: "mpr-group".to_owned(),
            witness: "test refusal witness".to_owned(),
        }];
        let floor_ns = 1;
        let rows = refusal_rows(&diags, "test", "edge-cases", "refused", floor_ns);

        assert_eq!(rows.len(), 1, "one diagnostic must produce exactly one row");
        assert!(
            rows.iter().all(|r| r.outcome == "refused"),
            "every refusal row must carry outcome=refused: {rows:?}"
        );
        assert!(
            rows.iter()
                .all(|r| r.median_ns.is_none() && r.min_ns.is_none() && r.max_ns.is_none()),
            "a refusal row must never carry a timing value (never a zero time): {rows:?}"
        );
        assert_eq!(
            rows[0].predicate.as_deref(),
            Some("mpr-group.overwrite-output")
        );
        assert_eq!(rows[0].construct.as_deref(), Some("mpr-group"));
        assert_eq!(rows[0].witness.as_deref(), Some("test refusal witness"));

        let csv = render_csv(&rows);
        assert!(
            csv.contains("refused"),
            "CSV must render the refusal outcome: {csv}"
        );
        assert!(
            csv.contains("mpr-group.overwrite-output"),
            "CSV must name the refusing predicate: {csv}"
        );
        let md = render_markdown(&rows, floor_ns, 1);
        assert!(md.contains("REFUSED"), "Markdown must render refusal: {md}");
        assert!(
            md.contains("mpr-group.overwrite-output"),
            "Markdown must name the refusing predicate: {md}"
        );
    }
    /// Non-vacuity + end-to-end gate: runs the full harness over every discovered fixture, asserts fixtures/words were measured, and writes both artifacts; `#[ignore]`d since it is categorically slower than a unit test.
    #[test]
    #[ignore = "runs the full conformance corpus through both engines -- see this test's own doc \
                for how to invoke it; use rust/tools/typology-speedup.sh"]
    fn full_corpus_report() {
        let fixtures = discover();
        assert!(
            !fixtures.is_empty(),
            "no conformance fixtures discovered -- check the `machine` submodule is initialized \
             (`git submodule update --init machine`) and conformance-staging/ exists"
        );

        let floor_ns = measure_timer_floor_ns();
        let n = repeats();
        eprintln!("[typology_speedup] timer floor calibrated at {floor_ns}ns, {n} repeats/word");

        let mut all_rows = Vec::new();
        let mut total_words = 0usize;
        for f in &fixtures {
            let words_yaml = f.load_words_yaml();
            total_words += words_yaml.words.len();
            eprintln!(
                "[typology_speedup] {} ({} words)...",
                f.label(),
                words_yaml.words.len()
            );
            all_rows.extend(process_fixture(f, floor_ns, n));
        }

        assert!(
            total_words > 0,
            "discovered fixtures carried zero words in total -- harness regression"
        );
        assert!(
            !all_rows.is_empty(),
            "harness produced zero rows despite non-empty fixtures/words"
        );

        let dir = out_dir();
        let (csv_path, md_path) = write_artifacts(&all_rows, floor_ns, n, &dir);
        eprintln!(
            "[typology_speedup] {} fixtures, {} words, {} rows -> {} / {}",
            fixtures.len(),
            total_words,
            all_rows.len(),
            csv_path.display(),
            md_path.display()
        );

        assert!(
            csv_path.is_file(),
            "CSV artifact must exist: {}",
            csv_path.display()
        );
        assert!(
            md_path.is_file(),
            "Markdown artifact must exist: {}",
            md_path.display()
        );
    }
}
