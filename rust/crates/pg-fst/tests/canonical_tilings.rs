//! Canonical tilings of an optional run: matches that differ only in how many run nodes are skipped between two consumed run nodes are enumerated once. Alphabet: one-hot bits in lane 0 (`a=0b01,b=0b10`).

use pg_fst::{CompileInput, CompileNode, Fst, Segment, Transduce};

const A: u64 = 0b01;
const B: u64 = 0b10;

/// `B  A  A  B`, each element its own capture group `g0..g3`.
fn chain() -> Fst {
    let nodes = [B, A, A, B]
        .iter()
        .enumerate()
        .map(|(i, &bit)| CompileNode::Group {
            name: format!("g{i}"),
            children: vec![CompileNode::Constraint(vec![bit])],
        })
        .collect();
    CompileInput::new(nodes).deterministic(false).compile()
}

/// A required `b`, `run` optional `a`s, a required `b`, then a trailing optional run of `tail` more `a`s.
fn word(run: usize, tail: usize) -> Vec<Segment> {
    let mut segments = vec![Segment::new(vec![B])];
    segments.extend((0..run).map(|_| Segment::optional(vec![A])));
    segments.push(Segment::new(vec![B]));
    segments.extend((0..tail).map(|_| Segment::optional(vec![A])));
    segments
}

type Spans = Vec<(i32, i32)>;

fn matches(
    fst: &Fst,
    segments: Vec<Segment>,
    runs: Option<(Vec<u32>, Vec<bool>)>,
) -> (Vec<Spans>, usize) {
    let budget = pg_fst::work::WorkBudget::new(usize::MAX / 2);
    let _scope = budget.enter();
    let mut transduce = Transduce::new(fst, segments).anchored(true, true);
    if let Some((run_end, arc_ok)) = runs {
        transduce = transduce.interchangeable_runs(run_end, arc_ok);
    }
    let spans = transduce
        .all_matches()
        .iter()
        .map(|m| {
            (0..4)
                .map(|i| {
                    fst.get_offsets(&format!("g{i}"), &m.registers)
                        .expect("captured")
                })
                .collect()
        })
        .collect();
    (spans, budget.used())
}

/// Every arc that closes one group and opens another is a boundary the caller declares safe.
fn boundary_arcs(fst: &Fst) -> Vec<bool> {
    fst.arc_group_events()
        .expect("plain commands")
        .iter()
        .map(|(closes, opens)| closes.len() == 1 && opens.len() == 1)
        .collect()
}

#[test]
fn each_boundary_arc_closes_one_group_and_opens_the_next() {
    let fst = chain();
    let events = fst.arc_group_events().expect("plain commands");
    assert_eq!(events.len(), fst.arc_count());
    assert_eq!(events[0], (vec!["g0"], vec!["g1"]));
    assert_eq!(events[1], (vec!["g1"], vec!["g2"]));
    assert_eq!(events[2], (vec!["g2"], vec!["g3"]));
    assert_eq!(boundary_arcs(&fst), [true, true, true, false]);
}

/// The kept matches are exactly the exhaustive matches with no skipped node between the two consumed run nodes, in the exhaustive order, and each dropped match has a kept one with the same first and last span.
#[test]
fn canonical_run_tilings_keep_one_match_per_class() {
    let fst = chain();
    let arc_ok = boundary_arcs(&fst);
    for (run, tail) in [(2, 0), (6, 0), (6, 3), (9, 1)] {
        let segments = word(run, tail);
        let n = segments.len();
        let mut run_end = vec![0u32; n];
        run_end[1..=run].fill(run as u32 + 1);
        let (exhaustive, exhaustive_work) = matches(&fst, segments.clone(), None);
        let (canonical, canonical_work) =
            matches(&fst, segments, Some((run_end.clone(), arc_ok.clone())));
        let want: Vec<Spans> = exhaustive
            .iter()
            .filter(|spans| spans[1].1 == spans[1].0 + 1 || spans[2].0 > run as i32)
            .cloned()
            .collect();
        assert_eq!(canonical, want, "run {run} tail {tail}");
        assert!(canonical.len() <= exhaustive.len());
        for dropped in &exhaustive {
            assert!(
                canonical.iter().any(|kept| kept[0] == dropped[0]
                    && kept[1].0 == dropped[1].0
                    && kept[3] == dropped[3]),
                "no kept match stands for {dropped:?}"
            );
        }
        if run >= 6 {
            assert!(canonical.len() * 2 < exhaustive.len());
            assert!(
                canonical_work * 2 < exhaustive_work,
                "{canonical_work} vs {exhaustive_work}"
            );
        }
    }
}

/// An arc that is not declared safe, or a segment outside every run, changes nothing.
#[test]
fn without_a_declared_boundary_or_a_run_nothing_is_dropped() {
    let fst = chain();
    let segments = word(6, 2);
    let n = segments.len();
    let (exhaustive, _) = matches(&fst, segments.clone(), None);
    let mut run_end = vec![0u32; n];
    run_end[1..=6].fill(7);
    let (no_arcs, _) = matches(
        &fst,
        segments.clone(),
        Some((run_end, vec![false; fst.arc_count()])),
    );
    assert_eq!(no_arcs, exhaustive);
    let (no_runs, _) = matches(&fst, segments, Some((vec![0; n], boundary_arcs(&fst))));
    assert_eq!(no_runs, exhaustive);
}
