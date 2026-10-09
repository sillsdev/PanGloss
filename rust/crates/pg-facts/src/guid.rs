use pg_snapshot::canonical_guid;
use rusqlite::{types::Value, Transaction};

use crate::FactsError;

/// The one column whose GUID text is kept as the importer wrote it.
const RAW_GUID_COLUMN: &str = "raw_guid";

/// Lowercases every stored GUID, leaving the Snapshot the compiler reads untouched.
pub(crate) fn canonicalize_stored_guids(tx: &Transaction<'_>) -> Result<(), FactsError> {
    tx.execute_batch("PRAGMA defer_foreign_keys = ON;")?;
    for table in stored_tables(tx)? {
        let columns = table_columns(tx, &table)?;
        let mut statement = tx.prepare(&format!("SELECT * FROM \"{table}\""))?;
        let mut rows: Vec<Vec<Value>> = statement
            .query_map([], |row| {
                (0..columns.len())
                    .map(|index| row.get::<_, Value>(index))
                    .collect()
            })?
            .collect::<Result<_, _>>()?;
        drop(statement);
        let mut changed = false;
        for row in &mut rows {
            for (value, column) in row.iter_mut().zip(&columns) {
                if column == RAW_GUID_COLUMN {
                    continue;
                }
                if let Value::Text(text) = value {
                    if let Some(lowered) = lowercase_embedded_guids(text) {
                        *text = lowered;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            continue;
        }
        tx.execute(&format!("DELETE FROM \"{table}\""), [])?;
        let placeholders = vec!["?"; columns.len()].join(", ");
        let mut insert = tx.prepare(&format!("INSERT INTO \"{table}\" VALUES ({placeholders})"))?;
        for row in &rows {
            insert.execute(rusqlite::params_from_iter(row.iter()))?;
        }
    }
    Ok(())
}

fn stored_tables(tx: &Transaction<'_>) -> Result<Vec<String>, FactsError> {
    let mut statement = tx.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' \
         AND name <> 'artifact_meta' ORDER BY name",
    )?;
    let names = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(names)
}

fn table_columns(tx: &Transaction<'_>, table: &str) -> Result<Vec<String>, FactsError> {
    let mut statement = tx.prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))?;
    let names = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(names)
}

/// Returns the text with each embedded GUID lowercased, or `None` when nothing changes.
fn lowercase_embedded_guids(text: &str) -> Option<String> {
    const GUID_LEN: usize = 36;
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut changed = false;
    while index < bytes.len() {
        let end = index + GUID_LEN;
        let starts_guid = end <= bytes.len()
            && (index == 0 || !is_guid_byte(bytes[index - 1]))
            && (end == bytes.len() || !is_guid_byte(bytes[end]))
            && text.get(index..end).and_then(canonical_guid).is_some();
        if starts_guid {
            let window = text[index..end].to_ascii_lowercase();
            changed |= window != text[index..end];
            out.extend_from_slice(window.as_bytes());
            index = end;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    changed.then(|| String::from_utf8(out).expect("GUID rewrite keeps UTF-8 intact"))
}

fn is_guid_byte(byte: u8) -> bool {
    byte.is_ascii_hexdigit() || byte == b'-'
}
