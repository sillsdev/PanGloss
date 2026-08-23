//! `batch --stats`'s cache-writing side and the `stats` subcommand's cache-reading side of `pg_stats::StatsCache`; `--engine=foma` has no collector hook yet, so it records word-level rows only, every counter marked `unsupported`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pg_grammar::model::{AllomorphId, Grammar, LexEntryId, MRuleId, PRuleId};
use serde::Serialize;

/// Bounds one `StatsCache::flush` transaction so a 10k-word run never holds every row in memory at once.
const STATS_FLUSH_BATCH: usize = 500;

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A caller-supplied timestamp string, matching `pack.rs`'s own `now_string` shape.
fn now_utc_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

fn canonical_or_raw(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string())
}

/// The cache's grammar-change fingerprint: `.json`/`.fwdata` hash their `Snapshot`; `.xml` (no `Snapshot` exists) hashes its own raw bytes.
fn grammar_hash_for(path: &str) -> Result<String, String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "json" => {
            let json = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
            let snapshot = pg_snapshot::Snapshot::from_json(&json)
                .map_err(|e| format!("parse snapshot {path}: {e}"))?;
            Ok(snapshot.grammar_hash())
        }
        "fwdata" => {
            let (snapshot, _report) = pg_fwdata::import_file(std::path::Path::new(path))
                .map_err(|e| format!("import {path}: {e}"))?;
            Ok(snapshot.grammar_hash())
        }
        _ => {
            let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
            Ok(sha256_hex(&bytes))
        }
    }
}

fn resolve_cache_path(project_path: &str, cache_override: Option<&str>) -> Result<PathBuf, String> {
    match cache_override {
        Some(p) => Ok(PathBuf::from(p)),
        None => pg_stats::default_cache_path(project_path).map_err(|e| e.to_string()),
    }
}

fn to_stats_kind(k: pg_grammar::stats_identity::ObjectKind) -> pg_stats::ObjectKind {
    use pg_grammar::stats_identity::ObjectKind as G;
    match k {
        G::MorphRule => pg_stats::ObjectKind::MorphRule,
        G::PhonRule => pg_stats::ObjectKind::PhonRule,
        G::LexEntry => pg_stats::ObjectKind::LexEntry,
        G::RootIndex => pg_stats::ObjectKind::RootIndex,
        G::Guesser => pg_stats::ObjectKind::Guesser,
        G::Overlay => pg_stats::ObjectKind::Overlay,
    }
}

fn to_stats_quality(q: pg_grammar::stats_identity::IdentityQuality) -> pg_stats::IdentityQuality {
    use pg_grammar::stats_identity::IdentityQuality as G;
    match q {
        G::Authored => pg_stats::IdentityQuality::Authored,
        G::Structural => pg_stats::IdentityQuality::Structural,
        G::Synthetic => pg_stats::IdentityQuality::Synthetic,
    }
}

/// Maps one collector row's runtime ids to durable identities via `pg_grammar::stats_identity`, treating `row` as opaque otherwise.
fn fact_record_from_stats_row(
    grammar: &Grammar,
    row: &pg_rules::stats::StatsRow,
) -> pg_stats::FactRecord {
    let identity = match row.kind {
        pg_rules::stats::ObjectKind::MorphRule => {
            pg_grammar::stats_identity::morph_rule_identity(grammar, MRuleId(row.object_index))
        }
        pg_rules::stats::ObjectKind::PhonRule => {
            pg_grammar::stats_identity::phon_rule_identity(grammar, PRuleId(row.object_index))
        }
        pg_rules::stats::ObjectKind::LexEntry => {
            pg_grammar::stats_identity::lex_entry_identity(grammar, LexEntryId(row.object_index))
        }
        pg_rules::stats::ObjectKind::RootIndex => {
            pg_grammar::stats_identity::root_index_identity(grammar, row.stratum)
        }
        pg_rules::stats::ObjectKind::Guesser => {
            pg_grammar::stats_identity::guesser_identity(grammar)
        }
        pg_rules::stats::ObjectKind::Overlay => {
            pg_grammar::stats_identity::overlay_identity(grammar)
        }
    };
    let stratum = pg_grammar::stats_identity::stratum_identity(grammar, row.stratum);
    let allomorph = if row.allomorph == pg_rules::stats::ALLOMORPH_NONE {
        None
    } else {
        let a = pg_grammar::stats_identity::allomorph_identity(grammar, AllomorphId(row.allomorph));
        Some(pg_stats::StructuralLocator::new(a.key, a.label))
    };
    let direction = match row.direction {
        pg_rules::stats::Direction::Analysis => pg_stats::Direction::Analysis,
        pg_rules::stats::Direction::Synthesis => pg_stats::Direction::Synthesis,
    };
    pg_stats::FactRecord {
        object_key: identity.key,
        object_kind: to_stats_kind(identity.kind),
        object_label: identity.label,
        identity_quality: to_stats_quality(identity.quality),
        stratum: Some(pg_stats::StructuralLocator::new(stratum.key, stratum.label)),
        allomorph,
        direction,
        attempts: row.counters.attempts,
        work: row.counters.work,
        outputs: row.counters.outputs,
        not_applied: row.counters.not_applied,
        no_root: row.counters.no_root,
        surface_mismatch: row.counters.surface_mismatch,
        uses: row.counters.uses,
        self_time_ns: row.counters.self_time_ns,
    }
}

fn rules_kind_to_stats_kind(k: pg_rules::stats::ObjectKind) -> pg_stats::ObjectKind {
    use pg_rules::stats::ObjectKind as R;
    match k {
        R::MorphRule => pg_stats::ObjectKind::MorphRule,
        R::PhonRule => pg_stats::ObjectKind::PhonRule,
        R::LexEntry => pg_stats::ObjectKind::LexEntry,
        R::RootIndex => pg_stats::ObjectKind::RootIndex,
        R::Guesser => pg_stats::ObjectKind::Guesser,
        R::Overlay => pg_stats::ObjectKind::Overlay,
    }
}

/// Delegates to `pg_rules::stats::WIRED_COUNTERS`, the single source of truth for what the collector actually populates.
fn is_wired(kind: pg_stats::ObjectKind, counter: &str) -> bool {
    pg_rules::stats::WIRED_COUNTERS
        .iter()
        .any(|&(k, c)| rules_kind_to_stats_kind(k) == kind && c == counter)
}

