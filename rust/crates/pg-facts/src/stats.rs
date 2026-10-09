use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::str::FromStr;

use pg_grammar::model::{Grammar, MorphRuleDef};
use pg_grammar::stats_identity::{
    self, IdentityQuality as GrammarIdentityQuality, ObjectIdentity,
    ObjectKind as GrammarObjectKind, OverlayPhase, StatsIdentityCatalog,
};
use pg_snapshot::morphology::MorphType;
use pg_snapshot::Snapshot;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::Deserialize;

use crate::{FactsError, ProducerIdentity};

const FORMAT: &str = "pangloss-batch-stats-manifest";
const VERSION: u32 = 1;
const LOGICAL_COUNTERS: [&str; 7] = [
    "attempts",
    "work",
    "outputs",
    "not_applied",
    "no_root",
    "surface_mismatch",
    "uses",
];
type RawDimension = (i64, Option<String>, Option<String>);
type SourceBridge = (String, String, String);
type AllomorphSourceBridge = (Option<String>, String, bool);

/// Inputs for attaching one frozen `batch --stats-manifest` run to a facts artifact.
#[derive(Debug, Clone, Copy)]
pub struct StatsInput<'a> {
    pub cache_path: &'a Path,
    pub manifest_path: &'a Path,
}

impl<'a> StatsInput<'a> {
    pub fn new(cache_path: &'a Path, manifest_path: &'a Path) -> Self {
        Self {
            cache_path,
            manifest_path,
        }
    }
}

pub(crate) struct StatsExpectation<'a> {
    pub source_sha256: &'a str,
    pub grammar_hash: &'a str,
    pub model_fingerprint: &'a str,
    pub compile_options_json: &'a str,
    pub producer: ProducerIdentity<'a>,
}

