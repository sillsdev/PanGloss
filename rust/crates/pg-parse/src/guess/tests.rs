use super::*;
use pg_shape::CdBits;

// Hand-built node constructors: `match_nodes_with_pattern` is a pure function over `&[GuessNode]`.

/// A concrete segment node (a real char-def, e.g. table id 1 = "a").
fn seg(cd: u32) -> GuessNode {
    GuessNode {
        kind: NodeKind::Segment,
        char_def: cd,
        lanes: vec![],
        cd_set: CdSet::Unrestricted,
        optional: false,
        iterative: false,
        deleted: false,
    }
}

/// A mandatory `[Any]` class-reference node: abstract, matches every segment.
fn any_class(optional: bool, iterative: bool) -> GuessNode {
    GuessNode {
        kind: NodeKind::Segment,
        char_def: NO_CHAR_DEF,
        lanes: vec![],
        cd_set: CdSet::Unrestricted,
        optional,
        iterative,
        deleted: false,
    }
}

/// A boundary node ("+"), optional (as every boundary is after segmentation).
fn boundary(cd: u32) -> GuessNode {
    GuessNode {
        kind: NodeKind::Boundary,
        char_def: cd,
        lanes: vec![],
        cd_set: CdSet::Unrestricted,
        optional: true,
        iterative: false,
        deleted: false,
    }
}

const A: u32 = 1; // the "a" char-def id used throughout these hand-built fixtures.
const I: u32 = 2; // a different literal char-def ("i").
const PLUS: u32 = 9; // the boundary char-def id ("+").

fn nodes(n: usize) -> Vec<GuessNode> {
    (0..n).map(|_| seg(A)).collect()
}

// Ported from `MorpherTests.TestMatchNodesWithPattern` (MorpherTests.cs:349-449).

/// C#'s "test feature matching" block: unifying two nodes that each constrain a different lane must produce a node carrying both constraints.
#[test]
fn unify_merges_disjoint_lane_constraints_from_both_sides() {
    let a_is_valuea = 0b01u64;
    let b_is_valueb = 0b01u64;
    let input = GuessNode {
        lanes: vec![a_is_valuea, u64::MAX],
        ..seg(A)
    };
    let pattern = GuessNode {
        lanes: vec![u64::MAX, b_is_valueb],
        ..seg(A)
    };
    let unified =
        unify_shape_nodes(&input, &pattern).expect("compatible: same char_def, unifiable lanes");
    assert_eq!(
        unified.lanes,
        vec![a_is_valuea, b_is_valueb],
        "both constraints must survive the unify"
    );
}

/// "Test sequences": different concrete char-defs never unify.
#[test]
fn one_node_against_a_different_literal_fails() {
    let got = match_nodes_with_pattern(&nodes(1), &[seg(I)]);
    assert!(got.is_empty());
}

#[test]
fn one_node_against_itself_matches_exactly_once() {
    let got = match_nodes_with_pattern(&nodes(1), &nodes(1));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0], nodes(1));
}

#[test]
fn two_and_three_nodes_against_themselves_match_exactly_once() {
    for n in [2usize, 3] {
        let got = match_nodes_with_pattern(&nodes(n), &nodes(n));
        assert_eq!(got.len(), 1, "n={n}");
        assert_eq!(got[0], nodes(n), "n={n}");
    }
}

/// "Test optionality": `([Any])` -- zero or one real node.
#[test]
fn optional_class_matches_zero_or_one_node_but_not_two() {
    let pattern = vec![any_class(true, false)];
    assert_eq!(
        match_nodes_with_pattern(&nodes(0), &pattern).len(),
        1,
        "zero nodes: skip"
    );
    let one = match_nodes_with_pattern(&nodes(1), &pattern);
    assert_eq!(one.len(), 1, "one node: consume");
    assert_eq!(one[0], nodes(1));
    assert!(
        match_nodes_with_pattern(&nodes(2), &pattern).is_empty(),
        "two nodes: no path consumes both"
    );
}

/// "Test ambiguity": two independently optional slots; one node matches via either, two distinct paths.
#[test]
fn two_independent_optionals_are_ambiguous_for_one_node_but_not_zero_or_two() {
    let pattern = vec![any_class(true, false), any_class(true, false)];
    let zero = match_nodes_with_pattern(&nodes(0), &pattern);
    assert_eq!(zero.len(), 1, "zero nodes: skip both, one path");
    assert_eq!(zero[0], nodes(0));

    let one = match_nodes_with_pattern(&nodes(1), &pattern);
    assert_eq!(
        one.len(),
        2,
        "one node: two ways to place it (first slot or second slot)"
    );
    assert!(one.iter().all(|m| *m == nodes(1)));

    let two = match_nodes_with_pattern(&nodes(2), &pattern);
    assert_eq!(
        two.len(),
        1,
        "two nodes: both slots consumed, exactly one path"
    );
    assert_eq!(two[0], nodes(2));

    assert!(
        match_nodes_with_pattern(&nodes(3), &pattern).is_empty(),
        "three nodes: only two slots, no path"
    );
}