/// Records, per `(kind, counter)`, whether this run's collector could measure it, driven by `is_wired`.
fn write_stats_coverage(
    cache: &pg_stats::StatsCache,
    run_id: i64,
    foma: bool,
) -> Result<(), String> {
    use pg_stats::{CoverageState, ObjectKind};
    const COUNTERS: [&str; 7] = [
        "attempts",
        "work",
        "outputs",
        "not_applied",
        "no_root",
        "surface_mismatch",
        "uses",
    ];
    const KINDS: [ObjectKind; 6] = [
        ObjectKind::MorphRule,
        ObjectKind::PhonRule,
        ObjectKind::LexEntry,
        ObjectKind::RootIndex,
        ObjectKind::Guesser,
        ObjectKind::Overlay,
    ];
    // One transaction for all rows, matching `flush`'s own all-or-nothing shape: an interrupted run must never leave partial coverage behind.
    let tx = cache
        .connection()
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    for kind in KINDS {
        for counter in COUNTERS {
            let state = if foma {
                CoverageState::Unsupported
            } else if is_wired(kind, counter) {
                CoverageState::Measured
            } else {
                CoverageState::Unsupported
            };
            cache
                .write_coverage(run_id, kind, counter, state)
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize)]
struct StatsOptionsRecord {
    engine: &'static str,
    step_cap: Option<usize>,
    word_timeout_ms: Option<u64>,
    memo: Option<bool>,
    guess: bool,
}

/// Shared tail of `run_batch_stats_hc`/`run_batch_stats_foma`: flushes `records` in batches, writes coverage, and prints the one summary line `batch --stats` promises.
fn finish_stats_flush(
    cache: &mut pg_stats::StatsCache,
    grammar_path: &str,
    grammar_hash: &str,
    options: &StatsOptionsRecord,
    records: Vec<pg_stats::WordRecord>,
    skipped: usize,
    elapsed: Duration,
    foma: bool,
) -> Result<(), String> {
    let options_json =
        serde_json::to_string(options).map_err(|e| format!("serialize stats options: {e}"))?;
    let options_hash = sha256_hex(options_json.as_bytes());
    let run = pg_stats::RunMetadata {
        build_info: format!("pangloss/{}", env!("CARGO_PKG_VERSION")),
        fwdata_path: canonical_or_raw(grammar_path),
        grammar_hash: grammar_hash.to_string(),
        engine: options.engine.to_string(),
        options_hash,
        options_json,
        created_utc: now_utc_string(),
    };

    let analyzed = records.len();
    let mut chunks: Vec<&[pg_stats::WordRecord]> = records.chunks(STATS_FLUSH_BATCH).collect();
    if chunks.is_empty() {
        chunks.push(&[]);
    }
    for chunk in chunks {
        let run_id = cache.flush(&run, chunk).map_err(|e| e.to_string())?;
        write_stats_coverage(cache, run_id, foma)?;
    }

    println!(
        "stats: analyzed={analyzed} skipped={skipped} elapsed_ms={:.3}",
        elapsed.as_secs_f64() * 1e3
    );
    Ok(())
}

/// `batch --stats`'s `--engine=hc` path: skips cached words, parses the rest via `Morpher::parse_word_with_stats`, and accumulates the result.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_batch_stats_hc(
    grammar: &Grammar,
    grammar_path: &str,
    morpher: &pg_parse::Morpher,
    opts: &pg_parse::ParseOptions,
    words: &[String],
    step_cap: usize,
    word_timeout_ms: Option<u64>,
    memo: bool,
    guess: bool,
    cache_override: Option<&str>,
) -> Result<(), String> {
    let grammar_hash = grammar_hash_for(grammar_path)?;
    let cache_path = resolve_cache_path(grammar_path, cache_override)?;
    let mut outcome =
        pg_stats::StatsCache::open(&cache_path, &grammar_hash).map_err(|e| e.to_string())?;
    if outcome.wiped {
        println!(
            "stats: cache wiped (grammar changed): {}",
            cache_path.display()
        );
    }

    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    let existing = outcome
        .cache
        .existing_words(&refs)
        .map_err(|e| e.to_string())?;
    let skipped = words
        .iter()
        .filter(|w| existing.contains(w.as_str()))
        .count();

    let start = Instant::now();
    let mut records = Vec::new();
    for word in words {
        if existing.contains(word.as_str()) {
            continue;
        }
        let word_start = Instant::now();
        let (parse_outcome, rows) = morpher.parse_word_with_stats(word, opts);
        let word_elapsed = word_start.elapsed();
        let facts = rows
            .iter()
            .map(|r| fact_record_from_stats_row(grammar, r))
            .collect();
        records.push(pg_stats::WordRecord {
            form: word.clone(),
            elapsed_ns: word_elapsed.as_nanos().min(u128::from(u64::MAX)) as u64,
            attempts: parse_outcome.steps as u64,
            passes: parse_outcome.analyses.len() as u64,
            capped: parse_outcome.capped,
            timed_out: parse_outcome.timed_out,
            invalid_shape: parse_outcome.invalid_shape,
            facts,
        });
    }
    let total_elapsed = start.elapsed();

    let options = StatsOptionsRecord {
        engine: "hc",
        step_cap: Some(step_cap),
        word_timeout_ms,
        memo: Some(memo),
        guess,
    };
    finish_stats_flush(
        &mut outcome.cache,
        grammar_path,
        &grammar_hash,
        &options,
        records,
        skipped,
        total_elapsed,
        false,
    )
}

