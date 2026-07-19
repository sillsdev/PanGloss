//! Acceptance-gate tests for the M2 FST engine.
//!
//! Correctness = "the automaton accepts exactly the right strings and reports the right capture
//! spans." Expected results are reasoned independently (hand-written oracles / by-hand spans),
//! not read back from the engine. The alphabet is one-hot symbol bits in lane 0:
//! `a = 0b001, b = 0b010, c = 0b100`; a natural class is the union of those bits.

use pg_fst::{CompileInput, CompileNode, Direction, FstResult, Segment, Transduce, ENTIRE_MATCH};

/// An optional captured `(start, end)` span for two groups (ambiguous-capture assertion).
type SpanPair = (Option<(i32, i32)>, Option<(i32, i32)>);

// --- alphabet helpers -------------------------------------------------------------------------

const A: u8 = 0;
const B: u8 = 1;
const C: u8 = 2;

/// One-hot constraint for a single symbol.
fn sym(i: u8) -> Vec<u64> {
    vec![1u64 << i]
}
/// A natural class from a set of symbols (their bits OR'd).
fn cls(syms: &[u8]) -> Vec<u64> {
    vec![syms.iter().fold(0u64, |acc, &s| acc | (1u64 << s))]
}
fn seg(i: u8) -> Segment {
    Segment::new(sym(i))
}
fn input(s: &[u8]) -> Vec<Segment> {
    s.iter().map(|&i| seg(i)).collect()
}

/// Enumerate all strings over {A,B,C} of length `0..=max_len`.
fn enumerate(max_len: usize) -> Vec<Vec<u8>> {
    let mut all = vec![vec![]];
    let mut frontier = vec![vec![]];
    for _ in 0..max_len {
        let mut next = Vec::new();
        for s in &frontier {
            for sym in [A, B, C] {
                let mut t = s.clone();
                t.push(sym);
                next.push(t);
            }
        }
        all.extend(next.iter().cloned());
        frontier = next;
    }
    all
}

/// Whole-string acceptance (anchored both ends).
fn accepts_whole(fst: &pg_fst::Fst, s: &[u8]) -> bool {
    if s.is_empty() {
        return false; // our patterns are all non-empty; empty input yields no traversal
    }
    Transduce::new(fst, input(s)).anchored(true, true).accepts()
}

/// Compile a pattern both determinized and epsilon-removed (nondeterministic).
fn both(nodes: Vec<CompileNode>) -> (pg_fst::Fst, pg_fst::Fst) {
    (
        CompileInput::new(nodes.clone())
            .deterministic(true)
            .compile(),
        CompileInput::new(nodes).deterministic(false).compile(),
    )
}

// =============================================================================================
// Class 1: hand-built patterns, accept/reject over enumerated strings (independent oracle).
// =============================================================================================

/// Assert both the determinized and the epsilon-removed automaton accept exactly the strings the
/// oracle accepts, over the whole enumerated input space. This simultaneously proves language
/// correctness (vs an independent oracle) and determinization language-equivalence (det == nondet).
fn check_language(nodes: Vec<CompileNode>, max_len: usize, oracle: impl Fn(&[u8]) -> bool) {
    let (det, nondet) = both(nodes);
    for s in enumerate(max_len) {
        if s.is_empty() {
            continue;
        }
        let want = oracle(&s);
        assert_eq!(accepts_whole(&det, &s), want, "det disagrees on {s:?}");
        assert_eq!(
            accepts_whole(&nondet, &s),
            want,
            "nondet disagrees on {s:?}"
        );
    }
}

#[test]
fn lang_a_bstar_c() {
    // a b* c
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Quantifier {
            min: 0,
            max: None,
            children: vec![CompileNode::Constraint(sym(B))],
        },
        CompileNode::Constraint(sym(C)),
    ];
    check_language(nodes, 5, |s| {
        s.len() >= 2
            && s[0] == A
            && *s.last().unwrap() == C
            && s[1..s.len() - 1].iter().all(|&x| x == B)
    });
}

