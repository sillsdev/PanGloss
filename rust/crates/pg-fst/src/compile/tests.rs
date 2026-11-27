use super::*;
use crate::nfa::ArcInput;

const H: u8 = 0;
const M: u8 = 1;

/// Per-state arcs as `(target, kind, priority type)` with kind `c` (input) or `e` (epsilon), in list order.
fn shape(nfa: &Nfa) -> Vec<Vec<(usize, char, u8)>> {
    nfa.states
        .iter()
        .map(|s| {
            s.arcs
                .iter()
                .map(|a| {
                    let k = if matches!(a.input, ArcInput::Epsilon) {
                        'e'
                    } else {
                        'c'
                    };
                    (a.target, k, a.ptype)
                })
                .collect()
        })
        .collect()
}

fn quantified(min: u32, max: Option<u32>, children: Vec<CompileNode>) -> (Nfa, usize) {
    let mut nfa = Nfa::new();
    let end = gen_quantifier(&mut nfa, 0, min, max, &children);
    (nfa, end)
}

fn c() -> Vec<CompileNode> {
    vec![CompileNode::Constraint(vec![1])]
}

/// Hand-derived from C# `Quantifier.GenerateNfa` (Quantifier.cs) with `Group.GenerateNfa` adding one epsilon per body; equal-type arcs insert as `ArcCollection.AddInternal` does.
#[test]
fn quantifier_zero_to_unbounded_matches_csharp_shape() {
    let (nfa, end) = quantified(0, None, c());
    assert_eq!(end, 4);
    assert_eq!(
        shape(&nfa),
        vec![
            vec![(4, 'e', M), (1, 'e', H)],
            vec![(2, 'c', M)],
            vec![(3, 'e', M)],
            vec![(0, 'e', H)],
            vec![],
        ]
    );
}

#[test]
fn quantifier_one_to_unbounded_matches_csharp_shape() {
    let (nfa, end) = quantified(1, None, c());
    assert_eq!(end, 2);
    assert_eq!(
        shape(&nfa),
        vec![vec![(1, 'c', M)], vec![(2, 'e', M)], vec![(0, 'e', H)]]
    );
}

#[test]
fn quantifier_zero_to_two_matches_csharp_shape() {
    let (nfa, end) = quantified(0, Some(2), c());
    assert_eq!(end, 6);
    assert_eq!(
        shape(&nfa),
        vec![
            vec![(6, 'e', M), (1, 'e', H)],
            vec![(2, 'c', M)],
            vec![(3, 'e', M)],
            vec![(6, 'e', M), (4, 'e', H)],
            vec![(5, 'c', M)],
            vec![(6, 'e', M)],
            vec![],
        ]
    );
}

/// `{2,3}`: min above zero concatenates two copies, then one optional copy with a skip arc from the end of the mandatory part.
#[test]
fn quantifier_two_to_three_matches_csharp_shape() {
    let (nfa, end) = quantified(2, Some(3), c());
    assert_eq!(end, 7);
    assert_eq!(
        shape(&nfa),
        vec![
            vec![(1, 'c', M)],
            vec![(2, 'e', M)],
            vec![(3, 'c', M)],
            vec![(4, 'e', M)],
            vec![(7, 'e', M), (5, 'e', H)],
            vec![(6, 'c', M)],
            vec![(7, 'e', M)],
            vec![],
        ]
    );
}

#[test]
fn nested_quantifier_matches_csharp_shape() {
    let inner = CompileNode::Quantifier {
        min: 1,
        max: None,
        children: c(),
    };
    let (nfa, end) = quantified(0, None, vec![inner]);
    assert_eq!(end, 5);
    assert_eq!(
        shape(&nfa),
        vec![
            vec![(5, 'e', M), (1, 'e', H)],
            vec![(2, 'c', M)],
            vec![(3, 'e', M)],
            vec![(4, 'e', M), (1, 'e', H)],
            vec![(0, 'e', H)],
            vec![],
        ]
    );
}
