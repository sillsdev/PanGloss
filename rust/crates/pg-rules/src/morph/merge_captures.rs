//! Folds adjacent single-use copies whose empty captures restore no material; internal split points cannot affect the emitted stem.

use super::{OutputAction, PartRef};
use crate::required_slots::{min_segments, restored_segments};
use pg_grammar_model::model::{Grammar, Pattern, TableId};

/// `len` consecutive right-hand actions starting at `start`, to be captured by one group.
pub(super) struct Run {
    pub(super) start: usize,
    pub(super) len: usize,
}

pub(super) fn runs(
    g: &Grammar,
    table: TableId,
    lhs_parts: &[(String, &Pattern)],
    rhs: &[OutputAction],
) -> Vec<Run> {
    let mut uses = vec![0usize; lhs_parts.len()];
    for action in rhs {
        if let OutputAction::Copy(PartRef::Input(i)) | OutputAction::Modify(PartRef::Input(i), _) =
            action
        {
            if let Some(n) = uses.get_mut(*i as usize) {
                *n += 1;
            }
        }
    }
    let foldable = |action: &OutputAction| -> Option<u16> {
        let OutputAction::Copy(PartRef::Input(i)) = action else {
            return None;
        };
        let part = lhs_parts.get(*i as usize)?.1;
        let matches_empty = min_segments(g, table, part) == 0;
        let safe = !matches_empty || restored_segments(g, table, &part.nodes) == 0;
        (uses[*i as usize] == 1 && safe).then_some(*i)
    };
    let mut runs = Vec::new();
    let mut at = 0;
    while at < rhs.len() {
        let Some(first) = foldable(&rhs[at]) else {
            at += 1;
            continue;
        };
        let mut len = 1;
        while rhs
            .get(at + len)
            .and_then(foldable)
            .is_some_and(|i| i as usize == first as usize + len)
        {
            len += 1;
        }
        if len > 1 {
            runs.push(Run { start: at, len });
        }
        at += len;
    }
    runs
}

#[cfg(test)]
#[path = "../../tests/common/mod.rs"]
mod test_common;

#[cfg(test)]
mod tests {
    use super::test_common as common;
    use super::*;
    use pg_grammar_model::model::{NatClassId, PatternNode, SimpleContext};

    fn ctx() -> PatternNode {
        PatternNode::Context(SimpleContext {
            nat_class: NatClassId(0),
            vars: Vec::new(),
        })
    }

    fn quantified(min: u32, max: Option<u32>) -> Pattern {
        Pattern {
            nodes: vec![PatternNode::Quantifier {
                min,
                max,
                children: vec![ctx()],
            }],
        }
    }

    fn copy(i: u16) -> OutputAction {
        OutputAction::Copy(PartRef::Input(i))
    }

    /// `(start, len)` of every folded run for `patterns` copied by `rhs`.
    fn folded(patterns: &[Pattern], rhs: &[OutputAction]) -> Vec<(usize, usize)> {
        let g = common::load_probe_grammar();
        let parts: Vec<(String, &Pattern)> = patterns
            .iter()
            .enumerate()
            .map(|(i, p)| (format!("part{i}"), p))
            .collect();
        runs(&g, TableId(0), &parts, rhs)
            .iter()
            .map(|r| (r.start, r.len))
            .collect()
    }

    #[test]
    fn consecutive_single_use_copies_fold() {
        let p = [quantified(0, None), quantified(1, None)];
        assert_eq!(folded(&p, &[copy(0), copy(1)]), vec![(0, 2)]);
    }

    #[test]
    fn a_bounded_nullable_part_restores_segments_and_never_folds() {
        let p = [quantified(0, Some(2)), quantified(0, None)];
        assert!(folded(&p, &[copy(0), copy(1)]).is_empty());
        assert!(folded(&p, &[copy(1), copy(0)]).is_empty());
    }

    #[test]
    fn a_part_used_twice_never_folds() {
        let p = [quantified(1, None), quantified(1, None)];
        assert!(folded(&p, &[copy(0), copy(1), copy(1)]).is_empty());
        assert!(folded(&p, &[copy(0), copy(0), copy(1)]).is_empty());
    }

    #[test]
    fn a_non_copy_action_between_copies_splits_the_run() {
        let p = [
            quantified(1, None),
            quantified(1, None),
            quantified(1, None),
        ];
        let modify = OutputAction::Modify(
            PartRef::Input(1),
            SimpleContext {
                nat_class: NatClassId(0),
                vars: Vec::new(),
            },
        );
        assert!(folded(&p, &[copy(0), modify.clone(), copy(2)]).is_empty());
        // A part that is modified is not a plain copy, so it is not folded even beside copies.
        assert!(folded(&p, &[copy(0), copy(2), modify]).is_empty());
    }

    #[test]
    fn copies_out_of_input_order_or_with_a_gap_never_fold() {
        let p = [
            quantified(1, None),
            quantified(1, None),
            quantified(1, None),
        ];
        assert!(folded(&p, &[copy(1), copy(0)]).is_empty());
        assert!(folded(&p, &[copy(0), copy(2)]).is_empty());
        // The consecutive prefix folds and the out-of-order tail is left alone.
        assert_eq!(folded(&p, &[copy(0), copy(1), copy(2)]), vec![(0, 3)]);
    }

    #[test]
    fn head_and_non_head_copies_never_fold() {
        let p = [quantified(1, None), quantified(1, None)];
        let rhs = [
            OutputAction::Copy(PartRef::Head(0)),
            OutputAction::Copy(PartRef::Head(1)),
        ];
        assert!(folded(&p, &rhs).is_empty());
    }
}
