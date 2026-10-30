//! A mandatory word edge that cannot unify with the pattern's mandatory edge rules out an anchored analysis match without running the matcher.

use pg_featstruct::flat_unifiable;
use pg_fst::{CompileNode, Segment};

use super::AnalysisLhs;

/// The constraint every match must meet at the first (last) pattern position, or `None` when the edge can match empty or branch.
fn edge_constraint(nodes: &[CompileNode], first: bool) -> Option<&[u64]> {
    let is_candidate = |node: &&CompileNode| !matches!(node, CompileNode::Group { children, .. } if children.is_empty());
    let edge = if first {
        nodes.iter().find(is_candidate)
    } else {
        nodes.iter().rev().find(is_candidate)
    };
    edge.and_then(|node| match node {
        CompileNode::Constraint(lanes) => Some(lanes.as_slice()),
        CompileNode::Group { children, .. } => edge_constraint(children, first),
        CompileNode::Quantifier { min, children, .. } if *min >= 1 => {
            edge_constraint(children, first)
        }
        CompileNode::Quantifier { .. } | CompileNode::Alternation(_) => None,
    })
}

fn edge_passes(constraint: Option<&[u64]>, edge: Option<&Segment>) -> bool {
    match (constraint, edge) {
        (Some(lanes), Some(seg)) if !seg.optional => flat_unifiable(&seg.lanes, lanes),
        _ => true,
    }
}

impl AnalysisLhs {
    pub(crate) fn edge_rejects_at(&self, edge: Option<&Segment>, first: bool) -> bool {
        !edge_passes(edge_constraint(&self.nodes, first), edge)
    }

    pub(crate) fn inserted_edge_rejects_at(&self, edge: &Segment, first: bool) -> bool {
        let node = if first {
            self.nodes.first()
        } else {
            self.nodes.last()
        };
        matches!(node, Some(CompileNode::Constraint(_))) && self.edge_rejects_at(Some(edge), first)
    }

    /// True when the matcher provably finds no match because a mandatory word edge cannot unify with the pattern's mandatory edge.
    pub(crate) fn edge_rejects(&self, segs: &[Segment]) -> bool {
        self.edge_rejects_at(segs.first(), true) || self.edge_rejects_at(segs.last(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHashMap as HashMap;

    const A: u64 = 0b01;
    const B: u64 = 0b10;

    fn lhs(nodes: Vec<CompileNode>) -> AnalysisLhs {
        AnalysisLhs {
            nodes,
            captured: HashMap::default(),
            modify: HashMap::default(),
        }
    }
    fn c(bits: u64) -> CompileNode {
        CompileNode::Constraint(vec![bits])
    }
    fn group(children: Vec<CompileNode>) -> CompileNode {
        CompileNode::Group {
            name: "p0".into(),
            children,
        }
    }
    /// A copied part that can match empty, then a mandatory suffix segment.
    fn suffix(bits: u64) -> AnalysisLhs {
        let any = CompileNode::Quantifier {
            min: 0,
            max: None,
            children: vec![c(u64::MAX)],
        };
        lhs(vec![group(vec![any]), c(bits)])
    }
    fn word(bits: &[u64]) -> Vec<Segment> {
        bits.iter().map(|b| Segment::new(vec![*b])).collect()
    }

    #[test]
    fn a_mandatory_edge_that_cannot_unify_rejects() {
        let suffix = suffix(B);
        assert!(suffix.edge_rejects(&word(&[A, A])));
        assert!(!suffix.edge_rejects(&word(&[A, B])));
        let prefix = lhs(vec![c(B), group(vec![c(A)])]);
        assert!(prefix.edge_rejects(&word(&[A, B])));
        assert!(!prefix.edge_rejects(&word(&[B, A])));
    }

    #[test]
    fn an_optional_word_edge_never_rejects() {
        let suffix = suffix(B);
        let mut segs = word(&[A, A]);
        segs[1].optional = true;
        assert!(!suffix.edge_rejects(&segs));
    }

    #[test]
    fn an_optional_or_branching_pattern_edge_never_rejects() {
        let optional_group = CompileNode::Quantifier {
            min: 0,
            max: Some(1),
            children: vec![c(B)],
        };
        assert!(!lhs(vec![optional_group, c(A)]).edge_rejects(&word(&[A, A])));
        assert!(!lhs(vec![
            c(A),
            CompileNode::Quantifier {
                min: 0,
                max: None,
                children: vec![c(B)]
            }
        ])
        .edge_rejects(&word(&[A, A])));
        let alternation = CompileNode::Alternation(vec![vec![c(B)], vec![c(A)]]);
        assert!(!lhs(vec![alternation]).edge_rejects(&word(&[A])));
    }

    #[test]
    fn a_required_repetition_still_decides_the_edge() {
        let plus = |bits| CompileNode::Quantifier {
            min: 1,
            max: None,
            children: vec![c(bits)],
        };
        assert!(lhs(vec![group(vec![plus(B)])]).edge_rejects(&word(&[A, B])));
        assert!(!lhs(vec![group(vec![plus(B)])]).edge_rejects(&word(&[B, B])));
    }

    #[test]
    fn a_variable_governed_edge_lane_is_all_ones_and_passes() {
        let alpha_edge = lhs(vec![c(u64::MAX), c(A)]);
        assert!(!alpha_edge.edge_rejects(&word(&[B, A])));
    }

    #[test]
    fn an_empty_word_or_pattern_never_rejects() {
        assert!(!lhs(vec![c(A)]).edge_rejects(&[]));
        assert!(!lhs(vec![]).edge_rejects(&word(&[A])));
    }
}
