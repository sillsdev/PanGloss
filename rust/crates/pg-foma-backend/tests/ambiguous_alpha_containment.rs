use super::rewrite_oracle_fixtures::{assert_capabilities, verify_owner, verify_templated};

const FIXTURES: &[(&str, bool)] = &[
    ("nullable-disagree-plain-ltr-right", false),
    ("nullable-disagree-bounded-ltr-left", true),
    ("nullable-disagree-bounded-ltr-right", true),
    ("nullable-disagree-bounded-rtl-left", true),
    ("nullable-disagree-bounded-rtl-right", true),
    ("nullable-disagree-unbounded-ltr-left", true),
    ("nullable-disagree-unbounded-ltr-right", true),
    ("nullable-disagree-unbounded-rtl-left", true),
    ("nullable-disagree-unbounded-rtl-right", true),
];

const PARTIAL_CLASS_FIXTURES: &[(&str, bool)] = &[
    ("partial-class-disagree-bounded-rtl-right", true),
    ("partial-class-disagree-unbounded-rtl-left", true),
    ("partial-class-disagree-bounded-ltr-left", true),
    ("partial-class-disagree-bounded-rtl-left", true),
    ("partial-class-disagree-unbounded-ltr-right", true),
    ("partial-class-disagree-unbounded-rtl-right", true),
];

#[test]
fn ambiguous_disagreement_is_admitted_for_confirmation() {
    for &(name, quantified) in FIXTURES {
        assert_capabilities(name, quantified);
    }
}

#[test]
fn ambiguous_disagreement_propose_confirm_matches_recorded_oracle() {
    let mut checked = 0;
    let mut positives = 0;
    let mut pruned = 0;
    for &(name, _) in FIXTURES {
        let counts = verify_owner(name);
        checked += counts.0;
        positives += counts.1;
        pruned += counts.2;
    }
    assert_eq!(checked, 144);
    assert!(positives > 0 && pruned > 0);
}

#[test]
fn templated_ambiguous_disagreement_matches_recorded_oracle() {
    let mut checked = 0;
    let mut positives = 0;
    for &(name, _) in FIXTURES {
        let counts = verify_templated(name);
        checked += counts.0;
        positives += counts.1;
    }
    assert_eq!(checked, 144);
    assert!(positives > 0);
}

#[test]
fn partial_class_disagreement_is_admitted_for_confirmation() {
    for &(name, quantified) in PARTIAL_CLASS_FIXTURES {
        assert_capabilities(name, quantified);
    }
}

#[test]
fn partial_class_disagreement_propose_confirm_matches_recorded_oracle() {
    let mut checked = 0;
    let mut positives = 0;
    let mut pruned = 0;
    for &(name, _) in PARTIAL_CLASS_FIXTURES {
        let counts = verify_owner(name);
        checked += counts.0;
        positives += counts.1;
        pruned += counts.2;
    }
    assert_eq!(checked, 96);
    assert_eq!(positives, 6);
    assert!(pruned > 0);
}

#[test]
fn templated_partial_class_disagreement_matches_recorded_oracle() {
    let mut checked = 0;
    let mut positives = 0;
    for &(name, _) in PARTIAL_CLASS_FIXTURES {
        let counts = verify_templated(name);
        checked += counts.0;
        positives += counts.1;
    }
    assert_eq!(checked, 96);
    assert_eq!(positives, 6);
}

#[test]
fn mixed_agreeing_class_matches_recorded_oracle_after_confirmation() {
    let name = "ambiguous-disagree-mixed-plus-class";
    assert_capabilities(name, false);
    let (checked, positives, pruned) = verify_owner(name);
    assert_eq!(checked, 16);
    assert!(positives > 0 && pruned > 0);
    assert_eq!(verify_templated(name), (16, 1));
}

#[test]
fn repeated_minus_with_partial_focus_matches_recorded_oracle_after_confirmation() {
    let name = "ambiguous-disagree-repeated-minus-partial-focus";
    assert_capabilities(name, true);
    let (checked, positives, pruned) = verify_owner(name);
    assert_eq!(checked, 16);
    assert!(positives > 0 && pruned > 0);
    assert_eq!(verify_templated(name), (16, 1));
}

#[test]
fn overwritten_alpha_matches_recorded_oracle_after_confirmation() {
    for (name, quantified) in [
        ("overridden-alpha-plain-ltr-left", false),
        ("overridden-alpha-bounded-ltr-left", true),
        ("overridden-alpha-unbounded-ltr-left", true),
        ("overridden-alpha-bounded-rtl-left", true),
    ] {
        assert_capabilities(name, quantified);
        let (checked, positives, pruned) = verify_owner(name);
        assert_eq!(checked, 16);
        assert!(positives > 0 && pruned > 0);
        assert_eq!(verify_templated(name), (16, 1));
    }
}
