use super::*;

fn probe_segs(match_lane: &[u64], other_lane: &[u64]) -> Vec<Segment> {
    vec![
        Segment::new(other_lane.to_vec()),
        Segment::new(match_lane.to_vec()),
        Segment::optional(other_lane.to_vec()),
        Segment::new(match_lane.to_vec()),
        Segment::optional(other_lane.to_vec()),
        Segment::new(match_lane.to_vec()),
    ]
}

/// `LeftToRight`: each row's START offset is reliable (only the END can be widened by a following skip); real matches sit at seg positions 1, 3, 5, expecting adjacent pairs (1,3) and (3,5) via each row's start.
#[test]
fn group_offsets_survive_interposed_optional_ltr() {
    let match_lane = vec![0b01u64];
    let other_lane = vec![0b10u64];
    let segs = probe_segs(&match_lane, &other_lane);
    let (fst, names) = compile_lane_fst_grouped(
        &[match_lane.clone(), match_lane.clone()],
        Direction::LeftToRight,
        false,
    );

    let mut pairs: Vec<(i32, i32)> = Transduce::new(&fst, segs.clone())
        .all_matches()
        .iter()
        .map(|r| {
            let g0 = fst.get_offsets(&names[0], &r.registers).expect("g0 set").0;
            let g1 = fst.get_offsets(&names[1], &r.registers).expect("g1 set").0;
            (g0, g1)
        })
        .collect();
    pairs.sort_unstable();
    pairs.dedup();
    assert_eq!(
        pairs,
        vec![(1, 3), (3, 5)],
        "LTR: each row's START must resolve to the real match position"
    );
}

/// `RightToLeft` (the direction `ana_feature`'s target actually compiles under): node order is document-reversed and `Fst::get_offsets` swaps `(start,end)` back, so each row's END is reliable instead of START — same real-match positions (1,3,5) and pairs, now read via `.1 - 1`.
#[test]
fn group_offsets_survive_interposed_optional_rtl() {
    let match_lane = vec![0b01u64];
    let other_lane = vec![0b10u64];
    let segs = probe_segs(&match_lane, &other_lane);
    let (fst, names) = compile_lane_fst_grouped(
        &[match_lane.clone(), match_lane.clone()],
        Direction::RightToLeft,
        false,
    );

    let mut pairs: Vec<(i32, i32)> = Transduce::new(&fst, segs.clone())
        .all_matches()
        .iter()
        .map(|r| {
            let g0 = fst.get_offsets(&names[0], &r.registers).expect("g0 set").1 - 1;
            let g1 = fst.get_offsets(&names[1], &r.registers).expect("g1 set").1 - 1;
            (g0, g1)
        })
        .collect();
    pairs.sort_unstable();
    pairs.dedup();
    assert_eq!(
        pairs,
        vec![(1, 3), (3, 5)],
        "RTL: each row's END-1 must resolve to the real match position"
    );
}
