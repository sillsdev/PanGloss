//! Bounded local structural-allomorph lowering for the templated proposer.
//!
//! This deliberately covers one affine, adjacent suffix shape without enumerating roots:
//! `lhs = [variable prefix, one tail atom]`, `rhs = [Copy(Input(0)), InsertSegments...]`.
//! The templated lexc emitter writes an allomorph-owned marker alternative; this module compiles
//! `tail marker -> inserted tokens` and the caller composes it after lexc and before phonology.
//! Unsupported shapes receive no marker and remain on the existing literal fallback path.

use foma::constructions::{fsm_compose, fsm_union, fsm_universal};
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;
use pg_grammar::chardef::{CharDefId, CharDefKind};
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
}

pub(crate) fn marker_for(allomorph: AllomorphId) -> Option<char> {
    char::from_u32(MARKER_BASE.checked_add(allomorph.0)?)
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

fn recipe_for(g: &Grammar, allomorph: &AffixAllomorphDef) -> Option<LocalRecipe> {
    if allomorph.lhs.len() != 2 {
        return None;
    }
    let tail_node = allomorph.lhs[1].nodes.as_slice();
    let [tail_node] = tail_node else { return None };
    let [OutputAction::Copy(PartRef::Input(0)), rest @ ..] = allomorph.rhs.as_slice() else {
        return None;
    };
    if rest.is_empty() {
        return None;
    }
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
    let table = table?;
    let alphabet = SegAlphabet::new(g.char_tables.get(table.0 as usize)?);
    let tail_members = class_members(g, table, tail_node)?;
    let inserted = inserted_shapes
        .into_iter()
        .map(|shape| alphabet.encode_shape(&shape.shape))
        .collect();
    Some(LocalRecipe {
        allomorph: allomorph.id,
        table,
        tail_members,
        inserted,
    })
}

pub(crate) fn structural_marker(g: &Grammar, allomorph: &AffixAllomorphDef) -> Option<char> {
    recipe_for(g, allomorph).and_then(|recipe| marker_for(recipe.allomorph))
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

/// A compact grammar-owned floating-marker heuristic: modifier letters and the degree-style
/// placeholder are technical realization symbols when they occur as singleton rewrite inputs.
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
                    recipe_for(g, allomorph).is_some()
                );
            }
            let Some(recipe) = recipe_for(g, allomorph) else {
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
            let regex = format!(
                "{} {} -> {}",
                atom(&tails),
                marker,
                spaced(&recipe.inserted)
            );
            let recipe_net = fsm_parse_regex(opts, &regex, None, None)
                .unwrap_or_else(|| panic!("foma rejected structural allomorph regex {regex:?}"));
            net = Some(match net {
                None => recipe_net,
                Some(previous) => fsm_compose(opts, previous, recipe_net),
            });
        }
    }
    net
}
