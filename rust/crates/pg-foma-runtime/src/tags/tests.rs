use super::*;

#[test]
fn width_scales_with_morpheme_count() {
    assert_eq!(tag_width(0), 1);
    assert_eq!(tag_width(1), 1);
    assert_eq!(tag_width(10), 1); // max index 9
    assert_eq!(tag_width(11), 2); // max index 10
    assert_eq!(tag_width(1503), 4); // max index 1502
}

#[test]
fn lexc_tag_distinguishes_leading_zeros_via_zero_glyph() {
    // Every would-be '0' digit is spelled with ZERO_GLYPH, so the two numerals stay distinguishable.
    let a = lexc_tag("R", 1, 4);
    let b = lexc_tag("R", 10, 4);
    assert_ne!(a, b);
    assert_eq!(a, "%<R%:zzz1%>");
    assert_eq!(b, "%<R%:zz1z%>");
}

/// Neither representation of any tag this module can produce ever contains the literal ASCII byte `'0'`, swept over a range wide enough to hit every digit position.
#[test]
fn no_tag_text_ever_contains_a_literal_zero_byte() {
    for width in [1usize, 2, 3, 4, 5] {
        for n in [0u32, 1, 9, 10, 40, 69, 90, 99, 100, 400, 900, 1000, 9999] {
            for prefix in ["R", "M"] {
                let lexc = lexc_tag(prefix, n, width);
                let text = tag_text(prefix, n, width);
                assert!(
                    !lexc.contains('0'),
                    "lexc_tag({prefix:?}, {n}, {width}) = {lexc:?} contains a literal '0'"
                );
                assert!(
                    !text.contains('0'),
                    "tag_text({prefix:?}, {n}, {width}) = {text:?} contains a literal '0'"
                );
            }
        }
    }
}

/// `decode_path` must recover the exact original `MorphemeId`, even when the padded numeral is nothing but `ZERO_GLYPH` substitutions.
#[test]
fn zero_glyph_tags_round_trip_through_decode_path() {
    for width in [1usize, 2, 3, 4, 5] {
        for n in [0u32, 1, 9, 10, 40, 69, 90, 99, 100, 400, 900, 1000, 9999] {
            let text = root_tag_text(MorphemeId(n), width);
            let path = decode_path(&text).expect("decodes");
            assert_eq!(
                path,
                vec![(true, MorphemeId(n))],
                "round trip failed for n={n} width={width} text={text:?}"
            );
        }
    }
}

#[test]
fn tag_text_round_trips_through_decode_path() {
    let width = 4;
    let s = format!(
        "pa{}ka{}ta{}",
        tag_text("M", 5, width),
        tag_text("R", 12, width),
        tag_text("M", 7, width)
    );
    let path = decode_path(&s).expect("decodes");
    assert_eq!(
        path,
        vec![
            (false, MorphemeId(5)),
            (true, MorphemeId(12)),
            (false, MorphemeId(7)),
        ]
    );
}

#[test]
fn decode_path_no_tags_is_empty_not_none() {
    assert_eq!(decode_path("plainword"), Some(vec![]));
}

#[test]
fn decode_path_rejects_malformed_tag() {
    assert_eq!(decode_path("abc<X:0001>"), None);
    assert_eq!(decode_path("abc<R:00a1>"), None);
    assert_eq!(decode_path("abc<R:0001"), None);
}

#[test]
fn to_candidates_no_root_yields_minus_one() {
    let path: RawPath = vec![(false, MorphemeId(3)), (false, MorphemeId(4))];
    let cands = to_candidates(&path);
    assert_eq!(
        cands,
        vec![Candidate {
            morphemes: vec![MorphemeId(3), MorphemeId(4)],
            root_index: -1,
        }]
    );
}