#[test]
fn lang_alternation_then_c() {
    // (a|b) c
    let nodes = vec![
        CompileNode::Alternation(vec![
            vec![CompileNode::Constraint(sym(A))],
            vec![CompileNode::Constraint(sym(B))],
        ]),
        CompileNode::Constraint(sym(C)),
    ];
    check_language(nodes, 4, |s| {
        s.len() == 2 && (s[0] == A || s[0] == B) && s[1] == C
    });
}

#[test]
fn lang_optional_middle() {
    // a b? c
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Quantifier {
            min: 0,
            max: Some(1),
            children: vec![CompileNode::Constraint(sym(B))],
        },
        CompileNode::Constraint(sym(C)),
    ];
    check_language(nodes, 4, |s| s == [A, C] || s == [A, B, C]);
}

#[test]
fn lang_bounded_quantifier() {
    // a{2,3}  -> "aa" or "aaa"
    let nodes = vec![CompileNode::Quantifier {
        min: 2,
        max: Some(3),
        children: vec![CompileNode::Constraint(sym(A))],
    }];
    check_language(nodes, 5, |s| s == [A, A] || s == [A, A, A]);
}

#[test]
fn lang_overlapping_natural_classes() {
    // [ab][bc] -- overlapping classes stress determinization's negated-condition subsets.
    let nodes = vec![
        CompileNode::Constraint(cls(&[A, B])),
        CompileNode::Constraint(cls(&[B, C])),
    ];
    check_language(nodes, 3, |s| {
        s.len() == 2 && (s[0] == A || s[0] == B) && (s[1] == B || s[1] == C)
    });
}

// =============================================================================================
// Class 2: determinization / epsilon-removal parity, incl. captured spans over the input space.
// =============================================================================================

/// Collect the set of *entire-match* spans found (unanchored, all matches) — used to confirm the
/// determinized and nondeterministic automata not only accept the same language but report the
/// same match spans (the advisor's "compare captured spans, not just accept/reject").
fn entire_spans(fst: &pg_fst::Fst, s: &[u8]) -> Vec<(i32, i32)> {
    let results = Transduce::new(fst, input(s)).all_matches();
    let mut spans: Vec<(i32, i32)> = results
        .iter()
        .filter_map(|r| fst.get_offsets(ENTIRE_MATCH, &r.registers))
        .collect();
    spans.sort();
    spans.dedup();
    spans
}

#[test]
fn det_and_nondet_report_same_entire_spans() {
    // [ab][bc] again, but now check *spans* over all start positions.
    let nodes = vec![
        CompileNode::Constraint(cls(&[A, B])),
        CompileNode::Constraint(cls(&[B, C])),
    ];
    let (det, nondet) = both(nodes);
    for s in enumerate(4) {
        if s.is_empty() {
            continue;
        }
        assert_eq!(
            entire_spans(&det, &s),
            entire_spans(&nondet, &s),
            "spans differ on {s:?}"
        );
    }
}

// =============================================================================================
// Class 3: registers / capture groups — exact start/end offsets reasoned by hand.
// =============================================================================================

#[test]
fn capture_simple_group_span() {
    // a (g:b) c ; on "abc" the group captures the middle segment: offsets (1, 2).
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Group {
            name: "g".into(),
            children: vec![CompileNode::Constraint(sym(B))],
        },
        CompileNode::Constraint(sym(C)),
    ];
    // capture groups take the nondeterministic path in HermitCrab (AllSubmatches); test that path.
    let fst = CompileInput::new(nodes).deterministic(false).compile();
    let res = Transduce::new(&fst, input(&[A, B, C]))
        .anchored(true, true)
        .first_match()
        .expect("should match abc");
    assert_eq!(fst.get_offsets(ENTIRE_MATCH, &res.registers), Some((0, 3)));
    assert_eq!(fst.get_offsets("g", &res.registers), Some((1, 2)));
}