/// "Test Kleene star": zero, one, or more real nodes, always exactly one path.
#[test]
fn kleene_star_matches_any_count_with_exactly_one_path() {
    let pattern = vec![any_class(true, true)];
    for n in [0usize, 1, 2] {
        let got = match_nodes_with_pattern(&nodes(n), &pattern);
        assert_eq!(got.len(), 1, "n={n}");
        assert_eq!(got[0], nodes(n), "n={n}");
    }
}

/// "Test Kleene plus look alike": `[Any]+` -- `+` is a boundary marker, not a Kleene-plus operator.
#[test]
fn plus_after_class_is_a_boundary_not_kleene_plus() {
    let pattern = vec![any_class(false, false), boundary(PLUS)];
    assert!(
        match_nodes_with_pattern(&nodes(0), &pattern).is_empty(),
        "mandatory class needs >=1 node"
    );

    let one = match_nodes_with_pattern(&nodes(1), &pattern);
    assert_eq!(one.len(), 1);
    assert_eq!(
        one[0],
        nodes(1),
        "the boundary pattern slot is skipped (no boundary in the input)"
    );

    assert!(
        match_nodes_with_pattern(&nodes(2), &pattern).is_empty(),
        "second real node can't match a boundary"
    );
}

// Additional `unify_shape_nodes` coverage: identity narrowing, not in the C# test.

#[test]
fn kind_mismatch_never_unifies() {
    assert!(match_nodes_with_pattern(&[seg(A)], &[boundary(PLUS)]).is_empty());
}

#[test]
fn concrete_input_against_a_restricted_class_pattern_narrows_to_the_concrete_id() {
    let pattern_in_class = GuessNode {
        cd_set: CdSet::Members(CdBits::from_ids([A, I])),
        ..any_class(false, false)
    };
    let got = match_nodes_with_pattern(&[seg(A)], std::slice::from_ref(&pattern_in_class));
    assert_eq!(got.len(), 1);
    assert_eq!(
        got[0][0].char_def, A,
        "the unified node keeps the concrete identity"
    );

    let pattern_excludes = GuessNode {
        cd_set: CdSet::Members(CdBits::from_ids([I])),
        ..any_class(false, false)
    };
    assert!(
        match_nodes_with_pattern(&[seg(A)], &[pattern_excludes]).is_empty(),
        "A is not a member of the pattern's restricted class"
    );
}

#[test]
fn two_abstract_classes_unify_to_their_intersection() {
    let left = GuessNode {
        cd_set: CdSet::Members(CdBits::from_ids([1, 2, 3])),
        ..any_class(false, false)
    };
    let right = GuessNode {
        cd_set: CdSet::Members(CdBits::from_ids([2, 3, 4])),
        ..any_class(false, false)
    };
    let unified = unify_shape_nodes(&left, &right).expect("2,3 are shared");
    match unified.cd_set {
        CdSet::Members(b) => {
            assert_eq!(b.count(), 2);
            assert!(b.contains(2) && b.contains(3));
            assert!(!b.contains(1) && !b.contains(4));
        }
        CdSet::Unrestricted => panic!("expected a narrowed Members set"),
    }

    let disjoint_right = GuessNode {
        cd_set: CdSet::Members(CdBits::from_ids([9])),
        ..any_class(false, false)
    };
    assert!(
        unify_shape_nodes(&left, &disjoint_right).is_none(),
        "disjoint classes cannot unify"
    );
}

// `render_match`: the rendering half, against a tiny real table.

/// Grammar-load-based table probe, mirroring this crate's established test convention.
fn render_grammar() -> pg_grammar_model::model::Grammar {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>RenderMatchProbe</Name>
<PartsOfSpeech><PartOfSpeech id="n"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
  <BoundaryDefinitions>
    <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
  </BoundaryDefinitions>
</CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;
    pg_grammar::load(XML).expect("render_match probe grammar loads")
}

#[test]
fn render_match_skips_boundaries_and_renders_concrete_and_class_nodes() {
    let g = render_grammar();
    let table = &g.char_tables[0];
    let a = table.lookup_nfd("a").unwrap().0;
    let i = table.lookup_nfd("i").unwrap().0;
    let plus = table.lookup_nfd("+").unwrap().0;
    let matched = vec![seg(a), boundary(plus), seg(i)];
    assert_eq!(
        render_match(table, &matched),
        "ai",
        "the boundary node must be skipped"
    );
}

#[test]
fn render_match_renders_the_first_matching_table_rep_for_an_abstract_node() {
    let g = render_grammar();
    let table = &g.char_tables[0];
    let a = table.lookup_nfd("a").unwrap().0;
    // An abstract node restricted to {"a"} renders "a" (table document order, first rep).
    let restricted = GuessNode {
        cd_set: CdSet::Members(CdBits::from_ids([a])),
        ..any_class(false, false)
    };
    assert_eq!(render_match(table, &[restricted]), "a");
}

#[test]
fn render_match_of_an_empty_match_is_the_empty_string() {
    let g = render_grammar();
    assert_eq!(render_match(&g.char_tables[0], &[]), "");
}
