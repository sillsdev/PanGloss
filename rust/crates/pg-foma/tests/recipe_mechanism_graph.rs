//! The mechanism vocabulary.
//!
//! Several assertions the initial commit made are deliberately gone, not relaxed. There is no
//! longer a test that a declared `IdentityGuarantee::Unknown` fails a declared
//! `IdentityRequirement::Preserved`, nor the multiplicity/copy-span/dynamic-state equivalents:
//! those compared two hand-written declarations to each other and would have passed just as
//! happily on Amharic's measured `identity-mismatch` candidate, which declared nothing and was
//! simply wrong. Analysis identity and multiplicity are the parity relation, measured against an
//! oracle, and this vocabulary no longer pretends to assert them.
//!
//! What replaces them is checks an edge cannot fake, because the edge no longer carries anything:
//! symbol space, boundary state and stratum are computed from the two endpoint NODES.

use std::collections::BTreeSet;

use pg_foma::capability::{CharacteristicKind, ModelLocation};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::recipe_mechanism::{
    mechanism_kind_for, BoundaryCleanupSpec, BoundaryState, ExecutionDisposition, InterfaceField,
    MechanismBinding, MechanismBody, MechanismEdge, MechanismEndpoint, MechanismGraph,
    MechanismGraphError, MechanismId, MechanismKind, MechanismNode, MechanismSource,
    MechanismSourceKind, MorphotacticsSpec, SymbolSpace, WireModelId, WireModelKind,
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

/// The `ModelLocation -> MechanismSource` join is total. This is what lets a provider attribute a
/// characteristic observation to a mechanism without touching `Grammar`.
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

    // The one source kind with no `ModelLocation` counterpart.
    let table = MechanismSource::character_table(TableId(0));
    assert_eq!(table.kind, MechanismSourceKind::CharacterTable);
    assert_eq!(
        table.owner.as_ref().map(|id| id.kind),
        Some(WireModelKind::Table)
    );
}

fn wire(kind: WireModelKind, value: u64) -> WireModelId {
    WireModelId { kind, value }
}

fn stratum_source() -> MechanismSource {
    MechanismSource {
        kind: MechanismSourceKind::Stratum,
        owner: Some(wire(WireModelKind::Stratum, 0)),
        child: None,
    }
}

fn morphotactics(id: &str) -> MechanismNode {
    MechanismNode {
        id: MechanismId(id.to_owned()),
        sources: vec![stratum_source()],
        symbol_space: SymbolSpace::Surface(wire(WireModelKind::Table, 0)),
        stratum: Some(wire(WireModelKind::Stratum, 0)),
        construct_requirements: BTreeSet::new(),
        body: MechanismBody::Morphotactics(MorphotacticsSpec {
            templates: vec![wire(WireModelKind::Template, 0)],
            max_depth: Some(1),
        }),
    }
}

fn cleanup(id: &str) -> MechanismNode {
    MechanismNode {
        id: MechanismId(id.to_owned()),
        sources: vec![MechanismSource::character_table(TableId(0))],
        symbol_space: SymbolSpace::Surface(wire(WireModelKind::Table, 0)),
        stratum: Some(wire(WireModelKind::Stratum, 0)),
        construct_requirements: BTreeSet::new(),
        body: MechanismBody::BoundaryCleanup(BoundaryCleanupSpec {
            boundary_symbols: vec!["#".to_owned()],
        }),
    }
}

fn edge(producer: &str, consumer: &str) -> MechanismEdge {
    MechanismEdge {
        producer: MechanismId(producer.to_owned()),
        consumer: MechanismId(consumer.to_owned()),
    }
}

fn graph(nodes: Vec<MechanismNode>, edges: Vec<MechanismEdge>) -> MechanismGraph {
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

/// A mechanism with no typed source reference is a mechanism nobody can justify.
#[test]
fn recipe_mechanism_rejects_a_node_with_no_typed_source() {
    let mut orphan = morphotactics("morph");
    orphan.sources.clear();
    let err = graph(vec![orphan, cleanup("cleanup")], vec![edge("morph", "cleanup")])
        .validate()
        .unwrap_err();
    assert_eq!(
        err,
        MechanismGraphError::MissingSource {
            mechanism: MechanismId("morph".to_owned())
        }
    );
}

#[test]
fn recipe_mechanism_rejects_self_edge() {
    let err = graph(vec![cleanup("cleanup")], vec![edge("cleanup", "cleanup")])
        .validate()
        .unwrap_err();
    assert_eq!(
        err,
        MechanismGraphError::SelfEdge {
            mechanism: MechanismId("cleanup".to_owned())
        }
    );
}

/// Symbol space is now a NODE fact, so a mismatch is a genuine disagreement between two
/// mechanisms rather than a contradiction inside one hand-written contract. Both the alphabet and
/// the table are part of it.
#[test]
fn recipe_mechanism_rejects_symbol_space_and_table_mismatch() {
    let mut alphabet = morphotactics("morph");
    alphabet.symbol_space = SymbolSpace::CharDefTokens(wire(WireModelKind::Table, 0));
    let err = graph(
        vec![alphabet, cleanup("cleanup")],
        vec![edge("morph", "cleanup")],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::SymbolSpaceMismatch { .. }
    ));

    let mut table = morphotactics("morph");
    table.symbol_space = SymbolSpace::Surface(wire(WireModelKind::Table, 1));
    let err = graph(
        vec![table, cleanup("cleanup")],
        vec![edge("morph", "cleanup")],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::SymbolSpaceMismatch { .. }
    ));
}

