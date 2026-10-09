use std::collections::BTreeSet;

use pg_grammar::chardef::{CharDef, CharDefId, CharDefKind, CharDefTable};
use pg_grammar::compile::{CompiledAllomorphOrder, CompiledMapping, CompiledOutput};
use pg_grammar::model::{
    AffixAllomorphDef, AllomorphOwner, Grammar, OutputAction, RootAllomorphDef, SegmentedText,
};
use pg_shape::NO_CHAR_DEF;
use pg_snapshot::Snapshot;
use rusqlite::{params, Transaction};

use crate::phonology::{canonical_key, segment_identity, SourceGuids};
use crate::FactsError;

pub(crate) fn insert(
    tx: &Transaction<'_>,
    outputs: &[CompiledOutput],
    mappings: &[CompiledMapping],
    allomorph_order: &[CompiledAllomorphOrder],
    form_segments: &[(u32, Vec<FormSegment>)],
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
    insert_form_segments(tx, form_segments)?;
    crate::variants::insert_compiled_order(tx, allomorph_order)
}

/// One `compiled_form_segment` row before it is numbered within its output.
pub(crate) struct FormSegment {
    pub(crate) segment_kind: &'static str,
    pub(crate) phoneme_guid: Option<String>,
    pub(crate) boundary_guid: Option<String>,
    pub(crate) natural_class_guid: Option<String>,
    token_text: Option<String>,
}

impl FormSegment {
    fn variable() -> Self {
        Self {
            segment_kind: "variable",
            phoneme_guid: None,
            boundary_guid: None,
            natural_class_guid: None,
            token_text: None,
        }
    }
}

/// Writes the segments of each compiled allomorph output.
fn insert_form_segments(
    tx: &Transaction<'_>,
    form_segments: &[(u32, Vec<FormSegment>)],
) -> Result<(), FactsError> {
    for (output_id, segments) in form_segments {
        for (ordinal, segment) in segments.iter().enumerate() {
            tx.execute(
                "INSERT INTO compiled_form_segment(output_id, ordinal, segment_kind, phoneme_guid, boundary_guid, natural_class_guid, token_text) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    i64::from(*output_id),
                    ordinal as i64,
                    segment.segment_kind,
                    segment.phoneme_guid,
                    segment.boundary_guid,
                    segment.natural_class_guid,
                    segment.token_text,
                ],
            )?;
        }
    }
    Ok(())
}

/// The segments of each compiled allomorph output, read once for both the form table and the reference index.
pub(crate) fn output_segments(
    snapshot: &Snapshot,
    grammar: &Grammar,
    allomorph_output_ids: &[Option<u32>],
) -> Result<Vec<(u32, Vec<FormSegment>)>, FactsError> {
    let sources = SourceGuids::new(snapshot);
    // A process copy is part of its output; a concatenative copy is the stem it attaches to.
    let process_guids: BTreeSet<String> = snapshot
        .lexicon
        .entries
        .iter()
        .flat_map(|entry| &entry.allomorphs)
        .filter(|allomorph| allomorph.process.is_some())
        .map(|allomorph| canonical_key(&allomorph.guid))
        .collect();
    let mut outputs = Vec::new();
    for (index, owner) in grammar.allomorph_owners.iter().enumerate() {
        let Some(output_id) = allomorph_output_ids.get(index).copied().flatten() else {
            continue;
        };
        let segments = match owner {
            AllomorphOwner::Root(entry, position) => {
                let entry = &grammar.entries[entry.0 as usize];
                let allomorph = &entry.allomorphs[usize::from(*position)];
                let stratum = grammar.morphemes[entry.morpheme.0 as usize].stratum;
                let table = table_of(grammar, grammar.strata[stratum.0 as usize].table.0)?;
                root_segments(allomorph, table, &sources)?
            }
            AllomorphOwner::Affix(rule, position) => {
                let allomorph = affix_allomorph(grammar, *rule, *position)?;
                let process = grammar.allomorph_sources.get(index).is_some_and(|source| {
                    source
                        .form_guids
                        .iter()
                        .flatten()
                        .any(|guid| process_guids.contains(&canonical_key(guid)))
                });
                affix_segments(grammar, allomorph, process, &sources)?
            }
        };
        let Some(segments) = segments else {
            continue;
        };
        outputs.push((output_id, segments));
    }
    Ok(outputs)
}

