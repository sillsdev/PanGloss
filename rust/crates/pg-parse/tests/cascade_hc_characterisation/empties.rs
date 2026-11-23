//! Plan section 2.8, the HC column of rows E1-E20: everything that can be empty. Each case is named `e<NN>_...`. A row that cannot be built from `<HermitCrabInput>` XML is stated in the test body and not asserted.

use super::fixture::*;
use super::phonology::{cv_lang, modify_last, r31_grammar, r5_grammar, rewrite_forward};

const EMPTY_CLASS: &str =
    r#"<SegmentNaturalClass id="ncEmpty"><Name>empty</Name></SegmentNaturalClass>"#;

fn word_outcome(g: &pg_grammar_model::model::Grammar, w: &str) -> pg_parse::ParseOutcome {
    morpher(g).parse_word(w)
}

/// E1: an alpha output lane with no value (opposite polarity, binder unspecified): the node unifies with nothing and is dropped.
#[test]
fn e01_alpha_output_lane_with_no_value_drops_the_node() {
    let g = r5_grammar();
    assert_eq!(
        analyses(&g, "aa"),
        ["APE"],
        "the root `ape` surfaces as `aa`"
    );
    assert!(
        analyses(&g, "ia").is_empty(),
        "no pattern is matched by the empty-lane node"
    );
}

/// E2: an any-value lane whose concrete binding no char-def has: the node is dropped.
#[test]
fn e02_any_value_lane_with_no_char_def_binding_drops_the_node() {
    let g = r5_grammar();
    // Rule R binds `vc` from the following vowel, which carries no `vc`: the bound value is any-value, and no consonant char-def has the opposite value. Built through the r5 rules, whose R binds `vc` from a following vowel (which has none); a grammar isolating a pure any-value binder was not built (open item in the W0 report).
    assert!(analyses(&g, "ape").is_empty() && analyses(&g, "apa").is_empty());
}

/// E3: a segment-list class `Modify` image no member unifies with: the node is dropped (the R3-1 case).
#[test]
fn e03_segment_list_modify_image_no_member_unifies_with_drops_the_node() {
    let g = r31_grammar();
    assert!(analyses(&g, "aa").is_empty(), "the Modify node whose image no member unifies with is dropped, and `aa` does not analyse as `ab`");
    assert_eq!(generate(&g, "AB", &["SUF"]), ["a"]);
}

/// E5: a segment-list class with no member as a pattern node matches nothing.
#[test]
fn e05_segment_list_class_with_no_member_matches_nothing() {
    let mut lang = Lang::new("ax");
    lang.classes += EMPTY_CLASS;
    lang.prules = rewrite("pE", &ctx("ncEmpty"), &lit("x"), "", "");
    let g = lang
        .stratum(Stratum::new("S").prules("pE").entries(&entry("A", "a")))
        .load();
    assert_eq!(analyses(&g, "a"), ["A"], "the rule never applies");
    assert!(analyses(&g, "x").is_empty());
    assert_eq!(rewrite_forward(&g, 0, "a"), None);
}

/// E6: a feature class no char-def is consistent with, used in a `Modify`: the node has no written form.
#[test]
fn e06_feature_class_with_no_consistent_member_drops_the_node() {
    let mut lang = cv_lang();
    lang.classes += &fclass("ncImpossible", &[("voc", "vocP"), ("vc", "vcM")]);
    let g = lang
        .stratum(
            Stratum::new("S")
                .rules("mM", &modify_last("M", "ncImpossible"))
                .entries(&entry("AB", "ab")),
        )
        .load();
    assert_eq!(
        generate(&g, "AB", &["M"]),
        ["a"],
        "the Modified node has no written form and is dropped"
    );
    for w in ["a", "ap", "at"] {
        assert!(
            analyses(&g, w).is_empty(),
            "{w}: the dropped node is not recovered by analysis"
        );
    }
    assert_eq!(analyses(&g, "ab"), ["AB"]);
}

