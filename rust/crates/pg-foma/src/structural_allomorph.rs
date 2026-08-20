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
use pg_grammar::chardef::{CharDefId, CharDefKind, CharDefTable};
use pg_grammar::model::{
    AffixAllomorphDef, AllomorphId, Grammar, MorphRuleDef, NaturalClassKind, OutputAction, PartRef,
    PatternNode, PhonRuleDef, TableId,
};

use crate::replace::SegAlphabet;

const MARKER_BASE: u32 = 0xF0000;

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