/// `batch --stats`'s `--engine=foma` path: word-level rows only (no stats hook yet), so `write_stats_coverage` marks every counter `unsupported` for this run.
pub(crate) fn run_batch_stats_foma(
    grammar: &Grammar,
    grammar_path: &str,
    analyzer: &mut pg_foma::composite::FomaAnalyzer,
    words: &[String],
    word_timeout_ms: Option<u64>,
    cache_override: Option<&str>,
) -> Result<(), String> {
    let grammar_hash = grammar_hash_for(grammar_path)?;
    let cache_path = resolve_cache_path(grammar_path, cache_override)?;
    let mut outcome =
        pg_stats::StatsCache::open(&cache_path, &grammar_hash).map_err(|e| e.to_string())?;
    if outcome.wiped {
        println!(
            "stats: cache wiped (grammar changed): {}",
            cache_path.display()
        );
    }

    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    let existing = outcome
        .cache
        .existing_words(&refs)
        .map_err(|e| e.to_string())?;
    let skipped = words
        .iter()
        .filter(|w| existing.contains(w.as_str()))
        .count();

    let start = Instant::now();
    let mut records = Vec::new();
    for word in words {
        if existing.contains(word.as_str()) {
            continue;
        }
        let invalid = crate::foma_invalid_shape(grammar, word);
        let (passes, word_elapsed) = if invalid {
            (0usize, Duration::ZERO)
        } else {
            let word_start = Instant::now();
            let foma_outcome = analyzer.analyze_word(word);
            (foma_outcome.analyses.len(), word_start.elapsed())
        };
        records.push(pg_stats::WordRecord {
            form: word.clone(),
            elapsed_ns: word_elapsed.as_nanos().min(u128::from(u64::MAX)) as u64,
            attempts: 0,
            passes: passes as u64,
            capped: false,
            timed_out: false,
            invalid_shape: invalid,
            facts: Vec::new(),
        });
    }
    let total_elapsed = start.elapsed();

    let options = StatsOptionsRecord {
        engine: "foma",
        step_cap: None,
        word_timeout_ms,
        memo: None,
        guess: false,
    };
    finish_stats_flush(
        &mut outcome.cache,
        grammar_path,
        &grammar_hash,
        &options,
        records,
        skipped,
        total_elapsed,
        true,
    )
}

// The `stats` subcommand: read-only, no grammar loaded.

const STATS_USAGE: &str = "usage: stats <project-or-grammar> [--group word|object|allomorph|stratum|direction|never-fires] [--kind K] [--object KEY] [--stratum KEY] [--direction analysis|synthesis] [--min-attempts N] [--top N] [--sort time|no-root] [--exclude-censored] [--show-work] [--cache <path>] [--out FILE]";

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportGroup {
    Word,
    Object,
    Allomorph,
    Stratum,
    Direction,
    NeverFires,
}

#[derive(Default)]
struct Filters {
    kind: Option<String>,
    object_key: Option<String>,
    stratum_key: Option<String>,
    /// `"analysis"` or `"synthesis"`; validated at parse time, threaded through unchanged.
    direction: Option<String>,
    min_attempts: Option<i64>,
    top_n: Option<usize>,
    sort: Option<pg_stats::SortKey>,
    exclude_censored: bool,
    /// `--show-work`: appends the raw `work` counter column, hidden by default.
    show_work: bool,
}

fn per_object_filter(f: &Filters) -> pg_stats::PerObjectFilter {
    pg_stats::PerObjectFilter {
        kind: f.kind.clone(),
        object_key: f.object_key.clone(),
        stratum_key: f.stratum_key.clone(),
        direction: f.direction.clone(),
        min_attempts: f.min_attempts,
        exclude_censored_words: f.exclude_censored,
        top_n: f.top_n,
        sort: f.sort.unwrap_or_default(),
    }
}

fn per_allomorph_filter(f: &Filters) -> pg_stats::PerAllomorphFilter {
    pg_stats::PerAllomorphFilter {
        kind: f.kind.clone(),
        object_key: f.object_key.clone(),
        direction: f.direction.clone(),
        min_attempts: f.min_attempts,
        exclude_censored_words: f.exclude_censored,
        top_n: f.top_n,
    }
}

fn per_stratum_filter(f: &Filters) -> pg_stats::PerStratumFilter {
    pg_stats::PerStratumFilter {
        kind: f.kind.clone(),
        object_key: f.object_key.clone(),
        direction: f.direction.clone(),
        min_attempts: f.min_attempts,
        exclude_censored_words: f.exclude_censored,
        top_n: f.top_n,
    }
}

/// Unlike the other filters, an unset `--min-attempts` still applies the sensible default floor.
fn never_fires_filter(f: &Filters) -> pg_stats::NeverFiresFilter {
    pg_stats::NeverFiresFilter {
        kind: f.kind.clone(),
        direction: f.direction.clone(),
        min_attempts: f
            .min_attempts
            .unwrap_or(pg_stats::NEVER_FIRES_DEFAULT_MIN_ATTEMPTS),
        exclude_censored_words: f.exclude_censored,
        top_n: f.top_n,
    }
}

fn per_direction_filter(f: &Filters) -> pg_stats::PerDirectionFilter {
    pg_stats::PerDirectionFilter {
        kind: f.kind.clone(),
        object_key: f.object_key.clone(),
        min_attempts: f.min_attempts,
        exclude_censored_words: f.exclude_censored,
    }
}

/// `(kind, counter) -> state` for one run, so a renderer can print `\u{2014}` instead of `0`.
type CoverageMap = HashMap<(String, String), String>;