/// E8: a token with no compatible char-def at all: the node is dropped.
#[test]
fn e08_rhs_feature_bundle_no_char_def_has_drops_the_node() {
    let mut lang = cv_lang();
    lang.classes += &fclass("ncImpossible", &[("voc", "vocP"), ("vc", "vcM")]);
    lang.prules = rewrite("pI", &lit("b"), &ctx("ncImpossible"), "", "");
    let g = lang
        .stratum(Stratum::new("S").prules("pI").entries(&entry("AB", "ab")))
        .load();
    assert_eq!(
        rewrite_forward(&g, 0, "ab").as_deref(),
        Some("a"),
        "the rewritten node is dropped"
    );
    for w in ["a", "ab", "ap"] {
        assert!(analyses(&g, w).is_empty(), "{w}");
    }
}

/// E9: a char-def with an empty representation matches without consuming.
#[test]
fn e09_char_def_with_empty_representation() {
    let mut lang = Lang::new("ab");
    lang.segments += &seg("", &[], &[]).replace("cb", "cEmpty");
    let xml = lang
        .stratum(Stratum::new("S").entries(&entry("A", "ab")))
        .xml();
    let g = pg_grammar::load(&xml).expect("a char-def with an empty representation loads");
    assert_eq!(analyses(&g, "ab"), ["A"]);
}

/// E10: a root allomorph with no segment is a bare entry.
#[test]
fn e10_root_allomorph_with_no_segment_is_a_bare_entry() {
    let g = Lang::new("ab")
        .stratum(Stratum::new("S").entries(&(entry("Z", "") + &entry("A", "a"))))
        .load();
    assert_eq!(analyses(&g, "a"), ["A"]);
    assert!(
        word_outcome(&g, "").analyses.is_empty(),
        "the empty typed word is not analysed even with a bare entry in the lexicon"
    );
}

/// E11: a null allomorph (an empty insert) is an identity application.
#[test]
fn e11_null_allomorph_is_an_identity_application() {
    let g = Lang::new("ak")
        .stratum(
            Stratum::new("S")
                .rules("mN", &affix("N", "", "", ""))
                .entries(&entry("K", "k")),
        )
        .load();
    assert_eq!(
        analyses(&g, "k"),
        ["K", "K+N"],
        "the bare root and the identity application are both reported"
    );
    assert_eq!(generate(&g, "K", &["N"]), ["k"]);
}

/// E12: an empty focus (epenthesis) and an empty rhs (deletion).
#[test]
fn e12_empty_focus_and_empty_rhs() {
    let mut lang = Lang::new("abe");
    lang.prules = rewrite("pDel", &lit("b"), "", &lit("a"), "")
        + &rewrite("pIns", "", &lit("e"), &lit("a"), &lit("a"));
    let g = lang
        .stratum(
            Stratum::new("S")
                .prules("pDel pIns")
                .entries(&(entry("K", "ab") + &entry("L", "aa"))),
        )
        .load();
    assert_eq!(
        rewrite_forward(&g, 0, "ab").as_deref(),
        Some("a"),
        "empty rhs: deletion"
    );
    assert_eq!(
        rewrite_forward(&g, 1, "aa").as_deref(),
        Some("aea"),
        "empty focus: epenthesis"
    );
    assert_eq!(analyses(&g, "a"), ["K"]);
    assert_eq!(analyses(&g, "aea"), ["L"]);
    for w in ["ab", "aa"] {
        assert!(analyses(&g, w).is_empty(), "{w}");
    }
}

/// E13: a rule whose pattern is empty never applies (same construction as E5, as a morphological rule).
#[test]
fn e13_rule_with_empty_pattern_never_applies() {
    let mut lang = Lang::new("ak");
    lang.classes += EMPTY_CLASS;
    let rule = affix_rule("N", "", &[("N", &[&ctx("ncEmpty")], &copy("N", 0))]);
    let g = lang
        .stratum(
            Stratum::new("S")
                .rules("mN", &rule)
                .entries(&entry("K", "k")),
        )
        .load();
    assert_eq!(analyses(&g, "k"), ["K"]);
    assert!(generate(&g, "K", &["N"]).is_empty());
}

