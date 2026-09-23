//! The read side: report aggregation and mixed-settings detection.
//!
//! Every function here takes a plain `&Connection` (via `crate::cache::StatsCache::connection`)
//! and returns plain Rust rows — no formatting, no rendering. That is the CLI layer's job.

use rusqlite::{named_params, params, Connection};

use crate::error::StatsError;

/// One row of the per-word report: form, actual elapsed time, and outcome flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerWordRow {
    pub form: String,
    pub elapsed_ns: i64,
    pub attempts: i64,
    pub passes: i64,
    pub capped: bool,
    pub timed_out: bool,
}

/// Form, actual elapsed, attempts, passes, capped/timed-out — ordered by elapsed descending.
///
/// `elapsed_ns` is this word's whole-parse wall clock; the per-object report's `self_time_ns` is a
/// finer per-object breakdown, so summing one does not need to reproduce the other exactly.
pub fn per_word_report(conn: &Connection) -> Result<Vec<PerWordRow>, StatsError> {
    let mut stmt = conn.prepare(
        "SELECT form, elapsed_ns, attempts, passes, capped, timed_out
         FROM word
         ORDER BY elapsed_ns DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(PerWordRow {
            form: row.get(0)?,
            elapsed_ns: row.get(1)?,
            attempts: row.get(2)?,
            passes: row.get(3)?,
            capped: row.get::<_, i64>(4)? != 0,
            timed_out: row.get::<_, i64>(5)? != 0,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Total (already word-deduplicated) elapsed wall-clock across the cache, or one word's own
/// `elapsed_ns` when `word` narrows to it -- the attribution check's denominator: "how much of
/// this real time did the object rows account for?"
pub fn word_elapsed_ns_total(conn: &Connection, word: Option<&str>) -> Result<i64, StatsError> {
    conn.query_row(
        "SELECT COALESCE(SUM(elapsed_ns), 0) FROM word WHERE (:word IS NULL OR form = :word)",
        named_params! { ":word": word },
        |row| row.get(0),
    )
    .map_err(Into::into)
}

/// Whether any fact row of `kind` has ever been recorded in this cache, regardless of the current
/// report's own narrowing -- lets an empty result say whether that is because this kind never
/// occurs at all, or because it exists but nothing matched the requested filter.
pub fn kind_has_any_recorded_object(conn: &Connection, kind: &str) -> Result<bool, StatsError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM object WHERE kind = ?1",
        params![kind],
        |row| row.get(0),
    )?;
    Ok(n > 0)
}

/// Which column an object-shaped report orders by. Measured self time is the default because it
/// is the direct over-application question; the others answer the next most common hand-derived
/// question directly, without an analyst dividing two columns themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    SelfTimeNs,
    NoRoot,
    /// `outputs / attempts` — how many results one attempt amplifies into.
    Amp,
    Uses,
    Attempts,
}

fn sort_key_code(sort: SortKey) -> i64 {
    match sort {
        SortKey::SelfTimeNs => 0,
        SortKey::NoRoot => 1,
        SortKey::Amp => 2,
        SortKey::Uses => 3,
        SortKey::Attempts => 4,
    }
}

/// The `CASE :sort_key WHEN ... END` expression shared by every sortable report's CTE.
fn sort_case_sql() -> &'static str {
    "CASE :sort_key
        WHEN 1 THEN no_root
        WHEN 2 THEN CAST(outputs AS REAL) / NULLIF(attempts, 0)
        WHEN 3 THEN uses
        WHEN 4 THEN attempts
        ELSE self_time_ns
     END"
}

/// Optional narrowing for `per_object_report`. Every field left at its default means "no
/// narrowing" — the full grammar, ordered by measured self time, no limit.
#[derive(Debug, Clone, Default)]
pub struct PerObjectFilter {
    pub kind: Option<String>,
    pub object_key: Option<String>,
    pub stratum_key: Option<String>,
    /// Narrows to `"analysis"` or `"synthesis"`; `None` sums both, recovering the undirected total.
    pub direction: Option<String>,
    /// Narrows to one word's own fact rows, so a bad word can be traced straight to its objects.
    pub word: Option<String>,
    pub exclude_censored_words: bool,
    /// Per-`kind`, not global: `Some(1)` returns the top object of `morph_rule`, the top of
    /// `phon_rule`, and so on, rather than hiding every kind but the one with the largest values.
    pub sort: SortKey,
}