#[test]
fn capture_group_over_multiple_segments() {
    // a (g:b b) c on "abbc": group spans (1,3).
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Group {
            name: "g".into(),
            children: vec![
                CompileNode::Constraint(sym(B)),
                CompileNode::Constraint(sym(B)),
            ],
        },
        CompileNode::Constraint(sym(C)),
    ];
    let fst = CompileInput::new(nodes).deterministic(false).compile();
    let res = Transduce::new(&fst, input(&[A, B, B, C]))
        .anchored(true, true)
        .first_match()
        .expect("should match abbc");
    assert_eq!(fst.get_offsets("g", &res.registers), Some((1, 3)));
}

/// The discriminating ambiguous-capture test (advisor's load-bearing case): overlapping
/// alternations put the same capture group at different indices along different parses. We assert
/// the *set* of (g1, g2) span pairs equals the two hand-reasoned parses of "abc":
///   [a][bc] -> g1=(0,1), g2=(1,3)
///   [ab][c] -> g1=(0,2), g2=(2,3)
#[test]
fn capture_ambiguous_alternation_spans() {
    let nodes = vec![
        CompileNode::Group {
            name: "g1".into(),
            children: vec![CompileNode::Alternation(vec![
                vec![CompileNode::Constraint(sym(A))],
                vec![
                    CompileNode::Constraint(sym(A)),
                    CompileNode::Constraint(sym(B)),
                ],
            ])],
        },
        CompileNode::Group {
            name: "g2".into(),
            children: vec![CompileNode::Alternation(vec![
                vec![
                    CompileNode::Constraint(sym(B)),
                    CompileNode::Constraint(sym(C)),
                ],
                vec![CompileNode::Constraint(sym(C))],
            ])],
        },
    ];
    let fst = CompileInput::new(nodes).deterministic(false).compile();
    let results = Transduce::new(&fst, input(&[A, B, C]))
        .anchored(true, true)
        .all_matches();
    let mut pairs: Vec<SpanPair> = results
        .iter()
        .map(|r| {
            (
                fst.get_offsets("g1", &r.registers),
                fst.get_offsets("g2", &r.registers),
            )
        })
        .collect();
    pairs.sort();
    pairs.dedup();
    let mut want = vec![(Some((0, 1)), Some((1, 3))), (Some((0, 2)), Some((2, 3)))];
    want.sort();
    assert_eq!(
        pairs, want,
        "ambiguous capture spans wrong (tag reindexing?)"
    );
}

#[test]
fn capture_group_in_quantifier_last_iteration() {
    // (g:a)+ on "aaa": the group register holds the LAST iteration's span (2,3) in the single best
    // match (greedy, longest). Reasoned from the register overwrite each iteration.
    let nodes = vec![CompileNode::Quantifier {
        min: 1,
        max: None,
        children: vec![CompileNode::Group {
            name: "g".into(),
            children: vec![CompileNode::Constraint(sym(A))],
        }],
    }];
    let fst = CompileInput::new(nodes).deterministic(false).compile();
    let res = Transduce::new(&fst, input(&[A, A, A]))
        .anchored(true, true)
        .first_match()
        .expect("should match aaa");
    assert_eq!(fst.get_offsets(ENTIRE_MATCH, &res.registers), Some((0, 3)));
    assert_eq!(fst.get_offsets("g", &res.registers), Some((2, 3)));
}

// =============================================================================================
// Class 4: result ordering — ResultCompare on both determinism branches and both directions.
// =============================================================================================

/// Build a trivial FST just to obtain a `Transduce` with the desired (deterministic, direction)
/// so we can exercise `ResultCompare` in isolation on hand-built `FstResult`s.
fn comparator(deterministic: bool, dir: Direction) -> pg_fst::Fst {
    CompileInput::new(vec![CompileNode::Constraint(sym(A))])
        .deterministic(deterministic)
        .compile_with_direction(dir)
}

fn mk_result(priority: i32, next_ann: i32, order: usize) -> FstResult {
    FstResult {
        id: None,
        registers: Vec::new(),
        priority,
        is_lazy: false,
        next_ann,
        order,
    }
}