#[test]
fn to_candidates_single_root() {
    let path: RawPath = vec![
        (false, MorphemeId(1)),
        (true, MorphemeId(2)),
        (false, MorphemeId(3)),
    ];
    let cands = to_candidates(&path);
    assert_eq!(
        cands,
        vec![Candidate {
            morphemes: vec![MorphemeId(1), MorphemeId(2), MorphemeId(3)],
            root_index: 1,
        }]
    );
}

#[test]
fn to_candidates_collapses_adjacent_repeated_rule_application() {
    let path: RawPath = vec![
        (true, MorphemeId(15)),
        (false, MorphemeId(1)),
        (false, MorphemeId(1)),
    ];
    let cands = to_candidates(&path);
    assert_eq!(
        cands,
        vec![Candidate {
            morphemes: vec![MorphemeId(15), MorphemeId(1)],
            root_index: 0,
        }]
    );
}

#[test]
fn to_candidates_does_not_collapse_non_adjacent_repeats() {
    let path: RawPath = vec![
        (true, MorphemeId(15)),
        (false, MorphemeId(1)),
        (false, MorphemeId(2)),
        (false, MorphemeId(1)),
    ];
    let cands = to_candidates(&path);
    assert_eq!(
        cands,
        vec![Candidate {
            morphemes: vec![MorphemeId(15), MorphemeId(1), MorphemeId(2), MorphemeId(1)],
            root_index: 0,
        }]
    );
}

#[test]
fn to_candidates_compound_splits_per_root() {
    let path: RawPath = vec![(true, MorphemeId(10)), (true, MorphemeId(20))];
    let cands = to_candidates(&path);
    assert_eq!(
        cands,
        vec![
            Candidate {
                morphemes: vec![MorphemeId(10), MorphemeId(20)],
                root_index: 0,
            },
            Candidate {
                morphemes: vec![MorphemeId(10), MorphemeId(20)],
                root_index: 1,
            },
        ]
    );
}

fn root(n: u32) -> (bool, MorphemeId) {
    (true, MorphemeId(n))
}

fn affix(n: u32) -> (bool, MorphemeId) {
    (false, MorphemeId(n))
}

fn visit_all(path: &RawPath) -> Vec<(Candidate, CandidateOrigin)> {
    let mut seen = Vec::new();
    let flow = visit_candidate_superset(path, |c, origin| {
        seen.push((c, origin));
        ControlFlow::Continue(())
    });
    assert_eq!(flow, ControlFlow::Continue(()));
    seen
}

fn identities(path: &RawPath) -> Vec<(Vec<u32>, i32)> {
    let mut out: Vec<_> = visit_all(path)
        .into_iter()
        .map(|(c, _)| (c.morphemes.iter().map(|m| m.0).collect(), c.root_index))
        .collect();
    out.sort();
    out
}

/// A path of all-distinct tags has one subsequence, so the relation is exactly the legacy candidates.
#[test]
fn superset_of_distinct_tags_is_the_legacy_candidates() {
    for path in [
        vec![],
        vec![affix(3), affix(4)],
        vec![affix(1), root(2), affix(3)],
        vec![root(10), root(20)],
    ] {
        let visited = visit_all(&path);
        let legacy = to_candidates(&path);
        assert_eq!(
            visited.iter().map(|(c, _)| c.clone()).collect::<Vec<_>>(),
            legacy
        );
        assert!(visited.iter().all(|(_, o)| *o == CandidateOrigin::Legacy));
    }
}

/// The two-allomorph loss: two equal root tags may be one allomorph reused (`[m]`) or two allomorphs (`[m, m]`, either head); the relation offers all three and labels the legacy one.
#[test]
fn superset_of_a_repeated_root_offers_one_and_two_occurrences() {
    let path: RawPath = vec![root(7), root(7)];
    let visited = visit_all(&path);
    assert_eq!(
        visited
            .iter()
            .map(|(c, o)| (c.morphemes.len(), c.root_index, *o))
            .collect::<Vec<_>>(),
        vec![
            (1, 0, CandidateOrigin::Legacy),
            (2, 0, CandidateOrigin::Recovered),
            (2, 1, CandidateOrigin::Recovered),
        ]
    );
}

