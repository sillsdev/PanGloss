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
use std::collections::HashSet;
use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{
    AffixAllomorphDef, AllomorphId, Grammar, MorphRuleDef, NaturalClassKind, OutputAction, PartRef,
    Pattern, PatternNode, PhonRuleDef, TableId,
};

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

/// The closed set of structural rewrites that the templated proposer can lower faithfully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorphologyRewrite {
    OrdinaryLiteral { variants: Vec<String> },
    DirectWholeRootWrapper {
        prefix_variants: Vec<String>,
        suffix_variants: Vec<String>,
    },
    MarkedStructural {
        shape_id: &'static str,
        recipe: MorphologyRecipe,
        marker: char,
        zone_requirement: ZoneRequirement,
    },
    Unsupported {
        shape_id: &'static str,
        reason_id: &'static str,
    },
}

/// Stable, validated data passed from classification to the marker relation.
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

/// Classifies one allomorph against its source and active pipeline tables.
pub struct MorphologyRewriteClassifier;

impl MorphologyRewriteClassifier {
    /// Compatibility entrypoint for grammars whose source and active tables are the same.
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
            Err((shape, reason)) => MorphologyRewrite::Unsupported {
                shape_id: shape,
                reason_id: reason,
            },
        }
    }
}

fn unsupported(shape: &'static str, reason: &'static str) -> Result<MorphologyRewrite, (&'static str, &'static str)> {
    Err((shape, reason))
}

fn unsupported_text<T>(shape: &'static str, reason: &'static str) -> Result<T, (&'static str, &'static str)> {
    Err((shape, reason))
}