#[test]
fn result_compare_accept_priority_first() {
    let fst = comparator(true, Direction::LeftToRight);
    let t = Transduce::new(&fst, Vec::new());
    // lower accept priority sorts first regardless of length.
    let x = mk_result(0, 1, 5);
    let y = mk_result(1, 9, 0);
    assert_eq!(t.result_compare(&x, &y), std::cmp::Ordering::Less);
}

#[test]
fn result_compare_longer_match_first_ltor() {
    // Same accept priority: -NextAnnotation => larger next_ann (longer match) sorts first.
    let fst = comparator(true, Direction::LeftToRight);
    let t = Transduce::new(&fst, Vec::new());
    let longer = mk_result(0, 3, 1);
    let shorter = mk_result(0, 1, 0);
    assert_eq!(
        t.result_compare(&longer, &shorter),
        std::cmp::Ordering::Less
    );
}

#[test]
fn result_compare_direction_flips_length_preference() {
    let fst = comparator(true, Direction::RightToLeft);
    let t = Transduce::new(&fst, Vec::new());
    let longer = mk_result(0, 3, 1);
    let shorter = mk_result(0, 1, 0);
    // RightToLeft negates the length comparison: shorter next_ann now sorts first.
    assert_eq!(
        t.result_compare(&longer, &shorter),
        std::cmp::Ordering::Greater
    );
}

#[test]
fn result_compare_order_is_final_tiebreak() {
    // Nondeterministic branch, equal accept priority AND equal next_ann: falls straight through
    // to Order (T-C tore out the Priorities zip that used to sit between these — see
    // rust-optimizations-phase2.md and result_compare's doc comment).
    let fst = comparator(false, Direction::LeftToRight);
    let t = Transduce::new(&fst, Vec::new());
    let x = mk_result(0, 2, 2);
    let y = mk_result(0, 2, 7);
    assert_eq!(t.result_compare(&x, &y), std::cmp::Ordering::Less); // 2 < 7
}

/// Entire-match spans in the *raw* `all_matches()` order (no re-sort) — used to verify the
/// engine emits results in `ResultCompare` order end-to-end.
fn entire_spans_in_order(
    fst: &pg_fst::Fst,
    segs: Vec<Segment>,
    start: bool,
    end: bool,
) -> Vec<(i32, i32)> {
    Transduce::new(fst, segs)
        .anchored(start, end)
        .all_matches()
        .iter()
        .filter_map(|r| fst.get_offsets(ENTIRE_MATCH, &r.registers))
        .collect()
}

#[test]
fn ordering_deterministic_end_to_end() {
    // a b?  on "ab", start-anchored: matches "ab"(next_ann=2) and "a"(next_ann=1). Deterministic
    // ResultCompare: same accept priority, -NextAnnotation => longer first. Hand-reasoned order.
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Quantifier {
            min: 0,
            max: Some(1),
            children: vec![CompileNode::Constraint(sym(B))],
        },
    ];
    let fst = CompileInput::new(nodes).deterministic(true).compile();
    assert_eq!(
        entire_spans_in_order(&fst, input(&[A, B]), true, false),
        vec![(0, 2), (0, 1)]
    );
}

#[test]
fn ordering_nondeterministic_end_to_end() {
    // a a?  on "aa", start-anchored, epsilon-removed (nondeterministic). Two matches "aa" and "a"
    // with different NextAnnotation, so the nondeterministic branch orders by -NextAnnotation:
    // longer first.
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Quantifier {
            min: 0,
            max: Some(1),
            children: vec![CompileNode::Constraint(sym(A))],
        },
    ];
    let fst = CompileInput::new(nodes).deterministic(false).compile();
    let results = Transduce::new(&fst, input(&[A, A]))
        .anchored(true, false)
        .all_matches();
    let spans: Vec<_> = results
        .iter()
        .filter_map(|r| fst.get_offsets(ENTIRE_MATCH, &r.registers))
        .collect();
    assert_eq!(spans, vec![(0, 2), (0, 1)]);
}

