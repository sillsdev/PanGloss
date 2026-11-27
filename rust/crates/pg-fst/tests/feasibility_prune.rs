//! End-anchored feasibility pruning: an arc is not taken when the required segments still ahead exceed the longest path left to acceptance. Alphabet: one-hot bits in lane 0 (`a=0b01,b=0b10`).

use pg_fst::{CompileInput, CompileNode, Segment, Transduce};

const A: u64 = 0b01;
const B: u64 = 0b10;

/// Every ways to consume `pattern` over `segments` when only optional segments may be skipped, as the per-element `(start, end)` spans a capture group per element reports.
fn reference_spans(pattern: &[u64], segments: &[(u64, bool)]) -> Vec<Vec<(i32, i32)>> {
    fn walk(
        pattern: &[u64],
        segments: &[(u64, bool)],
        element: usize,
        at: usize,
        consumed: &mut Vec<usize>,
        out: &mut Vec<Vec<(i32, i32)>>,
    ) {
        if element == pattern.len() {
            if segments[at..].iter().all(|s| s.1) {
                let n = segments.len() as i32;
                let mut spans = Vec::new();
                for (i, &c) in consumed.iter().enumerate() {
                    let end = consumed.get(i + 1).map_or(n, |&next| next as i32);
                    spans.push((c as i32, end));
                }
                out.push(spans);
            }
            return;
        }
        for c in at..segments.len() {
            if segments[c].0 & pattern[element] != 0 {
                consumed.push(c);
                walk(pattern, segments, element + 1, c + 1, consumed, out);
                consumed.pop();
            }
            if !segments[c].1 {
                break;
            }
        }
    }
    let mut out = Vec::new();
    walk(pattern, segments, 0, 0, &mut Vec::new(), &mut out);
    out
}

fn grouped_chain(pattern: &[u64]) -> pg_fst::Fst {
    let nodes = pattern
        .iter()
        .enumerate()
        .map(|(i, &bit)| CompileNode::Group {
            name: format!("g{i}"),
            children: vec![CompileNode::Constraint(vec![bit])],
        })
        .collect();
    CompileInput::new(nodes).deterministic(false).compile()
}

fn run(
    fst: &pg_fst::Fst,
    segments: &[(u64, bool)],
    elements: usize,
) -> (Vec<Vec<(i32, i32)>>, usize) {
    let input: Vec<Segment> = segments
        .iter()
        .map(|&(bit, optional)| {
            if optional {
                Segment::optional(vec![bit])
            } else {
                Segment::new(vec![bit])
            }
        })
        .collect();
    let budget = pg_fst::work::WorkBudget::new(usize::MAX / 2);
    let _scope = budget.enter();
    let matches = Transduce::new(fst, input)
        .anchored(true, true)
        .all_matches();
    let spans = matches
        .iter()
        .map(|m| {
            (0..elements)
                .map(|i| {
                    fst.get_offsets(&format!("g{i}"), &m.registers)
                        .expect("every element is captured")
                })
                .collect()
        })
        .collect();
    (spans, budget.used())
}

/// More required segments than the pattern has arcs left: nothing can match, and the optional run before them must not be explored.
#[test]
fn a_surplus_of_required_segments_is_refused_at_the_first_arc() {
    let pattern = [A, B, A];
    let fst = grouped_chain(&pattern);
    let mut segments = vec![(A, false)];
    segments.extend(std::iter::repeat_n((B, true), 12));
    segments.extend([(A, false), (A, false), (A, false)]);
    let (spans, used) = run(&fst, &segments, pattern.len());
    assert!(spans.is_empty());
    assert!(reference_spans(&pattern, &segments).is_empty());
    // The unpruned traversal spends 404 here.
    assert!(used <= 10, "work {used}");
}

/// The prune is exact where matches exist: every match of the exhaustive reference, in the same count.
#[test]
fn matches_equal_the_exhaustive_reference() {
    let pattern = [A, B, A];
    let fst = grouped_chain(&pattern);
    for optional_b in 0..6 {
        let mut segments = vec![(A, true), (A, false)];
        segments.extend(std::iter::repeat_n((B, true), optional_b));
        segments.extend([(B, false), (A, true), (A, true)]);
        let (spans, _) = run(&fst, &segments, pattern.len());
        let mut got = spans.clone();
        let mut want = reference_spans(&pattern, &segments);
        got.sort();
        want.sort();
        assert_eq!(got, want, "optional_b = {optional_b}");
    }
}

/// An unbounded pattern has no longest path, so nothing is pruned on that account.
#[test]
fn a_looping_pattern_is_never_pruned() {
    let nodes = vec![CompileNode::Quantifier {
        min: 1,
        max: None,
        children: vec![CompileNode::Constraint(vec![A])],
    }];
    let fst = CompileInput::new(nodes).deterministic(false).compile();
    let input: Vec<Segment> = (0..6).map(|_| Segment::new(vec![A])).collect();
    let matches = Transduce::new(&fst, input)
        .anchored(true, true)
        .all_matches();
    assert_eq!(matches.len(), 1);
}

/// Alternation: the longest path is the maximum over the branches, and the matches equal those of the expanded sequences.
#[test]
fn an_alternation_is_bounded_by_its_longest_branch() {
    let g = |name: &str, bit: u64| CompileNode::Group {
        name: name.to_string(),
        children: vec![CompileNode::Constraint(vec![bit])],
    };
    let nodes = vec![
        g("a0", A),
        CompileNode::Alternation(vec![
            vec![g("s1", B)],
            vec![g("l1", B), g("l2", A), g("l3", B)],
        ]),
        g("a2", A),
    ];
    let fst = CompileInput::new(nodes).deterministic(false).compile();
    let count = |segments: &[(u64, bool)]| -> (usize, usize) {
        let input: Vec<Segment> = segments
            .iter()
            .map(|&(bit, optional)| {
                if optional {
                    Segment::optional(vec![bit])
                } else {
                    Segment::new(vec![bit])
                }
            })
            .collect();
        let budget = pg_fst::work::WorkBudget::new(usize::MAX / 2);
        let _scope = budget.enter();
        let n = Transduce::new(&fst, input)
            .anchored(true, true)
            .all_matches()
            .len();
        (n, budget.used())
    };
    let short = [A, B, A];
    let long = [A, B, A, B, A];
    for pad in 0..5 {
        let mut segments = vec![(A, false), (B, true)];
        segments.extend(std::iter::repeat_n((A, true), pad));
        segments.extend([(B, true), (A, false)]);
        let plain: Vec<(u64, bool)> = segments.clone();
        let want = reference_spans(&short, &plain).len() + reference_spans(&long, &plain).len();
        assert_eq!(count(&segments).0, want, "pad {pad}");
    }
    // Six required segments exceed the longest branch (5): refused up front.
    let surplus = [
        (A, false),
        (B, false),
        (A, false),
        (B, false),
        (A, false),
        (A, false),
    ];
    let (n, used) = count(&surplus);
    assert_eq!(n, 0);
    assert!(used <= 10, "work {used}");
    // Five required segments take the long branch exactly.
    let exact = [(A, false), (B, false), (A, false), (B, false), (A, false)];
    assert_eq!(count(&exact).0, 1);
}
