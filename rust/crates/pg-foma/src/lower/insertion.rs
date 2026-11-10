//! Iterative empty-focus insertion: only the first insertion into a gap keeps the rule's environment, the rest are any replacement row.

use std::collections::HashMap;

use foma::{
    constructions::{fsm_compose, fsm_ignore},
    options::FomaOptions,
    regex::fsm_parse_regex,
    types::{Fsm, OP_IGNORE_ALL},
};
use pg_grammar::{
    chardef::CharDefId,
    model::{Grammar, RewriteRuleDef},
};

use super::{render_slots, AlphaAssignment, IntermediateBundles, Slot};
use crate::replace::SegAlphabet;

const CLOSE: &str = "\"@PG_INSERTION_CLOSE@\"";

fn open(subrule: usize) -> String {
    format!("\"@PG_INSERTION_OPEN_{subrule}@\"")
}

/// One subrule's insertion and environments, rendered; an empty environment text means none.
pub(crate) struct FedBranch {
    insert: String,
    left: String,
    right: String,
}

/// Every row any subrule's replacement can place; `None` when a replacement slot is not a plain row union.
pub(crate) fn replacement_rows(
    g: &Grammar,
    bundles: &IntermediateBundles,
    rule: &RewriteRuleDef,
) -> Option<Vec<CharDefId>> {
    let mut rows: Vec<CharDefId> = Vec::new();
    for subrule in &rule.subrules {
        for slot in bundles.slots(g, &subrule.rhs)? {
            let Slot::Union(members) = slot else {
                return None;
            };
            rows.extend(members);
        }
    }
    rows.sort_by_key(|id| id.0);
    rows.dedup();
    Some(rows)
}

fn render_union(alphabet: &SegAlphabet, rows: &[CharDefId]) -> String {
    let none = AlphaAssignment {
        values: HashMap::new(),
    };
    render_slots(alphabet, &[Slot::Union(rows.to_vec())], &none)
}

/// A subrule's replacement with any replacement row allowed after every node; `None` when it inserts nothing.
pub(crate) fn render_branch(
    alphabet: &SegAlphabet,
    rows: &[CharDefId],
    rhs: &[Slot],
    left: &[Slot],
    right: &[Slot],
    asg: &AlphaAssignment,
) -> Option<FedBranch> {
    if rhs.is_empty() {
        return None;
    }
    let any = Slot::Repeat {
        min: 0,
        max: None,
        children: vec![Slot::Union(rows.to_vec())],
    };
    let fed: Vec<Slot> = rhs.iter().flat_map(|node| [node.clone(), any.clone()]).collect();
    Some(FedBranch {
        insert: render_slots(alphabet, &fed, asg),
        left: render_slots(alphabet, left, asg),
        right: render_slots(alphabet, right, asg),
    })
}

/// Identity plus a marked block at any gap, kept only where its subrule's environment holds at the block's first node.
pub(crate) fn compile_fed_insertion(
    opts: &FomaOptions,
    alphabet: &SegAlphabet,
    rows: &[CharDefId],
    branches: &[Option<FedBranch>],
    boundaries: Option<Fsm>,
) -> Option<Fsm> {
    let live: Vec<(usize, &FedBranch)> = branches
        .iter()
        .enumerate()
        .filter_map(|(index, branch)| Some((index, branch.as_ref()?)))
        .collect();
    // Synthesis re-inserts at or after its first inserted node, so later insertions land in the block as replacement rows.
    if live.is_empty() {
        return fsm_parse_regex(opts, "[?]*", None, None);
    }
    let rows = render_union(alphabet, rows);
    let opens: Vec<String> = live.iter().map(|(index, _)| open(*index)).collect();
    let any_open = format!("[{}]", opens.join(" | "));
    let marks = format!("[{any_open} | {CLOSE}]");
    // The left environment sees the output so far; the right one sees the input, so every other block is skipped.
    let block = format!("[{any_open} [{rows}]* {CLOSE}]");
    let blocks: Vec<String> = live
        .iter()
        .map(|(index, branch)| {
            format!("[0 .x. [{} {} {CLOSE}]]", open(*index), branch.insert)
        })
        .collect();
    let expand = fsm_parse_regex(opts, &format!("[? | {}]*", blocks.join(" | ")), None, None)?;
    let mut rules: Vec<String> = live
        .iter()
        .map(|(index, branch)| {
            let left = (!branch.left.is_empty()).then(|| format!("[{} / {marks}]", branch.left));
            // The block's own remaining nodes are not yet there when its first node is placed.
            let right = (!branch.right.is_empty())
                .then(|| format!("[[{rows}]* {CLOSE} [{} / {block}]]", branch.right));
            match (left, right) {
                (Some(left), Some(right)) => format!("{} -> 0 || {left} _ {right}", open(*index)),
                (Some(left), None) => format!("{} -> 0 || {left} _", open(*index)),
                (None, Some(right)) => format!("{} -> 0 || _ {right}", open(*index)),
                (None, None) => format!("{} -> 0", open(*index)),
            }
        })
        .collect();
    rules.push(format!("{CLOSE} -> 0"));
    let gate = fsm_parse_regex(opts, &rules.join(" ,, "), None, None)?;
    let gate = match boundaries {
        Some(boundaries) => fsm_ignore(opts, gate, boundaries, OP_IGNORE_ALL),
        None => gate,
    };
    // A block whose first node found no environment keeps its mark and has no unmarked image.
    let unmark = fsm_parse_regex(opts, &format!("[? - {marks}]*"), None, None)?;
    Some(fsm_compose(opts, fsm_compose(opts, expand, gate), unmark))
}
