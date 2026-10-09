use std::collections::{BTreeMap, BTreeSet};

use pg_snapshot::{
    ConversionInventory, ConversionIssue, InventoryDelta, InventoryIdentity, InventoryKey,
    InventoryKind, LoadDecision, LoadDisposition, LoadPipelineStage, LoadReasonCode, Snapshot,
};
use rusqlite::{params, Transaction};
use serde::Serialize;
use serde_json::Value;

use crate::FactsError;

type IssueMatchKey = (String, String, String);
type IssueKeyIndex = BTreeMap<IssueMatchKey, Vec<String>>;
type InventoryCounts = BTreeMap<(&'static str, &'static str), usize>;

#[derive(Clone, Copy)]
pub(crate) enum InventoryPipeline {
    Import,
    Compile,
}

impl InventoryPipeline {
    fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Compile => "compile",
        }
    }
}

pub(crate) fn insert_inventory_and_issues(
    tx: &Transaction<'_>,
    snapshot: &Snapshot,
    compile_inventory: &InventoryDelta,
    compile_issues: &[ConversionIssue],
    load_decisions: &[LoadDecision],
) -> Result<(), FactsError> {
    let provenance = &snapshot.conversion_provenance;
    tx.execute(
        "INSERT INTO source_census(singleton, total_occurrences, ordered_header_sha256) VALUES (1, ?1, ?2)",
        params![
            provenance.source_census.total_occurrences as i64,
            provenance.source_census.ordered_header_sha256
        ],
    )?;
    for (class_name, count) in &provenance.source_census.class_occurrences {
        let unhandled = provenance
            .source_census
            .unhandled_class_occurrences
            .get(class_name)
            .copied()
            .unwrap_or_default();
        tx.execute(
            "INSERT INTO source_class_count(class_name, occurrences, unhandled_occurrences) VALUES (?1, ?2, ?3)",
            params![class_name, *count as i64, unhandled as i64],
        )?;
    }
    for (class_name, count) in &provenance.source_census.unhandled_class_occurrences {
        if provenance
            .source_census
            .class_occurrences
            .contains_key(class_name)
        {
            continue;
        }
        tx.execute(
            "INSERT INTO source_class_count(class_name, occurrences, unhandled_occurrences) VALUES (?1, 0, ?2)",
            params![class_name, *count as i64],
        )?;
    }
    // Non-grammar occurrences stay in source_class_count; only grammar objects get a row here.
    let grammar_sources: Vec<_> = provenance
        .source_objects
        .iter()
        .filter(|source_object| source_object.inventory_kind.is_some())
        .collect();
    for source_object in &grammar_sources {
        let source_key = subject_key(&source_object.key())?.1;
        let inventory_kind = source_object
            .inventory_kind
            .map(|kind| {
                let value = serde_json::to_value(kind)
                    .map_err(|error| FactsError::Serialization(error.to_string()))?;
                value.as_str().map(str::to_owned).ok_or_else(|| {
                    FactsError::Serialization("inventory kind did not serialize as text".into())
                })
            })
            .transpose()?;
        tx.execute(
            "INSERT INTO source_object(source_key, source_ordinal, class_name, raw_guid, canonical_guid, inventory_kind, handled, retained, duplicate) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                source_key,
                source_object.ordinal as i64,
                source_object.class_name,
                source_object.raw_guid,
                source_object.canonical_guid,
                inventory_kind,
                if source_object.handled { 1_i64 } else { 0_i64 },
                if source_object.retained { 1_i64 } else { 0_i64 },
                if source_object.duplicate { 1_i64 } else { 0_i64 }
            ],
        )?;
    }

    let mut item_counts = BTreeMap::new();
    write_inventory(
        tx,
        InventoryPipeline::Import,
        &provenance.graph_to_snapshot,
        &mut item_counts,
    )?;
    write_inventory(
        tx,
        InventoryPipeline::Compile,
        &compile_inventory.inventory,
        &mut item_counts,
    )?;
    let compacted: Vec<_> = load_decisions
        .iter()
        .filter(|decision| decision.pipeline_stage == LoadPipelineStage::Compact)
        .map(|decision| decision.subject.clone())
        .collect();
    write_inventory_set(
        tx,
        "compact",
        "rejected",
        compacted.iter(),
        &mut item_counts,
    )?;
    for pipeline_stage in ["import", "snapshot", "compile", "compact"] {
        for inventory_stage in [
            "authored",
            "considered",
            "selected",
            "represented",
            "rejected",
            "synthesized",
        ] {
            let count = item_counts
                .get(&(pipeline_stage, inventory_stage))
                .copied()
                .unwrap_or(0);
            tx.execute(
                "INSERT INTO conversion_stage(pipeline_stage, inventory_stage, item_count) VALUES (?1, ?2, ?3)",
                params![pipeline_stage, inventory_stage, count as i64],
            )?;
        }
    }

    let mut issue_rows = Vec::new();
    issue_rows.extend(
        provenance
            .import_issues
            .iter()
            .map(|issue| ("import", issue)),
    );
    issue_rows.extend(compile_issues.iter().map(|issue| ("compile", issue)));
    let issue_keys = insert_issues(tx, issue_rows)?;
    let grammar_source_keys: BTreeSet<_> = grammar_sources
        .iter()
        .map(|source_object| source_object.key())
        .collect();
    insert_load_facts(
        tx,
        provenance,
        compile_inventory,
        load_decisions,
        &issue_keys,
        &grammar_source_keys,
    )?;
    Ok(())
}