/// One row of the per-object report: identity, summed counters, and measured self time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerObjectRow {
    pub kind: String,
    pub label: String,
    pub identity_quality: String,
    pub attempts: i64,
    pub work: i64,
    pub outputs: i64,
    /// Rule-level only (the `allomorph_id = 0` row): one per invocation that produced nothing, matching `attempts`'s granularity. The per-allomorph breakdown lives in `PerAllomorphRow` instead.
    pub not_applied: i64,
    pub no_root: i64,
    pub surface_mismatch: i64,
    pub uses: i64,
    /// Measured wall-clock self time summed across every row this object owns --
    /// `pg_rules::stats::StatsCollector::time_enter`'s tier. Exact, not apportioned: each row's
    /// `self_time_ns` was booked directly at that object, so this sum needs no division.
    pub self_time_ns: i64,
}

fn per_object_sql() -> String {
    format!(
        "WITH agg AS (
            SELECT o.object_id, o.kind, o.label, o.identity_quality, o.key AS object_key_sort,
                   SUM(f.attempts) AS attempts, SUM(f.work) AS work, SUM(f.outputs) AS outputs,
                   SUM(CASE WHEN f.allomorph_id = 0 THEN f.not_applied ELSE 0 END) AS not_applied,
                   SUM(f.no_root) AS no_root, SUM(f.surface_mismatch) AS surface_mismatch,
                   SUM(f.uses) AS uses, SUM(f.self_time_ns) AS self_time_ns
            FROM fact f
            JOIN object o ON o.object_id = f.object_id
            JOIN word w ON w.word_id = f.word_id
            LEFT JOIN stratum s ON s.stratum_id = f.stratum_id
            WHERE (:kind IS NULL OR o.kind = :kind)
              AND (:object_key IS NULL OR o.key = :object_key)
              AND (:stratum_key IS NULL OR s.key = :stratum_key)
              AND (:direction IS NULL OR f.direction = :direction)
              AND (:word IS NULL OR w.form = :word)
              AND (:exclude_censored = 0 OR (w.capped = 0 AND w.timed_out = 0))
            GROUP BY o.object_id
        )
        SELECT kind, label, identity_quality, attempts, work, outputs, not_applied, no_root,
               surface_mismatch, uses, self_time_ns
        FROM agg
        ORDER BY {sort} DESC, object_key_sort ASC",
        sort = sort_case_sql()
    )
}

