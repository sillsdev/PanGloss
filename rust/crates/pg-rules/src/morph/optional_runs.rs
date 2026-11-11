//! Canonical tilings of runs of identical optional nodes in an analysis affix match; the soundness proof is `docs/divergences/130-optional-run-canonical-tilings.md`.

use std::collections::BTreeSet;

use pg_fst::{Fst, Segment};
use pg_grammar_model::model::{AffixAllomorphDef, Grammar, TableId};
use pg_shape::{EffectiveCdSet, Shape};

use super::AnalysisLhs;
use crate::required_slots::min_segments;

/// The arguments of `Transduce::interchangeable_runs` for one shape and allomorph.
pub(super) struct RunPlan {
    pub(super) run_end: Vec<u32>,
    pub(super) arc_ok: Vec<bool>,
}

/// A plan when the shape holds such a run and the allomorph's group chain is one the proof covers.
pub(super) fn plan(
    g: &Grammar,
    table: TableId,
    (shape, segs, node_of): (&Shape, &[Segment], &[usize]),
    allo: &AffixAllomorphDef,
    lhs: &AnalysisLhs,
    fst: &Fst,
) -> Option<RunPlan> {
    let run_end = run_ends(shape, segs, node_of)?;
    if !lhs.merged.is_empty() || !lhs.modify.is_empty() {
        return None;
    }
    let mut parts_ok = BTreeSet::new();
    for (i, part) in allo.lhs.iter().enumerate() {
        if min_segments(g, table, part) >= 1 {
            parts_ok.insert(i);
        }
    }
    let events = fst.arc_group_events()?;
    let mut any = false;
    let arc_ok: Vec<bool> = events
        .iter()
        .map(|(closes, opens)| {
            let ([closed], [opened]) = (closes.as_slice(), opens.as_slice()) else {
                return false;
            };
            let (Some((part_x, copy_x)), Some((part_y, copy_y))) =
                (parse_group(closed), parse_group(opened))
            else {
                return false;
            };
            let ok = closed != opened
                && parts_ok.contains(&part_x)
                && parts_ok.contains(&part_y)
                && match (copy_x == 0, copy_y == 0) {
                    (true, true) => part_y == part_x + 1,
                    (false, false) => true,
                    _ => false,
                };
            any |= ok;
            ok
        })
        .collect();
    any.then_some(RunPlan { run_end, arc_ok })
}

/// `(part, copy)` of a group named `p{part}_{copy}`.
fn parse_group(name: &str) -> Option<(usize, usize)> {
    let (part, copy) = name.strip_prefix('p')?.split_once('_')?;
    Some((part.parse().ok()?, copy.parse().ok()?))
}

/// Exclusive run end per segment, else 0; `None` when no run has two members.
fn run_ends(shape: &Shape, segs: &[Segment], node_of: &[usize]) -> Option<Vec<u32>> {
    let n = segs.len();
    let same = |i: usize| -> bool {
        let (a, b) = (&segs[i], &segs[i + 1]);
        a.optional
            && b.optional
            && node_of[i + 1] == node_of[i] + 1
            && a.lanes == b.lanes
            && a.explicit_full == b.explicit_full
            && nodes_identical(shape, node_of[i], node_of[i + 1])
    };
    let mut run_end = vec![0u32; n];
    let mut found = false;
    let mut start = 0;
    while start < n {
        let mut end = start + 1;
        while end < n && same(end - 1) {
            end += 1;
        }
        if end - start >= 2 {
            found = true;
            run_end[start..end].fill(end as u32);
        }
        start = end;
    }
    found.then_some(run_end)
}

/// Every per-node value `generate_shape` and `copy_agreement_refuses_match` read is equal.
fn nodes_identical(shape: &Shape, p: usize, q: usize) -> bool {
    let same_set = match (shape.node_cd_set(p), shape.node_cd_set(q)) {
        (EffectiveCdSet::Singleton(a), EffectiveCdSet::Singleton(b)) => a == b,
        (EffectiveCdSet::Unrestricted, EffectiveCdSet::Unrestricted) => true,
        (EffectiveCdSet::Members(a), EffectiveCdSet::Members(b)) => a == b,
        _ => false,
    };
    same_set
        && shape.kind(p) == shape.kind(q)
        && shape.char_def(p) == shape.char_def(q)
        && shape.flags(p) == shape.flags(q)
        && shape.node_lanes(p) == shape.node_lanes(q)
        && shape
            .unresolved_variable_features(p)
            .eq(shape.unresolved_variable_features(q))
        && shape
            .generated_full_features(p)
            .eq(shape.generated_full_features(q))
}

#[cfg(test)]
mod tests {
    use pg_shape::{CdBits, CdSet, NodeFlags, ShapeBuilder};

    use super::*;

    const LANES: [u64; 1] = [0b11];

