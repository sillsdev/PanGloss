use super::*;

/// Every construct is routed to exactly one mechanism, and every mechanism kind is reached (the partition is also onto).
#[test]
fn every_construct_routes_and_every_mechanism_is_reached() {
    let mut reached = BTreeSet::new();
    for &kind in CharacteristicKind::ALL {
        reached.insert(mechanism_kind_for(kind));
    }
    let all: BTreeSet<MechanismKind> = MechanismKind::COMPOSITION_ORDER.iter().copied().collect();
    assert_eq!(
        reached, all,
        "some mechanism kind has no construct routed to it"
    );
}

/// The disposition of the same mechanism differs by compiler; if it did not, `strategy` would be decoration.
#[test]
fn one_mechanism_gets_different_dispositions_from_different_compilers() {
    let node = MechanismNode {
        id: MechanismId("morphotactics".to_owned()),
        sources: vec![MechanismSource {
            kind: MechanismSourceKind::MorphRule,
            owner: Some(MRuleId(0).into()),
            child: None,
        }],
        symbol_space: SymbolSpace::Surface(TableId(0).into()),
        stratum: None,
        construct_requirements: [CharacteristicKind::ProcessMorphology]
            .into_iter()
            .collect(),
        body: MechanismBody::Morphotactics(MorphotacticsSpec {
            templates: vec![],
            max_depth: None,
        }),
    };

    let whole = MechanismBinding::derive(&node, EmissionStrategy::TunedSurfaceProbed);
    assert_eq!(whole.disposition(), ExecutionDisposition::ExactFst);
    assert!(whole.limiting_rows().is_empty());
}

/// Each representation maps to its disposition, including the two (refusal, known gap) no registered strategy currently produces.
#[test]
fn disposition_follows_the_worst_requirement_row() {
    use StrategyRepresentation::{CannotRepresent, Represents, RepresentsWithKnownGap};
    assert_eq!(
        disposition_for(CannotRepresent, MechanismKind::Morphotactics),
        ExecutionDisposition::Refused
    );
    assert_eq!(
        disposition_for(RepresentsWithKnownGap, MechanismKind::Morphotactics),
        ExecutionDisposition::ConfirmOnly
    );
    assert_eq!(
        disposition_for(Represents, MechanismKind::CopyProcess),
        ExecutionDisposition::Peeled
    );
    assert_eq!(
        disposition_for(Represents, MechanismKind::Morphotactics),
        ExecutionDisposition::ExactFst
    );
}

#[test]
fn mpr_wire_ids_cover_the_full_u32_identity_space() {
    for value in [255, 256, 299, u32::MAX] {
        let wire = WireModelId::from(MprId(value));
        assert!(wire.has_valid_range());
        assert_eq!(MprId::try_from(wire).unwrap(), MprId(value));
    }
    let invalid = WireModelId { kind: WireModelKind::Mpr, value: u32::MAX as u64 + 1 };
    assert!(!invalid.has_valid_range());
    assert!(MprId::try_from(invalid).is_err());
}
