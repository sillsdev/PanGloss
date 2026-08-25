//! Schema application: create fresh, or wipe and recreate — never migrate.

use rusqlite::Connection;

use crate::error::StatsError;

/// Embedded DDL; its shape is a compatibility break for direct queries, so it also gates a wipe.
const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Bumped when `schema.sql` changes shape. A cache is wiped, never migrated, on a mismatch.
pub const SCHEMA_VERSION: i64 = 6;

/// Bumped by hand when what a counter means changes; recorded per run rather than wiped on.
pub const COUNTER_SEMANTICS_VERSION: i64 = 2;

pub(crate) fn has_run_table(conn: &Connection) -> Result<bool, StatsError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'run'",
        [],
        |row| row.get(0),
    )?;
    Ok(count != 0)
}

pub(crate) fn create(conn: &Connection, grammar_hash: Option<&str>) -> Result<(), StatsError> {
    // An empty legacy cache has no run signature to trigger recreation, so remove stale coverage here.
    conn.execute_batch("DROP TABLE IF EXISTS coverage;")?;
    conn.execute_batch(SCHEMA_SQL)?;
    seed_sentinels(conn)?;
    if let Some(grammar_hash) = grammar_hash {
        conn.execute(
            "INSERT INTO cache_identity (cache_id, schema_version, grammar_hash, engine)
             VALUES (1, ?1, ?2, NULL)",
            rusqlite::params![SCHEMA_VERSION, grammar_hash],
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

pub(crate) fn wipe_and_recreate(
    conn: &Connection,
    grammar_hash: &str,
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
    create(conn, Some(grammar_hash))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A nullable `UNIQUE` key column exempts NULL from uniqueness, so both sentinels must coexist.
    #[test]
    fn sentinel_rows_seed_under_unique_key_indexes() {
        let conn = Connection::open_in_memory().unwrap();
        create(&conn, None).unwrap();

        let stratum_key: Option<String> = conn
            .query_row("SELECT key FROM stratum WHERE stratum_id = 0", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stratum_key, None);
        let allomorph_key: Option<String> = conn
            .query_row(
                "SELECT key FROM allomorph WHERE allomorph_id = 0",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(allomorph_key, None);
        let morpheme_key: Option<String> = conn
            .query_row(
                "SELECT key FROM morpheme WHERE morpheme_id = 0",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(morpheme_key, None);

        conn.execute(
            "INSERT INTO stratum (key, label) VALUES ('0:Root', 'Root')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO allomorph (key, label) VALUES ('allo-a', 'Allo A')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO morpheme (key, label) VALUES ('morph-a', 'Morph A')",
            [],
        )
        .unwrap();

        // Re-seeding (as a reopen without a flush does) must stay a no-op, not a conflict.
        seed_sentinels(&conn).unwrap();
    }

    #[test]
    fn wipe_removes_the_obsolete_coverage_table() {
        let conn = Connection::open_in_memory().unwrap();
        create(&conn, None).unwrap();
        conn.execute_batch(
            "CREATE TABLE coverage (
                run_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                counter TEXT NOT NULL,
                state TEXT NOT NULL
            );",
        )
        .unwrap();

        wipe_and_recreate(&conn, "hash-a").unwrap();

        let coverage_tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'coverage'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            coverage_tables, 0,
            "a schema wipe must remove the retired coverage table"
        );
    }

    #[test]
    fn create_removes_obsolete_coverage_from_an_empty_legacy_cache() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE coverage (
                run_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                counter TEXT NOT NULL,
                state TEXT NOT NULL
            );",
        )
        .unwrap();
        create(&conn, None).unwrap();
        let coverage_tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'coverage'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(coverage_tables, 0);
    }
}

/// The durable singleton identity, or `None` for a legacy/pre-schema-6 cache.
pub(crate) fn cache_identity(
    conn: &Connection,
) -> Result<Option<(i64, i64, String, Option<String>)>, StatsError> {
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