/// Kind, label, identity quality, summed counters, and measured self time — sorted and filtered
/// per `filter`, unlimited: a caller that shows only the top rows must still sum every row, or
/// each row's share is a share of the excerpt rather than of the run.
pub fn per_object_report(
    conn: &Connection,
    filter: &PerObjectFilter,
) -> Result<Vec<PerObjectRow>, StatsError> {
    let sql = per_object_sql();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        named_params! {
            ":kind": filter.kind.as_deref(),
            ":object_key": filter.object_key.as_deref(),
            ":stratum_key": filter.stratum_key.as_deref(),
            ":direction": filter.direction.as_deref(),
            ":word": filter.word.as_deref(),
            ":exclude_censored": i64::from(filter.exclude_censored_words),
            ":sort_key": sort_key_code(filter.sort),
        },
        |row| {
            Ok(PerObjectRow {
                kind: row.get(0)?,
                label: row.get(1)?,
                identity_quality: row.get(2)?,
                attempts: row.get(3)?,
                work: row.get(4)?,
                outputs: row.get(5)?,
                not_applied: row.get(6)?,
                no_root: row.get(7)?,
                surface_mismatch: row.get(8)?,
                uses: row.get(9)?,
                self_time_ns: row.get(10)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Optional narrowing for `per_allomorph_report`. Every field left at its default means "no
/// narrowing" — every object's allomorph breakdown, ordered by measured self time, no limit.
#[derive(Debug, Clone, Default)]
pub struct PerAllomorphFilter {
    pub kind: Option<String>,
    pub object_key: Option<String>,
    /// Narrows to `"analysis"` or `"synthesis"`; `None` sums both.
    pub direction: Option<String>,
    pub word: Option<String>,
    pub exclude_censored_words: bool,
}

/// One row of the per-allomorph report: the owning object's identity, the allomorph locator, and
/// the same summed counters `per_object_report` returns.
///
/// A row is keyed by `(object, allomorph)`, so `allomorph_key` is `None` only for that object's
/// `NONE` sentinel (`allomorph_id = 0`) — the residue its allomorph rows would otherwise fail to
/// add up to, not a missing value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerAllomorphRow {
    pub object_kind: String,
    pub object_label: String,
    pub allomorph_key: Option<String>,
    pub allomorph_label: String,
    pub attempts: i64,
    pub work: i64,
    pub outputs: i64,
    pub not_applied: i64,
    pub no_root: i64,
    pub surface_mismatch: i64,
    pub uses: i64,
    /// Measured wall-clock self time summed across this `(object, allomorph)` pair's rows.
    pub self_time_ns: i64,
}

const PER_ALLOMORPH_SQL: &str = "
    WITH agg AS (
        SELECT o.kind AS object_kind, o.label AS object_label, o.key AS object_key_sort,
               a.key AS allomorph_key, a.label AS allomorph_label,
               SUM(f.attempts) AS attempts, SUM(f.work) AS work, SUM(f.outputs) AS outputs,
               -- Deliberately unmasked: this report groups BY allomorph, so each row is one allomorph's own count and the rule-level residue is its own `NONE` row.
               SUM(f.not_applied) AS not_applied, SUM(f.no_root) AS no_root,
               SUM(f.surface_mismatch) AS surface_mismatch, SUM(f.uses) AS uses,
               SUM(f.self_time_ns) AS self_time_ns
        FROM fact f
        JOIN object o ON o.object_id = f.object_id
        JOIN word w ON w.word_id = f.word_id
        LEFT JOIN allomorph a ON a.allomorph_id = f.allomorph_id
        WHERE (:kind IS NULL OR o.kind = :kind)
          AND (:object_key IS NULL OR o.key = :object_key)
          AND (:direction IS NULL OR f.direction = :direction)
          AND (:word IS NULL OR w.form = :word)
          AND (:exclude_censored = 0 OR (w.capped = 0 AND w.timed_out = 0))
        GROUP BY o.object_id, f.allomorph_id
    )
    SELECT object_kind, object_label, allomorph_key, allomorph_label, attempts, work, outputs,
           not_applied, no_root, surface_mismatch, uses, self_time_ns
    FROM agg
    ORDER BY self_time_ns DESC, object_key_sort ASC, allomorph_key ASC
";

/// Object identity, allomorph locator, summed counters, and measured self time — one row per
/// `(object, allomorph)` pair with a fact, including each object's `NONE` sentinel residue.
pub fn per_allomorph_report(
    conn: &Connection,
    filter: &PerAllomorphFilter,
) -> Result<Vec<PerAllomorphRow>, StatsError> {
    let mut stmt = conn.prepare(PER_ALLOMORPH_SQL)?;
    let rows = stmt.query_map(
        named_params! {
            ":kind": filter.kind.as_deref(),
            ":object_key": filter.object_key.as_deref(),
            ":direction": filter.direction.as_deref(),
            ":word": filter.word.as_deref(),
            ":exclude_censored": i64::from(filter.exclude_censored_words),
        },
        |row| {
            Ok(PerAllomorphRow {
                object_kind: row.get(0)?,
                object_label: row.get(1)?,
                allomorph_key: row.get(2)?,
                allomorph_label: row.get(3)?,
                attempts: row.get(4)?,
                work: row.get(5)?,
                outputs: row.get(6)?,
                not_applied: row.get(7)?,
                no_root: row.get(8)?,
                surface_mismatch: row.get(9)?,
                uses: row.get(10)?,
                self_time_ns: row.get(11)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// `not_applied` is masked to each rule's own `ALLOMORPH_NONE` row, since it is recorded at two granularities and a raw SUM turns one failure into `1 + allomorphs_reached`.
const SUMMED_COUNTERS: &str = "SUM(f.attempts), SUM(f.work), SUM(f.outputs),
           SUM(CASE WHEN f.allomorph_id = 0 THEN f.not_applied ELSE 0 END),
           SUM(f.no_root), SUM(f.surface_mismatch), SUM(f.uses), SUM(f.self_time_ns)";

/// Optional narrowing for `per_kind_report`. Every field left at its default means "no narrowing".
#[derive(Debug, Clone, Default)]
pub struct PerKindFilter {
    pub kind: Option<String>,
    pub direction: Option<String>,
    pub word: Option<String>,
    pub exclude_censored_words: bool,
}

/// One row of the `group` orientation: one `ObjectKind`'s counters, summed across every object of
/// that kind -- "was it the compounding, the lexemes, or the allomorphs?" in one glance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerKindRow {
    pub kind: String,
    pub attempts: i64,
    pub work: i64,
    pub outputs: i64,
    pub not_applied: i64,
    pub no_root: i64,
    pub surface_mismatch: i64,
    pub uses: i64,
    pub self_time_ns: i64,
}

fn per_kind_sql() -> String {
    format!(
        "
    SELECT o.kind,
           {SUMMED_COUNTERS}
    FROM fact f
    JOIN object o ON o.object_id = f.object_id
    JOIN word w ON w.word_id = f.word_id
    WHERE (:kind IS NULL OR o.kind = :kind)
      AND (:direction IS NULL OR f.direction = :direction)
      AND (:word IS NULL OR w.form = :word)
      AND (:exclude_censored = 0 OR (w.capped = 0 AND w.timed_out = 0))
    GROUP BY o.kind
    ORDER BY SUM(f.self_time_ns) DESC, o.kind ASC
"
    )
}

/// One row per `ObjectKind` with a fact, summed across every object of that kind.
pub fn per_kind_report(
    conn: &Connection,
    filter: &PerKindFilter,
) -> Result<Vec<PerKindRow>, StatsError> {
    let mut stmt = conn.prepare(&per_kind_sql())?;
    let rows = stmt.query_map(
        named_params! {
            ":kind": filter.kind.as_deref(),
            ":direction": filter.direction.as_deref(),
            ":word": filter.word.as_deref(),
            ":exclude_censored": i64::from(filter.exclude_censored_words),
        },
        |row| {
            Ok(PerKindRow {
                kind: row.get(0)?,
                attempts: row.get(1)?,
                work: row.get(2)?,
                outputs: row.get(3)?,
                not_applied: row.get(4)?,
                no_root: row.get(5)?,
                surface_mismatch: row.get(6)?,
                uses: row.get(7)?,
                self_time_ns: row.get(8)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Optional narrowing for `per_morpheme_report`. Always scoped to `lex_entry` objects; every
/// other field left at its default means "no narrowing".
#[derive(Debug, Clone, Default)]
pub struct PerMorphemeFilter {
    pub direction: Option<String>,
    pub word: Option<String>,
    pub exclude_censored_words: bool,
}

/// One row of the `morpheme` orientation: a morpheme's locator and the summed counters of every
/// `lex_entry` object (and its allomorph rows) that realizes it. `morpheme_key` is `None` only for
/// the `NONE` sentinel -- a `lex_entry` whose morpheme could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerMorphemeRow {
    pub morpheme_key: Option<String>,
    pub morpheme_label: String,
    pub attempts: i64,
    pub work: i64,
    pub outputs: i64,
    pub not_applied: i64,
    pub no_root: i64,
    pub surface_mismatch: i64,
    pub uses: i64,
    pub self_time_ns: i64,
}

fn per_morpheme_sql() -> String {
    format!(
        "
    SELECT m.key, m.label,
           {SUMMED_COUNTERS}
    FROM fact f
    JOIN object o ON o.object_id = f.object_id
    JOIN word w ON w.word_id = f.word_id
    LEFT JOIN morpheme m ON m.morpheme_id = o.morpheme_id
    WHERE o.kind = 'lex_entry'
      AND (:direction IS NULL OR f.direction = :direction)
      AND (:word IS NULL OR w.form = :word)
      AND (:exclude_censored = 0 OR (w.capped = 0 AND w.timed_out = 0))
    GROUP BY o.morpheme_id
    ORDER BY SUM(f.self_time_ns) DESC, m.key ASC
"
    )
}

/// Morpheme locator and summed counters — one row per morpheme with a `lex_entry` fact, so
/// entries and allomorphs scattered across several rows collapse to the morpheme they realize.
pub fn per_morpheme_report(
    conn: &Connection,
    filter: &PerMorphemeFilter,
) -> Result<Vec<PerMorphemeRow>, StatsError> {
    let mut stmt = conn.prepare(&per_morpheme_sql())?;
    let rows = stmt.query_map(
        named_params! {
            ":direction": filter.direction.as_deref(),
            ":word": filter.word.as_deref(),
            ":exclude_censored": i64::from(filter.exclude_censored_words),
        },
        |row| {
            Ok(PerMorphemeRow {
                morpheme_key: row.get(0)?,
                morpheme_label: row.get(1)?,
                attempts: row.get(2)?,
                work: row.get(3)?,
                outputs: row.get(4)?,
                not_applied: row.get(5)?,
                no_root: row.get(6)?,
                surface_mismatch: row.get(7)?,
                uses: row.get(8)?,
                self_time_ns: row.get(9)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Below this many attempts, "never fires" is indistinguishable from ordinary noise, so the
/// default excludes it rather than defaulting to 0 -- the sensible-default this report needs to
/// be actionable rather than a dump of every rarely-tried object. Internal to this report only
/// (the CLI carries no general `--min-attempts` override); see `NeverFiresFilter`.
pub const NEVER_FIRES_DEFAULT_MIN_ATTEMPTS: i64 = 1000;

/// Optional narrowing for `never_fires_report`. `min_attempts` defaults to
/// `NEVER_FIRES_DEFAULT_MIN_ATTEMPTS`, not 0 -- see that constant's doc.
#[derive(Debug, Clone)]
pub struct NeverFiresFilter {
    pub kind: Option<String>,
    pub direction: Option<String>,
    pub word: Option<String>,
    pub min_attempts: i64,
    pub exclude_censored_words: bool,
}

impl Default for NeverFiresFilter {
    fn default() -> Self {
        NeverFiresFilter {
            kind: None,
            direction: None,
            word: None,
            min_attempts: NEVER_FIRES_DEFAULT_MIN_ATTEMPTS,
            exclude_censored_words: false,
        }
    }
}

/// One row of the never-fires report: an object heavily attempted in `direction` that produced
/// zero outputs there. Direction-scoped rather than undirected, so an object that fires only in
/// the other direction never appears -- see `NeverFiresFilter`'s doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeverFiresRow {
    pub kind: String,
    pub label: String,
    pub identity_quality: String,
    pub direction: String,
    pub attempts: i64,
}

/// Scoped to the kinds whose collector wires an `outputs` counter at all; `lex_entry` never populates it, so "zero outputs" would be an artifact there, not a finding.
const NEVER_FIRES_SQL: &str = "
    SELECT o.kind, o.label, o.identity_quality, f.direction, SUM(f.attempts) AS total_attempts
    FROM fact f
    JOIN object o ON o.object_id = f.object_id
    JOIN word w ON w.word_id = f.word_id
    WHERE o.kind IN ('morph_rule', 'phon_rule')
      AND (:kind IS NULL OR o.kind = :kind)
      AND (:direction IS NULL OR f.direction = :direction)
      AND (:word IS NULL OR w.form = :word)
      AND (:exclude_censored = 0 OR (w.capped = 0 AND w.timed_out = 0))
    GROUP BY o.object_id, f.direction
    HAVING SUM(f.outputs) = 0 AND total_attempts >= :min_attempts
    ORDER BY total_attempts DESC, o.key ASC, f.direction ASC
";

/// Objects heavily attempted (`>= filter.min_attempts`) that produced zero outputs, in the
/// direction they were attempted -- the single most actionable fact this cache can surface: a
/// rule entered hundreds of thousands of times for nothing.
pub fn never_fires_report(
    conn: &Connection,
    filter: &NeverFiresFilter,
) -> Result<Vec<NeverFiresRow>, StatsError> {
    let mut stmt = conn.prepare(NEVER_FIRES_SQL)?;
    let rows = stmt.query_map(
        named_params! {
            ":kind": filter.kind.as_deref(),
            ":direction": filter.direction.as_deref(),
            ":word": filter.word.as_deref(),
            ":exclude_censored": i64::from(filter.exclude_censored_words),
            ":min_attempts": filter.min_attempts,
        },
        |row| {
            Ok(NeverFiresRow {
                kind: row.get(0)?,
                label: row.get(1)?,
                identity_quality: row.get(2)?,
                direction: row.get(3)?,
                attempts: row.get(4)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Whether the cache holds rows computed under more than one `options_hash` or
/// `counter_semantics` — either makes a plain `SUM` across the whole cache misleading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixedSettings {
    pub distinct_options_hashes: i64,
    pub distinct_counter_semantics: i64,
}

impl MixedSettings {
    pub fn is_mixed(&self) -> bool {
        self.distinct_options_hashes > 1 || self.distinct_counter_semantics > 1
    }
}

/// Reports whether a query over this cache would silently span mixed collector settings.
pub fn mixed_settings(conn: &Connection) -> Result<MixedSettings, StatsError> {
    conn.query_row(
        "SELECT COUNT(DISTINCT options_hash), COUNT(DISTINCT counter_semantics) FROM run",
        [],
        |row| {
            Ok(MixedSettings {
                distinct_options_hashes: row.get(0)?,
                distinct_counter_semantics: row.get(1)?,
            })
        },
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests;