pub(crate) struct StatsProjection {
    cache_sha256: String,
    cache_bytes: u64,
    pub(crate) manifest_sha256: String,
    schema_version: i64,
    pub(crate) counter_semantics: i64,
    run: ExportRun,
    morphemes: Vec<ExportDimension>,
    strata: Vec<ExportDimension>,
    allomorphs: Vec<ExportAllomorph>,
    objects: Vec<ExportObject>,
    object_sources: Vec<ExportObjectSource>,
    allomorph_sources: Vec<ExportAllomorphSource>,
    words: Vec<ExportWord>,
    facts: Vec<ExportFact>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u32,
    source: ManifestSource,
    compiler: ManifestCompiler,
    cache: ManifestCache,
    run: ManifestRun,
    batch: ManifestBatch,
    input: ManifestInput,
    completion: ManifestCompletion,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestSource {
    kind: String,
    source_sha256: String,
    grammar_hash: String,
    model_fingerprint: String,
    compile_options_json: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestCompiler {
    version: String,
    build_identity: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestCache {
    sha256: String,
    bytes: u64,
    schema_version: i64,
    counter_semantics_version: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestRun {
    id: i64,
    engine: String,
    grammar_hash: String,
    options_hash: String,
    options_json: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestBatch {
    engine: String,
    threads: usize,
    step_cap: String,
    work_cap: usize,
    search_budget_semantics: u32,
    word_timeout_ms: Option<u64>,
    guess: bool,
    always_enforce_final_templates: bool,
    start: usize,
    analyses_requested: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestInput {
    word_count: usize,
    word_list_sha256: String,
    words: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestCompletion {
    requested: usize,
    complete: usize,
    incomplete: usize,
    invalid_shape: usize,
    missing: usize,
    words: Vec<ManifestWordCompletion>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestWordCompletion {
    index: usize,
    form: String,
    status: String,
    capped: bool,
    timed_out: bool,
    invalid_shape: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunOptions {
    engine: String,
    step_cap: Option<String>,
    work_cap: usize,
    search_budget_semantics: u32,
    word_timeout_ms: Option<u64>,
    guess: bool,
    always_enforce_final_templates: bool,
}

#[derive(Clone)]
struct ExportDimension {
    id: i64,
    key: Option<String>,
    label: Option<String>,
    identity_quality: Option<&'static str>,
}

struct ExportAllomorph {
    id: i64,
    key: Option<String>,
    label: Option<String>,
    identity_quality: Option<&'static str>,
}

struct ExportObject {
    id: i64,
    key: String,
    kind: String,
    label: String,
    identity_quality: String,
    morpheme_id: i64,
}

struct ExportObjectSource {
    object_id: i64,
    source_kind: String,
    source_guid: String,
    role: String,
}

struct ExportAllomorphSource {
    allomorph_id: i64,
    source_ordinal: i64,
    source_allomorph_guid: Option<String>,
    role: String,
    omitted: bool,
}

struct ExportRun {
    run_id: i64,
    engine: String,
    grammar_hash: String,
    options_hash: String,
    options_json: String,
    created_utc: String,
    step_cap: Option<i64>,
    cache_word_count: i64,
    requested_word_count: usize,
    batch_options_json: String,
}

struct ExportWord {
    id: i64,
    form: String,
    status: String,
    elapsed_ns: Option<u64>,
    attempts: Option<u64>,
    passes: Option<u64>,
    capped: Option<bool>,
    timed_out: Option<bool>,
    invalid_shape: Option<bool>,
    run_id: i64,
}

struct ExportFact {
    word_id: i64,
    object_id: i64,
    stratum_id: i64,
    allomorph_id: i64,
    direction: String,
    attempts: u64,
    work: u64,
    outputs: u64,
    not_applied: u64,
    no_root: u64,
    surface_mismatch: u64,
    uses: u64,
    self_time_ns: u64,
}

#[derive(Clone)]
struct ExpectedObject {
    key: String,
    kind: pg_stats::ObjectKind,
    labels: BTreeSet<String>,
    quality: &'static str,
    morpheme: Option<(String, String)>,
    sources: Vec<SourceBridge>,
}

#[derive(Clone)]
struct ExpectedLocator {
    key: String,
    label: String,
    quality: &'static str,
}

struct CacheData {
    schema_version: i64,
    counter_semantics: i64,
    run: ExportRun,
    morphemes: Vec<RawDimension>,
    strata: Vec<RawDimension>,
    allomorphs: Vec<RawDimension>,
    objects: Vec<RawObject>,
    words: Vec<RawWord>,
    facts: Vec<RawFact>,
}

struct RawObject {
    id: i64,
    key: String,
    kind: String,
    label: String,
    quality: String,
    morpheme_id: i64,
}

struct RawWord {
    id: i64,
    form: String,
    elapsed_ns: u64,
    attempts: u64,
    passes: u64,
    capped: bool,
    timed_out: bool,
    invalid_shape: bool,
}

struct RawFact {
    word_id: i64,
    object_id: i64,
    stratum_id: i64,
    allomorph_id: i64,
    direction: String,
    attempts: u64,
    work: u64,
    outputs: u64,
    not_applied: u64,
    no_root: u64,
    surface_mismatch: u64,
    uses: u64,
    self_time_ns: u64,
}

pub(crate) fn read_projection(
    input: StatsInput<'_>,
    expectation: StatsExpectation<'_>,
    snapshot: &Snapshot,
    grammar: &Grammar,
) -> Result<StatsProjection, FactsError> {
    let manifest_bytes = std::fs::read(input.manifest_path).map_err(|error| {
        FactsError::StatsRunUnavailable(format!(
            "read stats manifest {}: {error}",
            input.manifest_path.display()
        ))
    })?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        FactsError::StatsContextMismatch(format!("invalid stats manifest JSON: {error}"))
    })?;
    validate_manifest_context(&manifest, &expectation)?;
    let (cache_sha256, cache_bytes) = digest_cache(input.cache_path)?;
    validate_cache_digest(&manifest, &cache_sha256, cache_bytes)?;
    refuse_nonempty_wal(input.cache_path)?;
    let cache = read_cache(input.cache_path, &manifest)?;
    refuse_nonempty_wal(input.cache_path)?;
    let (after_sha256, after_bytes) = digest_cache(input.cache_path)?;
    if after_sha256 != cache_sha256 || after_bytes != cache_bytes {
        return Err(FactsError::StatsRunUnavailable(
            "stats cache changed while its facts were being read".into(),
        ));
    }
    validate_run_options(&manifest, &cache.run)?;
    let identities = StatsIdentityCatalog::new(grammar);
    let owners = expected_objects(snapshot, grammar, &identities)?;
    let dimensions = expected_dimensions(grammar, snapshot, &identities)?;
    remap_cache(
        cache,
        manifest,
        owners,
        dimensions,
        cache_sha256,
        cache_bytes,
        sha256(&manifest_bytes),
    )
}

fn validate_cache_digest(
    manifest: &Manifest,
    cache_sha256: &str,
    cache_bytes: u64,
) -> Result<(), FactsError> {
    if cache_sha256 != manifest.cache.sha256 || cache_bytes != manifest.cache.bytes {
        return Err(FactsError::StatsRunUnavailable(
            "stats cache digest or byte count differs from the frozen manifest".into(),
        ));
    }
    Ok(())
}

fn validate_manifest_context(
    manifest: &Manifest,
    expected: &StatsExpectation<'_>,
) -> Result<(), FactsError> {
    if manifest.format != FORMAT || manifest.version != VERSION {
        return Err(FactsError::StatsContextMismatch(format!(
            "unsupported stats manifest identity {} version {}",
            manifest.format, manifest.version
        )));
    }
    if manifest.source.kind != "snapshot"
        || manifest.source.source_sha256 != expected.source_sha256
        || manifest.source.grammar_hash != expected.grammar_hash
        || manifest.source.model_fingerprint != expected.model_fingerprint
        || manifest.source.compile_options_json.as_deref() != Some(expected.compile_options_json)
    {
        return Err(FactsError::StatsContextMismatch(
            "stats manifest source identity does not match the supplied Snapshot and production compile options".into(),
        ));
    }
    if manifest.compiler.version != expected.producer.compiler_version
        || manifest.compiler.build_identity != expected.producer.build_identity
    {
        return Err(FactsError::StatsContextMismatch(
            "stats manifest compiler/build identity does not match this facts writer".into(),
        ));
    }
    if manifest.batch.engine != "hc" || manifest.run.engine != "hc" {
        return Err(FactsError::StatsRunUnavailable(format!(
            "stats engine {:?} has no per-object facts projection",
            manifest.batch.engine
        )));
    }
    if manifest.batch.threads == 0 || manifest.batch.start != 0 {
        return Err(FactsError::StatsRunUnavailable(
            "frozen stats manifest has a non-positive thread count or resumed start index".into(),
        ));
    }
    let step_cap = pg_stats::StepCap::from_str(&manifest.batch.step_cap).map_err(|error| {
        FactsError::StatsRunUnavailable(format!("invalid stats manifest step cap: {error}"))
    })?;
    if manifest.batch.work_cap == 0 || manifest.batch.search_budget_semantics == 0 {
        return Err(FactsError::StatsRunUnavailable(
            "frozen stats manifest has invalid search budget options".into(),
        ));
    }
    let _ = step_cap;

    let word_bytes = serde_json::to_vec(&manifest.input.words).map_err(|error| {
        FactsError::StatsRunUnavailable(format!("serialize frozen stats input words: {error}"))
    })?;
    if manifest.input.word_count != manifest.input.words.len()
        || manifest.input.word_list_sha256 != sha256(&word_bytes)
        || manifest.completion.requested != manifest.input.word_count
        || manifest.completion.words.len() != manifest.input.word_count
    {
        return Err(FactsError::StatsRunUnavailable(
            "frozen stats manifest input identity or completion census is inconsistent".into(),
        ));
    }
    let mut unique_words = BTreeSet::new();
    let mut complete = 0;
    let mut incomplete = 0;
    let mut invalid_shape = 0;
    let mut missing = 0;
    for (index, (form, completion)) in manifest
        .input
        .words
        .iter()
        .zip(&manifest.completion.words)
        .enumerate()
    {
        if !unique_words.insert(form.as_str())
            || completion.index != index
            || completion.form != *form
        {
            return Err(FactsError::StatsRunUnavailable(format!(
                "frozen stats word list is duplicate or misordered at index {index}"
            )));
        }
        match completion.status.as_str() {
            "complete"
                if !completion.capped && !completion.timed_out && !completion.invalid_shape =>
            {
                complete += 1;
            }
            "incomplete"
                if (completion.capped || completion.timed_out) && !completion.invalid_shape =>
            {
                incomplete += 1;
            }
            "invalid_shape" if completion.invalid_shape => invalid_shape += 1,
            "missing"
                if !completion.capped && !completion.timed_out && !completion.invalid_shape =>
            {
                missing += 1;
            }
            _ => {
                return Err(FactsError::StatsRunUnavailable(format!(
                    "frozen stats completion status and flags disagree for word {form:?}"
                )));
            }
        }
    }
    if complete != manifest.completion.complete
        || incomplete != manifest.completion.incomplete
        || invalid_shape != manifest.completion.invalid_shape
        || missing != manifest.completion.missing
    {
        return Err(FactsError::StatsRunUnavailable(
            "frozen stats completion counts do not match per-word completion rows".into(),
        ));
    }
    Ok(())
}

fn validate_run_options(manifest: &Manifest, run: &ExportRun) -> Result<(), FactsError> {
    let options: RunOptions = serde_json::from_str(&run.options_json).map_err(|error| {
        FactsError::StatsRunUnavailable(format!("invalid stats run options JSON: {error}"))
    })?;
    let expected_step_cap = Some(manifest.batch.step_cap.as_str());
    let option_step_cap = options.step_cap.as_deref();
    if options.engine != manifest.batch.engine
        || option_step_cap != expected_step_cap
        || options.work_cap != manifest.batch.work_cap
        || options.search_budget_semantics != manifest.batch.search_budget_semantics
        || options.word_timeout_ms != manifest.batch.word_timeout_ms
        || options.guess != manifest.batch.guess
        || options.always_enforce_final_templates != manifest.batch.always_enforce_final_templates
        || run.engine != manifest.run.engine
        || run.grammar_hash != manifest.run.grammar_hash
        || run.options_hash != manifest.run.options_hash
        || run.options_json != manifest.run.options_json
        || run.options_hash != sha256_hex(run.options_json.as_bytes())
        || run.run_id != manifest.run.id
        || run.step_cap
            != pg_stats::StepCap::from_str(&manifest.batch.step_cap)
                .ok()
                .and_then(|cap| cap.to_storage().ok())
    {
        return Err(FactsError::StatsRunUnavailable(
            "stats batch options do not match the declared cache run".into(),
        ));
    }
    if manifest.batch.engine != "hc" {
        return Err(FactsError::StatsRunUnavailable(
            "only HermitCrab per-object stats are supported".into(),
        ));
    }
    Ok(())
}

fn read_cache(path: &Path, manifest: &Manifest) -> Result<CacheData, FactsError> {
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(stats_db_error)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(stats_db_error)?;
    let (identity_schema, identity_grammar, identity_engine): (i64, String, Option<String>) = tx
        .query_row(
            "SELECT schema_version, grammar_hash, engine FROM cache_identity WHERE cache_id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(stats_db_error)?;
    let run_count: i64 = tx
        .query_row("SELECT COUNT(*) FROM run", [], |row| row.get(0))
        .map_err(stats_db_error)?;
    if run_count != 1 {
        return Err(FactsError::StatsRunUnavailable(format!(
            "frozen stats cache must contain exactly one run, found {run_count}"
        )));
    }
    let (
        run_id,
        schema_version,
        counter_semantics,
        build_info,
        grammar_hash,
        engine,
        options_hash,
        options_json,
        word_count,
        step_cap,
        created_utc,
    ) = tx
        .query_row(
            "SELECT run_id, schema_version, counter_semantics, build_info, grammar_hash, engine, options_hash, options_json, word_count, step_cap, created_utc FROM run",
            [],
            |row| Ok((
                row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?,
                row.get::<_, String>(6)?, row.get::<_, String>(7)?, row.get::<_, i64>(8)?,
                row.get::<_, Option<i64>>(9)?, row.get::<_, String>(10)?,
            )),
        )
        .optional()
        .map_err(stats_db_error)?
        .ok_or_else(|| FactsError::StatsRunUnavailable("stats run row is absent".into()))?;
    if identity_schema != pg_stats::SCHEMA_VERSION
        || schema_version != pg_stats::SCHEMA_VERSION
        || identity_schema != schema_version
        || identity_grammar != grammar_hash
        || identity_grammar != manifest.source.grammar_hash
        || identity_engine.as_deref() != Some(engine.as_str())
        || engine != manifest.run.engine
        || grammar_hash != manifest.run.grammar_hash
        || options_hash != manifest.run.options_hash
        || options_json != manifest.run.options_json
        || run_id != manifest.run.id
        || build_info != manifest.compiler.build_identity
        || manifest.cache.schema_version != schema_version
        || manifest.cache.counter_semantics_version != counter_semantics
    {
        return Err(FactsError::StatsRunUnavailable(
            "stats cache identity and run row do not match the frozen manifest".into(),
        ));
    }
    let expected_cache_words = manifest.completion.requested - manifest.completion.missing;
    if word_count < 0 || word_count as usize != expected_cache_words {
        return Err(FactsError::StatsRunUnavailable(format!(
            "stats run declares {word_count} word rows but manifest requires {expected_cache_words}"
        )));
    }
    let run = ExportRun {
        run_id,
        engine,
        grammar_hash,
        options_hash,
        options_json,
        created_utc,
        step_cap,
        cache_word_count: word_count,
        requested_word_count: manifest.input.word_count,
        batch_options_json: serde_json::to_string(&serde_json::json!({
            "engine": manifest.batch.engine,
            "threads": manifest.batch.threads,
            "step_cap": manifest.batch.step_cap,
            "work_cap": manifest.batch.work_cap,
            "search_budget_semantics": manifest.batch.search_budget_semantics,
            "word_timeout_ms": manifest.batch.word_timeout_ms,
            "guess": manifest.batch.guess,
            "always_enforce_final_templates": manifest.batch.always_enforce_final_templates,
            "start": manifest.batch.start,
            "analyses_requested": manifest.batch.analyses_requested,
        }))
        .map_err(|error| FactsError::StatsRunUnavailable(error.to_string()))?,
    };

    let morphemes = read_dimensions(&tx, "morpheme", "morpheme_id")?;
    let strata = read_dimensions(&tx, "stratum", "stratum_id")?;
    let allomorphs = read_dimensions(&tx, "allomorph", "allomorph_id")?;
    let objects = {
        let mut statement = tx
            .prepare(
                "SELECT object_id, key, kind, label, identity_quality, morpheme_id FROM object",
            )
            .map_err(stats_db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(RawObject {
                    id: row.get(0)?,
                    key: row.get(1)?,
                    kind: row.get(2)?,
                    label: row.get(3)?,
                    quality: row.get(4)?,
                    morpheme_id: row.get(5)?,
                })
            })
            .map_err(stats_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(stats_db_error)?;
        rows
    };
    let words = {
        let mut statement = tx
            .prepare("SELECT word_id, form, elapsed_ns, attempts, passes, capped, timed_out, invalid_shape FROM word WHERE run_id=?1")
            .map_err(stats_db_error)?;
        let rows = statement
            .query_map(params![run_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, u64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            })
            .map_err(stats_db_error)?
            .map(|row| {
                let (id, form, elapsed_ns, attempts, passes, capped, timed_out, invalid_shape) =
                    row.map_err(stats_db_error)?;
                Ok(RawWord {
                    id,
                    form,
                    elapsed_ns,
                    attempts,
                    passes,
                    capped: cache_bool("capped", capped)?,
                    timed_out: cache_bool("timed_out", timed_out)?,
                    invalid_shape: cache_bool("invalid_shape", invalid_shape)?,
                })
            })
            .collect::<Result<Vec<_>, FactsError>>()?;
        rows
    };
    if words.len() as i64 != word_count {
        return Err(FactsError::StatsRunUnavailable(format!(
            "stats cache run declares {word_count} words but owns {} rows",
            words.len()
        )));
    }
    let facts = {
        let mut statement = tx
            .prepare("SELECT word_id, object_id, stratum_id, allomorph_id, direction, attempts, work, outputs, not_applied, no_root, surface_mismatch, uses, self_time_ns FROM fact ORDER BY word_id, object_id, stratum_id, allomorph_id, direction")
            .map_err(stats_db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(RawFact {
                    word_id: row.get(0)?,
                    object_id: row.get(1)?,
                    stratum_id: row.get(2)?,
                    allomorph_id: row.get(3)?,
                    direction: row.get(4)?,
                    attempts: row.get(5)?,
                    work: row.get(6)?,
                    outputs: row.get(7)?,
                    not_applied: row.get(8)?,
                    no_root: row.get(9)?,
                    surface_mismatch: row.get(10)?,
                    uses: row.get(11)?,
                    self_time_ns: row.get(12)?,
                })
            })
            .map_err(stats_db_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(stats_db_error)?;
        rows
    };
    tx.commit().map_err(stats_db_error)?;
    connection
        .close()
        .map_err(|(_, error)| stats_db_error(error))?;

    let mut actual_forms: BTreeSet<String> = BTreeSet::new();
    for word in &words {
        if !actual_forms.insert(word.form.clone()) {
            return Err(FactsError::StatsRunUnavailable(format!(
                "stats cache contains duplicate word {:?}",
                word.form
            )));
        }
    }
    Ok(CacheData {
        schema_version,
        counter_semantics,
        run,
        morphemes,
        strata,
        allomorphs,
        objects,
        words,
        facts,
    })
}

fn read_dimensions(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    id_column: &str,
) -> Result<Vec<RawDimension>, FactsError> {
    // Table and column names are closed constants selected by this module.
    let sql = format!("SELECT {id_column}, key, label FROM {table} ORDER BY {id_column}");
    let mut statement = tx.prepare(&sql).map_err(stats_db_error)?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(stats_db_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(stats_db_error)?;
    Ok(rows)
}

fn remap_cache(
    cache: CacheData,
    manifest: Manifest,
    owners: BTreeMap<(String, String), ExpectedObject>,
    dimensions: ExpectedDimensions,
    cache_sha256: String,
    cache_bytes: u64,
    manifest_sha256: String,
) -> Result<StatsProjection, FactsError> {
    let morphemes = remap_dimensions(cache.morphemes.clone(), &dimensions.morphemes, "morpheme")?;
    let strata = remap_dimensions(cache.strata.clone(), &dimensions.strata, "stratum")?;
    let allomorphs = remap_allomorphs(cache.allomorphs.clone(), &dimensions.allomorphs)?;
    let morpheme_id_map = raw_to_local(&cache.morphemes, &morphemes, "morpheme")?;
    let stratum_id_map = raw_to_local(&cache.strata, &strata, "stratum")?;
    let allomorph_id_map = raw_allomorph_to_local(&cache.allomorphs, &allomorphs)?;

    let mut sorted_objects = cache.objects;
    sorted_objects.sort_by(|a, b| (&a.kind, &a.key).cmp(&(&b.kind, &b.key)));
    let mut object_id_map = HashMap::new();
    let mut objects = Vec::with_capacity(sorted_objects.len());
    let mut object_sources = Vec::new();
    for (index, object) in sorted_objects.into_iter().enumerate() {
        let expected = owners
            .get(&(object.kind.clone(), object.key.clone()))
            .ok_or_else(|| {
                FactsError::StatsRunUnavailable(format!(
                    "stats object {:?} ({}) has no typed identity in the compiled Snapshot grammar",
                    object.key, object.kind
                ))
            })?;
        if object.kind != expected.kind.as_str()
            || !expected.labels.contains(&object.label)
            || object.quality != expected.quality
        {
            return Err(FactsError::StatsRunUnavailable(format!(
                "stats object {:?} identity fields do not match its typed grammar owner (kind {}, label {:?}, quality {}; expected kind {}, labels {:?}, quality {})",
                object.key,
                object.kind,
                object.label,
                object.quality,
                expected.kind.as_str(),
                expected.labels,
                expected.quality,
            )));
        }
        let morpheme_id = if let Some((key, label)) = &expected.morpheme {
            let (raw_id, _, _) = cache
                .morphemes
                .iter()
                .find(|(_, existing_key, existing_label)| {
                    existing_key.as_deref() == Some(key.as_str())
                        && existing_label.as_deref() == Some(label.as_str())
                })
                .ok_or_else(|| {
                    FactsError::StatsRunUnavailable(format!(
                        "lexical stats object {:?} has no matching morpheme dimension",
                        object.key
                    ))
                })?;
            if object.morpheme_id != *raw_id {
                return Err(FactsError::StatsRunUnavailable(format!(
                    "stats object {:?} points at a different morpheme dimension",
                    object.key
                )));
            }
            *morpheme_id_map.get(raw_id).ok_or_else(|| {
                FactsError::StatsRunUnavailable("morpheme handle did not remap".into())
            })?
        } else {
            if object.morpheme_id != 0 {
                return Err(FactsError::StatsRunUnavailable(format!(
                    "non-lexical stats object {:?} unexpectedly names a morpheme",
                    object.key
                )));
            }
            0
        };
        let local_id = i64::try_from(index + 1)
            .map_err(|_| FactsError::StatsRunUnavailable("too many stats objects".into()))?;
        object_id_map.insert(object.id, local_id);
        objects.push(ExportObject {
            id: local_id,
            key: object.key,
            kind: object.kind,
            label: expected.labels.iter().next().cloned().ok_or_else(|| {
                FactsError::StatsRunUnavailable("typed stats object has no grammar label".into())
            })?,
            identity_quality: object.quality,
            morpheme_id,
        });
        for (source_kind, source_guid, role) in &expected.sources {
            object_sources.push(ExportObjectSource {
                object_id: local_id,
                source_kind: source_kind.clone(),
                source_guid: source_guid.clone(),
                role: role.clone(),
            });
        }
    }
    object_sources.sort_by(|a, b| {
        (&a.object_id, &a.source_kind, &a.source_guid, &a.role).cmp(&(
            &b.object_id,
            &b.source_kind,
            &b.source_guid,
            &b.role,
        ))
    });

    let mut words_by_form: HashMap<String, RawWord> = cache
        .words
        .into_iter()
        .map(|word| (word.form.clone(), word))
        .collect();
    let mut word_id_map = HashMap::new();
    let mut words = Vec::with_capacity(manifest.input.word_count);
    for (index, (form, completion)) in manifest
        .input
        .words
        .iter()
        .zip(&manifest.completion.words)
        .enumerate()
    {
        let local_id = i64::try_from(index + 1)
            .map_err(|_| FactsError::StatsRunUnavailable("too many stats words".into()))?;
        if completion.status == "missing" {
            if words_by_form.contains_key(form) {
                return Err(FactsError::StatsRunUnavailable(format!(
                    "manifest calls word {form:?} missing but the cache contains its row"
                )));
            }
            words.push(ExportWord {
                id: local_id,
                form: form.clone(),
                status: "not_attempted".into(),
                elapsed_ns: None,
                attempts: None,
                passes: None,
                capped: None,
                timed_out: None,
                invalid_shape: None,
                run_id: cache.run.run_id,
            });
            continue;
        }
        let word = words_by_form.remove(form).ok_or_else(|| {
            FactsError::StatsRunUnavailable(format!(
                "manifest records word {form:?} but the frozen stats cache row is absent"
            ))
        })?;
        if word.capped != completion.capped
            || word.timed_out != completion.timed_out
            || word.invalid_shape != completion.invalid_shape
        {
            return Err(FactsError::StatsRunUnavailable(format!(
                "stats cache completion flags differ from manifest for {form:?}"
            )));
        }
        word_id_map.insert(word.id, local_id);
        words.push(ExportWord {
            id: local_id,
            form: form.clone(),
            status: completion.status.clone(),
            elapsed_ns: Some(word.elapsed_ns),
            attempts: Some(word.attempts),
            passes: Some(word.passes),
            capped: Some(word.capped),
            timed_out: Some(word.timed_out),
            invalid_shape: Some(word.invalid_shape),
            run_id: cache.run.run_id,
        });
    }
    if let Some(form) = words_by_form.keys().next() {
        return Err(FactsError::StatsRunUnavailable(format!(
            "stats cache contains unrequested word {form:?}"
        )));
    }

    let mut facts = Vec::with_capacity(cache.facts.len());
    for fact in cache.facts {
        let get_id = |map: &HashMap<i64, i64>, name: &str, raw_id: i64| {
            map.get(&raw_id).copied().ok_or_else(|| {
                FactsError::StatsRunUnavailable(format!(
                    "stats fact refers to an unknown {name} handle {raw_id}"
                ))
            })
        };
        if !matches!(fact.direction.as_str(), "analysis" | "synthesis") {
            return Err(FactsError::StatsRunUnavailable(format!(
                "stats fact has unknown direction {:?}",
                fact.direction
            )));
        }
        facts.push(ExportFact {
            word_id: get_id(&word_id_map, "word", fact.word_id)?,
            object_id: get_id(&object_id_map, "object", fact.object_id)?,
            stratum_id: get_id(&stratum_id_map, "stratum", fact.stratum_id)?,
            allomorph_id: get_id(&allomorph_id_map, "allomorph", fact.allomorph_id)?,
            direction: fact.direction,
            attempts: fact.attempts,
            work: fact.work,
            outputs: fact.outputs,
            not_applied: fact.not_applied,
            no_root: fact.no_root,
            surface_mismatch: fact.surface_mismatch,
            uses: fact.uses,
            self_time_ns: fact.self_time_ns,
        });
    }
    facts.sort_by(|a, b| {
        (
            &a.word_id,
            &a.object_id,
            &a.stratum_id,
            &a.allomorph_id,
            &a.direction,
        )
            .cmp(&(
                &b.word_id,
                &b.object_id,
                &b.stratum_id,
                &b.allomorph_id,
                &b.direction,
            ))
    });

    let allomorph_sources = build_allomorph_sources(&allomorphs, &dimensions.allomorph_sources)?;
    Ok(StatsProjection {
        cache_sha256,
        cache_bytes,
        manifest_sha256,
        schema_version: cache.schema_version,
        counter_semantics: cache.counter_semantics,
        run: cache.run,
        morphemes,
        strata,
        allomorphs,
        objects,
        object_sources,
        allomorph_sources,
        words,
        facts,
    })
}

struct ExpectedDimensions {
    morphemes: BTreeMap<String, ExpectedLocator>,
    strata: BTreeMap<String, ExpectedLocator>,
    allomorphs: BTreeMap<String, ExpectedLocator>,
    allomorph_sources: BTreeMap<String, Vec<AllomorphSourceBridge>>,
}

fn expected_dimensions(
    grammar: &Grammar,
    snapshot: &Snapshot,
    identities: &StatsIdentityCatalog,
) -> Result<ExpectedDimensions, FactsError> {
    let mut morphemes = BTreeMap::new();
    for index in 0..grammar.morphemes.len() {
        let id = pg_grammar::model::MorphemeId(index as u32);
        let identity = stats_identity::morpheme_identity(grammar, id);
        morphemes.insert(
            identity.key,
            ExpectedLocator {
                label: identity.label,
                quality: quality(identity.quality),
                key: String::new(),
            },
        );
    }
    let guessed =
        stats_identity::morpheme_identity(grammar, pg_grammar::model::MorphemeId::GUESSED);
    morphemes.insert(
        guessed.key,
        ExpectedLocator {
            key: String::new(),
            label: guessed.label,
            quality: quality(guessed.quality),
        },
    );
    for (key, expected) in &mut morphemes {
        expected.key = key.clone();
    }
    let mut strata = BTreeMap::new();
    for index in 0..grammar.strata.len() {
        let identity =
            stats_identity::stratum_identity(grammar, pg_grammar::model::StratumId(index as u8));
        strata.insert(
            identity.key.clone(),
            ExpectedLocator {
                key: identity.key,
                label: identity.label,
                quality: quality(identity.quality),
            },
        );
    }
    let mut allomorphs = BTreeMap::new();
    let mut allomorph_sources = BTreeMap::new();
    let source_form_types: BTreeMap<_, _> = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|entry| {
            entry
                .allomorphs
                .iter()
                .map(|allomorph| (allomorph.guid.as_str(), allomorph.morph_type))
        })
        .collect();
    for index in 0..grammar.allomorph_owners.len() {
        let id = pg_grammar::model::AllomorphId(index as u32);
        let identity = identities.allomorph(id);
        let source = grammar.allomorph_sources.get(index).ok_or_else(|| {
            FactsError::StatsRunUnavailable(format!(
                "compiled allomorph handle {} has no parallel source record",
                id.0
            ))
        })?;
        let mut sources = Vec::new();
        if source.omitted {
            if !source.form_guids.is_empty() {
                return Err(FactsError::StatsRunUnavailable(format!(
                    "omitted allomorph {} also names source forms",
                    id.0
                )));
            }
            sources.push((None, "null_affix".into(), true));
        } else {
            for guid in &source.form_guids {
                let role = match guid.as_deref() {
                    None => "unresolved_source_form".to_string(),
                    Some(guid) => {
                        let morph_type = source_form_types.get(guid).ok_or_else(|| {
                            FactsError::StatsRunUnavailable(format!(
                                "compiled allomorph source {guid:?} is absent from the exact Snapshot"
                            ))
                        })?;
                        morph_type_role(*morph_type).to_string()
                    }
                };
                sources.push((guid.clone(), role, false));
            }
        }
        allomorphs.insert(
            identity.key.clone(),
            ExpectedLocator {
                key: identity.key.clone(),
                label: identity.label,
                quality: quality(identity.quality),
            },
        );
        allomorph_sources.insert(identity.key, sources);
    }
    let guessed_allomorph = identities.allomorph(pg_grammar::model::AllomorphId::GUESSED);
    allomorphs.insert(
        guessed_allomorph.key.clone(),
        ExpectedLocator {
            key: guessed_allomorph.key.clone(),
            label: guessed_allomorph.label,
            quality: quality(guessed_allomorph.quality),
        },
    );
    allomorph_sources.insert(guessed_allomorph.key, Vec::new());
    Ok(ExpectedDimensions {
        morphemes,
        strata,
        allomorphs,
        allomorph_sources,
    })
}

fn morph_type_role(morph_type: MorphType) -> &'static str {
    match morph_type {
        MorphType::Stem => "stem",
        MorphType::BoundStem => "bound_stem",
        MorphType::Root => "root",
        MorphType::BoundRoot => "bound_root",
        MorphType::Prefix => "prefix",
        MorphType::Suffix => "suffix",
        MorphType::Infix => "infix",
        MorphType::Circumfix => "circumfix",
        MorphType::Proclitic => "proclitic",
        MorphType::Enclitic => "enclitic",
        MorphType::Clitic => "clitic",
        MorphType::Particle => "particle",
        MorphType::Phrase => "phrase",
        MorphType::DiscontigPhrase => "discontig_phrase",
        MorphType::PrefixingInterfix => "prefixing_interfix",
        MorphType::InfixingInterfix => "infixing_interfix",
        MorphType::SuffixingInterfix => "suffixing_interfix",
    }
}

fn build_allomorph_sources(
    allomorphs: &[ExportAllomorph],
    expected_sources: &BTreeMap<String, Vec<AllomorphSourceBridge>>,
) -> Result<Vec<ExportAllomorphSource>, FactsError> {
    let mut rows = Vec::new();
    for allomorph in allomorphs {
        let Some(key) = allomorph.key.as_deref() else {
            continue;
        };
        let sources = expected_sources.get(key).ok_or_else(|| {
            FactsError::StatsRunUnavailable(format!(
                "allomorph {:?} has no typed compiled source map",
                key
            ))
        })?;
        if sources.is_empty() {
            continue;
        }
        for (ordinal, (guid, role, omitted)) in sources.iter().enumerate() {
            rows.push(ExportAllomorphSource {
                allomorph_id: allomorph.id,
                source_ordinal: ordinal as i64,
                source_allomorph_guid: guid.clone(),
                role: role.clone(),
                omitted: *omitted,
            });
        }
    }
    rows.sort_by(|a, b| {
        (&a.allomorph_id, &a.source_ordinal).cmp(&(&b.allomorph_id, &b.source_ordinal))
    });
    Ok(rows)
}

fn expected_objects(
    snapshot: &Snapshot,
    grammar: &Grammar,
    catalog: &StatsIdentityCatalog,
) -> Result<BTreeMap<(String, String), ExpectedObject>, FactsError> {
    let mut entries_by_allomorph = BTreeMap::new();
    let mut entry_guids = BTreeSet::new();
    let mut msa_guids = BTreeSet::new();
    for entry in &snapshot.lexicon.entries {
        entry_guids.insert(entry.guid.clone());
        for allomorph in &entry.allomorphs {
            entries_by_allomorph.insert(allomorph.guid.clone(), entry.guid.clone());
        }
        for msa in &entry.msas {
            msa_guids.insert(msa.guid().to_string());
        }
    }
    let compound_guids: BTreeSet<_> = snapshot
        .morphology
        .compound_rules
        .iter()
        .map(|rule| rule.guid().to_string())
        .collect();
    let phon_guids: BTreeSet<_> = snapshot
        .phonology
        .rules
        .iter()
        .map(|rule| match rule {
            pg_snapshot::phonology::PhonologicalRule::Rewrite(rule) => rule.guid.clone(),
            pg_snapshot::phonology::PhonologicalRule::Metathesis(rule) => rule.guid.clone(),
        })
        .collect();
    let allomorph_morph_types: BTreeMap<_, _> = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|entry| {
            entry
                .allomorphs
                .iter()
                .map(|allomorph| (allomorph.guid.clone(), allomorph.morph_type))
        })
        .collect();
    let source_context = SourceContext {
        snapshot,
        grammar,
        msa_guids: &msa_guids,
        entries_by_allomorph: &entries_by_allomorph,
        allomorph_morph_types: &allomorph_morph_types,
    };
    let mut identities = BTreeMap::new();
    let mut add = |identity: ObjectIdentity,
                   morpheme: Option<(String, String)>,
                   sources: Vec<SourceBridge>| {
        let kind = grammar_kind(identity.kind);
        let value = ExpectedObject {
            key: identity.key.clone(),
            kind,
            labels: BTreeSet::from([identity.label]),
            quality: quality(identity.quality),
            morpheme,
            sources,
        };
        merge_expected_object(&mut identities, value)
    };
    for (index, entry) in grammar.entries.iter().enumerate() {
        let id = pg_grammar::model::LexEntryId(index as u32);
        let identity = catalog.lex_entry(id).clone();
        let mut sources = Vec::new();
        if let Some(guid) = entry.source_guid.as_deref() {
            if !entry_guids.contains(guid) {
                return Err(FactsError::StatsRunUnavailable(format!(
                    "compiled entry source {guid:?} is absent from the exact Snapshot"
                )));
            }
            sources.push(("entry".into(), guid.into(), "owner_entry".into()));
        }
        let morpheme_identity = stats_identity::morpheme_identity(grammar, entry.morpheme);
        add(
            identity,
            Some((morpheme_identity.key, morpheme_identity.label)),
            sources,
        )?;
    }
    for (index, rule) in grammar.mrules.iter().enumerate() {
        let id = pg_grammar::model::MRuleId(index as u32);
        let identity = catalog.morph_rule(id).clone();
        let mut sources = Vec::new();
        match rule {
            MorphRuleDef::Compounding(compound) => {
                if let Some(guid) = compound.source_guid.as_deref() {
                    if !compound_guids.contains(guid) {
                        return Err(FactsError::StatsRunUnavailable(format!(
                            "compiled compound source {guid:?} is absent from the exact Snapshot"
                        )));
                    }
                    sources.push(("compoundRule".into(), guid.into(), "compound_rule".into()));
                }
            }
            MorphRuleDef::AffixProcess(def) => append_morpheme_sources(
                &source_context,
                def.morpheme,
                &def.allomorphs,
                &mut sources,
            )?,
            MorphRuleDef::Realizational(def) => append_morpheme_sources(
                &source_context,
                def.morpheme,
                &def.allomorphs,
                &mut sources,
            )?,
        }
        add(identity, None, sources)?;
    }
    for (index, _rule) in grammar.prules.iter().enumerate() {
        let id = pg_grammar::model::PRuleId(index as u32);
        let identity = stats_identity::phon_rule_identity(grammar, id);
        let guid = identity.key.clone();
        let sources = if phon_guids.contains(&guid) {
            vec![("phonologicalRule".into(), guid, "phonological_rule".into())]
        } else {
            Vec::new()
        };
        add(identity, None, sources)?;
    }
    for index in 0..grammar.strata.len() {
        let identity =
            stats_identity::root_index_identity(grammar, pg_grammar::model::StratumId(index as u8));
        add(identity, None, Vec::new())?;
    }
    add(stats_identity::guesser_identity(grammar), None, Vec::new())?;
    for phase in OverlayPhase::ALL {
        add(
            stats_identity::overlay_identity(grammar, phase),
            None,
            Vec::new(),
        )?;
    }
    Ok(identities)
}

fn merge_expected_object(
    identities: &mut BTreeMap<(String, String), ExpectedObject>,
    mut incoming: ExpectedObject,
) -> Result<(), FactsError> {
    let map_key = (incoming.kind.as_str().to_string(), incoming.key.clone());
    let Some(existing) = identities.get_mut(&map_key) else {
        incoming.sources.sort();
        incoming.sources.dedup();
        identities.insert(map_key, incoming);
        return Ok(());
    };

    if existing.quality != incoming.quality || existing.morpheme != incoming.morpheme {
        return Err(FactsError::StatsRunUnavailable(format!(
            "compiled objects share stats identity {:?} but disagree on identity quality or morpheme owner",
            incoming.key
        )));
    }
    existing.labels.append(&mut incoming.labels);
    existing.sources.append(&mut incoming.sources);
    existing.sources.sort();
    existing.sources.dedup();
    Ok(())
}

struct SourceContext<'a> {
    snapshot: &'a Snapshot,
    grammar: &'a Grammar,
    msa_guids: &'a BTreeSet<String>,
    entries_by_allomorph: &'a BTreeMap<String, String>,
    allomorph_morph_types: &'a BTreeMap<String, MorphType>,
}

fn append_morpheme_sources(
    context: &SourceContext<'_>,
    morpheme_id: pg_grammar::model::MorphemeId,
    rule_allomorphs: &[pg_grammar::model::AffixAllomorphDef],
    sources: &mut Vec<SourceBridge>,
) -> Result<(), FactsError> {
    let morpheme = &context.grammar.morphemes[morpheme_id.0 as usize];
    if let Some(guid) = morpheme.source_msa_guid.as_deref() {
        if !context.msa_guids.contains(guid) {
            return Err(FactsError::StatsRunUnavailable(format!(
                "compiled MSA source {guid:?} is absent from the exact Snapshot"
            )));
        }
        sources.push(("msa".into(), guid.into(), "morpheme_msa".into()));
        for entry in &context.snapshot.lexicon.entries {
            if entry.msas.iter().any(|msa| msa.guid() == guid) {
                sources.push(("entry".into(), entry.guid.clone(), "msa_owner_entry".into()));
            }
        }
    }
    for rule_allomorph in rule_allomorphs {
        if let Some(source) = context
            .grammar
            .allomorph_sources
            .get(rule_allomorph.id.0 as usize)
        {
            for guid in source.form_guids.iter().flatten() {
                if !context.allomorph_morph_types.contains_key(guid) {
                    return Err(FactsError::StatsRunUnavailable(format!(
                        "compiled allomorph source {guid:?} is absent from the exact Snapshot"
                    )));
                }
                if let Some(entry_guid) = context.entries_by_allomorph.get(guid) {
                    sources.push((
                        "entry".into(),
                        entry_guid.clone(),
                        "source_form_owner_entry".into(),
                    ));
                }
            }
        }
    }
    sources.sort();
    sources.dedup();
    Ok(())
}

fn grammar_kind(kind: GrammarObjectKind) -> pg_stats::ObjectKind {
    match kind {
        GrammarObjectKind::MorphRule => pg_stats::ObjectKind::MorphRule,
        GrammarObjectKind::PhonRule => pg_stats::ObjectKind::PhonRule,
        GrammarObjectKind::LexEntry => pg_stats::ObjectKind::LexEntry,
        GrammarObjectKind::RootIndex => pg_stats::ObjectKind::RootIndex,
        GrammarObjectKind::Guesser => pg_stats::ObjectKind::Guesser,
        GrammarObjectKind::Overlay => pg_stats::ObjectKind::Overlay,
    }
}

fn quality(quality: GrammarIdentityQuality) -> &'static str {
    match quality {
        GrammarIdentityQuality::Authored => "authored",
        GrammarIdentityQuality::Structural => "structural",
        GrammarIdentityQuality::Synthetic => "synthetic",
    }
}

fn remap_dimensions(
    raw: Vec<RawDimension>,
    expected: &BTreeMap<String, ExpectedLocator>,
    name: &str,
) -> Result<Vec<ExportDimension>, FactsError> {
    validate_sentinel(&raw, name)?;
    let mut sorted = raw
        .into_iter()
        .filter(|(id, _, _)| *id != 0)
        .collect::<Vec<_>>();
    sorted.sort_by(|a, b| a.1.cmp(&b.1));
    let mut result = vec![ExportDimension {
        id: 0,
        key: None,
        label: Some(
            if name == "stratum" {
                "not applicable"
            } else {
                "NONE"
            }
            .into(),
        ),
        identity_quality: None,
    }];
    for (index, (_, key, label)) in sorted.into_iter().enumerate() {
        let key = key.ok_or_else(|| {
            FactsError::StatsRunUnavailable(format!(
                "{name} dimension has a NULL key outside its sentinel"
            ))
        })?;
        let label = label.ok_or_else(|| {
            FactsError::StatsRunUnavailable(format!("{name} dimension {key:?} has a NULL label"))
        })?;
        let identity = expected.get(&key).ok_or_else(|| {
            FactsError::StatsRunUnavailable(format!(
                "{name} dimension {key:?} has no typed identity"
            ))
        })?;
        if identity.label != label {
            return Err(FactsError::StatsRunUnavailable(format!(
                "{name} dimension {key:?} label differs from its typed identity"
            )));
        }
        result.push(ExportDimension {
            id: i64::try_from(index + 1).map_err(|_| {
                FactsError::StatsRunUnavailable(format!("too many {name} dimensions"))
            })?,
            key: Some(key),
            label: Some(label),
            identity_quality: Some(identity.quality),
        });
    }
    Ok(result)
}

fn remap_allomorphs(
    raw: Vec<RawDimension>,
    expected: &BTreeMap<String, ExpectedLocator>,
) -> Result<Vec<ExportAllomorph>, FactsError> {
    validate_sentinel(&raw, "allomorph")?;
    let mut sorted = raw
        .into_iter()
        .filter(|(id, _, _)| *id != 0)
        .collect::<Vec<_>>();
    sorted.sort_by(|a, b| a.1.cmp(&b.1));
    let mut result = vec![ExportAllomorph {
        id: 0,
        key: None,
        label: Some("NONE".into()),
        identity_quality: None,
    }];
    for (index, (_, key, label)) in sorted.into_iter().enumerate() {
        let key = key.ok_or_else(|| {
            FactsError::StatsRunUnavailable(
                "allomorph dimension has a NULL key outside its sentinel".into(),
            )
        })?;
        let label = label.ok_or_else(|| {
            FactsError::StatsRunUnavailable(format!("allomorph dimension {key:?} has a NULL label"))
        })?;
        let identity = expected.get(&key).ok_or_else(|| {
            FactsError::StatsRunUnavailable(format!(
                "allomorph dimension {key:?} has no typed identity"
            ))
        })?;
        if identity.label != label {
            return Err(FactsError::StatsRunUnavailable(format!(
                "allomorph dimension {key:?} label differs from its typed identity"
            )));
        }
        result.push(ExportAllomorph {
            id: i64::try_from(index + 1).map_err(|_| {
                FactsError::StatsRunUnavailable("too many allomorph dimensions".into())
            })?,
            key: Some(key),
            label: Some(label),
            identity_quality: Some(identity.quality),
        });
    }
    Ok(result)
}

fn raw_to_local(
    raw: &[RawDimension],
    local: &[ExportDimension],
    name: &str,
) -> Result<HashMap<i64, i64>, FactsError> {
    raw.iter()
        .map(|(raw_id, key, _)| {
            if *raw_id == 0 {
                return Ok((0, 0));
            }
            let key = key.as_deref().ok_or_else(|| {
                FactsError::StatsRunUnavailable(format!("{name} row {raw_id} has no key"))
            })?;
            let local_id = local
                .iter()
                .find(|dimension| dimension.key.as_deref() == Some(key))
                .map(|dimension| dimension.id)
                .ok_or_else(|| {
                    FactsError::StatsRunUnavailable(format!("{name} row {key:?} was not remapped"))
                })?;
            Ok((*raw_id, local_id))
        })
        .collect()
}

fn raw_allomorph_to_local(
    raw: &[RawDimension],
    local: &[ExportAllomorph],
) -> Result<HashMap<i64, i64>, FactsError> {
    raw.iter()
        .map(|(raw_id, key, _)| {
            if *raw_id == 0 {
                return Ok((0, 0));
            }
            let key = key.as_deref().ok_or_else(|| {
                FactsError::StatsRunUnavailable(format!("allomorph row {raw_id} has no key"))
            })?;
            let local_id = local
                .iter()
                .find(|dimension| dimension.key.as_deref() == Some(key))
                .map(|dimension| dimension.id)
                .ok_or_else(|| {
                    FactsError::StatsRunUnavailable(format!(
                        "allomorph row {key:?} was not remapped"
                    ))
                })?;
            Ok((*raw_id, local_id))
        })
        .collect()
}

fn validate_sentinel(raw: &[RawDimension], name: &str) -> Result<(), FactsError> {
    let sentinels: Vec<_> = raw.iter().filter(|(id, _, _)| *id == 0).collect();
    if sentinels.len() != 1 || sentinels[0].1.is_some() {
        return Err(FactsError::StatsRunUnavailable(format!(
            "{name} dimension does not contain exactly one NULL-key sentinel at handle 0"
        )));
    }
    Ok(())
}

pub(crate) fn insert_catalog(
    tx: &rusqlite::Transaction<'_>,
    counter_semantics: i64,
) -> Result<(), FactsError> {
    use pg_rules::stats::{CounterSupport, Direction, ObjectKind};
    for kind in [
        ObjectKind::MorphRule,
        ObjectKind::PhonRule,
        ObjectKind::LexEntry,
        ObjectKind::RootIndex,
        ObjectKind::Guesser,
        ObjectKind::Overlay,
    ] {
        for counter in LOGICAL_COUNTERS {
            let support = match pg_rules::stats::counter_support_for_semantics(
                kind,
                counter,
                counter_semantics,
            )
            .map_err(|version| {
                FactsError::StatsRunUnavailable(format!(
                    "unsupported counter semantics version {version}"
                ))
            })? {
                CounterSupport::Measured => "measured",
                CounterSupport::NotApplicable => "not_applicable",
                CounterSupport::NotWired => "not_wired",
            };
            tx.execute(
                "INSERT INTO stats_counter_support(engine, counter_semantics, object_kind, counter, direction, support) VALUES ('hc', ?1, ?2, ?3, 'both', ?4)",
                params![counter_semantics, stats_kind_name(kind), counter, support],
            )?;
        }
        for direction in [Direction::Analysis, Direction::Synthesis] {
            let supported = pg_rules::stats::self_time_supported_in_direction(kind, direction);
            tx.execute(
                "INSERT INTO stats_counter_support(engine, counter_semantics, object_kind, counter, direction, support) VALUES ('hc', ?1, ?2, 'self_time_ns', ?3, ?4)",
                params![
                    counter_semantics,
                    stats_kind_name(kind),
                    direction.as_str(),
                    if supported { "measured" } else { "not_applicable" },
                ],
            )?;
        }
    }
    Ok(())
}

pub(crate) fn insert(
    tx: &rusqlite::Transaction<'_>,
    projection: &StatsProjection,
) -> Result<(), FactsError> {
    tx.execute(
        "INSERT INTO stats_cache_identity(singleton, cache_sha256, cache_bytes, schema_version, counter_semantics, engine, grammar_hash) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
        params![projection.cache_sha256, projection.cache_bytes, projection.schema_version, projection.counter_semantics, projection.run.engine, projection.run.grammar_hash],
    )?;
    tx.execute(
        "INSERT INTO stats_run(run_id, engine, grammar_hash, options_hash, options_json, created_utc, step_cap, cache_word_count, requested_word_count, batch_options_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![projection.run.run_id, projection.run.engine, projection.run.grammar_hash, projection.run.options_hash, projection.run.options_json, projection.run.created_utc, projection.run.step_cap, projection.run.cache_word_count, projection.run.requested_word_count as i64, projection.run.batch_options_json],
    )?;
    for dim in &projection.morphemes {
        tx.execute("INSERT INTO stats_morpheme(morpheme_id, key, label, identity_quality) VALUES (?1, ?2, ?3, ?4)", params![dim.id, dim.key, dim.label, dim.identity_quality])?;
    }
    for dim in &projection.strata {
        tx.execute("INSERT INTO stats_stratum(stratum_id, key, label, identity_quality) VALUES (?1, ?2, ?3, ?4)", params![dim.id, dim.key, dim.label, dim.identity_quality])?;
    }
    for dim in &projection.allomorphs {
        tx.execute("INSERT INTO stats_allomorph(allomorph_id, key, label, identity_quality) VALUES (?1, ?2, ?3, ?4)", params![dim.id, dim.key, dim.label, dim.identity_quality])?;
    }
    for object in &projection.objects {
        tx.execute("INSERT INTO stats_object(object_id, key, kind, label, identity_quality, morpheme_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![object.id, object.key, object.kind, object.label, object.identity_quality, object.morpheme_id])?;
    }
    for source in &projection.object_sources {
        tx.execute("INSERT INTO stats_object_source(object_id, source_kind, source_guid, role) VALUES (?1, ?2, ?3, ?4)", params![source.object_id, source.source_kind, source.source_guid, source.role])?;
    }
    for source in &projection.allomorph_sources {
        tx.execute("INSERT INTO stats_allomorph_source(allomorph_id, source_ordinal, source_allomorph_guid, role, omitted) VALUES (?1, ?2, ?3, ?4, ?5)", params![source.allomorph_id, source.source_ordinal, source.source_allomorph_guid, source.role, i64::from(source.omitted)])?;
    }
    for word in &projection.words {
        tx.execute("INSERT INTO stats_word(word_id, run_id, form, status, elapsed_ns, attempts, passes, capped, timed_out, invalid_shape) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)", params![word.id, word.run_id, word.form, word.status, word.elapsed_ns, word.attempts, word.passes, word.capped.map(i64::from), word.timed_out.map(i64::from), word.invalid_shape.map(i64::from)])?;
    }
    for fact in &projection.facts {
        tx.execute("INSERT INTO stats_fact(word_id, object_id, stratum_id, allomorph_id, direction, attempts, work, outputs, not_applied, no_root, surface_mismatch, uses, self_time_ns) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)", params![fact.word_id, fact.object_id, fact.stratum_id, fact.allomorph_id, fact.direction, fact.attempts, fact.work, fact.outputs, fact.not_applied, fact.no_root, fact.surface_mismatch, fact.uses, fact.self_time_ns])?;
    }
    tx.execute(
        "UPDATE artifact_meta SET run_manifest_sha256=?1 WHERE singleton=1",
        params![projection.manifest_sha256],
    )?;
    Ok(())
}

fn stats_kind_name(kind: pg_rules::stats::ObjectKind) -> &'static str {
    match kind {
        pg_rules::stats::ObjectKind::MorphRule => "morph_rule",
        pg_rules::stats::ObjectKind::PhonRule => "phon_rule",
        pg_rules::stats::ObjectKind::LexEntry => "lex_entry",
        pg_rules::stats::ObjectKind::RootIndex => "root_index",
        pg_rules::stats::ObjectKind::Guesser => "guesser",
        pg_rules::stats::ObjectKind::Overlay => "overlay",
    }
}

fn cache_bool(field: &'static str, value: i64) -> Result<bool, FactsError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(FactsError::StatsRunUnavailable(format!(
            "cache word has invalid {field} flag {value}"
        ))),
    }
}

