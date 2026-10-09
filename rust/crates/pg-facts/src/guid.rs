use pg_snapshot::canonical_guid;
use rusqlite::{types::Value, Transaction};

use crate::FactsError;

/// Stored GUID-bearing columns by table; free text, JSON and stats fact rows are never listed.
const GUID_COLUMNS: &[(&str, &[&str])] = &[
    (
        "category",
        &["guid", "parent_guid", "default_inflection_class_guid"],
    ),
    ("category_feature", &["category_guid", "feature_guid"]),
    (
        "inflection_class",
        &["guid", "owner_category_guid", "parent_guid"],
    ),
    ("category_ancestor", &["category_guid", "ancestor_guid"]),
    (
        "inflection_class_ancestor",
        &["inflection_class_guid", "ancestor_guid"],
    ),
    ("affix_slot", &["guid", "category_guid"]),
    ("affix_template", &["guid", "category_guid"]),
    ("template_slot", &["template_guid", "slot_guid"]),
    ("lex_entry", &["guid"]),
    ("allomorph", &["guid", "entry_guid", "stem_name_guid"]),
    ("allomorph_form", &["allomorph_guid"]),
    ("allomorph_gate", &["allomorph_guid", "target_guid"]),
    ("entry_citation_form", &["entry_guid"]),
    ("msa", &["msa_guid", "entry_guid"]),
    ("msa_category", &["msa_guid", "category_guid"]),
    ("msa_slot", &["msa_guid", "slot_guid"]),
    ("msa_inflection_class", &["msa_guid", "class_guid"]),
    ("msa_stem_name", &["msa_guid", "stem_name_guid"]),
    ("msa_exception_feature", &["msa_guid", "target_guid"]),
    ("sense", &["sense_guid", "entry_guid", "msa_guid"]),
    ("sense_text", &["sense_guid"]),
    (
        "entry_variant",
        &["variant_entry_guid", "ref_guid", "component_guid"],
    ),
    ("entry_variant_type", &["ref_guid", "type_guid"]),
    ("adhoc_prohibition", &["prohibition_guid", "primary_guid"]),
    ("adhoc_other", &["prohibition_guid", "target_guid"]),
    ("adhoc_group", &["group_guid"]),
    ("adhoc_group_text", &["group_guid"]),
    ("adhoc_group_member", &["group_guid", "member_guid"]),
    ("source_object", &["source_key", "canonical_guid"]),
    ("conversion_item", &["subject_key", "subject_guid"]),
    ("conversion_issue", &["issue_key", "source_guid"]),
    (
        "load_fact",
        &["subject_key", "subject_guid", "context_key", "issue_key"],
    ),
    ("object_state", &["subject_guid"]),
    ("compiled_output", &["key", "stratum_key", "gate_signature"]),
    ("compiled_mapping", &["source_guid", "source_key"]),
    (
        "compiled_allomorph_order",
        &[
            "source_entry_guid",
            "source_msa_guid",
            "source_allomorph_key",
            "source_allomorph_guid",
        ],
    ),
    (
        "compiled_form_segment",
        &["phoneme_guid", "boundary_guid", "natural_class_guid"],
    ),
    ("stats_morpheme", &["key"]),
    ("stats_stratum", &["key"]),
    ("stats_allomorph", &["key"]),
    ("stats_object", &["key"]),
    ("stats_object_source", &["source_guid"]),
    ("stats_allomorph_source", &["source_allomorph_guid"]),
    ("phoneme_set", &["guid"]),
    ("phoneme", &["guid"]),
    ("phoneme_grapheme", &["phoneme_guid"]),
    ("boundary_marker", &["guid"]),
    ("boundary_grapheme", &["boundary_guid"]),
    ("feature", &["guid", "feature_type_guid"]),
    ("feature_value", &["guid", "feature_guid"]),
    ("feature_structure", &["owner_guid"]),
    ("feature_assignment", &["feature_guid", "value_guid"]),
    ("natural_class", &["guid"]),
    (
        "natural_class_member",
        &["natural_class_guid", "phoneme_guid"],
    ),
    (
        "natural_class_effective_member",
        &[
            "natural_class_guid",
            "table_key",
            "member_key",
            "phoneme_guid",
        ],
    ),
    ("feature_constraint", &["guid", "feature_guid"]),
    (
        "allomorph_environment",
        &["allomorph_guid", "environment_guid"],
    ),
    ("environment", &["guid"]),
    (
        "environment_natural_class",
        &[
            "environment_guid",
            "compile_context_key",
            "natural_class_guid",
        ],
    ),
    ("environment_side", &["environment_guid", "canonical_key"]),
    (
        "environment_side_member",
        &[
            "environment_guid",
            "member_key",
            "phoneme_guid",
            "boundary_guid",
        ],
    ),
    (
        "environment_usage",
        &[
            "allomorph_guid",
            "compile_context_key",
            "environment_guid",
            "resolved_environment_guid",
            "load_subject_key",
            "load_context_key",
        ],
    ),
    ("phonological_rule", &["guid", "effective_stratum_key"]),
    (
        "phonological_rule_variable",
        &["rule_guid", "feature_constraint_guid"],
    ),
    ("rewrite_rhs", &["rule_guid"]),
    ("rewrite_rhs_pos", &["rule_guid", "category_guid"]),
    ("rewrite_rhs_rule_feature", &["rule_guid", "target_guid"]),
    ("stratum", &["stratum_key", "table_key"]),
    ("rule_stratum", &["rule_guid", "stratum_key"]),
    ("pattern_root", &["owner_guid"]),
    (
        "pattern_node",
        &["phoneme_guid", "natural_class_guid", "boundary_guid"],
    ),
    ("affix_process_input", &["allomorph_guid"]),
    (
        "affix_process_output",
        &["allomorph_guid", "natural_class_guid"],
    ),
    ("compound_rule", &["guid"]),
    (
        "compound_rule_side",
        &["rule_guid", "category_guid", "inflection_class_guid"],
    ),
    (
        "compound_rule_exception_feature",
        &["rule_guid", "target_guid"],
    ),
    ("stem_name", &["guid", "category_guid"]),
    ("stem_name_region", &["stem_name_guid"]),
    ("exception_feature", &["guid"]),
    ("lex_entry_infl_type", &["guid"]),
    ("lex_entry_infl_type_slot", &["infl_type_guid", "slot_guid"]),
    ("pattern_variable", &["feature_constraint_guid"]),
    ("statement_reference", &["target_guid", "referrer_guid"]),
];

