use std::collections::BTreeSet;

use pg_foma::capability::ModelLocation;
use pg_foma::recipe_mechanism::{
    BoundaryCleanupSpec, BoundaryGuarantee, BoundaryRequirement, CopySpanGuarantee,
    CopySpanRequirement, DynamicState, ExecutionDisposition, IdentityGuarantee,
    IdentityRequirement, InterfaceContract, MechanismBody, MechanismEdge, MechanismEndpoint,
    MechanismGraph, MechanismGraphError, MechanismId, MechanismSource, MechanismSourceKind,
    MechanismSpec, MorphotacticsSpec, MultiplicityGuarantee, MultiplicityRequirement,
    ProvidedInterface, RequiredInterface, SymbolSpace, WireModelId, WireModelKind,
};
use pg_grammar::chardef::CharDefId;
use pg_grammar::model::{
    AllomorphId, FamilyId, LexEntryId, MRuleId, MorphemeId, MprId, NatClassId, PRuleId, StemNameId,
    StratumId, TableId, TemplateId, VarId,
};

#[test]
fn recipe_mechanism_wire_ids_round_trip_every_native_domain() {
    macro_rules! round_trip {
        ($value:expr, $native:ty) => {
            let native: $native = $value;
            let wire: WireModelId = native.into();
            let recovered = <$native as TryFrom<WireModelId>>::try_from(wire).unwrap();
            assert_eq!(recovered, native);
        };
    }

    round_trip!(CharDefId(1), CharDefId);
    round_trip!(StratumId(1), StratumId);
    round_trip!(MRuleId(2), MRuleId);
    round_trip!(PRuleId(3), PRuleId);
    round_trip!(TemplateId(4), TemplateId);
    round_trip!(LexEntryId(5), LexEntryId);
    round_trip!(MorphemeId(6), MorphemeId);
    round_trip!(AllomorphId(7), AllomorphId);
    round_trip!(NatClassId(8), NatClassId);
    round_trip!(StemNameId(9), StemNameId);
    round_trip!(FamilyId(10), FamilyId);
    round_trip!(TableId(11), TableId);
    round_trip!(VarId(12), VarId);
    round_trip!(MprId(13), MprId);
}

#[test]
fn recipe_mechanism_sources_convert_every_model_location_variant() {
    let cases = [
        (
            ModelLocation::MorphRule(MRuleId(1)),
            MechanismSourceKind::MorphRule,
            Some(WireModelKind::MRule),
            None,
        ),
        (
            ModelLocation::AffixAllomorph {
                rule: MRuleId(2),
                allomorph_index: 3,
            },
            MechanismSourceKind::AffixAllomorph,
            Some(WireModelKind::MRule),
            Some(WireModelKind::AllomorphIndex),
        ),
        (
            ModelLocation::Stratum(StratumId(4)),
            MechanismSourceKind::Stratum,
            Some(WireModelKind::Stratum),
            None,
        ),
        (
            ModelLocation::MprGroup(5),
            MechanismSourceKind::MprGroup,
            Some(WireModelKind::MprGroup),
            None,
        ),
        (
            ModelLocation::PhonRule(PRuleId(6)),
            MechanismSourceKind::PhonRule,
            Some(WireModelKind::PRule),
            None,
        ),
        (
            ModelLocation::RewriteSubrule {
                rule: PRuleId(7),
                subrule_index: 8,
            },
            MechanismSourceKind::RewriteSubrule,
            Some(WireModelKind::PRule),
            Some(WireModelKind::SubruleIndex),
        ),
        (
            ModelLocation::NaturalClass(NatClassId(9)),
            MechanismSourceKind::NaturalClass,
            Some(WireModelKind::NatClass),
            None,
        ),
        (
            ModelLocation::MorphemeCoOccurrence(10),
            MechanismSourceKind::MorphemeCoOccurrence,
            Some(WireModelKind::MorphemeIndex),
            None,
        ),
        (
            ModelLocation::AllomorphCoOccurrence(AllomorphId(11)),
            MechanismSourceKind::AllomorphCoOccurrence,
            Some(WireModelKind::Allomorph),
            None,
        ),
    ];

    for (location, kind, owner_kind, child_kind) in cases {
        let source = MechanismSource::from(location);
        assert_eq!(source.kind, kind);
        assert_eq!(source.owner.as_ref().map(|id| id.kind), owner_kind);
        assert_eq!(source.child.as_ref().map(|id| id.kind), child_kind);
    }
}

