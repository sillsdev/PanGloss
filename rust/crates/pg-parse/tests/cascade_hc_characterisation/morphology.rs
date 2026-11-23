//! Morphology: orders, repeat limits, templates, compounding, families (TEST 2, 5, 6, 8, 9).

use super::fixture::*;

/// TEST 6: in a Linear stratum the rules apply in nondecreasing declared order.
#[test]
fn test_06_linear_order_is_nondecreasing() {
    let g = Lang::new("kxy")
        .stratum(
            Stratum::new("S")
                .order("linear")
                .rules("mA mB", &(suffix("A", "x") + &suffix("B", "y")))
                .entries(&entry("K", "k")),
        )
        .load();
    assert_eq!(analyses(&g, "k"), ["K"]);
    assert_eq!(analyses(&g, "kx"), ["K+A"]);
    assert_eq!(
        analyses(&g, "kxy"),
        ["K+A+B"],
        "A then B is the declared order"
    );
    assert!(
        analyses(&g, "kyx").is_empty(),
        "B then A runs against the declared order"
    );
}

/// TEST 5, slot role: a rule placed in two slots still counts its applications against `max_apps`.
#[test]
fn test_05_max_apps_is_enforced_for_slot_roles() {
    let one = |max: &str| {
        Lang::new("ka")
            .stratum(
                Stratum::new("S")
                    .defs(&affix("A", "", "a", max))
                    .templates(&template(&["mA", "mA"]))
                    .entries(&entry("K", "k")),
            )
            .load()
    };
    let g1 = one("");
    assert_eq!(analyses(&g1, "ka"), ["K+A"]);
    assert!(
        analyses(&g1, "kaa").is_empty(),
        "max_apps 1: a second application in the second slot is refused"
    );
    let g2 = one(r#"multipleApplication="2""#);
    assert_eq!(
        analyses(&g2, "kaa").len(),
        1,
        "max_apps 2: both slots may apply"
    );
}

/// TEST 5, compounding role.
#[test]
fn test_05_max_apps_is_enforced_for_compounding_roles() {
    let make = |max: &str| {
        let mut g = Lang::new("ka")
            .stratum(
                Stratum::new("S")
                    .rules("mC", &compound("C", "", max))
                    .entries(&entry("K", "ka")),
            )
            .load();
        // The stem-count cap (default 2, not settable from XML) would refuse three roots before `max_apps` is consulted.
        g.max_stem_count = 4;
        g
    };
    let g1 = make("");
    assert_eq!(analyses(&g1, "kaka").len(), 1);
    assert!(
        analyses(&g1, "kakaka").is_empty(),
        "max_apps 1: two applications of one compounding rule"
    );
    let g2 = make(r#"multipleApplication="2""#);
    assert_eq!(analyses(&g2, "kakaka").len(), 1);
}

/// TEST 8: in an Unordered stratum a second template pass needs an mrule application in between. CONTRADICTS PLAN: TEST 8 (plan 2.1, "after a template pass the mrules run again on a changed word ... passes alternate"). With HC's default `final="true"` template no mrule ever follows the pass (the template closes the word), so `ktm`, `ktmt` and every longer alternation are refused; with `final="false"` nothing at all parses in any stratum shape tried (a non-final template last-applied is refused at the blocking stratum, and the optional empty pass is itself such a word). The alternation the plan describes is therefore not observable at the parse or generate surface.
#[test]
fn test_08_unordered_template_passes_alternate_with_mrules() {
    let build = |is_final: bool, optional: bool| {
        Lang::new("kmt")
            .stratum(
                Stratum::new("S")
                    .order("unordered")
                    .rules("mM", &affix("M", "", "m", r#"multipleApplication="3""#))
                    .defs(&affix("T", "", "t", r#"multipleApplication="3""#))
                    .templates(&template_with(&["mT"], is_final, optional))
                    .entries(&entry("K", "k")),
            )
            .load()
    };
    let g = build(true, true);
    let language: Vec<String> = strings_over("mt", 5)
        .into_iter()
        .map(|s| format!("k{s}"))
        .filter(|w| !analyses(&g, w).is_empty())
        .collect();
    assert_eq!(
        language,
        ["km", "kt", "kmm", "kmt", "kmmm", "kmmt", "kmmmt"],
        "mrules, then at most one template pass, then nothing"
    );
    assert_eq!(analyses(&g, "kmt"), ["K+M+T"]);
    for refused in ["ktm", "ktmt", "ktt", "ktmtmt"] {
        assert!(analyses(&g, refused).is_empty(), "{refused}");
    }
    use pg_parse::GenMorpheme;
    let gen = |ids: &[&str]| {
        morpher(&g).generate_words(
            lex_entry(&g, "K"),
            &ids.iter()
                .map(|r| GenMorpheme::Rule(mrule(&g, r)))
                .collect::<Vec<_>>(),
            pg_featstruct::FeatureStruct::EMPTY,
        )
    };
    assert_eq!(
        gen(&["T", "M"]),
        ["kmt"],
        "forward synthesis puts the template pass last whatever order the rules are named in"
    );
    assert!(gen(&["M", "T", "M"]).is_empty());
    for optional in [true, false] {
        let g = build(false, optional);
        for w in ["k", "km", "kt", "kmt", "ktm"] {
            assert!(
                analyses(&g, w).is_empty(),
                "a non-final template refuses every word: {w}"
            );
        }
    }
}

/// TEST 7: a realizational rule's output is made only of the four output actions, and it applies at most once per derivation.
#[test]
fn test_07_realizational_rules_use_the_four_output_actions_and_apply_once() {
    use pg_grammar_model::model::{MorphRuleDef, OutputAction};
    let g = Lang::new("kr")
        .stratum(
            Stratum::new("S")
                .rules("mR", &realizational("R", "", "r"))
                .entries(&entry("K", "k")),
        )
        .load();
    for rule in &g.mrules {
        let MorphRuleDef::Realizational(real) = rule else {
            continue;
        };
        for allomorph in &real.allomorphs {
            for action in &allomorph.rhs {
                match action {
                    OutputAction::Copy(_)
                    | OutputAction::InsertSegments { .. }
                    | OutputAction::Modify(..)
                    | OutputAction::InsertContext(_) => {}
                }
            }
        }
    }
    assert_eq!(analyses(&g, "kr"), ["K+R"]);
    assert!(
        analyses(&g, "krr").is_empty(),
        "a realizational rule applies once per derivation"
    );
}

/// TEST 9: the compounding non-head is a bare lexical shape; the compounding rule's own stratum decides which roots it takes. CONTRADICTS PLAN: TEST 9 (plan 2.4.6 line 851, "a prefix or suffix block drawn from `Lex` (strata <= s)"). HC looks the non-head up in the root trie of the compounding rule's own stratum only (`search_roots(stratum, ..)`, pg-parse/src/morpher.rs:1051-1060): a non-head root of a lower stratum is found in neither its bare nor its rewritten spelling.
#[test]
fn test_09_compounding_non_head_is_a_bare_lexical_shape() {
    let classes = r#"<FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass><SegmentNaturalClass id="ncY"><Name>y</Name><Segment segment="cy" /></SegmentNaturalClass>"#;
    let build = |compound_in_s1: bool, non_head_in_s1: bool| {
        let mut lang = Lang::new("hkxy");
        lang.classes = classes.to_string();
        lang.prules = rewrite("pX", &lit("x"), &ctx("ncY"), "", "");
        let mut s0 = Stratum::new("S0").prules("pX");
        let mut s1 = Stratum::new("S1");
        let (head, nh, comp) = (entry("H", "h"), entry("NH", "kx"), compound("C", "", ""));
        if compound_in_s1 {
            s1 = s1.rules("mC", &comp).entries(&head);
        } else {
            s0 = s0.rules("mC", &comp).entries(&head);
        }
        if non_head_in_s1 {
            s1 = s1.entries(&nh);
        } else {
            s0 = s0.entries(&nh);
        }
        lang.stratum(s0).stratum(s1).load()
    };
    // Everything in stratum 0: the rewrite runs on the joined word, so the non-head's x is rewritten.
    let g = build(false, false);
    assert_eq!(analyses(&g, "hky"), ["H+NH"]);
    assert!(analyses(&g, "hkx").is_empty());
    // Everything in stratum 1, which has no rewrite: the bare shape is joined unchanged.
    let g = build(true, true);
    assert_eq!(analyses(&g, "hkx"), ["H+NH"], "bare lexical shape");
    assert!(analyses(&g, "hky").is_empty());
    // CONTRADICTS PLAN: TEST 9 -- a lower-stratum non-head is not reachable at all.
    let g = build(true, false);
    for w in ["hkx", "hky"] {
        assert!(analyses(&g, w).is_empty(), "{w}");
    }
}

/// TEST 2: blocking replaces the word by another family member's seed, so the identity HC reports is not the derivation that produced the surface. CONTRADICTS PLAN: TEST 2 (plan line 730, "a replacement, not a removal"), forward side only: `generate_words` for the blocked derivation returns no word at all, not the family relative's word. The parse side matches the plan: with the family the surface `aat` is analysed only as the bare root `B`, never as `A+T`, and a pin group {root A, rule T} confirms nothing.
#[test]
fn test_02_blocking_replaces_the_derivation_at_the_confirm_seam() {
    let build = |family: bool| {
        let mut lang = Lang::new("abst");
        let attr = if family {
            lang.families = r#"<Family id="famF">F</Family>"#.to_string();
            r#"family="famF""#
        } else {
            ""
        };
        lang.stratum(
            Stratum::new("S")
                .rules("mT", &suffix("T", "t"))
                .entries(&(entry_with("A", "aa", attr) + &entry_with("B", "aat", attr))),
        )
        .load()
    };
    let plain = build(false);
    assert_eq!(
        analyses(&plain, "aat"),
        ["A+T", "B"],
        "no family: both derivations are reported"
    );
    assert_eq!(generate(&plain, "A", &["T"]), ["aat"]);
    let fam = build(true);
    assert_eq!(
        analyses(&fam, "aat"),
        ["B"],
        "the family replaces A+T by B's seed"
    );
    assert_eq!(analyses(&fam, "aa"), ["A"]);
    assert!(generate(&fam, "A", &["T"]).is_empty());
    assert_eq!(generate(&fam, "B", &[]), ["aat"]);
    assert!(
        restricted(&fam, "aat", &["A"], &["T"]).is_empty(),
        "the pins {{A, T}} confirm nothing"
    );
    assert_eq!(restricted(&fam, "aat", &["B"], &[]), ["B"]);
}

/// TEST 26: a root that enters at a later stratum is touched by no rule of an earlier stratum; a compound is rewritten after its parts are joined.
#[test]
fn test_26_no_earlier_stratum_rule_touches_a_later_stratum_root() {
    let mut lang = Lang::new("abxy");
    lang.classes += &sclass("ncY", &["y"]);
    lang.prules = rewrite("pX", &lit("x"), &ctx("ncY"), "", "");
    let g = lang
        .stratum(Stratum::new("S0").prules("pX").entries(&entry("E0", "ax")))
        .stratum(Stratum::new("S1").entries(&entry("E1", "bx")))
        .load();
    assert_eq!(
        analyses(&g, "ay"),
        ["E0"],
        "the stratum 0 rule rewrites the stratum 0 root"
    );
    assert!(analyses(&g, "ax").is_empty());
    assert_eq!(
        analyses(&g, "bx"),
        ["E1"],
        "the stratum 1 root passes stratum 0 untouched"
    );
    assert!(analyses(&g, "by").is_empty());
}

/// TEST 26, compounding: within the compounding rule's own stratum the joined word is rewritten as one (a compound's morpheme list shows each distinct root once).
#[test]
fn test_26_a_compound_in_its_own_stratum_is_rewritten_as_a_whole() {
    let mut lang = Lang::new("axy");
    lang.classes += &sclass("ncY", &["y"]);
    lang.prules = rewrite("pX", &lit("x"), &ctx("ncY"), "", "");
    let g = lang
        .stratum(
            Stratum::new("S0")
                .prules("pX")
                .rules("mC", &compound("C", "", ""))
                .entries(&entry("E0", "ax")),
        )
        .load();
    let out = morpher(&g).parse_word("ayay");
    assert_eq!(sorted(&out), ["E0"]);
    assert_eq!(
        out.structured[0].morph_occurrences.len(),
        2,
        "head and non-head"
    );
    for w in ["axax", "axay", "ayax"] {
        assert!(analyses(&g, w).is_empty(), "{w}: both parts are rewritten");
    }
}

/// TEST 28: template passes and the mrules of an Unordered stratum interleave freely, but one pass applies its slots in order. CONTRADICTS PLAN: TEST 28 (plan 2.1, Unordered interleave): with the default final template a pass closes the word to later mrules, so only `M* . one pass` words exist (see TEST 8).
#[test]
fn test_28_unordered_mrules_interleave_with_template_passes() {
    let g = Lang::new("kmt12")
        .stratum(
            Stratum::new("S")
                .order("unordered")
                .rules("mM", &affix("M", "", "m", r#"multipleApplication="2""#))
                .defs(&affix("T1", "", "1", ""))
                .defs(&affix("T2", "", "2", ""))
                .templates(&template(&["mT1", "mT2"]))
                .entries(&entry("K", "k")),
        )
        .load();
    let language: Vec<String> = strings_over("m12", 5)
        .into_iter()
        .map(|s| format!("k{s}"))
        .filter(|w| !analyses(&g, w).is_empty())
        .collect();
    for w in ["k1", "k2", "k12", "km1", "km12", "kmm12"] {
        assert!(language.iter().any(|l| l == w), "{w} is a word");
    }
    for w in ["k21", "k1m", "k2m1", "km12m", "k11"] {
        assert!(!language.iter().any(|l| l == w), "{w} is refused");
    }
}

/// TEST 30: with `max_apps` 2 a mixed rule applies its first matching subrule each time; the reviewer's A (`pat`, `pait`, `paits`).
#[test]
fn test_30_max_apps_two_alternation_a_infix_then_suffix() {
    let parts_inf: Vec<String> = vec![lit("p") + &lit("a"), lit("t")];
    let parts_inf: Vec<&str> = parts_inf.iter().map(String::as_str).collect();
    let out_inf = copy("inf", 0) + &insert("i") + &copy("inf", 1);
    let parts_suf: Vec<String> = vec![ANY.to_string()];
    let parts_suf: Vec<&str> = parts_suf.iter().map(String::as_str).collect();
    let out_suf = copy("suf", 0) + &insert("s");
    let rule = affix_rule(
        "R",
        r#"multipleApplication="2""#,
        &[("inf", &parts_inf, &out_inf), ("suf", &parts_suf, &out_suf)],
    );
    let g = Lang::new("paist")
        .stratum(
            Stratum::new("S")
                .rules("mR", &rule)
                .entries(&entry("PAT", "pat")),
        )
        .load();
    assert_eq!(
        generate(&g, "PAT", &["R"]),
        ["pait"],
        "first application: the infix subrule matches"
    );
    assert_eq!(
        generate(&g, "PAT", &["R", "R"]),
        ["paits"],
        "second application: only the suffix subrule matches"
    );
    assert_eq!(analyses(&g, "pait"), ["PAT+R"]);
    assert_eq!(analyses(&g, "paits"), ["PAT+R+R"]);
    assert!(analyses(&g, "pats").is_empty());
}

/// TEST 30, the reviewer's B (`pas`, `past`, `pasit`).
#[test]
fn test_30_max_apps_two_alternation_b_suffix_then_infix() {
    let parts_inf: Vec<String> = vec![lit("p") + &lit("a") + &lit("s"), lit("t")];
    let parts_inf: Vec<&str> = parts_inf.iter().map(String::as_str).collect();
    let out_inf = copy("inf", 0) + &insert("i") + &copy("inf", 1);
    let parts_suf: Vec<String> = vec![ANY.to_string()];
    let parts_suf: Vec<&str> = parts_suf.iter().map(String::as_str).collect();
    let out_suf = copy("suf", 0) + &insert("t");
    let rule = affix_rule(
        "R",
        r#"multipleApplication="2""#,
        &[("inf", &parts_inf, &out_inf), ("suf", &parts_suf, &out_suf)],
    );
    let g = Lang::new("paist")
        .stratum(
            Stratum::new("S")
                .rules("mR", &rule)
                .entries(&entry("PAS", "pas")),
        )
        .load();
    assert_eq!(
        generate(&g, "PAS", &["R"]),
        ["past"],
        "first application: the infix input `pas|t` does not match `pas`, so the suffix applies"
    );
    assert_eq!(
        generate(&g, "PAS", &["R", "R"]),
        ["pasit"],
        "second application: the infix subrule now matches `pas|t`"
    );
    assert_eq!(analyses(&g, "past"), ["PAS+R"]);
    assert_eq!(analyses(&g, "pasit"), ["PAS+R+R"]);
    assert!(analyses(&g, "pasti").is_empty());
}
