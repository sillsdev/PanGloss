//! Phonology: rewrite sites, metathesis windows, `dirty`, alpha binders, `cd_set` and spelling (TEST 3, 4, 14, 17, 20, 21, 23, 24).

use super::fixture::*;
use pg_grammar_model::model::{Grammar, PhonRuleDef};
use pg_parse::surface::to_plain_string;
use pg_rules::shape_feat::segment_with_features;

/// Apply rewrite rule `rule` (index into `g.prules`) forward to `word`; `None` when it does not apply.
pub fn rewrite_forward(g: &Grammar, rule: usize, word: &str) -> Option<String> {
    let PhonRuleDef::Rewrite(def) = &g.prules[rule] else {
        panic!("not a rewrite rule")
    };
    let table = &g.char_tables[0];
    let shape = segment_with_features(g, table, word).expect("word segments");
    let out = pg_rules::rewrite::synthesize(g, def, &shape);
    out.first().map(|s| to_plain_string(table, s, true))
}

/// Apply metathesis rule `rule` forward to `word`.
pub fn metathesis_forward(g: &Grammar, rule: usize, word: &str) -> Option<String> {
    let PhonRuleDef::Metathesis(def) = &g.prules[rule] else {
        panic!("not a metathesis rule")
    };
    let table = &g.char_tables[0];
    let shape = segment_with_features(g, table, word).expect("word segments");
    let out = pg_rules::metathesis::synthesize(g, def, &shape);
    out.first().map(|s| to_plain_string(table, s, true))
}

/// TEST 3: a simultaneous rule whose lhs cannot overlap itself rewrites exactly the disjoint occurrences; an overlapping lhs is recorded as it behaves.
#[test]
fn test_03_simultaneous_multi_token_sites_never_overlap_when_the_lhs_cannot_self_overlap() {
    let build = |lhs: String, rhs: String| {
        let mut lang = Lang::new("abcxyz");
        lang.prules = rewrite_with(
            "p",
            r#"multipleApplicationOrder="simultaneous""#,
            &lhs,
            &rhs,
            "",
            "",
        );
        lang.stratum(Stratum::new("S").prules("p").entries(&entry("K", "a")))
            .load()
    };
    // lhs `abc` has no proper suffix that is also a prefix.
    let g = build(
        lit("a") + &lit("b") + &lit("c"),
        lit("x") + &lit("y") + &lit("z"),
    );
    for s in strings_over("abc", 7) {
        let want = s.replace("abc", "xyz");
        let got = rewrite_forward(&g, 0, &s).unwrap_or_else(|| s.clone());
        assert_eq!(got, want, "input {s}");
    }
    // lhs `aa` overlaps itself: three a's hold two overlapping sites.
    let g = build(lit("a") + &lit("a"), lit("x") + &lit("y"));
    for (s, want) in [
        ("aa", "xy"),
        ("aaa", "xxy"),
        ("aaaa", "xxxy"),
        ("aaaaa", "xxxxy"),
    ] {
        assert_eq!(
            rewrite_forward(&g, 0, s).as_deref(),
            Some(want),
            "overlapping sites on the unchanged string: {s}"
        );
    }
}

/// TEST 4: metathesis windows chain on the mutated string, and an adjacent single-node switch pair gives disjoint input pairs.
#[test]
fn test_04_metathesis_windows_chain_and_adjacent_switches_stay_disjoint() {
    let mut lang = Lang::new("abtk");
    lang.classes += &(sclass("ncA", &["a"])
        + &sclass("ncB", &["b"])
        + &sclass("ncAT", &["a", "t"])
        + &sclass("ncT", &["t"])
        + &sclass("ncK", &["k"]));
    lang.prules = metathesis("m2", "x1", "x0", &[("x0", "ncA"), ("x1", "ncB")])
        + &metathesis(
            "m3",
            "x2",
            "x1",
            &[("x0", "ncAT"), ("x1", "ncT"), ("x2", "ncK")],
        );
    let g = lang
        .stratum(Stratum::new("S").prules("m2 m3").entries(&entry("K", "a")))
        .load();
    // Admitted shape: every window is an `ab` pair of the input, replaced by `ba`.
    for s in strings_over("ab", 8) {
        let want = s.replace("ab", "ba");
        let got = metathesis_forward(&g, 0, &s).unwrap_or_else(|| s.clone());
        assert_eq!(got, want, "input {s}");
    }
    // General windows: a context node `t` that an earlier swap produced starts a second window.
    assert_eq!(metathesis_forward(&g, 1, "atk").as_deref(), Some("akt"));
    assert_eq!(
        metathesis_forward(&g, 1, "atktk").as_deref(),
        Some("aktkt"),
        "the window rescans the mutated string, so the swaps chain"
    );
}

