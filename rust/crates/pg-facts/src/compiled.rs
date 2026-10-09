use pg_grammar::compile::{CompiledAllomorphOrder, CompiledMapping, CompiledOutput};
use rusqlite::{params, Transaction};

use crate::FactsError;

pub(crate) fn insert(
    tx: &Transaction<'_>,
    outputs: &[CompiledOutput],
    mappings: &[CompiledMapping],
    allomorph_order: &[CompiledAllomorphOrder],
) -> Result<(), FactsError> {
    for output in outputs {
        let conditioning = output.conditioning.as_ref();
        tx.execute(
            "INSERT INTO compiled_output(output_id, kind, key, owner_output_id, stratum_key, bucket, compiled_order, identity_quality, realization_kind, has_phone_condition, has_morph_gate, gate_signature, is_unconditioned) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                i64::from(output.output_id),
                output.kind.as_str(),
                output.key,
                output.owner_output_id.map(i64::from),
                output.stratum_key,
                output.bucket,
                output.compiled_order.map(i64::from),
                output.identity_quality,
                conditioning.map(|c| c.realization_kind),
                conditioning.map(|c| i64::from(c.has_phone_condition)),
                conditioning.map(|c| i64::from(c.has_morph_gate)),
                conditioning.map(|c| c.gate_signature.as_str()),
                conditioning.map(|c| i64::from(c.is_unconditioned)),
            ],
        )?;
    }
    for mapping in mappings {
        tx.execute(
            "INSERT INTO compiled_mapping(source_kind, source_guid, source_key, output_id, relation_role, source_ordinal, identity_quality) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                mapping.source_kind,
                mapping.source_guid,
                mapping.source_key,
                i64::from(mapping.output_id),
                mapping.relation_role,
                i64::from(mapping.source_ordinal),
                mapping.identity_quality,
            ],
        )?;
    }
    crate::variants::insert_compiled_order(tx, allomorph_order)
}
