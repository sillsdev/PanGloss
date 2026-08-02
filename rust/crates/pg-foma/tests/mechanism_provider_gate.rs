//! Task 7.4: providers derive from the shared `GrammarSemantics` and from nothing else.
//!
//! Synthetic, delanguaged fixtures only (no natural-language names, per this repo's standing
//! conformance rule), built through `pg_grammar::load` exactly as `capability.rs`'s own test module
//! does.

use pg_foma::capability::CharacteristicKind;
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::mechanism_provider::derive_mechanism_graph;
use pg_foma::recipe_mechanism::{
    ExecutionDisposition, MechanismBody, MechanismId, MechanismKind, MechanismSourceKind,
};
use pg_grammar::model::Grammar;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// A single affixation rule whose only allomorph carries `redupMorphType="prefix"` -- a
/// `ReduplicationHint` -- while its output copies its one input part EXACTLY ONCE. The hint is
/// therefore inert: nothing is reduplicated.
const INERT_REDUPLICATION_HINT_XML: &str = r#"<HermitCrabInput><Language><Name>InertHint</Name>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
    <BoundaryDefinitions>
      <BoundaryDefinition id="b1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
    </BoundaryDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRules="mr1">
      <Name>S</Name>
      <LexicalEntries>
        <LexicalEntry id="e0">
          <Allomorphs><Allomorph id="allo0"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          <Gloss>e0</Gloss>
        </LexicalEntry>
      </LexicalEntries>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mr1">
          <Name>inert</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="sub1">
              <MorphologicalInput>
                <PhoneticSequence id="q0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
              </MorphologicalInput>
              <MorphologicalOutput redupMorphType="prefix">
                <CopyFromInput index="q0" />
              </MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
        </MorphologicalRule>
      </MorphologicalRuleDefinitions>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// The same shape, but the output copies its one input part TWICE -- real reduplication, which
/// `rhs_has_true_reduplication` (the single authority) recognizes.
const TRUE_REDUPLICATION_XML: &str = r#"<HermitCrabInput><Language><Name>TrueRedup</Name>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
    <BoundaryDefinitions>
      <BoundaryDefinition id="b1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
    </BoundaryDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRules="mr1">
      <Name>S</Name>
      <LexicalEntries>
        <LexicalEntry id="e0">
          <Allomorphs><Allomorph id="allo0"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
          <Gloss>e0</Gloss>
        </LexicalEntry>
      </LexicalEntries>
      <MorphologicalRuleDefinitions>
        <MorphologicalRule id="mr1">
          <Name>redup</Name>
          <MorphologicalSubrules>
            <MorphologicalSubrule id="sub1">
              <MorphologicalInput>
                <PhoneticSequence id="q0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
              </MorphologicalInput>
              <MorphologicalOutput redupMorphType="prefix">
                <CopyFromInput index="q0" />
                <CopyFromInput index="q0" />
              </MorphologicalOutput>
            </MorphologicalSubrule>
          </MorphologicalSubrules>
        </MorphologicalRule>
      </MorphologicalRuleDefinitions>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// A grammar declaring a table and nothing else: no rules, no entries, no observations.
const EMPTY_XML: &str = r#"<HermitCrabInput><Language><Name>Empty</Name>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <Strata>
    <Stratum characterDefinitionTable="t1"><Name>S</Name></Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

fn kinds(graph: &pg_foma::recipe_mechanism::MechanismGraph) -> Vec<MechanismKind> {
    graph.nodes.iter().map(|n| n.kind()).collect()
}

// ---------------------------------------------------------------------------------------------
// REQUIRED TEST 1: canonical graph identity is byte-identical across a fresh load.
// ---------------------------------------------------------------------------------------------