fn stats_db_error(error: rusqlite::Error) -> FactsError {
    FactsError::StatsRunUnavailable(format!("read frozen stats cache: {error}"))
}

fn refuse_nonempty_wal(cache_path: &Path) -> Result<(), FactsError> {
    let mut wal_name = cache_path.as_os_str().to_os_string();
    wal_name.push("-wal");
    let wal_path = wal_name;
    match std::fs::metadata(&wal_path) {
        Ok(metadata) if metadata.len() > 0 => Err(FactsError::StatsRunUnavailable(format!(
            "frozen stats cache still has unpublished WAL bytes at {}",
            Path::new(&wal_path).display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(FactsError::StatsRunUnavailable(format!(
            "inspect frozen stats WAL {}: {error}",
            Path::new(&wal_path).display()
        ))),
    }
}

fn digest_cache(path: &Path) -> Result<(String, u64), FactsError> {
    use sha2::{Digest, Sha256};
    let mut file = File::open(path).map_err(|error| {
        FactsError::StatsRunUnavailable(format!("open stats cache {}: {error}", path.display()))
    })?;
    let bytes = file
        .metadata()
        .map_err(|error| {
            FactsError::StatsRunUnavailable(format!("stat stats cache {}: {error}", path.display()))
        })?
        .len();
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            FactsError::StatsRunUnavailable(format!("read stats cache {}: {error}", path.display()))
        })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    let hex = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok((format!("sha256:{hex}"), bytes))
}

fn sha256(bytes: &[u8]) -> String {
    pg_assess::source_sha256(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    sha256(bytes)
        .strip_prefix("sha256:")
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE_SHA256: &str = "sha256:source";
    const GRAMMAR_HASH: &str = "grammar-hash";
    const MODEL_FINGERPRINT: &str = "model-fingerprint";
    const OPTIONS_JSON: &str = "{}";

    fn manifest_value() -> serde_json::Value {
        serde_json::json!({
            "format": FORMAT,
            "version": VERSION,
            "source": {
                "kind": "snapshot",
                "source_sha256": SOURCE_SHA256,
                "grammar_hash": GRAMMAR_HASH,
                "model_fingerprint": MODEL_FINGERPRINT,
                "compile_options_json": OPTIONS_JSON,
            },
            "compiler": {
                "version": "0.6.1",
                "build_identity": "fixture-build",
            },
            "cache": {
                "sha256": "sha256:cache",
                "bytes": 10,
                "schema_version": pg_stats::SCHEMA_VERSION,
                "counter_semantics_version": pg_stats::COUNTER_SEMANTICS_VERSION,
            },
            "run": {
                "id": 1,
                "engine": "hc",
                "grammar_hash": GRAMMAR_HASH,
                "options_hash": "options-hash",
                "options_json": "{\"engine\":\"hc\"}",
            },
            "batch": {
                "engine": "hc",
                "threads": 1,
                "step_cap": "200000",
                "work_cap": 2000000,
                "search_budget_semantics": 2,
                "word_timeout_ms": null,
                "guess": false,
                "always_enforce_final_templates": false,
                "start": 0,
                "analyses_requested": false,
            },
            "input": {
                "word_count": 1,
                "word_list_sha256": sha256(br#"["kuma"]"#),
                "words": ["kuma"],
            },
            "completion": {
                "requested": 1,
                "complete": 1,
                "incomplete": 0,
                "invalid_shape": 0,
                "missing": 0,
                "words": [{
                    "index": 0,
                    "form": "kuma",
                    "status": "complete",
                    "capped": false,
                    "timed_out": false,
                    "invalid_shape": false,
                }],
            },
        })
    }

    fn fixture_manifest() -> Manifest {
        serde_json::from_value(manifest_value()).unwrap()
    }

    fn expectation() -> StatsExpectation<'static> {
        StatsExpectation {
            source_sha256: SOURCE_SHA256,
            grammar_hash: GRAMMAR_HASH,
            model_fingerprint: MODEL_FINGERPRINT,
            compile_options_json: OPTIONS_JSON,
            producer: ProducerIdentity {
                compiler_version: "0.6.1",
                source_revision: "fixture-revision",
                build_identity: "fixture-build",
            },
        }
    }

    #[test]
    fn legacy_frozen_phon_rule_zero_keeps_unwired_support() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("legacy.sqlite");
        let mut cache = pg_stats::StatsCache::open(&path, GRAMMAR_HASH)
            .unwrap()
            .cache;
        let mut manifest = fixture_manifest();
        let run_id = cache
            .flush(
                &pg_stats::RunMetadata {
                    build_info: manifest.compiler.build_identity.clone(),
                    fwdata_path: "fixture.fwdata".into(),
                    grammar_hash: GRAMMAR_HASH.into(),
                    engine: "hc".into(),
                    options_hash: manifest.run.options_hash.clone(),
                    options_json: manifest.run.options_json.clone(),
                    created_utc: "unix:1".into(),
                    step_cap: Some("200000".parse().unwrap()),
                },
                &[pg_stats::WordRecord {
                    form: "kuma".into(),
                    elapsed_ns: 1,
                    attempts: 1,
                    passes: 1,
                    capped: false,
                    timed_out: false,
                    invalid_shape: false,
                    facts: vec![pg_stats::FactRecord {
                        object_key: "phon-rule".into(),
                        object_kind: pg_stats::ObjectKind::PhonRule,
                        object_label: "Phon rule".into(),
                        identity_quality: pg_stats::IdentityQuality::Authored,
                        stratum: None,
                        allomorph: None,
                        morpheme: None,
                        direction: pg_stats::Direction::Analysis,
                        attempts: 1,
                        work: 1,
                        outputs: 0,
                        not_applied: 0,
                        no_root: 0,
                        surface_mismatch: 0,
                        uses: 0,
                        self_time_ns: 0,
                    }],
                }],
            )
            .unwrap();
        cache.checkpoint_and_close().unwrap();
        Connection::open(&path)
            .unwrap()
            .execute("UPDATE run SET counter_semantics=3", [])
            .unwrap();
        manifest.run.id = run_id;
        manifest.cache.counter_semantics_version = 3;
        let run = read_cache(&path, &manifest).unwrap();
        assert_eq!(run.counter_semantics, 3);
        assert_eq!(run.facts.len(), 1);
        assert_eq!(run.facts[0].uses, 0);

        let mut facts = Connection::open_in_memory().unwrap();
        facts.execute_batch(include_str!("schema.sql")).unwrap();
        let tx = facts.transaction().unwrap();
        insert_catalog(&tx, run.counter_semantics).unwrap();
        let support: String = tx.query_row(
            "SELECT support FROM stats_counter_support WHERE object_kind='phon_rule' AND counter='uses' AND direction='both'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(
            support, "not_wired",
            "a legacy stored zero has no measurement provenance"
        );
    }

    #[test]
    fn unknown_frozen_counter_semantics_refuses_catalog() {
        let mut facts = Connection::open_in_memory().unwrap();
        facts.execute_batch(include_str!("schema.sql")).unwrap();
        let tx = facts.transaction().unwrap();
        let error = insert_catalog(&tx, 999).unwrap_err();
        assert!(
            matches!(error, FactsError::StatsRunUnavailable(ref reason) if reason.contains("999"))
        );
        let count: i64 = tx
            .query_row("SELECT COUNT(*) FROM stats_counter_support", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn stats_manifest_is_closed_and_bound_to_the_exact_model() {
        let manifest = fixture_manifest();
        validate_manifest_context(&manifest, &expectation()).unwrap();

        let mut stale_model = fixture_manifest();
        stale_model.source.model_fingerprint = "stale-model".into();
        assert!(matches!(
            validate_manifest_context(&stale_model, &expectation()),
            Err(FactsError::StatsContextMismatch(_))
        ));

        let mut stale_version = fixture_manifest();
        stale_version.version += 1;
        assert!(matches!(
            validate_manifest_context(&stale_version, &expectation()),
            Err(FactsError::StatsContextMismatch(_))
        ));

        let mut wrong_compile_options = expectation();
        wrong_compile_options.compile_options_json = "{\"different\":true}";
        assert!(matches!(
            validate_manifest_context(&manifest, &wrong_compile_options),
            Err(FactsError::StatsContextMismatch(_))
        ));

        let mut open_manifest = manifest_value();
        open_manifest["unexpected"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<Manifest>(open_manifest).is_err());
    }

    #[test]
    fn stats_manifest_completion_and_cache_digest_must_match() {
        let manifest = fixture_manifest();
        validate_cache_digest(&manifest, "sha256:cache", 10).unwrap();
        assert!(matches!(
            validate_cache_digest(&manifest, "sha256:changed", 10),
            Err(FactsError::StatsRunUnavailable(_))
        ));
        assert!(matches!(
            validate_cache_digest(&manifest, "sha256:cache", 11),
            Err(FactsError::StatsRunUnavailable(_))
        ));

        let mut inconsistent_completion = manifest;
        inconsistent_completion.completion.complete = 0;
        assert!(matches!(
            validate_manifest_context(&inconsistent_completion, &expectation()),
            Err(FactsError::StatsRunUnavailable(_))
        ));
    }

    #[test]
    fn duplicate_compiled_rule_identities_merge_labels_and_authoritative_sources() {
        let mut identities = BTreeMap::new();
        let key = ("morph_rule".to_string(), "msa-guid".to_string());
        merge_expected_object(
            &mut identities,
            ExpectedObject {
                key: key.1.clone(),
                kind: pg_stats::ObjectKind::MorphRule,
                labels: BTreeSet::from(["z-form".to_string()]),
                quality: "authored",
                morpheme: None,
                sources: vec![("msa".into(), "msa-guid".into(), "morpheme_msa".into())],
            },
        )
        .unwrap();
        merge_expected_object(
            &mut identities,
            ExpectedObject {
                key: key.1.clone(),
                kind: pg_stats::ObjectKind::MorphRule,
                labels: BTreeSet::from(["a-form".to_string()]),
                quality: "authored",
                morpheme: None,
                sources: vec![
                    ("msa".into(), "msa-guid".into(), "morpheme_msa".into()),
                    (
                        "entry".into(),
                        "entry-guid".into(),
                        "source_form_owner_entry".into(),
                    ),
                ],
            },
        )
        .unwrap();

        let merged = &identities[&key];
        assert_eq!(
            merged.labels,
            BTreeSet::from(["a-form".into(), "z-form".into()])
        );
        assert_eq!(
            merged.sources,
            vec![
                (
                    "entry".into(),
                    "entry-guid".into(),
                    "source_form_owner_entry".into()
                ),
                ("msa".into(), "msa-guid".into(), "morpheme_msa".into()),
            ]
        );
    }
}