/// E14: a template with no slot, or a slot with no rule, changes nothing.
#[test]
fn e14_template_with_no_slot_or_a_slot_with_no_rule_changes_nothing() {
    for templates in [
        r#"<AffixTemplate requiredPartsOfSpeech="posN"><Name>T</Name></AffixTemplate>"#.to_string(),
        template(&[""]),
    ] {
        let g = Lang::new("k")
            .stratum(
                Stratum::new("S")
                    .templates(&templates)
                    .entries(&entry("K", "k")),
            )
            .load();
        assert_eq!(analyses(&g, "k"), ["K"]);
        assert!(analyses(&g, "kk").is_empty());
    }
}

/// E15: a pin group with no rule yields bare roots only; no root at all yields nothing.
#[test]
fn e15_empty_pin_group_and_empty_root_filter() {
    let g = Lang::new("kat")
        .stratum(
            Stratum::new("S")
                .rules("mT", &suffix("T", "t"))
                .entries(&entry("K", "ka")),
        )
        .load();
    assert_eq!(restricted(&g, "ka", &["K"], &[]), ["K"]);
    assert!(
        restricted(&g, "kat", &["K"], &[]).is_empty(),
        "no rule in the group: no suffix"
    );
    assert!(
        restricted(&g, "ka", &[], &["T"]).is_empty(),
        "no root in the group: nothing"
    );
    assert_eq!(restricted(&g, "kat", &["K"], &["T"]), ["K+T"]);
}

/// E16 and E20: the empty word, and a word with a unit the grammar cannot write.
#[test]
fn e16_e20_empty_word_and_unwritable_unit() {
    let g = Lang::new("ka")
        .stratum(Stratum::new("S").entries(&entry("K", "ka")))
        .load();
    let empty = word_outcome(&g, "");
    assert!(
        empty.analyses.is_empty() && !empty.capped && !empty.invalid_shape,
        "the empty word is a valid shape with no analysis"
    );
    let bad = word_outcome(&g, "kz");
    assert!(
        bad.analyses.is_empty() && bad.invalid_shape,
        "a unit the grammar cannot write makes the word an invalid shape"
    );
}

/// E17: an any-value or EMPTY lane as the binder for a later alpha rule (see also TEST 17, TEST 24).
#[test]
fn e17_any_value_or_empty_binder_for_a_later_alpha_rule() {
    let g = r5_grammar();
    assert_eq!(analyses(&g, "aa"), ["APE"]);
    assert!(
        analyses(&g, "ia").is_empty(),
        "the empty-lane node of R matches no pattern, so S never sees it"
    );
}

/// E18: an empty segment-list class on the rhs of a phonological rule: the node has no written form and is dropped.
#[test]
fn e18_empty_segment_class_on_the_rhs_drops_the_node() {
    let mut lang = Lang::new("ab");
    lang.classes += EMPTY_CLASS;
    lang.prules = rewrite("pE", &lit("b"), &ctx("ncEmpty"), "", "");
    let xml = lang
        .stratum(Stratum::new("S").prules("pE").entries(&entry("AB", "ab")))
        .xml();
    let g =
        pg_grammar::load(&xml).expect("the loader accepts an empty segment-list class on the rhs");
    assert_eq!(
        rewrite_forward(&g, 0, "ab").as_deref(),
        Some("a"),
        "the node has no written form and is dropped"
    );
    for w in ["a", "ab"] {
        assert!(analyses(&g, w).is_empty(), "{w}");
    }
}

/// E19: a rule application that changes nothing leaves the word as it is.
#[test]
fn e19_rule_application_that_changes_nothing() {
    let mut lang = Lang::new("ab");
    lang.prules = rewrite("pSame", &lit("a"), &lit("a"), "", "");
    let g = lang
        .stratum(
            Stratum::new("S")
                .prules("pSame")
                .entries(&entry("AB", "ab")),
        )
        .load();
    assert_eq!(analyses(&g, "ab"), ["AB"]);
    assert_eq!(rewrite_forward(&g, 0, "ab").as_deref(), Some("ab"));
}