/// Two INDEPENDENT loads of the same source, each with its own `GrammarSemantics`, must produce
/// graphs that are equal as data and byte-identical as a projection.
///
/// This is not free. Every collection reaching the projection has a source whose natural order is
/// a hash order: `gate::partition_entries` returns `HashMap` iteration order (which is why
/// `GrammarSemantics::entry_partition` sorts by gate key), a group's `entries` is a `HashSet`
/// (which is why the provider sorts members), and grouping the observations goes through a map
/// (which is why it is a `BTreeMap` and the node order is `COMPOSITION_ORDER`). Remove any one of
/// those and this assertion fails on a rerun.
#[test]
fn canonical_graph_identity_is_byte_identical_across_a_fresh_load() {
    for xml in [
        INERT_REDUPLICATION_HINT_XML,
        TRUE_REDUPLICATION_XML,
        EMPTY_XML,
    ] {
        let g1 = load(xml);
        let g2 = load(xml);
        let a = derive_mechanism_graph(&GrammarSemantics::derive(&g1));
        let b = derive_mechanism_graph(&GrammarSemantics::derive(&g2));

        assert_eq!(a, b, "two fresh loads produced different graphs");
        assert_eq!(
            a.canonical_projection(),
            b.canonical_projection(),
            "two fresh loads produced different canonical projections"
        );

        // ...and re-deriving from the SAME semantics is stable too, so a memoized field cannot
        // hand the second caller a different answer than the first.
        let semantics = GrammarSemantics::derive(&g1);
        assert_eq!(
            derive_mechanism_graph(&semantics).canonical_projection(),
            derive_mechanism_graph(&semantics).canonical_projection()
        );

        a.validate().expect("a derived graph must validate");
    }
}

// ---------------------------------------------------------------------------------------------
// REQUIRED TEST 2: an inert hint creates no mechanism.
// ---------------------------------------------------------------------------------------------

