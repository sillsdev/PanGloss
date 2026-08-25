//! Open, wipe-on-mismatch, accumulate, and write: the cache's write side.
//!
//! Read queries live in `crate::report`; this module only knows how to get bytes in.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::error::StatsError;
use crate::model::{
    FactRecord, IdentityQuality, ObjectKind, RunMetadata, StructuralLocator, WordRecord,
};
use crate::schema;
use crate::util::to_i64;

/// An open stats cache: one SQLite connection, WAL mode, a caller-chosen busy timeout.
pub struct StatsCache {
    conn: Connection,
}

/// Result of `StatsCache::open`: the cache, and whether opening it wiped prior data.
///
/// `wiped` exists so a caller can report the wipe rather than let it look like the accumulation
/// feature silently failing — this repo's rule that "something happened" must never be swallowed.
pub struct OpenOutcome {
    pub cache: StatsCache,
    pub wiped: bool,
}

impl StatsCache {
    /// Opens (creating if absent) the cache at `cache_path` for the given grammar hash.
    ///
    /// Wipes and recreates when the stored `grammar_hash` differs from `grammar_hash`, or when
    /// the stored `schema_version` differs from this build's — a cache is never migrated.
    /// `cache_path` may be `crate::path::default_cache_path`'s result or a caller-supplied
    /// override; this function does not care which.
    pub fn open(cache_path: &Path, grammar_hash: &str) -> Result<OpenOutcome, StatsError> {
        if let Some(parent) = cache_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(cache_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Self::open_on(conn, grammar_hash)
    }

    /// Opens an in-memory cache. Test-only: a real cache is always file-backed so it survives
    /// across `batch --stats` invocations.
    #[cfg(test)]
    pub(crate) fn open_in_memory(grammar_hash: &str) -> Result<OpenOutcome, StatsError> {
        let conn = Connection::open_in_memory()?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Self::open_on(conn, grammar_hash)
    }

    fn open_on(conn: Connection, grammar_hash: &str) -> Result<OpenOutcome, StatsError> {
        let wiped = match schema::latest_run_signature(&conn)? {
            None => {
                schema::create(&conn)?;
                false
            }
            Some((stored_schema_version, stored_grammar_hash)) => {
                if stored_schema_version != schema::SCHEMA_VERSION
                    || stored_grammar_hash != grammar_hash
                {
                    schema::wipe_and_recreate(&conn)?;
                    true
                } else {
                    false
                }
            }
        };
        Ok(OpenOutcome {
            cache: StatsCache { conn },
            wiped,
        })
    }

    /// Direct access to the underlying connection, for `crate::report`'s read queries.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Which of `forms` already have a `word` row, so a caller can skip recomputing them.
    pub fn existing_words(&self, forms: &[&str]) -> Result<HashSet<String>, StatsError> {
        if forms.is_empty() {
            return Ok(HashSet::new());
        }
        let placeholders = vec!["?"; forms.len()].join(",");
        let sql = format!("SELECT form FROM word WHERE form IN ({placeholders})");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(forms.iter()), |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect::<Result<HashSet<_>, _>>().map_err(Into::into)
    }

    /// Returns the stable id for `(key, kind)`, inserting a new `object` row on first sight.
    /// `morpheme` is only ever `Some` for a `lex_entry` object; every other kind interns the
    /// `morpheme` sentinel (id 0).
    pub fn intern_object(
        &self,
        key: &str,
        kind: ObjectKind,
        label: &str,
        identity_quality: IdentityQuality,
        morpheme: Option<&StructuralLocator>,
    ) -> Result<i64, StatsError> {
        intern_object_on(&self.conn, key, kind, label, identity_quality, morpheme)
    }

    /// Returns the stable id for a stratum `key`, inserting a new row on first sight.
    pub fn intern_stratum(&self, locator: &StructuralLocator) -> Result<i64, StatsError> {
        intern_stratum_on(&self.conn, locator)
    }

    /// Returns the stable id for an allomorph `key`, inserting a new row on first sight.
    pub fn intern_allomorph(&self, locator: &StructuralLocator) -> Result<i64, StatsError> {
        intern_allomorph_on(&self.conn, locator)
    }

    /// Returns the stable id for a morpheme `key`, inserting a new row on first sight.
    pub fn intern_morpheme(&self, locator: &StructuralLocator) -> Result<i64, StatsError> {
        intern_morpheme_on(&self.conn, locator)
    }

    /// Writes one run's metadata and every word/fact row it produced, in a single transaction.
    ///
    /// Word rows upsert on `form` (two runs can compute the same word concurrently); fact rows
    /// upsert on their composite key for the same reason. Returns the new `run_id`.
    pub fn flush(&mut self, run: &RunMetadata, words: &[WordRecord]) -> Result<i64, StatsError> {
        let total_elapsed_ns: u64 = words.iter().map(|w| w.elapsed_ns).sum();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let incompatible_engine: Option<String> = tx
            .query_row(
                "SELECT engine FROM run WHERE engine <> ?1 LIMIT 1",
                params![&run.engine],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(existing) = incompatible_engine {
            return Err(StatsError::EngineMismatch {
                existing,
                requested: run.engine.clone(),
            });
        }

        tx.execute(
            "INSERT INTO run (schema_version, counter_semantics, build_info, fwdata_path, grammar_hash, engine, options_hash, options_json, created_utc, word_count, total_elapsed_ns)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                schema::SCHEMA_VERSION,
                schema::COUNTER_SEMANTICS_VERSION,
                run.build_info,
                run.fwdata_path,
                run.grammar_hash,
                run.engine,
                run.options_hash,
                run.options_json,
                run.created_utc,
                to_i64("word_count", words.len() as u64)?,
                to_i64("total_elapsed_ns", total_elapsed_ns)?,
            ],
        )?;
        let run_id = tx.last_insert_rowid();

        for word in words {
            let word_id = upsert_word(&tx, run_id, word)?;
            for fact in &word.facts {
                write_fact(&tx, word_id, fact)?;
            }
        }

        tx.commit()?;
        Ok(run_id)
    }
}

fn upsert_word(tx: &Transaction, run_id: i64, word: &WordRecord) -> Result<i64, StatsError> {
    tx.execute(
        "INSERT INTO word (run_id, form, elapsed_ns, attempts, passes, capped, timed_out, invalid_shape)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(form) DO UPDATE SET
           run_id = excluded.run_id,
           elapsed_ns = excluded.elapsed_ns,
           attempts = excluded.attempts,
           passes = excluded.passes,
           capped = excluded.capped,
           timed_out = excluded.timed_out,
           invalid_shape = excluded.invalid_shape",
        params![
            run_id,
            word.form,
            to_i64("elapsed_ns", word.elapsed_ns)?,
            to_i64("attempts", word.attempts)?,
            to_i64("passes", word.passes)?,
            i64::from(word.capped),
            i64::from(word.timed_out),
            i64::from(word.invalid_shape),
        ],
    )?;
    tx.query_row(
        "SELECT word_id FROM word WHERE form = ?1",
        params![word.form],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn write_fact(tx: &Transaction, word_id: i64, fact: &FactRecord) -> Result<(), StatsError> {
    let object_id = intern_object_on(
        tx,
        &fact.object_key,
        fact.object_kind,
        &fact.object_label,
        fact.identity_quality,
        fact.morpheme.as_ref(),
    )?;
    let stratum_id = match &fact.stratum {
        Some(locator) => intern_stratum_on(tx, locator)?,
        None => 0,
    };
    let allomorph_id = match &fact.allomorph {
        Some(locator) => intern_allomorph_on(tx, locator)?,
        None => 0,
    };
    tx.execute(
        "INSERT INTO fact (word_id, object_id, stratum_id, allomorph_id, direction, attempts, work, outputs, not_applied, no_root, surface_mismatch, uses, self_time_ns)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(word_id, object_id, stratum_id, allomorph_id, direction) DO UPDATE SET
           attempts = excluded.attempts,
           work = excluded.work,
           outputs = excluded.outputs,
           not_applied = excluded.not_applied,
           no_root = excluded.no_root,
           surface_mismatch = excluded.surface_mismatch,
           uses = excluded.uses,
           self_time_ns = excluded.self_time_ns",
        params![
            word_id,
            object_id,
            stratum_id,
            allomorph_id,
            fact.direction.as_str(),
            to_i64("attempts", fact.attempts)?,
            to_i64("work", fact.work)?,
            to_i64("outputs", fact.outputs)?,
            to_i64("not_applied", fact.not_applied)?,
            to_i64("no_root", fact.no_root)?,
            to_i64("surface_mismatch", fact.surface_mismatch)?,
            to_i64("uses", fact.uses)?,
            to_i64("self_time_ns", fact.self_time_ns)?,
        ],
    )?;
    Ok(())
}

/// `object`'s `UNIQUE(key, kind)` index makes `INSERT OR IGNORE` + `SELECT` race-safe across processes.
fn intern_object_on(
    conn: &Connection,
    key: &str,
    kind: ObjectKind,
    label: &str,
    identity_quality: IdentityQuality,
    morpheme: Option<&StructuralLocator>,
) -> Result<i64, StatsError> {
    let morpheme_id = match morpheme {
        Some(locator) => intern_morpheme_on(conn, locator)?,
        None => 0,
    };
    conn.execute(
        "INSERT OR IGNORE INTO object (key, kind, label, identity_quality, morpheme_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![key, kind.as_str(), label, identity_quality.as_str(), morpheme_id],
    )?;
    conn.query_row(
        "SELECT object_id FROM object WHERE key = ?1 AND kind = ?2",
        params![key, kind.as_str()],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

/// `stratum_key`'s unique index makes `INSERT OR IGNORE` + `SELECT` race-safe, matching `intern_object_on`.
fn intern_stratum_on(conn: &Connection, locator: &StructuralLocator) -> Result<i64, StatsError> {
    conn.execute(
        "INSERT OR IGNORE INTO stratum (key, label) VALUES (?1, ?2)",
        params![locator.key, locator.label],
    )?;
    conn.query_row(
        "SELECT stratum_id FROM stratum WHERE key = ?1",
        params![locator.key],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

/// See `intern_stratum_on` — `allomorph_key` gives `allomorph` the same shape.
fn intern_allomorph_on(conn: &Connection, locator: &StructuralLocator) -> Result<i64, StatsError> {
    conn.execute(
        "INSERT OR IGNORE INTO allomorph (key, label) VALUES (?1, ?2)",
        params![locator.key, locator.label],
    )?;
    conn.query_row(
        "SELECT allomorph_id FROM allomorph WHERE key = ?1",
        params![locator.key],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

/// See `intern_stratum_on` — `morpheme_key` gives `morpheme` the same shape.
fn intern_morpheme_on(conn: &Connection, locator: &StructuralLocator) -> Result<i64, StatsError> {
    conn.execute(
        "INSERT OR IGNORE INTO morpheme (key, label) VALUES (?1, ?2)",
        params![locator.key, locator.label],
    )?;
    conn.query_row(
        "SELECT morpheme_id FROM morpheme WHERE key = ?1",
        params![locator.key],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Direction;
    use crate::test_support::TempDir;
    use std::sync::{Arc, Barrier};
    use std::thread;

    fn sample_word(form: &str) -> WordRecord {
        WordRecord {
            form: form.to_string(),
            elapsed_ns: 1_000,
            attempts: 10,
            passes: 1,
            capped: false,
            timed_out: false,
            invalid_shape: false,
            facts: vec![FactRecord {
                object_key: "rule-a".to_string(),
                object_kind: ObjectKind::MorphRule,
                object_label: "Rule A".to_string(),
                identity_quality: IdentityQuality::Authored,
                stratum: Some(StructuralLocator::new("0:Root", "Root")),
                allomorph: None,
                morpheme: None,
                direction: Direction::Analysis,
                attempts: 5,
                work: 20,
                outputs: 2,
                not_applied: 1,
                no_root: 0,
                surface_mismatch: 0,
                uses: 1,
                self_time_ns: 0,
            }],
        }
    }

    fn sample_run() -> RunMetadata {
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

    #[test]
    fn create_write_and_read_back_exactly() {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        assert!(!outcome.wiped);
        let run = sample_run();
        let words = vec![sample_word("apu")];
        outcome.cache.flush(&run, &words).unwrap();

        let per_word = crate::report::per_word_report(outcome.cache.connection()).unwrap();
        assert_eq!(per_word.len(), 1);
        assert_eq!(per_word[0].form, "apu");
        assert_eq!(per_word[0].attempts, 10);
        assert_eq!(per_word[0].passes, 1);
        assert!(!per_word[0].capped);
        assert!(!per_word[0].timed_out);

        let per_object = crate::report::per_object_report(
            outcome.cache.connection(),
            &crate::report::PerObjectFilter::default(),
        )
        .unwrap();
        assert_eq!(per_object.len(), 1);
        assert_eq!(per_object[0].label, "Rule A");
        assert_eq!(per_object[0].attempts, 5);
        assert_eq!(per_object[0].work, 20);
        assert_eq!(per_object[0].outputs, 2);
        assert_eq!(per_object[0].not_applied, 1);
        assert_eq!(per_object[0].no_root, 0);
        assert_eq!(per_object[0].uses, 1);
    }

    #[test]
    fn wipes_on_grammar_hash_change_and_reports_it() {
        let path = TempDir::new("pg-stats-wipe");
        let cache_path = path.path().join("cache.sqlite3");

        let mut first = StatsCache::open(&cache_path, "hash-a").unwrap();
        assert!(!first.wiped);
        first
            .cache
            .flush(&sample_run(), &[sample_word("apu")])
            .unwrap();

        let second = StatsCache::open(&cache_path, "hash-b").unwrap();
        assert!(
            second.wiped,
            "opening with a different grammar hash must wipe"
        );

        let per_word = crate::report::per_word_report(second.cache.connection()).unwrap();
        assert!(per_word.is_empty(), "old rows must be gone after a wipe");
    }

    #[test]
    fn no_wipe_on_same_hash_and_accumulates_new_words_only() {
        let path = TempDir::new("pg-stats-accumulate");
        let cache_path = path.path().join("cache.sqlite3");

        let mut first = StatsCache::open(&cache_path, "hash-a").unwrap();
        first
            .cache
            .flush(&sample_run(), &[sample_word("apu")])
            .unwrap();
        drop(first);

        let mut second = StatsCache::open(&cache_path, "hash-a").unwrap();
        assert!(!second.wiped, "same grammar hash must not wipe");

        let already = second.cache.existing_words(&["apu", "beta"]).unwrap();
        assert!(already.contains("apu"));
        assert!(!already.contains("beta"));

        second
            .cache
            .flush(&sample_run(), &[sample_word("beta")])
            .unwrap();

        let per_word = crate::report::per_word_report(second.cache.connection()).unwrap();
        assert_eq!(
            per_word.len(),
            2,
            "both words must be present after accumulation"
        );
        let forms: HashSet<_> = per_word.iter().map(|r| r.form.clone()).collect();
        assert!(forms.contains("apu"));
        assert!(forms.contains("beta"));
    }

    #[test]
    fn mixed_options_hash_is_detected() {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        let mut run_a = sample_run();
        run_a.options_hash = "opts-a".to_string();
        outcome.cache.flush(&run_a, &[sample_word("apu")]).unwrap();

        let clean = crate::report::mixed_settings(outcome.cache.connection()).unwrap();
        assert!(!clean.is_mixed());

        let mut run_b = sample_run();
        run_b.options_hash = "opts-b".to_string();
        outcome.cache.flush(&run_b, &[sample_word("beta")]).unwrap();

        let mixed = crate::report::mixed_settings(outcome.cache.connection()).unwrap();
        assert!(mixed.is_mixed());
    }

    #[test]
    fn interning_is_stable_across_calls_and_reopen() {
        let path = TempDir::new("pg-stats-intern");
        let cache_path = path.path().join("cache.sqlite3");

        let first = StatsCache::open(&cache_path, "hash-a").unwrap();
        let id_a = first
            .cache
            .intern_object(
                "rule-a",
                ObjectKind::MorphRule,
                "Rule A",
                IdentityQuality::Authored,
                None,
            )
            .unwrap();
        let id_a_again = first
            .cache
            .intern_object(
                "rule-a",
                ObjectKind::MorphRule,
                "Rule A",
                IdentityQuality::Authored,
                None,
            )
            .unwrap();
        assert_eq!(id_a, id_a_again);
        drop(first);

        let second = StatsCache::open(&cache_path, "hash-a").unwrap();
        let id_a_reopened = second
            .cache
            .intern_object(
                "rule-a",
                ObjectKind::MorphRule,
                "Rule A",
                IdentityQuality::Authored,
                None,
            )
            .unwrap();
        assert_eq!(id_a, id_a_reopened);

        let stratum_a = second
            .cache
            .intern_stratum(&StructuralLocator::new("0:Root", "Root"))
            .unwrap();
        let stratum_a_again = second
            .cache
            .intern_stratum(&StructuralLocator::new("0:Root", "Root"))
            .unwrap();
        assert_eq!(stratum_a, stratum_a_again);
    }

    #[test]
    fn stratum_interning_converges_across_separate_connections_to_same_file() {
        let path = TempDir::new("pg-stats-stratum-cross-conn");
        let cache_path = path.path().join("cache.sqlite3");
        let locator = StructuralLocator::new("0:Root", "Root");

        let handle_a = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
        let handle_b = StatsCache::open(&cache_path, "hash-a").unwrap().cache;

        let id_from_a = handle_a.intern_stratum(&locator).unwrap();
        let id_from_b = handle_b.intern_stratum(&locator).unwrap();
        assert_eq!(id_from_a, id_from_b);
    }

    #[test]
    fn allomorph_interning_converges_across_separate_connections_to_same_file() {
        let path = TempDir::new("pg-stats-allomorph-cross-conn");
        let cache_path = path.path().join("cache.sqlite3");
        let locator = StructuralLocator::new("allo-a", "Allo A");

        let handle_a = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
        let handle_b = StatsCache::open(&cache_path, "hash-a").unwrap().cache;

        let id_from_a = handle_a.intern_allomorph(&locator).unwrap();
        let id_from_b = handle_b.intern_allomorph(&locator).unwrap();
        assert_eq!(id_from_a, id_from_b);
    }

    #[test]
    fn huge_counter_round_trips_and_overflow_is_rejected() {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        let mut word = sample_word("apu");
        word.facts[0].work = i64::MAX as u64;
        outcome.cache.flush(&sample_run(), &[word]).unwrap();

        let per_object = crate::report::per_object_report(
            outcome.cache.connection(),
            &crate::report::PerObjectFilter::default(),
        )
        .unwrap();
        assert_eq!(per_object[0].work, i64::MAX);

        let mut overflowing = sample_word("beta");
        overflowing.facts[0].work = i64::MAX as u64 + 1;
        let err = outcome
            .cache
            .flush(&sample_run(), &[overflowing])
            .unwrap_err();
        assert!(matches!(
            err,
            StatsError::CounterOverflow {
                counter: "work",
                ..
            }
        ));
    }

    #[test]
    fn flush_rejects_a_different_engine_after_the_first_run() {
        let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
        outcome
            .cache
            .flush(&sample_run(), &[sample_word("apu")])
            .unwrap();
        let mut other = sample_run();
        other.engine = "foma".to_string();
        let err = outcome
            .cache
            .flush(&other, &[sample_word("beta")])
            .expect_err("one cache must not accept facts from two engines");
        assert!(matches!(
            err,
            StatsError::EngineMismatch {
                existing,
                requested
            } if existing == "hc" && requested == "foma"
        ));
        let run_count: i64 = outcome
            .cache
            .connection()
            .query_row("SELECT COUNT(*) FROM run", [], |row| row.get(0))
            .unwrap();
        assert_eq!(run_count, 1, "a rejected flush must write nothing");
    }

    #[test]
    fn concurrent_first_writers_cannot_mix_engines() {
        let dir = TempDir::new("pg-stats-engine-race");
        let cache_path = dir.path().join("cache.sqlite3");
        drop(StatsCache::open(&cache_path, "hash-a").unwrap());

        let barrier = Arc::new(Barrier::new(2));
        let spawn_writer = |engine: &'static str, form: &'static str| {
            let cache_path = cache_path.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let mut cache = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
                let mut run = sample_run();
                run.engine = engine.to_string();
                barrier.wait();
                cache.flush(&run, &[sample_word(form)])
            })
        };
        let hc = spawn_writer("hc", "apu");
        let foma = spawn_writer("foma", "beta");
        let results = [hc.join().unwrap(), foma.join().unwrap()];

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(StatsError::EngineMismatch { .. })))
                .count(),
            1
        );
        let cache = StatsCache::open(&cache_path, "hash-a").unwrap().cache;
        let engines: i64 = cache
            .connection()
            .query_row("SELECT COUNT(DISTINCT engine) FROM run", [], |row| row.get(0))
            .unwrap();
        assert_eq!(engines, 1);
    }
}