#[test]
fn ordering_nondeterministic_tie_falls_through_to_order_end_to_end() {
    // Two parses of "aa" with the SAME entire span (0,2) but the capture group at different
    // positions -> equal NextAnnotation. C# breaks this with a `Priorities` zip (Fst.cs:431);
    // T-C tore that out as byte-parity-only (rust-optimizations-phase2.md), so this now falls
    // straight through to Order. Both survive Distinct (different registers); we assert the
    // wiring still produces a well-defined, self-consistent order.
    let nodes = vec![CompileNode::Alternation(vec![
        vec![
            CompileNode::Group {
                name: "g".into(),
                children: vec![CompileNode::Constraint(sym(A))],
            },
            CompileNode::Constraint(sym(A)),
        ],
        vec![
            CompileNode::Constraint(sym(A)),
            CompileNode::Group {
                name: "g".into(),
                children: vec![CompileNode::Constraint(sym(A))],
            },
        ],
    ])];
    let fst = CompileInput::new(nodes).deterministic(false).compile();
    let results = Transduce::new(&fst, input(&[A, A]))
        .anchored(true, true)
        .all_matches();
    let t = Transduce::new(&fst, Vec::new()); // for result_compare (uses fst determinism/direction)
                                              // both parses, both with entire span (0,2) but different g capture.
    let gs: std::collections::BTreeSet<_> = results
        .iter()
        .filter_map(|r| fst.get_offsets("g", &r.registers))
        .collect();
    assert_eq!(gs, [(0, 1), (1, 2)].into_iter().collect());
    // Emitted in nondecreasing ResultCompare order (single start position -> one sorted block).
    for w in results.windows(2) {
        assert_ne!(t.result_compare(&w[0], &w[1]), std::cmp::Ordering::Greater);
    }
}

#[test]
fn integration_longest_match_wins_single() {
    // a b?  on "ab" (start-anchored, not end-anchored): matches "a" and "ab"; single best = "ab".
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Quantifier {
            min: 0,
            max: Some(1),
            children: vec![CompileNode::Constraint(sym(B))],
        },
    ];
    let fst = CompileInput::new(nodes).deterministic(true).compile();
    let res = Transduce::new(&fst, input(&[A, B]))
        .anchored(true, false)
        .first_match()
        .expect("matches");
    assert_eq!(fst.get_offsets(ENTIRE_MATCH, &res.registers), Some((0, 2)));
}

// =============================================================================================
// Structural sanity: CSR shape and multi-start matching.
// =============================================================================================

#[test]
fn unanchored_finds_match_at_any_start() {
    // pattern "b c"; input "a b c" -> one entire match at (1,3).
    let nodes = vec![
        CompileNode::Constraint(sym(B)),
        CompileNode::Constraint(sym(C)),
    ];
    let fst = CompileInput::new(nodes).deterministic(true).compile();
    let res = Transduce::new(&fst, input(&[A, B, C])).all_matches();
    let spans: Vec<_> = res
        .iter()
        .filter_map(|r| fst.get_offsets(ENTIRE_MATCH, &r.registers))
        .collect();
    assert_eq!(spans, vec![(1, 3)]);
}

// =============================================================================================
// Class 5: RightToLeft traversal (Fst.Transduce `_dir` handling; AnalysisRewriteRule runs the
// matcher reversed). The SAME forward-built automaton is walked from the opposite physical end,
// so an R2L walk accepts the *reversed* reading of the input. All expected values below are
// hand-reasoned PHYSICAL spans; `get_offsets` performs the R2L un-swap (Fst.cs:128-137).
// =============================================================================================

fn compile_dir(nodes: Vec<CompileNode>, det: bool, dir: Direction) -> pg_fst::Fst {
    CompileInput::new(nodes)
        .deterministic(det)
        .compile_with_direction(dir)
}

