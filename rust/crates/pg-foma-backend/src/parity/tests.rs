use super::*;
use crate::test_support::{parity_analysis as analysis, parity_fixture_grammar as test_grammar};

#[test]
fn duplicate_paths_collapse_into_one_set_member_and_are_counted() {
    let g = test_grammar();
    let projected = OccurrenceIdentities::project(&[analysis(0), analysis(0), analysis(1)], &g)
        .expect("every ordinal has a model row");
    assert_eq!(projected.len(), 2, "two DISTINCT identities");
    assert_eq!(projected.raw_analyses(), 3);
    assert_eq!(projected.collapsed_paths(), 1);
    let duplicated_key = g.morphemes[0].xml_key.clone();
    let first = projected
        .entries()
        .iter()
        .find(|entry| entry.identity.morphemes == vec![Some(duplicated_key.clone())])
        .expect("the duplicated identity is a member");
    assert_eq!(first.duplicate_paths, 2);
}

#[test]
fn discovery_order_does_not_change_the_set() {
    let g = test_grammar();
    let forward = OccurrenceIdentities::project(&[analysis(0), analysis(1)], &g).unwrap();
    let reversed = OccurrenceIdentities::project(&[analysis(1), analysis(0)], &g).unwrap();
    assert!(forward.same_identities(&reversed));
    assert_eq!(
        forward, reversed,
        "canonical order makes the values equal too"
    );
}

#[test]
fn an_unresolvable_ordinal_is_a_projection_error_not_an_empty_set() {
    let g = test_grammar();
    let err = OccurrenceIdentities::project(&[analysis(9_999)], &g)
        .expect_err("ordinal 9999 has no model row");
    assert!(matches!(
        err,
        IdentityError::UnresolvedMorpheme { ordinal: 9_999 }
    ));
}

#[test]
fn an_admission_key_standing_for_two_identities_is_counted_not_ignored() {
    // Two identities differing only in `category` -- the shape that makes admission-key containment coarser than identity containment, so it must be COUNTED, not collapsed.
    let shared = vec![Some("m0".to_string())];
    let one = AnalysisIdentity {
        morphemes: shared.clone(),
        root_index: 0,
        category: Some("posA".into()),
    };
    let two = AnalysisIdentity {
        morphemes: shared,
        root_index: 0,
        category: Some("posB".into()),
    };
    // Built directly, not projected: the property under test is a property of the SET, not of projection.
    let set = OccurrenceIdentities {
        entries: vec![
            IdentityEvidence {
                identity: one,
                duplicate_paths: 1,
                guessed: false,
                supplied_root: false,
            },
            IdentityEvidence {
                identity: two,
                duplicate_paths: 1,
                guessed: false,
                supplied_root: false,
            },
        ],
    };
    assert_eq!(set.admission_key_collisions(), 1);

    // A set whose members differ in a component of the key itself has no collision.
    let g = test_grammar();
    let distinct = OccurrenceIdentities::project(&[analysis(0), analysis(1)], &g).unwrap();
    assert_eq!(distinct.len(), 2);
    assert_eq!(distinct.admission_key_collisions(), 0);
}

#[test]
fn divergence_separates_the_two_directions_and_never_calls_nothing_agreement() {
    let g = test_grammar();
    let oracle = OccurrenceIdentities::project(&[analysis(0), analysis(1)], &g).unwrap();
    let candidate = OccurrenceIdentities::project(&[analysis(1), analysis(2)], &g).unwrap();
    let divergence = IdentityDivergence::compare(&oracle, &candidate);
    assert_eq!(divergence.occurrences_compared, 1);
    assert_eq!(divergence.oracle_identities, 2);
    assert_eq!(divergence.candidate_identities, 2);
    // `analysis(0)` is oracle-only (recall failure); `analysis(2)` is candidate-only (soundness hazard) -- never summed into one "they differ" number.
    assert_eq!(divergence.oracle_only_identities, 1);
    assert_eq!(divergence.candidate_only_identities, 1);
    assert_eq!(divergence.occurrences_with_candidate_only, 1);
    assert!(
        !divergence.supports_free_containment(),
        "a candidate-only identity must withdraw support for free containment"
    );

    let agreeing = IdentityDivergence::compare(&oracle, &oracle);
    assert_eq!(agreeing.candidate_only_identities, 0);
    assert_eq!(agreeing.oracle_only_identities, 0);
    assert!(agreeing.supports_free_containment());

    // "I could not look" is not "everything is fine": zero candidate-only identities over zero compared occurrences supports nothing.
    let blind = IdentityDivergence::not_compared(7);
    assert_eq!(blind.candidate_only_identities, 0);
    assert_eq!(blind.occurrences_not_compared, 7);
    assert!(
        !blind.supports_free_containment(),
        "an uncompared run must not read as a clean run"
    );

    let mut total = agreeing;
    total.absorb(blind);
    assert_eq!(total.occurrences_compared, 1);
    assert_eq!(total.occurrences_not_compared, 7);
}

