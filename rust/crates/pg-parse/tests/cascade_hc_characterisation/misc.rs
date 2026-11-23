//! Seams the cascade backend leans on: root patterns, class members, spelling, normalisation, budgets (TEST 13, 19, 22, 29, 31, 33).

use super::fixture::*;
use pg_parse::ParseOptions;

/// TEST 13: a root allomorph with an optional or iterative non-boundary node is a lexical pattern; ordinary lookup never returns it, so no such shape reaches `is_match`.
#[test]
fn test_13_pattern_roots_are_not_looked_up_on_the_ordinary_path() {
    let g = Lang::new("ka")
        .stratum(
            Stratum::new("S")
                .entries(&(entry("P", "k[Any]*") + &entry("Q", "ka") + &entry("O", "k([Any])"))),
        )
        .load();
    let patterns: Vec<bool> = g
        .entries
        .iter()
        .map(|e| e.allomorphs[0].is_pattern)
        .collect();
    assert_eq!(
        patterns,
        [true, false, true],
        "iterative and optional non-boundary nodes make a pattern"
    );
    let m = morpher(&g);
    assert_eq!(m.lexical_patterns().len(), 2);
    assert_eq!(analyses(&g, "ka"), ["Q"], "only the ordinary root is found");
    assert!(
        analyses(&g, "kaa").is_empty(),
        "the pattern `k[Any]*` is not consulted"
    );
    assert!(analyses(&g, "k").is_empty(), "nor is `k([Any])`");
    let guessed = m.parse_word_opts("kaa", &ParseOptions::default().with_guess_root(true));
    assert!(
        guessed.guessed,
        "only the guess branch uses patterns, and confirm never turns it on"
    );
}

/// TEST 19: whether the loader accepts a boundary char-def as a member of a segment-list class.
#[test]
fn test_19_boundary_char_def_in_a_segment_list_class() {
    let mut lang = Lang::new("ak");
    lang.boundaries = boundary_def("plus", "+");
    lang.classes += &(sclass("ncMixed", &["a"]).replace(
        "</SegmentNaturalClass>",
        r#"<Segment segment="bplus" /></SegmentNaturalClass>"#,
    ));
    let xml = lang
        .stratum(Stratum::new("S").entries(&entry("K", "ak")))
        .xml();
    let g = pg_grammar::load(&xml)
        .expect("the loader accepts a boundary char-def as a member of a segment-list class");
    assert_eq!(analyses(&g, "ak"), ["K"]);
}

/// TEST 22: HC segments the typed word greedily, so a surface that spells as a digraph is read as the digraph.
#[test]
fn test_22_hc_segments_greedily_and_does_not_read_a_digraph_as_two_segments() {
    let mut lang = Lang::new("xab");
    lang.segments += &seg("ab", &[], &[]);
    let g = lang
        .stratum(
            Stratum::new("S")
                .order("unordered")
                .rules("mA mB", &(suffix("A", "a") + &suffix("B", "b")))
                .entries(&entry("X", "x")),
        )
        .load();
    assert_eq!(analyses(&g, "xa"), ["X+A"]);
    assert_eq!(analyses(&g, "xb"), ["X+B"]);
    assert!(
        analyses(&g, "xab").is_empty(),
        "`ab` is read as the one digraph segment, never as `a` then `b`"
    );
    assert_eq!(analyses(&g, "xba"), ["X+B+A"]);
}

/// TEST 29: an edge-only application keeps a boundary inside material an earlier step inserted.
#[test]
fn test_29_edge_only_application_keeps_a_boundary_inside_earlier_inserted_material() {
    let mut lang = Lang::new("kabx");
    lang.boundaries = boundary_def("plus", "+");
    lang.prules = rewrite("R", &lit("a"), &lit("x"), "", &bnd("plus"));
    let g = lang
        .stratum(
            Stratum::new("S")
                .rules("mS1 mS2", &(suffix("S1", "a+") + &suffix("S2", "b")))
                .prules("R")
                .entries(&entry("K", "k")),
        )
        .load();
    assert_eq!(
        generate(&g, "K", &["S1"]),
        ["kx"],
        "the boundary is at the word edge, so `a` is rewritten and the boundary is spent"
    );
    assert_eq!(analyses(&g, "kx"), ["K+S1"]);
    assert_eq!(
        analyses(&g, "kxb"),
        ["K+S1+S2"],
        "the boundary the first suffix inserted stays inside the word when `b` follows"
    );
    for w in ["kab", "ka"] {
        assert!(analyses(&g, w).is_empty(), "{w}");
    }
}

