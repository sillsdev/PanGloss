use super::*;
use crate::test_support::TempDir;
use crate::{StatsCache, WorkCap};

fn seed(path: &Path, compact: bool) {
    let cache = if compact {
        StatsCache::open_compact(path, "hash-a")
    } else {
        StatsCache::open(path, "hash-a")
    }
    .unwrap()
    .cache;
    cache.connection().execute_batch("INSERT INTO run VALUES (1, 11, 1, 'build', 'test.xml', 'hash-a', 'hc', 'opts', '{}', 'now', 1, 7, -1);
        UPDATE cache_identity SET engine = 'hc';
        INSERT INTO word VALUES (1, 1, 'tok', 7, 9, 2, 0, 0, 0);
        INSERT INTO object VALUES (1, 'rule', 'morph_rule', 'Rule', 'authored', 0);
        INSERT INTO fact VALUES (1, 1, 0, 0, 'analysis', 3, 4, 9, 2, 1, 0, 0, 2, 7);").unwrap();
    if compact {
        cache
            .connection()
            .execute("UPDATE run SET schema_version = 12", [])
            .unwrap();
    }
    cache.checkpoint_and_close().unwrap();
}

fn facts(conn: &Connection) -> Vec<Vec<rusqlite::types::Value>> {
    query_rows(
        conn,
        "SELECT * FROM fact ORDER BY word_id, object_id, direction",
    )
}

fn query_rows(conn: &Connection, sql: &str) -> Vec<Vec<rusqlite::types::Value>> {
    let mut s = conn.prepare(sql).unwrap();
    let n = s.column_count();
    s.query_map([], |r| (0..n).map(|i| r.get(i)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn packed_round_trip_preserves_both_schemas_and_direct_word_query() {
    for compact in [false, true] {
        let dir = TempDir::new("pack-roundtrip");
        let path = dir.path().join("stats.sqlite");
        seed(&path, compact);
        let plain = open_read(&path).unwrap();
        let before = facts(&plain);
        let meta = metadata_digest(&plain).unwrap();
        let word_schema: String = plain
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'word'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        drop(plain);
        pack(&path).unwrap();
        let outer = read_only(&path).unwrap();
        assert!(is_packed(&outer).unwrap());
        let word: (String, i64, i64, i64) = outer
            .query_row(
                "SELECT form, elapsed_ns, passes, work FROM word ORDER BY word_id",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(word, ("tok".into(), 7, 2, 9));
        let packed_word_schema: String = outer
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'word'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(packed_word_schema, word_schema);
        assert!(outer.prepare("SELECT attempts FROM word").is_err());
        assert!(outer.prepare("SELECT * FROM fact").is_err());
        drop(outer);
        let expanded = open_read(&path).unwrap();
        assert_eq!(facts(&expanded), before);
        assert_eq!(metadata_digest(&expanded).unwrap(), meta);
        drop(expanded);
        unpack(&path).unwrap();
        let after = open_read(&path).unwrap();
        assert!(!is_packed(&after).unwrap());
        assert_eq!(facts(&after), before);
        assert_eq!(metadata_digest(&after).unwrap(), meta);
        assert!(after.prepare("SELECT attempts FROM word").is_err());
        assert_eq!(schema::is_compact(&after).unwrap(), compact);
    }
}

#[test]
fn packed_corruption_truncation_and_metadata_changes_refuse_without_mutation() {
    for damage in [
        "truncate",
        "corrupt",
        "identity",
        "word",
        "dimension",
        "version",
        "length",
    ] {
        let dir = TempDir::new("pack-corrupt");
        let path = dir.path().join("stats.sqlite");
        seed(&path, false);
        pack(&path).unwrap();
        let c = Connection::open(&path).unwrap();
        c.execute_batch(match damage {
            "truncate" => {
                "DELETE FROM stats_pack_chunk WHERE seq = (SELECT MAX(seq) FROM stats_pack_chunk)"
            }
            "corrupt" => "UPDATE stats_pack_chunk SET data = x'010203' WHERE seq = 0",
            "identity" => "UPDATE cache_identity SET grammar_hash = 'changed'",
            "word" => "UPDATE word SET work = work + 1",
            "dimension" => "CREATE TABLE object (label TEXT)",
            "version" => "UPDATE stats_pack SET version = 999",
            _ => "UPDATE stats_pack SET unpacked_bytes = unpacked_bytes - 1",
        })
        .unwrap();
        drop(c);
        let bytes = std::fs::read(&path).unwrap();
        let error = open_read(&path).err().expect(damage).to_string();
        assert!(error.contains("packed stats cache"), "{damage}: {error}");
        assert!(unpack(&path).is_err(), "{damage}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(StatsCache::open(&path, "hash-a").is_err(), "{damage}");
    }
}

#[test]
fn packed_stale_writer_refuses_and_reopen_restores_for_resume_or_mode_switch() {
    let dir = TempDir::new("pack-writer");
    let path = dir.path().join("stats.sqlite");
    seed(&path, false);
    let mut stale = StatsCache::open(&path, "hash-a").unwrap().cache;
    pack(&path).unwrap();
    assert!(stale
        .intern_stratum(&crate::StructuralLocator {
            key: "test".into(),
            label: "test".into()
        })
        .is_err());
    let run = crate::RunMetadata {
        build_info: "build".into(),
        fwdata_path: "test.xml".into(),
        grammar_hash: "hash-a".into(),
        engine: "hc".into(),
        options_hash: "opts".into(),
        options_json: "{}".into(),
        created_utc: "now".into(),
        work_cap: Some(WorkCap::Unbounded),
    };
    assert!(stale
        .flush(&run, &[])
        .unwrap_err()
        .to_string()
        .contains("reopen"));
    drop(stale);
    let reopened = StatsCache::open(&path, "hash-a").unwrap();
    assert!(!reopened.wiped);
    assert_eq!(facts(reopened.cache.connection()).len(), 1);
    reopened.cache.checkpoint_and_close().unwrap();
    pack(&path).unwrap();
    let switched = StatsCache::open_compact(&path, "hash-a").unwrap();
    assert!(switched.wiped);
    assert!(schema::is_compact(switched.cache.connection()).unwrap());
}

#[test]
fn packing_plain_defaults_and_missing_paths_are_unchanged() {
    let dir = TempDir::new("pack-default");
    let path = dir.path().join("stats.sqlite");
    seed(&path, false);
    let bytes = std::fs::read(&path).unwrap();
    drop(open_read(&path).unwrap());
    unpack(&path).unwrap();
    assert_eq!(bytes, std::fs::read(&path).unwrap());
    assert!(pack(&dir.path().join("missing")).is_err());
    assert!(!dir.path().join("missing").exists());
}

#[test]
fn multi_chunk_snapshot_round_trip_and_missing_middle_chunk() {
    let dir = TempDir::new("pack-multi");
    let path = dir.path().join("stats.sqlite");
    seed(&path, false);
    let c = Connection::open(&path).unwrap();
    c.execute(
        "UPDATE object SET label = ?1",
        ["q".repeat(3 * CHUNK_BYTES)],
    )
    .unwrap();
    drop(c);
    pack(&path).unwrap();
    assert!(
        std::fs::metadata(&path).unwrap().len() < CHUNK_BYTES as u64,
        "packing must reclaim the original pages"
    );
    let expanded = open_read(&path).unwrap();
    assert_eq!(
        expanded
            .query_row(
                "SELECT length(label) FROM object WHERE object_id = 1",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
        3 * CHUNK_BYTES
    );
    drop(expanded);
    let c = Connection::open(&path).unwrap();
    assert!(
        c.query_row("SELECT COUNT(*) FROM stats_pack_chunk", [], |r| r
            .get::<_, i64>(0))
            .unwrap()
            >= 4
    );
    c.execute("DELETE FROM stats_pack_chunk WHERE seq = 1", [])
        .unwrap();
    drop(c);
    assert!(open_read(&path)
        .err()
        .unwrap()
        .to_string()
        .contains("missing or out-of-order"));
}

const MOTIF_OBJECT_TIMINGS_FROM: &str = r#"FROM fact AS f
JOIN word AS w ON w.word_id = f.word_id
JOIN object AS o ON o.object_id = f.object_id
GROUP BY w.form, o.key, o.identity_quality, o.kind, o.label, f.direction"#;

const MOTIF_BOUNDED_OBJECT_TIMINGS: &str = r#"WITH canonical AS (
    SELECT MIN(object_id) AS id, key, identity_quality, kind, label
    FROM object GROUP BY key, identity_quality, kind, label
), object_ids AS (
    SELECT o.object_id, c.id FROM object o JOIN canonical c
      ON o.key = c.key AND o.identity_quality = c.identity_quality
      AND o.kind = c.kind AND o.label = c.label
), grouped AS (
    SELECT f.word_id, ids.id, f.direction, SUM(f.attempts) AS attempts,
           SUM(f.self_time_ns) AS elapsed, SUM(f.work_steps) AS work
    FROM fact f JOIN object_ids ids ON ids.object_id = f.object_id
    JOIN word w ON w.word_id = f.word_id
    GROUP BY f.word_id, ids.id, f.direction
)
SELECT w.form, o.key, o.identity_quality, o.kind, o.label, g.direction,
       g.attempts, g.elapsed, g.work
FROM grouped g JOIN word w ON w.word_id = g.word_id JOIN object o ON o.object_id = g.id
ORDER BY w.form, o.kind, o.key, g.direction;"#;

#[test]
fn current_motif_fact_queries_require_expansion_and_preserve_results() {
    let object_query = format!("SELECT w.form, o.key, o.identity_quality, o.kind, o.label, f.direction, SUM(f.attempts), SUM(f.self_time_ns), SUM(f.work_steps)\n{MOTIF_OBJECT_TIMINGS_FROM}\nORDER BY w.form, o.kind, o.key, f.direction;");
    let count_query = format!("SELECT COUNT(*) FROM (SELECT 1 {MOTIF_OBJECT_TIMINGS_FROM})");
    for compact in [false, true] {
        let dir = TempDir::new("pack-motif-queries");
        let path = dir.path().join("stats.sqlite");
        seed(&path, compact);
        let c = Connection::open(&path).unwrap();
        c.execute(
            "UPDATE run SET counter_semantics = ?1",
            [crate::COUNTER_SEMANTICS_VERSION],
        )
        .unwrap();
        if !compact {
            c.execute_batch(
                "INSERT INTO stratum VALUES (1, 'zone', 'Zone');
                INSERT INTO allomorph VALUES (1, 'variant', 'Variant');
                INSERT INTO fact VALUES (1, 1, 1, 1, 'analysis', 4, 6, 8, 3, 2, 1, 0, 1, 11);",
            )
            .unwrap();
        }
        let queries = [&*object_query, MOTIF_BOUNDED_OBJECT_TIMINGS, &*count_query];
        let before: Vec<_> = queries.iter().map(|sql| query_rows(&c, sql)).collect();
        assert_eq!(
            before[0], before[1],
            "both Motif fact readers report the same observations"
        );
        assert_eq!(before[0].len(), 1);
        assert_eq!(
            before[0][0][6],
            rusqlite::types::Value::Integer(if compact { 3 } else { 7 })
        );
        drop(c);
        pack(&path).unwrap();
        let outer = read_only(&path).unwrap();
        let version: i64 = outer
            .query_row(
                "SELECT schema_version FROM cache_identity WHERE cache_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, if compact { 12 } else { 11 });
        let semantics: i64 = outer
            .query_row("SELECT counter_semantics FROM run", [], |r| r.get(0))
            .unwrap();
        assert_eq!(semantics, 8);
        for sql in queries {
            assert!(outer
                .prepare(sql)
                .unwrap_err()
                .to_string()
                .contains("no such table"));
        }
        drop(outer);
        let expanded = open_read(&path).unwrap();
        for (sql, rows) in queries.iter().zip(&before) {
            assert_eq!(query_rows(&expanded, sql), *rows);
        }
        drop(expanded);
        unpack(&path).unwrap();
        let plain = read_only(&path).unwrap();
        for (sql, rows) in queries.iter().zip(&before) {
            assert_eq!(query_rows(&plain, sql), *rows);
        }
    }
}