fn coverage_map_for_latest_run(conn: &rusqlite::Connection) -> Result<CoverageMap, String> {
    let latest_run_id: Option<i64> = conn
        .query_row("SELECT MAX(run_id) FROM run", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    let Some(run_id) = latest_run_id else {
        return Ok(CoverageMap::new());
    };
    Ok(pg_stats::coverage_rows(conn, run_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| ((r.kind, r.counter), r.state))
        .collect())
}

fn counter_cell(kind: &str, counter: &str, value: i64, coverage: &CoverageMap) -> String {
    // An absent coverage row is "I could not look", never "everything is fine" -- it must render like an unsupported one, not a bare number.
    match coverage
        .get(&(kind.to_string(), counter.to_string()))
        .map(String::as_str)
    {
        Some("measured") => value.to_string(),
        _ => "\u{2014}".to_string(),
    }
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn write_csv(path: &str, headers: &[&str], rows: &[Vec<String>]) -> Result<(), String> {
    let mut out = String::new();
    out.push_str(
        &headers
            .iter()
            .map(|h| csv_field(h))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in rows {
        out.push_str(
            &row.iter()
                .map(|c| csv_field(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    std::fs::write(path, out).map_err(|e| format!("write {path}: {e}"))
}

fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }
    let mut out = String::new();
    for (i, h) in headers.iter().enumerate() {
        out.push_str(&format!("{:<width$}  ", h, width = widths[i]));
    }
    out.push('\n');
    for w in &widths {
        out.push_str(&"-".repeat(*w));
        out.push_str("  ");
    }
    out.push('\n');
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            out.push_str(&format!("{:<width$}  ", cell, width = widths[i]));
        }
        out.push('\n');
    }
    out
}

const WORD_HEADERS: [&str; 6] = [
    "form",
    "elapsed_ms_actual",
    "attempts",
    "passes",
    "capped",
    "timed_out",
];

fn word_rows_as_strings(rows: &[pg_stats::PerWordRow]) -> Vec<Vec<String>> {
    rows.iter()
        .map(|r| {
            vec![
                r.form.clone(),
                format!("{:.3}", r.elapsed_ns as f64 / 1e6),
                r.attempts.to_string(),
                r.passes.to_string(),
                r.capped.to_string(),
                r.timed_out.to_string(),
            ]
        })
        .collect()
}

fn render_word(conn: &rusqlite::Connection, out: Option<&str>) -> Result<(), String> {
    let rows = pg_stats::per_word_report(conn).map_err(|e| e.to_string())?;
    let table_rows = word_rows_as_strings(&rows);
    match out {
        Some(path) => write_csv(path, &WORD_HEADERS, &table_rows),
        None => {
            print!("{}", render_table(&WORD_HEADERS, &table_rows));
            Ok(())
        }
    }
}

const OBJECT_HEADERS_BASE: [&str; 11] = [
    "kind",
    "label",
    "identity_quality",
    "attempts",
    "measured_time_ms",
    "outputs",
    "amplification",
    "Didn't apply",
    "No root found",
    "Didn't match the word",
    "uses",
];

/// `OBJECT_HEADERS_BASE`, plus `work` when `--show-work` was passed; the raw counter is opt-in since its per-attempt weighting is provisional (see the spec's "Known limitations").
fn object_headers(show_work: bool) -> Vec<&'static str> {
    let mut h = OBJECT_HEADERS_BASE.to_vec();
    if show_work {
        h.push("work");
    }
    h
}

/// Printed with every per-object/per-allomorph report: `measured_time_ms` is real, so per-kind totals sum exactly.
const OBJECT_TIME_NOTE: &str = "measured_time_ms is wall-clock self time recorded during this --stats run; it sums exactly to the per-kind and per-word totals";

fn amplification_cell(attempts: i64, outputs: i64) -> String {
    if attempts == 0 {
        "\u{2014}".to_string()
    } else {
        format!("{:.2}", outputs as f64 / attempts as f64)
    }
}

/// Measured self time, same units the TSV's other millisecond columns use.
fn measured_cell(self_time_ns: i64) -> String {
    format!("{:.3}", self_time_ns as f64 / 1e6)
}

fn object_row_cells(
    r: &pg_stats::PerObjectRow,
    coverage: &CoverageMap,
    show_work: bool,
) -> Vec<String> {
    let mut cells = vec![
        r.kind.clone(),
        r.label.clone(),
        r.identity_quality.clone(),
        counter_cell(&r.kind, "attempts", r.attempts, coverage),
        measured_cell(r.self_time_ns),
        counter_cell(&r.kind, "outputs", r.outputs, coverage),
        amplification_cell(r.attempts, r.outputs),
        counter_cell(&r.kind, "not_applied", r.not_applied, coverage),
        counter_cell(&r.kind, "no_root", r.no_root, coverage),
        counter_cell(&r.kind, "surface_mismatch", r.surface_mismatch, coverage),
        counter_cell(&r.kind, "uses", r.uses, coverage),
    ];
    if show_work {
        cells.push(counter_cell(&r.kind, "work", r.work, coverage));
    }
    cells
}

fn render_object(
    conn: &rusqlite::Connection,
    coverage: &CoverageMap,
    filters: &Filters,
    out: Option<&str>,
) -> Result<(), String> {
    let filter = per_object_filter(filters);
    let rows = pg_stats::per_object_report(conn, &filter).map_err(|e| e.to_string())?;
    let headers = object_headers(filters.show_work);
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|r| object_row_cells(r, coverage, filters.show_work))
        .collect();
    match out {
        Some(path) => {
            eprintln!("stats: {OBJECT_TIME_NOTE}");
            write_csv(path, &headers, &table_rows)
        }
        None => {
            println!("# {OBJECT_TIME_NOTE}");
            print!("{}", render_table(&headers, &table_rows));
            Ok(())
        }
    }
}

const ALLOMORPH_HEADERS_BASE: [&str; 12] = [
    "object_kind",
    "object_label",
    "allomorph_key",
    "allomorph_label",
    "attempts",
    "measured_time_ms",
    "outputs",
    "amplification",
    "Didn't apply",
    "No root found",
    "Didn't match the word",
    "uses",
];

/// See `object_headers`.
fn allomorph_headers(show_work: bool) -> Vec<&'static str> {
    let mut h = ALLOMORPH_HEADERS_BASE.to_vec();
    if show_work {
        h.push("work");
    }
    h
}

fn allomorph_row_cells(
    r: &pg_stats::PerAllomorphRow,
    coverage: &CoverageMap,
    show_work: bool,
) -> Vec<String> {
    let mut cells = vec![
        r.object_kind.clone(),
        r.object_label.clone(),
        r.allomorph_key.clone().unwrap_or_default(),
        r.allomorph_label.clone(),
        counter_cell(&r.object_kind, "attempts", r.attempts, coverage),
        measured_cell(r.self_time_ns),
        counter_cell(&r.object_kind, "outputs", r.outputs, coverage),
        amplification_cell(r.attempts, r.outputs),
        counter_cell(&r.object_kind, "not_applied", r.not_applied, coverage),
        counter_cell(&r.object_kind, "no_root", r.no_root, coverage),
        counter_cell(
            &r.object_kind,
            "surface_mismatch",
            r.surface_mismatch,
            coverage,
        ),
        counter_cell(&r.object_kind, "uses", r.uses, coverage),
    ];
    if show_work {
        cells.push(counter_cell(&r.object_kind, "work", r.work, coverage));
    }
    cells
}

fn render_allomorph(
    conn: &rusqlite::Connection,
    coverage: &CoverageMap,
    filters: &Filters,
    out: Option<&str>,
) -> Result<(), String> {
    let filter = per_allomorph_filter(filters);
    let rows = pg_stats::per_allomorph_report(conn, &filter).map_err(|e| e.to_string())?;
    let headers = allomorph_headers(filters.show_work);
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|r| allomorph_row_cells(r, coverage, filters.show_work))
        .collect();
    match out {
        Some(path) => {
            eprintln!("stats: {OBJECT_TIME_NOTE}");
            write_csv(path, &headers, &table_rows)
        }
        None => {
            println!("# {OBJECT_TIME_NOTE}");
            print!("{}", render_table(&headers, &table_rows));
            Ok(())
        }
    }
}

