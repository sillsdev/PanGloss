//! Block relation: the proposing fallback for focus shapes the per-site construction refuses.

use std::collections::{BTreeMap, HashSet};

use foma::{options::FomaOptions, regex::fsm_parse_regex, types::Fsm};
use pg_grammar::{
    chardef::{CharDefId, CharDefKind},
    featsys::TYPE_SEGMENT_BITS,
    model::{Grammar, PatternNode, RewriteMode, RewriteRuleDef, TableId},
};
use pg_rules::rewrite::{focus_outputs, BlockNode};

use super::IntermediateBundles;
use crate::replace::{
    has_quantified_target, is_fully_supported_shape, PhonologyLoweringError, SegAlphabet,
};

/// Resource containment: focus tuples enumerated per rule before the any-value widening is used.
pub(crate) const MAX_TUPLES: usize = 65536;

/// One focus occurrence and what synthesis leaves in its place; `None` is any boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ExactBlock {
    focus: Vec<Option<CharDefId>>,
    output: Vec<CharDefId>,
}

/// How a block rule's rows and relation are produced.
#[derive(Debug)]
pub(crate) enum BlockPlan {
    /// Every focus tuple was run through synthesis; `tuples` is how many were tried.
    Exact {
        blocks: Vec<ExactBlock>,
        tuples: usize,
    },
    /// Too many tuples: positions are independent and alpha output lanes are any value.
    Widened,
}

/// Whether `rule` has a multi-token iterative focus or a one-token simultaneous focus the site construction does not take.
pub(crate) fn is_block_rule(g: &Grammar, rule: &RewriteRuleDef) -> bool {
    let simple = |nodes: &[PatternNode]| {
        nodes
            .iter()
            .all(|node| matches!(node, PatternNode::CharDef(_) | PatternNode::Context(_)))
    };
    let focus_shape = match rule.mode {
        RewriteMode::Iterative => rule.lhs.nodes.len() >= 2,
        RewriteMode::Simultaneous => {
            rule.lhs.nodes.len() == 1 && !is_fully_supported_shape(g, rule)
        }
    };
    focus_shape
        && !rule.subrules.is_empty()
        && !has_quantified_target(rule)
        && simple(&rule.lhs.nodes)
        && rule
            .subrules
            .iter()
            .all(|subrule| simple(&subrule.rhs.nodes))
}