/// A repeat separated by other tags, which adjacency never merges, can still be merged away or kept; every distinct tag stays represented.
#[test]
fn superset_merges_non_adjacent_repeats_but_never_drops_a_distinct_tag() {
    let path: RawPath = vec![root(15), affix(1), affix(2), affix(1)];
    let got = identities(&path);
    let expected: Vec<(Vec<u32>, i32)> = vec![
        (vec![15, 1, 2], 0),
        (vec![15, 1, 2, 1], 0),
        (vec![15, 2, 1], 0),
    ];
    assert_eq!(got, expected);
    for (c, _) in visit_all(&path) {
        for needed in [15, 1, 2] {
            assert!(c.morphemes.contains(&MorphemeId(needed)));
        }
    }
}

/// A run of `k` equal root tags yields `k` lengths and `k(k+1)/2` head candidates, not `2^k` index subsets.
#[test]
fn superset_run_of_equal_tags_has_one_sequence_per_length() {
    for k in 1..=6usize {
        let path: RawPath = vec![root(5); k];
        let visited = visit_all(&path);
        assert_eq!(visited.len(), k * (k + 1) / 2, "k = {k}");
        let mut keys: Vec<_> = visited
            .iter()
            .map(|(c, _)| (c.morphemes.clone(), c.root_index))
            .collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), visited.len(), "duplicate candidate at k = {k}");
    }
}

/// Every owner-style canonical projection (a first-occurrence subsequence that keeps each distinct tag) belongs to the relation, over every path of length <= 6 on three tags.
#[test]
fn superset_contains_every_distinct_tag_preserving_subsequence() {
    let alphabet = [root(1), affix(2), affix(3)];
    let mut paths: Vec<RawPath> = vec![vec![]];
    let mut frontier: Vec<RawPath> = vec![vec![]];
    for _ in 0..6 {
        let mut next = Vec::new();
        for base in &frontier {
            for tag in alphabet {
                let mut extended = base.clone();
                extended.push(tag);
                next.push(extended);
            }
        }
        paths.extend(next.iter().cloned());
        frontier = next;
    }
    for path in paths {
        let relation = identities(&path);
        let mut distinct = path.clone();
        distinct.sort();
        distinct.dedup();
        // Brute force: every index subset that keeps all distinct tags.
        let n = path.len();
        let mut brute: Vec<(Vec<u32>, i32)> = Vec::new();
        for mask in 0u32..(1 << n) {
            let q: RawPath = (0..n)
                .filter(|i| mask & (1 << i) != 0)
                .map(|i| path[i])
                .collect();
            if distinct.iter().all(|t| q.contains(t)) {
                for c in candidates_of(&q) {
                    brute.push((c.morphemes.iter().map(|m| m.0).collect(), c.root_index));
                }
            }
        }
        brute.sort();
        brute.dedup();
        assert_eq!(relation, brute, "path {path:?}");
        let legacy: Vec<_> = to_candidates(&path)
            .into_iter()
            .map(|c| {
                (
                    c.morphemes.iter().map(|m| m.0).collect::<Vec<u32>>(),
                    c.root_index,
                )
            })
            .collect();
        for l in legacy {
            assert!(relation.contains(&l), "legacy candidate lost for {path:?}");
        }
    }
}

/// `Break` stops the traversal immediately and is returned to the caller.
#[test]
fn superset_stops_when_the_visitor_breaks() {
    let path: RawPath = vec![root(5); 8];
    let mut calls = 0;
    let flow = visit_candidate_superset(&path, |_, _| {
        calls += 1;
        if calls == 4 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    assert_eq!(flow, ControlFlow::Break(()));
    assert_eq!(calls, 4);
}