fn classify_rewrite(
    g: &Grammar,
    a: &AffixAllomorphDef,
    source_table: TableId,
    active_table: TableId,
) -> Result<MorphologyRewrite, (&'static str, &'static str)> {
    if a.lhs.is_empty() {
        return unsupported("UnlistedTopology", "missing-input-copy");
    }
    if a.lhs.iter().any(|part| part.nodes.is_empty()) {
        return unsupported("UnlistedTopology", "empty-input-part");
    }
    if a.rhs.iter().any(|action| matches!(action, OutputAction::InsertContext(_))) {
        return unsupported("InsertContext", "insert-context");
    }

    // Ordinary output contains only finite InsertSegments translated into the active table.
    if a.rhs.is_empty()
        || a.rhs.iter().all(|action| matches!(action, OutputAction::InsertSegments { .. }))
    {
        let variants = literal_variants(g, active_table, &a.rhs)?;
        return Ok(MorphologyRewrite::OrdinaryLiteral { variants });
    }

    // Validate every Input reference before matching a structural recipe.
    let refs = a
        .rhs
        .iter()
        .filter_map(|action| match action {
            OutputAction::Copy(PartRef::Input(index)) => Some(*index),
            OutputAction::Modify(PartRef::Input(index), _) => Some(*index),
            OutputAction::Copy(_) => Some(u16::MAX),
            OutputAction::Modify(_, _) => Some(u16::MAX),
            _ => None,
        })
        .collect::<Vec<_>>();
    if a.rhs.iter().any(|action| matches!(
        action,
        OutputAction::Copy(PartRef::Head(_) | PartRef::NonHead(_))
            | OutputAction::Modify(PartRef::Head(_) | PartRef::NonHead(_), _)
    )) {
        return unsupported("InvalidReferences", "invalid-part-reference-kind");
    }
    if refs.iter().any(|index| *index == u16::MAX || (*index as usize) >= a.lhs.len()) {
        return unsupported("InvalidReferences", "invalid-input-reference");
    }
    let mut sorted = refs.clone();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() != refs.len() {
        return unsupported("UnlistedTopology", "repeated-input-reference");
    }
    if refs.windows(2).any(|pair| pair[0] > pair[1]) {
        return unsupported("UnlistedTopology", "reordered-input-reference");
    }

    // Modification requires exactly C(0)..C(n-2), then one final Modify(Input(n-1), Q).
    if a.rhs.iter().any(|action| matches!(action, OutputAction::Modify(PartRef::Head(_) | PartRef::NonHead(_), _))) {
        return unsupported("InvalidReferences", "invalid-part-reference-kind");
    }
    if a.rhs.iter().any(|action| matches!(action, OutputAction::Modify(..))) {
        let n = a.lhs.len();
        let Some(OutputAction::Modify(PartRef::Input(modify_index), context)) = a.rhs.last() else {
            return unsupported("ModifyFromInput", "modify-nonterminal");
        };
        if *modify_index as usize != n.saturating_sub(1) {
            return unsupported("ModifyFromInput", "modify-nonterminal");
        }
        if n < 2 || a.rhs.len() != n {
            return unsupported("ModifyFromInput", "unlisted-topology");
        }
        if !a.rhs[..n - 1].iter().enumerate().all(|(index, action)| {
            matches!(action, OutputAction::Copy(PartRef::Input(reference)) if *reference as usize == index)
        }) {
            return unsupported("ModifyFromInput", "unlisted-topology");
        }
        if !context.vars.is_empty() {
            return unsupported("ModifyFromInput", "terminal-modify-variable");
        }
        if !is_exactly_one_segment(a.lhs.get(n - 1)) {
            let reason = if is_quantified(a.lhs.get(n - 1)) {
                "terminal-modify-quantified"
            } else {
                "terminal-modify-multi-segment"
            };
            return unsupported("ModifyFromInput", reason);
        }
        if !lowerable_atom(g, source_table, a.lhs.get(n - 1)) {
            return unsupported("ModifyFromInput", "terminal-modify-source-atom");
        }
        let outputs = class_members(g, source_table, &PatternNode::Context(context.clone()))
            .ok_or(("ModifyFromInput", "terminal-modify-empty-output"))?;
        let output_segments = translated_ids(g, source_table, active_table, &outputs)
            .ok_or(("ModifyFromInput", "untranslatable-output-table"))?;
        if output_segments.is_empty() {
            return unsupported("ModifyFromInput", "terminal-modify-empty-output");
        }
        return Ok(MorphologyRewrite::MarkedStructural {
            shape_id: "AmharicTerminalModify",
            recipe: MorphologyRecipe {
                refs,
                literal_runs: Vec::new(),
                output_segments,
            },
            marker: marker_for(a.id).ok_or(("InvalidReferences", "invalid-allomorph-id"))?,
            zone_requirement: ZoneRequirement::Caller,
        });
    }

    let copy_refs = refs.clone();

    // Whole-root wrappers preserve all parts and reject interior literals.
    if copy_refs == (0..a.lhs.len() as u16).collect::<Vec<_>>() {
        if let Some((prefix, suffix)) = wrapper_runs(g, active_table, &a.rhs, a.lhs.len())? {
            return Ok(MorphologyRewrite::DirectWholeRootWrapper {
                prefix_variants: prefix,
                suffix_variants: suffix,
            });
        }
    }

    // Interior insertion preserves parts and places literals between adjacent copies.
    if a.lhs.len() >= 2
        && copy_refs == (0..a.lhs.len() as u16).collect::<Vec<_>>()
    {
        if let Some(runs) = interior_runs(g, active_table, &a.rhs, a.lhs.len())? {
            return marked(
                g,
                a,
                "AmharicInteriorInsertion",
                copy_refs,
                runs,
                ZoneRequirement::Caller,
            );
        }
    }

    // Initial replacement consumes one fixed CharDef and copies the remainder.
    if a.lhs.len() == 2
        && a.rhs.len() >= 2
        && a.rhs.last() == Some(&OutputAction::Copy(PartRef::Input(1)))
        && is_fixed_atom(a.lhs.first())
        && lowerable_atom(g, source_table, a.lhs.first())
    {
        let literal_actions = &a.rhs[..a.rhs.len() - 1];
        if literal_actions.iter().all(|action| matches!(action, OutputAction::InsertSegments { .. })) {
            let variants = literal_variants(g, active_table, literal_actions)?;
            if variants != vec![String::new()] {
                return marked(
                    g,
                    a,
                    "AmharicInitialVowelReplacement",
                    vec![1],
                    vec![variants],
                    ZoneRequirement::Intrinsic(MarkerZone::Prefix),
                );
            }
        }
    }

    // Bounded drops omit exactly one lowerable edge part.
    if a.lhs.len() == 2
        && copy_refs == [0]
        && a.rhs.first() == Some(&OutputAction::Copy(PartRef::Input(0)))
        && a.rhs[1..].iter().all(|action| matches!(action, OutputAction::InsertSegments { .. }))
        && lowerable_atom(g, source_table, a.lhs.get(1))
    {
        let variants = literal_variants(g, active_table, &a.rhs[1..])?;
        return marked(
            g,
            a,
            "AdjacentTerminalDrop",
            copy_refs,
            vec![if variants == vec![String::new()] { Vec::new() } else { variants }],
            ZoneRequirement::Intrinsic(MarkerZone::Suffix),
        );
    }
    if a.lhs.len() == 2
        && copy_refs == [1]
        && a.rhs == [OutputAction::Copy(PartRef::Input(1))]
        && lowerable_atom(g, source_table, a.lhs.first())
    {
        return marked(
            g,
            a,
            "AdjacentInitialDrop",
            copy_refs,
            Vec::new(),
            ZoneRequirement::Intrinsic(MarkerZone::Prefix),
        );
    }

    if copy_refs.len() != a.lhs.len() {
        return unsupported("UnlistedTopology", "missing-input-copy");
    }
    unsupported("UnlistedTopology", "unlisted-topology")
}