/// Columns the pass deliberately leaves alone, each with the reason its text must stay as written.
#[cfg(test)]
const EXCLUDED_COLUMNS: &[(&str, &str, &str)] = &[
    (
        "source_object",
        "raw_guid",
        "the authored spelling of the source header",
    ),
    (
        "parser_config",
        "setting_key",
        "a setting name, not an identity",
    ),
];

/// Tables the pass never reads: run metadata and digests whose text is exact.
#[cfg(test)]
const EXCLUDED_TABLES: &[&str] = &["artifact_meta"];

/// Lowercases the GUIDs embedded in listed columns, rewriting only the rows that change.
pub(crate) fn canonicalize_stored_guids(tx: &Transaction<'_>) -> Result<(), FactsError> {
    tx.execute_batch("PRAGMA defer_foreign_keys = ON;")?;
    for (table, columns) in GUID_COLUMNS {
        rewrite_table(tx, table, columns)?;
    }
    Ok(())
}

fn rewrite_table(tx: &Transaction<'_>, table: &str, columns: &[&str]) -> Result<(), FactsError> {
    let identity = row_identity(tx, table)?;
    let predicate = columns
        .iter()
        .map(|column| format!("\"{column}\" <> lower(\"{column}\")"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let select = identity
        .iter()
        .map(String::as_str)
        .chain(columns.iter().copied())
        .map(|name| {
            if name == "rowid" {
                name.to_string()
            } else {
                format!("\"{name}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let changed: Vec<(Vec<Value>, Vec<Value>)> = {
        let mut statement = tx.prepare(&format!(
            "SELECT {select} FROM \"{table}\" WHERE {predicate}"
        ))?;
        let width = identity.len();
        let rows = statement.query_map([], |row| {
            let key = (0..width)
                .map(|index| row.get::<_, Value>(index))
                .collect::<Result<Vec<_>, _>>()?;
            let values = (width..width + columns.len())
                .map(|index| row.get::<_, Value>(index))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((key, values))
        })?;
        let mut changed = Vec::new();
        for row in rows {
            let (key, values) = row?;
            let mut any_change = false;
            let lowered: Vec<Value> = values
                .into_iter()
                .map(|value| match &value {
                    Value::Text(text) => match lowercase_embedded_guids(text) {
                        Some(lowered) => {
                            any_change = true;
                            Value::Text(lowered)
                        }
                        None => value,
                    },
                    _ => value,
                })
                .collect();
            if any_change {
                changed.push((key, lowered));
            }
        }
        changed
    };
    if changed.is_empty() {
        return Ok(());
    }
    let assignments = columns
        .iter()
        .enumerate()
        .map(|(index, column)| format!("\"{column}\" = ?{}", index + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let filter = identity
        .iter()
        .enumerate()
        .map(|(index, name)| format!("{name} = ?{}", columns.len() + index + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    let mut update = tx.prepare(&format!(
        "UPDATE \"{table}\" SET {assignments} WHERE {filter}"
    ))?;
    for (key, values) in &changed {
        let params = values.iter().chain(key.iter());
        update.execute(rusqlite::params_from_iter(params))?;
    }
    Ok(())
}

/// The columns that identify one row: its rowid, or the primary key of a WITHOUT ROWID table.
fn row_identity(tx: &Transaction<'_>, table: &str) -> Result<Vec<String>, FactsError> {
    let sql: String = tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?;
    if !sql.to_ascii_uppercase().contains("WITHOUT ROWID") {
        return Ok(vec!["rowid".to_string()]);
    }
    let mut statement =
        tx.prepare("SELECT name FROM pragma_table_info(?1) WHERE pk > 0 ORDER BY pk")?;
    let names = statement
        .query_map([table], |row| row.get(0))?
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

/// A column named like a GUID or key must be classified, so a new one fails here until listed.
#[cfg(test)]
fn is_identity_column(name: &str) -> bool {
    name == "guid" || name == "key" || name.ends_with("_guid") || name.ends_with("_key")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn schema() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("schema.sql")).unwrap();
        db
    }

    fn names(db: &Connection, sql: &str, arg: Option<&str>) -> Vec<String> {
        let mut statement = db.prepare(sql).unwrap();
        let args: Vec<&str> = arg.into_iter().collect();
        statement
            .query_map(rusqlite::params_from_iter(args), |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn every_guid_and_key_column_is_listed_or_excluded_with_a_reason() {
        let db = schema();
        let tables = names(
            &db,
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            None,
        );
        let mut unclassified = Vec::new();
        for table in tables
            .iter()
            .filter(|table| !EXCLUDED_TABLES.contains(&table.as_str()))
        {
            let columns = names(&db, "SELECT name FROM pragma_table_info(?1)", Some(table));
            for column in columns.iter().filter(|column| is_identity_column(column)) {
                let listed = GUID_COLUMNS
                    .iter()
                    .any(|(t, cols)| t == table && cols.contains(&column.as_str()));
                let excluded = EXCLUDED_COLUMNS
                    .iter()
                    .any(|(t, c, _)| t == table && c == column);
                if !listed && !excluded {
                    unclassified.push(format!("{table}.{column}"));
                }
            }
        }
        assert!(
            unclassified.is_empty(),
            "unclassified GUID/key columns: {unclassified:?}"
        );
    }

    #[test]
    fn every_listed_or_excluded_column_exists_in_the_schema() {
        let db = schema();
        for (table, columns) in GUID_COLUMNS {
            let present = names(&db, "SELECT name FROM pragma_table_info(?1)", Some(table));
            for column in *columns {
                assert!(
                    present.iter().any(|name| name == column),
                    "{table}.{column} is not in the schema"
                );
            }
        }
        for (table, column, reason) in EXCLUDED_COLUMNS {
            let present = names(&db, "SELECT name FROM pragma_table_info(?1)", Some(table));
            assert!(
                present.iter().any(|name| name == column),
                "{table}.{column} is not in the schema"
            );
            assert!(!reason.is_empty());
        }
    }

    #[test]
    fn lowercases_only_guid_tokens_and_reports_when_nothing_changes() {
        let upper = "morph_rule:D7F71344-6E6E-4E5B-8E2A-0A1B2C3D4E5F@Morphology";
        assert_eq!(
            lowercase_embedded_guids(upper).as_deref(),
            Some("morph_rule:d7f71344-6e6e-4e5b-8e2a-0a1b2c3d4e5f@Morphology")
        );
        assert_eq!(lowercase_embedded_guids("lower-case text"), None);
        let glued = "x D7F71344-6E6E-4E5B-8E2A-0A1B2C3D4E5FD";
        assert_eq!(
            lowercase_embedded_guids(glued),
            None,
            "a longer hex run is not a GUID"
        );
    }

    #[test]
    fn rewrites_listed_columns_and_leaves_free_text_alone() {
        let mut db = schema();
        let upper = "D7F71344-6E6E-4E5B-8E2A-0A1B2C3D4E5F";
        let lower = upper.to_ascii_lowercase();
        db.execute(
            "INSERT INTO category(guid, parent_guid, sibling_ordinal, name, abbreviation, \
             default_inflection_class_guid) VALUES (?1, NULL, 0, ?2, 'N', NULL)",
            rusqlite::params![upper, format!("name {upper}")],
        )
        .unwrap();
        let tx = db.transaction().unwrap();
        canonicalize_stored_guids(&tx).unwrap();
        tx.commit().unwrap();
        let (guid, name): (String, String) = db
            .query_row("SELECT guid, name FROM category", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(guid, lower);
        assert_eq!(
            name,
            format!("name {upper}"),
            "free text keeps its authored case"
        );
    }
}