/// Interns the rows `rule`'s focus tuples produce, to a fixpoint; `Widened` leaves the generic update to the caller.
pub(super) fn close_rule(
    bundles: &mut IntermediateBundles,
    g: &Grammar,
    table_id: TableId,
    rule: &RewriteRuleDef,
) -> Result<BlockPlan, PhonologyLoweringError> {
    // Without features a row cannot tell segments apart, so tuples carry no exact outputs.
    if g.phon_features.is_empty() {
        return Ok(BlockPlan::Widened);
    }
    let table = &g.char_tables[table_id.0 as usize];
    let type_lane = g.phon_features.type_flat().0 as usize;
    let boundaries = bundles.boundary_ids(g);
    let (mut blocks, mut seen) = (Vec::new(), HashSet::new());
    let (mut done, mut tuples) = (0, 0);
    loop {
        let limit = bundles.rows.len();
        let slots: Vec<Vec<Option<usize>>> = rule
            .lhs
            .nodes
            .iter()
            .map(|node| match node {
                PatternNode::CharDef(id) if table.get(*id).kind() == CharDefKind::Boundary => {
                    boundaries.first().map(|_| None).into_iter().collect()
                }
                _ => bundles
                    .member_rows(g, table, node, limit)
                    .into_iter()
                    .map(Some)
                    .collect(),
            })
            .collect();
        let product = slots
            .iter()
            .try_fold(1usize, |product, slot| product.checked_mul(slot.len()));
        if product.is_none_or(|product| product > MAX_TUPLES) {
            return Ok(BlockPlan::Widened);
        }
        let mut choice = vec![0usize; slots.len()];
        while !slots.iter().any(Vec::is_empty) {
            let tuple: Vec<Option<usize>> =
                choice.iter().zip(&slots).map(|(&i, slot)| slot[i]).collect();
            if tuple.iter().any(|row| row.is_some_and(|row| row >= done)) || done == 0 {
                tuples += 1;
                let focus: Vec<BlockNode> = tuple
                    .iter()
                    .map(|row| match row {
                        Some(row) => {
                            let (id, lanes) = &bundles.rows[*row];
                            let attested = (id.0 as usize) < table.len();
                            BlockNode::Segment {
                                lanes: lanes.clone(),
                                explicit_full: if attested {
                                    table.get(*id).explicit_full_features().to_vec()
                                } else {
                                    Vec::new()
                                },
                            }
                        }
                        None => BlockNode::Boundary(boundaries[0]),
                    })
                    .collect();
                for subrule in 0..rule.subrules.len() {
                    let Some(out) = focus_outputs(g, table_id, rule, subrule, &focus) else {
                        continue;
                    };
                    let mut output = Vec::with_capacity(out.len());
                    for node in out {
                        output.push(match node {
                            BlockNode::Boundary(id) => id,
                            BlockNode::Segment { mut lanes, .. } => {
                                lanes[type_lane] = TYPE_SEGMENT_BITS;
                                bundles.intern(lanes, &rule.xml_id)?
                            }
                        });
                    }
                    let focus = tuple
                        .iter()
                        .map(|row| row.map(|row| bundles.rows[row].0))
                        .collect();
                    let block = ExactBlock { focus, output };
                    if seen.insert(block.clone()) {
                        blocks.push(block);
                    }
                }
            }
            let Some(position) = (0..choice.len())
                .rev()
                .find(|&position| choice[position] + 1 < slots[position].len())
            else {
                break;
            };
            choice[position] += 1;
            choice[position + 1..].fill(0);
        }
        done = limit;
        if bundles.rows.len() == limit {
            return Ok(BlockPlan::Exact { blocks, tuples });
        }
    }
}

/// The relation `[ ? | block ]*`, and how many blocks it holds.
pub(crate) fn compile_block(
    opts: &FomaOptions,
    g: &Grammar,
    bundles: &IntermediateBundles,
    rule: &RewriteRuleDef,
) -> Result<(Fsm, usize), PhonologyLoweringError> {
    let unsupported = || PhonologyLoweringError::UnsupportedRewrite(rule.xml_id.clone());
    let table = &g.char_tables[bundles.table.0 as usize];
    let alphabet = SegAlphabet::new(table);
    let union = |ids: &[CharDefId]| {
        let tokens: Vec<String> = ids.iter().map(|id| alphabet.token(*id).to_string()).collect();
        format!("[{}]", tokens.join(" | "))
    };
    let boundaries = bundles.boundary_ids(g);
    // Boundaries are transparent between consecutive focus nodes.
    let skip = if boundaries.is_empty() {
        None
    } else {
        Some(format!("{}*", union(&boundaries)))
    };
    let plan = bundles.blocks.get(&rule.xml_id).ok_or_else(unsupported)?;
    let (alternatives, count) = match plan {
        BlockPlan::Exact { blocks, .. } => {
            let entries: Vec<Vec<String>> = blocks
                .iter()
                .map(|block| {
                    exact_entry(block, &alphabet, &boundaries, skip.as_deref(), &union)
                })
                .collect();
            let refs: Vec<&[String]> = entries.iter().map(Vec::as_slice).collect();
            (
                if refs.is_empty() { Vec::new() } else { vec![trie(&refs)] },
                blocks.len(),
            )
        }
        BlockPlan::Widened => {
            let alternatives = widened_alternatives(g, bundles, rule, skip.as_deref(), &union);
            let count = alternatives.len();
            (alternatives, count)
        }
    };
    let mut parts = vec!["?".to_owned()];
    parts.extend(alternatives);
    let net = fsm_parse_regex(opts, &format!("[{}]*", parts.join(" | ")), None, None)
        .ok_or_else(unsupported)?;
    Ok((net, count))
}