fn write_inventory(
    tx: &Transaction<'_>,
    pipeline: InventoryPipeline,
    inventory: &ConversionInventory,
    counts: &mut InventoryCounts,
) -> Result<(), FactsError> {
    let pipeline = pipeline.as_str();
    for (stage, set) in [
        ("authored", &inventory.authored),
        ("considered", &inventory.considered),
        ("selected", &inventory.selected),
        ("represented", &inventory.represented),
        ("rejected", &inventory.rejected),
        ("synthesized", &inventory.synthesized),
    ] {
        write_inventory_set(tx, pipeline, stage, set.iter(), counts)?;
    }
    Ok(())
}

fn write_inventory_set<'a>(
    tx: &Transaction<'_>,
    pipeline: &'static str,
    stage: &'static str,
    items: impl Iterator<Item = &'a InventoryKey>,
    counts: &mut InventoryCounts,
) -> Result<(), FactsError> {
    for key in items {
        let (kind, subject_key, subject_guid) = subject_key(key)?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO conversion_item(pipeline_stage, inventory_stage, subject_kind, subject_key, subject_guid) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![pipeline, stage, kind, subject_key, subject_guid],
        )?;
        if inserted != 0 {
            *counts.entry((pipeline, stage)).or_default() += 1;
        }
    }
    Ok(())
}

fn insert_issues<'a>(
    tx: &Transaction<'_>,
    issues: impl IntoIterator<Item = (&'static str, &'a ConversionIssue)>,
) -> Result<IssueKeyIndex, FactsError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct IssueIdentity<'a> {
        stage: &'static str,
        code: &'a str,
        issue_class: &'static str,
        fatal: bool,
        source_kind: Option<String>,
        source_guid: Option<String>,
        message: &'a str,
    }

    let mut rows = issues
        .into_iter()
        .map(|(stage, issue)| {
            let identity = IssueIdentity {
                stage,
                code: issue.code.wire(),
                issue_class: issue_class(issue.class),
                fatal: issue.fatal,
                source_kind: issue
                    .source
                    .as_ref()
                    .map(|source| {
                        let value = serde_json::to_value(source.kind)
                            .map_err(|error| FactsError::Serialization(error.to_string()))?;
                        value.as_str().map(str::to_owned).ok_or_else(|| {
                            FactsError::Serialization(
                                "inventory kind did not serialize as text".into(),
                            )
                        })
                    })
                    .transpose()?,
                source_guid: issue
                    .source
                    .as_ref()
                    .and_then(|source| pg_snapshot::canonical_guid(&source.id)),
                message: &issue.message,
            };
            let canonical = pg_assess::canonicalize(
                &serde_json::to_value(&identity)
                    .map_err(|error| FactsError::Serialization(error.to_string()))?,
            )
            .map_err(|error| FactsError::Serialization(error.to_string()))?;
            Ok((canonical, identity))
        })
        .collect::<Result<Vec<_>, FactsError>>()?;
    rows.sort_by(|left, right| left.0.cmp(&right.0));
    let mut duplicate_counts = BTreeMap::<String, u32>::new();
    let mut issue_keys = BTreeMap::new();
    for (canonical, identity) in rows {
        let occurrence = duplicate_counts.entry(canonical.clone()).or_default();
        let key_input = format!("{canonical}#{occurrence}");
        *occurrence += 1;
        let issue_key = pg_assess::sha256_bytes(key_input.as_bytes());
        tx.execute(
            "INSERT INTO conversion_issue(issue_key, pipeline_stage, code, issue_class, fatal, source_kind, source_guid, message) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                issue_key,
                identity.stage,
                identity.code,
                identity.issue_class,
                if identity.fatal { 1_i64 } else { 0_i64 },
                identity.source_kind,
                identity.source_guid,
                identity.message
            ],
        )?;
        if let (Some(source_kind), Some(guid)) = (identity.source_kind, identity.source_guid) {
            issue_keys
                .entry((identity.code.to_string(), source_kind, guid))
                .or_insert_with(Vec::new)
                .push(issue_key);
        }
    }
    Ok(issue_keys)
}

