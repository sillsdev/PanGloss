//! Authored allomorph identity, order, and writing-system forms.

use pg_snapshot::lexicon::Allomorph;
use rusqlite::{params, Transaction};

use crate::morphology::checked_guid;
use crate::FactsError;

pub(crate) fn insert_allomorphs(
    tx: &Transaction<'_>,
    entry_guid: &str,
    allomorphs: &[Allomorph],
) -> Result<(), FactsError> {
    for (ordinal, allomorph) in allomorphs.iter().enumerate() {
        let allomorph_guid = checked_guid(&allomorph.guid, "allomorph.guid")?;
        let morph_type = enum_string(&allomorph.morph_type)?;
        let stem_name_guid = allomorph
            .stem_name
            .as_deref()
            .map(|guid| checked_guid(guid, "allomorph.stemName"))
            .transpose()?;
        tx.execute(
            "INSERT INTO allomorph(guid, entry_guid, ordinal, morph_type, is_abstract, stem_name_guid) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                allomorph_guid,
                entry_guid,
                ordinal as i64,
                morph_type,
                if allomorph.is_abstract { 1_i64 } else { 0_i64 },
                stem_name_guid
            ],
        )?;
        for (form_ordinal, form) in allomorph.forms.iter().enumerate() {
            tx.execute(
                "INSERT INTO allomorph_form(allomorph_guid, ordinal, writing_system, form) VALUES (?1, ?2, ?3, ?4)",
                params![allomorph_guid, form_ordinal as i64, form.ws, form.form],
            )?;
        }
    }
    Ok(())
}

fn enum_string<T: serde::Serialize>(value: &T) -> Result<String, FactsError> {
    let value = serde_json::to_value(value)
        .map_err(|error| FactsError::Serialization(error.to_string()))?;
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| FactsError::Serialization("expected a string enum value".into()))
}
