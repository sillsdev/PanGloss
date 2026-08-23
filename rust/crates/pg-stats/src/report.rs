//! The read side: the two v1 reports, mixed-settings detection, and coverage.
//!
//! Every function here takes a plain `&Connection` (via `crate::cache::StatsCache::connection`)
//! and returns plain Rust rows — no formatting, no CSV. Rendering is the CLI layer's job.

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
/// Actual time only; this report never appears alongside the per-object report's *estimated*
/// time in the same table (they will disagree, and that disagreement is not a bug).
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

/// Which column `per_object_report` orders by. Estimated time is the default because it is the
/// direct over-application question; `no_root` answers "which object manufactures bogus forms"
/// directly, without an analyst dividing two columns by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    EstimatedNs,
    NoRoot,
}

impl Default for SortKey {
    fn default() -> Self {
        SortKey::EstimatedNs
    }
}

/// Optional narrowing for `per_object_report`. Every field left at its default means "no
/// narrowing" — the full grammar, ordered by estimated time, no limit.
#[derive(Debug, Clone, Default)]
pub struct PerObjectFilter {
    pub kind: Option<String>,
    pub object_key: Option<String>,
    pub stratum_key: Option<String>,
    /// Narrows to `"analysis"` or `"synthesis"`; `None` sums both, recovering the undirected total.
    pub direction: Option<String>,
    pub min_attempts: Option<i64>,
    pub exclude_censored_words: bool,
    pub top_n: Option<usize>,
    pub sort: SortKey,
}

/// One row of the per-object report: identity, summed counters, and an estimated-time column.
///
/// `estimated_ns` is `None` when no `op_cost` row exists for this object's kind — an unmeasured
/// calibration, not a zero-cost claim.
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
    pub estimated_ns: Option<i64>,
}

const PER_OBJECT_SQL: &str = "
    SELECT o.kind, o.label, o.identity_quality,
           SUM(f.attempts), SUM(f.work), SUM(f.outputs),
           SUM(CASE WHEN f.allomorph_id = 0 THEN f.not_applied ELSE 0 END),
           SUM(f.no_root), SUM(f.surface_mismatch), SUM(f.uses),
           SUM(f.work) * oc.ns_per_unit AS estimated_ns
    FROM fact f
    JOIN object o ON o.object_id = f.object_id
    JOIN word w ON w.word_id = f.word_id
    LEFT JOIN op_cost oc ON oc.kind = o.kind
    LEFT JOIN stratum s ON s.stratum_id = f.stratum_id
    WHERE (:kind IS NULL OR o.kind = :kind)
      AND (:object_key IS NULL OR o.key = :object_key)
      AND (:stratum_key IS NULL OR s.key = :stratum_key)
      AND (:direction IS NULL OR f.direction = :direction)
      AND (:exclude_censored = 0 OR (w.capped = 0 AND w.timed_out = 0))
    GROUP BY o.object_id
    HAVING (:min_attempts IS NULL OR SUM(f.attempts) >= :min_attempts)
    -- An uncalibrated kind must not sink to the bottom on a NULL: raw work is the
    -- calibration-independent ranking, and within one kind the constant only rescales it.
    ORDER BY CASE WHEN :sort_no_root = 1 THEN SUM(f.no_root)
                  ELSE SUM(f.work) * COALESCE(oc.ns_per_unit, 1) END DESC
    LIMIT :limit
";

