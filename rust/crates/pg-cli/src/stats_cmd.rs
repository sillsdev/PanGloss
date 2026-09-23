//! The HC `batch --stats` cache-writing path and the `stats` subcommand's cache-reading/reporting side of `pg_stats::StatsCache`, including synthetic Foma-cache report semantics.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use pg_grammar::model::{AllomorphId, Grammar, LexEntryId, MRuleId, PRuleId};
use pg_stats::StepCap;
use serde::Serialize;
use serde_json::Value;

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

pub(crate) fn resolve_cache_path(
    project_path: &str,
    cache_override: Option<&str>,
) -> Result<PathBuf, String> {
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
        pg_rules::stats::ObjectKind::Overlay => pg_grammar::stats_identity::overlay_identity(
            grammar,
            pg_grammar::stats_identity::OverlayPhase::from_index(row.object_index),
        ),
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

fn stats_kind_to_rules_kind(k: pg_stats::ObjectKind) -> pg_rules::stats::ObjectKind {
    use pg_stats::ObjectKind as S;
    match k {
        S::MorphRule => pg_rules::stats::ObjectKind::MorphRule,
        S::PhonRule => pg_rules::stats::ObjectKind::PhonRule,
        S::LexEntry => pg_rules::stats::ObjectKind::LexEntry,
        S::RootIndex => pg_rules::stats::ObjectKind::RootIndex,
        S::Guesser => pg_rules::stats::ObjectKind::Guesser,
        S::Overlay => pg_rules::stats::ObjectKind::Overlay,
    }
}

/// Errors when `cache_path` already holds a different engine's runs, checked before analyzing a word or writing a row -- a report cannot span two engines.
fn refuse_if_cache_engine_differs(
    cache: &pg_stats::StatsCache,
    new_engine: &str,
    cache_path: &std::path::Path,
) -> Result<(), String> {
    let mut stmt = cache
        .connection()
        .prepare("SELECT DISTINCT engine FROM run")
        .map_err(|e| e.to_string())?;
    let existing: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if let Some(other) = existing.iter().find(|e| e.as_str() != new_engine) {
        return Err(format!(
            "stats: cache at {} already holds runs from engine `{other}`; a report cannot span two \
             engines -- point --cache at another path for a `{new_engine}` run",
            cache_path.display()
        ));
    }
    Ok(())
}

fn refuse_if_final_template_policy_differs(
    cache: &pg_stats::StatsCache,
    cache_path: &std::path::Path,
    requested: bool,
) -> Result<(), String> {
    let mut stmt = cache
        .connection()
        .prepare("SELECT options_json FROM run ORDER BY run_id")
        .map_err(|e| e.to_string())?;
    let prior: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for options_json in prior {
        let value = serde_json::from_str::<Value>(&options_json).map_err(|err| {
            format!(
                "stats: cache at {} has malformed prior options JSON ({err})",
                cache_path.display()
            )
        })?;
        let object = value.as_object().ok_or_else(|| {
            format!(
                "stats: cache at {} has prior options JSON that is not an object",
                cache_path.display()
            )
        })?;
        let recorded = match object.get("always_enforce_final_templates") {
            None => false,
            Some(Value::Bool(value)) => *value,
            Some(_) => {
                return Err(format!(
                    "stats: cache at {} has non-boolean always_enforce_final_templates",
                    cache_path.display()
                ));
            }
        };
        if recorded != requested {
            return Err(format!(
                "stats: cache at {} records always_enforce_final_templates={} but requested {}; use a separate cache",
                cache_path.display(), recorded, requested
            ));
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct StatsOptionsRecord {
    engine: &'static str,
    step_cap: Option<StepCap>,
    word_timeout_ms: Option<u64>,
    guess: bool,
    always_enforce_final_templates: bool,
}

fn final_template_stats_line(counters: pg_rules::stats::PruneCounters) -> String {
    format!(
        "FINAL_TEMPLATE_STATS_NEWLY_ANALYZED\ttemplate_entries={}\ttemplate_batteries_skipped={}\tfinal_templates_skipped={}",
        counters.template_entries,
        counters.template_batteries_skipped,
        counters.final_templates_skipped,
    )
}

/// Flushes HC `batch --stats` records in batches and prints the one summary line it promises.
fn finish_stats_flush(
    cache: &mut pg_stats::StatsCache,
    grammar_path: &str,
    grammar_hash: &str,
    options: &StatsOptionsRecord,
    records: Vec<pg_stats::WordRecord>,
    skipped: usize,
    elapsed: Duration,
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
        step_cap: options.step_cap,
    };

    let analyzed = records.len();
    let mut chunks: Vec<&[pg_stats::WordRecord]> = records.chunks(STATS_FLUSH_BATCH).collect();
    if chunks.is_empty() {
        chunks.push(&[]);
    }
    for chunk in chunks {
        cache.flush(&run, chunk).map_err(|e| e.to_string())?;
    }

    println!(
        "stats: analyzed={analyzed} skipped={skipped} summed_word_ms={:.3}",
        elapsed.as_secs_f64() * 1e3
    );
    Ok(())
}

pub(crate) struct BatchStatsWord {
    pub(crate) record: pg_stats::WordRecord,
    pub(crate) prunes: pg_rules::stats::PruneCounters,
}

pub(crate) fn batch_stats_word(
    grammar: &Grammar,
    word: &str,
    outcome: &pg_parse::BatchWordOutcome,
    rows: &[pg_rules::stats::StatsRow],
    prune_rows: &[pg_rules::stats::PruneRow],
) -> BatchStatsWord {
    let mut prunes = pg_rules::stats::PruneCounters::default();
    for row in prune_rows {
        prunes.template_entries += row.counters.template_entries;
        prunes.template_batteries_skipped += row.counters.template_batteries_skipped;
        prunes.final_templates_skipped += row.counters.final_templates_skipped;
    }
    let facts = rows
        .iter()
        .map(|row| fact_record_from_stats_row(grammar, row))
        .collect();
    BatchStatsWord {
        record: pg_stats::WordRecord {
            form: word.to_string(),
            elapsed_ns: outcome.elapsed.as_nanos().min(u128::from(u64::MAX)) as u64,
            attempts: outcome.outcome.steps as u64,
            passes: outcome.outcome.analyses.len() as u64,
            capped: outcome.outcome.capped,
            timed_out: outcome.outcome.timed_out,
            invalid_shape: outcome.outcome.invalid_shape,
            facts,
        },
        prunes,
    }
}

pub(crate) struct BatchStatsCache {
    cache: pg_stats::StatsCache,
    grammar_path: String,
    grammar_hash: String,
    existing_words: HashSet<String>,
    skipped: usize,
    options: StatsOptionsRecord,
}

impl BatchStatsCache {
    pub(crate) fn contains(&self, word: &str) -> bool {
        self.existing_words.contains(word)
    }

    pub(crate) fn existing_words(&self) -> &HashSet<String> {
        &self.existing_words
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_batch_stats_hc(
    grammar_path: &str,
    words: &[String],
    step_cap: StepCap,
    word_timeout_ms: Option<u64>,
    guess: bool,
    always_enforce_final_templates: bool,
    cache_override: Option<&str>,
) -> Result<BatchStatsCache, String> {
    let grammar_hash = grammar_hash_for(grammar_path)?;
    let cache_path = resolve_cache_path(grammar_path, cache_override)?;
    let outcome =
        pg_stats::StatsCache::open(&cache_path, &grammar_hash).map_err(|e| e.to_string())?;
    if outcome.wiped {
        println!(
            "stats: cache wiped (grammar changed): {}",
            cache_path.display()
        );
    }
    refuse_if_cache_engine_differs(&outcome.cache, "hc", &cache_path)?;
    refuse_if_final_template_policy_differs(
        &outcome.cache,
        &cache_path,
        always_enforce_final_templates,
    )?;
    outcome
        .cache
        .refuse_if_step_cap_differs(step_cap)
        .map_err(|e| e.to_string())?;

    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    let existing = outcome
        .cache
        .existing_words(&refs)
        .map_err(|e| e.to_string())?;
    let skipped = words
        .iter()
        .filter(|word| existing.contains(word.as_str()))
        .count();

    Ok(BatchStatsCache {
        cache: outcome.cache,
        grammar_path: grammar_path.to_owned(),
        grammar_hash,
        existing_words: existing,
        skipped,
        options: StatsOptionsRecord {
            engine: "hc",
            step_cap: Some(step_cap),
            word_timeout_ms,
            guess,
            always_enforce_final_templates,
        },
    })
}

pub(crate) fn finish_batch_stats_hc(
    mut prepared: BatchStatsCache,
    stats_words: Vec<BatchStatsWord>,
) -> Result<pg_rules::stats::PruneCounters, String> {
    let mut prune_totals = pg_rules::stats::PruneCounters::default();
    let mut total_elapsed_ns = 0u128;
    let records: Vec<_> = stats_words
        .into_iter()
        .filter(|stats_word| {
            !prepared
                .existing_words
                .contains(stats_word.record.form.as_str())
        })
        .map(|stats_word| {
            prune_totals.template_entries += stats_word.prunes.template_entries;
            prune_totals.template_batteries_skipped += stats_word.prunes.template_batteries_skipped;
            prune_totals.final_templates_skipped += stats_word.prunes.final_templates_skipped;
            total_elapsed_ns += u128::from(stats_word.record.elapsed_ns);
            stats_word.record
        })
        .collect();
    let total_elapsed = Duration::from_nanos(total_elapsed_ns.min(u128::from(u64::MAX)) as u64);
    finish_stats_flush(
        &mut prepared.cache,
        &prepared.grammar_path,
        &prepared.grammar_hash,
        &prepared.options,
        records,
        prepared.skipped,
        total_elapsed,
    )?;
    println!("{}", final_template_stats_line(prune_totals));
    Ok(prune_totals)
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) fn run_batch_stats_hc(
    grammar_path: &str,
    words: &[String],
    stats_words: Vec<BatchStatsWord>,
    step_cap: StepCap,
    word_timeout_ms: Option<u64>,
    guess: bool,
    always_enforce_final_templates: bool,
    cache_override: Option<&str>,
) -> Result<pg_rules::stats::PruneCounters, String> {
    let prepared = prepare_batch_stats_hc(
        grammar_path,
        words,
        step_cap,
        word_timeout_ms,
        guess,
        always_enforce_final_templates,
        cache_override,
    )?;
    finish_batch_stats_hc(prepared, stats_words)
}

// The `stats` subcommand: read-only, no grammar loaded.

const STATS_USAGE: &str = "usage: stats <project-or-grammar> [--group word|object|allomorph|morpheme|group|never-fires] [--kind K] [--object KEY] [--stratum KEY] [--direction analysis|synthesis] [--word FORM] [--top N] [--sort time|no-root|amp|uses|attempts] [--exclude-censored] [--wide] [--by-kind] [--format text|jsonl] [--cache <path>] [--out FILE]";

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportGroup {
    Word,
    Object,
    Allomorph,
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

/// `None` for any per-kind `CounterSupport` other than `Measured`, both of which render `-` here.
fn cell_value(kind: &str, counter: &str, value: i64) -> Option<i64> {
    let stats_kind: pg_stats::ObjectKind = kind.parse().unwrap_or_else(|_| {
        panic!("kind string from the cache must be a known ObjectKind: {kind}")
    });
    match pg_rules::stats::counter_support(stats_kind_to_rules_kind(stats_kind), counter) {
        pg_rules::stats::CounterSupport::Measured => Some(value),
        pg_rules::stats::CounterSupport::NotApplicable
        | pg_rules::stats::CounterSupport::NotWired => None,
    }
}

/// The seven per-object counters, in the fixed display/JSONL order.
const ALL_COUNTERS: [&str; 7] = [
    "attempts",
    "work",
    "outputs",
    "not_applied",
    "no_root",
    "surface_mismatch",
    "uses",
];

/// Counters this cache's engine can never write into a `fact` row at all, uniform across every kind -- unlike `cell_value`'s per-kind masking, this drops the whole column.
fn engine_omitted_counters(engine: &str) -> &'static [&'static str] {
    if engine == "foma" {
        &ALL_COUNTERS
    } else {
        &[]
    }
}

/// The text-mode note explaining an engine-level column omission; `None` when nothing is omitted.
fn omitted_counters(engine: &str, _orientation: &str) -> Vec<&'static str> {
    engine_omitted_counters(engine).to_vec()
}

fn engine_omission_note(engine: &str, omitted: &[&str]) -> Option<String> {
    if omitted.is_empty() {
        return None;
    }
    Some(format!(
        "note: engine={engine} never records {}; those columns are omitted (this is not \"zero\")\n",
        omitted.join("/")
    ))
}

/// The JSONL meta line's `unmeasured` map: per omitted counter, why -- so a GUI need not guess.
fn unmeasured_json(engine: &str, orientation: &str, omitted: &[&str]) -> serde_json::Value {
    let map: serde_json::Map<String, serde_json::Value> = omitted
        .iter()
        .map(|c| {
            (
                c.to_string(),
                serde_json::Value::String(format!("engine={engine} never records it")),
            )
        })
        .collect();
    let mut map = map;
    if orientation == "allomorph" && engine != "foma" {
        for counter in ["attempts", "uses", "no_root"] {
            map.insert(
                counter.to_string(),
                serde_json::Value::String(format!(
                    "MorphRule named-allomorph rows have rule-level {counter} only"
                )),
            );
        }
    }
    serde_json::Value::Object(map)
}

/// Nulls every `RowView` field this cache's engine or report orientation cannot measure.
fn apply_engine_omission(rows: Vec<RowView>, omitted: &[&str]) -> Vec<RowView> {
    if omitted.is_empty() {
        return rows;
    }
    let has = |c: &str| omitted.contains(&c);
    rows.into_iter()
        .map(|mut r| {
            if has("attempts") {
                r.attempts = None;
            }
            if has("work") {
                r.work = None;
            }
            if has("outputs") {
                r.outputs = None;
            }
            if has("not_applied") {
                r.not_applied = None;
            }
            if has("no_root") {
                r.no_root = None;
            }
            if has("surface_mismatch") {
                r.surface_mismatch = None;
            }
            if has("uses") {
                r.uses = None;
            }
            r
        })
        .collect()
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

fn object_row_view(r: &pg_stats::PerObjectRow) -> RowView {
    RowView {
        label: format!("{}: {}", r.kind, r.label),
        kind: Some(r.kind.clone()),
        identity_quality: Some(r.identity_quality.clone()),
        self_time_ns: self_time_value(&r.kind, r.self_time_ns),
        attempts: cell_value(&r.kind, "attempts", r.attempts),
        outputs: cell_value(&r.kind, "outputs", r.outputs),
        uses: cell_value(&r.kind, "uses", r.uses),
        work: cell_value(&r.kind, "work", r.work),
        not_applied: cell_value(&r.kind, "not_applied", r.not_applied),
        no_root: cell_value(&r.kind, "no_root", r.no_root),
        surface_mismatch: cell_value(&r.kind, "surface_mismatch", r.surface_mismatch),
    }
}

fn allomorph_row_view(r: &pg_stats::PerAllomorphRow) -> RowView {
    RowView {
        label: format!(
            "{}: {} [{}]",
            r.object_kind, r.object_label, r.allomorph_label
        ),
        kind: Some(r.object_kind.clone()),
        identity_quality: None,
        self_time_ns: self_time_value(&r.object_kind, r.self_time_ns),
        attempts: if r.allomorph_key.is_some() && r.object_kind == "morph_rule" {
            None
        } else {
            cell_value(&r.object_kind, "attempts", r.attempts)
        },
        outputs: cell_value(&r.object_kind, "outputs", r.outputs),
        uses: if r.allomorph_key.is_some() && r.object_kind == "morph_rule" {
            None
        } else {
            cell_value(&r.object_kind, "uses", r.uses)
        },
        work: cell_value(&r.object_kind, "work", r.work),
        not_applied: cell_value(&r.object_kind, "not_applied", r.not_applied),
        no_root: if r.allomorph_key.is_some() && r.object_kind == "morph_rule" {
            None
        } else {
            cell_value(&r.object_kind, "no_root", r.no_root)
        },
        surface_mismatch: cell_value(&r.object_kind, "surface_mismatch", r.surface_mismatch),
    }
}

fn morpheme_row_view(r: &pg_stats::PerMorphemeRow) -> RowView {
    const KIND: &str = "lex_entry";
    RowView {
        label: r.morpheme_label.clone(),
        kind: Some(KIND.to_string()),
        identity_quality: None,
        self_time_ns: self_time_value(KIND, r.self_time_ns),
        attempts: cell_value(KIND, "attempts", r.attempts),
        outputs: cell_value(KIND, "outputs", r.outputs),
        uses: cell_value(KIND, "uses", r.uses),
        work: cell_value(KIND, "work", r.work),
        not_applied: cell_value(KIND, "not_applied", r.not_applied),
        no_root: cell_value(KIND, "no_root", r.no_root),
        surface_mismatch: cell_value(KIND, "surface_mismatch", r.surface_mismatch),
    }
}

fn kind_row_view(r: &pg_stats::PerKindRow) -> RowView {
    RowView {
        label: r.kind.clone(),
        kind: Some(r.kind.clone()),
        identity_quality: None,
        self_time_ns: self_time_value(&r.kind, r.self_time_ns),
        attempts: cell_value(&r.kind, "attempts", r.attempts),
        outputs: cell_value(&r.kind, "outputs", r.outputs),
        uses: cell_value(&r.kind, "uses", r.uses),
        work: cell_value(&r.kind, "work", r.work),
        not_applied: cell_value(&r.kind, "not_applied", r.not_applied),
        no_root: cell_value(&r.kind, "no_root", r.no_root),
        surface_mismatch: cell_value(&r.kind, "surface_mismatch", r.surface_mismatch),
    }
}

fn direction_time_view(mut view: RowView, direction: Option<&str>) -> RowView {
    let direction = match direction {
        Some("analysis") => Some(pg_rules::stats::Direction::Analysis),
        Some("synthesis") => Some(pg_rules::stats::Direction::Synthesis),
        _ => None,
    };
    if let (Some(kind), Some(direction)) = (view.kind.as_deref(), direction) {
        let stats_kind: pg_stats::ObjectKind = kind.parse().unwrap_or_else(|_| {
            panic!("kind string from the cache must be a known ObjectKind: {kind}")
        });
        if !pg_rules::stats::self_time_supported_in_direction(
            stats_kind_to_rules_kind(stats_kind),
            direction,
        ) {
            view.self_time_ns = None;
        }
    }
    view
}

fn fmt_ms(ns: Option<i64>) -> String {
    match ns {
        Some(v) => format!("{:.3}", v as f64 / 1e6),
        None => "-".to_string(),
    }
}

fn fmt_pct(v: Option<i64>, total: Option<i64>) -> String {
    match (v, total) {
        (Some(v), Some(total)) if total > 0 => {
            format!("{:.1}%", v as f64 / total as f64 * 100.0)
        }
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

// Sum the measured subset, but preserve the difference between no measurement and measured zero.
fn sum_optional<I>(values: I) -> Option<i64>
where
    I: IntoIterator<Item = Option<i64>>,
{
    let mut total = 0;
    let mut any = false;
    for value in values.into_iter().flatten() {
        any = true;
        total += value;
    }
    any.then_some(total)
}

// A subtotal is unavailable when any row lacks the measurement.
fn sum_complete<I>(values: I) -> Option<i64>
where
    I: IntoIterator<Item = Option<i64>>,
{
    let mut total = 0;
    let mut any = false;
    for value in values {
        let value = value?;
        any = true;
        total += value;
    }
    any.then_some(total)
}

/// Kinds paired with their summed `attempts`, heaviest first, ties broken by name for determinism.
fn attempts_by_kind_desc(rows: &[RowView]) -> Vec<(Option<String>, Option<i64>)> {
    let mut totals: HashMap<Option<String>, (i64, bool)> = HashMap::new();
    for r in rows {
        let entry = totals.entry(r.kind.clone()).or_insert((0, false));
        if let Some(value) = r.attempts {
            entry.0 += value;
            entry.1 = true;
        }
    }
    let mut out: Vec<(Option<String>, Option<i64>)> = totals
        .into_iter()
        .map(|(kind, (total, any))| (kind, any.then_some(total)))
        .collect();
    out.sort_by(|a, b| {
        b.1.is_some()
            .cmp(&a.1.is_some())
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.0.cmp(&b.0))
    });
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
    total_time_ns: Option<i64>,
    /// Per kind, because a rule's `attempts` is an invocation while a lexical entry's is a candidate materialization: one cross-kind share compares different units.
    attempts_by_kind: HashMap<Option<String>, Option<i64>>,
}

impl Denominators {
    fn of(rows: &[RowView]) -> Self {
        Denominators {
            total_time_ns: sum_optional(rows.iter().map(|r| r.self_time_ns)),
            attempts_by_kind: attempts_by_kind_desc(rows).into_iter().collect(),
        }
    }

    fn attempts_of(&self, kind: &Option<String>) -> Option<i64> {
        self.attempts_by_kind.get(kind).copied().flatten()
    }
}

/// Denominators are arguments, never sums of `rows`: `rows` may be a `--top` excerpt, and a share of an excerpt is not the share a reader reads it as. `omitted` drops a counter's column (and its derived `%`/`amp` columns) entirely, rather than merely masking its cells -- see `engine_omitted_counters`.
fn render_narrow(
    rows: &[RowView],
    wide: bool,
    denoms: &Denominators,
    omitted: &[&str],
) -> (Vec<&'static str>, Vec<Vec<String>>) {
    let show = |c: &str| !omitted.contains(&c);
    let show_attempts = show("attempts");
    let show_amp = show_attempts && show("outputs");
    let show_uses = show("uses");
    let show_work = show("work");
    let show_not_applied = show("not_applied");
    let show_no_root = show("no_root");
    let show_surface_mismatch = show("surface_mismatch");

    let mut headers: Vec<&'static str> = vec!["label", "time_ms", "time%"];
    if show_attempts {
        headers.push("attempts");
        headers.push("attempts%");
    }
    if show_amp {
        headers.push("amp");
    }
    if show_uses {
        headers.push("uses");
    }
    if wide {
        if show_work {
            headers.push("work");
        }
        if show_not_applied {
            headers.push("not_applied");
        }
        if show_no_root {
            headers.push("no_root");
        }
        if show_surface_mismatch {
            headers.push("surface_mismatch");
        }
        headers.push("identity_quality");
    }
    let table_rows = rows
        .iter()
        .map(|r| {
            let mut cells = vec![
                r.label.clone(),
                fmt_ms(r.self_time_ns),
                fmt_pct(r.self_time_ns, denoms.total_time_ns),
            ];
            if show_attempts {
                cells.push(fmt_opt_i64(r.attempts));
                cells.push(fmt_pct(r.attempts, denoms.attempts_of(&r.kind)));
            }
            if show_amp {
                cells.push(fmt_amp(r.attempts, r.outputs));
            }
            if show_uses {
                cells.push(fmt_opt_i64(r.uses));
            }
            if wide {
                if show_work {
                    cells.push(fmt_opt_i64(r.work));
                }
                if show_not_applied {
                    cells.push(fmt_opt_i64(r.not_applied));
                }
                if show_no_root {
                    cells.push(fmt_opt_i64(r.no_root));
                }
                if show_surface_mismatch {
                    cells.push(fmt_opt_i64(r.surface_mismatch));
                }
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
    total_time_ns: Option<i64>,
    /// Per kind, in descending count order: one cross-kind `attempts` sum adds different units.
    attempts_by_kind: Vec<(Option<String>, Option<i64>)>,
    total_uses: Option<i64>,
    run_elapsed_ns: i64,
}

impl TotalsSummary {
    /// `rows` must be the full matched set; `shown_rows` only narrows what the table displayed.
    fn from_rows(rows: &[RowView], shown_rows: usize, run_elapsed_ns: i64) -> Self {
        TotalsSummary {
            matched_rows: rows.len(),
            shown_rows,
            total_time_ns: sum_optional(rows.iter().map(|r| r.self_time_ns)),
            attempts_by_kind: attempts_by_kind_desc(rows),
            total_uses: sum_optional(rows.iter().map(|r| r.uses)),
            run_elapsed_ns,
        }
    }

    fn attributed_pct(&self) -> Option<f64> {
        self.total_time_ns.and_then(|total| {
            (self.run_elapsed_ns > 0).then_some(total as f64 / self.run_elapsed_ns as f64 * 100.0)
        })
    }

    /// `attempts a  b  c` across kinds, or a bare count when every row shares one kind.
    fn attempts_text(&self) -> String {
        match self.attempts_by_kind.as_slice() {
            [] => "-".to_string(),
            [(_, Some(n))] => n.to_string(),
            [(_, None)] => "-".to_string(),
            many => many
                .iter()
                .map(|(kind, n)| format!("{} {}", kind.as_deref().unwrap_or("-"), fmt_opt_i64(*n)))
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
            "TOTAL  {} row(s){}   time {} ({} attributed of {:.3}ms recorded)   attempts {}   uses {}\n",
            self.matched_rows,
            self.shown_note(),
            self.total_time_ns
                .map(|value| format!("{:.3}ms", value as f64 / 1e6))
                .unwrap_or_else(|| "-".to_string()),
            self.attributed_pct()
                .map(|value| format!("{value:.1}%"))
                .unwrap_or_else(|| "-".to_string()),
            self.run_elapsed_ns as f64 / 1e6,
            self.attempts_text(),
            fmt_opt_i64(self.total_uses),
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
    let distinct_grammars: i64 = conn
        .query_row("SELECT COUNT(DISTINCT grammar_hash) FROM run", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    if distinct_grammars > 1 {
        return Err(
            "stats: cache spans more than one grammar hash; report refused because rows are not comparable"
                .to_string(),
        );
    }
    let distinct_engines: i64 = conn
        .query_row("SELECT COUNT(DISTINCT engine) FROM run", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    if distinct_engines > 1 {
        return Err(
            "stats: cache spans more than one engine; report refused because counters are not comparable"
                .to_string(),
        );
    }
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
    let omitted = omitted_counters(&engine, orientation);
    Ok(serde_json::json!({
        "meta": true,
        "orientation": orientation,
        "grammar_hash": grammar_hash,
        "engine": engine,
        "filters": filters_json(filters),
        "totals": totals.to_json(),
        "unmeasured": unmeasured_json(&engine, orientation, &omitted),
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

/// Applies top-N per kind after totals so crowded kinds cannot hide others or narrow denominators.
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
    let (_, engine) = run_identity(conn)?;
    let omitted = omitted_counters(&engine, orientation);
    let rows = apply_engine_omission(rows, &omitted);
    let note = if orientation == "allomorph" && engine != "foma" {
        Some(
            "note: MorphRule named-allomorph rows show - for rule-level attempts, uses, and no_root; these are not zero\n"
                .to_string(),
        )
    } else {
        engine_omission_note(&engine, &omitted)
    };

    if rows.is_empty() {
        let reason = empty_explanation(conn, kind_scope)?;
        let out = empty_output(conn, orientation, filters, reason, format)?;
        return Ok(match (format, &note) {
            (OutputFormat::Text, Some(n)) => format!("{n}{out}"),
            _ => out,
        });
    }
    let run_elapsed_ns = pg_stats::word_elapsed_ns_total(conn, filters.word.as_deref())
        .map_err(|e| e.to_string())?;
    let shown = truncate_per_kind(rows.clone(), filters.top_n);
    let totals = TotalsSummary::from_rows(&rows, shown.len(), run_elapsed_ns);
    let denoms = Denominators::of(&rows);
    match format {
        OutputFormat::Text if filters.by_kind || orientation == "object" => {
            let mut out = String::new();
            if let Some(n) = &note {
                out.push_str(n);
            }
            for kind in kinds_in_row_order(&shown) {
                let section: Vec<RowView> =
                    shown.iter().filter(|r| r.kind == kind).cloned().collect();
                let (headers, table_rows) =
                    render_narrow(&section, filters.wide, &denoms, &omitted);
                out.push_str(&format!("== {} ==\n", kind.as_deref().unwrap_or("-")));
                out.push_str(&render_table(&headers, &table_rows));
                out.push_str(&subtotal_line(&section, &kind, &denoms));
                out.push('\n');
            }
            out.push_str(&totals.text_line());
            Ok(out)
        }
        OutputFormat::Text => {
            let (headers, table_rows) = render_narrow(&shown, filters.wide, &denoms, &omitted);
            let mut out = String::new();
            if let Some(n) = &note {
                out.push_str(n);
            }
            out.push_str(&render_table(&headers, &table_rows));
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
    let time = sum_complete(section.iter().map(|r| r.self_time_ns));
    let attempts = sum_complete(section.iter().map(|r| r.attempts));
    format!(
        "SUBTOTAL {}  {} row(s)  time {} ({} of total time)  attempts {}\n",
        kind.as_deref().unwrap_or("-"),
        section.len(),
        time.map(|value| format!("{:.3}ms", value as f64 / 1e6))
            .unwrap_or_else(|| "-".to_string()),
        fmt_pct(time, denoms.total_time_ns),
        fmt_opt_i64(attempts),
    )
}

fn subtotal_json(
    section: &[RowView],
    kind: &Option<String>,
    denoms: &Denominators,
) -> serde_json::Value {
    let time = sum_complete(section.iter().map(|r| r.self_time_ns));
    let attempts = sum_complete(section.iter().map(|r| r.attempts));
    serde_json::json!({
        "subtotal": true,
        "kind": kind,
        "rows": section.len(),
        "time_ns": time,
        "share_of_total_time_pct": match (time, denoms.total_time_ns) {
            (Some(time), Some(total)) if total > 0 => {
                Some(time as f64 / total as f64 * 100.0)
            }
            _ => None,
        },
        "attempts": attempts,
    })
}

fn render_object(
    conn: &rusqlite::Connection,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows = pg_stats::per_object_report(conn, &per_object_filter(filters))
        .map_err(|e| e.to_string())?;
    let views: Vec<RowView> = rows
        .iter()
        .map(|row| direction_time_view(object_row_view(row), filters.direction.as_deref()))
        .collect();
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
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows = pg_stats::per_allomorph_report(conn, &per_allomorph_filter(filters))
        .map_err(|e| e.to_string())?;
    let views: Vec<RowView> = rows
        .iter()
        .map(|row| direction_time_view(allomorph_row_view(row), filters.direction.as_deref()))
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

fn render_morpheme(
    conn: &rusqlite::Connection,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows = pg_stats::per_morpheme_report(conn, &per_morpheme_filter(filters))
        .map_err(|e| e.to_string())?;
    let views: Vec<RowView> = rows
        .iter()
        .map(|row| direction_time_view(morpheme_row_view(row), filters.direction.as_deref()))
        .collect();
    render_rowview_body(conn, "morpheme", filters, views, Some("lex_entry"), format)
}

fn render_group(
    conn: &rusqlite::Connection,
    filters: &Filters,
    format: OutputFormat,
) -> Result<String, String> {
    let rows =
        pg_stats::per_kind_report(conn, &per_kind_filter(filters)).map_err(|e| e.to_string())?;
    let views: Vec<RowView> = rows
        .iter()
        .map(|row| direction_time_view(kind_row_view(row), filters.direction.as_deref()))
        .collect();
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
    let (grammar_hash, engine) = run_identity(conn)?;
    let attempts_measured = engine != "foma";
    let total_elapsed: i64 = rows.iter().map(|r| r.elapsed_ns).sum();
    let total_attempts: i64 = rows.iter().map(|r| r.attempts).sum();
    match format {
        OutputFormat::Text => {
            let mut headers = vec!["form", "time_ms", "time%"];
            if attempts_measured {
                headers.extend(["attempts", "attempts%"]);
            }
            headers.extend(["passes", "capped", "timed_out"]);
            let table_rows: Vec<Vec<String>> = rows
                .iter()
                .map(|r| {
                    let mut row = vec![
                        r.form.clone(),
                        format!("{:.3}", r.elapsed_ns as f64 / 1e6),
                        fmt_pct(Some(r.elapsed_ns), Some(total_elapsed)),
                    ];
                    if attempts_measured {
                        row.push(r.attempts.to_string());
                        row.push(fmt_pct(Some(r.attempts), Some(total_attempts)));
                    }
                    row.extend([
                        r.passes.to_string(),
                        r.capped.to_string(),
                        r.timed_out.to_string(),
                    ]);
                    row
                })
                .collect();
            let mut out = render_table(&headers, &table_rows);
            if !attempts_measured {
                out.insert_str(0, "note: engine=foma never records attempts; attempts and attempts% are omitted (this is not \"zero\")\n");
            }
            out.push_str(&format!(
                "TOTAL  {} word(s)   time {:.3}ms (100.0% attributed; word rows ARE the recorded \
                 total){}\n",
                rows.len(),
                total_elapsed as f64 / 1e6,
                if attempts_measured {
                    format!("   attempts {total_attempts}")
                } else {
                    String::new()
                },
            ));
            Ok(out)
        }
        OutputFormat::Jsonl => {
            let meta = serde_json::json!({
                "meta": true,
                "orientation": "word",
                "grammar_hash": grammar_hash,
                "engine": engine,
                "filters": filters_json(filters),
                "totals": {"rows": rows.len(), "time_ns": total_elapsed, "attempts": if attempts_measured { serde_json::json!(total_attempts) } else { serde_json::Value::Null }},
                "unmeasured": if attempts_measured { serde_json::json!({}) } else { serde_json::json!({"attempts": "engine=foma never records it"}) },
            });
            let mut lines = vec![serde_json::to_string(&meta).map_err(|e| e.to_string())?];
            for r in &rows {
                let v = serde_json::json!({
                    "form": r.form,
                    "elapsed_ns": r.elapsed_ns,
                    "attempts": if attempts_measured { serde_json::json!(r.attempts) } else { serde_json::Value::Null },
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
    let (grammar_hash, engine) = run_identity(conn)?;
    if engine == "foma" {
        return match format {
            OutputFormat::Text => Ok(
                "engine=foma cannot measure never-fires; no object facts are recorded\n"
                    .to_string(),
            ),
            OutputFormat::Jsonl => {
                let value = serde_json::json!({
                    "meta": true,
                    "orientation": "never-fires",
                    "grammar_hash": grammar_hash,
                    "engine": engine,
                    "filters": filters_json(filters),
                    "totals": {"rows": 0, "attempts_by_kind": {}},
                    "unmeasured": {"never_fires": "engine=foma cannot measure it"},
                });
                Ok(format!(
                    "{}\n",
                    serde_json::to_string(&value).map_err(|e| e.to_string())?
                ))
            }
        };
    }
    let rows = pg_stats::never_fires_report(conn, &never_fires_filter(filters))
        .map_err(|e| e.to_string())?;
    if rows.is_empty() {
        let reason = empty_explanation(conn, filters.kind.as_deref())?;
        return empty_output(conn, "never-fires", filters, reason, format);
    }
    let mut attempts_by_kind: BTreeMap<String, i64> = BTreeMap::new();
    for row in &rows {
        *attempts_by_kind.entry(row.kind.clone()).or_insert(0) += row.attempts;
    }
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
                        fmt_pct(
                            Some(r.attempts),
                            attempts_by_kind
                                .get(&r.kind)
                                .copied()
                                .map(Some)
                                .unwrap_or(None),
                        ),
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
                attempts_by_kind
                    .iter()
                    .map(|(kind, attempts)| format!("{kind} {attempts}"))
                    .collect::<Vec<_>>()
                    .join("  ")
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
                "totals": {"rows": rows.len(), "attempts_by_kind": attempts_by_kind},
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

fn render_default(conn: &rusqlite::Connection, filters: &Filters) -> Result<String, String> {
    let mut out = render_word(conn, filters, OutputFormat::Text)?;
    out.push('\n');
    out.push_str(&render_object(conn, filters, OutputFormat::Text)?);

    let (_, engine) = run_identity(conn)?;
    let never_fires_rows =
        pg_stats::never_fires_report(conn, &pg_stats::NeverFiresFilter::default())
            .map_err(|e| e.to_string())?;
    if engine == "foma" || !never_fires_rows.is_empty() {
        out.push('\n');
        out.push_str(&render_never_fires(conn, filters, OutputFormat::Text)?);
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
            s => {
                crate::reject_unknown_option("stats", s)?;
                positional.push(s);
            }
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
        Some("morpheme") => Some(ReportGroup::Morpheme),
        Some("group") => Some(ReportGroup::Group),
        Some("never-fires") => Some(ReportGroup::NeverFires),
        Some(other) => {
            return Err(format!(
            "invalid --group: {other} (expected word|object|allomorph|morpheme|group|never-fires)"
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
    if matches!(group, None | Some(ReportGroup::Object))
        && filters.kind.is_none()
        && !matches!(filters.sort, None | Some(pg_stats::SortKey::SelfTimeNs))
    {
        return Err(
            "non-time object sorting requires --kind because counter units are only comparable within a kind"
                .to_string(),
        );
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
    run_identity(&conn)?;

    let body = match group {
        None => render_default(&conn, &filters)?,
        Some(ReportGroup::Word) => render_word(&conn, &filters, format)?,
        Some(ReportGroup::Object) => render_object(&conn, &filters, format)?,
        Some(ReportGroup::Allomorph) => render_allomorph(&conn, &filters, format)?,
        Some(ReportGroup::Morpheme) => render_morpheme(&conn, &filters, format)?,
        Some(ReportGroup::Group) => render_group(&conn, &filters, format)?,
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

fn self_time_value(kind: &str, value: i64) -> Option<i64> {
    let stats_kind: pg_stats::ObjectKind = kind.parse().unwrap_or_else(|_| {
        panic!("kind string from the cache must be a known ObjectKind: {kind}")
    });
    pg_rules::stats::self_time_supported(stats_kind_to_rules_kind(stats_kind)).then_some(value)
}

#[cfg(test)]
mod tests;
