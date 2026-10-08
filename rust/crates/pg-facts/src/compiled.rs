use pg_grammar::compile::{CompiledAllomorphOrder, CompiledMapping};
use rusqlite::{params, Transaction};

use crate::FactsError;

pub(crate) fn insert(
    tx: &Transaction<'_>,
    mappings: &[CompiledMapping],
    allomorph_order: &[CompiledAllomorphOrder],
) -> Result<(), FactsError> {
    for mapping in mappings {
        tx.execute(
            "INSERT INTO compiled_mapping(source_kind, source_guid, source_key, output_kind, output_key, identity_quality) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                mapping.source_kind,
                mapping.source_guid,
                mapping.source_key,
                mapping.output_kind,
                mapping.output_key,
                mapping.identity_quality,
            ],
        )?;
    }
    crate::variants::insert_compiled_order(tx, allomorph_order)
}
