use super::*;

/// Test helper: a concrete-node path; `cd_set` is never consulted for a concrete edge.
fn concrete(segs: &[(u32, Vec<u64>)]) -> Vec<(u32, Vec<u64>, CdSet)> {
    segs.iter()
        .map(|(cd, l)| (*cd, l.clone(), CdSet::Unrestricted))
        .collect()
}

// A tiny hand-built trie: cd 10="p"[0b01], cd 11="a"[0b10], cd 12="b"[0b01]; roots A=/pa/, B=/pab/, C=/a/.
fn tiny_trie() -> RootAllomorphTrie {
    let mut t = RootAllomorphTrie {
        nodes: vec![TrieNode::default()],
        table: TableId(0),
        feat_width: 1,
        allomorph_count: 0,
    };
    t.add_path(
        &concrete(&[(10, vec![0b01]), (11, vec![0b10])]),
        AllomorphId(100),
        LexEntryId(0),
    );
    t.add_path(
        &concrete(&[(10, vec![0b01]), (11, vec![0b10]), (12, vec![0b01])]),
        AllomorphId(101),
        LexEntryId(1),
    );
    t.add_path(
        &concrete(&[(11, vec![0b10])]),
        AllomorphId(102),
        LexEntryId(2),
    );
    t
}

#[test]
fn exact_match_returns_the_root() {
    let t = tiny_trie();
    // Search /p a/ (exact same char_defs + lanes as root A) ⇒ {allo 100}.
    let got = t.search_segs(&[(10, vec![0b01]), (11, vec![0b10])]);
    assert_eq!(got, vec![(AllomorphId(100), LexEntryId(0))]);
}

#[test]
fn prefix_of_a_longer_root_does_not_accept() {
    let t = tiny_trie();
    // /p a b/ is root B, not A: end-anchored, so /p a/ must NOT return B, and /p a b/ returns B only.
    let got = t.search_segs(&[(10, vec![0b01]), (11, vec![0b10]), (12, vec![0b01])]);
    assert_eq!(got, vec![(AllomorphId(101), LexEntryId(1))]);
}

#[test]
fn feature_unification_matches_underspecified_input() {
    let t = tiny_trie();
    // Superset lane [0b11] unifies with stored [0b01] (AND != 0); this is unification, not identity.
    let got = t.search_segs(&[(10, vec![0b11]), (11, vec![0b10])]);
    assert_eq!(got, vec![(AllomorphId(100), LexEntryId(0))]);
}

#[test]
fn feature_conflict_rejects_despite_char_def_match() {
    let t = tiny_trie();
    // Same char_def, but lanes [0b10] conflict with stored [0b01] (AND = 0): features clash despite char_def match.
    let got = t.search_segs(&[(10, vec![0b10]), (11, vec![0b10])]);
    assert!(got.is_empty(), "feature conflict must reject, got {got:?}");
}

#[test]
fn char_def_mismatch_rejects() {
    let t = tiny_trie();
    // char_def 99 has no edge at the root node, so no match even though lanes could unify.
    let got = t.search_segs(&[(99, vec![0b01]), (11, vec![0b10])]);
    assert!(got.is_empty(), "char_def mismatch must reject, got {got:?}");
}

// The build-time unifiability closure (Design A) as an equality-miss fallback in `edge_matches`.

#[test]
fn closure_cross_matches_a_distinct_unifiable_char_def_only_when_provided() {
    let t = tiny_trie();
    // Declares cd 10 and cd 99 (a distinct char_def) as closure siblings.
    let mut closure = vec![CdBits::empty(); 100];
    closure[10].insert(10);
    closure[10].insert(99);
    closure[99].insert(99);
    closure[99].insert(10);

    // With Some(closure) and lane-compatible input, the equality-miss fallback lets root A's edge match.
    let got = t.search_segs_with_closure(&[(99, vec![0b01]), (11, vec![0b10])], Some(&closure));
    assert_eq!(
        got,
        vec![(AllomorphId(100), LexEntryId(0))],
        "closure hit must cross-match"
    );

    // The same query with closure = None must not match: closure absent means bit-for-bit prior behavior.
    let got_none = t.search_segs_with_closure(&[(99, vec![0b01]), (11, vec![0b10])], None);
    assert!(
        got_none.is_empty(),
        "closure disabled must still reject a distinct char_def"
    );
}