/// TEST 31: a concatenation of segment representations whose marks are out of canonical order is never accepted.
#[test]
fn test_31_non_canonical_mark_order_is_never_accepted() {
    let acute = "\u{301}";
    let dot = "\u{323}";
    let mut lang = Lang::new("");
    lang.segments = seg("a", &[], &[])
        + &seg(&format!("a{acute}"), &[], &[])
        + &seg(dot, &[], &[])
        + &seg(acute, &[], &[]);
    let g = lang
        .stratum(
            Stratum::new("S")
                .rules("mS mT", &(suffix("S", dot) + &suffix("T", acute)))
                .entries(&(entry("R", &format!("a{acute}")) + &entry("Q", &format!("a{dot}")))),
        )
        .load();
    let bad = generate(&g, "R", &["S"]);
    let good = generate(&g, "Q", &["T"]);
    // CONTRADICTS PLAN: TEST 31. HC's forward synthesis does produce the out-of-order concatenation (acute then dot below), and HC's analysis normalises its input to canonical order first, so the typed non-canonical string is accepted, as the canonical derivation `Q+T`. The plan's Units cut ("HC can never accept that concatenation") holds only for the cascade's own concatenation; a typed word must be normalised before the product reads it.
    assert_eq!(bad, [format!("a{acute}{dot}")]);
    assert_eq!(good, [format!("a{dot}{acute}")]);
    for form in [&bad[0], &good[0]] {
        assert_eq!(analyses(&g, form), ["Q+T"], "{form:?}");
    }
}

/// TEST 33: one `WordBudget` shared by restricted parses of different strings carries nothing between them but the work count and the latched flags.
#[test]
fn test_33_a_shared_word_budget_carries_only_work_and_latches_between_strings() {
    let g = Lang::new("kabs")
        .stratum(
            Stratum::new("S")
                .order("unordered")
                .rules("mA mB", &(suffix("A", "a") + &suffix("B", "b")))
                .entries(&(entry("K", "kab") + &entry("L", "kb"))),
        )
        .load();
    let m = morpher(&g);
    let words = ["kab", "kaba", "kbb", "kabab", "kba", "ks"];
    let opts = ParseOptions::default();
    let key = |o: &pg_parse::ParseOutcome| sorted(o);
    let fresh: Vec<_> = words
        .iter()
        .map(|w| m.parse_word_selected(w, &opts, None, None))
        .collect();
    // One budget over every word, in order and reversed.
    for order in [
        words.to_vec(),
        words.iter().rev().copied().collect::<Vec<_>>(),
    ] {
        let budget = m.word_budget();
        let mut total = 0usize;
        for w in &order {
            let shared = m.parse_word_selected_with_budget(w, &opts, None, None, &budget);
            let alone = &fresh[words.iter().position(|x| x == w).unwrap()];
            assert_eq!(
                key(&shared),
                key(alone),
                "{w}: analyses under a shared budget"
            );
            assert!(!shared.capped);
            total += alone.work_steps;
            assert_eq!(
                shared.work_steps, total,
                "{w}: work_steps is cumulative within the budget"
            );
        }
    }
    // A budget that latches stays latched for the next string.
    let m_tight = pg_parse::Morpher::new(&g, 40);
    let budget = m_tight.word_budget();
    let first = m_tight.parse_word_selected_with_budget("kabab", &opts, None, None, &budget);
    let second = m_tight.parse_word_selected_with_budget("kab", &opts, None, None, &budget);
    assert!(first.capped, "the tight budget latches on the first string");
    assert!(
        second.capped && second.analyses.is_empty(),
        "and stays latched for the next string"
    );
}