const STRATUM_HEADERS_BASE: [&str; 8] = [
    "stratum_key",
    "stratum_label",
    "attempts",
    "outputs",
    "Didn't apply",
    "No root found",
    "Didn't match the word",
    "uses",
];

/// See `object_headers`.
fn stratum_headers(show_work: bool) -> Vec<&'static str> {
    let mut h = STRATUM_HEADERS_BASE.to_vec();
    if show_work {
        h.push("work");
    }
    h
}

/// A stratum row can sum several object kinds at once, so no single `coverage` lookup fits it; narrow with `--kind` for a coverage-accurate view.
fn stratum_row_cells(r: &pg_stats::PerStratumRow, show_work: bool) -> Vec<String> {
    let mut cells = vec![
        r.stratum_key.clone().unwrap_or_default(),
        r.stratum_label.clone(),
        r.attempts.to_string(),
        r.outputs.to_string(),
        r.not_applied.to_string(),
        r.no_root.to_string(),
        r.surface_mismatch.to_string(),
        r.uses.to_string(),
    ];
    if show_work {
        cells.push(r.work.to_string());
    }
    cells
}

fn render_stratum(
    conn: &rusqlite::Connection,
    filters: &Filters,
    out: Option<&str>,
) -> Result<(), String> {
    let filter = per_stratum_filter(filters);
    let rows = pg_stats::per_stratum_report(conn, &filter).map_err(|e| e.to_string())?;
    let headers = stratum_headers(filters.show_work);
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|r| stratum_row_cells(r, filters.show_work))
        .collect();
    match out {
        Some(path) => write_csv(path, &headers, &table_rows),
        None => {
            print!("{}", render_table(&headers, &table_rows));
            Ok(())
        }
    }
}

const DIRECTION_HEADERS_BASE: [&str; 7] = [
    "direction",
    "attempts",
    "outputs",
    "Didn't apply",
    "No root found",
    "Didn't match the word",
    "uses",
];

/// See `object_headers`.
fn direction_headers(show_work: bool) -> Vec<&'static str> {
    let mut h = DIRECTION_HEADERS_BASE.to_vec();
    if show_work {
        h.push("work");
    }
    h
}

/// Only two rows ever exist, so -- like `render_stratum` -- no single `coverage` lookup fits a row that can span several object kinds; narrow with `--kind` for a coverage-accurate view.
fn direction_row_cells(r: &pg_stats::PerDirectionRow, show_work: bool) -> Vec<String> {
    let mut cells = vec![
        r.direction.clone(),
        r.attempts.to_string(),
        r.outputs.to_string(),
        r.not_applied.to_string(),
        r.no_root.to_string(),
        r.surface_mismatch.to_string(),
        r.uses.to_string(),
    ];
    if show_work {
        cells.push(r.work.to_string());
    }
    cells
}

fn render_direction(
    conn: &rusqlite::Connection,
    filters: &Filters,
    out: Option<&str>,
) -> Result<(), String> {
    let filter = per_direction_filter(filters);
    let rows = pg_stats::per_direction_report(conn, &filter).map_err(|e| e.to_string())?;
    let headers = direction_headers(filters.show_work);
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|r| direction_row_cells(r, filters.show_work))
        .collect();
    match out {
        Some(path) => write_csv(path, &headers, &table_rows),
        None => {
            print!("{}", render_table(&headers, &table_rows));
            Ok(())
        }
    }
}

const NEVER_FIRES_HEADERS: [&str; 5] =
    ["kind", "label", "identity_quality", "direction", "attempts"];

fn never_fires_row_cells(r: &pg_stats::NeverFiresRow) -> Vec<String> {
    vec![
        r.kind.clone(),
        r.label.clone(),
        r.identity_quality.clone(),
        r.direction.clone(),
        r.attempts.to_string(),
    ]
}

fn render_never_fires(
    conn: &rusqlite::Connection,
    filters: &Filters,
    out: Option<&str>,
) -> Result<(), String> {
    let filter = never_fires_filter(filters);
    let rows = pg_stats::never_fires_report(conn, &filter).map_err(|e| e.to_string())?;
    let table_rows: Vec<Vec<String>> = rows.iter().map(never_fires_row_cells).collect();
    match out {
        Some(path) => write_csv(path, &NEVER_FIRES_HEADERS, &table_rows),
        None => {
            print!("{}", render_table(&NEVER_FIRES_HEADERS, &table_rows));
            Ok(())
        }
    }
}

fn render_default(conn: &rusqlite::Connection, coverage: &CoverageMap) -> Result<(), String> {
    render_word(conn, None)?;
    println!();
    render_object(conn, coverage, &Filters::default(), None)?;

    let never_fires_rows =
        pg_stats::never_fires_report(conn, &pg_stats::NeverFiresFilter::default())
            .map_err(|e| e.to_string())?;
    if !never_fires_rows.is_empty() {
        println!();
        println!(
            "# never-fires: attempted >= {} time(s) with zero outputs",
            pg_stats::NEVER_FIRES_DEFAULT_MIN_ATTEMPTS
        );
        let table_rows: Vec<Vec<String>> =
            never_fires_rows.iter().map(never_fires_row_cells).collect();
        print!("{}", render_table(&NEVER_FIRES_HEADERS, &table_rows));
    }
    Ok(())
}

