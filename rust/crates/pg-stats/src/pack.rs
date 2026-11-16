//! A SQLite envelope keeps word reads independent of compressed fact storage.

use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OpenFlags, TransactionBehavior};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::{schema, StatsError};

const CHUNK_BYTES: usize = 1024 * 1024;
const PACK_VERSION: i64 = 1;
const METADATA_TABLES: &[&str] = &["cache_identity", "run", "word"];
const ARCHIVED_TABLES: &[&str] = &["object", "morpheme", "stratum", "allomorph", "fact"];

/// A report connection owns its expanded temporary file until the connection closes.
/// Plain caches are opened read-only without copying or changing their bytes.
pub struct ReadCache {
    conn: Connection,
    _expanded: Option<NamedTempFile>,
}

impl Deref for ReadCache {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl DerefMut for ReadCache {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}

fn invalid(message: impl Into<String>) -> StatsError {
    StatsError::PackedCache(message.into())
}

fn read_only(path: &Path) -> Result<Connection, StatsError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(conn)
}

/// Detects the storage envelope independently of the full/compact logical schema version.
pub fn is_packed(conn: &Connection) -> Result<bool, StatsError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'stats_pack')",
        [],
        |r| r.get(0),
    )?)
}

pub(crate) fn refuse_packed_writer(conn: &Connection) -> Result<(), StatsError> {
    if is_packed(conn)? {
        return Err(invalid(
            "cache was packed; reopen the writer to unpack before writing",
        ));
    }
    Ok(())
}

/// Expands packed caches to a temporary SQLite file; verifies the digest and visible metadata.
/// The source remains unchanged. Uncompressed caches require no temporary file.
pub fn open_read(path: &Path) -> Result<ReadCache, StatsError> {
    let conn = read_only(path)?;
    let snapshot = conn.unchecked_transaction()?;
    if !is_packed(&snapshot)? {
        snapshot.commit()?;
        return Ok(ReadCache {
            conn,
            _expanded: None,
        });
    }
    let expanded = expand(&snapshot).map_err(|e| invalid(e.to_string()))?;
    snapshot.commit()?;
    let expanded_conn = read_only(expanded.path())?;
    Ok(ReadCache {
        conn: expanded_conn,
        _expanded: Some(expanded),
    })
}

fn metadata_digest(conn: &Connection) -> Result<Vec<u8>, StatsError> {
    let mut digest = Sha256::new();
    for table in METADATA_TABLES {
        digest.update(table.as_bytes());
        let mut stmt = conn.prepare(&format!("SELECT * FROM {table} ORDER BY 1"))?;
        let n = stmt.column_count();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            digest.update([0xff]);
            for i in 0..n {
                use rusqlite::types::ValueRef;
                match row.get_ref(i)? {
                    ValueRef::Null => digest.update([0]),
                    ValueRef::Integer(v) => {
                        digest.update([1]);
                        digest.update(v.to_le_bytes());
                    }
                    ValueRef::Real(v) => {
                        digest.update([2]);
                        digest.update(v.to_le_bytes());
                    }
                    ValueRef::Text(v) | ValueRef::Blob(v) => {
                        digest.update([if matches!(row.get_ref(i)?, ValueRef::Text(_)) {
                            3
                        } else {
                            4
                        }]);
                        digest.update((v.len() as u64).to_le_bytes());
                        digest.update(v);
                    }
                }
            }
        }
    }
    Ok(digest.finalize().to_vec())
}

fn expand(conn: &Connection) -> Result<NamedTempFile, StatsError> {
    let (version, expected_bytes, expected_digest): (i64, u64, Vec<u8>) = conn.query_row(
        "SELECT version, unpacked_bytes, sha256 FROM stats_pack WHERE id = 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if version != PACK_VERSION {
        return Err(invalid(format!("unsupported storage version {version}")));
    }
    let mut file = NamedTempFile::new()?;
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut count = 0i64;
    let mut stmt = conn.prepare("SELECT seq, data FROM stats_pack_chunk ORDER BY seq")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let seq: i64 = row.get(0)?;
        if seq != count {
            return Err(invalid("missing or out-of-order archive chunk"));
        }
        let compressed = row
            .get_ref(1)?
            .as_blob()
            .map_err(|e| invalid(e.to_string()))?;
        let bytes = zstd::bulk::decompress(compressed, CHUNK_BYTES)
            .map_err(|e| invalid(format!("corrupt archive chunk {seq}: {e}")))?;
        if bytes.is_empty() || total + bytes.len() as u64 > expected_bytes {
            return Err(invalid("invalid archive length"));
        }
        total += bytes.len() as u64;
        digest.update(&bytes);
        file.write_all(&bytes)?;
        count += 1;
    }
    if total != expected_bytes || total == 0 || digest.finalize().as_slice() != expected_digest {
        return Err(invalid("archive truncated or digest mismatch"));
    }
    file.flush()?;
    let expanded = read_only(file.path())?;
    for table in ARCHIVED_TABLES {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = ?1)",
            [table],
            |r| r.get(0),
        )?;
        if exists {
            return Err(invalid(format!("unexpected live archived table {table}")));
        }
    }
    if is_packed(&expanded)? {
        return Err(invalid("nested packed archive"));
    }
    if metadata_digest(conn)? != metadata_digest(&expanded)? {
        return Err(invalid(
            "identity or visible metadata changed since packing",
        ));
    }
    Ok(file)
}

