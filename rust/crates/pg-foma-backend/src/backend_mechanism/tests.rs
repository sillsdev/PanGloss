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

    let holed = MechanismBinding::derive(&node, EmissionStrategy::PlanComposed);
    assert_eq!(holed.disposition(), ExecutionDisposition::Refused);
    assert_eq!(holed.strategy(), EmissionStrategy::PlanComposed);
    assert_eq!(
        holed.limiting_rows().len(),
        1,
        "the refusal must carry strategy_coverage's own citation"
    );

    let whole = MechanismBinding::derive(&node, EmissionStrategy::TunedSurfaceProbed);
    assert_eq!(whole.disposition(), ExecutionDisposition::ExactFst);
    assert!(whole.limiting_rows().is_empty());
}
