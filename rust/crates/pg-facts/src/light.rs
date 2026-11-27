//! The one list of what `facts --light` omits; writers consult it before computing a row, and the file declares it.

use serde::Serialize;

use crate::metadata::SectionStatus;

/// One thing a light file does not contain, or holds in a smaller form.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "omit", rename_all = "camelCase")]
pub(crate) enum Omit {
    /// No row of `table` is written.
    Table { table: &'static str },
    /// Rows of `table` whose `column` holds one of `values` are not written.
    RowsWhere {
        table: &'static str,
        column: &'static str,
        values: &'static [&'static str],
    },
    /// Only rows of `table` whose `column` holds one of `values` are written.
    RowsExcept {
        table: &'static str,
        column: &'static str,
        values: &'static [&'static str],
    },
    /// Rows whose `column` holds one of `values` are not written, unless `unless_column` holds one of `unless_values`.
    RowsWhereUnless {
        table: &'static str,
        column: &'static str,
        values: &'static [&'static str],
        unless_column: &'static str,
        unless_values: &'static [&'static str],
    },
    /// `columns` of `table` are written as NULL.
    ColumnsNull {
        table: &'static str,
        columns: &'static [&'static str],
    },
    /// `column` of `table` is written in the shorter form `instead` describes; it still identifies the row.
    ColumnShortened {
        table: &'static str,
        column: &'static str,
        instead: &'static str,
    },
    /// The index is not created.
    Index { index: &'static str },
}

/// An omission and the section whose scope it narrows (`load_accounting` covers the `load_fact` family); `None` when none does.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Skip {
    pub omit: Omit,
    pub section: Option<&'static str>,
}

/// The reason code a section carries when a light file omits part of it.
pub(crate) const LIGHT_REASON: &str = "light";

const fn skip(omit: Omit, section: Option<&'static str>) -> Skip {
    Skip { omit, section }
}

/// Inventory kinds whose load decisions no Motif statement reads.
const UNREAD_LOAD_KINDS: &[&str] = &["sense", "msa", "entry", "entryReference"];

/// Indexes no Motif statement's query plan uses.
const UNUSED_INDEXES: &[&str] = &[
    "category_ancestor_ancestor",
    "inflection_class_ancestor_ancestor",
    "allomorph_gate_target",
    "source_object_canonical_guid",
    "object_state_guid",
    "compiled_allomorph_order_source",
    "pattern_node_phoneme",
    "msa_category_category",
];

/// Everything `--light` omits. `conversion_stage` counts what `conversion_item` lists, so it goes with it.
pub(crate) const LIGHT_SKIPS: &[Skip] = &[
    skip(
        Omit::Table {
            table: "source_object",
        },
        Some("source_census"),
    ),
    skip(
        Omit::Table {
            table: "conversion_item",
        },
        Some("conversion_inventory"),
    ),
    skip(
        Omit::Table {
            table: "conversion_stage",
        },
        Some("conversion_inventory"),
    ),
    skip(
        Omit::Table {
            table: "object_state",
        },
        Some("load_accounting"),
    ),
    skip(
        Omit::Table {
            table: "compiled_form_segment",
        },
        Some("effective_grammar"),
    ),
    skip(
        Omit::RowsWhere {
            table: "sense_text",
            column: "kind",
            values: &["definition"],
        },
        Some("entries"),
    ),
    skip(
        Omit::RowsExcept {
            table: "statement_reference",
            column: "target_kind",
            values: &["environment", "naturalClass"],
        },
        Some("load_accounting"),
    ),
    skip(
        Omit::RowsWhere {
            table: "load_fact",
            column: "subject_kind",
            values: UNREAD_LOAD_KINDS,
        },
        Some("load_accounting"),
    ),
    skip(
        Omit::RowsWhereUnless {
            table: "load_fact",
            column: "pipeline_stage",
            values: &["import", "snapshot"],
            unless_column: "subject_kind",
            unless_values: &["affixProcess"],
        },
        Some("load_accounting"),
    ),
    skip(
        Omit::ColumnsNull {
            table: "load_fact",
            columns: &["effective_value_json", "issue_key"],
        },
        Some("load_accounting"),
    ),
    skip(
        Omit::ColumnShortened {
            table: "load_fact",
            column: "subject_key",
            instead: "the subject GUID for a plain object identity; the full key otherwise",
        },
        Some("load_accounting"),
    ),
    skip(
        Omit::Index {
            index: UNUSED_INDEXES[0],
        },
        None,
    ),
    skip(
        Omit::Index {
            index: UNUSED_INDEXES[1],
        },
        None,
    ),
    skip(
        Omit::Index {
            index: UNUSED_INDEXES[2],
        },
        None,
    ),
    skip(
        Omit::Index {
            index: UNUSED_INDEXES[3],
        },
        None,
    ),
    skip(
        Omit::Index {
            index: UNUSED_INDEXES[4],
        },
        None,
    ),
    skip(
        Omit::Index {
            index: UNUSED_INDEXES[5],
        },
        None,
    ),
    skip(
        Omit::Index {
            index: UNUSED_INDEXES[6],
        },
        None,
    ),
    skip(
        Omit::Index {
            index: UNUSED_INDEXES[7],
        },
        None,
    ),
];

