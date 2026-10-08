use pg_grammar::compile::CompiledAllomorphOrder;
use rusqlite::{params, Transaction};

use crate::FactsError;

pub(crate) fn insert_compiled_order(
    tx: &Transaction<'_>,
    rows: &[CompiledAllomorphOrder],
) -> Result<(), FactsError> {
    for row in rows {
        tx.execute(
            "INSERT INTO compiled_allomorph_order(owner_key, source_entry_guid, source_msa_guid, bucket, source_allomorph_key, source_allomorph_guid, output_key, compiled_order, is_final_elsewhere_case) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                row.owner_key,
                row.source_entry_guid,
                row.source_msa_guid,
                row.bucket,
                row.source_allomorph_guid.as_deref().unwrap_or(""),
                row.source_allomorph_guid,
                row.output_key.as_deref().unwrap_or(""),
                row.compiled_order.map(i64::from),
                i64::from(row.is_final_elsewhere_case),
            ],
        )?;
    }
    Ok(())
}