/// A small ambiguous grammar for the restricted-parse laws: two strata, an unordered stratum with a template, a compounding rule and an obligatory rewrite.
fn law_grammar() -> pg_grammar_model::model::Grammar {
    let mut lang = Lang::new("kait");
    lang.prules = rewrite("pI", &lit("a"), &lit("i"), "", &lit("t"));
    lang.stratum(
        Stratum::new("S0")
            .rules("mS1 mP1", &(suffix("S1", "t") + &affix("P1", "i", "", "")))
            .entries(&(entry("K", "ka") + &entry("L", "kat"))),
    )
    .stratum(
        Stratum::new("S1")
            .order("unordered")
            .prules("pI")
            .rules(
                "mS2 mC",
                &(affix("S2", "", "a", r#"multipleApplication="2""#) + &compound("C", "", "")),
            )
            .defs(&suffix("T3", "k"))
            .templates(&template(&["mT3"]))
            .entries(&entry("M", "ta")),
    )
    .load()
}

/// A deterministic xorshift stream, so a failure reproduces from its seed.
struct Seeded(u64);
impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

/// TEST 15 (L1) and TEST 16 (L2): a filter holding a derivation's roots and rules returns that analysis, and a restricted parse never returns an analysis plain HC lacks, with the same multiplicity.
#[test]
fn test_15_and_16_restricted_parse_laws_over_seeded_filters() {
    let g = law_grammar();
    let root_ids: Vec<String> = g
        .entries
        .iter()
        .map(|e| g.morphemes[e.morpheme.0 as usize].morph_id.clone().unwrap())
        .collect();
    let rule_ids: Vec<&str> = ["S1", "P1", "S2", "T3"].to_vec();
    let mut rng = Seeded(0x9e37_79b9_7f4a_7c15);
    let (mut words, mut checked, mut analyses_seen) = (0usize, 0usize, 0usize);
    for w in strings_over("kait", 5) {
        let plain = analyses(&g, &w);
        if plain.is_empty() {
            continue;
        }
        words += 1;
        analyses_seen += plain.len();
        let mut distinct = plain.clone();
        distinct.dedup();
        for a in distinct {
            let tokens: Vec<&str> = a.split('+').collect();
            let need_roots: Vec<&str> = tokens
                .iter()
                .copied()
                .filter(|t| root_ids.iter().any(|r| r == t))
                .collect();
            let need_rules: Vec<&str> = tokens
                .iter()
                .copied()
                .filter(|t| rule_ids.contains(t))
                .collect();
            for _ in 0..4 {
                let mut roots: Vec<&str> = need_roots.clone();
                for r in &root_ids {
                    if rng.coin() && !roots.contains(&r.as_str()) {
                        roots.push(r);
                    }
                }
                let mut rules: Vec<&str> = need_rules.clone();
                for r in &rule_ids {
                    if rng.coin() && !rules.contains(r) {
                        rules.push(r);
                    }
                }
                let got = restricted(&g, &w, &roots, &rules);
                // L1: the derivation survives, with its plain multiplicity.
                let want = plain.iter().filter(|p| **p == a).count();
                assert_eq!(
                    got.iter().filter(|p| **p == a).count(),
                    want,
                    "L1: {w} {a} roots {roots:?} rules {rules:?}"
                );
                // L2: nothing plain HC lacks, with equal multiplicity.
                for r in &got {
                    assert_eq!(
                        got.iter().filter(|x| *x == r).count(),
                        plain.iter().filter(|x| *x == r).count(),
                        "L2: {w} {r} roots {roots:?} rules {rules:?}"
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(words >= 20 && checked >= 100, "the grammar must exercise the laws: {words} words, {analyses_seen} analyses, {checked} filter pairs");
    eprintln!("L1/L2: {words} words, {analyses_seen} analyses, {checked} filter pairs");
}