/// Whole-string acceptance for an FST of arbitrary direction (anchored both ends).
fn whole(fst: &pg_fst::Fst, s: &[u8]) -> bool {
    Transduce::new(fst, input(s)).anchored(true, true).accepts()
}

fn entire_span_first(
    fst: &pg_fst::Fst,
    segs: Vec<Segment>,
    start: bool,
    end: bool,
) -> Option<(i32, i32)> {
    Transduce::new(fst, segs)
        .anchored(start, end)
        .first_match()
        .and_then(|r| fst.get_offsets(ENTIRE_MATCH, &r.registers))
}

/// GUARD #3 (asymmetric language). Pattern `a b c` compiled once, walked both directions. L2R
/// consumes the input left-to-right so it accepts physical `a b c`; R2L walks the same automaton
/// from the right, so traversal index 0 is the physically-last segment — it accepts physical
/// `c b a` and REJECTS `a b c`. This proves R2L walks right-to-left rather than reproducing the
/// L2R set. Checked for both determinized and epsilon-removed compilations.
#[test]
fn rtl_asymmetric_language_walks_right_to_left() {
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Constraint(sym(B)),
        CompileNode::Constraint(sym(C)),
    ];
    for det in [true, false] {
        let ltr = compile_dir(nodes.clone(), det, Direction::LeftToRight);
        let rtl = compile_dir(nodes.clone(), det, Direction::RightToLeft);

        // L2R: accepts "a b c", rejects the reversal.
        assert!(whole(&ltr, &[A, B, C]), "L2R must accept a b c (det={det})");
        assert!(
            !whole(&ltr, &[C, B, A]),
            "L2R must reject c b a (det={det})"
        );

        // R2L: accepts the physical reversal "c b a", rejects "a b c".
        assert!(whole(&rtl, &[C, B, A]), "R2L must accept c b a (det={det})");
        assert!(
            !whole(&rtl, &[A, B, C]),
            "R2L must reject a b c (det={det})"
        );

        // The whole-string entire span is (0,3) regardless of direction.
        assert_eq!(
            entire_span_first(&ltr, input(&[A, B, C]), true, true),
            Some((0, 3))
        );
        assert_eq!(
            entire_span_first(&rtl, input(&[C, B, A]), true, true),
            Some((0, 3))
        );
    }
}

/// GUARD #1 (anchoring flips start<->end under reversal). Pattern `a b` (automaton) R2L-accepts
/// physical `b a`. `startAnchor` under R2L binds to the physical END (rightmost, traversal index
/// 0); `endAnchor` binds to the physical START (leftmost, consumed last).
#[test]
fn rtl_start_anchor_binds_physical_end() {
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Constraint(sym(B)),
    ];
    let fst = compile_dir(nodes, true, Direction::RightToLeft);

    // "b a": R2L traversal is [a (phys 1), b (phys 0)]; rightmost segment 'a' matches the first
    // arc, so a start-anchored (rightmost-anchored) match succeeds.
    assert!(
        Transduce::new(&fst, input(&[B, A]))
            .anchored(true, false)
            .accepts(),
        "b a start-anchored R2L"
    );

    // "b a c": rightmost segment is now 'c'. Start-anchored R2L must begin there -> arc 'a' vs 'c'
    // fails, so NO start-anchored match. But unanchored still finds "b a" at physical (0,2). This
    // is the discriminator: the anchor bound to the physical END, not the L2R start.
    assert!(
        !Transduce::new(&fst, input(&[B, A, C]))
            .anchored(true, false)
            .accepts(),
        "b a c start-anchored R2L rejects"
    );
    assert_eq!(
        entire_span_first(&fst, input(&[B, A, C]), false, false),
        Some((0, 2)),
        "b a c unanchored R2L finds (0,2)"
    );
}

