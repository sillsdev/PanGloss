//! Schema application: create fresh, or wipe and recreate — never migrate.

use rusqlite::Connection;

use crate::error::StatsError;

/// Embedded DDL; its shape is a compatibility break for direct queries, so it also gates a wipe.
const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Bumped when `schema.sql` changes shape. A cache is wiped, never migrated, on a mismatch.
pub const SCHEMA_VERSION: i64 = 3;

/// Bumped by hand when what a counter means changes; recorded per run rather than wiped on.
pub const COUNTER_SEMANTICS_VERSION: i64 = 2;

pub(crate) fn create(conn: &Connection) -> Result<(), StatsError> {
    conn.execute_batch(SCHEMA_SQL)?;
    seed_sentinels(conn)
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
    Ok(())
}

pub(crate) fn wipe_and_recreate(conn: &Connection) -> Result<(), StatsError> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS fact;
         DROP TABLE IF EXISTS word;
         DROP TABLE IF EXISTS coverage;
         DROP TABLE IF EXISTS allomorph;
         DROP TABLE IF EXISTS stratum;
         DROP TABLE IF EXISTS object;
         DROP TABLE IF EXISTS run;",
    )?;
    create(conn)
}

/// The latest run's `(schema_version, grammar_hash)`, or `None` if `run` is absent or empty.
pub(crate) fn latest_run_signature(conn: &Connection) -> Result<Option<(i64, String)>, StatsError> {
    let table_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'run'",
        [],
        |row| row.get(0),
    )?;
    if table_exists == 0 {
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
        create(&conn).unwrap();

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

        // Re-seeding (as a reopen without a flush does) must stay a no-op, not a conflict.
        seed_sentinels(&conn).unwrap();
    }
}