    /// Two adjacent optional segment nodes; `tweak` changes only the second.
    fn pair(tweak: impl FnOnce(&mut ShapeBuilder)) -> Shape {
        let mut b = ShapeBuilder::with_features(1);
        for _ in 0..2 {
            b.push_segment_with_lanes(7, &LANES);
            b.set_last_flags(NodeFlags::OPTIONAL);
        }
        tweak(&mut b);
        b.finish()
    }

    fn segments(n: usize) -> Vec<Segment> {
        (0..n).map(|_| Segment::optional(LANES.to_vec())).collect()
    }

    fn ends(shape: &Shape, segs: &[Segment]) -> Option<Vec<u32>> {
        let node_of: Vec<usize> = (1..=segs.len()).collect();
        run_ends(shape, segs, &node_of)
    }

    #[test]
    fn equal_adjacent_optional_nodes_form_one_run() {
        let mut b = ShapeBuilder::with_features(1);
        b.push_segment_with_lanes(3, &LANES);
        for _ in 0..3 {
            b.push_segment_with_lanes(7, &LANES);
            b.set_last_flags(NodeFlags::OPTIONAL);
        }
        let shape = b.finish();
        let mut segs = segments(4);
        segs[0].optional = false;
        assert_eq!(ends(&shape, &segs), Some(vec![0, 4, 4, 4]));
    }

    #[test]
    fn a_run_may_start_at_the_first_segment() {
        let shape = pair(|_| {});
        assert_eq!(ends(&shape, &segments(2)), Some(vec![2, 2]));
    }

    #[test]
    fn nodes_differing_in_one_shape_column_are_not_a_run() {
        let differing: Vec<(&str, Shape)> = vec![
            ("char_def", {
                let mut b = ShapeBuilder::with_features(1);
                b.push_segment_with_lanes(7, &LANES);
                b.set_last_flags(NodeFlags::OPTIONAL);
                b.push_segment_with_lanes(8, &LANES);
                b.set_last_flags(NodeFlags::OPTIONAL);
                b.finish()
            }),
            ("lanes", {
                let mut b = ShapeBuilder::with_features(1);
                b.push_segment_with_lanes(7, &LANES);
                b.set_last_flags(NodeFlags::OPTIONAL);
                b.push_segment_with_lanes(7, &[0b01]);
                b.set_last_flags(NodeFlags::OPTIONAL);
                b.finish()
            }),
            ("kind", {
                let mut b = ShapeBuilder::with_features(1);
                b.push_segment_with_lanes(7, &LANES);
                b.set_last_flags(NodeFlags::OPTIONAL);
                b.push_boundary_with_lanes(7, &LANES);
                b.finish()
            }),
            ("flags", {
                let mut b = ShapeBuilder::with_features(1);
                b.push_segment_with_lanes(7, &LANES);
                b.set_last_flags(NodeFlags::OPTIONAL);
                b.push_segment_with_lanes(7, &LANES);
                b.finish()
            }),
            ("cd_set", {
                let mut b = ShapeBuilder::with_features(1);
                b.push_segment_with_lanes_and_set(&LANES, CdSet::Unrestricted);
                b.set_last_flags(NodeFlags::OPTIONAL);
                b.push_segment_with_lanes_and_set(&LANES, CdSet::Members(CdBits::from_ids([1])));
                b.set_last_flags(NodeFlags::OPTIONAL);
                b.finish()
            }),
            ("generated_full", pair(|b| b.set_generated_full(2, &[0]))),
            (
                "unresolved_variables",
                pair(|b| b.set_unresolved_variables(2, &[0])),
            ),
        ];
        for (column, shape) in differing {
            assert_eq!(ends(&shape, &segments(2)), None, "{column}");
        }
    }

    #[test]
    fn segments_differing_in_one_matcher_column_are_not_a_run() {
        let shape = pair(|_| {});
        let mut lanes = segments(2);
        lanes[1].lanes = vec![0b01];
        let mut full = segments(2);
        full[1].explicit_full = vec![0];
        let mut required = segments(2);
        required[1].optional = false;
        for (column, segs) in [
            ("lanes", lanes),
            ("explicit_full", full),
            ("optional", required),
        ] {
            assert_eq!(ends(&shape, &segs), None, "{column}");
        }
    }

    #[test]
    fn nodes_not_adjacent_in_the_shape_are_not_a_run() {
        let shape = pair(|_| {});
        assert_eq!(run_ends(&shape, &segments(2), &[1, 3]), None);
    }

    #[test]
    fn parse_group_reads_part_and_copy() {
        assert_eq!(parse_group("p3_0"), Some((3, 0)));
        assert_eq!(parse_group("p12_2"), Some((12, 2)));
        assert_eq!(parse_group("g1"), None);
    }
}
