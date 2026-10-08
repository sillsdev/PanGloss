use pg_grammar::model::{Grammar, MorphRuleDef};
use pg_snapshot::Snapshot;
use rusqlite::{params, Transaction};
use serde::Serialize;

use crate::FactsError;

pub(crate) fn insert_parser_config(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    grammar: Option<&Grammar>,
) -> Result<(), FactsError> {
    let params = &snapshot.morphology.parser_parameters;
    insert(
        tx,
        "snapshotParserParameters",
        Some(params),
        Some(params),
        "not_retained",
    )?;
    insert(
        tx,
        "authoredStrata",
        params.strata.as_ref(),
        params.strata.as_ref(),
        if params.strata.is_some() {
            "present"
        } else {
            "absent"
        },
    )?;
    let effective_strata = grammar.map(|grammar| {
        grammar
            .strata
            .iter()
            .map(|stratum| stratum.name.as_deref().unwrap_or(""))
            .collect::<Vec<_>>()
    });
    insert(
        tx,
        "compiledStrata",
        params.strata.as_ref(),
        effective_strata.as_ref(),
        if params.strata.is_some() {
            "present"
        } else {
            "absent"
        },
    )?;
    let effective_compound_limits = grammar.map(|grammar| {
        grammar
            .mrules
            .iter()
            .filter_map(|rule| match rule {
                MorphRuleDef::Compounding(rule) => Some((
                    rule.source_guid.as_deref().unwrap_or(&rule.xml_id),
                    rule.max_apps,
                )),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    insert(
        tx,
        "compiledCompoundRuleMaxApplications",
        Some(&params.compound_rule_max_applications),
        effective_compound_limits.as_ref(),
        "not_retained",
    )?;
    Ok(())
}

fn insert<S: Serialize, E: Serialize>(
    tx: &Transaction<'_>,
    key: &str,
    source: Option<&S>,
    effective: Option<&E>,
    source_presence: &str,
) -> Result<(), FactsError> {
    let source_json = source
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| FactsError::Serialization(error.to_string()))?;
    let effective_json = effective
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| FactsError::Serialization(error.to_string()))?
        .unwrap_or_else(|| "null".into());
    tx.execute(
        "INSERT INTO parser_config(setting_key, source_value_json, effective_value_json, source_presence) VALUES (?1, ?2, ?3, ?4)",
        params![key, source_json, effective_json, source_presence],
    )?;
    Ok(())
}