/// TEST 14: `dirty` does not persist across rules: a later rule rewrites a node an earlier rule produced.
#[test]
fn test_14_dirty_does_not_persist_across_rules() {
    let mut lang = Lang::new("abc");
    lang.prules =
        rewrite("p1", &lit("a"), &lit("b"), "", "") + &rewrite("p2", &lit("b"), &lit("c"), "", "");
    let g = lang
        .stratum(Stratum::new("S").prules("p1 p2").entries(&entry("A", "a")))
        .load();
    assert_eq!(analyses(&g, "c"), ["A"], "a becomes b becomes c");
    assert!(analyses(&g, "b").is_empty());
    assert!(analyses(&g, "a").is_empty());
    assert_eq!(generate(&g, "A", &[]), ["c"]);
}

/// TEST 17: a two-node focus whose nodes share an alpha variable; the first node an any-value row.
#[test]
fn test_17_exact_plan_for_a_shared_alpha_with_an_any_value_first_node() {
    use pg_grammar_model::model::TableId;
    use pg_rules::rewrite::{focus_outputs, BlockNode};
    let features = binary_feature("f");
    let mut lang = featured(&features, &[("p", &[("f", "fP")]), ("q", &[("f", "fM")])]);
    lang.prules = rewrite_vars(
        "r",
        r#"multipleApplicationOrder="leftToRightIterative""#,
        &variable_feature("varF", "f"),
        &(ctx_alpha("ncAny", "varF", "plus") + &ctx_alpha("ncAny", "varF", "plus")),
        &(lit("q") + &lit("q")),
        "",
        "",
    );
    let g = lang
        .stratum(Stratum::new("S").prules("r").entries(&entry("K", "p")))
        .load();
    // HC rewrites two agreeing nodes and leaves disagreeing ones alone.
    assert_eq!(rewrite_forward(&g, 0, "pp").as_deref(), Some("qq"));
    assert_eq!(rewrite_forward(&g, 0, "qq").as_deref(), Some("qq"));
    assert_eq!(rewrite_forward(&g, 0, "pq"), None);
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!()
    };
    let table = &g.char_tables[0];
    let nf = g.phon_features.len();
    let row = |text: &str| -> Vec<u64> {
        let shape = segment_with_features(&g, table, text).unwrap();
        shape.node_lanes(0)[..nf].to_vec()
    };
    let node = |lanes: Vec<u64>| BlockNode::Segment {
        lanes,
        explicit_full: Vec::new(),
    };
    let p = row("p");
    let any = vec![0u64; nf];
    let concrete = focus_outputs(&g, TableId(0), rule, 0, &[node(p.clone()), node(p.clone())]);
    assert!(
        concrete.is_some(),
        "two agreeing concrete nodes have a block"
    );
    let any_first = focus_outputs(
        &g,
        TableId(0),
        rule,
        0,
        &[node(any.clone()), node(p.clone())],
    );
    assert!(any_first.is_none(), "an any-value first node binds the variable to 0 and the second node fails: no block, though HC rewrites `pp`");
    let any_both = focus_outputs(&g, TableId(0), rule, 0, &[node(any.clone()), node(any)]);
    assert!(any_both.is_some());
}

const VOICE_FEATURES: &str = r#"<SymbolicFeature id="voc"><Name>voc</Name><Symbols><Symbol id="vocP">+</Symbol><Symbol id="vocM">-</Symbol></Symbols></SymbolicFeature><SymbolicFeature id="vc"><Name>vc</Name><Symbols><Symbol id="vcP">+</Symbol><Symbol id="vcM">-</Symbol></Symbols></SymbolicFeature><SymbolicFeature id="pl"><Name>pl</Name><Symbols><Symbol id="plL">lab</Symbol><Symbol id="plC">cor</Symbol><Symbol id="plV">vel</Symbol></Symbols></SymbolicFeature>"#;

