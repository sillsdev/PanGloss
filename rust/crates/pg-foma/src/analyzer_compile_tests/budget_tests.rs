//! Trust classification regression tests for Foma proposal construction.

use super::*;

#[test]
fn partial_emission_requires_the_explicit_unproven_constructor() {
    assert!(tier_requires_unproven_build(&FomaTier::Partial {
        uncovered: 1
    }));
    assert!(!tier_requires_unproven_build(&FomaTier::Full));
    assert!(!tier_requires_unproven_build(&FomaTier::Unsupported {
        reason: "synthetic refusal".to_string()
    }));
}
