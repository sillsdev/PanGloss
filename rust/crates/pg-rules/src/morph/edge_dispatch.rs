//! A compile-time, per-rule dispatch index over the mandatory word edges of an affix rule's allomorphs, consulted once before the allomorph loop.

use pg_fst::Segment;

use super::edge_prefilter::edge_passes;
use super::AnalysisLhs;

/// Constraint ids 1..=63 are real; id 0 is the wildcard (optional, branching or unknown edge) and always passes.
const MAX_DISTINCT: usize = 63;

/// Per-allomorph (first, last) edge constraint ids, plus the distinct constraints they name.
pub(crate) struct EdgeDispatch {
    first: Box<[Box<[u64]>]>,
    last: Box<[Box<[u64]>]>,
    keys: Box<[(u8, u8)]>,
}

/// Which distinct edge constraints the word's edges satisfy: bit `k` for constraint id `k`, bit 0 always set.
pub(crate) struct Admission<'a> {
    index: &'a EdgeDispatch,
    first: u64,
    last: u64,
}

fn intern(table: &mut Vec<Vec<u64>>, constraint: Option<&[u64]>) -> u8 {
    let Some(lanes) = constraint else { return 0 };
    if let Some(at) = table.iter().position(|known| known == lanes) {
        return at as u8 + 1;
    }
    if table.len() >= MAX_DISTINCT {
        return 0;
    }
    table.push(lanes.to_vec());
    table.len() as u8
}

impl EdgeDispatch {
    /// One entry per allomorph, in rule order; an allomorph with no compiled analysis LHS is a wildcard.
    pub(crate) fn build<'a>(lhss: impl Iterator<Item = Option<&'a AnalysisLhs>>) -> Self {
        let (mut first, mut last) = (Vec::new(), Vec::new());
        let keys = lhss
            .map(|lhs| match lhs.map(AnalysisLhs::edge_constraints) {
                Some((f, l)) => (intern(&mut first, f), intern(&mut last, l)),
                None => (0, 0),
            })
            .collect();
        let boxed = |table: Vec<Vec<u64>>| table.into_iter().map(Vec::into_boxed_slice).collect();
        EdgeDispatch {
            first: boxed(first),
            last: boxed(last),
            keys,
        }
    }

    /// True when no allomorph is constrained at either edge, so consulting the index can never skip one.
    pub(crate) fn is_all_wildcard(&self) -> bool {
        self.first.is_empty() && self.last.is_empty()
    }

    /// Evaluates each distinct constraint once against the word's edges.
    pub(crate) fn admit(&self, segs: &[Segment]) -> Admission<'_> {
        self.admit_edges(segs.first(), segs.last())
    }

    /// `admit` for a caller that holds only the word's first and last segment.
    pub(crate) fn admit_edges(
        &self,
        first: Option<&Segment>,
        last: Option<&Segment>,
    ) -> Admission<'_> {
        let mask = |table: &[Box<[u64]>], edge: Option<&Segment>| {
            table.iter().enumerate().fold(1u64, |bits, (k, lanes)| {
                bits | (u64::from(edge_passes(Some(lanes), edge)) << (k + 1))
            })
        };
        Admission {
            index: self,
            first: mask(&self.first, first),
            last: mask(&self.last, last),
        }
    }

    /// Heap bytes this index keeps for the life of the grammar's cache.
    pub(crate) fn retained_bytes(&self) -> usize {
        let table = |t: &[Box<[u64]>]| {
            std::mem::size_of_val(t) + t.iter().map(|l| std::mem::size_of_val(&**l)).sum::<usize>()
        };
        std::mem::size_of::<Self>()
            + table(&self.first)
            + table(&self.last)
            + std::mem::size_of_val(&*self.keys)
    }
}

impl Admission<'_> {
    /// True when at least one allomorph is admitted; false means the whole rule cannot match this word.
    pub(crate) fn any(&self) -> bool {
        (0..self.index.keys.len()).any(|i| self.allows(i))
    }

    /// False only when allomorph `i` provably cannot match because a mandatory word edge cannot unify with its mandatory pattern edge.
    pub(crate) fn allows(&self, i: usize) -> bool {
        let (f, l) = self.index.keys[i];
        (self.first >> f) & 1 == 1 && (self.last >> l) & 1 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pg_fst::CompileNode;
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
    fn any_star() -> CompileNode {
        CompileNode::Quantifier {
            min: 0,
            max: None,
            children: vec![c(u64::MAX)],
        }
    }
    fn suffix(bits: u64) -> AnalysisLhs {
        lhs(vec![any_star(), c(bits)])
    }
    fn word(bits: &[u64]) -> Vec<Segment> {
        bits.iter().map(|b| Segment::new(vec![*b])).collect()
    }
    fn admitted(d: &EdgeDispatch, segs: &[Segment]) -> Vec<bool> {
        let a = d.admit(segs);
        (0..d.keys.len()).map(|i| a.allows(i)).collect()
    }

    #[test]
    fn dispatch_agrees_with_the_per_allomorph_prefilter() {
        let rules = [
            suffix(A),
            suffix(B),
            suffix(A),
            lhs(vec![c(B), any_star()]),
            lhs(vec![CompileNode::Alternation(vec![vec![c(A)], vec![c(B)]])]),
        ];
        let d = EdgeDispatch::build(rules.iter().map(Some));
        for w in [
            word(&[A, A]),
            word(&[B, A]),
            word(&[A, B]),
            word(&[B, B]),
            vec![],
        ] {
            let want: Vec<bool> = rules.iter().map(|r| !r.edge_rejects(&w)).collect();
            assert_eq!(admitted(&d, &w), want);
        }
    }

    #[test]
    fn equal_constraints_share_one_table_entry() {
        let rules = [suffix(A), suffix(B), suffix(A), suffix(A)];
        let d = EdgeDispatch::build(rules.iter().map(Some));
        assert_eq!(d.last.len(), 2);
        assert!(d.first.is_empty());
    }

    #[test]
    fn an_unknown_or_unconstrained_allomorph_is_always_admitted() {
        let d = EdgeDispatch::build([None, Some(&lhs(vec![]))].into_iter());
        assert!(d.is_all_wildcard());
        assert_eq!(admitted(&d, &word(&[A])), [true, true]);
    }

    #[test]
    fn an_optional_word_edge_admits_every_allomorph() {
        let rules = [suffix(B), suffix(A)];
        let d = EdgeDispatch::build(rules.iter().map(Some));
        let mut segs = word(&[A, A]);
        segs[1].optional = true;
        assert_eq!(admitted(&d, &segs), [true, true]);
    }

    #[test]
    fn constraints_past_the_table_limit_fall_back_to_the_wildcard() {
        let rules: Vec<AnalysisLhs> = (0..70u64).map(|k| suffix(1 << (k % 64))).collect();
        let d = EdgeDispatch::build(rules.iter().map(Some));
        assert_eq!(d.last.len(), MAX_DISTINCT);
        // Whatever the word, the index never rejects what the prefilter accepts.
        for k in 0..64u64 {
            let w = word(&[1 << k]);
            let got = admitted(&d, &w);
            for (i, r) in rules.iter().enumerate() {
                assert!(got[i] || r.edge_rejects(&w), "rule {i} word {k}");
            }
        }
    }

    #[test]
    fn retained_bytes_counts_tables_and_keys() {
        let rules = [suffix(A), suffix(B), suffix(A)];
        let d = EdgeDispatch::build(rules.iter().map(Some));
        let floor = std::mem::size_of::<EdgeDispatch>() + 3 * 2 + 2 * 8;
        assert!(d.retained_bytes() >= floor);
    }
}