/// The consonant/vowel feature grammar of TEST 20, 21 and 23: features voc, vc, pl; segments a (vowel), p b (labial), t d (coronal), h (voc- only).
pub fn cv_lang() -> Lang {
    let mut lang = featured(
        VOICE_FEATURES,
        &[
            ("a", &[("voc", "vocP"), ("vc", "vcP")]),
            ("p", &[("voc", "vocM"), ("vc", "vcM"), ("pl", "plL")]),
            ("b", &[("voc", "vocM"), ("vc", "vcP"), ("pl", "plL")]),
            ("t", &[("voc", "vocM"), ("vc", "vcM"), ("pl", "plC")]),
            ("d", &[("voc", "vocM"), ("vc", "vcP"), ("pl", "plC")]),
            ("h", &[("voc", "vocM")]),
        ],
    );
    lang.classes += &(fclass("ncLab", &[("pl", "plL")])
        + &fclass("ncCor", &[("pl", "plC")])
        + &fclass("ncVoiceless", &[("vc", "vcM")])
        + &sclass("ncP", &["p"]));
    lang
}

/// A suffix-like rule that copies the stem but for its last node, which it `Modify`s through `class`.
pub fn modify_last(id: &str, class: &str) -> String {
    affix_rule(
        id,
        "",
        &[(
            id,
            &[ANY0, &ctx("ncAny")],
            &(copy(id, 0) + &modify(id, 1, class)),
        )],
    )
}