fn wire(kind: WireModelKind, value: u64) -> WireModelId {
    WireModelId { kind, value }
}

fn source() -> MechanismSource {
    MechanismSource {
        kind: MechanismSourceKind::Stratum,
        owner: Some(wire(WireModelKind::Stratum, 0)),
        child: None,
    }
}

fn morphotactics(id: &str) -> MechanismSpec {
    MechanismSpec {
        id: MechanismId(id.to_owned()),
        sources: vec![source()],
        stratum: Some(wire(WireModelKind::Stratum, 0)),
        body: MechanismBody::Morphotactics(MorphotacticsSpec {
            strata: vec![wire(WireModelKind::Stratum, 0)],
            templates: vec![wire(WireModelKind::Template, 0)],
            rules: vec![wire(WireModelKind::MRule, 0)],
            cooccurrence_units: vec![vec!["root".to_owned()]],
            priority_chains: vec![],
            max_depth: Some(1),
        }),
    }
}

fn cleanup(id: &str) -> MechanismSpec {
    MechanismSpec {
        id: MechanismId(id.to_owned()),
        sources: vec![source()],
        stratum: Some(wire(WireModelKind::Stratum, 0)),
        body: MechanismBody::BoundaryCleanup(BoundaryCleanupSpec {
            table: wire(WireModelKind::Table, 0),
            boundary_symbols: vec!["#".to_owned()],
        }),
    }
}

fn contract() -> InterfaceContract {
    InterfaceContract {
        provided: ProvidedInterface {
            symbol_space: SymbolSpace::Surface(wire(WireModelKind::Table, 0)),
            analysis_identity: IdentityGuarantee::Preserved,
            root_identity: IdentityGuarantee::Preserved,
            multiplicity: MultiplicityGuarantee::ExactMultiset,
            boundaries: BoundaryGuarantee::Present,
            dynamic_state: DynamicState {
                pos: BTreeSet::new(),
                mpr: BTreeSet::new(),
                lexical_classes: BTreeSet::new(),
                stem_families: BTreeSet::new(),
            },
            stratum: Some(wire(WireModelKind::Stratum, 0)),
            disposition: ExecutionDisposition::ExactFst,
            copy_span: CopySpanGuarantee::None,
        },
        required: RequiredInterface {
            symbol_space: SymbolSpace::Surface(wire(WireModelKind::Table, 0)),
            analysis_identity: IdentityRequirement::Preserved,
            root_identity: IdentityRequirement::Preserved,
            multiplicity: MultiplicityRequirement::ExactMultiset,
            boundaries: BoundaryRequirement::Present,
            dynamic_state: DynamicState {
                pos: BTreeSet::new(),
                mpr: BTreeSet::new(),
                lexical_classes: BTreeSet::new(),
                stem_families: BTreeSet::new(),
            },
            stratum: Some(wire(WireModelKind::Stratum, 0)),
            accepted_dispositions: [ExecutionDisposition::ExactFst].into_iter().collect(),
            copy_span: CopySpanRequirement::None,
        },
    }
}

fn edge(producer: &str, consumer: &str) -> MechanismEdge {
    MechanismEdge {
        producer: MechanismId(producer.to_owned()),
        consumer: MechanismId(consumer.to_owned()),
        contract: contract(),
    }
}

fn graph(nodes: Vec<MechanismSpec>, edges: Vec<MechanismEdge>) -> MechanismGraph {
    MechanismGraph { nodes, edges }
}

#[test]
fn recipe_mechanism_rejects_missing_producer() {
    let err = graph(vec![cleanup("cleanup")], vec![edge("missing", "cleanup")])
        .validate()
        .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::MissingEndpoint {
            endpoint: MechanismEndpoint::Producer,
            ..
        }
    ));
}