/// `stats <project-or-grammar> [options]`: reads the cache via a raw `rusqlite::Connection`, never `StatsCache::open`, so a report-only read can never trip the grammar-hash wipe gate.
pub(crate) fn run_stats(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut group: Option<String> = None;
    let mut filters = Filters::default();
    let mut sort_arg: Option<String> = None;
    let mut cache_override: Option<String> = None;
    let mut out_path: Option<String> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--group" => group = Some(it.next().ok_or("--group requires a value")?.clone()),
            s if s.starts_with("--group=") => group = Some(s["--group=".len()..].to_string()),
            "--kind" => filters.kind = Some(it.next().ok_or("--kind requires a value")?.clone()),
            s if s.starts_with("--kind=") => filters.kind = Some(s["--kind=".len()..].to_string()),
            "--object" => {
                filters.object_key = Some(it.next().ok_or("--object requires a value")?.clone())
            }
            s if s.starts_with("--object=") => {
                filters.object_key = Some(s["--object=".len()..].to_string())
            }
            "--stratum" => {
                filters.stratum_key = Some(it.next().ok_or("--stratum requires a value")?.clone())
            }
            s if s.starts_with("--stratum=") => {
                filters.stratum_key = Some(s["--stratum=".len()..].to_string())
            }
            "--direction" => {
                filters.direction = Some(it.next().ok_or("--direction requires a value")?.clone())
            }
            s if s.starts_with("--direction=") => {
                filters.direction = Some(s["--direction=".len()..].to_string())
            }
            "--min-attempts" => {
                let v = it.next().ok_or("--min-attempts requires a value")?;
                filters.min_attempts = Some(
                    v.parse()
                        .map_err(|_| format!("invalid --min-attempts: {v}"))?,
                );
            }
            s if s.starts_with("--min-attempts=") => {
                let v = &s["--min-attempts=".len()..];
                filters.min_attempts = Some(
                    v.parse()
                        .map_err(|_| format!("invalid --min-attempts: {v}"))?,
                );
            }
            "--top" => {
                let v = it.next().ok_or("--top requires a value")?;
                filters.top_n = Some(v.parse().map_err(|_| format!("invalid --top: {v}"))?);
            }
            s if s.starts_with("--top=") => {
                let v = &s["--top=".len()..];
                filters.top_n = Some(v.parse().map_err(|_| format!("invalid --top: {v}"))?);
            }
            "--sort" => sort_arg = Some(it.next().ok_or("--sort requires a value")?.clone()),
            s if s.starts_with("--sort=") => sort_arg = Some(s["--sort=".len()..].to_string()),
            "--exclude-censored" => filters.exclude_censored = true,
            "--show-work" => filters.show_work = true,
            "--cache" => {
                cache_override = Some(it.next().ok_or("--cache requires a value")?.clone())
            }
            s if s.starts_with("--cache=") => {
                cache_override = Some(s["--cache=".len()..].to_string())
            }
            "--out" => out_path = Some(it.next().ok_or("--out requires a value")?.clone()),
            s if s.starts_with("--out=") => out_path = Some(s["--out=".len()..].to_string()),
            s => positional.push(s),
        }
    }

    let [project_path] = positional[..] else {
        return Err(STATS_USAGE.to_string());
    };

    let group = match group.as_deref() {
        None => None,
        Some("word") => Some(ReportGroup::Word),
        Some("object") => Some(ReportGroup::Object),
        Some("allomorph") => Some(ReportGroup::Allomorph),
        Some("stratum") => Some(ReportGroup::Stratum),
        Some("direction") => Some(ReportGroup::Direction),
        Some("never-fires") => Some(ReportGroup::NeverFires),
        Some(other) => {
            return Err(format!(
                "invalid --group: {other} (expected word|object|allomorph|stratum|direction|never-fires)"
            ))
        }
    };
    if let Some(d) = filters.direction.as_deref() {
        if d != "analysis" && d != "synthesis" {
            return Err(format!(
                "invalid --direction: {d} (expected analysis|synthesis)"
            ));
        }
    }
    if filters.direction.is_some() && matches!(group, Some(ReportGroup::Word)) {
        return Err(
            "--direction does not apply to --group word: word rows have no direction dimension"
                .to_string(),
        );
    }
    filters.sort = match sort_arg.as_deref() {
        None => None,
        Some("time") => Some(pg_stats::SortKey::SelfTimeNs),
        Some("no-root") => Some(pg_stats::SortKey::NoRoot),
        Some(other) => return Err(format!("invalid --sort: {other} (expected time|no-root)")),
    };
    if filters.sort.is_some() && !matches!(group, None | Some(ReportGroup::Object)) {
        return Err("--sort only applies to --group object (or the default view)".to_string());
    }
    if filters.stratum_key.is_some() && !matches!(group, None | Some(ReportGroup::Object)) {
        return Err("--stratum only applies to --group object (or the default view)".to_string());
    }
    if out_path.is_some() && group.is_none() {
        return Err(
            "--out requires --group (a CSV file holds one table shape; the default view renders two)"
                .to_string(),
        );
    }

    let cache_path = resolve_cache_path(project_path, cache_override.as_deref())?;
    if !cache_path.exists() {
        println!("stats: no cache found at {}", cache_path.display());
        return Ok(());
    }
    let conn = rusqlite::Connection::open(&cache_path).map_err(|e| e.to_string())?;
    let run_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM run", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if run_count == 0 {
        println!(
            "stats: cache at {} has no recorded runs yet",
            cache_path.display()
        );
        return Ok(());
    }

    let mixed = pg_stats::mixed_settings(&conn).map_err(|e| e.to_string())?;
    if mixed.is_mixed() {
        println!(
            "warning: this cache spans {} distinct options_hash and {} distinct counter_semantics value(s); counters may not be directly comparable",
            mixed.distinct_options_hashes, mixed.distinct_counter_semantics
        );
    }
    let distinct_engines: i64 = conn
        .query_row("SELECT COUNT(DISTINCT engine) FROM run", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    if distinct_engines > 1 {
        println!(
            "warning: this cache spans more than one engine; per-object counters mix measured and unsupported rows"
        );
    }

    let coverage = coverage_map_for_latest_run(&conn)?;

    match group {
        None => render_default(&conn, &coverage),
        Some(ReportGroup::Word) => render_word(&conn, out_path.as_deref()),
        Some(ReportGroup::Object) => render_object(&conn, &coverage, &filters, out_path.as_deref()),
        Some(ReportGroup::Allomorph) => {
            render_allomorph(&conn, &coverage, &filters, out_path.as_deref())
        }
        Some(ReportGroup::Stratum) => render_stratum(&conn, &filters, out_path.as_deref()),
        Some(ReportGroup::Direction) => render_direction(&conn, &filters, out_path.as_deref()),
        Some(ReportGroup::NeverFires) => render_never_fires(&conn, &filters, out_path.as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "pangloss-stats-cmd-test-{tag}-{}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// One conformance fixture's grammar plus a word guaranteed not `expect_skip`.
    fn fixture_grammar_and_word(category: &str, name: &str) -> (String, String) {
        let fixtures = pg_conformance_fixtures::discover();
        let f = fixtures
            .iter()
            .find(|f| f.category == category && f.name == name)
            .unwrap_or_else(|| panic!("fixture {category}/{name} must be discoverable"));
        let words = f.load_words_yaml();
        let word = words
            .words
            .iter()
            .find(|w| !w.expect_skip)
            .expect("fixture must have at least one non-skip word")
            .word
            .clone();
        (f.load_grammar_xml(), word)
    }

    /// Exercises at least one morphological/phonological rule, unlike a bare-root grammar.
    fn primary_fixture() -> (String, String) {
        fixture_grammar_and_word("languages", "metathesis-phase-isolation")
    }

    /// A second, structurally different fixture, for the grammar-change/wipe test.
    fn secondary_fixture() -> (String, String) {
        fixture_grammar_and_word("edge-cases", "truncate-morphotactic")
    }

    fn run_batch_args(
        dir: &std::path::Path,
        grammar_xml: &str,
        words_text: &str,
        extra: &[&str],
    ) -> (Vec<String>, PathBuf) {
        let grammar_path = dir.join("grammar.xml");
        let words_path = dir.join("words.txt");
        let out_path = dir.join("out.tsv");
        fs::write(&grammar_path, grammar_xml).expect("write grammar");
        fs::write(&words_path, words_text).expect("write words");
        let mut args: Vec<String> = vec![
            grammar_path.to_string_lossy().into_owned(),
            words_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        (args, out_path)
    }

    #[test]
    fn batch_stats_produces_nonempty_object_report_and_tsv_stays_byte_identical() {
        let (grammar_xml, word) = primary_fixture();
        let words_text = format!("{word}\n");

        let dir_plain = scratch_dir("tsv-plain");
        let (args_plain, out_plain) = run_batch_args(&dir_plain, &grammar_xml, &words_text, &[]);
        crate::run_batch(&args_plain).expect("plain batch run");
        let tsv_plain = fs::read_to_string(&out_plain).expect("read plain tsv");

        let dir_stats = scratch_dir("tsv-stats");
        let cache_path = dir_stats.join("cache.sqlite3");
        let (args_stats, out_stats) = run_batch_args(
            &dir_stats,
            &grammar_xml,
            &words_text,
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args_stats).expect("stats batch run");
        let tsv_stats = fs::read_to_string(&out_stats).expect("read stats tsv");

        assert_eq!(
            tsv_plain, tsv_stats,
            "batch's TSV output must be byte-identical with or without --stats"
        );

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let rows =
            pg_stats::per_object_report(&conn, &pg_stats::PerObjectFilter::default()).unwrap();
        assert!(
            !rows.is_empty(),
            "a fixture with morphological rules must produce at least one per-object row"
        );
        let coverage = coverage_map_for_latest_run(&conn).unwrap();
        assert_eq!(
            coverage
                .get(&("morph_rule".to_string(), "attempts".to_string()))
                .map(String::as_str),
            Some("measured"),
            "the hc engine's attempts counter must be marked measured"
        );
    }

    #[test]
    fn batch_stats_run_twice_skips_already_cached_words() {
        let (grammar_xml, word) = primary_fixture();
        let words_text = format!("{word}\n");
        let dir = scratch_dir("accumulate");
        let cache_path = dir.join("cache.sqlite3");

        let (args_first, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &words_text,
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args_first).expect("first stats batch run");

        let (args_second, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &words_text,
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args_second).expect("second stats batch run");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let word_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM word", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            word_count, 1,
            "the same word run twice must accumulate to exactly one word row, not two"
        );
    }

    #[test]
    fn batch_stats_wipes_cache_and_reports_it_when_grammar_changes() {
        let (grammar_xml, word) = primary_fixture();
        let (other_grammar_xml, other_word) = secondary_fixture();
        let dir = scratch_dir("wipe");
        let cache_path = dir.join("cache.sqlite3");

        let (args_first, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &format!("{word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args_first).expect("first stats batch run");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let first_word_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM word", [], |row| row.get(0))
            .unwrap();
        assert_eq!(first_word_count, 1);
        drop(conn);

        let (args_second, _) = run_batch_args(
            &dir,
            &other_grammar_xml,
            &format!("{other_word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args_second).expect("second stats batch run after grammar change");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let second_word_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM word", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            second_word_count, 1,
            "a grammar change must wipe the old word before recording the new one"
        );
        let form: String = conn
            .query_row("SELECT form FROM word", [], |row| row.get(0))
            .unwrap();
        assert_eq!(form, other_word, "only the new grammar's word must remain");
    }

    /// Root-only, no rules at all, so `FomaAnalyzer::new` always has a trivial network to compile.
    const FOMA_FRIENDLY_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>StatsCmdFomaFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>Noun</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="seg1"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bdry1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="le1" partOfSpeech="n">
            <Allomorphs><Allomorph id="le1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn batch_stats_accepts_engine_foma_and_marks_no_root_unsupported() {
        let dir = scratch_dir("foma");
        let cache_path = dir.join("cache.sqlite3");
        let (args, _) = run_batch_args(
            &dir,
            FOMA_FRIENDLY_GRAMMAR_XML,
            "kat\n",
            &[
                "--engine=foma",
                "--stats",
                "--cache",
                cache_path.to_str().unwrap(),
            ],
        );
        crate::run_batch(&args).expect("--engine=foma --stats must be accepted, not refused");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let coverage = coverage_map_for_latest_run(&conn).unwrap();
        assert_eq!(
            coverage
                .get(&("morph_rule".to_string(), "no_root".to_string()))
                .map(String::as_str),
            Some("unsupported"),
            "the foma engine's analysis-side no_root counter must be marked unsupported, not measured"
        );
    }

    fn coverage_entry(state: pg_stats::CoverageState) -> CoverageMap {
        let mut m = CoverageMap::new();
        m.insert(
            ("morph_rule".to_string(), "no_root".to_string()),
            state.as_str().to_string(),
        );
        m
    }

    fn sample_object_row(no_root: i64) -> pg_stats::PerObjectRow {
        pg_stats::PerObjectRow {
            kind: "morph_rule".to_string(),
            label: "Rule A".to_string(),
            identity_quality: "authored".to_string(),
            attempts: 5,
            work: 20,
            outputs: 2,
            not_applied: 1,
            no_root,
            surface_mismatch: 0,
            uses: 1,
            self_time_ns: 150,
        }
    }

    #[test]
    fn unsupported_counter_renders_em_dash_not_zero() {
        let coverage = coverage_entry(pg_stats::CoverageState::Unsupported);
        let row = sample_object_row(0);
        let cells = object_row_cells(&row, &coverage, false);
        let no_root_col = object_headers(false)
            .iter()
            .position(|h| *h == "No root found")
            .unwrap();
        assert_eq!(cells[no_root_col], "\u{2014}");

        let measured = coverage_entry(pg_stats::CoverageState::Measured);
        let cells_measured = object_row_cells(&row, &measured, false);
        assert_eq!(
            cells_measured[no_root_col], "0",
            "falsifiability check: a measured zero must still render as a plain 0"
        );
    }

    #[test]
    fn absent_coverage_row_renders_em_dash_not_a_number() {
        // No entry at all for (morph_rule, no_root) -- distinct from an explicit "unsupported" row.
        let coverage = CoverageMap::new();
        let row = sample_object_row(7);
        let cells = object_row_cells(&row, &coverage, false);
        let no_root_col = object_headers(false)
            .iter()
            .position(|h| *h == "No root found")
            .unwrap();
        assert_eq!(
            cells[no_root_col], "\u{2014}",
            "a coverage row that was never written must never read as a measured value"
        );
    }

    #[test]
    fn deleted_coverage_row_renders_em_dash_not_zero_in_a_real_cache() {
        let (grammar_xml, word) = primary_fixture();
        let dir = scratch_dir("coverage-delete");
        let cache_path = dir.join("cache.sqlite3");
        let (args, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &format!("{word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args).expect("seed the cache via batch --stats");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        conn.execute(
            "DELETE FROM coverage WHERE kind = 'morph_rule' AND counter = 'attempts'",
            [],
        )
        .unwrap();

        let coverage = coverage_map_for_latest_run(&conn).unwrap();
        assert!(
            coverage
                .get(&("morph_rule".to_string(), "attempts".to_string()))
                .is_none(),
            "sanity: the row must actually be gone before checking the render"
        );

        let rows =
            pg_stats::per_object_report(&conn, &pg_stats::PerObjectFilter::default()).unwrap();
        let morph_row = rows
            .iter()
            .find(|r| r.kind == "morph_rule")
            .expect("a morph_rule row must exist for this fixture");
        let cells = object_row_cells(morph_row, &coverage, false);
        let attempts_col = object_headers(false)
            .iter()
            .position(|h| *h == "attempts")
            .unwrap();
        assert_eq!(
            cells[attempts_col], "\u{2014}",
            "a deleted coverage row must render as unmeasured, never as a bare number"
        );
    }

    #[test]
    fn group_object_renders_measured_not_actual_and_group_word_is_the_reverse() {
        assert!(object_headers(false).contains(&"measured_time_ms"));
        assert!(!object_headers(false).iter().any(|h| h.contains("actual")));
        assert!(WORD_HEADERS.contains(&"elapsed_ms_actual"));
        assert!(!WORD_HEADERS.iter().any(|h| h.contains("measured")));
    }

    #[test]
    fn show_work_appends_the_work_column_and_is_absent_by_default() {
        assert!(!object_headers(false).contains(&"work"));
        assert!(object_headers(true).contains(&"work"));
        assert!(!allomorph_headers(false).contains(&"work"));
        assert!(allomorph_headers(true).contains(&"work"));
        assert!(!stratum_headers(false).contains(&"work"));
        assert!(stratum_headers(true).contains(&"work"));
        assert!(!direction_headers(false).contains(&"work"));
        assert!(direction_headers(true).contains(&"work"));

        let row = sample_object_row(0);
        let coverage = coverage_entry(pg_stats::CoverageState::Measured);
        assert_eq!(
            object_row_cells(&row, &coverage, false).len(),
            object_headers(false).len()
        );
        assert_eq!(
            object_row_cells(&row, &coverage, true).len(),
            object_headers(true).len()
        );
    }

    #[test]
    fn csv_output_has_stable_header_and_is_parseable() {
        let dir = scratch_dir("csv");
        let out_path = dir.join("out.csv");
        let rows = vec![vec![
            "morph_rule".to_string(),
            "Rule, A".to_string(),
            "authored".to_string(),
            "5".to_string(),
            "0.200".to_string(),
            "2".to_string(),
            "0.40".to_string(),
            "1".to_string(),
            "\u{2014}".to_string(),
            "0".to_string(),
            "1".to_string(),
        ]];
        let headers = object_headers(false);
        write_csv(out_path.to_str().unwrap(), &headers, &rows).unwrap();
        let text = fs::read_to_string(&out_path).unwrap();
        let mut lines = text.lines();
        assert_eq!(
            lines.next().unwrap(),
            headers.join(","),
            "header row must match object_headers(false) in stable order"
        );
        assert!(
            lines.next().unwrap().contains("\"Rule, A\""),
            "an embedded comma must be quoted"
        );
    }

    #[test]
    fn run_stats_end_to_end_group_object_writes_csv_and_default_view_succeeds() {
        let (grammar_xml, word) = primary_fixture();
        let dir = scratch_dir("run-stats-e2e");
        let cache_path = dir.join("cache.sqlite3");
        let (batch_args, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &format!("{word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&batch_args).expect("seed the cache via batch --stats");

        let grammar_path = dir.join("grammar.xml");
        let csv_path = dir.join("object.csv");
        let stats_args: Vec<String> = vec![
            grammar_path.to_string_lossy().into_owned(),
            "--group".to_string(),
            "object".to_string(),
            "--cache".to_string(),
            cache_path.to_string_lossy().into_owned(),
            "--out".to_string(),
            csv_path.to_string_lossy().into_owned(),
        ];
        run_stats(&stats_args).expect("run_stats --group object --out must succeed");
        let csv_text = fs::read_to_string(&csv_path).unwrap();
        assert_eq!(
            csv_text.lines().next().unwrap(),
            object_headers(false).join(",")
        );

        let default_args: Vec<String> = vec![
            grammar_path.to_string_lossy().into_owned(),
            "--cache".to_string(),
            cache_path.to_string_lossy().into_owned(),
        ];
        run_stats(&default_args)
            .expect("run_stats with no --group must render the dual default view");
    }
}