/// Kind, label, identity quality, summed counters, and estimated time — sorted and filtered per
/// `filter`. Object count is bounded by grammar size, so an unset `top_n` prints everything.
pub fn per_object_report(
    conn: &Connection,
    filter: &PerObjectFilter,
) -> Result<Vec<PerObjectRow>, StatsError> {
    let mut stmt = conn.prepare(PER_OBJECT_SQL)?;
    let limit: i64 = filter.top_n.map(|n| n as i64).unwrap_or(-1);
    let rows = stmt.query_map(
        named_params! {
            ":kind": filter.kind.as_deref(),
            ":object_key": filter.object_key.as_deref(),
            ":stratum_key": filter.stratum_key.as_deref(),
            ":direction": filter.direction.as_deref(),
            ":exclude_censored": i64::from(filter.exclude_censored_words),
            ":min_attempts": filter.min_attempts,
            ":sort_no_root": i64::from(filter.sort == SortKey::NoRoot),
            ":limit": limit,
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
                estimated_ns: row.get(10)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Optional narrowing for `per_allomorph_report`. Every field left at its default means "no
/// narrowing" — every object's allomorph breakdown, ordered by estimated time, no limit.
#[derive(Debug, Clone, Default)]
pub struct PerAllomorphFilter {
    pub kind: Option<String>,
    pub object_key: Option<String>,
    /// Narrows to `"analysis"` or `"synthesis"`; `None` sums both.
    pub direction: Option<String>,
    pub min_attempts: Option<i64>,
    pub exclude_censored_words: bool,
    pub top_n: Option<usize>,
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
    pub estimated_ns: Option<i64>,
}

const PER_ALLOMORPH_SQL: &str = "
    SELECT o.kind, o.label, a.key, a.label,
           SUM(f.attempts), SUM(f.work), SUM(f.outputs), SUM(f.not_applied),
           SUM(f.no_root), SUM(f.surface_mismatch), SUM(f.uses),
           SUM(f.work) * oc.ns_per_unit AS estimated_ns
    FROM fact f
    JOIN object o ON o.object_id = f.object_id
    JOIN word w ON w.word_id = f.word_id
    LEFT JOIN allomorph a ON a.allomorph_id = f.allomorph_id
    LEFT JOIN op_cost oc ON oc.kind = o.kind
    WHERE (:kind IS NULL OR o.kind = :kind)
      AND (:object_key IS NULL OR o.key = :object_key)
      AND (:direction IS NULL OR f.direction = :direction)
      AND (:exclude_censored = 0 OR (w.capped = 0 AND w.timed_out = 0))
    GROUP BY o.object_id, f.allomorph_id
    HAVING (:min_attempts IS NULL OR SUM(f.attempts) >= :min_attempts)
    ORDER BY SUM(f.work) * COALESCE(oc.ns_per_unit, 1) DESC, o.key ASC, a.key ASC
    LIMIT :limit
";

/// Object identity, allomorph locator, summed counters, and estimated time — one row per
/// `(object, allomorph)` pair with a fact, including each object's `NONE` sentinel residue.
pub fn per_allomorph_report(
    conn: &Connection,
    filter: &PerAllomorphFilter,
) -> Result<Vec<PerAllomorphRow>, StatsError> {
    let mut stmt = conn.prepare(PER_ALLOMORPH_SQL)?;
    let limit: i64 = filter.top_n.map(|n| n as i64).unwrap_or(-1);
    let rows = stmt.query_map(
        named_params! {
            ":kind": filter.kind.as_deref(),
            ":object_key": filter.object_key.as_deref(),
            ":direction": filter.direction.as_deref(),
            ":exclude_censored": i64::from(filter.exclude_censored_words),
            ":min_attempts": filter.min_attempts,
            ":limit": limit,
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
                estimated_ns: row.get(11)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Optional narrowing for `per_stratum_report`. Every field left at its default means "no
/// narrowing" — every stratum across the whole grammar, no limit.
#[derive(Debug, Clone, Default)]
pub struct PerStratumFilter {
    pub kind: Option<String>,
    pub object_key: Option<String>,
    /// Narrows to `"analysis"` or `"synthesis"`; `None` sums both.
    pub direction: Option<String>,
    pub min_attempts: Option<i64>,
    pub exclude_censored_words: bool,
    pub top_n: Option<usize>,
}

/// One row of the per-stratum report: the stratum locator and the same summed counters
/// `per_object_report` returns, summed across every object that contributed to this stratum.
///
/// `stratum_key` is `None` for the not-applicable sentinel row (`stratum_id = 0`), which must
/// appear rather than be dropped so a per-object sum over strata still reaches the object's total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerStratumRow {
    pub stratum_key: Option<String>,
    pub stratum_label: String,
    pub attempts: i64,
    pub work: i64,
    pub outputs: i64,
    pub not_applied: i64,
    pub no_root: i64,
    pub surface_mismatch: i64,
    pub uses: i64,
}

const PER_STRATUM_SQL: &str = "
    SELECT s.key, s.label,
           SUM(f.attempts), SUM(f.work), SUM(f.outputs), SUM(f.not_applied),
           SUM(f.no_root), SUM(f.surface_mismatch), SUM(f.uses)
    FROM fact f
    JOIN object o ON o.object_id = f.object_id
    JOIN word w ON w.word_id = f.word_id
    LEFT JOIN stratum s ON s.stratum_id = f.stratum_id
    WHERE (:kind IS NULL OR o.kind = :kind)
      AND (:object_key IS NULL OR o.key = :object_key)
      AND (:direction IS NULL OR f.direction = :direction)
      AND (:exclude_censored = 0 OR (w.capped = 0 AND w.timed_out = 0))
    GROUP BY f.stratum_id
    HAVING (:min_attempts IS NULL OR SUM(f.attempts) >= :min_attempts)
    ORDER BY SUM(f.work) DESC, s.key ASC
    LIMIT :limit
";

/// Stratum locator and summed counters — one row per stratum with a fact, including the
/// not-applicable sentinel, optionally narrowed to one object or one kind.
pub fn per_stratum_report(
    conn: &Connection,
    filter: &PerStratumFilter,
) -> Result<Vec<PerStratumRow>, StatsError> {
    let mut stmt = conn.prepare(PER_STRATUM_SQL)?;
    let limit: i64 = filter.top_n.map(|n| n as i64).unwrap_or(-1);
    let rows = stmt.query_map(
        named_params! {
            ":kind": filter.kind.as_deref(),
            ":object_key": filter.object_key.as_deref(),
            ":direction": filter.direction.as_deref(),
            ":exclude_censored": i64::from(filter.exclude_censored_words),
            ":min_attempts": filter.min_attempts,
            ":limit": limit,
        },
        |row| {
            Ok(PerStratumRow {
                stratum_key: row.get(0)?,
                stratum_label: row.get(1)?,
                attempts: row.get(2)?,
                work: row.get(3)?,
                outputs: row.get(4)?,
                not_applied: row.get(5)?,
                no_root: row.get(6)?,
                surface_mismatch: row.get(7)?,
                uses: row.get(8)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Optional narrowing for `per_direction_report`. Every field left at its default means "no
/// narrowing" — both directions, unlimited (there are only ever two rows, so no `top_n`).
#[derive(Debug, Clone, Default)]
pub struct PerDirectionFilter {
    pub kind: Option<String>,
    pub object_key: Option<String>,
    pub min_attempts: Option<i64>,
    pub exclude_censored_words: bool,
}

/// One row of the per-direction report: the direction tag and the same summed counters
/// `per_object_report` returns, summed across every object/stratum/allomorph that shares it.
/// Direction partitions rather than duplicates: summing this report's rows recovers the same
/// total `per_object_report` shows with no `direction` filter, pinned by
/// `per_direction_report_partitions_rather_than_duplicates_the_object_total`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerDirectionRow {
    pub direction: String,
    pub attempts: i64,
    pub work: i64,
    pub outputs: i64,
    pub not_applied: i64,
    pub no_root: i64,
    pub surface_mismatch: i64,
    pub uses: i64,
}

const PER_DIRECTION_SQL: &str = "
    SELECT f.direction,
           SUM(f.attempts), SUM(f.work), SUM(f.outputs), SUM(f.not_applied),
           SUM(f.no_root), SUM(f.surface_mismatch), SUM(f.uses)
    FROM fact f
    JOIN object o ON o.object_id = f.object_id
    JOIN word w ON w.word_id = f.word_id
    WHERE (:kind IS NULL OR o.kind = :kind)
      AND (:object_key IS NULL OR o.key = :object_key)
      AND (:exclude_censored = 0 OR (w.capped = 0 AND w.timed_out = 0))
    GROUP BY f.direction
    HAVING (:min_attempts IS NULL OR SUM(f.attempts) >= :min_attempts)
    -- `direction` is a plain two-value tag, not a measured quantity, so ordering by the tag itself
    -- is already exact and stable -- unlike work/estimated-time sorts elsewhere in this file, no
    -- tie-break column is needed.
    ORDER BY f.direction ASC
";

/// Direction tag and summed counters — one row per direction with a fact, optionally narrowed to
/// one object or one kind.
pub fn per_direction_report(
    conn: &Connection,
    filter: &PerDirectionFilter,
) -> Result<Vec<PerDirectionRow>, StatsError> {
    let mut stmt = conn.prepare(PER_DIRECTION_SQL)?;
    let rows = stmt.query_map(
        named_params! {
            ":kind": filter.kind.as_deref(),
            ":object_key": filter.object_key.as_deref(),
            ":exclude_censored": i64::from(filter.exclude_censored_words),
            ":min_attempts": filter.min_attempts,
        },
        |row| {
            Ok(PerDirectionRow {
                direction: row.get(0)?,
                attempts: row.get(1)?,
                work: row.get(2)?,
                outputs: row.get(3)?,
                not_applied: row.get(4)?,
                no_root: row.get(5)?,
                surface_mismatch: row.get(6)?,
                uses: row.get(7)?,
            })
        },
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Below this many attempts, "never fires" is indistinguishable from ordinary noise, so the
/// default excludes it rather than defaulting to 0 -- the sensible-default this report needs to
/// be actionable rather than a dump of every rarely-tried object.
pub const NEVER_FIRES_DEFAULT_MIN_ATTEMPTS: i64 = 1000;

/// Optional narrowing for `never_fires_report`. `min_attempts` defaults to
/// `NEVER_FIRES_DEFAULT_MIN_ATTEMPTS`, not 0 -- see that constant's doc.
#[derive(Debug, Clone)]
pub struct NeverFiresFilter {
    pub kind: Option<String>,
    pub direction: Option<String>,
    pub min_attempts: i64,
    pub exclude_censored_words: bool,
    pub top_n: Option<usize>,
}

impl Default for NeverFiresFilter {
    fn default() -> Self {
        NeverFiresFilter {
            kind: None,
            direction: None,
            min_attempts: NEVER_FIRES_DEFAULT_MIN_ATTEMPTS,
            exclude_censored_words: false,
            top_n: None,
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
      AND (:exclude_censored = 0 OR (w.capped = 0 AND w.timed_out = 0))
    GROUP BY o.object_id, f.direction
    HAVING SUM(f.outputs) = 0 AND total_attempts >= :min_attempts
    ORDER BY total_attempts DESC, o.key ASC, f.direction ASC
    LIMIT :limit
";

/// Objects heavily attempted (`>= filter.min_attempts`) that produced zero outputs, in the
/// direction they were attempted -- the single most actionable fact this cache can surface: a
/// rule entered hundreds of thousands of times for nothing.
pub fn never_fires_report(
    conn: &Connection,
    filter: &NeverFiresFilter,
) -> Result<Vec<NeverFiresRow>, StatsError> {
    let mut stmt = conn.prepare(NEVER_FIRES_SQL)?;
    let limit: i64 = filter.top_n.map(|n| n as i64).unwrap_or(-1);
    let rows = stmt.query_map(
        named_params! {
            ":kind": filter.kind.as_deref(),
            ":direction": filter.direction.as_deref(),
            ":exclude_censored": i64::from(filter.exclude_censored_words),
            ":min_attempts": filter.min_attempts,
            ":limit": limit,
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

/// One `coverage` row: whether `counter` could be measured at all for `kind` in this cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageRow {
    pub kind: String,
    pub counter: String,
    pub state: String,
}

/// `run_id`'s recorded coverage rows, so a renderer can print "—" for a counter an engine never
/// touches (foma mode's `no_root`, for example) instead of a misleading zero. Scoped to one run so
/// a later run's coverage state can never mask an earlier run's in the same accumulating cache.
pub fn coverage_rows(conn: &Connection, run_id: i64) -> Result<Vec<CoverageRow>, StatsError> {
    let mut stmt = conn.prepare("SELECT kind, counter, state FROM coverage WHERE run_id = ?1")?;
    let rows = stmt.query_map(params![run_id], |row| {
        Ok(CoverageRow {
            kind: row.get(0)?,
            counter: row.get(1)?,
            state: row.get(2)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::StatsCache;
    use crate::model::{
        Direction, FactRecord, IdentityQuality, ObjectKind, RunMetadata, StructuralLocator,
        WordRecord,
    };

    fn run() -> RunMetadata {
        RunMetadata {
            build_info: "test-build".to_string(),
            fwdata_path: "C:/x/project.fwdata".to_string(),
            grammar_hash: "hash-a".to_string(),
            engine: "hc".to_string(),
            options_hash: "opts-a".to_string(),
            options_json: "{}".to_string(),
            created_utc: "2026-08-22T00:00:00Z".to_string(),
        }
    }

    fn fact(
        object_key: &str,
        kind: ObjectKind,
        attempts: u64,
        work: u64,
        no_root: u64,
    ) -> FactRecord {
        fact_with_direction(
            object_key,
            kind,
            Direction::Analysis,
            attempts,
            work,
            no_root,
        )
    }

    fn fact_with_direction(
        object_key: &str,
        kind: ObjectKind,
        direction: Direction,
        attempts: u64,
        work: u64,
        no_root: u64,
    ) -> FactRecord {
        FactRecord {
            object_key: object_key.to_string(),
            object_kind: kind,
            object_label: object_key.to_string(),
            identity_quality: IdentityQuality::Authored,
            stratum: Some(StructuralLocator::new("0:Root", "Root")),
            allomorph: None,
            direction,
            attempts,
            work,
            outputs: attempts,
            not_applied: 0,
            no_root,
            surface_mismatch: 0,
            uses: 0,
        }
    }

    fn seeded_cache() -> StatsCache {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        outcome
            .cache
            .write_op_cost(ObjectKind::MorphRule, 10, "test provenance")
            .unwrap();
        outcome
            .cache
            .write_op_cost(ObjectKind::LexEntry, 100, "test provenance")
            .unwrap();

        let words = vec![
            WordRecord {
                form: "apu".to_string(),
                elapsed_ns: 500,
                attempts: 3,
                passes: 1,
                capped: false,
                timed_out: false,
                invalid_shape: false,
                facts: vec![
                    fact("rule-a", ObjectKind::MorphRule, 5, 20, 1),
                    fact("root-a", ObjectKind::LexEntry, 2, 4, 5),
                ],
            },
            WordRecord {
                form: "beta".to_string(),
                elapsed_ns: 1_500,
                attempts: 7,
                passes: 0,
                capped: true,
                timed_out: false,
                invalid_shape: false,
                facts: vec![fact("rule-a", ObjectKind::MorphRule, 9, 40, 0)],
            },
        ];
        outcome.cache.flush(&run(), &words).unwrap();
        outcome.cache
    }

    #[test]
    fn per_word_orders_by_elapsed_descending() {
        let cache = seeded_cache();
        let rows = per_word_report(cache.connection()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].form, "beta");
        assert_eq!(rows[1].form, "apu");
        assert!(rows[0].capped);
        assert!(!rows[1].capped);
    }

    #[test]
    fn per_object_kind_filter_narrows_rows() {
        let cache = seeded_cache();
        let filter = PerObjectFilter {
            kind: Some("lex_entry".to_string()),
            ..Default::default()
        };
        let rows = per_object_report(cache.connection(), &filter).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "lex_entry");
    }

    #[test]
    fn per_object_min_attempts_excludes_small_objects() {
        let cache = seeded_cache();
        let filter = PerObjectFilter {
            min_attempts: Some(10),
            ..Default::default()
        };
        let rows = per_object_report(cache.connection(), &filter).unwrap();
        // rule-a sums to 5 + 9 = 14 attempts across both words; root-a sums to 2.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "rule-a");
    }

    #[test]
    fn per_object_exclude_censored_words_drops_their_contribution() {
        let cache = seeded_cache();
        let unfiltered =
            per_object_report(cache.connection(), &PerObjectFilter::default()).unwrap();
        let rule_row = unfiltered.iter().find(|r| r.label == "rule-a").unwrap();
        assert_eq!(rule_row.attempts, 14, "sanity: both words' facts summed");

        let filter = PerObjectFilter {
            exclude_censored_words: true,
            ..Default::default()
        };
        let filtered = per_object_report(cache.connection(), &filter).unwrap();
        let rule_row = filtered.iter().find(|r| r.label == "rule-a").unwrap();
        assert_eq!(rule_row.attempts, 5, "beta is capped and must be excluded");
    }

    #[test]
    fn per_object_sort_key_changes_order() {
        let cache = seeded_cache();
        // rule-a estimates higher (work 60 vs 4) but root-a has more no_root (5 vs 1) -- the sort keys disagree.
        let by_estimate =
            per_object_report(cache.connection(), &PerObjectFilter::default()).unwrap();
        assert_eq!(by_estimate[0].label, "rule-a");
        assert_eq!(by_estimate[1].label, "root-a");

        let filter = PerObjectFilter {
            sort: SortKey::NoRoot,
            ..Default::default()
        };
        let by_no_root = per_object_report(cache.connection(), &filter).unwrap();
        assert_eq!(by_no_root[0].label, "root-a");
        assert_eq!(by_no_root[0].no_root, 5);
        assert_eq!(by_no_root[1].label, "rule-a");
    }

    #[test]
    fn per_object_top_n_limits_rows() {
        let cache = seeded_cache();
        let filter = PerObjectFilter {
            top_n: Some(1),
            ..Default::default()
        };
        let rows = per_object_report(cache.connection(), &filter).unwrap();
        assert_eq!(rows.len(), 1);
    }

    fn fact_with_allomorph(
        object_key: &str,
        kind: ObjectKind,
        allomorph: Option<(&str, &str)>,
        attempts: u64,
        work: u64,
    ) -> FactRecord {
        FactRecord {
            object_key: object_key.to_string(),
            object_kind: kind,
            object_label: object_key.to_string(),
            identity_quality: IdentityQuality::Authored,
            stratum: Some(StructuralLocator::new("0:Root", "Root")),
            allomorph: allomorph.map(|(k, l)| StructuralLocator::new(k, l)),
            direction: Direction::Analysis,
            attempts,
            work,
            outputs: attempts,
            not_applied: 0,
            no_root: 0,
            surface_mismatch: 0,
            uses: 0,
        }
    }

    fn seeded_allomorph_cache() -> StatsCache {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        let words = vec![WordRecord {
            form: "gamma".to_string(),
            elapsed_ns: 700,
            attempts: 10,
            passes: 1,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts: vec![
                fact_with_allomorph(
                    "rule-b",
                    ObjectKind::MorphRule,
                    Some(("rule-b:0", "Allo 0")),
                    4,
                    8,
                ),
                fact_with_allomorph(
                    "rule-b",
                    ObjectKind::MorphRule,
                    Some(("rule-b:1", "Allo 1")),
                    3,
                    6,
                ),
                fact_with_allomorph("rule-b", ObjectKind::MorphRule, None, 2, 5),
                fact_with_allomorph(
                    "rule-c",
                    ObjectKind::MorphRule,
                    Some(("rule-c:0", "C Allo 0")),
                    1,
                    1,
                ),
            ],
        }];
        outcome.cache.flush(&run(), &words).unwrap();
        outcome.cache
    }

    #[test]
    fn per_allomorph_rows_sum_to_object_total_including_none_sentinel() {
        let cache = seeded_allomorph_cache();
        let object_rows = per_object_report(
            cache.connection(),
            &PerObjectFilter {
                object_key: Some("rule-b".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(object_rows.len(), 1);
        let object_row = &object_rows[0];

        let allomorph_rows = per_allomorph_report(
            cache.connection(),
            &PerAllomorphFilter {
                object_key: Some("rule-b".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            allomorph_rows.len(),
            3,
            "two named allomorphs plus the NONE sentinel"
        );

        let summed_attempts: i64 = allomorph_rows.iter().map(|r| r.attempts).sum();
        let summed_work: i64 = allomorph_rows.iter().map(|r| r.work).sum();
        assert_eq!(summed_attempts, object_row.attempts);
        assert_eq!(summed_work, object_row.work);

        let none_row = allomorph_rows
            .iter()
            .find(|r| r.allomorph_key.is_none())
            .expect("NONE sentinel row must be present, not filtered out");
        assert_eq!(none_row.allomorph_label, "NONE");
        assert_eq!(none_row.attempts, 2);
        assert_eq!(none_row.work, 5);
    }

    #[test]
    fn per_allomorph_object_filter_narrows_rows() {
        let cache = seeded_allomorph_cache();
        let narrowed = per_allomorph_report(
            cache.connection(),
            &PerAllomorphFilter {
                object_key: Some("rule-b".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(narrowed.len(), 3);
        assert!(narrowed.iter().all(|r| r.object_label == "rule-b"));

        let unfiltered =
            per_allomorph_report(cache.connection(), &PerAllomorphFilter::default()).unwrap();
        assert_eq!(unfiltered.len(), 4, "rule-c's allomorph must also appear");
    }

    #[test]
    fn per_allomorph_orders_deterministically_on_ties() {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        let words = vec![WordRecord {
            form: "tie".to_string(),
            elapsed_ns: 1,
            attempts: 2,
            passes: 1,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts: vec![
                fact_with_allomorph("rule-y", ObjectKind::MorphRule, Some(("a0", "A0")), 1, 5),
                fact_with_allomorph("rule-x", ObjectKind::MorphRule, Some(("a0", "A0")), 1, 5),
            ],
        }];
        outcome.cache.flush(&run(), &words).unwrap();

        let rows = per_allomorph_report(outcome.cache.connection(), &PerAllomorphFilter::default())
            .unwrap();
        assert_eq!(rows.len(), 2);
        let labels: Vec<_> = rows.iter().map(|r| r.object_label.clone()).collect();
        assert_eq!(
            labels,
            vec!["rule-x".to_string(), "rule-y".to_string()],
            "equal estimated cost must still order deterministically via the key tie-break"
        );
    }

    fn fact_with_allomorph_not_applied(
        object_key: &str,
        allomorph: Option<(&str, &str)>,
        attempts: u64,
        not_applied: u64,
    ) -> FactRecord {
        FactRecord {
            object_key: object_key.to_string(),
            object_kind: ObjectKind::MorphRule,
            object_label: object_key.to_string(),
            identity_quality: IdentityQuality::Authored,
            stratum: Some(StructuralLocator::new("0:Root", "Root")),
            allomorph: allomorph.map(|(k, l)| StructuralLocator::new(k, l)),
            direction: Direction::Analysis,
            attempts,
            work: 1,
            outputs: 0,
            not_applied,
            no_root: 0,
            surface_mismatch: 0,
            uses: 0,
        }
    }

    /// Two failing allomorphs must not inflate per-object `not_applied` past the one invocation.
    #[test]
    fn per_object_not_applied_counts_rule_level_invocations_not_per_allomorph_failures() {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        let words = vec![WordRecord {
            form: "granularity".to_string(),
            elapsed_ns: 1,
            attempts: 1,
            passes: 0,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts: vec![
                // Rule-level residual: one invocation, reached both allomorphs, produced nothing.
                fact_with_allomorph_not_applied("rule-f", None, 1, 1),
                fact_with_allomorph_not_applied("rule-f", Some(("rule-f:0", "Allo 0")), 0, 1),
                fact_with_allomorph_not_applied("rule-f", Some(("rule-f:1", "Allo 1")), 0, 1),
            ],
        }];
        outcome.cache.flush(&run(), &words).unwrap();

        let object_rows =
            per_object_report(outcome.cache.connection(), &PerObjectFilter::default()).unwrap();
        assert_eq!(object_rows.len(), 1);
        assert_eq!(
            object_rows[0].not_applied, 1,
            "one failed invocation must read as 1, not 3 -- summing every allomorph's own failure \
             mixes a per-allomorph count into a per-invocation column"
        );

        let allomorph_rows =
            per_allomorph_report(outcome.cache.connection(), &PerAllomorphFilter::default())
                .unwrap();
        assert_eq!(
            allomorph_rows.len(),
            3,
            "the per-allomorph report keeps all three rows, including the NONE sentinel"
        );
        assert!(
            allomorph_rows.iter().all(|r| r.not_applied == 1),
            "the per-allomorph report is unaffected -- each row still reports its own failure"
        );
    }

    fn fact_with_stratum(
        object_key: &str,
        kind: ObjectKind,
        stratum: Option<(&str, &str)>,
        attempts: u64,
        work: u64,
    ) -> FactRecord {
        FactRecord {
            object_key: object_key.to_string(),
            object_kind: kind,
            object_label: object_key.to_string(),
            identity_quality: IdentityQuality::Authored,
            stratum: stratum.map(|(k, l)| StructuralLocator::new(k, l)),
            allomorph: None,
            direction: Direction::Analysis,
            attempts,
            work,
            outputs: attempts,
            not_applied: 0,
            no_root: 0,
            surface_mismatch: 0,
            uses: 0,
        }
    }

    fn seeded_stratum_cache() -> StatsCache {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        let words = vec![WordRecord {
            form: "delta".to_string(),
            elapsed_ns: 900,
            attempts: 11,
            passes: 1,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts: vec![
                fact_with_stratum(
                    "rule-d",
                    ObjectKind::MorphRule,
                    Some(("0:Root", "Root")),
                    3,
                    6,
                ),
                fact_with_stratum(
                    "rule-d",
                    ObjectKind::MorphRule,
                    Some(("1:Suffix", "Suffix")),
                    2,
                    4,
                ),
                fact_with_stratum("rule-d", ObjectKind::MorphRule, None, 1, 2),
                fact_with_stratum(
                    "root-d",
                    ObjectKind::LexEntry,
                    Some(("0:Root", "Root")),
                    5,
                    10,
                ),
            ],
        }];
        outcome.cache.flush(&run(), &words).unwrap();
        outcome.cache
    }

    #[test]
    fn per_stratum_rows_sum_to_object_total_including_sentinel() {
        let cache = seeded_stratum_cache();
        let object_rows = per_object_report(
            cache.connection(),
            &PerObjectFilter {
                object_key: Some("rule-d".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(object_rows.len(), 1);
        let object_row = &object_rows[0];

        let stratum_rows = per_stratum_report(
            cache.connection(),
            &PerStratumFilter {
                object_key: Some("rule-d".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(stratum_rows.len(), 3);

        let summed_attempts: i64 = stratum_rows.iter().map(|r| r.attempts).sum();
        let summed_work: i64 = stratum_rows.iter().map(|r| r.work).sum();
        assert_eq!(summed_attempts, object_row.attempts);
        assert_eq!(summed_work, object_row.work);

        let na_row = stratum_rows
            .iter()
            .find(|r| r.stratum_key.is_none())
            .expect("not-applicable sentinel row must be present, not filtered out");
        assert_eq!(na_row.stratum_label, "not applicable");
        assert_eq!(na_row.attempts, 1);
        assert_eq!(na_row.work, 2);
    }

    #[test]
    fn per_stratum_kind_filter_isolates_that_kind() {
        let cache = seeded_stratum_cache();
        let by_kind = per_stratum_report(
            cache.connection(),
            &PerStratumFilter {
                kind: Some("lex_entry".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(by_kind.len(), 1);
        assert_eq!(by_kind[0].stratum_label, "Root");
        assert_eq!(by_kind[0].attempts, 5);
        assert_eq!(by_kind[0].work, 10);
    }

    #[test]
    fn per_stratum_unfiltered_aggregates_across_objects_sharing_a_stratum() {
        let cache = seeded_stratum_cache();
        let rows = per_stratum_report(cache.connection(), &PerStratumFilter::default()).unwrap();
        let root_row = rows.iter().find(|r| r.stratum_label == "Root").unwrap();
        assert_eq!(root_row.attempts, 3 + 5);
        assert_eq!(root_row.work, 6 + 10);
    }

    #[test]
    fn per_stratum_orders_deterministically_on_ties() {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        let words = vec![WordRecord {
            form: "tie2".to_string(),
            elapsed_ns: 1,
            attempts: 2,
            passes: 1,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts: vec![
                fact_with_stratum(
                    "rule-e",
                    ObjectKind::MorphRule,
                    Some(("z:Last", "Last")),
                    1,
                    5,
                ),
                fact_with_stratum(
                    "rule-e",
                    ObjectKind::MorphRule,
                    Some(("a:First", "First")),
                    1,
                    5,
                ),
            ],
        }];
        outcome.cache.flush(&run(), &words).unwrap();

        let rows =
            per_stratum_report(outcome.cache.connection(), &PerStratumFilter::default()).unwrap();
        assert_eq!(rows.len(), 2);
        let labels: Vec<_> = rows.iter().map(|r| r.stratum_label.clone()).collect();
        assert_eq!(
            labels,
            vec!["First".to_string(), "Last".to_string()],
            "equal work must still order deterministically via the key tie-break"
        );
    }

    #[test]
    fn coverage_rows_round_trip() {
        let outcome = StatsCache::open_in_memory("hash-a").unwrap();
        outcome
            .cache
            .write_coverage(
                1,
                ObjectKind::RootIndex,
                "no_root",
                crate::model::CoverageState::Unsupported,
            )
            .unwrap();
        let rows = coverage_rows(outcome.cache.connection(), 1).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, "unsupported");
    }

    #[test]
    fn coverage_is_scoped_per_run_and_does_not_overwrite() {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        let run_id_a = outcome
            .cache
            .flush(
                &run(),
                &[WordRecord {
                    form: "apu".to_string(),
                    elapsed_ns: 1,
                    attempts: 1,
                    passes: 1,
                    capped: false,
                    timed_out: false,
                    invalid_shape: false,
                    facts: vec![],
                }],
            )
            .unwrap();
        let run_id_b = outcome
            .cache
            .flush(
                &run(),
                &[WordRecord {
                    form: "beta".to_string(),
                    elapsed_ns: 1,
                    attempts: 1,
                    passes: 1,
                    capped: false,
                    timed_out: false,
                    invalid_shape: false,
                    facts: vec![],
                }],
            )
            .unwrap();
        assert_ne!(
            run_id_a, run_id_b,
            "sanity: two flushes produce distinct runs"
        );

        outcome
            .cache
            .write_coverage(
                run_id_a,
                ObjectKind::RootIndex,
                "no_root",
                crate::model::CoverageState::Measured,
            )
            .unwrap();
        outcome
            .cache
            .write_coverage(
                run_id_b,
                ObjectKind::RootIndex,
                "no_root",
                crate::model::CoverageState::Unsupported,
            )
            .unwrap();

        let rows_a = coverage_rows(outcome.cache.connection(), run_id_a).unwrap();
        let rows_b = coverage_rows(outcome.cache.connection(), run_id_b).unwrap();
        assert_eq!(rows_a.len(), 1);
        assert_eq!(rows_a[0].state, "measured");
        assert_eq!(rows_b.len(), 1);
        assert_eq!(rows_b[0].state, "unsupported");
    }

    /// One rule fact recorded under both directions, sharing `(word, object, stratum, allomorph)`.
    fn seeded_direction_cache() -> StatsCache {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        let words = vec![WordRecord {
            form: "dirword".to_string(),
            elapsed_ns: 1_000,
            attempts: 8,
            passes: 1,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts: vec![
                fact_with_direction(
                    "rule-dir",
                    ObjectKind::MorphRule,
                    Direction::Analysis,
                    5,
                    20,
                    2,
                ),
                fact_with_direction(
                    "rule-dir",
                    ObjectKind::MorphRule,
                    Direction::Synthesis,
                    3,
                    12,
                    0,
                ),
            ],
        }];
        outcome.cache.flush(&run(), &words).unwrap();
        outcome.cache
    }

    #[test]
    fn per_object_direction_filter_narrows_to_one_direction() {
        let cache = seeded_direction_cache();

        let analysis_only = per_object_report(
            cache.connection(),
            &PerObjectFilter {
                direction: Some("analysis".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(analysis_only.len(), 1);
        assert_eq!(analysis_only[0].attempts, 5);
        assert_eq!(analysis_only[0].no_root, 2);

        let synthesis_only = per_object_report(
            cache.connection(),
            &PerObjectFilter {
                direction: Some("synthesis".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(synthesis_only.len(), 1);
        assert_eq!(synthesis_only[0].attempts, 3);
        assert_eq!(synthesis_only[0].no_root, 0);

        let unfiltered =
            per_object_report(cache.connection(), &PerObjectFilter::default()).unwrap();
        assert_eq!(
            unfiltered[0].attempts, 8,
            "no direction filter must sum both directions' attempts, recovering the undirected total"
        );
    }

    #[test]
    fn per_direction_report_partitions_rather_than_duplicates_the_object_total() {
        let cache = seeded_direction_cache();

        let object_total = per_object_report(cache.connection(), &PerObjectFilter::default())
            .unwrap()
            .into_iter()
            .find(|r| r.label == "rule-dir")
            .unwrap();

        let direction_rows =
            per_direction_report(cache.connection(), &PerDirectionFilter::default()).unwrap();
        assert_eq!(
            direction_rows.len(),
            2,
            "both directions must have a row, since both carry non-zero counters"
        );
        let summed_attempts: i64 = direction_rows.iter().map(|r| r.attempts).sum();
        let summed_work: i64 = direction_rows.iter().map(|r| r.work).sum();
        let summed_no_root: i64 = direction_rows.iter().map(|r| r.no_root).sum();
        assert_eq!(
            summed_attempts, object_total.attempts,
            "summing over direction must recover the object's undirected total, not double it"
        );
        assert_eq!(summed_work, object_total.work);
        assert_eq!(summed_no_root, object_total.no_root);
    }

    #[test]
    fn per_direction_report_orders_deterministically() {
        let cache = seeded_direction_cache();
        let rows =
            per_direction_report(cache.connection(), &PerDirectionFilter::default()).unwrap();
        let directions: Vec<_> = rows.iter().map(|r| r.direction.clone()).collect();
        assert_eq!(
            directions,
            vec!["analysis".to_string(), "synthesis".to_string()],
            "direction rows must sort in a fixed, explicit order, not an incidental one"
        );
    }

    fn never_fires_fact(
        object_key: &str,
        direction: Direction,
        attempts: u64,
        outputs: u64,
    ) -> FactRecord {
        FactRecord {
            object_key: object_key.to_string(),
            object_kind: ObjectKind::MorphRule,
            object_label: object_key.to_string(),
            identity_quality: IdentityQuality::Authored,
            stratum: Some(StructuralLocator::new("0:Root", "Root")),
            allomorph: None,
            direction,
            attempts,
            work: attempts,
            outputs,
            not_applied: attempts.saturating_sub(outputs),
            no_root: 0,
            surface_mismatch: 0,
            uses: 0,
        }
    }

    fn seeded_never_fires_cache() -> StatsCache {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        let words = vec![WordRecord {
            form: "neverfiresword".to_string(),
            elapsed_ns: 1,
            attempts: 1,
            passes: 0,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts: vec![
                // Heavily attempted, zero outputs -- the motivating case.
                never_fires_fact("rule-dead", Direction::Analysis, 2_500, 0),
                // Heavily attempted but productive -- must never appear.
                never_fires_fact("rule-alive", Direction::Analysis, 2_500, 40),
                // Zero outputs but below the sensible default floor -- noise, not signal.
                never_fires_fact("rule-thin", Direction::Analysis, 5, 0),
                // Dead in analysis, alive in synthesis -- direction-aware, only one row must appear.
                never_fires_fact("rule-dir-split", Direction::Analysis, 3_000, 0),
                never_fires_fact("rule-dir-split", Direction::Synthesis, 500, 10),
            ],
        }];
        outcome.cache.flush(&run(), &words).unwrap();
        outcome.cache
    }

    #[test]
    fn never_fires_report_finds_heavily_attempted_zero_output_objects() {
        let cache = seeded_never_fires_cache();
        let rows = never_fires_report(cache.connection(), &NeverFiresFilter::default()).unwrap();
        let labels: Vec<_> = rows.iter().map(|r| r.label.clone()).collect();

        assert!(
            labels.contains(&"rule-dead".to_string()),
            "a heavily-attempted, zero-output rule must be reported"
        );
        assert!(
            !labels.contains(&"rule-alive".to_string()),
            "a rule that produced outputs must never appear, no matter how many attempts"
        );
        assert!(
            !labels.contains(&"rule-thin".to_string()),
            "below the sensible default min_attempts, a zero-output rule is noise, not signal"
        );
    }

    #[test]
    fn never_fires_report_is_direction_aware() {
        let cache = seeded_never_fires_cache();
        let rows = never_fires_report(cache.connection(), &NeverFiresFilter::default()).unwrap();
        let split_rows: Vec<_> = rows
            .iter()
            .filter(|r| r.label == "rule-dir-split")
            .collect();

        assert_eq!(
            split_rows.len(),
            1,
            "the rule fires in synthesis, so only its analysis row may be reported"
        );
        assert_eq!(split_rows[0].direction, "analysis");
    }

    #[test]
    fn never_fires_report_orders_by_attempts_descending_and_respects_explicit_min_attempts() {
        let cache = seeded_never_fires_cache();
        let rows = never_fires_report(
            cache.connection(),
            &NeverFiresFilter {
                min_attempts: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let labels: Vec<_> = rows.iter().map(|r| r.label.clone()).collect();
        assert_eq!(
            labels,
            vec![
                "rule-dir-split".to_string(),
                "rule-dead".to_string(),
                "rule-thin".to_string(),
            ],
            "descending by attempts (3000, 2500, 5), and a low explicit min_attempts admits rule-thin"
        );
    }
}
