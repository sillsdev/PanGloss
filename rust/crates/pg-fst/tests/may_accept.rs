//! The affix-acceptance prefilter: `Transduce::may_accept` is a register-free superset of `all_matches`. `false` must prove the traversal finds nothing (a wrong reject would silently drop analyses); `true` may be a false alarm. Alphabet: one-hot bits in lane 0 (`a=0b001,b=0b010,c=0b100`).

use pg_fst::{CompileInput, CompileNode, Fst, Segment, Transduce};

const A: u64 = 0b001;
const B: u64 = 0b010;
const C: u64 = 0b100;

fn k(bit: u64) -> CompileNode {
    CompileNode::Constraint(vec![bit])
}

fn compile(nodes: Vec<CompileNode>, deterministic: bool) -> Fst {
    CompileInput::new(nodes)
        .deterministic(deterministic)
        .compile()
}

fn segs(word: &[(u64, bool)]) -> Vec<Segment> {
    word.iter()
        .map(|&(bit, optional)| {
            if optional {
                Segment::optional(vec![bit])
            } else {
                Segment::new(vec![bit])
            }
        })
        .collect()
}

fn t<'f>(fst: &'f Fst, word: &[(u64, bool)], start: bool, end: bool) -> Transduce<'f> {
    Transduce::new(fst, segs(word)).anchored(start, end)
}

fn req(word: &[u64]) -> Vec<(u64, bool)> {
    word.iter().map(|&b| (b, false)).collect()
}

#[test]
fn a_word_the_pattern_cannot_match_is_rejected() {
    let fst = compile(vec![k(A), k(B)], true);
    let (accepts, arc_checks) = t(&fst, &req(&[A, C]), true, true).may_accept();
    assert!(!accepts);
    assert!(
        arc_checks > 0,
        "the pass reports the arcs it checked so the caller can charge them"
    );
    assert!(!t(&fst, &req(&[B, A]), true, true).may_accept().0);
}

#[test]
fn a_word_the_pattern_matches_is_not_rejected() {
    let fst = compile(vec![k(A), k(B)], true);
    let transduce = t(&fst, &req(&[A, B]), true, true);
    assert!(!transduce.all_matches().is_empty());
    assert!(transduce.may_accept().0);
}

#[test]
fn an_end_anchor_rejects_a_pattern_that_stops_early_and_an_open_end_does_not() {
    let fst = compile(vec![k(A)], true);
    assert!(!t(&fst, &req(&[A, A]), true, true).may_accept().0);
    assert!(t(&fst, &req(&[A, A]), true, false).may_accept().0);
    assert!(t(&fst, &req(&[B, A]), false, true).may_accept().0);
}

#[test]
fn an_optional_word_segment_can_be_skipped_but_a_required_one_cannot() {
    let fst = compile(vec![k(A), k(B)], true);
    // a, (c optional), b: matches only by skipping the optional c.
    let word = [(A, false), (C, true), (B, false)];
    let transduce = t(&fst, &word, true, true);
    assert!(!transduce.all_matches().is_empty());
    assert!(transduce.may_accept().0);
    let word = [(A, false), (C, false), (B, false)];
    assert!(!t(&fst, &word, true, true).may_accept().0);
}

#[test]
fn a_nondeterministic_alternation_is_not_rejected_on_any_branch() {
    let fst = compile(
        vec![CompileNode::Alternation(vec![
            vec![k(A), k(B)],
            vec![k(A), k(C)],
        ])],
        false,
    );
    assert!(t(&fst, &req(&[A, B]), true, true).may_accept().0);
    assert!(t(&fst, &req(&[A, C]), true, true).may_accept().0);
    assert!(!t(&fst, &req(&[A, A]), true, true).may_accept().0);
}

#[test]
fn an_empty_word_is_rejected_without_checking_arcs() {
    let fst = compile(vec![k(A)], true);
    assert_eq!(t(&fst, &[], true, true).may_accept(), (false, 0));
}

/// Exhaustive: over every word up to length 4 and a spread of patterns, a rejection never coincides with a match.
#[test]
fn a_rejection_never_coincides_with_a_match() {
    let patterns: Vec<(Vec<CompileNode>, bool)> = vec![
        (vec![k(A)], true),
        (vec![k(A), k(B)], true),
        (vec![k(A), k(A | B), k(C)], false),
        (
            vec![
                CompileNode::Quantifier {
                    min: 0,
                    max: None,
                    children: vec![k(A)],
                },
                k(B),
            ],
            false,
        ),
        (
            vec![
                CompileNode::Group {
                    name: "g".into(),
                    children: vec![k(A | B)],
                },
                CompileNode::Quantifier {
                    min: 0,
                    max: Some(1),
                    children: vec![k(C)],
                },
            ],
            false,
        ),
        (
            vec![CompileNode::Alternation(vec![vec![k(A)], vec![k(B), k(C)]])],
            true,
        ),
    ];
    let mut words: Vec<Vec<u64>> = vec![vec![]];
    let mut frontier = vec![vec![]];
    for _ in 0..4 {
        let mut next = Vec::new();
        for w in &frontier {
            for bit in [A, B, C] {
                let mut x: Vec<u64> = w.clone();
                x.push(bit);
                next.push(x);
            }
        }
        words.extend(next.iter().cloned());
        frontier = next;
    }
    let mut checked = 0;
    let mut rejected = 0;
    let mut matched = 0;
    for (nodes, deterministic) in &patterns {
        let fst = compile(nodes.clone(), *deterministic);
        for w in &words {
            // Every second segment optional, so the optional-skip path is covered too.
            for flags in [false, true] {
                let word: Vec<(u64, bool)> = w
                    .iter()
                    .enumerate()
                    .map(|(i, &b)| (b, flags && i % 2 == 1))
                    .collect();
                for (s, e) in [(true, true), (true, false), (false, true), (false, false)] {
                    let transduce = t(&fst, &word, s, e);
                    let found = !transduce.all_matches().is_empty();
                    let accepts = transduce.may_accept().0;
                    assert!(
                        accepts || !found,
                        "wrong reject: word {w:?} optional-alternate {flags} anchors ({s},{e})"
                    );
                    checked += 1;
                    rejected += usize::from(!accepts);
                    matched += usize::from(found);
                }
            }
        }
    }
    assert!(
        rejected > 0 && matched > 0,
        "the sweep must exercise both outcomes ({checked} calls)"
    );
}