fn marked(
    _g: &Grammar,
    a: &AffixAllomorphDef,
    shape_id: &'static str,
    refs: Vec<u16>,
    literal_runs: Vec<Vec<String>>,
    zone_requirement: ZoneRequirement,
) -> Result<MorphologyRewrite, (&'static str, &'static str)> {
    Ok(MorphologyRewrite::MarkedStructural {
        shape_id,
        recipe: MorphologyRecipe {
            refs,
            literal_runs,
            output_segments: Vec::new(),
        },
        marker: marker_for(a.id).ok_or(("InvalidReferences", "invalid-allomorph-id"))?,
        zone_requirement,
    })
}

fn is_quantified(pattern: Option<&Pattern>) -> bool {
    pattern.is_some_and(|pattern| pattern.nodes.iter().any(|node| matches!(node, PatternNode::Quantifier { .. })))
}

fn is_exactly_one_segment(pattern: Option<&Pattern>) -> bool {
    pattern.is_some_and(|pattern| pattern.nodes.len() == 1 && is_segment_node(&pattern.nodes[0]))
}

fn is_fixed_atom(pattern: Option<&Pattern>) -> bool {
    pattern.is_some_and(|pattern| pattern.nodes.len() == 1 && matches!(pattern.nodes[0], PatternNode::CharDef(_)))
}

fn is_segment_node(node: &PatternNode) -> bool {
    matches!(node, PatternNode::CharDef(_) | PatternNode::Context(_))
}

fn lowerable_atom(g: &Grammar, table: TableId, pattern: Option<&Pattern>) -> bool {
    let Some(pattern) = pattern else {
        return false;
    };
    let [node] = pattern.nodes.as_slice() else {
        return false;
    };
    let Some(members) = class_members(g, table, node) else {
        return false;
    };
    let Some(table_ref) = g.char_tables.get(table.0 as usize) else {
        return false;
    };
    members.iter().all(|id| table_ref.iter().any(|(candidate, _)| candidate == *id))
}

/// Translate source-table IDs through representation text into active-table variants.
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
        let source_def = source.iter().find_map(|(candidate, definition)| {
            (candidate == *id).then_some(definition)
        })?;
        let mut mapped = false;
        for representation in source_def.representations_nfd() {
            let Some(active_id) = active.lookup_nfd(representation) else {
                continue;
            };
            for active_representation in active.get(active_id).representations_nfd() {
                mapped = true;
                if seen.insert(active_representation.clone()) {
                    out.push(active_representation.clone());
                }
            }
        }
        if !mapped {
            return None;
        }
    }
    (!out.is_empty()).then_some(out)
}