fn table_of(grammar: &Grammar, table: u16) -> Result<&CharDefTable, FactsError> {
    grammar
        .char_tables
        .get(usize::from(table))
        .ok_or_else(|| FactsError::Serialization(format!("character table {table} is not defined")))
}

fn affix_allomorph(
    grammar: &Grammar,
    rule: pg_grammar::model::MRuleId,
    position: u16,
) -> Result<&AffixAllomorphDef, FactsError> {
    grammar.mrules[rule.0 as usize]
        .affix_allomorphs()
        .and_then(|allomorphs| allomorphs.get(usize::from(position)))
        .ok_or_else(|| {
            FactsError::Serialization(format!(
                "affix allomorph {position} of rule {} is not defined",
                rule.0
            ))
        })
}

fn root_segments(
    allomorph: &RootAllomorphDef,
    table: &CharDefTable,
    sources: &SourceGuids,
) -> Result<Option<Vec<FormSegment>>, FactsError> {
    if allomorph.is_pattern {
        return Ok(None);
    }
    shape_segments(table, &allomorph.shape, sources)
}

/// A concatenative affix publishes its inserted shape, not the stem copy it attaches to.
fn affix_segments(
    grammar: &Grammar,
    allomorph: &AffixAllomorphDef,
    process: bool,
    sources: &SourceGuids,
) -> Result<Option<Vec<FormSegment>>, FactsError> {
    let mut segments = Vec::new();
    for action in &allomorph.rhs {
        match action {
            OutputAction::Copy(_) | OutputAction::Modify(..) if !process => {}
            OutputAction::Copy(_) | OutputAction::Modify(..) => {
                segments.push(FormSegment::variable());
            }
            OutputAction::InsertContext(context) => {
                let class = grammar
                    .natural_classes
                    .get(context.nat_class.0 as usize)
                    .ok_or_else(|| {
                        FactsError::Serialization(format!(
                            "natural class index {} is not defined",
                            context.nat_class.0
                        ))
                    })?;
                segments.push(FormSegment {
                    segment_kind: "natural_class",
                    phoneme_guid: None,
                    boundary_guid: None,
                    natural_class_guid: Some(canonical_key(&class.xml_id)),
                    token_text: None,
                });
            }
            OutputAction::InsertSegments { table, shape } => {
                let table = table_of(grammar, table.0)?;
                match shape_segments(table, shape, sources)? {
                    Some(inserted) => segments.extend(inserted),
                    None => return Ok(None),
                }
            }
        }
    }
    Ok(Some(segments))
}

/// One row per interior node; a node with no char def makes the whole shape publish nothing.
fn shape_segments(
    table: &CharDefTable,
    shape: &SegmentedText,
    sources: &SourceGuids,
) -> Result<Option<Vec<FormSegment>>, FactsError> {
    let mut segments = Vec::new();
    for (_, _, char_def, _) in shape.shape.interior() {
        if char_def == NO_CHAR_DEF {
            return Ok(None);
        }
        segments.push(segment_row(table.get(CharDefId(char_def)), sources)?);
    }
    Ok(Some(segments))
}

fn segment_row(definition: &CharDef, sources: &SourceGuids) -> Result<FormSegment, FactsError> {
    let identity = segment_identity(definition, sources)?;
    // The compiler's morpheme boundary is a synthetic boundary definition, kept as kind boundary.
    let segment_kind = match definition.kind() {
        CharDefKind::Boundary => "boundary",
        CharDefKind::Segment => identity.member_kind,
    };
    Ok(FormSegment {
        segment_kind,
        phoneme_guid: identity.phoneme_guid,
        boundary_guid: identity.boundary_guid,
        natural_class_guid: None,
        token_text: definition.representations().first().cloned(),
    })
}
