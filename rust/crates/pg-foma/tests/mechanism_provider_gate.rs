//! Providers derive from the shared `GrammarSemantics` and from nothing else. Synthetic fixtures only, built through `pg_grammar::load`.

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

/// A single affixation rule carrying a `ReduplicationHint` whose output copies its one input part EXACTLY ONCE, so the hint is inert: nothing is reduplicated.
const INERT_REDUPLICATION_HINT_XML: &str = r#"<HermitCrabInput><Language><Name>InertHint</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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

/// The same shape, but the output copies its one input part TWICE -- real reduplication, recognized by `rhs_has_true_reduplication`.
const TRUE_REDUPLICATION_XML: &str = r#"<HermitCrabInput><Language><Name>TrueRedup</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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

/// Two MPR-gated phonological subrules over six lexical entries, so `gate::partition_entries` produces FOUR groups, two with more than one member -- what makes the byte-identity test non-vacuous, since `partition_entries` collects into a `HashMap` and needs both the group and member sorts to be stable.
const GATED_PARTITION_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>GatedPartition</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <MorphologicalPhonologicalRuleFeatures>
    <MorphologicalPhonologicalRuleFeature id="mpr1">f1</MorphologicalPhonologicalRuleFeature>
    <MorphologicalPhonologicalRuleFeature id="mpr2">f2</MorphologicalPhonologicalRuleFeature>
  </MorphologicalPhonologicalRuleFeatures>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
    <BoundaryDefinitions>
      <BoundaryDefinition id="b1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
    </BoundaryDefinitions>
  </CharacterDefinitionTable>
  <PhonologicalRuleDefinitions>
    <PhonologicalRule id="prule1"><Name>gate1</Name>
      <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
      <PhonologicalSubrules>
        <PhonologicalSubrule requiredMPRFeatures="mpr1">
          <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
        </PhonologicalSubrule>
      </PhonologicalSubrules>
    </PhonologicalRule>
    <PhonologicalRule id="prule2"><Name>gate2</Name>
      <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
      <PhonologicalSubrules>
        <PhonologicalSubrule requiredMPRFeatures="mpr2">
          <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
        </PhonologicalSubrule>
      </PhonologicalSubrules>
    </PhonologicalRule>
  </PhonologicalRuleDefinitions>
  <Strata>
    <Stratum characterDefinitionTable="t1" phonologicalRules="prule1 prule2"><Name>S</Name>
      <LexicalEntries>
        <LexicalEntry id="e0" partOfSpeech="posV"><Allomorphs><Allomorph id="a0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs><Gloss>e0</Gloss></LexicalEntry>
        <LexicalEntry id="e1" partOfSpeech="posV"><Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs><Gloss>e1</Gloss></LexicalEntry>
        <LexicalEntry id="e2" partOfSpeech="posV" ruleFeatures="mpr1"><Allomorphs><Allomorph id="a2"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs><Gloss>e2</Gloss></LexicalEntry>
        <LexicalEntry id="e3" partOfSpeech="posV" ruleFeatures="mpr1"><Allomorphs><Allomorph id="a3"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs><Gloss>e3</Gloss></LexicalEntry>
        <LexicalEntry id="e4" partOfSpeech="posV" ruleFeatures="mpr2"><Allomorphs><Allomorph id="a4"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs><Gloss>e4</Gloss></LexicalEntry>
        <LexicalEntry id="e5" partOfSpeech="posV" ruleFeatures="mpr1 mpr2"><Allomorphs><Allomorph id="a5"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs><Gloss>e5</Gloss></LexicalEntry>
      </LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#;

/// A grammar declaring one table and one otherwise-empty stratum. NOT an observation-free grammar: `capability::characterize` raises one `OrderedMorphRuleApplication` per stratum unconditionally, so a truly-empty graph never occurs for a loadable fixture.
const MINIMAL_XML: &str = r#"<HermitCrabInput><Language><Name>Minimal</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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

// REQUIRED TEST 1: canonical graph identity is byte-identical across a fresh load.

/// Two INDEPENDENT loads of the same source, each with its own `GrammarSemantics`, must produce graphs that are equal as data and byte-identical as a projection -- not free, since every collection reaching the projection has a hash-order source that needs an explicit sort to stay stable.
#[test]
fn canonical_graph_identity_is_byte_identical_across_a_fresh_load() {
    for xml in [
        INERT_REDUPLICATION_HINT_XML,
        TRUE_REDUPLICATION_XML,
        GATED_PARTITION_XML,
        MINIMAL_XML,
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

        // ...and re-deriving from the SAME semantics is stable too, so a memoized field can't hand the second caller a different answer.
        let semantics = GrammarSemantics::derive(&g1);
        assert_eq!(
            derive_mechanism_graph(&semantics).canonical_projection(),
            derive_mechanism_graph(&semantics).canonical_projection()
        );

        a.validate().expect("a derived graph must validate");
    }
}