fn issue_class(class: pg_snapshot::IssueClass) -> &'static str {
    match class {
        pg_snapshot::IssueClass::MalformedSource => "malformedSource",
        pg_snapshot::IssueClass::InvalidSource => "invalidSource",
        pg_snapshot::IssueClass::AmbiguousSource => "ambiguousSource",
        pg_snapshot::IssueClass::UnrepresentableForHc => "unrepresentableForHc",
        pg_snapshot::IssueClass::SubstrateUnresolvable => "substrateUnresolvable",
        pg_snapshot::IssueClass::MigrationDifference => "migrationDifference",
        pg_snapshot::IssueClass::UnreachableInGrammar => "unreachableInGrammar",
    }
}

fn insert_load_facts(
    tx: &Transaction<'_>,
    provenance: &pg_snapshot::ConversionProvenance,
    inventory: &InventoryDelta,
    decisions: &[LoadDecision],
    issue_keys: &IssueKeyIndex,
    grammar_source_keys: &BTreeSet<InventoryKey>,
) -> Result<(), FactsError> {
    let records = load_records(provenance, inventory, decisions);
    let mut records = records
        .into_iter()
        .filter(|decision| {
            decision.subject.kind != InventoryKind::SourceObject
                || grammar_source_keys.contains(&decision.subject)
        })
        .map(|decision| {
            let subject_key = subject_key(&decision.subject)?.1;
            Ok((decision, subject_key))
        })
        .collect::<Result<Vec<_>, FactsError>>()?;
    records.sort_by(|(left, left_key), (right, right_key)| {
        (
            left.pipeline_stage,
            left_key,
            &left.context_key,
            left.decision_ordinal,
        )
            .cmp(&(
                right.pipeline_stage,
                right_key,
                &right.context_key,
                right.decision_ordinal,
            ))
    });
    for (decision, _) in records {
        insert_load_fact(tx, &decision, issue_keys)?;
    }
    Ok(())
}

pub(crate) fn accounting_is_complete(
    provenance: &pg_snapshot::ConversionProvenance,
    inventory: &InventoryDelta,
    decisions: &[LoadDecision],
) -> bool {
    load_records(provenance, inventory, decisions)
        .iter()
        .all(|decision| decision.disposition != LoadDisposition::Unknown)
}

fn load_records(
    provenance: &pg_snapshot::ConversionProvenance,
    inventory: &InventoryDelta,
    decisions: &[LoadDecision],
) -> Vec<LoadDecision> {
    let mut records = provenance.import_load_decisions.clone();
    records.extend(decisions.iter().cloned());
    let imported_keys = inventory_keys(&provenance.graph_to_snapshot);
    let import_recorded: BTreeSet<_> = records
        .iter()
        .filter(|decision| decision.pipeline_stage == LoadPipelineStage::Import)
        .map(|decision| decision.subject.clone())
        .collect();
    for subject in imported_keys.difference(&import_recorded) {
        records.push(unknown_decision(
            subject.clone(),
            LoadPipelineStage::Import,
            LoadReasonCode::DecisionUnrecorded,
        ));
    }
    let recorded: BTreeSet<_> = records
        .iter()
        .filter(|decision| decision.pipeline_stage == LoadPipelineStage::Compile)
        .map(|decision| decision.subject.clone())
        .collect();
    let compiler_keys = inventory_keys(&inventory.inventory);
    for subject in compiler_keys.difference(&recorded) {
        records.push(unknown_decision(
            subject.clone(),
            LoadPipelineStage::Compile,
            LoadReasonCode::DecisionUnrecorded,
        ));
    }
    records
}

fn inventory_keys(inventory: &ConversionInventory) -> BTreeSet<InventoryKey> {
    inventory
        .authored
        .iter()
        .chain(&inventory.considered)
        .chain(&inventory.selected)
        .chain(&inventory.represented)
        .chain(&inventory.rejected)
        .chain(&inventory.synthesized)
        .cloned()
        .collect()
}

