//! Bounded local structural-allomorph lowering for the templated proposer.
//!
//! Covers one affine, adjacent suffix shape without enumerating roots: `lhs = [variable prefix,
//! one tail atom]`, `rhs = [Copy(Input(0)), InsertSegments...]`. The templated lexc emitter writes
//! an allomorph-owned marker alternative; this module compiles the local deletion relation and the
//! caller composes it after lexc and before phonology. Unsupported shapes receive no marker and
//! remain on the existing literal fallback path.
//!
//! The two-sided (circumfix) shape (`lhs = [one whole-root part]`, `rhs = [InsertSegments...,
//! Copy(Input(0)), InsertSegments...]`) needs no marker or rewrite composition at all: both halves'
//! text is already known statically, so `circumfix_texts` just hands the caller the two encoded
//! strings, and `crate::emit` writes each directly at its own real chain position.

use foma::constructions::{fsm_compose, fsm_union, fsm_universal};
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;
use pg_grammar::chardef::{CharDef, CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{
    AffixAllomorphDef, AllomorphId, Grammar, MorphRuleDef, NaturalClassKind, OutputAction, PartRef,
    Pattern, PatternNode, PhonRuleDef, SimpleContext, TableId,
};
use pg_shape::{NodeKind, Shape};
use std::collections::HashSet;

use crate::replace::SegAlphabet;

const MARKER_BASE: u32 = 0xF0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerZone {
    Prefix,
    Suffix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneRequirement {
    Caller,
    Intrinsic(MarkerZone),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerKey {
    pub allomorph: AllomorphId,
    pub zone: MarkerZone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerBinding {
    pub key: MarkerKey,
    pub symbol: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerBindingError {
    IntrinsicZoneMismatch {
        required: MarkerZone,
        actual: MarkerZone,
    },
    InvalidScalar,
}

/// Allocate a marker only after checked arithmetic has proved that the result is a Unicode
/// scalar.  Prefix and suffix bindings intentionally occupy different slots for one allomorph.
pub fn marker_binding_for(
    key: MarkerKey,
    requirement: ZoneRequirement,
) -> Result<MarkerBinding, MarkerBindingError> {
    if let ZoneRequirement::Intrinsic(required) = requirement {
        if required != key.zone {
            return Err(MarkerBindingError::IntrinsicZoneMismatch {
                required,
                actual: key.zone,
            });
        }
    }
    let zone_offset = match key.zone {
        MarkerZone::Prefix => 0,
        MarkerZone::Suffix => 1,
    };
    let code = MARKER_BASE
        .checked_add(
            key.allomorph
                .0
                .checked_mul(2)
                .ok_or(MarkerBindingError::InvalidScalar)?,
        )
        .and_then(|value| value.checked_add(zone_offset))
        .ok_or(MarkerBindingError::InvalidScalar)?;
    let symbol = char::from_u32(code).ok_or(MarkerBindingError::InvalidScalar)?;
    Ok(MarkerBinding { key, symbol })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorphologyRewrite {
    OrdinaryLiteral {
        variants: Vec<String>,
    },
    DirectWholeRootWrapper {
        prefix_variants: Vec<String>,
        suffix_variants: Vec<String>,
    },
    MarkedStructural {
        shape_id: &'static str,
        recipe: MorphologyRecipe,
        zone_requirement: ZoneRequirement,
    },
    Unsupported {
        shape_id: &'static str,
        reason_id: &'static str,
        allomorph: AllomorphId,
        source_table: TableId,
        active_table: TableId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorphologyRecipe {
    refs: Vec<u16>,
    literal_runs: Vec<Vec<String>>,
    output_segments: Vec<String>,
}

impl MorphologyRecipe {
    pub fn input_refs(&self) -> Vec<u16> {
        self.refs.clone()
    }

    pub fn literal_runs(&self) -> Vec<Vec<String>> {
        self.literal_runs.clone()
    }

    pub fn output_segments(&self) -> Vec<String> {
        self.output_segments.clone()
    }
}

pub struct MorphologyRewriteClassifier;

impl MorphologyRewriteClassifier {
    pub fn classify(
        grammar: &Grammar,
        allomorph: &AffixAllomorphDef,
        active_table: TableId,
    ) -> MorphologyRewrite {
        Self::classify_with_tables(grammar, allomorph, active_table, active_table)
    }

    pub fn classify_with_tables(
        grammar: &Grammar,
        allomorph: &AffixAllomorphDef,
        source_table: TableId,
        active_table: TableId,
    ) -> MorphologyRewrite {
        match classify_rewrite(grammar, allomorph, source_table, active_table) {
            Ok(result) => result,
            Err((shape_id, reason_id)) => MorphologyRewrite::Unsupported {
                shape_id,
                reason_id,
                allomorph: allomorph.id,
                source_table,
                active_table,
            },
        }
    }
}

type ClassifierResult<T> = Result<T, (&'static str, &'static str)>;

fn classify_rewrite(
    g: &Grammar,
    a: &AffixAllomorphDef,
    source_table: TableId,
    active_table: TableId,
) -> ClassifierResult<MorphologyRewrite> {
    if g.char_tables.get(source_table.0 as usize).is_none()
        || g.char_tables.get(active_table.0 as usize).is_none()
    {
        return Err(("InvalidReferences", "invalid-table-reference"));
    }
    if a.lhs.is_empty() {
        return Err(("UnlistedTopology", "missing-input-copy"));
    }
    for part in &a.lhs {
        if part.nodes.is_empty() {
            return Err(("UnlistedTopology", "empty-input-part"));
        }
        validate_pattern(g, source_table, part).map_err(|reason| ("InvalidReferences", reason))?;
    }

    if a.rhs
        .iter()
        .any(|action| matches!(action, OutputAction::InsertContext(_)))
    {
        return Err(("InsertContext", "insert-context"));
    }

    if a.rhs.is_empty()
        || a.rhs
            .iter()
            .all(|action| matches!(action, OutputAction::InsertSegments { .. }))
    {
        let variants = translated_literal_variants(g, &a.rhs, active_table)
            .map_err(|reason| ("OrdinaryLiteral", reason))?;
        return Ok(MorphologyRewrite::OrdinaryLiteral { variants });
    }

    let refs = referenced_inputs(&a.rhs).map_err(|reason| ("InvalidReferences", reason))?;
    if refs.iter().any(|index| (*index as usize) >= a.lhs.len()) {
        return Err(("InvalidReferences", "invalid-input-reference"));
    }
    if refs.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(("UnlistedTopology", "repeated-input-reference"));
    }
    if refs.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(("UnlistedTopology", "reordered-input-reference"));
    }

    if a.rhs
        .iter()
        .any(|action| matches!(action, OutputAction::Modify(..)))
    {
        return classify_terminal_modify(g, a, source_table, active_table, &refs);
    }

    let expected = (0..a.lhs.len() as u16).collect::<Vec<_>>();
    if refs == expected {
        if let Some((prefix, suffix)) = wrapper_runs(g, &a.rhs, active_table, a.lhs.len())
            .map_err(|reason| ("DirectWholeRootWrapper", reason))?
        {
            return Ok(MorphologyRewrite::DirectWholeRootWrapper {
                prefix_variants: prefix,
                suffix_variants: suffix,
            });
        }
        if a.lhs.len() >= 2 {
            if a.lhs
                .iter()
                .any(|part| pattern_has_boundary(g, source_table, part))
            {
                return Err(("AmharicInteriorInsertion", "non-segment-input-atom"));
            }
            if let Some(runs) = interior_runs(g, &a.rhs, active_table, a.lhs.len())
                .map_err(|reason| ("AmharicInteriorInsertion", reason))?
            {
                return Ok(marked(
                    "AmharicInteriorInsertion",
                    refs,
                    runs,
                    Vec::new(),
                    ZoneRequirement::Caller,
                ));
            }
        }
    }

    if a.lhs.len() == 2
        && a.rhs.len() >= 2
        && a.rhs.last() == Some(&OutputAction::Copy(PartRef::Input(1)))
        && is_fixed_segment_atom(g, source_table, a.lhs.first())
        && a.rhs[..a.rhs.len() - 1]
            .iter()
            .all(|action| matches!(action, OutputAction::InsertSegments { .. }))
    {
        if pattern_has_boundary(g, source_table, &a.lhs[0]) {
            return Err(("AmharicInitialVowelReplacement", "non-segment-input-atom"));
        }
        let variants = translated_literal_variants(g, &a.rhs[..a.rhs.len() - 1], active_table)
            .map_err(|reason| ("AmharicInitialVowelReplacement", reason))?;
        if variants != vec![String::new()] {
            return Ok(marked(
                "AmharicInitialVowelReplacement",
                vec![1],
                vec![variants],
                Vec::new(),
                ZoneRequirement::Intrinsic(MarkerZone::Prefix),
            ));
        }
    }

    if a.lhs.len() == 2
        && refs == [0]
        && matches!(a.rhs.first(), Some(OutputAction::Copy(PartRef::Input(0))))
        && a.rhs[1..]
            .iter()
            .all(|action| matches!(action, OutputAction::InsertSegments { .. }))
    {
        if !is_segment_only_atom(g, source_table, a.lhs.get(1)) {
            return Err(("AdjacentTerminalDrop", "non-segment-input-atom"));
        }
        let variants = translated_literal_variants(g, &a.rhs[1..], active_table)
            .map_err(|reason| ("AdjacentTerminalDrop", reason))?;
        return Ok(marked(
            "AdjacentTerminalDrop",
            vec![0],
            vec![variants],
            Vec::new(),
            ZoneRequirement::Intrinsic(MarkerZone::Suffix),
        ));
    }
    if a.lhs.len() == 2 && refs == [1] && a.rhs == [OutputAction::Copy(PartRef::Input(1))] {
        if !is_segment_only_atom(g, source_table, a.lhs.first()) {
            return Err(("AdjacentTerminalDrop", "non-segment-input-atom"));
        }
        return Ok(marked(
            "AdjacentInitialDrop",
            vec![1],
            Vec::new(),
            Vec::new(),
            ZoneRequirement::Intrinsic(MarkerZone::Prefix),
        ));
    }

    if refs.len() != a.lhs.len() {
        return Err(("UnlistedTopology", "missing-input-copy"));
    }
    Err(("UnlistedTopology", "unlisted-topology"))
}

fn marked(
    shape_id: &'static str,
    refs: Vec<u16>,
    literal_runs: Vec<Vec<String>>,
    output_segments: Vec<String>,
    zone_requirement: ZoneRequirement,
) -> MorphologyRewrite {
    MorphologyRewrite::MarkedStructural {
        shape_id,
        recipe: MorphologyRecipe {
            refs,
            literal_runs,
            output_segments,
        },
        zone_requirement,
    }
}

fn referenced_inputs(rhs: &[OutputAction]) -> Result<Vec<u16>, &'static str> {
    let mut refs = Vec::new();
    for action in rhs {
        let part = match action {
            OutputAction::Copy(PartRef::Input(index))
            | OutputAction::Modify(PartRef::Input(index), _) => *index,
            OutputAction::Copy(_) | OutputAction::Modify(_, _) => {
                return Err("invalid-part-reference-kind");
            }
            OutputAction::InsertSegments { .. } => continue,
            OutputAction::InsertContext(_) => continue,
        };
        refs.push(part);
    }
    Ok(refs)
}

fn lookup_char_def(table: &CharDefTable, id: CharDefId) -> Option<&CharDef> {
    table
        .iter()
        .find_map(|(candidate, definition)| (candidate == id).then_some(definition))
}

fn validate_pattern(g: &Grammar, table: TableId, pattern: &Pattern) -> Result<(), &'static str> {
    for node in &pattern.nodes {
        validate_node(g, table, node)?;
    }
    Ok(())
}

fn validate_node(g: &Grammar, table: TableId, node: &PatternNode) -> Result<(), &'static str> {
    match node {
        PatternNode::CharDef(id) => {
            let Some(_def) = lookup_char_def(&g.char_tables[table.0 as usize], *id) else {
                return Err("invalid-source-char-def");
            };
        }
        PatternNode::Context(context) => {
            if !context_members(g, table, context).is_some_and(|members| !members.is_empty()) {
                return Err("invalid-source-context");
            }
        }
        PatternNode::Quantifier { children, .. } => {
            for child in children {
                validate_node(g, table, child)?;
            }
        }
        PatternNode::Segments {
            table: node_table,
            shape,
        } => {
            if *node_table != table {
                return Err("invalid-source-table");
            }
            for (_, _, id, _) in shape.shape.interior() {
                let Some(_def) = lookup_char_def(&g.char_tables[table.0 as usize], CharDefId(id))
                else {
                    return Err("invalid-source-char-def");
                };
            }
        }
        PatternNode::Anchor(_) => {}
    }
    Ok(())
}

fn pattern_has_boundary(g: &Grammar, table: TableId, pattern: &Pattern) -> bool {
    pattern
        .nodes
        .iter()
        .any(|node| pattern_has_boundary_node(g, table, node))
}

fn pattern_has_boundary_node(g: &Grammar, table: TableId, node: &PatternNode) -> bool {
    match node {
        PatternNode::CharDef(id) => lookup_char_def(&g.char_tables[table.0 as usize], *id)
            .is_some_and(|definition| definition.kind() == CharDefKind::Boundary),
        PatternNode::Context(_) | PatternNode::Anchor(_) => false,
        PatternNode::Quantifier { children, .. } => children
            .iter()
            .any(|child| pattern_has_boundary_node(g, table, child)),
        PatternNode::Segments { shape, .. } => shape
            .shape
            .interior()
            .any(|(_, kind, _, _)| kind == NodeKind::Boundary),
    }
}

fn context_members(g: &Grammar, table: TableId, context: &SimpleContext) -> Option<Vec<CharDefId>> {
    let class = g.natural_classes.get(context.nat_class.0 as usize)?;
    let table_ref = g.char_tables.get(table.0 as usize)?;
    let members = match &class.kind {
        NaturalClassKind::Segments(ids) => ids.clone(),
        NaturalClassKind::Feature(pairs) => table_ref
            .iter()
            .filter(|(_, def)| def.kind() == CharDefKind::Segment)
            .filter(|(_, def)| {
                pairs.iter().all(|(feature, values)| {
                    def.feature_lanes()[feature.0 as usize] & values.0 != 0
                })
            })
            .map(|(id, _)| id)
            .collect(),
    };
    let mut out = Vec::new();
    for id in members {
        let def = lookup_char_def(table_ref, id)?;
        if def.kind() != CharDefKind::Segment {
            return None;
        }
        out.push(id);
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

fn is_segment_only_atom(g: &Grammar, table: TableId, pattern: Option<&Pattern>) -> bool {
    let Some(pattern) = pattern else { return false };
    let [node] = pattern.nodes.as_slice() else {
        return false;
    };
    match node {
        PatternNode::CharDef(id) => g
            .char_tables
            .get(table.0 as usize)
            .and_then(|table| lookup_char_def(table, *id))
            .is_some_and(|def| def.kind() == CharDefKind::Segment),
        PatternNode::Context(context) => {
            context_members(g, table, context).is_some_and(|m| !m.is_empty())
        }
        _ => false,
    }
}

fn is_fixed_segment_atom(g: &Grammar, table: TableId, pattern: Option<&Pattern>) -> bool {
    let Some(pattern) = pattern else { return false };
    let [PatternNode::CharDef(id)] = pattern.nodes.as_slice() else {
        return false;
    };
    g.char_tables
        .get(table.0 as usize)
        .and_then(|table| lookup_char_def(table, *id))
        .is_some_and(|def| def.kind() == CharDefKind::Segment)
}

fn translated_literal_variants(
    g: &Grammar,
    actions: &[OutputAction],
    active_table: TableId,
) -> Result<Vec<String>, &'static str> {
    let mut variants = vec![String::new()];
    for action in actions {
        let OutputAction::InsertSegments {
            table: source_table,
            shape,
        } = action
        else {
            return Err("nonliteral-output-action");
        };
        let Some(source) = g.char_tables.get(source_table.0 as usize) else {
            return Err("untranslatable-output-table");
        };
        if shape.shape.interior().any(|(_, _, id, _)| {
            lookup_char_def(source, CharDefId(id))
                .is_some_and(|definition| definition.kind() != CharDefKind::Segment)
        }) {
            return Err("non-segment-output-atom");
        }
        let pieces = translated_shape_variants(g, *source_table, active_table, &shape.shape)
            .ok_or("untranslatable-output-table")?;
        let mut next = Vec::new();
        for prefix in &variants {
            for piece in &pieces {
                let value = format!("{prefix}{piece}");
                if !next.contains(&value) {
                    next.push(value);
                }
            }
        }
        variants = next;
    }
    Ok(variants)
}

fn translated_shape_variants(
    g: &Grammar,
    source_table: TableId,
    active_table: TableId,
    shape: &Shape,
) -> Option<Vec<String>> {
    let source = g.char_tables.get(source_table.0 as usize)?;
    let active = g.char_tables.get(active_table.0 as usize)?;
    let mut variants = vec![String::new()];
    for (_, _, raw_id, _) in shape.interior() {
        let source_id = CharDefId(raw_id);
        let source_def = lookup_char_def(source, source_id)?;
        if source_def.kind() != CharDefKind::Segment {
            return None;
        }
        let mut pieces = Vec::new();
        for representation in source_def.representations_nfd() {
            let Some(active_id) = active.lookup_nfd(representation) else {
                continue;
            };
            let active_def = lookup_char_def(active, active_id)?;
            if active_def.kind() != CharDefKind::Segment {
                return None;
            }
            pieces.extend(active_def.representations_nfd().iter().cloned());
        }
        pieces.dedup();
        if pieces.is_empty() {
            return None;
        }
        let mut next = Vec::new();
        for prefix in &variants {
            for piece in &pieces {
                next.push(format!("{prefix}{piece}"));
            }
        }
        variants = next;
    }
    Some(variants)
}

fn wrapper_runs(
    g: &Grammar,
    actions: &[OutputAction],
    active_table: TableId,
    parts: usize,
) -> Result<Option<(Vec<String>, Vec<String>)>, &'static str> {
    let first_copy = actions
        .iter()
        .position(|action| matches!(action, OutputAction::Copy(PartRef::Input(_))));
    let Some(first_copy) = first_copy else {
        return Ok(None);
    };
    let mut cursor = first_copy;
    for expected in 0..parts as u16 {
        if !matches!(actions.get(cursor), Some(OutputAction::Copy(PartRef::Input(index))) if *index == expected)
        {
            return Ok(None);
        }
        cursor += 1;
    }
    if actions[first_copy + parts..]
        .iter()
        .any(|action| !matches!(action, OutputAction::InsertSegments { .. }))
        || actions[..first_copy]
            .iter()
            .any(|action| !matches!(action, OutputAction::InsertSegments { .. }))
    {
        return Ok(None);
    }
    let prefix = translated_literal_variants(g, &actions[..first_copy], active_table)?;
    let suffix = translated_literal_variants(g, &actions[cursor..], active_table)?;
    if prefix == vec![String::new()] || suffix == vec![String::new()] {
        return Ok(None);
    }
    Ok(Some((prefix, suffix)))
}

fn interior_runs(
    g: &Grammar,
    actions: &[OutputAction],
    active_table: TableId,
    parts: usize,
) -> Result<Option<Vec<Vec<String>>>, &'static str> {
    let mut cursor = 0;
    let mut runs = Vec::with_capacity(parts.saturating_sub(1));
    for expected in 0..parts as u16 {
        if !matches!(actions.get(cursor), Some(OutputAction::Copy(PartRef::Input(index))) if *index == expected)
        {
            return Ok(None);
        }
        cursor += 1;
        if expected + 1 == parts as u16 {
            break;
        }
        let start = cursor;
        while matches!(
            actions.get(cursor),
            Some(OutputAction::InsertSegments { .. })
        ) {
            cursor += 1;
        }
        let values = translated_literal_variants(g, &actions[start..cursor], active_table)?;
        runs.push(if values == vec![String::new()] {
            Vec::new()
        } else {
            values
        });
    }
    if cursor != actions.len() || runs.iter().all(Vec::is_empty) {
        return Ok(None);
    }
    Ok(Some(runs))
}

fn classify_terminal_modify(
    g: &Grammar,
    a: &AffixAllomorphDef,
    source_table: TableId,
    active_table: TableId,
    refs: &[u16],
) -> ClassifierResult<MorphologyRewrite> {
    let n = a.lhs.len();
    let Some(OutputAction::Modify(PartRef::Input(index), context)) = a.rhs.last() else {
        return Err(("ModifyFromInput", "modify-nonterminal"));
    };
    if n < 2 || *index as usize != n - 1 || a.rhs.len() != n {
        return Err(("ModifyFromInput", "modify-nonterminal"));
    }
    if !a.rhs[..n - 1].iter().enumerate().all(|(expected, action)| {
        matches!(action, OutputAction::Copy(PartRef::Input(index)) if *index as usize == expected)
    }) {
        return Err(("ModifyFromInput", "modify-nonterminal"));
    }
    if !context.vars.is_empty() {
        return Err(("ModifyFromInput", "terminal-modify-variable"));
    }
    let Some(last) = a.lhs.get(n - 1) else {
        return Err(("ModifyFromInput", "terminal-modify-multi-segment"));
    };
    let [node] = last.nodes.as_slice() else {
        if last
            .nodes
            .iter()
            .any(|node| matches!(node, PatternNode::Quantifier { .. }))
        {
            return Err(("ModifyFromInput", "terminal-modify-quantified"));
        }
        return Err(("ModifyFromInput", "terminal-modify-multi-segment"));
    };
    if matches!(node, PatternNode::Quantifier { .. }) {
        return Err(("ModifyFromInput", "terminal-modify-quantified"));
    }
    if !matches!(node, PatternNode::CharDef(_) | PatternNode::Context(_)) {
        return Err(("ModifyFromInput", "terminal-modify-multi-segment"));
    }
    if !is_segment_only_atom(g, source_table, Some(last)) {
        return Err(("ModifyFromInput", "terminal-modify-source-atom"));
    }
    let members = context_members(g, source_table, context)
        .ok_or(("ModifyFromInput", "terminal-modify-empty-output"))?;
    if members.is_empty() {
        return Err(("ModifyFromInput", "terminal-modify-empty-output"));
    }
    let output_segments = translated_ids(g, source_table, active_table, &members)
        .ok_or(("ModifyFromInput", "untranslatable-output-table"))?;
    if output_segments.is_empty() {
        return Err(("ModifyFromInput", "terminal-modify-empty-output"));
    }
    Ok(marked(
        "AmharicTerminalModify",
        refs.to_vec(),
        Vec::new(),
        output_segments,
        ZoneRequirement::Caller,
    ))
}

fn translated_ids(
    g: &Grammar,
    source_table: TableId,
    active_table: TableId,
    ids: &[CharDefId],
) -> Option<Vec<String>> {
    let source = g.char_tables.get(source_table.0 as usize)?;
    let active = g.char_tables.get(active_table.0 as usize)?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        let source_def = lookup_char_def(source, *id)?;
        if source_def.kind() != CharDefKind::Segment {
            return None;
        }
        let mut mapped = false;
        for rep in source_def.representations_nfd() {
            let active_id = active.lookup_nfd(rep)?;
            let active_def = lookup_char_def(active, active_id)?;
            if active_def.kind() != CharDefKind::Segment {
                return None;
            }
            mapped = true;
            for active_rep in active_def.representations_nfd() {
                if seen.insert(active_rep.clone()) {
                    out.push(active_rep.clone());
                }
            }
        }
        if !mapped {
            return None;
        }
    }
    (!out.is_empty()).then_some(out)
}

#[derive(Debug, Clone)]
struct LocalRecipe {
    allomorph: AllomorphId,
    table: TableId,
    tail_members: Vec<CharDefId>,
    inserted: String,
    leading: bool,
}

pub(crate) fn marker_for(allomorph: AllomorphId) -> Option<char> {
    char::from_u32(MARKER_BASE.checked_add(allomorph.0)?)
}

fn encode_insert_actions(
    g: &Grammar,
    alphabet: &SegAlphabet,
    actions: &[OutputAction],
) -> Option<Vec<String>> {
    if actions.is_empty() {
        return None;
    }
    let mut variants = vec![String::new()];
    for action in actions {
        let OutputAction::InsertSegments { table, shape } = action else {
            return None;
        };
        let origin = g.char_tables.get(table.0 as usize)?;
        let pieces = crate::emit::underlying_shape_variants(alphabet, origin, &shape.shape);
        if pieces.is_empty() {
            return None;
        }
        let mut next = Vec::with_capacity(variants.len() * pieces.len());
        for prefix in &variants {
            for piece in &pieces {
                let mut encoded = prefix.clone();
                encoded.push_str(piece);
                next.push(encoded);
            }
        }
        variants = next;
    }
    Some(variants)
}

/// The already-encoded `(prefix, suffix)` token text for `allomorph`, if it matches a single,
/// non-reduplicated `Copy(Input(0))` wrapped by leading and trailing `InsertSegments` over a
/// 1-part LHS (`Role::CircumfixPrefix`'s shape); an interior insert or a repeated copy of the same
/// part needs root-internal splitting or duplication this cannot represent, so stays uncovered.
pub(crate) fn circumfix_texts(
    g: &Grammar,
    alphabet: &SegAlphabet,
    allomorph: &AffixAllomorphDef,
) -> Option<Vec<(String, String)>> {
    if allomorph.lhs.len() != 1 {
        return None;
    }
    let rhs = allomorph.rhs.as_slice();
    let mut copy_positions = rhs
        .iter()
        .enumerate()
        .filter_map(|(i, a)| matches!(a, OutputAction::Copy(PartRef::Input(0))).then_some(i));
    let copy_pos = copy_positions.next()?;
    if copy_positions.next().is_some() {
        return None;
    }
    if copy_pos == 0 || copy_pos == rhs.len() - 1 {
        return None;
    }
    let prefixes = encode_insert_actions(g, alphabet, &rhs[..copy_pos])?;
    let suffixes = encode_insert_actions(g, alphabet, &rhs[copy_pos + 1..])?;
    let mut pairs = Vec::with_capacity(prefixes.len() * suffixes.len());
    for prefix in prefixes {
        for suffix in &suffixes {
            pairs.push((prefix.clone(), suffix.clone()));
        }
    }
    Some(pairs)
}

#[cfg(test)]
mod circumfix_text_tests {
    use super::*;

    const XML: &str = r#"<HermitCrabInput><Language><Name>CrossTableCircumfix</Name>
      <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="inner"><Name>Inner</Name><SegmentDefinitions>
        <SegmentDefinition id="ix"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="iz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="iq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions></CharacterDefinitionTable>
      <CharacterDefinitionTable id="outer"><Name>Outer</Name><SegmentDefinitions>
        <SegmentDefinition id="ow"><Representations><Representation>w</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="oq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="oz"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ox"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions></CharacterDefinitionTable>
      <NaturalClasses><FeatureNaturalClass id="any"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
      <Strata><Stratum characterDefinitionTable="inner" morphologicalRuleOrder="unordered" morphologicalRules="m">
        <Name>Inner</Name><MorphologicalRuleDefinitions><MorphologicalRule id="m" requiredPartsOfSpeech="p" outputPartOfSpeech="p">
          <Name>M</Name><MorphologicalSubrules><MorphologicalSubrule id="a">
            <MorphologicalInput><PhoneticSequence id="s"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="any" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><InsertSegments><PhoneticShape>x</PhoneticShape></InsertSegments><CopyFromInput index="s" /><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule></MorphologicalSubrules><MorphemeId>M</MorphemeId>
        </MorphologicalRule></MorphologicalRuleDefinitions></Stratum>
        <Stratum characterDefinitionTable="outer" morphologicalRuleOrder="unordered"><Name>Outer</Name></Stratum>
      </Strata></Language></HermitCrabInput>"#;

    #[test]
    fn circumfix_text_uses_surface_table_tokens_for_foreign_insertions() {
        let g = pg_grammar::load(XML).expect("fixture must load");
        let allomorph = match &g.mrules[0] {
            MorphRuleDef::AffixProcess(def) => &def.allomorphs[0],
            other => panic!("m must be affix-process, got {other:?}"),
        };
        let surface = &g.char_tables[1];
        let alphabet = SegAlphabet::new(surface);
        let texts = circumfix_texts(&g, &alphabet, allomorph).expect("circumfix shape");
        assert_eq!(
            texts,
            vec![(
                alphabet.token(surface.lookup_nfd("x").unwrap()).to_string(),
                alphabet.token(surface.lookup_nfd("z").unwrap()).to_string(),
            )]
        );
    }
}

fn class_members(g: &Grammar, table: TableId, node: &PatternNode) -> Option<Vec<CharDefId>> {
    let table_ref = g.char_tables.get(table.0 as usize)?;
    let mut members = match node {
        PatternNode::CharDef(id) => vec![*id],
        PatternNode::Context(context) if context.vars.is_empty() => {
            match &g.natural_classes.get(context.nat_class.0 as usize)?.kind {
                NaturalClassKind::Segments(ids) => ids.clone(),
                NaturalClassKind::Feature(pairs) => table_ref
                    .iter()
                    .filter(|(_, definition)| definition.kind() == CharDefKind::Segment)
                    .filter(|(_, definition)| {
                        pairs.iter().all(|(feature, values)| {
                            definition.feature_lanes()[feature.0 as usize] & values.0 != 0
                        })
                    })
                    .map(|(id, _)| id)
                    .collect(),
            }
        }
        PatternNode::Segments {
            table: node_table,
            shape,
        } if *node_table == table => shape
            .shape
            .interior()
            .map(|(_, _, char_def, _)| CharDefId(char_def))
            .collect(),
        _ => return None,
    };
    members.sort_by_key(|id| id.0);
    members.dedup();
    (!members.is_empty()).then_some(members)
}

fn recipe_for(
    g: &Grammar,
    allomorph: &AffixAllomorphDef,
    table_hint: &CharDefTable,
) -> Option<LocalRecipe> {
    if allomorph.lhs.len() != 2 {
        return None;
    }
    let (leading, dropped_node, rest) = match allomorph.rhs.as_slice() {
        [OutputAction::Copy(PartRef::Input(0)), rest @ ..] => (false, &allomorph.lhs[1], rest),
        [OutputAction::Copy(PartRef::Input(1))] => {
            (true, &allomorph.lhs[0], &[] as &[OutputAction])
        }
        _ => return None,
    };
    let [dropped_node] = dropped_node.nodes.as_slice() else {
        return None;
    };
    let mut table = None;
    let mut inserted_shapes = Vec::new();
    for action in rest {
        let OutputAction::InsertSegments {
            table: action_table,
            shape,
        } = action
        else {
            return None;
        };
        if table.is_some_and(|existing| existing != *action_table) {
            return None;
        }
        table = Some(*action_table);
        inserted_shapes.push(shape);
    }
    let table = match table {
        Some(table) => table,
        None => g
            .char_tables
            .iter()
            .position(|candidate| std::ptr::eq(candidate, table_hint))
            .and_then(|index| u16::try_from(index).ok())
            .map(TableId)?,
    };
    let alphabet = SegAlphabet::new(g.char_tables.get(table.0 as usize)?);
    let tail_members = class_members(g, table, dropped_node)?;
    let inserted = inserted_shapes
        .into_iter()
        .map(|shape| alphabet.encode_shape(&shape.shape))
        .collect();
    Some(LocalRecipe {
        allomorph: allomorph.id,
        table,
        tail_members,
        inserted,
        leading,
    })
}

pub(crate) fn structural_marker_for_zone(
    g: &Grammar,
    allomorph: &AffixAllomorphDef,
    table_hint: &CharDefTable,
    prefix_zone: bool,
) -> Option<char> {
    recipe_for(g, allomorph, table_hint)
        .filter(|_| !prefix_zone)
        .and_then(|recipe| marker_for(recipe.allomorph))
}

fn atom(tokens: &[char]) -> String {
    match tokens {
        [only] => only.to_string(),
        many => format!(
            "[{}]",
            many.iter()
                .map(char::to_string)
                .collect::<Vec<_>>()
                .join(" | ")
        ),
    }
}

fn spaced(text: &str) -> String {
    text.chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Modifier letters and the degree-style placeholder count as technical realization symbols when they occur as singleton rewrite inputs.
fn is_floating_marker_representation(representation: &str) -> bool {
    !representation.is_empty()
        && representation
            .chars()
            .all(|ch| matches!(ch as u32, 0x02B0..=0x02FF | 0x1D2C..=0x1DFF) || ch == '°')
}

/// Compile a recall-safe identity-or-cleanup relation for technical floating markers that the
/// grammar itself uses as singleton rewrite inputs. The normal environment-sensitive cascade gets
/// first chance to realize them; this final fallback permits an unrealized marker to disappear.
/// Ordinary IPA segments and multi-member natural classes are deliberately excluded.
pub fn compile_authored_deletion_fallback(
    opts: &FomaOptions,
    g: &Grammar,
    pipeline_alphabet: &SegAlphabet,
) -> Option<Fsm> {
    let pipeline_table = g
        .char_tables
        .iter()
        .position(|table| std::ptr::eq(table, pipeline_alphabet.table()))?;
    let pipeline_table = TableId(pipeline_table as u16);
    let mut targets = Vec::new();
    for stratum in &g.strata {
        if stratum.table != pipeline_table {
            continue;
        }
        for rule_id in &stratum.prules {
            let PhonRuleDef::Rewrite(rule) = g.prules.get(rule_id.0 as usize)? else {
                continue;
            };
            let [node] = rule.lhs.nodes.as_slice() else {
                continue;
            };
            let Some(ids) = class_members(g, stratum.table, node) else {
                continue;
            };
            let [id] = ids.as_slice() else {
                continue;
            };
            let definition = pipeline_alphabet.table().get(*id);
            if definition.kind() != CharDefKind::Segment
                || !definition
                    .representations()
                    .iter()
                    .any(|text| is_floating_marker_representation(text))
            {
                continue;
            }
            targets.push(pipeline_alphabet.token(*id));
        }
    }
    targets.sort_unstable();
    targets.dedup();
    if targets.is_empty() {
        return None;
    }
    let regex = targets
        .iter()
        .map(|token| format!("{token} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    let delete = fsm_parse_regex(opts, &regex, None, None)?;
    Some(fsm_union(opts, delete, fsm_universal()))
}
/// Compile every supported local recipe. Returns `None` when the grammar contains no supported
/// shape, so existing grammars remain byte-for-byte on the old pipeline.
pub fn compile_layer(
    opts: &FomaOptions,
    g: &Grammar,
    pipeline_alphabet: &SegAlphabet,
) -> Option<Fsm> {
    let pipeline_table = g
        .char_tables
        .iter()
        .position(|table| std::ptr::eq(table, pipeline_alphabet.table()))?;
    let pipeline_table = TableId(pipeline_table as u16);
    let mut net = None;
    for rule in &g.mrules {
        let MorphRuleDef::AffixProcess(definition) = rule else {
            continue;
        };
        for allomorph in &definition.allomorphs {
            if std::env::var_os("PANGLOSS_TRACE_STRUCTURAL_RECIPES").is_some() {
                eprintln!(
                    "structural-shape\t{:?}\t{:?}\tmatched={}",
                    allomorph.lhs,
                    allomorph.rhs,
                    recipe_for(g, allomorph, pipeline_alphabet.table()).is_some()
                );
            }
            let Some(recipe) = recipe_for(g, allomorph, pipeline_alphabet.table()) else {
                continue;
            };
            if recipe.table != pipeline_table {
                continue;
            }
            let marker = marker_for(recipe.allomorph)?;
            let tails: Vec<char> = recipe
                .tail_members
                .iter()
                .map(|id| pipeline_alphabet.token(*id))
                .collect();
            let output = if recipe.inserted.is_empty() {
                "0".to_string()
            } else {
                spaced(&recipe.inserted)
            };
            let regex = if recipe.leading {
                let segments: Vec<char> = g
                    .char_tables
                    .get(recipe.table.0 as usize)
                    .into_iter()
                    .flat_map(|table| table.iter())
                    .filter(|(_, definition)| definition.kind() == CharDefKind::Segment)
                    .map(|(id, _)| pipeline_alphabet.token(id))
                    .collect();
                let right_context = format!("{}* {}", atom(&segments), marker);
                let drop = fsm_parse_regex(
                    opts,
                    &format!("{} -> 0 || .#. _ {}", atom(&tails), right_context),
                    None,
                    None,
                )
                .unwrap_or_else(|| panic!("foma rejected structural allomorph regex"));
                let marker_delete = fsm_parse_regex(opts, &format!("{} -> 0", marker), None, None)
                    .unwrap_or_else(|| panic!("foma rejected structural allomorph marker cleanup"));
                fsm_compose(opts, drop, marker_delete)
            } else {
                let regex = format!("{} {} -> {}", atom(&tails), marker, output);
                fsm_parse_regex(opts, &regex, None, None)
                    .unwrap_or_else(|| panic!("foma rejected structural allomorph regex {regex:?}"))
            };
            net = Some(match net {
                None => regex,
                Some(previous) => fsm_compose(opts, previous, regex),
            });
        }
    }
    net
}
