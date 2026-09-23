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
