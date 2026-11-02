use std::collections::{BTreeMap, BTreeSet};

use pg_snapshot::{
    ConversionInventory, ConversionIssue, InventoryDelta, InventoryIdentity, InventoryKey,
    InventoryKind, LoadDecision, LoadDisposition, LoadPipelineStage, LoadReasonCode, Snapshot,
};
use rusqlite::{params, Transaction};
use serde::Serialize;
use serde_json::Value;

use crate::light::Profile;
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
    profile: Profile,
) -> Result<(), FactsError> {
    let provenance = &snapshot.conversion_provenance;
    crate::sql::execute(
        tx,
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
        crate::sql::execute(
            tx,
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
        crate::sql::execute(
            tx,
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
    let write_source_objects = !profile.omits_table("source_object");
    for source_object in grammar_sources.iter().filter(|_| write_source_objects) {
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
        crate::sql::execute(
            tx,
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

    if !profile.omits_table("conversion_item") {
        insert_conversion_items(
            tx,
            provenance,
            compile_inventory,
            load_decisions,
            !profile.omits_table("conversion_stage"),
        )?;
    }

    let mut issue_rows = Vec::new();
    issue_rows.extend(
        provenance
            .import_issues
            .iter()
            .map(|issue| ("import", issue)),
    );
    issue_rows.extend(compile_issues.iter().map(|issue| ("compile", issue)));
    let issue_keys = insert_issues(
        tx,
        issue_rows,
        !profile.nulls_column("load_fact", "issue_key"),
    )?;
    insert_load_facts(
        tx,
        provenance,
        compile_inventory,
        load_decisions,
        &issue_keys,
        profile,
    )?;
    if !profile.omits_table("object_state") {
        insert_object_state(tx)?;
    }
    Ok(())
}

/// Writes every inventory item and, when asked, the per-stage counts of the items it wrote.
fn insert_conversion_items(
    tx: &Transaction<'_>,
    provenance: &pg_snapshot::ConversionProvenance,
    compile_inventory: &InventoryDelta,
    load_decisions: &[LoadDecision],
    with_stage_counts: bool,
) -> Result<(), FactsError> {
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
    if !with_stage_counts {
        return Ok(());
    }
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
            crate::sql::execute(
                tx,
                "INSERT INTO conversion_stage(pipeline_stage, inventory_stage, item_count) VALUES (?1, ?2, ?3)",
                params![pipeline_stage, inventory_stage, count as i64],
            )?;
        }
    }
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
        let inserted = crate::sql::execute(
            tx,
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
    index_keys: bool,
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
        crate::sql::execute(
            tx,
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
        if !index_keys {
            continue;
        }
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

/// A load decision with the subject columns of its `load_fact` row.
struct LoadRow {
    decision: LoadDecision,
    kind: &'static str,
    subject_key: String,
    subject_guid: Option<String>,
}

fn insert_load_facts(
    tx: &Transaction<'_>,
    provenance: &pg_snapshot::ConversionProvenance,
    inventory: &InventoryDelta,
    decisions: &[LoadDecision],
    issue_keys: &IssueKeyIndex,
    profile: Profile,
) -> Result<(), FactsError> {
    let mut kept_cells = [[None::<bool>; 4]; 64];
    let mut keeps = |kind: InventoryKind, stage: LoadPipelineStage| {
        let compute = || {
            profile.keeps_row(
                "load_fact",
                &[
                    ("subject_kind", kind_name(kind)),
                    ("pipeline_stage", stage.as_str()),
                ],
            )
        };
        match kept_cells
            .get_mut(kind as usize)
            .and_then(|row| row.get_mut(stage as usize))
        {
            Some(cell) => *cell.get_or_insert_with(compute),
            None => compute(),
        }
    };
    let mut grammar_source_keys: Option<BTreeSet<InventoryKey>> = None;
    let mut rows = Vec::new();
    for decision in load_records(provenance, inventory, decisions, &mut keeps) {
        if decision.subject.kind == InventoryKind::SourceObject {
            let known = grammar_source_keys.get_or_insert_with(|| {
                provenance
                    .source_objects
                    .iter()
                    .filter(|source_object| source_object.inventory_kind.is_some())
                    .map(|source_object| source_object.key())
                    .collect()
            });
            if !known.contains(&decision.subject) {
                continue;
            }
        }
        let (subject_key, subject_guid) = load_subject(profile, &decision.subject)?;
        rows.push(LoadRow {
            kind: kind_name(decision.subject.kind),
            decision,
            subject_key,
            subject_guid,
        });
    }
    rows.sort_by(|left, right| {
        (
            left.decision.pipeline_stage,
            &left.subject_key,
            &left.decision.context_key,
            left.decision.decision_ordinal,
        )
            .cmp(&(
                right.decision.pipeline_stage,
                &right.subject_key,
                &right.decision.context_key,
                right.decision.decision_ordinal,
            ))
    });
    for row in &rows {
        insert_load_fact(tx, row, issue_keys, profile)?;
    }
    Ok(())
}

/// The subject columns of a load row: the full canonical key, or in a light file the GUID alone for a plain object.
fn load_subject(
    profile: Profile,
    subject: &InventoryKey,
) -> Result<(String, Option<String>), FactsError> {
    if profile.shortens_column("load_fact", "subject_key") {
        if let InventoryIdentity::Object { guid } = &subject.identity {
            let guid = normalize_guid(guid)?;
            return Ok((guid.clone(), Some(guid)));
        }
    }
    let (_, key, guid) = subject_key(subject)?;
    Ok((key, guid))
}

/// Publishes each subject's final load state, stage by the rule documented on `object_state`.
fn insert_object_state(tx: &Transaction<'_>) -> Result<(), FactsError> {
    type Row = (
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
        Option<i64>,
        String,
    );
    let mut statement = tx.prepare(
        "SELECT subject_kind, subject_guid, pipeline_stage, decision_ordinal, subject_key, \
         context_key, disposition, loaded, reason_code FROM load_fact \
         WHERE subject_guid IS NOT NULL ORDER BY subject_kind, subject_guid",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok::<Row, rusqlite::Error>((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let mut subjects: BTreeMap<(String, String), Vec<Row>> = BTreeMap::new();
    for row in rows {
        subjects
            .entry((row.0.clone(), row.1.clone()))
            .or_default()
            .push(row);
    }
    for ((kind, guid), rows) in subjects {
        let final_rank = rows.iter().map(|row| stage_rank(&row.2)).max().unwrap_or(0);
        let at_final: Vec<&Row> = rows
            .iter()
            .filter(|row| stage_rank(&row.2) == final_rank)
            .collect();
        let final_stage = at_final[0].2.clone();
        let first_loaded = at_final[0].7;
        let loaded = first_loaded.filter(|_| at_final.iter().all(|row| row.7 == first_loaded));
        let dispositions: BTreeSet<&str> = at_final.iter().map(|row| row.6.as_str()).collect();
        let dispositions = dispositions.into_iter().collect::<Vec<_>>().join(",");
        let primary = at_final
            .iter()
            .min_by(|left, right| (left.3, &left.4, &left.5).cmp(&(right.3, &right.4, &right.5)))
            .map(|row| row.8.clone())
            .unwrap_or_default();
        crate::sql::execute(
            tx,
            "INSERT INTO object_state(subject_kind, subject_guid, final_stage, loaded, dispositions, primary_reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![kind, guid, final_stage, loaded, dispositions, primary],
        )?;
    }
    Ok(())
}

/// Later stages supersede earlier ones: compact over compile over snapshot over import.
fn stage_rank(stage: &str) -> u8 {
    match stage {
        "import" => 0,
        "snapshot" => 1,
        "compile" => 2,
        _ => 3,
    }
}

pub(crate) fn accounting_is_complete(
    provenance: &pg_snapshot::ConversionProvenance,
    inventory: &InventoryDelta,
    decisions: &[LoadDecision],
) -> bool {
    unrecorded_subjects(provenance, inventory, decisions).is_empty()
        && provenance
            .import_load_decisions
            .iter()
            .chain(decisions)
            .all(|decision| decision.disposition != LoadDisposition::Unknown)
}

/// The imported and compiled subjects no stage recorded a decision for.
fn unrecorded_subjects<'a>(
    provenance: &'a pg_snapshot::ConversionProvenance,
    inventory: &'a InventoryDelta,
    decisions: &'a [LoadDecision],
) -> Vec<(&'a InventoryKey, LoadPipelineStage)> {
    let recorded = |stage: LoadPipelineStage| -> BTreeSet<&InventoryKey> {
        provenance
            .import_load_decisions
            .iter()
            .chain(decisions)
            .filter(|decision| decision.pipeline_stage == stage)
            .map(|decision| &decision.subject)
            .collect()
    };
    let imported = recorded(LoadPipelineStage::Import);
    let compiled = recorded(LoadPipelineStage::Compile);
    let mut unrecorded = Vec::new();
    for subject in inventory_keys(&provenance.graph_to_snapshot).difference(&imported) {
        unrecorded.push((*subject, LoadPipelineStage::Import));
    }
    for subject in inventory_keys(&inventory.inventory).difference(&compiled) {
        unrecorded.push((*subject, LoadPipelineStage::Compile));
    }
    unrecorded
}

/// The decisions a file writes: recorded ones plus an unknown for each unrecorded subject, `keeps` choosing by kind and stage before anything is cloned.
fn load_records(
    provenance: &pg_snapshot::ConversionProvenance,
    inventory: &InventoryDelta,
    decisions: &[LoadDecision],
    keeps: &mut impl FnMut(InventoryKind, LoadPipelineStage) -> bool,
) -> Vec<LoadDecision> {
    let mut records: Vec<LoadDecision> = provenance
        .import_load_decisions
        .iter()
        .chain(decisions)
        .filter(|decision| keeps(decision.subject.kind, decision.pipeline_stage))
        .cloned()
        .collect();
    for (subject, stage) in unrecorded_subjects(provenance, inventory, decisions) {
        if keeps(subject.kind, stage) {
            records.push(unknown_decision(
                subject.clone(),
                stage,
                LoadReasonCode::DecisionUnrecorded,
            ));
        }
    }
    records
}

fn inventory_keys(inventory: &ConversionInventory) -> BTreeSet<&InventoryKey> {
    inventory
        .authored
        .iter()
        .chain(&inventory.considered)
        .chain(&inventory.selected)
        .chain(&inventory.represented)
        .chain(&inventory.rejected)
        .chain(&inventory.synthesized)
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
    row: &LoadRow,
    issue_keys: &IssueKeyIndex,
    profile: Profile,
) -> Result<(), FactsError> {
    let decision = &row.decision;
    let pipeline_stage = decision.pipeline_stage.as_str();
    let issue_key = if profile.nulls_column("load_fact", "issue_key") {
        None
    } else {
        decision
            .issue_code
            .as_ref()
            .and_then(|code| {
                row.subject_guid
                    .as_ref()
                    .map(|guid| (code.clone(), row.kind.to_string(), guid.clone()))
            })
            .and_then(|key| issue_keys.get(&key))
            .filter(|keys| keys.len() == 1)
            .and_then(|keys| keys.first())
    };
    let effective_value = if profile.nulls_column("load_fact", "effective_value_json") {
        None
    } else {
        decision.effective_value_json.as_ref()
    };
    crate::sql::execute(
        tx,
        "INSERT INTO load_fact(subject_kind, subject_key, pipeline_stage, decision_ordinal, subject_guid, context_key, disposition, loaded, reason_code, effective_value_json, issue_key) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            row.kind,
            row.subject_key,
            pipeline_stage,
            decision.decision_ordinal as i64,
            row.subject_guid,
            decision.context_key,
            decision.disposition.as_str(),
            decision.loaded.map(|loaded| if loaded { 1_i64 } else { 0_i64 }),
            decision.reason_code.as_str(),
            effective_value,
            issue_key
        ],
    )?;
    Ok(())
}

/// The serialized `kind` of an inventory kind, the text `subject_kind` holds.
fn kind_name(kind: InventoryKind) -> &'static str {
    match kind {
        InventoryKind::Entry => "entry",
        InventoryKind::Sense => "sense",
        InventoryKind::EntryReference => "entryReference",
        InventoryKind::Msa => "msa",
        InventoryKind::Allomorph => "allomorph",
        InventoryKind::AffixProcess => "affixProcess",
        InventoryKind::Environment => "environment",
        InventoryKind::Phoneme => "phoneme",
        InventoryKind::BoundaryMarker => "boundaryMarker",
        InventoryKind::NaturalClass => "naturalClass",
        InventoryKind::FeatureDefinition => "featureDefinition",
        InventoryKind::FeatureValue => "featureValue",
        InventoryKind::FeatureStructure => "featureStructure",
        InventoryKind::PhonologicalContext => "phonologicalContext",
        InventoryKind::FeatureConstraint => "featureConstraint",
        InventoryKind::MorphemeCoOccurrence => "morphemeCoOccurrence",
        InventoryKind::AllomorphCoOccurrence => "allomorphCoOccurrence",
        InventoryKind::PhonologicalRule => "phonologicalRule",
        InventoryKind::CompoundRule => "compoundRule",
        InventoryKind::PartOfSpeech => "partOfSpeech",
        InventoryKind::InflectionClass => "inflectionClass",
        InventoryKind::StemName => "stemName",
        InventoryKind::RuleFeature => "ruleFeature",
        InventoryKind::ParserSetting => "parserSetting",
        InventoryKind::StrataConfiguration => "strataConfiguration",
        InventoryKind::Template => "template",
        InventoryKind::TemplateSlot => "templateSlot",
        InventoryKind::SourceObject => "sourceObject",
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `kind_name` stands in for serializing the kind, so it must match serde for every kind.
    #[test]
    fn kind_name_matches_the_serialized_kind() {
        use InventoryKind::*;
        for kind in [
            Entry,
            Sense,
            EntryReference,
            Msa,
            Allomorph,
            AffixProcess,
            Environment,
            Phoneme,
            BoundaryMarker,
            NaturalClass,
            FeatureDefinition,
            FeatureValue,
            FeatureStructure,
            PhonologicalContext,
            FeatureConstraint,
            MorphemeCoOccurrence,
            AllomorphCoOccurrence,
            PhonologicalRule,
            CompoundRule,
            PartOfSpeech,
            InflectionClass,
            StemName,
            RuleFeature,
            ParserSetting,
            StrataConfiguration,
            Template,
            TemplateSlot,
            SourceObject,
        ] {
            let serialized = serde_json::to_value(kind).unwrap();
            assert_eq!(serialized.as_str(), Some(kind_name(kind)));
            assert!((kind as usize) < 64);
        }
    }
}