/// Whether this build writes the full file or the light one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Profile {
    Full,
    Light,
}

impl Profile {
    pub(crate) fn new(light: bool) -> Self {
        if light {
            Self::Light
        } else {
            Self::Full
        }
    }

    pub(crate) fn is_light(self) -> bool {
        self == Self::Light
    }

    fn skips(self) -> &'static [Skip] {
        match self {
            Self::Full => &[],
            Self::Light => LIGHT_SKIPS,
        }
    }

    /// True when no row of `table` is written.
    pub(crate) fn omits_table(self, table: &str) -> bool {
        self.skips()
            .iter()
            .any(|skip| matches!(skip.omit, Omit::Table { table: omitted } if omitted == table))
    }

    /// True when a row of `table` with these column values is written; a column not listed matches no rule.
    pub(crate) fn keeps_row(self, table: &str, row: &[(&str, &str)]) -> bool {
        let value_of = |column: &str| {
            row.iter()
                .find(|(name, _)| *name == column)
                .map(|(_, value)| *value)
        };
        let holds = |column: &str, values: &[&str]| {
            value_of(column).is_some_and(|value| values.contains(&value))
        };
        self.skips().iter().all(|skip| match skip.omit {
            Omit::Table { table: omitted } => omitted != table,
            Omit::RowsWhere {
                table: on_table,
                column,
                values,
            } => on_table != table || !holds(column, values),
            Omit::RowsExcept {
                table: on_table,
                column,
                values,
            } => on_table != table || value_of(column).is_none() || holds(column, values),
            Omit::RowsWhereUnless {
                table: on_table,
                column,
                values,
                unless_column,
                unless_values,
            } => on_table != table || !holds(column, values) || holds(unless_column, unless_values),
            Omit::ColumnsNull { .. } | Omit::ColumnShortened { .. } | Omit::Index { .. } => true,
        })
    }

    /// True when `column` of `table` is written as NULL.
    pub(crate) fn nulls_column(self, table: &str, column: &str) -> bool {
        self.skips().iter().any(|skip| {
            matches!(skip.omit, Omit::ColumnsNull { table: on_table, columns }
                if on_table == table && columns.contains(&column))
        })
    }

    /// True when `column` of `table` is written in a shorter form.
    pub(crate) fn shortens_column(self, table: &str, column: &str) -> bool {
        self.skips().iter().any(|skip| {
            matches!(skip.omit, Omit::ColumnShortened { table: on_table, column: on_column, .. }
                if on_table == table && on_column == column)
        })
    }

    /// The indexes this profile does not create.
    pub(crate) fn dropped_indexes(self) -> impl Iterator<Item = &'static str> {
        self.skips().iter().filter_map(|skip| match skip.omit {
            Omit::Index { index } => Some(index),
            _ => None,
        })
    }

    /// The omissions as JSON for `artifact_meta.light_omissions`; `None` for a full file.
    pub(crate) fn omissions_json(self) -> Result<Option<String>, crate::FactsError> {
        if !self.is_light() {
            return Ok(None);
        }
        let omits: Vec<&Omit> = self.skips().iter().map(|skip| &skip.omit).collect();
        serde_json::to_string(&omits)
            .map(Some)
            .map_err(|error| crate::FactsError::Serialization(error.to_string()))
    }

    /// Narrows each section a skip names: a complete section becomes partial with reason `light`.
    pub(crate) fn declare(self, sections: &mut [SectionStatus]) {
        for section in sections {
            let omitted: Vec<String> = self
                .skips()
                .iter()
                .filter(|skip| skip.section == Some(section.section.as_str()))
                .map(|skip| skip.omit.describe())
                .collect();
            if omitted.is_empty() {
                continue;
            }
            if section.status == "complete" {
                section.status = "partial".into();
                section.reason_code = Some(LIGHT_REASON.into());
            }
            section.source_scope = format!(
                "{} Light file, omitted: {}.",
                section.source_scope,
                omitted.join("; ")
            );
        }
    }
}