fn literal_variants(
    g: &Grammar,
    active_table: TableId,
    actions: &[OutputAction],
) -> Result<Vec<String>, (&'static str, &'static str)> {
    let mut variants = vec![String::new()];
    for action in actions {
        let OutputAction::InsertSegments { table: action_table, shape } = action else {
            return unsupported_text("OrdinaryLiteral", "nonliteral-output-action");
        };
        let ids = shape
            .shape
            .interior()
            .map(|(_, _, id, _)| CharDefId(id))
            .collect::<Vec<_>>();
        let pieces = translated_shape_variants(g, *action_table, active_table, &ids)
            .ok_or(("OrdinaryLiteral", "untranslatable-output-table"))?;
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
    ids: &[CharDefId],
) -> Option<Vec<String>> {
    let mut variants = vec![String::new()];
    for id in ids {
        let pieces = translated_ids(g, source_table, active_table, &[*id])?;
        let mut next = Vec::with_capacity(variants.len() * pieces.len());
        for prefix in &variants {
            for piece in &pieces {
                next.push(format!("{prefix}{piece}"));
            }
        }
        variants = next;
    }
    Some(variants)
}

/// Parse `I* C(0)..C(n-1) I*`, with literals only outside the copied root.
fn wrapper_runs(
    g: &Grammar,
    active_table: TableId,
    actions: &[OutputAction],
    parts: usize,
) -> Result<Option<(Vec<String>, Vec<String>)>, (&'static str, &'static str)> {
    let mut copies = Vec::new();
    for (position, action) in actions.iter().enumerate() {
        if let OutputAction::Copy(PartRef::Input(index)) = action {
            copies.push((position, *index));
        }
    }
    if copies.len() != parts {
        return Ok(None);
    }
    let Some((first_position, _)) = copies.first().copied() else {
        return Ok(None);
    };
    let Some((last_position, _)) = copies.last().copied() else {
        return Ok(None);
    };
    if !copies
        .iter()
        .enumerate()
        .all(|(expected, (_, actual))| *actual as usize == expected)
    {
        return Ok(None);
    }
    if actions[..first_position]
        .iter()
        .any(|action| !matches!(action, OutputAction::InsertSegments { .. }))
        || actions[last_position + 1..]
            .iter()
            .any(|action| !matches!(action, OutputAction::InsertSegments { .. }))
    {
        return Ok(None);
    }
    for pair in copies.windows(2) {
        if pair[1].0 != pair[0].0 + 1 {
            return Ok(None);
        }
    }
    let prefix = literal_variants(g, active_table, &actions[..first_position])?;
    let suffix = literal_variants(g, active_table, &actions[last_position + 1..])?;
    Ok(Some((prefix, suffix)))
}

/// Parse `C(0) J(0) C(1) ... J(n-2) C(n-1)`, retaining one Cartesian variant set per gap.
fn interior_runs(
    g: &Grammar,
    active_table: TableId,
    actions: &[OutputAction],
    parts: usize,
) -> Result<Option<Vec<Vec<String>>>, (&'static str, &'static str)> {
    let expected_copies = (0..parts as u16).collect::<Vec<_>>();
    let mut runs = Vec::with_capacity(parts.saturating_sub(1));
    let mut cursor = 0usize;
    for (gap, expected) in expected_copies.iter().enumerate() {
        if !matches!(actions.get(cursor), Some(OutputAction::Copy(PartRef::Input(index))) if index == expected) {
            return Ok(None);
        }
        cursor += 1;
        if gap + 1 == parts {
            break;
        }
        let start = cursor;
        while matches!(actions.get(cursor), Some(OutputAction::InsertSegments { .. })) {
            cursor += 1;
        }
        let variants = literal_variants(g, active_table, &actions[start..cursor])?;
        runs.push(if variants == vec![String::new()] { Vec::new() } else { variants });
    }
    if cursor != actions.len() || runs.iter().all(Vec::is_empty) {
        return Ok(None);
    }
    Ok(Some(runs))
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