/// A `ReduplicationHint` on an allomorph that does not actually reduplicate must create NO
/// `CopyProcess` mechanism -- and the second half of the test is what makes the first half mean
/// something: the identically-shaped grammar that DOES reduplicate creates exactly one.
///
/// The repo's standing trap here is the `redup_hint != Implicit` shortcut, which
/// `rhs_has_true_reduplication`'s own doc names. The provider is immune to it by construction: it
/// never reads the hint, only `characterize`'s observations, and `characterize` uses the structural
/// test. This pins that the immunity is real rather than incidental.
#[test]
fn an_inert_reduplication_hint_creates_no_copy_process_mechanism() {
    let inert = load(INERT_REDUPLICATION_HINT_XML);
    let inert_sem = GrammarSemantics::derive(&inert);

    // The hint IS present in the loaded model -- otherwise the fixture would prove nothing.
    assert!(
        matches!(
            &inert.mrules[0],
            pg_grammar::model::MorphRuleDef::AffixProcess(def)
                if def.allomorphs[0].redup_hint != pg_grammar::model::ReduplicationHint::Implicit
        ),
        "fixture must actually carry a non-Implicit ReduplicationHint"
    );
    // ...and the semantic owner correctly reports it as no reduplication.
    assert!(!inert_sem.has_reduplication());

    let inert_graph = derive_mechanism_graph(&inert_sem);
    assert!(
        !kinds(&inert_graph).contains(&MechanismKind::CopyProcess),
        "an inert hint created a CopyProcess mechanism: {:?}",
        kinds(&inert_graph)
    );
    assert!(
        !inert_graph
            .nodes
            .iter()
            .any(|n| n.construct_requirements.contains(&CharacteristicKind::Reduplication)),
        "an inert hint created a Reduplication requirement"
    );

    let real = load(TRUE_REDUPLICATION_XML);
    let real_graph = derive_mechanism_graph(&GrammarSemantics::derive(&real));
    assert_eq!(
        kinds(&real_graph)
            .iter()
            .filter(|k| **k == MechanismKind::CopyProcess)
            .count(),
        1,
        "real reduplication must create exactly one CopyProcess mechanism: {:?}",
        kinds(&real_graph)
    );
    let copy = real_graph
        .node(&MechanismId(MechanismKind::CopyProcess.label().to_owned()))
        .expect("the copy mechanism is named by its kind");
    assert!(copy
        .construct_requirements
        .contains(&CharacteristicKind::Reduplication));
    // Peel handles reduplication outside the compiled FST for every strategy alike.
    for strategy in [
        EmissionStrategy::PlanComposed,
        EmissionStrategy::TunedSurfaceProbed,
        EmissionStrategy::TemplatedUnderlyingTokens,
    ] {
        assert_eq!(
            pg_foma::recipe_mechanism::MechanismBinding::derive(copy, strategy).disposition(),
            ExecutionDisposition::Peeled,
            "{strategy:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Structure of a derived graph.
// ---------------------------------------------------------------------------------------------

/// A grammar with no observed construct derives an EMPTY graph -- not a skeleton of six empty
/// mechanisms, and not a lone cleanup with nothing to clean. A mechanism exists because something
/// was observed.
#[test]
fn a_grammar_with_no_observations_derives_no_mechanisms() {
    let g = load(EMPTY_XML);
    let semantics = GrammarSemantics::derive(&g);
    assert!(semantics.characteristics().observations().is_empty());

    let graph = derive_mechanism_graph(&semantics);
    assert!(graph.nodes.is_empty(), "{:?}", kinds(&graph));
    assert!(graph.edges.is_empty());
    graph.validate().expect("an empty graph validates");
}

/// Nodes appear in the single canonical composition order, edges chain the present ones, and the
/// terminal mechanism is always cleanup. There is exactly one such order -- no permutation of it is
/// representable, because Wave 3 measured plan-shape permutation to vary nothing.
#[test]
fn a_derived_graph_is_a_canonical_spine_terminating_in_cleanup() {
    let g = load(TRUE_REDUPLICATION_XML);
    let graph = derive_mechanism_graph(&GrammarSemantics::derive(&g));
    graph.validate().expect("derived graph validates");

    let observed = kinds(&graph);
    let expected_order: Vec<MechanismKind> = MechanismKind::COMPOSITION_ORDER
        .iter()
        .copied()
        .filter(|k| observed.contains(k))
        .collect();
    assert_eq!(observed, expected_order, "nodes are not in composition order");
    assert_eq!(observed.last(), Some(&MechanismKind::BoundaryCleanup));
    assert_eq!(graph.edges.len(), graph.nodes.len() - 1, "not a single chain");

    // Cleanup's source is the character table it cleans -- the one source kind with no
    // `ModelLocation` counterpart -- and its body carries the table's own boundary inventory.
    let cleanup = graph
        .node(&MechanismId(MechanismKind::BoundaryCleanup.label().to_owned()))
        .expect("cleanup exists");
    assert!(cleanup
        .sources
        .iter()
        .any(|s| s.kind == MechanismSourceKind::CharacterTable));
    match &cleanup.body {
        MechanismBody::BoundaryCleanup(spec) => {
            assert_eq!(spec.boundary_symbols, vec!["+".to_owned()]);
        }
        other => panic!("cleanup body is {other:?}"),
    }
}

/// Every node's typed sources trace back to a real characteristic observation (or, for cleanup, to
/// the character table), and every requirement is a construct that was actually observed. A
/// mechanism cannot appear for a construct the grammar does not contain.
#[test]
fn every_requirement_and_source_traces_to_an_observation() {
    let g = load(TRUE_REDUPLICATION_XML);
    let semantics = GrammarSemantics::derive(&g);
    let observed: Vec<CharacteristicKind> = semantics
        .characteristics()
        .observations()
        .iter()
        .map(|o| o.kind)
        .collect();
    let graph = derive_mechanism_graph(&semantics);

    for node in &graph.nodes {
        assert!(!node.sources.is_empty(), "{:?} has no source", node.id);
        for requirement in &node.construct_requirements {
            assert!(
                observed.contains(requirement),
                "{:?} requires {requirement:?}, which was never observed",
                node.id
            );
            assert_eq!(
                pg_foma::recipe_mechanism::mechanism_kind_for(*requirement),
                node.kind(),
                "{requirement:?} landed on the wrong mechanism"
            );
        }
    }
}