#[test]
fn closure_membership_does_not_bypass_the_lane_conjunct() {
    let t = tiny_trie();
    // cd 10 and cd 99 are still declared closure siblings...
    let mut closure = vec![CdBits::empty(); 100];
    closure[10].insert(99);
    closure[99].insert(10);
    // ...but this query's lanes for cd 99 are [0b10], conflicting with the stored edge's [0b01]: the closure hit is refined by the existing `flat_unifiable` conjunct, never a substitute for it.
    let got = t.search_segs_with_closure(&[(99, vec![0b10]), (11, vec![0b10])], Some(&closure));
    assert!(
        got.is_empty(),
        "closure membership must not bypass the phonological-lane conjunct"
    );
}

#[test]
fn closure_present_but_unrelated_char_defs_still_reject() {
    let t = tiny_trie();
    // A closure exists but declares no relation for cd 99 (an all-empty row), so it must behave exactly like the no-closure case.
    let closure = vec![CdBits::empty(); 100];
    let got = t.search_segs_with_closure(&[(99, vec![0b01]), (11, vec![0b10])], Some(&closure));
    assert!(
        got.is_empty(),
        "an empty closure row must not manufacture a match"
    );
}

#[test]
fn single_segment_root_matches_and_is_distinct_from_prefixes() {
    let t = tiny_trie();
    // /a/ is root C. It must NOT collide with the /p .../ paths.
    let got = t.search_segs(&[(11, vec![0b10])]);
    assert_eq!(got, vec![(AllomorphId(102), LexEntryId(2))]);
}

#[test]
fn homographs_accumulate_at_one_accepting_node() {
    // Two entries sharing the identical surface /p a/ both accept at the same node.
    let mut t = tiny_trie();
    t.add_path(
        &concrete(&[(10, vec![0b01]), (11, vec![0b10])]),
        AllomorphId(200),
        LexEntryId(7),
    );
    let got = t.search_segs(&[(10, vec![0b01]), (11, vec![0b10])]);
    assert_eq!(
        got,
        vec![
            (AllomorphId(100), LexEntryId(0)),
            (AllomorphId(200), LexEntryId(7))
        ],
    );
}

#[test]
fn empty_and_too_long_inputs_do_not_match() {
    let t = tiny_trie();
    // Empty input: root node has no accepts ⇒ nothing.
    assert!(t.search_segs(&[]).is_empty());
    // /p a b b/: after /p a b/ there is no further edge ⇒ end-anchored fail.
    let got = t.search_segs(&[
        (10, vec![0b01]),
        (11, vec![0b10]),
        (12, vec![0b01]),
        (12, vec![0b01]),
    ]);
    assert!(got.is_empty());
}

#[test]
fn zero_phon_feature_discrimination_is_by_char_def() {
    // A synthetic feat_width-0 stratum (all lanes empty): no real grammar builds one any more (phon_features always has the synthetic Type feature), so this is a decoupled exercise of the width-0 codepath.
    let mut t = RootAllomorphTrie {
        nodes: vec![TrieNode::default()],
        table: TableId(0),
        feat_width: 0,
        allomorph_count: 0,
    };
    t.add_path(
        &concrete(&[(1, vec![]), (2, vec![])]),
        AllomorphId(1),
        LexEntryId(0),
    ); // /b a/
    t.add_path(
        &concrete(&[(3, vec![]), (4, vec![])]),
        AllomorphId(2),
        LexEntryId(1),
    ); // /m u/
    assert_eq!(
        t.search_segs(&[(1, vec![]), (2, vec![])]),
        vec![(AllomorphId(1), LexEntryId(0))]
    );
    assert_eq!(
        t.search_segs(&[(3, vec![]), (4, vec![])]),
        vec![(AllomorphId(2), LexEntryId(1))]
    );
    // A different length or a swapped char_def must not match either root.
    assert!(t.search_segs(&[(1, vec![]), (4, vec![])]).is_empty());
    assert_eq!(t.allomorph_count(), 2);
}