// REQUIRED TEST 2: an inert hint creates no mechanism.

/// A `ReduplicationHint` on an allomorph that doesn't actually reduplicate must create NO `CopyProcess` mechanism, and the identically-shaped grammar that DOES reduplicate must create exactly one -- pinning that the provider is immune to the `redup_hint != Implicit` shortcut trap by construction.
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
        !inert_graph.nodes.iter().any(|n| n
            .construct_requirements
            .contains(&CharacteristicKind::Reduplication)),
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

// Structure of a derived graph.

/// A mechanism exists because a construct was OBSERVED, so a grammar observing one construct derives one mechanism (plus terminal cleanup) with the other FOUR absent -- the one construct is not a fixture choice, since `characterize` raises `OrderedMorphRuleApplication` for every stratum unconditionally.
#[test]
fn a_minimal_grammar_derives_only_the_mechanisms_its_observations_justify() {
    let g = load(MINIMAL_XML);
    let semantics = GrammarSemantics::derive(&g);
    let observed: Vec<CharacteristicKind> = semantics
        .characteristics()
        .observations()
        .iter()
        .map(|o| o.kind)
        .collect();
    assert_eq!(
        observed,
        vec![CharacteristicKind::OrderedMorphRuleApplication],
        "fixture no longer observes exactly one construct"
    );

    let graph = derive_mechanism_graph(&semantics);
    graph.validate().expect("derived graph validates");
    assert_eq!(
        kinds(&graph),
        vec![MechanismKind::Morphotactics, MechanismKind::BoundaryCleanup],
        "one observed construct plus the terminal cleanup, and nothing else"
    );
    for absent in [
        MechanismKind::StaticPartition,
        MechanismKind::StructuralAllomorph,
        MechanismKind::CopyProcess,
        MechanismKind::OrderedPhonology,
    ] {
        assert!(
            !kinds(&graph).contains(&absent),
            "{absent:?} was created with no observation to justify it"
        );
    }
    assert_eq!(graph.edges.len(), 1);
}

/// Nodes appear in the single canonical composition order, edges chain the present ones, and the terminal mechanism is always cleanup -- no permutation of that order is representable.
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
    assert_eq!(
        observed, expected_order,
        "nodes are not in composition order"
    );
    assert_eq!(observed.last(), Some(&MechanismKind::BoundaryCleanup));
    assert_eq!(
        graph.edges.len(),
        graph.nodes.len() - 1,
        "not a single chain"
    );

    // Cleanup's source is the character table it cleans -- the one source kind with no `ModelLocation` counterpart -- and its body carries the table's boundary inventory.
    let cleanup = graph
        .node(&MechanismId(
            MechanismKind::BoundaryCleanup.label().to_owned(),
        ))
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

/// The `StaticPartition` body is deterministically ordered: groups ascending by gate key, members ascending within each group, group count agreeing with `GrammarSemantics::partition_count` -- what the byte-identity assertion above actually rides on.
#[test]
fn the_static_partition_body_is_sorted_by_gate_key_and_by_member() {
    let g = load(GATED_PARTITION_XML);
    let semantics = GrammarSemantics::derive(&g);
    assert!(semantics.has_gated_exceptions());
    let graph = derive_mechanism_graph(&semantics);
    graph.validate().expect("derived graph validates");

    let partition = graph
        .node(&MechanismId(
            MechanismKind::StaticPartition.label().to_owned(),
        ))
        .expect("a gated grammar derives a StaticPartition mechanism");
    let MechanismBody::StaticPartition(groups) = &partition.body else {
        panic!("wrong body: {:?}", partition.body);
    };

    assert_eq!(groups.len() as u64, semantics.partition_count());
    assert!(
        groups.len() > 1,
        "fixture must produce more than one group, else ordering is untested"
    );
    assert!(
        groups.iter().any(|group| group.members.len() > 1),
        "fixture must produce a multi-member group, else member ordering is untested"
    );

    let keys: Vec<&Vec<bool>> = groups.iter().map(|group| &group.key).collect();
    let mut sorted_keys = keys.clone();
    sorted_keys.sort();
    assert_eq!(keys, sorted_keys, "groups are not sorted by gate key");

    for group in groups {
        let mut sorted = group.members.clone();
        sorted.sort();
        assert_eq!(group.members, sorted, "members are not sorted");
    }
    assert!(partition
        .construct_requirements
        .contains(&CharacteristicKind::SubruleGating));
}

/// Every node's typed sources trace back to a real characteristic observation (or, for cleanup, to the character table), and every requirement is a construct that was actually observed.
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
