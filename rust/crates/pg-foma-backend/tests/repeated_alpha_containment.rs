use super::rewrite_oracle_fixtures::{assert_capabilities, verify_owner, verify_templated};

const FIXTURES: &[&str] = &[
    "quantified-alpha-bounded-ltr-left",
    "quantified-alpha-bounded-ltr-right",
    "quantified-alpha-bounded-rtl-left",
    "quantified-alpha-bounded-rtl-right",
    "quantified-alpha-unbounded-ltr-left",
    "quantified-alpha-unbounded-ltr-right",
    "quantified-alpha-unbounded-rtl-left",
    "quantified-alpha-unbounded-rtl-right",
    "quantified-alpha-bounded-zero-left",
    "quantified-alpha-bounded-zero-right",
    "quantified-alpha-unbounded-zero-left",
    "quantified-alpha-unbounded-zero-right",
];

#[test]
fn repeated_alpha_environments_are_admitted_for_confirmation() {
    for name in FIXTURES {
        assert_capabilities(name, true);
    }
}

#[test]
fn repeated_alpha_propose_confirm_matches_recorded_oracle() {
    let mut checked = 0;
    let mut positives = 0;
    let mut pruned = 0;
    for name in FIXTURES {
        let counts = verify_owner(name);
        checked += counts.0;
        positives += counts.1;
        pruned += counts.2;
    }
    assert_eq!(checked, 200);
    assert!(positives > 0 && pruned > 0);
}

#[test]
fn templated_repeated_alpha_matches_recorded_oracle() {
    let mut checked = 0;
    let mut positives = 0;
    for name in FIXTURES {
        let counts = verify_templated(name);
        checked += counts.0;
        positives += counts.1;
    }
    assert_eq!(checked, 200);
    assert!(positives > 0);
}