#[test]
fn mismatch_direction_distinguishes_recall_miss_from_over_generation() {
    let g = test_grammar();
    let one = OccurrenceIdentities::project(&[analysis(1)], &g).unwrap();
    let one_and_two = OccurrenceIdentities::project(&[analysis(1), analysis(2)], &g).unwrap();
    let zero_and_one = OccurrenceIdentities::project(&[analysis(0), analysis(1)], &g).unwrap();

    // Candidate is missing `analysis(0)` and offers nothing extra: a pure recall miss.
    let recall_miss = IdentityDivergence::compare(&zero_and_one, &one);
    assert_eq!(recall_miss.oracle_only_identities, 1);
    assert_eq!(recall_miss.candidate_only_identities, 0);
    assert_eq!(
        IdentityMismatchDirection::from_mismatch_divergence(&recall_miss),
        IdentityMismatchDirection::RecallMiss
    );

    // Candidate finds everything the oracle found, plus `analysis(2)` the oracle never licensed: a pure surviving over-generation.
    let over_generation = IdentityDivergence::compare(&one, &one_and_two);
    assert_eq!(over_generation.oracle_only_identities, 0);
    assert_eq!(over_generation.candidate_only_identities, 1);
    assert_eq!(
        IdentityMismatchDirection::from_mismatch_divergence(&over_generation),
        IdentityMismatchDirection::OverGeneration
    );

    // Both directions at once must not collapse into either single-direction variant.
    let both = IdentityDivergence::compare(&zero_and_one, &one_and_two);
    assert_eq!(both.oracle_only_identities, 1);
    assert_eq!(both.candidate_only_identities, 1);
    assert_eq!(
        IdentityMismatchDirection::from_mismatch_divergence(&both),
        IdentityMismatchDirection::Both
    );
}

#[test]
fn the_admission_key_is_the_key_confirm_routes_on() {
    // `admission_key` must equal `(morpheme_ids, root_morpheme_index)`, the pair `confirm_batch` routes on, pinned by `the_admission_key_is_the_key_confirm_routes_on`.
    let a = analysis(1);
    assert_eq!(
        admission_key(&a),
        (a.morpheme_ids.clone(), a.root_morpheme_index)
    );
}

#[test]
fn every_fault_names_its_cause_and_its_side() {
    // The six stage strings are a closed set that gates match exactly; pin them.
    let projection = ParityFault::IdentityProjection {
        side: ParitySide::Oracle,
        error: IdentityError::UnresolvedMorpheme { ordinal: 1 },
    };
    assert_eq!(projection.stage(), "identity-projection-failed-oracle");
    assert_eq!(
        ParityFault::SuppliedRoot {
            side: ParitySide::Candidate
        }
        .stage(),
        "supplied-root-refused-candidate"
    );
    assert_eq!(
        ParityFault::Guessed {
            side: ParitySide::Oracle
        }
        .stage(),
        "guessing-refused-oracle"
    );
}

#[test]
fn projection_records_annotations_while_certification_refuses_them() {
    // Layering: `project` annotates, `certified_occurrence` applies policy -- a policy-only test couldn't distinguish "annotated then refused" from "never recorded".
    let g = test_grammar();
    let mut guessed = analysis(0);
    guessed.guessed = true;
    let projected = OccurrenceIdentities::project(std::slice::from_ref(&guessed), &g)
        .expect("a guessed analysis still projects");
    assert!(projected.any_guessed());
    assert_eq!(projected.len(), 1, "policy is not applied by the projector");

    let refused = certified_occurrence(std::slice::from_ref(&guessed), &g, ParitySide::Oracle)
        .expect_err("v1 certification disables guessing");
    assert_eq!(refused.stage(), "guessing-refused-oracle");
}
