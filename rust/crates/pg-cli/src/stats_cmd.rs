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
    // Only a `lex_entry` object realizes a single morpheme; every other kind carries no locator.
    let morpheme = if row.kind == pg_rules::stats::ObjectKind::LexEntry {
        let morpheme_id = grammar.entries[row.object_index as usize].morpheme;
        let m = pg_grammar::stats_identity::morpheme_identity(grammar, morpheme_id);
        Some(pg_stats::StructuralLocator::new(m.key, m.label))
    } else {
        None
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
        morpheme,
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

const STATS_USAGE: &str = "usage: stats <project-or-grammar> [--group word|object|allomorph|stratum|direction|morpheme|group|never-fires] [--kind K] [--object KEY] [--stratum KEY] [--direction analysis|synthesis] [--word FORM] [--top N] [--sort time|no-root|amp|uses|attempts] [--exclude-censored] [--wide] [--by-kind] [--format text|jsonl] [--cache <path>] [--out FILE]";

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportGroup {
    Word,
    Object,
    Allomorph,
    Stratum,
    Direction,
    Morpheme,
    Group,
    NeverFires,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Jsonl,
}

#[derive(Default)]
struct Filters {
    kind: Option<String>,
    object_key: Option<String>,
    stratum_key: Option<String>,
    /// `"analysis"` or `"synthesis"`; validated at parse time, threaded through unchanged.
    direction: Option<String>,
    /// Narrows to one word's own fact rows -- the find-the-bad-word-then-the-bad-rule workflow.
    word: Option<String>,
    top_n: Option<usize>,
    sort: Option<pg_stats::SortKey>,
    exclude_censored: bool,
    /// `--wide`: appends `work`/`not_applied`/`no_root`/`surface_mismatch`/`identity_quality`.
    wide: bool,
    /// `--by-kind`: one section per kind, each with its share of the run's total time.
    by_kind: bool,
}

fn per_object_filter(f: &Filters) -> pg_stats::PerObjectFilter {
    pg_stats::PerObjectFilter {
        kind: f.kind.clone(),
        object_key: f.object_key.clone(),
        stratum_key: f.stratum_key.clone(),
        direction: f.direction.clone(),
        word: f.word.clone(),
        exclude_censored_words: f.exclude_censored,
        sort: f.sort.unwrap_or_default(),
    }
}

fn per_allomorph_filter(f: &Filters) -> pg_stats::PerAllomorphFilter {
    pg_stats::PerAllomorphFilter {
        kind: f.kind.clone(),
        object_key: f.object_key.clone(),
        direction: f.direction.clone(),
        word: f.word.clone(),
        exclude_censored_words: f.exclude_censored,
    }
}

fn per_stratum_filter(f: &Filters) -> pg_stats::PerStratumFilter {
    pg_stats::PerStratumFilter {
        kind: f.kind.clone(),
        object_key: f.object_key.clone(),
        direction: f.direction.clone(),
        word: f.word.clone(),
        exclude_censored_words: f.exclude_censored,
    }
}

fn per_direction_filter(f: &Filters) -> pg_stats::PerDirectionFilter {
    pg_stats::PerDirectionFilter {
        kind: f.kind.clone(),
        object_key: f.object_key.clone(),
        word: f.word.clone(),
        exclude_censored_words: f.exclude_censored,
    }
}

fn per_kind_filter(f: &Filters) -> pg_stats::PerKindFilter {
    pg_stats::PerKindFilter {
        kind: f.kind.clone(),
        direction: f.direction.clone(),
        word: f.word.clone(),
        exclude_censored_words: f.exclude_censored,
    }
}

fn per_morpheme_filter(f: &Filters) -> pg_stats::PerMorphemeFilter {
    pg_stats::PerMorphemeFilter {
        direction: f.direction.clone(),
        word: f.word.clone(),
        exclude_censored_words: f.exclude_censored,
    }
}

/// The CLI carries no general `--min-attempts` override; never-fires always uses its own internal default.
fn never_fires_filter(f: &Filters) -> pg_stats::NeverFiresFilter {
    pg_stats::NeverFiresFilter {
        kind: f.kind.clone(),
        direction: f.direction.clone(),
        word: f.word.clone(),
        min_attempts: pg_stats::NEVER_FIRES_DEFAULT_MIN_ATTEMPTS,
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

/// `None` unless `coverage` explicitly marks `(kind, counter)` measured -- an absent row is unsupported too.
fn masked(kind: &str, counter: &str, value: i64, coverage: &CoverageMap) -> Option<i64> {
    match coverage
        .get(&(kind.to_string(), counter.to_string()))
        .map(String::as_str)
    {
        Some("measured") => Some(value),
        _ => None,
    }
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

/// A shape-normalized row for the six counter-bearing orientations; `None` renders as `-`, whether that means "not applicable" or "unmeasured".
#[derive(Clone)]
struct RowView {
    label: String,
    kind: Option<String>,
    identity_quality: Option<String>,
    self_time_ns: Option<i64>,
    attempts: Option<i64>,
    outputs: Option<i64>,
    uses: Option<i64>,
    work: Option<i64>,
    not_applied: Option<i64>,
    no_root: Option<i64>,
    surface_mismatch: Option<i64>,
}

fn object_row_view(r: &pg_stats::PerObjectRow, coverage: &CoverageMap) -> RowView {
    RowView {
        label: format!("{}: {}", r.kind, r.label),
        kind: Some(r.kind.clone()),
        identity_quality: Some(r.identity_quality.clone()),
        self_time_ns: Some(r.self_time_ns),
        attempts: masked(&r.kind, "attempts", r.attempts, coverage),
        outputs: masked(&r.kind, "outputs", r.outputs, coverage),
        uses: masked(&r.kind, "uses", r.uses, coverage),
        work: masked(&r.kind, "work", r.work, coverage),
        not_applied: masked(&r.kind, "not_applied", r.not_applied, coverage),
        no_root: masked(&r.kind, "no_root", r.no_root, coverage),
        surface_mismatch: masked(&r.kind, "surface_mismatch", r.surface_mismatch, coverage),
    }
}

fn allomorph_row_view(r: &pg_stats::PerAllomorphRow, coverage: &CoverageMap) -> RowView {
    RowView {
        label: format!(
            "{}: {} [{}]",
            r.object_kind, r.object_label, r.allomorph_label
        ),
        kind: Some(r.object_kind.clone()),
        identity_quality: None,
        self_time_ns: Some(r.self_time_ns),
        attempts: masked(&r.object_kind, "attempts", r.attempts, coverage),
        outputs: masked(&r.object_kind, "outputs", r.outputs, coverage),
        uses: masked(&r.object_kind, "uses", r.uses, coverage),
        work: masked(&r.object_kind, "work", r.work, coverage),
        not_applied: masked(&r.object_kind, "not_applied", r.not_applied, coverage),
        no_root: masked(&r.object_kind, "no_root", r.no_root, coverage),
        surface_mismatch: masked(
            &r.object_kind,
            "surface_mismatch",
            r.surface_mismatch,
            coverage,
        ),
    }
}

/// A stratum can span several kinds at once, so counters render unmasked; narrow with `--kind`.
fn stratum_row_view(r: &pg_stats::PerStratumRow) -> RowView {
    RowView {
        label: r.stratum_label.clone(),
        kind: None,
        identity_quality: None,
        self_time_ns: Some(r.self_time_ns),
        attempts: Some(r.attempts),
        outputs: Some(r.outputs),
        uses: Some(r.uses),
        work: Some(r.work),
        not_applied: Some(r.not_applied),
        no_root: Some(r.no_root),
        surface_mismatch: Some(r.surface_mismatch),
    }
}

/// See `stratum_row_view` -- only two rows ever exist, but they too can span several kinds.
fn direction_row_view(r: &pg_stats::PerDirectionRow) -> RowView {
    RowView {
        label: r.direction.clone(),
        kind: None,
        identity_quality: None,
        self_time_ns: Some(r.self_time_ns),
        attempts: Some(r.attempts),
        outputs: Some(r.outputs),
        uses: Some(r.uses),
        work: Some(r.work),
        not_applied: Some(r.not_applied),
        no_root: Some(r.no_root),
        surface_mismatch: Some(r.surface_mismatch),
    }
}

fn morpheme_row_view(r: &pg_stats::PerMorphemeRow, coverage: &CoverageMap) -> RowView {
    const KIND: &str = "lex_entry";
    RowView {
        label: r.morpheme_label.clone(),
        kind: Some(KIND.to_string()),
        identity_quality: None,
        self_time_ns: Some(r.self_time_ns),
        attempts: masked(KIND, "attempts", r.attempts, coverage),
        outputs: masked(KIND, "outputs", r.outputs, coverage),
        uses: masked(KIND, "uses", r.uses, coverage),
        work: masked(KIND, "work", r.work, coverage),
        not_applied: masked(KIND, "not_applied", r.not_applied, coverage),
        no_root: masked(KIND, "no_root", r.no_root, coverage),
        surface_mismatch: masked(KIND, "surface_mismatch", r.surface_mismatch, coverage),
    }
}

fn kind_row_view(r: &pg_stats::PerKindRow, coverage: &CoverageMap) -> RowView {
    RowView {
        label: r.kind.clone(),
        kind: Some(r.kind.clone()),
        identity_quality: None,
        self_time_ns: Some(r.self_time_ns),
        attempts: masked(&r.kind, "attempts", r.attempts, coverage),
        outputs: masked(&r.kind, "outputs", r.outputs, coverage),
        uses: masked(&r.kind, "uses", r.uses, coverage),
        work: masked(&r.kind, "work", r.work, coverage),
        not_applied: masked(&r.kind, "not_applied", r.not_applied, coverage),
        no_root: masked(&r.kind, "no_root", r.no_root, coverage),
        surface_mismatch: masked(&r.kind, "surface_mismatch", r.surface_mismatch, coverage),
    }
}

fn fmt_ms(ns: Option<i64>) -> String {
    match ns {
        Some(v) => format!("{:.3}", v as f64 / 1e6),
        None => "-".to_string(),
    }
}

fn fmt_pct(v: Option<i64>, total: i64) -> String {
    match v {
        Some(v) if total > 0 => format!("{:.1}%", v as f64 / total as f64 * 100.0),
        _ => "-".to_string(),
    }
}

fn fmt_amp(attempts: Option<i64>, outputs: Option<i64>) -> String {
    match (attempts, outputs) {
        (Some(a), Some(o)) if a > 0 => format!("{:.2}", o as f64 / a as f64),
        _ => "-".to_string(),
    }
}

fn fmt_opt_i64(v: Option<i64>) -> String {
    v.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string())
}

const NARROW_HEADERS: [&str; 7] = [
    "label",
    "time_ms",
    "time%",
    "attempts",
    "attempts%",
    "amp",
    "uses",
];
const WIDE_EXTRA_HEADERS: [&str; 5] = [
    "work",
    "not_applied",
    "no_root",
    "surface_mismatch",
    "identity_quality",
];

/// Kinds paired with their summed `attempts`, heaviest first, ties broken by name for determinism.
fn attempts_by_kind_desc(rows: &[RowView]) -> Vec<(Option<String>, i64)> {
    let mut totals: HashMap<Option<String>, i64> = HashMap::new();
    for r in rows {
        if let Some(a) = r.attempts {
            *totals.entry(r.kind.clone()).or_insert(0) += a;
        }
    }
    let mut out: Vec<(Option<String>, i64)> = totals.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// Kinds in the order they first appear in `rows`, which is already the caller's sort order.
fn kinds_in_row_order(rows: &[RowView]) -> Vec<Option<String>> {
    let mut seen: Vec<Option<String>> = Vec::new();
    for r in rows {
        if !seen.contains(&r.kind) {
            seen.push(r.kind.clone());
        }
    }
    seen
}

/// Percentage denominators, taken from every matched row rather than from a `--top` excerpt.
struct Denominators {
    /// Global: measured self time means the same thing whatever kind produced it.
    total_time_ns: i64,
    /// Per kind, because a rule's `attempts` is an invocation while a lexical entry's is a candidate materialization: one cross-kind share compares different units.
    attempts_by_kind: HashMap<Option<String>, i64>,
}

impl Denominators {
    fn of(rows: &[RowView]) -> Self {
        let mut attempts_by_kind: HashMap<Option<String>, i64> = HashMap::new();
        for r in rows {
            if let Some(a) = r.attempts {
                *attempts_by_kind.entry(r.kind.clone()).or_insert(0) += a;
            }
        }
        Denominators {
            total_time_ns: rows.iter().filter_map(|r| r.self_time_ns).sum(),
            attempts_by_kind,
        }
    }

    fn attempts_of(&self, kind: &Option<String>) -> i64 {
        self.attempts_by_kind.get(kind).copied().unwrap_or(0)
    }
}

/// Denominators are arguments, never sums of `rows`: `rows` may be a `--top` excerpt, and a share of an excerpt is not the share a reader reads it as.
fn render_narrow(
    rows: &[RowView],
    wide: bool,
    denoms: &Denominators,
) -> (Vec<&'static str>, Vec<Vec<String>>) {
    let mut headers: Vec<&'static str> = NARROW_HEADERS.to_vec();
    if wide {
        headers.extend(WIDE_EXTRA_HEADERS);
    }
    let table_rows = rows
        .iter()
        .map(|r| {
            let mut cells = vec![
                r.label.clone(),
                fmt_ms(r.self_time_ns),
                fmt_pct(r.self_time_ns, denoms.total_time_ns),
                fmt_opt_i64(r.attempts),
                fmt_pct(r.attempts, denoms.attempts_of(&r.kind)),
                fmt_amp(r.attempts, r.outputs),
                fmt_opt_i64(r.uses),
            ];
            if wide {
                cells.push(fmt_opt_i64(r.work));
                cells.push(fmt_opt_i64(r.not_applied));
                cells.push(fmt_opt_i64(r.no_root));
                cells.push(fmt_opt_i64(r.surface_mismatch));
                cells.push(
                    r.identity_quality
                        .clone()
                        .unwrap_or_else(|| "-".to_string()),
                );
            }
            cells
        })
        .collect();
    (headers, table_rows)
}

fn row_view_to_json(r: &RowView) -> serde_json::Value {
    let amp = match (r.attempts, r.outputs) {
        (Some(a), Some(o)) if a > 0 => Some(o as f64 / a as f64),
        _ => None,
    };
    serde_json::json!({
        "label": r.label,
        "kind": r.kind,
        "identity_quality": r.identity_quality,
        "time_ns": r.self_time_ns,
        "attempts": r.attempts,
        "outputs": r.outputs,
        "amp": amp,
        "uses": r.uses,
        "work": r.work,
        "not_applied": r.not_applied,
        "no_root": r.no_root,
        "surface_mismatch": r.surface_mismatch,
    })
}

/// The totals/attribution line printed under every orientation: row count, row-time sum, and its "N% attributed" check against `run_elapsed_ns`.
struct TotalsSummary {
    /// Every row the filters matched, whether or not `--top` displayed it.
    matched_rows: usize,
    shown_rows: usize,
    total_time_ns: i64,
    /// Per kind, in descending count order: one cross-kind `attempts` sum adds different units.
    attempts_by_kind: Vec<(Option<String>, i64)>,
    total_uses: i64,
    run_elapsed_ns: i64,
}

impl TotalsSummary {
    /// `rows` must be the full matched set; `shown_rows` only narrows what the table displayed.
    fn from_rows(rows: &[RowView], shown_rows: usize, run_elapsed_ns: i64) -> Self {
        TotalsSummary {
            matched_rows: rows.len(),
            shown_rows,
            total_time_ns: rows.iter().filter_map(|r| r.self_time_ns).sum(),
            attempts_by_kind: attempts_by_kind_desc(rows),
            total_uses: rows.iter().filter_map(|r| r.uses).sum(),
            run_elapsed_ns,
        }
    }

    fn attributed_pct(&self) -> f64 {
        if self.run_elapsed_ns > 0 {
            self.total_time_ns as f64 / self.run_elapsed_ns as f64 * 100.0
        } else {
            0.0
        }
    }

    /// `attempts a  b  c` across kinds, or a bare count when every row shares one kind.
    fn attempts_text(&self) -> String {
        match self.attempts_by_kind.as_slice() {
            [] => "-".to_string(),
            [(_, n)] => n.to_string(),
            many => many
                .iter()
                .map(|(kind, n)| format!("{} {n}", kind.as_deref().unwrap_or("-")))
                .collect::<Vec<_>>()
                .join("  "),
        }
    }

    /// Named so a truncated table can never read as the whole of what matched.
    fn shown_note(&self) -> String {
        if self.shown_rows < self.matched_rows {
            format!(" ({} shown)", self.shown_rows)
        } else {
            String::new()
        }
    }

    fn text_line(&self) -> String {
        format!(
            "TOTAL  {} row(s){}   time {:.3}ms ({:.1}% attributed of {:.3}ms recorded)   attempts {}   uses {}\n",
            self.matched_rows,
            self.shown_note(),
            self.total_time_ns as f64 / 1e6,
            self.attributed_pct(),
            self.run_elapsed_ns as f64 / 1e6,
            self.attempts_text(),
            self.total_uses,
        )
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "rows": self.matched_rows,
            "rows_shown": self.shown_rows,
            "time_ns": self.total_time_ns,
            "attempts_by_kind": self
                .attempts_by_kind
                .iter()
                .map(|(k, n)| {
                    (
                        k.clone().unwrap_or_else(|| "-".to_string()),
                        serde_json::json!(n),
                    )
                })
                .collect::<serde_json::Map<String, serde_json::Value>>(),
            "uses": self.total_uses,
            "run_elapsed_ns": self.run_elapsed_ns,
            "attributed_pct": self.attributed_pct(),
        })
    }
}

fn sort_key_name(s: pg_stats::SortKey) -> &'static str {
    match s {
        pg_stats::SortKey::SelfTimeNs => "time",
        pg_stats::SortKey::NoRoot => "no-root",
        pg_stats::SortKey::Amp => "amp",
        pg_stats::SortKey::Uses => "uses",
        pg_stats::SortKey::Attempts => "attempts",
    }
}

fn filters_json(f: &Filters) -> serde_json::Value {
    serde_json::json!({
        "kind": f.kind,
        "object": f.object_key,
        "stratum": f.stratum_key,
        "direction": f.direction,
        "word": f.word,
        "top": f.top_n,
        "sort": f.sort.map(sort_key_name),
        "exclude_censored": f.exclude_censored,
        "by_kind": f.by_kind,
    })
}

fn run_identity(conn: &rusqlite::Connection) -> Result<(String, String), String> {
    conn.query_row(
        "SELECT grammar_hash, engine FROM run ORDER BY run_id DESC LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|e| e.to_string())
}

fn jsonl_meta_value(
    conn: &rusqlite::Connection,
    orientation: &str,
    filters: &Filters,
    totals: &TotalsSummary,
) -> Result<serde_json::Value, String> {
    let (grammar_hash, engine) = run_identity(conn)?;
    Ok(serde_json::json!({
        "meta": true,
        "orientation": orientation,
        "grammar_hash": grammar_hash,
        "engine": engine,
        "filters": filters_json(filters),
        "totals": totals.to_json(),
    }))
}

fn jsonl_meta_line(
    conn: &rusqlite::Connection,
    orientation: &str,
    filters: &Filters,
    totals: &TotalsSummary,
) -> Result<String, String> {
    let meta = jsonl_meta_value(conn, orientation, filters, totals)?;
    serde_json::to_string(&meta).map_err(|e| e.to_string())
}

/// An empty result still has to be machine-readable: prose on stdout would break a strict JSONL parser.
fn empty_output(
    conn: &rusqlite::Connection,
    orientation: &str,
    filters: &Filters,
    reason: String,
    format: OutputFormat,
) -> Result<String, String> {
    match format {
        OutputFormat::Text => Ok(reason),
        OutputFormat::Jsonl => {
            let run_elapsed_ns = pg_stats::word_elapsed_ns_total(conn, filters.word.as_deref())
                .map_err(|e| e.to_string())?;
            let totals = TotalsSummary::from_rows(&[], 0, run_elapsed_ns);
            let mut meta = jsonl_meta_value(conn, orientation, filters, &totals)?;
            meta["empty_reason"] = serde_json::Value::String(reason.trim().to_string());
            Ok(format!(
                "{}\n",
                serde_json::to_string(&meta).map_err(|e| e.to_string())?
            ))
        }
    }
}

/// Distinguishes "never recorded in this cache" from "exists but matched nothing" for an empty result.
fn empty_explanation(
    conn: &rusqlite::Connection,
    kind_scope: Option<&str>,
) -> Result<String, String> {
    let exists = match kind_scope {
        Some(k) => pg_stats::kind_has_any_recorded_object(conn, k).map_err(|e| e.to_string())?,
        None => {
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM object", [], |row| row.get(0))
                .map_err(|e| e.to_string())?;
            n > 0
        }
    };
    let scope_desc = kind_scope.map(|k| format!("{k} ")).unwrap_or_default();
    Ok(if exists {
        format!(
            "stats: {scope_desc}objects exist in this cache, but none match this filter (0 rows \
             recorded) -- try loosening --word/--direction/--object/--stratum\n"
        )
    } else {
        format!(
            "stats: no {scope_desc}objects have ever been recorded in this cache -- this grammar \
             may have none of this kind, or they have never fired\n"
        )
    })
}

/// Per-kind so one crowded kind hides no other, and after the totals rather than in SQL, since a LIMIT would narrow the denominators too.
fn truncate_per_kind(rows: Vec<RowView>, top_n: Option<usize>) -> Vec<RowView> {
    let Some(n) = top_n else { return rows };
    let mut taken: HashMap<Option<String>, usize> = HashMap::new();
    rows.into_iter()
        .filter(|r| {
            let count = taken.entry(r.kind.clone()).or_insert(0);
            *count += 1;
            *count <= n
        })
        .collect()
}

fn render_rowview_body(
    conn: &rusqlite::Connection,
    orientation: &str,
    filters: &Filters,
    rows: Vec<RowView>,
    kind_scope: Option<&str>,
    format: OutputFormat,
) -> Result<String, String> {
    if rows.is_empty() {
        let reason = empty_explanation(conn, kind_scope)?;
        return empty_output(conn, orientation, filters, reason, format);
    }
    let run_elapsed_ns = pg_stats::word_elapsed_ns_total(conn, filters.word.as_deref())
        .map_err(|e| e.to_string())?;
    let shown = truncate_per_kind(rows.clone(), filters.top_n);
    let totals = TotalsSummary::from_rows(&rows, shown.len(), run_elapsed_ns);
    let denoms = Denominators::of(&rows);
    match format {
        OutputFormat::Text if filters.by_kind => {
            let mut out = String::new();
            for kind in kinds_in_row_order(&shown) {
                let section: Vec<RowView> =
                    shown.iter().filter(|r| r.kind == kind).cloned().collect();
                let (headers, table_rows) = render_narrow(&section, filters.wide, &denoms);
                out.push_str(&format!("== {} ==\n", kind.as_deref().unwrap_or("-")));
                out.push_str(&render_table(&headers, &table_rows));
                out.push_str(&subtotal_line(&section, &kind, &denoms));
                out.push('\n');
            }
            out.push_str(&totals.text_line());
            Ok(out)
        }
        OutputFormat::Text => {
            let (headers, table_rows) = render_narrow(&shown, filters.wide, &denoms);
            let mut out = render_table(&headers, &table_rows);
            out.push_str(&totals.text_line());
            Ok(out)
        }
        OutputFormat::Jsonl => {
            let mut lines = vec![jsonl_meta_line(conn, orientation, filters, &totals)?];
            for r in &shown {
                lines.push(serde_json::to_string(&row_view_to_json(r)).map_err(|e| e.to_string())?);
            }
            if filters.by_kind {
                for kind in kinds_in_row_order(&shown) {
                    let section: Vec<RowView> =
                        shown.iter().filter(|r| r.kind == kind).cloned().collect();
                    lines.push(
                        serde_json::to_string(&subtotal_json(&section, &kind, &denoms))
                            .map_err(|e| e.to_string())?,
                    );
                }
            }
            lines.push(String::new());
            Ok(lines.join("\n"))
        }
    }
}

/// The percentage is the group's share of the run's total time, never a within-group 100% that would say nothing about what this kind cost.
fn subtotal_line(section: &[RowView], kind: &Option<String>, denoms: &Denominators) -> String {
    let time: i64 = section.iter().filter_map(|r| r.self_time_ns).sum();
    let attempts: i64 = section.iter().filter_map(|r| r.attempts).sum();
    format!(
        "SUBTOTAL {}  {} row(s)  time {:.3}ms ({} of total time)  attempts {}\n",
        kind.as_deref().unwrap_or("-"),
        section.len(),
        time as f64 / 1e6,
        fmt_pct(Some(time), denoms.total_time_ns),
        attempts,
    )
}

fn subtotal_json(
    section: &[RowView],
    kind: &Option<String>,
    denoms: &Denominators,
) -> serde_json::Value {
    let time: i64 = section.iter().filter_map(|r| r.self_time_ns).sum();
    let attempts: i64 = section.iter().filter_map(|r| r.attempts).sum();
    serde_json::json!({
        "subtotal": true,
        "kind": kind,
        "rows": section.len(),
        "time_ns": time,
        "share_of_total_time_pct": if denoms.total_time_ns > 0 {
            Some(time as f64 / denoms.total_time_ns as f64 * 100.0)
        } else {
            None
        },
        "attempts": attempts,
    })
}

fn render_object(
    conn: &rusqlite::Connection,
    coverage: &CoverageMap,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows = pg_stats::per_object_report(conn, &per_object_filter(filters))
        .map_err(|e| e.to_string())?;
    let views: Vec<RowView> = rows.iter().map(|r| object_row_view(r, coverage)).collect();
    render_rowview_body(
        conn,
        "object",
        filters,
        views,
        filters.kind.as_deref(),
        format,
    )
}

fn render_allomorph(
    conn: &rusqlite::Connection,
    coverage: &CoverageMap,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows = pg_stats::per_allomorph_report(conn, &per_allomorph_filter(filters))
        .map_err(|e| e.to_string())?;
    let views: Vec<RowView> = rows
        .iter()
        .map(|r| allomorph_row_view(r, coverage))
        .collect();
    render_rowview_body(
        conn,
        "allomorph",
        filters,
        views,
        filters.kind.as_deref(),
        format,
    )
}

fn render_stratum(
    conn: &rusqlite::Connection,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows = pg_stats::per_stratum_report(conn, &per_stratum_filter(filters))
        .map_err(|e| e.to_string())?;
    let views: Vec<RowView> = rows.iter().map(stratum_row_view).collect();
    render_rowview_body(
        conn,
        "stratum",
        filters,
        views,
        filters.kind.as_deref(),
        format,
    )
}

fn render_direction(
    conn: &rusqlite::Connection,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows = pg_stats::per_direction_report(conn, &per_direction_filter(filters))
        .map_err(|e| e.to_string())?;
    let views: Vec<RowView> = rows.iter().map(direction_row_view).collect();
    render_rowview_body(
        conn,
        "direction",
        filters,
        views,
        filters.kind.as_deref(),
        format,
    )
}

fn render_morpheme(
    conn: &rusqlite::Connection,
    coverage: &CoverageMap,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows = pg_stats::per_morpheme_report(conn, &per_morpheme_filter(filters))
        .map_err(|e| e.to_string())?;
    let views: Vec<RowView> = rows
        .iter()
        .map(|r| morpheme_row_view(r, coverage))
        .collect();
    render_rowview_body(conn, "morpheme", filters, views, Some("lex_entry"), format)
}

fn render_group(
    conn: &rusqlite::Connection,
    coverage: &CoverageMap,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows =
        pg_stats::per_kind_report(conn, &per_kind_filter(filters)).map_err(|e| e.to_string())?;
    let views: Vec<RowView> = rows.iter().map(|r| kind_row_view(r, coverage)).collect();
    render_rowview_body(
        conn,
        "group",
        filters,
        views,
        filters.kind.as_deref(),
        format,
    )
}

fn render_word(
    conn: &rusqlite::Connection,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let mut rows = pg_stats::per_word_report(conn).map_err(|e| e.to_string())?;
    if let Some(w) = filters.word.as_deref() {
        rows.retain(|r| r.form == w);
    }
    if rows.is_empty() {
        let reason = match filters.word.as_deref() {
            Some(w) => format!("stats: no word named {w} found in this cache\n"),
            None => "stats: cache has no recorded words\n".to_string(),
        };
        return empty_output(conn, "word", filters, reason, format);
    }
    let total_elapsed: i64 = rows.iter().map(|r| r.elapsed_ns).sum();
    let total_attempts: i64 = rows.iter().map(|r| r.attempts).sum();
    match format {
        OutputFormat::Text => {
            let headers = [
                "form",
                "time_ms",
                "time%",
                "attempts",
                "attempts%",
                "passes",
                "capped",
                "timed_out",
            ];
            let table_rows: Vec<Vec<String>> = rows
                .iter()
                .map(|r| {
                    vec![
                        r.form.clone(),
                        format!("{:.3}", r.elapsed_ns as f64 / 1e6),
                        fmt_pct(Some(r.elapsed_ns), total_elapsed),
                        r.attempts.to_string(),
                        fmt_pct(Some(r.attempts), total_attempts),
                        r.passes.to_string(),
                        r.capped.to_string(),
                        r.timed_out.to_string(),
                    ]
                })
                .collect();
            let mut out = render_table(&headers, &table_rows);
            out.push_str(&format!(
                "TOTAL  {} word(s)   time {:.3}ms (100.0% attributed; word rows ARE the recorded \
                 total)   attempts {}\n",
                rows.len(),
                total_elapsed as f64 / 1e6,
                total_attempts,
            ));
            Ok(out)
        }
        OutputFormat::Jsonl => {
            let (grammar_hash, engine) = run_identity(conn)?;
            let meta = serde_json::json!({
                "meta": true,
                "orientation": "word",
                "grammar_hash": grammar_hash,
                "engine": engine,
                "filters": filters_json(filters),
                "totals": {"rows": rows.len(), "time_ns": total_elapsed, "attempts": total_attempts},
            });
            let mut lines = vec![serde_json::to_string(&meta).map_err(|e| e.to_string())?];
            for r in &rows {
                let v = serde_json::json!({
                    "form": r.form,
                    "elapsed_ns": r.elapsed_ns,
                    "attempts": r.attempts,
                    "passes": r.passes,
                    "capped": r.capped,
                    "timed_out": r.timed_out,
                });
                lines.push(serde_json::to_string(&v).map_err(|e| e.to_string())?);
            }
            lines.push(String::new());
            Ok(lines.join("\n"))
        }
    }
}

fn render_never_fires(
    conn: &rusqlite::Connection,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows = pg_stats::never_fires_report(conn, &never_fires_filter(filters))
        .map_err(|e| e.to_string())?;
    if rows.is_empty() {
        let reason = empty_explanation(conn, filters.kind.as_deref())?;
        return empty_output(conn, "never-fires", filters, reason, format);
    }
    let total_attempts: i64 = rows.iter().map(|r| r.attempts).sum();
    match format {
        OutputFormat::Text => {
            let headers = [
                "kind",
                "label",
                "identity_quality",
                "direction",
                "attempts",
                "attempts%",
            ];
            let table_rows: Vec<Vec<String>> = rows
                .iter()
                .map(|r| {
                    vec![
                        r.kind.clone(),
                        r.label.clone(),
                        r.identity_quality.clone(),
                        r.direction.clone(),
                        r.attempts.to_string(),
                        fmt_pct(Some(r.attempts), total_attempts),
                    ]
                })
                .collect();
            let mut out = format!(
                "# never-fires: attempted >= {} time(s) with zero outputs\n",
                pg_stats::NEVER_FIRES_DEFAULT_MIN_ATTEMPTS
            );
            out.push_str(&render_table(&headers, &table_rows));
            out.push_str(&format!(
                "TOTAL  {} object(s) never fired   attempts wasted {}\n",
                rows.len(),
                total_attempts
            ));
            Ok(out)
        }
        OutputFormat::Jsonl => {
            let (grammar_hash, engine) = run_identity(conn)?;
            let meta = serde_json::json!({
                "meta": true,
                "orientation": "never-fires",
                "grammar_hash": grammar_hash,
                "engine": engine,
                "filters": filters_json(filters),
                "totals": {"rows": rows.len(), "attempts": total_attempts},
            });
            let mut lines = vec![serde_json::to_string(&meta).map_err(|e| e.to_string())?];
            for r in &rows {
                let v = serde_json::json!({
                    "kind": r.kind,
                    "label": r.label,
                    "identity_quality": r.identity_quality,
                    "direction": r.direction,
                    "attempts": r.attempts,
                });
                lines.push(serde_json::to_string(&v).map_err(|e| e.to_string())?);
            }
            lines.push(String::new());
            Ok(lines.join("\n"))
        }
    }
}

fn render_default(conn: &rusqlite::Connection, coverage: &CoverageMap) -> Result<String, String> {
    let default_filters = Filters::default();
    let mut out = render_word(conn, &default_filters, OutputFormat::Text)?;
    out.push('\n');
    out.push_str(&render_object(
        conn,
        coverage,
        &default_filters,
        OutputFormat::Text,
    )?);

    let never_fires_rows =
        pg_stats::never_fires_report(conn, &pg_stats::NeverFiresFilter::default())
            .map_err(|e| e.to_string())?;
    if !never_fires_rows.is_empty() {
        out.push('\n');
        out.push_str(&render_never_fires(
            conn,
            &default_filters,
            OutputFormat::Text,
        )?);
    }
    Ok(out)
}

/// `stats <project-or-grammar> [options]`: reads the cache via a raw `rusqlite::Connection`, never `StatsCache::open`, so a report-only read can never trip the grammar-hash wipe gate.
pub(crate) fn run_stats(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut group: Option<String> = None;
    let mut filters = Filters::default();
    let mut sort_arg: Option<String> = None;
    let mut format_arg: Option<String> = None;
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
            "--word" => filters.word = Some(it.next().ok_or("--word requires a value")?.clone()),
            s if s.starts_with("--word=") => filters.word = Some(s["--word=".len()..].to_string()),
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
            "--by-kind" => filters.by_kind = true,
            "--wide" => filters.wide = true,
            "--format" => format_arg = Some(it.next().ok_or("--format requires a value")?.clone()),
            s if s.starts_with("--format=") => {
                format_arg = Some(s["--format=".len()..].to_string())
            }
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
        Some("morpheme") => Some(ReportGroup::Morpheme),
        Some("group") => Some(ReportGroup::Group),
        Some("never-fires") => Some(ReportGroup::NeverFires),
        Some(other) => {
            return Err(format!(
                "invalid --group: {other} (expected word|object|allomorph|stratum|direction|morpheme|group|never-fires)"
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
        Some("amp") => Some(pg_stats::SortKey::Amp),
        Some("uses") => Some(pg_stats::SortKey::Uses),
        Some("attempts") => Some(pg_stats::SortKey::Attempts),
        Some(other) => {
            return Err(format!(
                "invalid --sort: {other} (expected time|no-root|amp|uses|attempts)"
            ))
        }
    };
    if filters.sort.is_some() && !matches!(group, None | Some(ReportGroup::Object)) {
        return Err("--sort only applies to --group object (or the default view)".to_string());
    }
    if filters.stratum_key.is_some() && !matches!(group, None | Some(ReportGroup::Object)) {
        return Err("--stratum only applies to --group object (or the default view)".to_string());
    }
    if filters.object_key.is_some()
        && matches!(
            group,
            Some(ReportGroup::Morpheme)
                | Some(ReportGroup::Group)
                | Some(ReportGroup::Word)
                | Some(ReportGroup::NeverFires)
        )
    {
        return Err("--object does not apply to this --group".to_string());
    }
    if filters.kind.is_some() && matches!(group, Some(ReportGroup::Morpheme)) {
        return Err(
            "--kind does not apply to --group morpheme: always scoped to lex_entry".to_string(),
        );
    }
    if filters.by_kind
        && !matches!(
            group,
            Some(ReportGroup::Object) | Some(ReportGroup::Allomorph)
        )
    {
        return Err(
            "--by-kind applies to --group object and --group allomorph only: every other orientation is either one row per kind already or carries no kind at all"
                .to_string(),
        );
    }
    if matches!(group, Some(ReportGroup::NeverFires))
        && filters
            .kind
            .as_deref()
            .is_some_and(|k| k != "morph_rule" && k != "phon_rule")
    {
        return Err(
            "--group never-fires covers morph_rule and phon_rule only: no other kind wires an `outputs` counter, so \"zero outputs\" would be an artifact there"
                .to_string(),
        );
    }

    let format = match format_arg.as_deref() {
        None | Some("text") => OutputFormat::Text,
        Some("jsonl") => OutputFormat::Jsonl,
        Some(other) => return Err(format!("invalid --format: {other} (expected text|jsonl)")),
    };
    if out_path.is_some() && group.is_none() {
        return Err(
            "--out requires --group (a single file holds one report shape; the default view renders several)"
                .to_string(),
        );
    }
    if matches!(format, OutputFormat::Jsonl) && group.is_none() {
        return Err(
            "--format jsonl requires --group (the default view has no single row shape)"
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
        eprintln!(
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
        eprintln!(
            "warning: this cache spans more than one engine; per-object counters mix measured and unsupported rows"
        );
    }

    let coverage = coverage_map_for_latest_run(&conn)?;

    let body = match group {
        None => render_default(&conn, &coverage)?,
        Some(ReportGroup::Word) => render_word(&conn, &filters, format)?,
        Some(ReportGroup::Object) => render_object(&conn, &coverage, &filters, format)?,
        Some(ReportGroup::Allomorph) => render_allomorph(&conn, &coverage, &filters, format)?,
        Some(ReportGroup::Stratum) => render_stratum(&conn, &filters, format)?,
        Some(ReportGroup::Direction) => render_direction(&conn, &filters, format)?,
        Some(ReportGroup::Morpheme) => render_morpheme(&conn, &coverage, &filters, format)?,
        Some(ReportGroup::Group) => render_group(&conn, &coverage, &filters, format)?,
        Some(ReportGroup::NeverFires) => render_never_fires(&conn, &filters, format)?,
    };
    match out_path {
        Some(path) => std::fs::write(&path, body).map_err(|e| format!("write {path}: {e}")),
        None => {
            print!("{body}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {

    /// Sums the displayed rows for its own denominators -- correct only where the caller shows every matched row.
    fn render_narrow_self_totals(
        rows: &[RowView],
        wide: bool,
    ) -> (Vec<&'static str>, Vec<Vec<String>>) {
        render_narrow(rows, wide, &Denominators::of(rows))
    }
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
        let view = object_row_view(&row, &coverage);
        let (headers, table_rows) = render_narrow_self_totals(std::slice::from_ref(&view), true);
        let no_root_col = headers.iter().position(|h| *h == "no_root").unwrap();
        assert_eq!(table_rows[0][no_root_col], "-");

        let measured = coverage_entry(pg_stats::CoverageState::Measured);
        let view_measured = object_row_view(&row, &measured);
        let (_, table_rows_measured) =
            render_narrow_self_totals(std::slice::from_ref(&view_measured), true);
        assert_eq!(
            table_rows_measured[0][no_root_col], "0",
            "falsifiability check: a measured zero must still render as a plain 0"
        );
    }

    #[test]
    fn absent_coverage_row_renders_em_dash_not_a_number() {
        // No entry at all for (morph_rule, no_root) -- distinct from an explicit "unsupported" row.
        let coverage = CoverageMap::new();
        let row = sample_object_row(7);
        let view = object_row_view(&row, &coverage);
        let (headers, table_rows) = render_narrow_self_totals(std::slice::from_ref(&view), true);
        let no_root_col = headers.iter().position(|h| *h == "no_root").unwrap();
        assert_eq!(
            table_rows[0][no_root_col], "-",
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
        let view = object_row_view(morph_row, &coverage);
        let (headers, table_rows) = render_narrow_self_totals(std::slice::from_ref(&view), false);
        let attempts_col = headers.iter().position(|h| *h == "attempts").unwrap();
        assert_eq!(
            table_rows[0][attempts_col], "-",
            "a deleted coverage row must render as unmeasured, never as a bare number"
        );
    }

    #[test]
    fn wide_appends_extra_columns_and_is_absent_by_default() {
        let row = sample_object_row(0);
        let coverage = coverage_entry(pg_stats::CoverageState::Measured);
        let view = object_row_view(&row, &coverage);

        let (narrow_headers, narrow_rows) =
            render_narrow_self_totals(std::slice::from_ref(&view), false);
        assert!(!narrow_headers.contains(&"work"));
        assert!(!narrow_headers.contains(&"identity_quality"));
        assert_eq!(narrow_rows[0].len(), narrow_headers.len());

        let (wide_headers, wide_rows) =
            render_narrow_self_totals(std::slice::from_ref(&view), true);
        assert!(wide_headers.contains(&"work"));
        assert!(wide_headers.contains(&"not_applied"));
        assert!(wide_headers.contains(&"no_root"));
        assert!(wide_headers.contains(&"surface_mismatch"));
        assert!(wide_headers.contains(&"identity_quality"));
        assert_eq!(wide_rows[0].len(), wide_headers.len());
    }

    #[test]
    fn shares_sum_to_100_pct_per_orientation() {
        let (grammar_xml, word) = primary_fixture();
        let dir = scratch_dir("shares");
        let cache_path = dir.join("cache.sqlite3");
        let (args, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &format!("{word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args).expect("seed the cache via batch --stats");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let coverage = coverage_map_for_latest_run(&conn).unwrap();
        let rows =
            pg_stats::per_object_report(&conn, &pg_stats::PerObjectFilter::default()).unwrap();
        assert!(!rows.is_empty(), "sanity: the fixture must produce rows");
        let views: Vec<RowView> = rows.iter().map(|r| object_row_view(r, &coverage)).collect();
        let (headers, table_rows) = render_narrow_self_totals(&views, false);

        let time_col = headers.iter().position(|h| *h == "time%").unwrap();
        let attempts_col = headers.iter().position(|h| *h == "attempts%").unwrap();
        let pct_of = |row: &[String], col: usize| -> Option<f64> {
            row[col].trim_end_matches('%').parse::<f64>().ok()
        };

        // Self time means the same thing whatever kind produced it, so its shares span the report.
        let time_sum: f64 = table_rows.iter().filter_map(|r| pct_of(r, time_col)).sum();
        assert!(
            (99.0..=101.0).contains(&time_sum),
            "time% must sum to ~100% with no --top narrowing: got {time_sum}"
        );

        // `attempts` does not: each kind counts a different event, so shares close within a kind.
        let mut by_kind: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        for (view, row) in views.iter().zip(table_rows.iter()) {
            if let Some(p) = pct_of(row, attempts_col) {
                *by_kind
                    .entry(view.kind.clone().unwrap_or_else(|| "-".to_string()))
                    .or_insert(0.0) += p;
            }
        }
        assert!(
            by_kind.len() > 1,
            "sanity: this fixture must span several kinds, or the per-kind claim is untested"
        );
        for (kind, sum) in &by_kind {
            assert!(
                (99.0..=101.0).contains(sum),
                "attempts% must sum to ~100% within {kind}, not across kinds: got {sum}"
            );
        }
    }

    #[test]
    fn totals_line_attribution_matches_hand_summed_rows() {
        let (grammar_xml, word) = primary_fixture();
        let dir = scratch_dir("attribution");
        let cache_path = dir.join("cache.sqlite3");
        let (args, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &format!("{word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args).expect("seed the cache via batch --stats");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let coverage = coverage_map_for_latest_run(&conn).unwrap();
        let body =
            render_object(&conn, &coverage, &Filters::default(), OutputFormat::Text).unwrap();
        let total_line = body
            .lines()
            .find(|l| l.starts_with("TOTAL"))
            .expect("a TOTAL line must be printed");

        let rows =
            pg_stats::per_object_report(&conn, &pg_stats::PerObjectFilter::default()).unwrap();
        let hand_summed_time_ns: i64 = rows.iter().map(|r| r.self_time_ns).sum();
        let run_elapsed_ns = pg_stats::word_elapsed_ns_total(&conn, None).unwrap();
        let expected_pct = if run_elapsed_ns > 0 {
            hand_summed_time_ns as f64 / run_elapsed_ns as f64 * 100.0
        } else {
            0.0
        };
        assert!(
            total_line.contains(&format!("{:.3}ms", hand_summed_time_ns as f64 / 1e6)),
            "TOTAL line must report the exact hand-summed row time: {total_line}"
        );
        assert!(
            total_line.contains(&format!("{expected_pct:.1}% attributed")),
            "TOTAL line's attribution percentage must match summing rows by hand: {total_line}"
        );
    }

    #[test]
    fn word_filter_narrows_and_absent_word_explains_itself() {
        let (grammar_xml, word) = primary_fixture();
        let dir = scratch_dir("word-filter");
        let cache_path = dir.join("cache.sqlite3");
        let (args, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &format!("{word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args).expect("seed the cache via batch --stats");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let coverage = coverage_map_for_latest_run(&conn).unwrap();

        let matching = Filters {
            word: Some(word.clone()),
            ..Filters::default()
        };
        let body = render_object(&conn, &coverage, &matching, OutputFormat::Text).unwrap();
        assert!(
            body.contains("TOTAL"),
            "the word that was actually analyzed must still produce rows: {body}"
        );

        let absent = Filters {
            word: Some("this-word-was-never-analyzed-xyz".to_string()),
            ..Filters::default()
        };
        let empty_body = render_object(&conn, &coverage, &absent, OutputFormat::Text).unwrap();
        assert!(
            empty_body.contains("objects exist in this cache, but none match this filter"),
            "a real kind narrowed to an absent word must explain the empty result: {empty_body}"
        );
    }

    #[test]
    fn top_n_is_scoped_per_kind_and_never_narrows_the_totals() {
        let (grammar_xml, word) = primary_fixture();
        let dir = scratch_dir("top-per-kind");
        let cache_path = dir.join("cache.sqlite3");
        let (args, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &format!("{word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args).expect("seed the cache via batch --stats");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let coverage = coverage_map_for_latest_run(&conn).unwrap();
        let all_rows =
            pg_stats::per_object_report(&conn, &pg_stats::PerObjectFilter::default()).unwrap();
        let kinds: std::collections::HashSet<_> = all_rows.iter().map(|r| r.kind.clone()).collect();
        assert!(
            all_rows.len() > kinds.len(),
            "this fixture must record more objects than kinds, or --top drops nothing to test"
        );

        let untopped =
            render_object(&conn, &coverage, &Filters::default(), OutputFormat::Text).unwrap();
        let topped = render_object(
            &conn,
            &coverage,
            &Filters {
                top_n: Some(1),
                ..Filters::default()
            },
            OutputFormat::Text,
        )
        .unwrap();

        for kind in &kinds {
            let shown = topped
                .lines()
                .filter(|l| l.starts_with(&format!("{kind}: ")))
                .count();
            assert_eq!(
                shown, 1,
                "--top 1 must keep exactly one row of kind {kind}, never collapse to a single \
                 global winner: {topped}"
            );
        }

        let total_time_field = |body: &str| {
            body.lines()
                .find(|l| l.starts_with("TOTAL"))
                .expect("every rendered table carries a TOTAL line")
                .split("   ")
                .find(|f| f.trim_start().starts_with("time "))
                .expect("the TOTAL line carries a time field")
                .trim()
                .to_string()
        };
        assert_eq!(
            total_time_field(&topped),
            total_time_field(&untopped),
            "a --top excerpt must total (and attribute) every matched row, not just shown ones"
        );
        let total_line = topped.lines().find(|l| l.starts_with("TOTAL")).unwrap();
        assert!(
            total_line.contains(&format!(
                "{} row(s) ({} shown)",
                all_rows.len(),
                kinds.len()
            )),
            "a truncated table must say how many of the matched rows it displayed: {total_line}"
        );
    }

    #[test]
    fn a_shown_row_states_its_share_of_every_matched_row_not_of_the_excerpt() {
        let row = RowView {
            label: "r".to_string(),
            kind: Some("morph_rule".to_string()),
            identity_quality: None,
            self_time_ns: Some(1_000),
            attempts: Some(10),
            outputs: Some(0),
            uses: Some(0),
            work: Some(0),
            not_applied: Some(0),
            no_root: Some(0),
            surface_mismatch: Some(0),
        };
        // One row displayed out of a matched set totalling four times its time and attempts.
        let denoms = Denominators {
            total_time_ns: 4_000,
            attempts_by_kind: HashMap::from([(Some("morph_rule".to_string()), 40)]),
        };
        let (headers, rows) = render_narrow(&[row], false, &denoms);
        let time_pct = headers.iter().position(|h| *h == "time%").unwrap();
        let attempts_pct = headers.iter().position(|h| *h == "attempts%").unwrap();
        assert_eq!(
            (rows[0][time_pct].as_str(), rows[0][attempts_pct].as_str()),
            ("25.0%", "25.0%"),
            "a lone displayed row must not read as 100% of the run"
        );
    }

    #[test]
    fn jsonl_first_line_is_meta_and_every_row_parses() {
        let (grammar_xml, word) = primary_fixture();
        let dir = scratch_dir("jsonl");
        let cache_path = dir.join("cache.sqlite3");
        let (args, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &format!("{word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args).expect("seed the cache via batch --stats");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let coverage = coverage_map_for_latest_run(&conn).unwrap();
        let body =
            render_object(&conn, &coverage, &Filters::default(), OutputFormat::Jsonl).unwrap();
        let mut lines = body.lines();

        let meta_line = lines.next().expect("jsonl output must have a meta line");
        let meta: serde_json::Value =
            serde_json::from_str(meta_line).expect("meta line must be valid JSON");
        assert_eq!(meta["meta"], serde_json::Value::Bool(true));
        assert_eq!(meta["orientation"], "object");

        let mut row_count = 0;
        for line in lines {
            let _: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("row line failed to parse: {line}: {e}"));
            row_count += 1;
        }
        assert!(
            row_count > 0,
            "at least one data row must follow the meta line"
        );
    }

    #[test]
    fn empty_result_distinguishes_never_recorded_from_filtered_to_nothing() {
        let dir = scratch_dir("empty-explain");
        let cache_path = dir.join("cache.sqlite3");
        let mut outcome = pg_stats::StatsCache::open(&cache_path, "hash-a").unwrap();
        let run = pg_stats::RunMetadata {
            build_info: "test".to_string(),
            fwdata_path: "x".to_string(),
            grammar_hash: "hash-a".to_string(),
            engine: "hc".to_string(),
            options_hash: "opts".to_string(),
            options_json: "{}".to_string(),
            created_utc: "unix:0".to_string(),
        };
        let word_record = pg_stats::WordRecord {
            form: "onlyword".to_string(),
            elapsed_ns: 1_000,
            attempts: 1,
            passes: 1,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts: vec![pg_stats::FactRecord {
                object_key: "rule-a".to_string(),
                object_kind: pg_stats::ObjectKind::MorphRule,
                object_label: "Rule A".to_string(),
                identity_quality: pg_stats::IdentityQuality::Authored,
                stratum: Some(pg_stats::StructuralLocator::new("0:Root", "Root")),
                allomorph: None,
                morpheme: None,
                direction: pg_stats::Direction::Analysis,
                attempts: 5,
                work: 10,
                outputs: 2,
                not_applied: 1,
                no_root: 0,
                surface_mismatch: 0,
                uses: 1,
                self_time_ns: 100,
            }],
        };
        outcome.cache.flush(&run, &[word_record]).unwrap();

        let conn = outcome.cache.connection();
        let coverage = coverage_map_for_latest_run(conn).unwrap();

        let never_recorded = Filters {
            kind: Some("phon_rule".to_string()),
            ..Filters::default()
        };
        let body_a = render_object(conn, &coverage, &never_recorded, OutputFormat::Text).unwrap();
        assert!(
            body_a.contains("no phon_rule objects have ever been recorded"),
            "must explain that this kind never occurred at all: {body_a}"
        );

        let filtered_to_nothing = Filters {
            kind: Some("morph_rule".to_string()),
            word: Some("a-word-never-analyzed".to_string()),
            ..Filters::default()
        };
        let body_b =
            render_object(conn, &coverage, &filtered_to_nothing, OutputFormat::Text).unwrap();
        assert!(
            body_b.contains("morph_rule objects exist in this cache, but none match this filter"),
            "must explain that this kind exists but the filter matched nothing: {body_b}"
        );
    }

    #[test]
    fn group_orientation_reports_one_row_per_kind() {
        let (grammar_xml, word) = primary_fixture();
        let dir = scratch_dir("group-orientation");
        let cache_path = dir.join("cache.sqlite3");
        let (args, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &format!("{word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&args).expect("seed the cache via batch --stats");

        let conn = rusqlite::Connection::open(&cache_path).unwrap();
        let coverage = coverage_map_for_latest_run(&conn).unwrap();
        let body = render_group(&conn, &coverage, &Filters::default(), OutputFormat::Text).unwrap();
        assert!(
            body.contains("morph_rule"),
            "group orientation must surface the morph_rule kind: {body}"
        );
    }

    #[test]
    fn morpheme_orientation_collapses_scattered_entries_through_the_cli() {
        let dir = scratch_dir("morpheme-cli");
        let cache_path = dir.join("cache.sqlite3");
        let mut outcome = pg_stats::StatsCache::open(&cache_path, "hash-a").unwrap();
        let run = pg_stats::RunMetadata {
            build_info: "test".to_string(),
            fwdata_path: "x".to_string(),
            grammar_hash: "hash-a".to_string(),
            engine: "hc".to_string(),
            options_hash: "opts".to_string(),
            options_json: "{}".to_string(),
            created_utc: "unix:0".to_string(),
        };
        let make_fact = |key: &str, morpheme_key: &str, attempts: u64| pg_stats::FactRecord {
            object_key: key.to_string(),
            object_kind: pg_stats::ObjectKind::LexEntry,
            object_label: key.to_string(),
            identity_quality: pg_stats::IdentityQuality::Authored,
            stratum: Some(pg_stats::StructuralLocator::new("0:Root", "Root")),
            allomorph: None,
            morpheme: Some(pg_stats::StructuralLocator::new(morpheme_key, morpheme_key)),
            direction: pg_stats::Direction::Analysis,
            attempts,
            work: attempts,
            outputs: attempts,
            not_applied: 0,
            no_root: 0,
            surface_mismatch: 0,
            uses: 0,
            self_time_ns: attempts * 10,
        };
        let word_record = pg_stats::WordRecord {
            form: "w".to_string(),
            elapsed_ns: 1_000,
            attempts: 1,
            passes: 1,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts: vec![
                make_fact("entry-1", "cat-morph", 3),
                make_fact("entry-2", "cat-morph", 2),
            ],
        };
        outcome.cache.flush(&run, &[word_record]).unwrap();

        let conn = outcome.cache.connection();
        let coverage = coverage_map_for_latest_run(conn).unwrap();
        let body =
            render_morpheme(conn, &coverage, &Filters::default(), OutputFormat::Text).unwrap();
        let data_rows = body.lines().filter(|l| l.contains("cat-morph")).count();
        assert_eq!(
            data_rows, 1,
            "two entries sharing a morpheme must collapse to one row: {body}"
        );
    }

    #[test]
    fn run_stats_end_to_end_group_object_writes_jsonl_and_default_view_succeeds() {
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
        let jsonl_path = dir.join("object.jsonl");
        let stats_args: Vec<String> = vec![
            grammar_path.to_string_lossy().into_owned(),
            "--group".to_string(),
            "object".to_string(),
            "--cache".to_string(),
            cache_path.to_string_lossy().into_owned(),
            "--format".to_string(),
            "jsonl".to_string(),
            "--out".to_string(),
            jsonl_path.to_string_lossy().into_owned(),
        ];
        run_stats(&stats_args).expect("run_stats --group object --format jsonl --out must succeed");
        let jsonl_text = fs::read_to_string(&jsonl_path).unwrap();
        let mut lines = jsonl_text.lines();
        let meta: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(meta["meta"], serde_json::Value::Bool(true));
        let mut n = 0;
        for line in lines {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
            n += 1;
        }
        assert!(n > 0);

        let default_args: Vec<String> = vec![
            grammar_path.to_string_lossy().into_owned(),
            "--cache".to_string(),
            cache_path.to_string_lossy().into_owned(),
        ];
        run_stats(&default_args).expect("run_stats with no --group must render the default view");
    }

    #[test]
    fn jsonl_format_requires_group() {
        let (grammar_xml, word) = primary_fixture();
        let dir = scratch_dir("jsonl-requires-group");
        let cache_path = dir.join("cache.sqlite3");
        let (batch_args, _) = run_batch_args(
            &dir,
            &grammar_xml,
            &format!("{word}\n"),
            &["--stats", "--cache", cache_path.to_str().unwrap()],
        );
        crate::run_batch(&batch_args).expect("seed the cache via batch --stats");

        let grammar_path = dir.join("grammar.xml");
        let args: Vec<String> = vec![
            grammar_path.to_string_lossy().into_owned(),
            "--cache".to_string(),
            cache_path.to_string_lossy().into_owned(),
            "--format".to_string(),
            "jsonl".to_string(),
        ];
        let err = run_stats(&args).expect_err("--format jsonl with no --group must be refused");
        assert!(err.contains("--format jsonl requires --group"));
    }
}
