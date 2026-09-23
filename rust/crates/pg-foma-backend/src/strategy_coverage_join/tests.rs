use super::*;

#[test]
fn no_exhibiting_fixture_is_no_evidence_regardless_of_representation() {
    for rep in [
        StrategyRepresentation::Represents,
        StrategyRepresentation::RepresentsWithKnownGap,
        StrategyRepresentation::CannotRepresent,
    ] {
        assert_eq!(
            classify(rep, false, false),
            JoinVerdict::NoEvidence,
            "{rep:?}"
        );
    }
}

#[test]
fn cannot_represent_with_an_exact_witness_is_contradicted() {
    assert_eq!(
        classify(StrategyRepresentation::CannotRepresent, true, true),
        JoinVerdict::Contradicted
    );
}

#[test]
fn cannot_represent_with_no_exact_witness_is_agreed() {
    assert_eq!(
        classify(StrategyRepresentation::CannotRepresent, true, false),
        JoinVerdict::Agreed
    );
}

#[test]
fn represents_with_an_exact_witness_is_agreed() {
    assert_eq!(
        classify(StrategyRepresentation::Represents, true, true),
        JoinVerdict::Agreed
    );
    assert_eq!(
        classify(StrategyRepresentation::RepresentsWithKnownGap, true, true),
        JoinVerdict::Agreed
    );
}

#[test]
fn represents_with_no_exact_witness_is_unsupported_never_contradicted() {
    assert_eq!(
        classify(StrategyRepresentation::Represents, true, false),
        JoinVerdict::Unsupported
    );
    assert_eq!(
        classify(StrategyRepresentation::RepresentsWithKnownGap, true, false),
        JoinVerdict::Unsupported
    );
}

fn exactness(label: &str, exact: bool) -> FixtureExactness {
    FixtureExactness {
        label: label.to_string(),
        exact,
    }
}

#[test]
fn witnesses_prefer_exact_fixtures_and_name_the_contradiction() {
    let exhibiting = [
        exactness("refuses-here", false),
        exactness("works-here", true),
    ];
    let (verdict, witnesses) =
        classify_with_witnesses(StrategyRepresentation::CannotRepresent, &exhibiting);
    assert_eq!(verdict, JoinVerdict::Contradicted);
    assert_eq!(witnesses, vec!["works-here".to_string()]);
}

#[test]
fn witnesses_name_every_exhibiting_fixture_when_none_is_exact() {
    let exhibiting = [exactness("a", false), exactness("b", false)];
    let (verdict, witnesses) =
        classify_with_witnesses(StrategyRepresentation::Represents, &exhibiting);
    assert_eq!(verdict, JoinVerdict::Unsupported);
    assert_eq!(witnesses, vec!["a".to_string(), "b".to_string()]);
}

/// Feeding in every id a kind maps to must recover that kind (no second hand-copied mapping).
#[test]
fn kinds_exercised_by_recovers_every_kind_from_its_own_construct_ids() {
    for &kind in CharacteristicKind::ALL {
        let ids: HashSet<&str> = construct_ids_for(kind).iter().copied().collect();
        if ids.is_empty() {
            continue; // Unmappable kinds (none today) have nothing to recover from.
        }
        let recovered = kinds_exercised_by(&ids);
        assert!(
            recovered.contains(&kind),
            "{kind:?} not recovered from its own ids {ids:?}"
        );
    }
}

#[test]
fn kinds_exercised_by_is_empty_for_an_unknown_id() {
    let ids: HashSet<&str> = ["not-a-real-construct-id"].into_iter().collect();
    assert!(kinds_exercised_by(&ids).is_empty());
}
