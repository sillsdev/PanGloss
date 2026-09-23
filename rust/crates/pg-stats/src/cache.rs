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
use crate::step_cap::StepCap;
use crate::util::to_i64;

/// An open stats cache: one SQLite connection, WAL mode, a caller-chosen busy timeout.
pub struct StatsCache {
    conn: Connection,
    grammar_hash: String,
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

    fn open_on(mut conn: Connection, grammar_hash: &str) -> Result<OpenOutcome, StatsError> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let identity = schema::cache_identity(&tx)?;
        let run_table_exists = schema::has_run_table(&tx)?;
        let wiped = match identity {
            None if !run_table_exists => {
                schema::create(&tx, Some(grammar_hash))?;
                false
            }
            None => {
                schema::wipe_and_recreate(&tx, grammar_hash)?;
                true
            }
            Some((_, stored_schema_version, stored_grammar_hash, _)) => {
                let latest = schema::latest_run_signature(&tx)?;
                let run_mismatch = latest.is_some_and(|(version, hash)| {
                    version != schema::SCHEMA_VERSION || hash != grammar_hash
                });
                if !run_table_exists
                    || stored_schema_version != schema::SCHEMA_VERSION
                    || stored_grammar_hash != grammar_hash
                    || run_mismatch
                {
                    schema::wipe_and_recreate(&tx, grammar_hash)?;
                    true
                } else {
                    false
                }
            }
        };
        tx.commit()?;
        Ok(OpenOutcome {
            cache: StatsCache {
                conn,
                grammar_hash: grammar_hash.to_string(),
            },
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

    /// Errors when this cache already holds a run recorded under a step cap other than
    /// `requested`. Unlike engine or grammar hash, an absent recorded step cap imposes no
    /// constraint by itself (a run that recorded none, e.g. `foma`, cannot conflict with anything)
    /// -- only two differing recorded values conflict. Call this before deciding which words to
    /// skip (`existing_words`): a cache hit produced under a different cap is not interchangeable
    /// with one produced under `requested`, since a word that hit the old cap might complete under
    /// a larger one, or vice versa.
    pub fn refuse_if_step_cap_differs(&self, requested: StepCap) -> Result<(), StatsError> {
        let requested_storage = requested.to_storage()?;
        match conflicting_step_cap(&self.conn, requested_storage)? {
            Some(existing_storage) => Err(step_cap_mismatch(existing_storage, requested)),
            None => Ok(()),
        }
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

        let Some((_, identity_schema_version, identity_grammar_hash, identity_engine)) =
            schema::cache_identity(&tx)?
        else {
            return Err(StatsError::CacheIdentityMissing);
        };
        if identity_schema_version != schema::SCHEMA_VERSION {
            return Err(StatsError::SchemaMismatch {
                existing: identity_schema_version,
                requested: schema::SCHEMA_VERSION,
            });
        }
        if identity_grammar_hash != self.grammar_hash {
            return Err(StatsError::GrammarMismatch {
                existing: identity_grammar_hash,
                requested: self.grammar_hash.clone(),
            });
        }
        if run.grammar_hash != self.grammar_hash {
            return Err(StatsError::GrammarMismatch {
                existing: self.grammar_hash.clone(),
                requested: run.grammar_hash.clone(),
            });
        }

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
        if let Some(existing) = identity_engine {
            if existing != run.engine {
                return Err(StatsError::EngineMismatch {
                    existing,
                    requested: run.engine.clone(),
                });
            }
        } else {
            tx.execute(
                "UPDATE cache_identity SET engine = ?1 WHERE cache_id = 1 AND engine IS NULL",
                params![&run.engine],
            )?;
        }

        let step_cap_storage = run.step_cap.map(StepCap::to_storage).transpose()?;
        if let Some(requested_storage) = step_cap_storage {
            if let Some(existing_storage) = conflicting_step_cap(&tx, requested_storage)? {
                return Err(step_cap_mismatch(
                    existing_storage,
                    run.step_cap
                        .expect("step_cap_storage is only Some when run.step_cap is Some"),
                ));
            }
        }

        tx.execute(
            "INSERT INTO run (schema_version, counter_semantics, build_info, fwdata_path, grammar_hash, engine, options_hash, options_json, created_utc, word_count, total_elapsed_ns, step_cap)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                step_cap_storage,
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

/// The stored step cap of any run in `run` that differs from `requested_storage`, if any.
fn conflicting_step_cap(
    conn: &Connection,
    requested_storage: i64,
) -> Result<Option<i64>, StatsError> {
    conn.query_row(
        "SELECT step_cap FROM run WHERE step_cap IS NOT NULL AND step_cap <> ?1 LIMIT 1",
        params![requested_storage],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn step_cap_mismatch(existing_storage: i64, requested: StepCap) -> StatsError {
    StatsError::StepCapMismatch {
        existing: StepCap::from_storage(existing_storage).to_string(),
        requested: requested.to_string(),
    }
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
mod tests;