#[test]
fn rtl_end_anchor_binds_physical_start() {
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Constraint(sym(B)),
    ];
    let fst = compile_dir(nodes, true, Direction::RightToLeft);

    // "b a": consuming both segments ends at the physically-leftmost segment 'b' (traversal end),
    // so an end-anchored (leftmost-anchored) match succeeds. Physical entire span (0,2).
    assert!(
        Transduce::new(&fst, input(&[B, A]))
            .anchored(false, true)
            .accepts(),
        "b a end-anchored R2L"
    );
    assert_eq!(
        entire_span_first(&fst, input(&[B, A]), false, true),
        Some((0, 2))
    );

    // "c b a": R2L matches "a b" consuming physical segments 2,1, ending at traversal index 2 with
    // the leftmost 'c' still unconsumed -> end-anchor requires reaching data end, so NO match.
    // Unanchored still matches at physical (1,3). Proves end-anchor bound to the physical START.
    assert!(
        !Transduce::new(&fst, input(&[C, B, A]))
            .anchored(false, true)
            .accepts(),
        "c b a end-anchored R2L rejects"
    );
    assert_eq!(
        entire_span_first(&fst, input(&[C, B, A]), false, false),
        Some((1, 3)),
        "c b a unanchored R2L finds (1,3)"
    );
}

/// GUARD #2 (capture offsets under reversal). Pattern `a (g:b) c` walked R2L against physical
/// `c b a`. Register offsets are direction-relative (measured from the right), and `get_offsets`
/// un-swaps them. Hand-reasoned PHYSICAL spans: the whole match covers `[0,3)` and group `g`
/// wraps the single 'b', which sits at physical index 1 occupying `[1,2)`. A start<->end offset
/// direction bug would surface here (e.g. `(2,3)` or a swapped/None span).
#[test]
fn rtl_capture_group_offsets() {
    let nodes = vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Group {
            name: "g".into(),
            children: vec![CompileNode::Constraint(sym(B))],
        },
        CompileNode::Constraint(sym(C)),
    ];
    // capture groups take the nondeterministic path in HermitCrab (AllSubmatches).
    let fst = compile_dir(nodes, false, Direction::RightToLeft);
    let res = Transduce::new(&fst, input(&[C, B, A]))
        .anchored(true, true)
        .first_match()
        .expect("R2L should match c b a");
    assert_eq!(
        fst.get_offsets(ENTIRE_MATCH, &res.registers),
        Some((0, 3)),
        "entire span physical (0,3)"
    );
    assert_eq!(
        fst.get_offsets("g", &res.registers),
        Some((1, 2)),
        "group g physical (1,2)"
    );
}

/// GUARD #4 (hand-built automaton, det + nondet match sets under R2L). Pattern `(a|b) c` R2L
/// accepts the reversed readings: physical `c a` and `c b` (the automaton's first arc `(a|b)` is
/// matched against the rightmost segment, `c` against the leftmost). Enumerate all length-2
/// strings and compare the R2L accept set to a hand-reasoned oracle, for both compilations.
#[test]
fn rtl_hand_built_match_set() {
    let nodes = vec![
        CompileNode::Alternation(vec![
            vec![CompileNode::Constraint(sym(A))],
            vec![CompileNode::Constraint(sym(B))],
        ]),
        CompileNode::Constraint(sym(C)),
    ];
    // Oracle: R2L accepts physical `s` iff its reversal is in the L2R language {(a|b) c}, i.e.
    // s == [c, a] or s == [c, b].
    let oracle = |s: &[u8]| s.len() == 2 && s[0] == C && (s[1] == A || s[1] == B);
    for det in [true, false] {
        let rtl = compile_dir(nodes.clone(), det, Direction::RightToLeft);
        for s in enumerate(3) {
            if s.is_empty() {
                continue;
            }
            assert_eq!(
                whole(&rtl, &s),
                oracle(&s),
                "R2L match set wrong on {s:?} (det={det})"
            );
        }
    }
}

#[test]
fn csr_is_populated() {
    let fst = CompileInput::new(vec![
        CompileNode::Constraint(sym(A)),
        CompileNode::Constraint(sym(B)),
    ])
    .compile();
    assert!(fst.state_count() >= 3);
    assert!(fst.arc_count() >= 2);
    assert!(fst.is_deterministic());
}