/// TEST 21: a featured rewrite of a node a segment-list `Modify` produced drops the `cd_set` the `Modify` left.
#[test]
fn test_21_a_featured_rewrite_resets_the_cd_set_a_modify_left() {
    let mut lang = cv_lang();
    lang.prules = rewrite_with(
        "pr",
        "",
        &ctx("ncLab"),
        &ctx("ncCor"),
        "",
        &tmpl("", r#"finalBoundaryCondition="true""#),
    );
    let g = lang
        .stratum(
            Stratum::new("S")
                .rules("mM", &modify_last("M", "ncP"))
                .prules("pr")
                .entries(&entry("AB", "ab")),
        )
        .load();
    assert_eq!(
        generate(&g, "AB", &["M"]),
        ["at"],
        "the Modify image `p` is rewritten to coronal `t`"
    );
    assert_eq!(analyses(&g, "at"), ["AB+M"]);
    assert!(
        analyses(&g, "ap").is_empty(),
        "the cd_set the Modify left was reset, so the rewrite applies"
    );
    assert_eq!(
        analyses(&g, "ad"),
        ["AB"],
        "the root `ab` itself surfaces as `ad`"
    );
}

/// TEST 23: a feature-class `Modify` leaves a `cd_set` holding every char-def that unifies with its image.
#[test]
fn test_23_feature_class_modify_cd_set_holds_every_unifying_char_def() {
    let lang = cv_lang();
    let g = lang
        .stratum(
            Stratum::new("S")
                .rules("mM", &modify_last("M", "ncVoiceless"))
                .entries(&entry("AB", "ab")),
        )
        .load();
    assert_eq!(generate(&g, "AB", &["M"]), ["ap"]);
    assert_eq!(
        analyses(&g, "ap"),
        ["AB+M"],
        "the voiceless member `p` unifies with the image"
    );
    assert_eq!(
        analyses(&g, "ah"),
        ["AB", "AB+M"],
        "the underspecified `h` unifies with the image and, as a surface, with the root's `b`"
    );
    assert!(
        analyses(&g, "at").is_empty(),
        "`t` is voiceless but not a member of ncVoiceless that the pin sets leave"
    );
}

/// The review r3 grammar of TEST 20: voice, nasal, place; a segment-list class `{p, n}` whose image no member unifies with.
pub fn r31_grammar() -> Grammar {
    let features = r#"<SymbolicFeature id="voc"><Name>voc</Name><Symbols><Symbol id="vocP">+</Symbol><Symbol id="vocM">-</Symbol></Symbols></SymbolicFeature><SymbolicFeature id="vc"><Name>vc</Name><Symbols><Symbol id="vcP">+</Symbol><Symbol id="vcM">-</Symbol></Symbols></SymbolicFeature><SymbolicFeature id="nas"><Name>nas</Name><Symbols><Symbol id="nasP">+</Symbol><Symbol id="nasM">-</Symbol></Symbols></SymbolicFeature><SymbolicFeature id="pl"><Name>pl</Name><Symbols><Symbol id="plL">lab</Symbol><Symbol id="plC">cor</Symbol><Symbol id="plV">vel</Symbol></Symbols></SymbolicFeature>"#;
    let mut lang = featured(
        features,
        &[
            ("a", &[("voc", "vocP")]),
            ("e", &[("voc", "vocP")]),
            (
                "p",
                &[
                    ("voc", "vocM"),
                    ("vc", "vcM"),
                    ("nas", "nasM"),
                    ("pl", "plL"),
                ],
            ),
            (
                "b",
                &[
                    ("voc", "vocM"),
                    ("vc", "vcP"),
                    ("nas", "nasM"),
                    ("pl", "plL"),
                ],
            ),
            (
                "n",
                &[
                    ("voc", "vocM"),
                    ("vc", "vcP"),
                    ("nas", "nasP"),
                    ("pl", "plC"),
                ],
            ),
        ],
    );
    lang.classes += &sclass("ncC", &["p", "n"]);
    lang.prules = rewrite("D", &lit("b"), "", &lit("a"), &lit("a"))
        + &rewrite_with(
            "E",
            "",
            &lit("e"),
            &lit("a"),
            "",
            &tmpl("", r#"finalBoundaryCondition="true""#),
        );
    lang.stratum(
        Stratum::new("S")
            .rules("mSUF", &modify_last("SUF", "ncC"))
            .prules("D E")
            .entries(&entry("AB", "ab")),
    )
    .load()
}

/// TEST 20: the R3-1 grammar: does HC return root `ab` for word `aa`?
#[test]
fn test_20_r31_grammar_word_aa() {
    let g = r31_grammar();
    assert_eq!(
        generate(&g, "AB", &["SUF"]),
        ["a"],
        "the Modify node and the inserted `e` leave `ab+SUF` as `a`"
    );
    assert_eq!(analyses(&g, "ab"), ["AB"]);
    assert!(
        analyses(&g, "aa").is_empty(),
        "HC does not return root `ab` for the word `aa`"
    );
    for w in ["aba", "ae", "a"] {
        assert!(analyses(&g, w).is_empty(), "{w}");
    }
}

/// The review r5 grammar: voice, rules D, E, R, S; root `ape`.
pub fn r5_grammar() -> Grammar {
    let features = r#"<SymbolicFeature id="voc"><Name>voc</Name><Symbols><Symbol id="vocP">+</Symbol><Symbol id="vocM">-</Symbol></Symbols></SymbolicFeature><SymbolicFeature id="vc"><Name>vc</Name><Symbols><Symbol id="vcP">+</Symbol><Symbol id="vcM">-</Symbol></Symbols></SymbolicFeature>"#;
    let mut lang = featured(
        features,
        &[
            ("a", &[("voc", "vocP")]),
            ("e", &[("voc", "vocP")]),
            ("i", &[("voc", "vocP")]),
            ("p", &[("voc", "vocM"), ("vc", "vcM")]),
            ("b", &[("voc", "vocM"), ("vc", "vcP")]),
        ],
    );
    lang.classes += &(fclass("ncCons", &[("voc", "vocM")]) + &fclass("ncVow", &[("voc", "vocP")]));
    lang.prules = rewrite("D", &ctx("ncCons"), "", &lit("a"), &lit("a"))
        + &rewrite_with(
            "E",
            "",
            &lit("e"),
            &lit("a"),
            "",
            &tmpl("", r#"finalBoundaryCondition="true""#),
        )
        + &rewrite_vars(
            "R",
            "",
            &variable_feature("varVc", "vc"),
            &ctx("ncCons"),
            &ctx_alpha("ncAny", "varVc", "minus"),
            "",
            &tmpl(&ctx_alpha("ncVow", "varVc", "plus"), ""),
        )
        + &rewrite("S", &lit("a"), &lit("i"), "", &ctx("ncCons"));
    lang.stratum(
        Stratum::new("S0")
            .prules("D E R S")
            .entries(&entry("APE", "ape")),
    )
    .load()
}

/// TEST 24: the empty alpha binding: rules D, E, R of review r5, root `ape`, word `aa`.
#[test]
fn test_24_empty_alpha_binding_rules_d_e_r() {
    let g = r5_grammar();
    assert_eq!(generate(&g, "APE", &[]), ["aa"]);
    assert_eq!(analyses(&g, "aa"), ["APE"]);
    for w in ["ia", "apa", "aba", "ape"] {
        assert!(analyses(&g, w).is_empty(), "{w}");
    }
}
