use pg_grammar::compile::CompiledAllomorphOrder;
use rusqlite::{params, Transaction};

use crate::FactsError;

pub(crate) fn insert_compiled_order(
    tx: &Transaction<'_>,
    rows: &[CompiledAllomorphOrder],
) -> Result<(), FactsError> {
    for row in rows {
        tx.execute(
            "INSERT INTO compiled_allomorph_order(owner_output_id, source_entry_guid, source_msa_guid, source_allomorph_key, source_allomorph_guid, output_id, compiled_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                row.owner_output_id.map(i64::from),
                row.source_entry_guid,
                row.source_msa_guid,
                row.source_allomorph_guid.as_deref().unwrap_or(""),
                row.source_allomorph_guid,
                row.output_id.map(i64::from),
                row.compiled_order.map(i64::from),
            ],
        )?;
    }
    Ok(())
}