fn exact_entry(
    block: &ExactBlock,
    alphabet: &SegAlphabet,
    boundaries: &[CharDefId],
    skip: Option<&str>,
    union: &dyn Fn(&[CharDefId]) -> String,
) -> Vec<String> {
    let token = |id: CharDefId| alphabet.token(id);
    let in_place = block.focus.len() == block.output.len();
    // The first replacement node rides on the last target's arc so equal tails share one state.
    let carried = |position: usize| {
        if in_place {
            block.output.get(position)
        } else if position + 1 == block.focus.len() {
            block.output.first()
        } else {
            None
        }
    };
    let mut elements = Vec::new();
    for (position, focus) in block.focus.iter().enumerate() {
        if let (true, Some(skip)) = (position > 0, skip) {
            elements.push(skip.to_owned());
        }
        elements.push(match (focus, carried(position)) {
            (Some(id), Some(out)) => format!("{}:{}", token(*id), token(*out)),
            (Some(id), None) => format!("{}:0", token(*id)),
            (None, Some(out)) => format!("[{} .x. {}]", union(boundaries), token(*out)),
            (None, None) => format!("[{} .x. 0]", union(boundaries)),
        });
    }
    if !in_place {
        elements.extend(block.output.iter().skip(1).map(|id| format!("0:{}", token(*id))));
    }
    elements
}

/// Prefix-shared alternation of element sequences.
fn trie(entries: &[&[String]]) -> String {
    let mut groups: BTreeMap<&str, Vec<&[String]>> = BTreeMap::new();
    let mut ends = false;
    for entry in entries {
        match entry.split_first() {
            Some((head, tail)) => groups.entry(head).or_default().push(tail),
            None => ends = true,
        }
    }
    let mut alternatives: Vec<String> = groups
        .into_iter()
        .map(|(head, tails)| {
            if tails.iter().all(|tail| tail.is_empty()) {
                head.to_owned()
            } else {
                format!("{head} {}", trie(&tails))
            }
        })
        .collect();
    if ends {
        alternatives.push("0".to_owned());
    }
    format!("[{}]", alternatives.join(" | "))
}

fn widened_alternatives(
    g: &Grammar,
    bundles: &IntermediateBundles,
    rule: &RewriteRuleDef,
    skip: Option<&str>,
    union: &dyn Fn(&[CharDefId]) -> String,
) -> Vec<String> {
    let table = &g.char_tables[bundles.table.0 as usize];
    let members = |nodes: &[PatternNode]| -> Option<Vec<Vec<CharDefId>>> {
        let slots: Option<Vec<_>> = nodes
            .iter()
            .map(|node| bundles.node_members(g, node))
            .collect();
        slots.filter(|slots| slots.iter().all(|slot| !slot.is_empty()))
    };
    let is_boundary = |node: &PatternNode| {
        matches!(node, PatternNode::CharDef(id) if table.get(*id).kind() == CharDefKind::Boundary)
    };
    let Some(focus) = members(&rule.lhs.nodes) else {
        return Vec::new();
    };
    let mut alternatives = Vec::new();
    for subrule in &rule.subrules {
        let in_place = rule.lhs.nodes.len() == subrule.rhs.nodes.len();
        if in_place && rule.lhs.nodes.iter().any(is_boundary) {
            continue;
        }
        let Some(output) = members(&subrule.rhs.nodes) else {
            continue;
        };
        let mut elements = Vec::new();
        for (position, slot) in focus.iter().enumerate() {
            if let (true, Some(skip)) = (position > 0, skip) {
                elements.push(skip.to_owned());
            }
            let carried = if in_place {
                output.get(position)
            } else if position + 1 == focus.len() {
                output.first()
            } else {
                None
            };
            elements.push(match carried {
                Some(out) => format!("[{} .x. {}]", union(slot), union(out)),
                None => format!("[{} .x. 0]", union(slot)),
            });
        }
        if !in_place {
            elements.extend(
                output
                    .iter()
                    .skip(1)
                    .map(|slot| format!("[0 .x. {}]", union(slot))),
            );
        }
        alternatives.push(format!("[{}]", elements.join(" ")));
    }
    alternatives
}