/// Packs in place, retaining word and identity tables for direct SQLite readers.
/// The source must be a supported stats cache. Packing holds a writer lock; readers can
/// continue seeing the prior committed state. A failure before commit rolls back all changes.
/// Facts are recovered from a vacuumed snapshot in bounded, independently compressed chunks.
pub fn pack(path: &Path) -> Result<(), StatsError> {
    let mut conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if is_packed(&tx)? {
        return Err(invalid("cache is already packed"));
    }
    let (_, version, _, _) =
        schema::cache_identity(&tx)?.ok_or(StatsError::CacheIdentityMissing)?;
    if !schema::supported_schema_version(version) {
        return Err(invalid(format!("unsupported logical schema {version}")));
    }
    let snapshot = NamedTempFile::new()?;
    let source = read_only(path)?;
    source.execute(
        "VACUUM INTO ?1",
        [snapshot.path().to_string_lossy().as_ref()],
    )?;
    drop(source);
    let copy = read_only(snapshot.path())?;
    if metadata_digest(&tx)? != metadata_digest(&copy)? {
        return Err(invalid("snapshot identity changed"));
    }
    copy.prepare("SELECT * FROM fact")?;
    drop(copy);
    tx.execute_batch("CREATE TABLE stats_pack (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL, unpacked_bytes INTEGER NOT NULL, sha256 BLOB NOT NULL);
        CREATE TABLE stats_pack_chunk (seq INTEGER PRIMARY KEY, data BLOB NOT NULL);")?;
    let mut input = std::fs::File::open(snapshot.path())?;
    let mut buf = vec![0; CHUNK_BYTES];
    let mut total = 0u64;
    let mut digest = Sha256::new();
    let mut seq = 0i64;
    loop {
        let mut n = 0;
        while n < buf.len() {
            let read = input.read(&mut buf[n..])?;
            if read == 0 {
                break;
            }
            n += read;
        }
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
        total += n as u64;
        let compressed = zstd::bulk::compress(&buf[..n], 3)?;
        tx.execute(
            "INSERT INTO stats_pack_chunk VALUES (?1, ?2)",
            params![seq, compressed],
        )?;
        seq += 1;
    }
    tx.execute(
        "INSERT INTO stats_pack VALUES (1, ?1, ?2, ?3)",
        params![PACK_VERSION, total, digest.finalize().to_vec()],
    )?;
    tx.execute_batch("DROP TABLE fact; DROP TABLE object; DROP TABLE morpheme; DROP TABLE stratum; DROP TABLE allomorph;")?;
    tx.commit()?;
    compact_file(&conn)
}

fn compact_file(conn: &Connection) -> Result<(), StatsError> {
    conn.execute_batch("VACUUM;")?;
    let (busy, log, checkpointed): (i64, i64, i64) =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    if busy != 0 || log > 0 || (log >= 0 && checkpointed != log) {
        return Err(StatsError::CheckpointIncomplete {
            busy,
            log_frames: log,
            checkpointed_frames: checkpointed,
        });
    }
    Ok(())
}

/// Restores facts in place without changing full/compact mode, word outcomes or run identity.
/// An already plain cache is left unchanged. Frozen manifests must be regenerated after any
/// explicit storage transition because their digest covers exact file bytes.
pub fn unpack(path: &Path) -> Result<(), StatsError> {
    let mut conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    unpack_on(&mut conn)
}

pub(crate) fn unpack_on(conn: &mut Connection) -> Result<(), StatsError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if !is_packed(&tx)? {
        tx.commit()?;
        return Ok(());
    }
    let file = expand(&tx).map_err(|e| invalid(e.to_string()))?;
    let expanded = read_only(file.path())?;
    let mut ddl = Vec::new();
    for table in ARCHIVED_TABLES {
        let table_ddl: String = expanded.query_row(
            "SELECT sql FROM sqlite_master WHERE name = ?1 AND type = 'table'",
            [table],
            |r| r.get(0),
        )?;
        ddl.push(table_ddl);
        let mut indexes = expanded.prepare("SELECT sql FROM sqlite_master WHERE tbl_name = ?1 AND type = 'index' AND sql IS NOT NULL ORDER BY name")?;
        ddl.extend(
            indexes
                .query_map([table], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    drop(expanded);
    tx.execute(
        "ATTACH DATABASE ?1 AS unpacked_stats",
        [file.path().to_string_lossy().as_ref()],
    )?;
    for sql in ddl {
        tx.execute_batch(&sql)?;
    }
    for table in ARCHIVED_TABLES {
        tx.execute_batch(&format!(
            "INSERT INTO main.{table} SELECT * FROM unpacked_stats.{table}"
        ))?;
    }
    tx.execute_batch("DROP TABLE stats_pack_chunk; DROP TABLE stats_pack;")?;
    tx.commit()?;
    conn.execute_batch("DETACH DATABASE unpacked_stats")?;
    compact_file(conn)
}

#[cfg(test)]
mod tests;
