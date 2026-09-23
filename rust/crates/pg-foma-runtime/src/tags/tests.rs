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