// Pattern-derived (NO_CHAR_DEF + CdSet) edges, loader N3 end-to-end.

/// A root "b[Vowel]t" (cd 20="b", cd 22="t"; Vowel = {21, 23}) — the loader-N3 fixture shape.
fn pattern_trie() -> RootAllomorphTrie {
    let mut t = RootAllomorphTrie {
        nodes: vec![TrieNode::default()],
        table: TableId(0),
        feat_width: 0,
        allomorph_count: 0,
    };
    t.add_path(
        &[
            (20, vec![], CdSet::Unrestricted),
            (
                NO_CHAR_DEF,
                vec![],
                CdSet::Members(CdBits::from_ids([21, 23])),
            ),
            (22, vec![], CdSet::Unrestricted),
        ],
        AllomorphId(300),
        LexEntryId(9),
    );
    t
}

#[test]
fn pattern_edge_matches_each_class_member() {
    let t = pattern_trie();
    // Both "bat" (21) and "bet" (23) reach the accept through the class edge.
    assert_eq!(
        t.search_segs(&[(20, vec![]), (21, vec![]), (22, vec![])]),
        vec![(AllomorphId(300), LexEntryId(9))],
    );
    assert_eq!(
        t.search_segs(&[(20, vec![]), (23, vec![]), (22, vec![])]),
        vec![(AllomorphId(300), LexEntryId(9))],
    );
}

#[test]
fn pattern_edge_rejects_a_non_member() {
    let t = pattern_trie();
    // "bit" (24 not in {21, 23}): the membership gate must reject even though a NO_CHAR_DEF edge exists there and the empty lanes trivially unify.
    assert!(t
        .search_segs(&[(20, vec![]), (24, vec![]), (22, vec![])])
        .is_empty());
}

#[test]
fn no_char_def_query_still_passes_a_pattern_edge() {
    let t = pattern_trie();
    // A reinserted/unidentified query segment (NO_CHAR_DEF) keeps its wildcard behavior against pattern edges too.
    assert_eq!(
        t.search_segs(&[(20, vec![]), (NO_CHAR_DEF, vec![]), (22, vec![])]),
        vec![(AllomorphId(300), LexEntryId(9))],
    );
}

#[test]
fn distinct_class_edges_do_not_merge() {
    // Two pattern roots whose classes differ must get separate edges: "x[A]" with A={1} and "x[B]" with B={2}.
    let mut t = RootAllomorphTrie {
        nodes: vec![TrieNode::default()],
        table: TableId(0),
        feat_width: 0,
        allomorph_count: 0,
    };
    t.add_path(
        &[
            (5, vec![], CdSet::Unrestricted),
            (NO_CHAR_DEF, vec![], CdSet::Members(CdBits::from_ids([1]))),
        ],
        AllomorphId(1),
        LexEntryId(0),
    );
    t.add_path(
        &[
            (5, vec![], CdSet::Unrestricted),
            (NO_CHAR_DEF, vec![], CdSet::Members(CdBits::from_ids([2]))),
        ],
        AllomorphId(2),
        LexEntryId(1),
    );
    assert_eq!(
        t.search_segs(&[(5, vec![]), (1, vec![])]),
        vec![(AllomorphId(1), LexEntryId(0))]
    );
    assert_eq!(
        t.search_segs(&[(5, vec![]), (2, vec![])]),
        vec![(AllomorphId(2), LexEntryId(1))]
    );
    // Identical classes DO share an edge (prefix sharing still works for patterns).
    t.add_path(
        &[
            (5, vec![], CdSet::Unrestricted),
            (NO_CHAR_DEF, vec![], CdSet::Members(CdBits::from_ids([1]))),
        ],
        AllomorphId(3),
        LexEntryId(2),
    );
    assert_eq!(
        t.search_segs(&[(5, vec![]), (1, vec![])]),
        vec![
            (AllomorphId(1), LexEntryId(0)),
            (AllomorphId(3), LexEntryId(2))
        ],
    );
}