#[test]
fn recipe_mechanism_rejects_symbol_space_mismatch() {
    let mut contract = contract();
    contract.required.symbol_space = SymbolSpace::CharDefTokens(wire(WireModelKind::Table, 0));
    let err = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![MechanismEdge {
            producer: MechanismId("morph".to_owned()),
            consumer: MechanismId("cleanup".to_owned()),
            contract,
        }],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::SymbolSpaceMismatch { .. }
    ));
}

#[test]
fn recipe_mechanism_rejects_active_table_mismatch_and_wrong_wire_domains() {
    let mut mismatched_table = contract();
    mismatched_table.required.symbol_space = SymbolSpace::Surface(wire(WireModelKind::Table, 1));
    let err = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![MechanismEdge {
            producer: MechanismId("morph".to_owned()),
            consumer: MechanismId("cleanup".to_owned()),
            contract: mismatched_table,
        }],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::SymbolSpaceMismatch { .. }
    ));

    let mut wrong_domain = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![edge("morph", "cleanup")],
    );
    wrong_domain.edges[0].contract.provided.symbol_space =
        SymbolSpace::Surface(wire(WireModelKind::MRule, 0));
    wrong_domain.edges[0].contract.required.symbol_space =
        SymbolSpace::Surface(wire(WireModelKind::MRule, 0));
    let err = wrong_domain.validate().unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::InvalidWireId {
            expected: WireModelKind::Table,
            ..
        }
    ));

    let mut out_of_range = cleanup("cleanup");
    if let MechanismBody::BoundaryCleanup(spec) = &mut out_of_range.body {
        spec.table.value = u16::MAX as u64 + 1;
    }
    let err = graph(vec![out_of_range], vec![]).validate().unwrap_err();
    assert!(matches!(err, MechanismGraphError::InvalidWireId { .. }));
}

#[test]
fn recipe_mechanism_rejects_boundary_cleanup_before_boundary_consumer() {
    let err = graph(
        vec![
            morphotactics("morph"),
            cleanup("cleanup"),
            cleanup("consumer"),
        ],
        vec![edge("morph", "cleanup"), edge("cleanup", "consumer")],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::CleanupNotTerminal { .. }
    ));
}

#[test]
fn recipe_mechanism_rejects_cleanup_table_mismatch_and_disconnected_paths() {
    let mut wrong_cleanup = cleanup("cleanup");
    if let MechanismBody::BoundaryCleanup(spec) = &mut wrong_cleanup.body {
        spec.table = wire(WireModelKind::Table, 1);
    }
    let err = graph(
        vec![morphotactics("morph"), wrong_cleanup],
        vec![edge("morph", "cleanup")],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::CleanupContractMismatch { .. }
    ));

    let err = graph(
        vec![
            morphotactics("connected"),
            morphotactics("disconnected"),
            cleanup("cleanup"),
        ],
        vec![edge("connected", "cleanup")],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::PathDoesNotTerminateInCleanup { mechanism }
            if mechanism == MechanismId("disconnected".to_owned())
    ));
}

#[test]
fn recipe_mechanism_rejects_duplicate_mechanism_id() {
    let err = graph(
        vec![morphotactics("duplicate"), cleanup("duplicate")],
        vec![],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(err, MechanismGraphError::DuplicateId { .. }));
}

#[test]
fn recipe_mechanism_rejects_cycle() {
    let err = graph(
        vec![
            morphotactics("left"),
            morphotactics("right"),
            morphotactics("tail"),
        ],
        vec![
            edge("left", "right"),
            edge("right", "left"),
            edge("right", "tail"),
        ],
    )
    .validate()
    .unwrap_err();
    assert_eq!(
        err,
        MechanismGraphError::Cycle {
            members: vec![MechanismId("left".into()), MechanismId("right".into())]
        }
    );
}