#[test]
fn recipe_mechanism_rejects_wrong_and_out_of_range_wire_domains() {
    let mut wrong_domain = morphotactics("morph");
    wrong_domain.symbol_space = SymbolSpace::Surface(wire(WireModelKind::MRule, 0));
    let err = graph(vec![wrong_domain], vec![]).validate().unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::InvalidWireId {
            expected: WireModelKind::Table,
            ..
        }
    ));

    let mut out_of_range = cleanup("cleanup");
    out_of_range.symbol_space = SymbolSpace::Surface(WireModelId {
        kind: WireModelKind::Table,
        value: u16::MAX as u64 + 1,
    });
    let err = graph(vec![out_of_range], vec![]).validate().unwrap_err();
    assert!(matches!(err, MechanismGraphError::InvalidWireId { .. }));
}

#[test]
fn recipe_mechanism_rejects_boundary_cleanup_before_another_consumer() {
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

/// Boundary state is DERIVED from the mechanism kind, so the only node that can leave boundaries
/// removed is a cleanup -- and a cleanup with an outgoing edge is caught by `CleanupNotTerminal`
/// first. This test records that the boundary rule is therefore structurally unviolable rather
/// than merely checked: the two derived answers are what they must be, and no vocabulary a caller
/// can write changes either.
#[test]
fn recipe_mechanism_boundary_state_is_derived_not_declared() {
    assert_eq!(morphotactics("m").boundary_input(), BoundaryState::Present);
    assert_eq!(morphotactics("m").boundary_output(), BoundaryState::Present);
    assert_eq!(cleanup("c").boundary_input(), BoundaryState::Present);
    assert_eq!(cleanup("c").boundary_output(), BoundaryState::Removed);
}

#[test]
fn recipe_mechanism_rejects_disconnected_paths() {
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
fn recipe_mechanism_rejects_stratum_mismatch() {
    let mut other_stratum = cleanup("cleanup");
    other_stratum.stratum = Some(wire(WireModelKind::Stratum, 1));
    let err = graph(
        vec![morphotactics("morph"), other_stratum],
        vec![edge("morph", "cleanup")],
    )
    .validate()
    .unwrap_err();
    assert!(matches!(
        err,
        MechanismGraphError::UnsatisfiedState {
            field: InterfaceField::Stratum,
            ..
        }
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

// -------------------------------------------------------------------------------------------
// Bindings: the only place a disposition exists, and it always names its compiler.
// -------------------------------------------------------------------------------------------

/// THE ROW THIS REWORK EXISTS FOR. A mechanism requiring `RealizationalMorphology` is refused by
/// `PlanComposed` -- whose only lexicon emitter writes no line for a realizational rule -- and
/// exact for the whole-grammar compilers. Same node, same graph, three different answers, each
/// carrying the compiler's name. Under the initial commit's vocabulary this fact was
/// inexpressible: `ExecutionDisposition` lived on an edge contract with no strategy anywhere in
/// the module.
#[test]
fn recipe_mechanism_binding_answers_per_compiler_and_never_anonymously() {
    let mut node = morphotactics("morph");
    node.construct_requirements = [CharacteristicKind::RealizationalMorphology]
        .into_iter()
        .collect();
    let g = graph(vec![node, cleanup("cleanup")], vec![edge("morph", "cleanup")]);
    g.validate().expect("graph is composable regardless of compiler");

    let refused = g.refusals(EmissionStrategy::PlanComposed);
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].mechanism(), &MechanismId("morph".to_owned()));
    assert_eq!(refused[0].strategy(), EmissionStrategy::PlanComposed);
    assert!(
        !refused[0].limiting_rows().is_empty(),
        "a refusal must carry strategy_coverage's own citation"
    );

    for strategy in [
        EmissionStrategy::TunedSurfaceProbed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ] {
        assert!(
            g.refusals(strategy).is_empty(),
            "{strategy:?} represents RealizationalMorphology"
        );
        let bindings = g.bind(strategy);
        assert!(bindings
            .iter()
            .all(|b| b.disposition() == ExecutionDisposition::ExactFst));
        assert!(bindings.iter().all(|b| b.strategy() == strategy));
    }
}

/// A documented partial gap is confirm-gated, never silently exact and never a refusal.
#[test]
fn recipe_mechanism_binding_folds_a_known_gap_to_confirm_only() {
    let mut node = morphotactics("morph");
    node.construct_requirements = [CharacteristicKind::CircumfixOutputAction]
        .into_iter()
        .collect();
    let binding = MechanismBinding::derive(&node, EmissionStrategy::PlanComposed);
    assert_eq!(binding.disposition(), ExecutionDisposition::ConfirmOnly);
    assert_eq!(binding.limiting_rows().len(), 1);
}

/// Every construct lands in exactly one mechanism, and every mechanism receives at least one.
#[test]
fn recipe_mechanism_routing_is_a_total_onto_partition_of_the_construct_set() {
    let reached: BTreeSet<MechanismKind> = CharacteristicKind::ALL
        .iter()
        .copied()
        .map(mechanism_kind_for)
        .collect();
    assert_eq!(
        reached,
        MechanismKind::COMPOSITION_ORDER
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(MechanismKind::COMPOSITION_ORDER.len(), 6);
}