impl Omit {
    fn describe(&self) -> String {
        match self {
            Self::Table { table } => (*table).to_string(),
            Self::RowsWhere {
                table,
                column,
                values,
            } => format!("{table} rows with {column} in ({})", values.join(", ")),
            Self::RowsExcept {
                table,
                column,
                values,
            } => format!("{table} rows with {column} not in ({})", values.join(", ")),
            Self::RowsWhereUnless {
                table,
                column,
                values,
                unless_column,
                unless_values,
            } => format!(
                "{table} rows with {column} in ({}) unless {unless_column} in ({})",
                values.join(", "),
                unless_values.join(", ")
            ),
            Self::ColumnsNull { table, columns } => {
                format!("{table} columns {} (written as NULL)", columns.join(", "))
            }
            Self::ColumnShortened {
                table,
                column,
                instead,
            } => format!("{table}.{column} (holds {instead})"),
            Self::Index { index } => format!("index {index}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_profile_omits_nothing() {
        assert!(!Profile::Full.omits_table("conversion_item"));
        assert!(Profile::Full.keeps_row("sense_text", &[("kind", "definition")]));
        assert!(Profile::Full.keeps_row("load_fact", &[("subject_kind", "sense")]));
        assert!(!Profile::Full.nulls_column("load_fact", "issue_key"));
        assert!(!Profile::Full.shortens_column("load_fact", "subject_key"));
        assert_eq!(Profile::Full.dropped_indexes().count(), 0);
        assert_eq!(Profile::Full.omissions_json().unwrap(), None);
    }

    #[test]
    fn the_light_profile_applies_each_kind_of_omission() {
        let light = Profile::Light;
        assert!(light.omits_table("object_state"));
        assert!(!light.omits_table("load_fact"));
        assert!(light.keeps_row("sense_text", &[("kind", "gloss")]));
        assert!(!light.keeps_row("sense_text", &[("kind", "definition")]));
        assert!(light.keeps_row("statement_reference", &[("target_kind", "environment")]));
        assert!(!light.keeps_row("statement_reference", &[("target_kind", "phoneme")]));
        assert!(light.keeps_row("allomorph", &[("guid", "anything")]));
        assert!(light.nulls_column("load_fact", "issue_key"));
        assert!(!light.nulls_column("load_fact", "reason_code"));
        assert!(light.shortens_column("load_fact", "subject_key"));
        assert!(light
            .dropped_indexes()
            .any(|index| index == "object_state_guid"));
    }

    fn load_row(kind: &str, stage: &str) -> [(&'static str, String); 2] {
        [
            ("subject_kind", kind.to_string()),
            ("pipeline_stage", stage.to_string()),
        ]
    }

    fn keeps_load(kind: &str, stage: &str) -> bool {
        let row = load_row(kind, stage);
        Profile::Light.keeps_row("load_fact", &[(row[0].0, &row[0].1), (row[1].0, &row[1].1)])
    }

    #[test]
    fn light_load_facts_keep_the_stages_motif_reads_and_every_affix_process() {
        assert!(keeps_load("environment", "compile"));
        assert!(keeps_load("allomorph", "compact"));
        assert!(!keeps_load("environment", "import"));
        assert!(!keeps_load("naturalClass", "snapshot"));
        assert!(keeps_load("affixProcess", "import"));
        assert!(keeps_load("affixProcess", "snapshot"));
        for kind in UNREAD_LOAD_KINDS {
            assert!(!keeps_load(kind, "compile"));
            assert!(!keeps_load(kind, "compact"));
        }
        assert!(!keeps_load("sense", "import"));
    }

    fn section(status: &str) -> SectionStatus {
        SectionStatus {
            section: "source_census".into(),
            status: status.into(),
            source_scope: "census".into(),
            reason_code: (status == "unavailable").then(|| "source_inventory_unknown".into()),
        }
    }

    #[test]
    fn declare_narrows_a_complete_section_and_keeps_an_unavailable_one() {
        let mut sections = [section("complete"), section("unavailable")];
        Profile::Light.declare(&mut sections);
        assert_eq!(sections[0].status, "partial");
        assert_eq!(sections[0].reason_code.as_deref(), Some("light"));
        assert!(sections[0]
            .source_scope
            .ends_with("Light file, omitted: source_object."));
        assert_eq!(sections[1].status, "unavailable");
        assert_eq!(
            sections[1].reason_code.as_deref(),
            Some("source_inventory_unknown")
        );
        assert!(sections[1].source_scope.contains("source_object"));

        let mut full = [section("complete")];
        Profile::Full.declare(&mut full);
        assert_eq!(full[0].status, "complete");
        assert_eq!(full[0].source_scope, "census");
    }

    /// A complete `load_accounting` must not read as complete once the `load_fact` rows are cut.
    #[test]
    fn a_complete_load_accounting_is_narrowed_in_a_light_file() {
        let mut sections = [SectionStatus {
            section: "load_accounting".into(),
            status: "complete".into(),
            source_scope: "accounting".into(),
            reason_code: None,
        }];
        Profile::Light.declare(&mut sections);
        assert_eq!(sections[0].status, "partial");
        assert_eq!(sections[0].reason_code.as_deref(), Some("light"));
        assert!(sections[0].source_scope.contains("load_fact"));
    }

    /// `conversion_stage` counts the items `conversion_item` lists, so it cannot be kept without them.
    #[test]
    fn conversion_stage_is_omitted_whenever_conversion_item_is() {
        for profile in [Profile::Full, Profile::Light] {
            assert_eq!(
                profile.omits_table("conversion_item"),
                profile.omits_table("conversion_stage")
            );
        }
    }

    /// An omission naming a table or section the schema lacks would declare nothing.
    #[test]
    fn every_omission_names_a_real_table_and_section() {
        let schema = include_str!("schema.sql");
        for skip in LIGHT_SKIPS {
            let table = match skip.omit {
                Omit::Index { index } => {
                    assert!(
                        schema.contains(&format!("CREATE INDEX {index} ON ")),
                        "{index} is not an index"
                    );
                    continue;
                }
                Omit::Table { table }
                | Omit::RowsWhere { table, .. }
                | Omit::RowsExcept { table, .. }
                | Omit::RowsWhereUnless { table, .. }
                | Omit::ColumnsNull { table, .. }
                | Omit::ColumnShortened { table, .. } => table,
            };
            assert!(
                schema.contains(&format!("CREATE TABLE {table} (")),
                "{table} is not a table"
            );
            if let Some(section) = skip.section {
                assert!(
                    schema.contains(&format!("'{section}'")),
                    "{section} is not a section"
                );
            }
        }
    }
}