#[test]
fn recipe_mechanism_rejects_lost_analysis_or_root_identity() {
    let mut contract = contract();
    contract.provided.analysis_identity = IdentityGuarantee::Unknown;
    let err = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![MechanismEdge {
            producer: MechanismId("morph".to_owned()),
            consumer: MechanismId("cleanup".to_owned()),
            contract,
        }],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::UnsatisfiedState {
            field: pg_foma::recipe_mechanism::InterfaceField::Identity,
            ..
        }
    ));
}

#[test]
fn recipe_mechanism_rejects_multiplicity_weakening() {
    let mut contract = contract();
    contract.provided.multiplicity = MultiplicityGuarantee::SetOnly;
    let err = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![MechanismEdge {
            producer: MechanismId("morph".to_owned()),
            consumer: MechanismId("cleanup".to_owned()),
            contract,
        }],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::UnsatisfiedState {
            field: pg_foma::recipe_mechanism::InterfaceField::Multiplicity,
            ..
        }
    ));
}

#[test]
fn recipe_mechanism_rejects_dynamic_state_or_stratum_mismatch() {
    let mut dynamic_contract = contract();
    dynamic_contract
        .provided
        .dynamic_state
        .pos
        .insert("verb".to_owned());
    dynamic_contract
        .required
        .dynamic_state
        .pos
        .insert("noun".to_owned());
    let err = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![MechanismEdge {
            producer: MechanismId("morph".to_owned()),
            consumer: MechanismId("cleanup".to_owned()),
            contract: dynamic_contract,
        }],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(err, MechanismGraphError::UnsatisfiedState { .. }));

    let mut contract = contract();
    contract.required.stratum = Some(wire(WireModelKind::Stratum, 1));
    let err = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![MechanismEdge {
            producer: MechanismId("morph".to_owned()),
            consumer: MechanismId("cleanup".to_owned()),
            contract,
        }],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::UnsatisfiedState {
            field: pg_foma::recipe_mechanism::InterfaceField::Stratum,
            ..
        }
    ));
}

#[test]
fn recipe_mechanism_rejects_boundary_and_copy_span_weakening() {
    let mut boundary = contract();
    boundary.provided.boundaries = BoundaryGuarantee::Removed;
    let err = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![MechanismEdge {
            producer: MechanismId("morph".to_owned()),
            consumer: MechanismId("cleanup".to_owned()),
            contract: boundary,
        }],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::UnsatisfiedState {
            field: pg_foma::recipe_mechanism::InterfaceField::Boundaries,
            ..
        }
    ));

    let mut copy = contract();
    copy.provided.copy_span = CopySpanGuarantee::UnboundedPreserved;
    copy.required.copy_span = CopySpanRequirement::BoundedAtMost(4);
    let err = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![MechanismEdge {
            producer: MechanismId("morph".to_owned()),
            consumer: MechanismId("cleanup".to_owned()),
            contract: copy,
        }],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::UnsatisfiedState {
            field: pg_foma::recipe_mechanism::InterfaceField::CopySpan,
            ..
        }
    ));
}

#[test]
fn recipe_mechanism_rejects_exact_consumer_after_confirm_only_producer() {
    let mut confirm_only_contract = contract();
    confirm_only_contract.provided.disposition = ExecutionDisposition::ConfirmOnly;
    let err = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![MechanismEdge {
            producer: MechanismId("morph".to_owned()),
            consumer: MechanismId("cleanup".to_owned()),
            contract: confirm_only_contract,
        }],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::DispositionMismatch { .. }
    ));

    let mut contract = contract();
    contract.provided.disposition = ExecutionDisposition::Refused;
    contract
        .required
        .accepted_dispositions
        .insert(ExecutionDisposition::Refused);
    let err = graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![MechanismEdge {
            producer: MechanismId("morph".to_owned()),
            consumer: MechanismId("cleanup".to_owned()),
            contract,
        }],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::DispositionMismatch { .. }
    ));
}

#[test]
fn recipe_mechanism_accepts_composable_morphotactics_cleanup_graph() {
    graph(
        vec![morphotactics("morph"), cleanup("cleanup")],
        vec![edge("morph", "cleanup")],
    )
    .validate()
    .expect("the complete morphotactics-to-cleanup graph is composable");
}