fn unknown_decision(
    subject: InventoryKey,
    pipeline_stage: LoadPipelineStage,
    reason_code: LoadReasonCode,
) -> LoadDecision {
    LoadDecision {
        subject,
        pipeline_stage,
        decision_ordinal: 0,
        context_key: String::new(),
        disposition: LoadDisposition::Unknown,
        loaded: None,
        reason_code,
        effective_value_json: None,
        issue_code: None,
    }
}

fn insert_load_fact(
    tx: &Transaction<'_>,
    decision: &LoadDecision,
    issue_keys: &IssueKeyIndex,
) -> Result<(), FactsError> {
    let (kind, subject_key, subject_guid) = subject_key(&decision.subject)?;
    let pipeline_stage = decision.pipeline_stage.as_str();
    let issue_key = decision
        .issue_code
        .as_ref()
        .and_then(|code| {
            subject_guid
                .as_ref()
                .map(|guid| (code.clone(), kind.clone(), guid.clone()))
        })
        .and_then(|key| issue_keys.get(&key))
        .filter(|keys| keys.len() == 1)
        .and_then(|keys| keys.first());
    tx.execute(
        "INSERT INTO load_fact(subject_kind, subject_key, pipeline_stage, decision_ordinal, subject_guid, context_key, disposition, loaded, reason_code, effective_value_json, issue_key) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            kind,
            subject_key,
            pipeline_stage,
            decision.decision_ordinal as i64,
            subject_guid,
            decision.context_key,
            decision.disposition.as_str(),
            decision.loaded.map(|loaded| if loaded { 1_i64 } else { 0_i64 }),
            decision.reason_code.as_str(),
            decision.effective_value_json,
            issue_key
        ],
    )?;
    Ok(())
}

pub(crate) fn subject_key(
    key: &InventoryKey,
) -> Result<(String, String, Option<String>), FactsError> {
    let normalized = normalize_key(key)?;
    let value = serde_json::to_value(&normalized)
        .map_err(|error| FactsError::Serialization(error.to_string()))?;
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| FactsError::Serialization("inventory key has no kind".into()))?
        .to_string();
    let subject_guid = match &normalized.identity {
        InventoryIdentity::Object { guid } => Some(guid.clone()),
        InventoryIdentity::SourceOccurrence { canonical_guid, .. } => canonical_guid.clone(),
        _ => None,
    };
    let subject_key = pg_assess::canonicalize(&value)
        .map_err(|error| FactsError::Serialization(error.to_string()))?;
    Ok((kind, subject_key, subject_guid))
}

fn normalize_key(key: &InventoryKey) -> Result<InventoryKey, FactsError> {
    let identity = match &key.identity {
        InventoryIdentity::Object { guid } => InventoryIdentity::Object {
            guid: normalize_guid(guid)?,
        },
        InventoryIdentity::Attachment {
            owner_guid,
            target_guid,
            role,
        } => InventoryIdentity::Attachment {
            owner_guid: normalize_guid(owner_guid)?,
            target_guid: normalize_guid(target_guid)?,
            role: role.clone(),
        },
        InventoryIdentity::Expansion {
            owner_guid,
            member_guids,
            role,
        } => InventoryIdentity::Expansion {
            owner_guid: normalize_guid(owner_guid)?,
            member_guids: member_guids
                .iter()
                .map(|guid| normalize_guid(guid))
                .collect::<Result<Vec<_>, _>>()?,
            role: role.clone(),
        },
        InventoryIdentity::Synthetic { key } => InventoryIdentity::Synthetic { key: key.clone() },
        InventoryIdentity::Setting { name } => InventoryIdentity::Setting { name: name.clone() },
        InventoryIdentity::SourceOccurrence {
            ordinal,
            class_name,
            raw_guid,
            canonical_guid,
        } => InventoryIdentity::SourceOccurrence {
            ordinal: *ordinal,
            class_name: class_name.clone(),
            raw_guid: raw_guid.clone(),
            canonical_guid: canonical_guid.as_deref().map(normalize_guid).transpose()?,
        },
    };
    Ok(InventoryKey {
        kind: key.kind,
        identity,
    })
}

fn normalize_guid(value: &str) -> Result<String, FactsError> {
    pg_snapshot::canonical_guid(value)
        .ok_or_else(|| FactsError::InvalidSnapshot(format!("invalid inventory GUID {value:?}")))
}
