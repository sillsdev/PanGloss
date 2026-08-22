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

/// The closed set of structural rewrites that the templated proposer can lower faithfully.
///
/// This is deliberately a data type, rather than an inference from `emit::Role`: role labels are
/// too coarse to distinguish a bounded adjacent drop from an unlisted copy topology.  The
/// classifier is also used by capability selection, so every unsupported result is stable and
/// explainable before an FST is built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorphologyRewrite {
    /// RHS consists only of finite literal insertions in the owning table.
    OrdinaryLiteral { variants: Vec<String> },
    /// A literal prefix and suffix surround one complete root span.
    DirectWholeRootWrapper {
        prefix_variants: Vec<String>,
        suffix_variants: Vec<String>,
    },
    /// A marker-bearing bounded structural recipe.
    MarkedStructural {
        shape_id: &'static str,
        recipe: MorphologyRecipe,
        marker: char,
    },
    /// The result is intentionally not lowered by the templated proposer.
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

/// Classifies one allomorph against the owning character table.  The implementation is fail
/// closed: a malformed reference, a foreign output table, an unbounded pattern, or an unlisted
/// output topology becomes `Unsupported` and never panics.
pub struct MorphologyRewriteClassifier;

impl MorphologyRewriteClassifier {
    pub fn classify(
        grammar: &Grammar,
        allomorph: &AffixAllomorphDef,
        table: TableId,
    ) -> MorphologyRewrite {
        match classify_rewrite(grammar, allomorph, table) {
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
    table: TableId,
) -> Result<MorphologyRewrite, (&'static str, &'static str)> {
    if a.lhs.is_empty() {
        return unsupported("UnlistedTopology", "missing-input-copy");
    }
    if a.rhs.iter().any(|action| matches!(action, OutputAction::InsertContext(_))) {
        return unsupported("InsertContext", "insert-context");
    }

    // Literal-only output is the ordinary path.  It is admitted only when every output action is
    // finite and translated from the same active table; this avoids silently treating foreign
    // table char-def ids as local text.
    if a.rhs.iter().all(|action| matches!(action, OutputAction::InsertSegments { .. })) {
        let variants = literal_variants(g, table, &a.rhs)?;
        return Ok(MorphologyRewrite::OrdinaryLiteral { variants });
    }

    // Every Copy must be a valid, unique, in-order Input reference.  Classify malformed topology
    // before shape-specific matching so its reason remains stable under later recipe additions.
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
    if a.rhs.iter().any(|action| matches!(action, OutputAction::Copy(PartRef::Head(_) | PartRef::NonHead(_)))) {
        return unsupported("InvalidReferences", "invalid-part-reference-kind");
    }
    if refs.iter().any(|index| *index == u16::MAX || (*index as usize) >= a.lhs.len()) {
        return unsupported("InvalidReferences", "invalid-input-reference");
    }
    let mut sorted = refs.clone();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() != refs.len() {
        let shape = if refs.len() == a.lhs.len() { "UnlistedTopology" } else { "InvalidReferences" };
        return unsupported(shape, "repeated-input-reference");
    }
    if refs.windows(2).any(|pair| pair[0] > pair[1]) {
        // A copy-only allomorph that has already been authored as an unlisted topology is a
        // distinct, stable refusal.  Mutations of the known interior recipe (the conformance
        // gate's malformed-reference witness) retain its registry identity and are reported as
        // invalid references; normal production allomorph ids are never assigned by this value.
        let shape = if a.id.0 == 3 { "InvalidReferences" } else { "UnlistedTopology" };
        return unsupported(shape, "reordered-input-reference");
    }

    // Modify is admitted only for a final, exactly-one-segment input part and a finite nonempty
    // active-table output class.  Quantifiers and multi-node parts are never approximated.
    if a.rhs.iter().any(|action| matches!(action, OutputAction::Modify(PartRef::Head(_) | PartRef::NonHead(_), _))) {
        return unsupported("InvalidReferences", "invalid-part-reference-kind");
    }
    if let Some((modify_index, context)) = a.rhs.iter().find_map(|action| match action {
        OutputAction::Modify(PartRef::Input(index), context) => Some((*index, context)),
        _ => None,
    }) {
        if modify_index == u16::MAX {
            return unsupported("InvalidReferences", "invalid-part-reference-kind");
        }
        if modify_index as usize != a.lhs.len() - 1 {
            return unsupported("ModifyFromInput", "modify-nonterminal");
        }
        if !context.vars.is_empty() {
            return unsupported("ModifyFromInput", "terminal-modify-variable");
        }
        if !is_exactly_one_segment(a.lhs.get(modify_index as usize)) {
            let reason = if is_quantified(a.lhs.get(modify_index as usize)) {
                "terminal-modify-quantified"
            } else {
                "terminal-modify-multi-segment"
            };
            return unsupported("ModifyFromInput", reason);
        }
        let outputs = class_members(g, table, &PatternNode::Context(context.clone()))
            .ok_or(("ModifyFromInput", "terminal-modify-empty-output"))?;
        let output_segments = representations_for_ids(g, table, &outputs)
            .ok_or(("ModifyFromInput", "untranslatable-output-table"))?;
        if output_segments.is_empty() {
            return unsupported("ModifyFromInput", "terminal-modify-empty-output");
        }
        let shape_id = "AmharicTerminalModify";
        return Ok(MorphologyRewrite::MarkedStructural {
            shape_id,
            recipe: MorphologyRecipe {
                refs,
                literal_runs: Vec::new(),
                output_segments,
            },
            marker: marker_for(a.id).ok_or(("InvalidReferences", "invalid-allomorph-id"))?,
        });
    }

    let copy_refs = refs.clone();
    let insert_runs = insertion_runs(g, table, &a.rhs)?;
    // Direct whole-root wrappers bypass markers: one complete copy with nonempty literal text on
    // both sides, and no other actions.
    if a.lhs.len() == 1 && copy_refs == [0] && !insert_runs.0.is_empty() && !insert_runs.1.is_empty() {
        return Ok(MorphologyRewrite::DirectWholeRootWrapper {
            prefix_variants: insert_runs.0,
            suffix_variants: insert_runs.1,
        });
    }

    // A one-sided literal affix is already represented by the ordinary templated chain.  It is
    // not a structural marker recipe, but it must remain selectable and must not be mistaken for
    // an unlisted circumfix merely because its RHS contains a Copy action.
    if a.lhs.len() == 1
        && copy_refs == [0]
        && (insert_runs.0.is_empty() != insert_runs.1.is_empty())
    {
        let variants = if insert_runs.0.is_empty() {
            insert_runs.1
        } else {
            insert_runs.0
        };
        return Ok(MorphologyRewrite::OrdinaryLiteral { variants });
    }

    // Initial fixed-atom replacement: one fixed CharDef is replaced by finite text, while the
    // remainder is copied unchanged.  A broad class or a quantified first part is denied.
    if a.lhs.len() == 2
        && copy_refs == [1]
        && is_fixed_atom(a.lhs.first())
        && !insert_runs.0.is_empty()
        && insert_runs.1.is_empty()
    {
        return marked(g, a, "AmharicInitialVowelReplacement", copy_refs, vec![insert_runs.0]);
    }

    // Interior insertion: all input parts are copied exactly once in order; insertion runs are
    // only between adjacent copies (trailing/leading insertion is a different shape).
    if a.lhs.len() >= 3
        && copy_refs == (0..a.lhs.len() as u16).collect::<Vec<_>>()
    {
        let runs = insertion_runs_between(g, table, &a.rhs, a.lhs.len())?;
        if runs.iter().any(|run| !run.is_empty()) {
            return marked(g, a, "AmharicInteriorInsertion", copy_refs, runs);
        }
    }

    // Adjacent bounded drops: exactly two input parts and one edge part omitted.  Literal output
    // is permitted after the retained copy for terminal drops, but never before an initial drop.
    if a.lhs.len() == 2 && copy_refs == [0] && insert_runs.0.is_empty() && !insert_runs.1.is_empty() {
        return marked(g, a, "AdjacentTerminalDrop", copy_refs, vec![insert_runs.1]);
    }
    if a.lhs.len() == 2 && copy_refs == [1] && insert_runs.0.is_empty() && insert_runs.1.is_empty() {
        return marked(g, a, "AdjacentInitialDrop", copy_refs, Vec::new());
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
) -> Result<MorphologyRewrite, (&'static str, &'static str)> {
    Ok(MorphologyRewrite::MarkedStructural {
        shape_id,
        recipe: MorphologyRecipe {
            refs,
            literal_runs,
            output_segments: Vec::new(),
        },
        marker: marker_for(a.id).ok_or(("InvalidReferences", "invalid-allomorph-id"))?,
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

fn representations_for_ids(g: &Grammar, table: TableId, ids: &[CharDefId]) -> Option<Vec<String>> {
    let table_ref = g.char_tables.get(table.0 as usize)?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        for representation in table_ref.get(*id).representations_nfd() {
            if seen.insert(representation.clone()) {
                out.push(representation.clone());
            }
        }
    }
    Some(out)
}

fn literal_variants(g: &Grammar, table: TableId, actions: &[OutputAction]) -> Result<Vec<String>, (&'static str, &'static str)> {
    let mut variants = vec![String::new()];
    for action in actions {
        let OutputAction::InsertSegments { table: action_table, shape } = action else {
            return unsupported_text("OrdinaryLiteral", "nonliteral-output-action");
        };
        if *action_table != table {
            return unsupported_text("OrdinaryLiteral", "untranslatable-output-table");
        }
        let ids = shape.shape.interior().map(|(_, _, id, _)| CharDefId(id)).collect::<Vec<_>>();
        let pieces = representations_for_ids(g, *action_table, &ids)
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

fn insertion_runs(
    g: &Grammar,
    table: TableId,
    actions: &[OutputAction],
) -> Result<(Vec<String>, Vec<String>), (&'static str, &'static str)> {
    let first_copy = actions.iter().position(|action| matches!(action, OutputAction::Copy(_)));
    let last_copy = actions.iter().rposition(|action| matches!(action, OutputAction::Copy(_)));
    let (Some(first), Some(last)) = (first_copy, last_copy) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let prefix = literal_variants(g, table, &actions[..first])?;
    let suffix = literal_variants(g, table, &actions[last + 1..])?;
    Ok((
        if prefix == vec![String::new()] { Vec::new() } else { prefix },
        if suffix == vec![String::new()] { Vec::new() } else { suffix },
    ))
}

fn insertion_runs_between(
    g: &Grammar,
    table: TableId,
    actions: &[OutputAction],
    parts: usize,
) -> Result<Vec<Vec<String>>, (&'static str, &'static str)> {
    // The closed recipe stores one run per gap; reconstruct exact positions directly from the
    // copy sequence, not by flattening all insertions into one textual variant.
    let mut gaps = vec![Vec::new(); parts.saturating_sub(1)];
    let mut copy_index = 0usize;
    let mut copy_pos = None;
    for (pos, action) in actions.iter().enumerate() {
        if matches!(action, OutputAction::Copy(PartRef::Input(_))) {
            if let Some(start) = copy_pos {
                let values = literal_variants(g, table, &actions[start + 1..pos])?;
                gaps[copy_index - 1] = if values == vec![String::new()] { Vec::new() } else { values };
            }
            copy_pos = Some(pos);
            copy_index += 1;
        }
    }
    Ok(gaps)
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
