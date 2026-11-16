//! Schema application: create fresh, or wipe and recreate — never migrate.

use rusqlite::Connection;

use crate::error::StatsError;

/// Embedded DDL; its shape is a compatibility break for direct queries, so it also gates a wipe.
const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Bumped when `schema.sql` changes shape. A cache is wiped, never migrated, on a mismatch.
pub const SCHEMA_VERSION: i64 = 11;

/// Compact caches discard the stratum/allomorph split; full caches retain version 11.
pub const COMPACT_SCHEMA_VERSION: i64 = 12;

pub fn supported_schema_version(version: i64) -> bool {
    matches!(version, SCHEMA_VERSION | COMPACT_SCHEMA_VERSION)
}

/// Whether reports must refuse dimensions discarded by the compact writer.
pub fn is_compact(conn: &Connection) -> Result<bool, StatsError> {
    Ok(cache_identity(conn)?.is_some_and(|(_, version, _, _)| version == COMPACT_SCHEMA_VERSION))
}

/// Bumped when counter meaning or object attribution changes; recorded per run rather than wiped on.
pub use pg_rules::stats::COUNTER_SEMANTICS_VERSION;

pub(crate) fn has_run_table(conn: &Connection) -> Result<bool, StatsError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'run'",
        [],
        |row| row.get(0),
    )?;
    Ok(count != 0)
}

pub(crate) fn create(conn: &Connection, grammar_hash: Option<&str>) -> Result<(), StatsError> {
    create_version(conn, grammar_hash, SCHEMA_VERSION)
}

pub(crate) fn create_version(
    conn: &Connection,
    grammar_hash: Option<&str>,
    version: i64,
) -> Result<(), StatsError> {
    // An empty legacy cache has no run signature to trigger recreation, so remove stale coverage here.
    conn.execute_batch("DROP TABLE IF EXISTS coverage;")?;
    if version == COMPACT_SCHEMA_VERSION {
        let sql = SCHEMA_SQL
            .replace(
                "stratum_id       INTEGER NOT NULL",
                "stratum_id       INTEGER NOT NULL CHECK (stratum_id = 0)",
            )
            .replace(
                "allomorph_id     INTEGER NOT NULL",
                "allomorph_id     INTEGER NOT NULL CHECK (allomorph_id = 0)",
            )
            .replace(
                "PRIMARY KEY (word_id, object_id, stratum_id, allomorph_id, direction)",
                "PRIMARY KEY (word_id, object_id, direction)",
            );
        conn.execute_batch(&sql)?;
    } else {
        conn.execute_batch(SCHEMA_SQL)?;
    }
    seed_sentinels(conn)?;
    if let Some(grammar_hash) = grammar_hash {
        conn.execute(
            "INSERT INTO cache_identity (cache_id, schema_version, grammar_hash, engine)
             VALUES (1, ?1, ?2, NULL)",
            rusqlite::params![version, grammar_hash],
        )?;
    }
    Ok(())
}

fn seed_sentinels(conn: &Connection) -> Result<(), StatsError> {
    conn.execute(
        "INSERT OR IGNORE INTO stratum (stratum_id, key, label) VALUES (0, NULL, 'not applicable')",
        [],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO allomorph (allomorph_id, key, label) VALUES (0, NULL, 'NONE')",
        [],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO morpheme (morpheme_id, key, label) VALUES (0, NULL, 'NONE')",
        [],
    )?;
    Ok(())
}

pub(crate) fn wipe_and_recreate(conn: &Connection, grammar_hash: &str) -> Result<(), StatsError> {
    wipe_and_recreate_version(conn, grammar_hash, SCHEMA_VERSION)
}

pub(crate) fn wipe_and_recreate_version(
    conn: &Connection,
    grammar_hash: &str,
    version: i64,
) -> Result<(), StatsError> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS fact;
         DROP TABLE IF EXISTS word;
         DROP TABLE IF EXISTS coverage;
         DROP TABLE IF EXISTS allomorph;
         DROP TABLE IF EXISTS stratum;
         DROP TABLE IF EXISTS morpheme;
         DROP TABLE IF EXISTS object;
         DROP TABLE IF EXISTS run;
         DROP TABLE IF EXISTS cache_identity;",
    )?;
    create_version(conn, Some(grammar_hash), version)
}

/// The latest run's `(schema_version, grammar_hash)`, or `None` if `run` is absent or empty.
pub(crate) fn latest_run_signature(conn: &Connection) -> Result<Option<(i64, String)>, StatsError> {
    if !has_run_table(conn)? {
        return Ok(None);
    }
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT schema_version, grammar_hash FROM run ORDER BY run_id DESC LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(Into::into)
}

/// The `cache_identity` row as stored: cache id, schema version, grammar hash, engine.
pub(crate) type CacheIdentity = (i64, i64, String, Option<String>);

/// The durable singleton identity, or `None` for a legacy/pre-schema-6 cache.
pub(crate) fn cache_identity(conn: &Connection) -> Result<Option<CacheIdentity>, StatsError> {
    let table_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'cache_identity'",
        [],
        |row| row.get(0),
    )?;
    if table_exists == 0 {
        return Ok(None);
    }
    use rusqlite::OptionalExtension;
    conn.query_row(
        "SELECT cache_id, schema_version, grammar_hash, engine
         FROM cache_identity WHERE cache_id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )
    .optional()
    .map_err(Into::into)
}

#[cfg(test)]
mod tests;
