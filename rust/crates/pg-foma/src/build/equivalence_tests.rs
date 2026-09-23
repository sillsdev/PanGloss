//! The correctness argument for this module's equivalence claim, made semantically meaningful
//! rather than trivial. For an in-crate gated synthetic fixture, builds BOTH (a)
//! `compile_gated_grammar` (today's direct-compile path) and (b)
//! `build_controllable(enumerate_default(...))` (this module's plan-walk), then asserts the two
//! resulting networks are EQUIVALENT BY APPLY -- `apply_up` on every distinguishing query word
//! must yield IDENTICAL result sets. This is exactly the predicate a future differential oracle
//! would use; the module doc explains why it -- not a structural/byte-identity
//! claim -- is the one that matters. Minimized state/arc counts are ALSO asserted equal, as a
//! cheap and (here) meaningful extra signal, never a substitute for the apply comparison.

use std::collections::HashSet;

use foma::apply::apply_init;
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_grammar::model::{Grammar, PhonRuleDef};

use super::*;
use crate::enumerate::enumerate_default;
use crate::gate::compile_gated_grammar;
use crate::junctions::PhonologyProbe;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

fn prules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|&id| &g.prules[id.0 as usize])
        .collect()
}

/// One MPR-gated subrule and two entries realizing both gate-key values: `e0` keeps "p" as "p" (gate false), `e1` surfaces it as "q" (gate true) -- so "p"/"q" each analyze under exactly one group.
fn gated_two_group_fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>BuildControllableGatedTwoGroupFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mpr1">f1</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>gate1</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule requiredMPRFeatures="mpr1">
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e0" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e0</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e1" partOfSpeech="posV" ruleFeatures="mpr1">
            <Allomorphs><Allomorph id="allo1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
}

/// Every raw string `apply_up` yields for `word` against `net` -- the full literal upper-tape output set, not a decoded/collapsed projection.
fn apply_up_results(net: &Fsm, alphabet: &SegAlphabet, word: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(query) = alphabet.encode_query(word) else {
        return out;
    };
    let mut h = apply_init(net);
    for s in h.up(&query) {
        out.insert(s);
    }
    out
}

#[test]
fn plan_walk_matches_direct_compile_by_apply_on_gated_two_group_fixture() {
    let g = load(gated_two_group_fixture_xml());
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    // (a) today's direct-compile path, unmodified.
    let direct =
        compile_gated_grammar(&opts, &g, &alphabet, &ro).expect("direct compile must succeed");
    let direct_net = direct
        .net
        .clone()
        .expect("direct compile must produce a non-empty net");

    // (b) the plan-walk this module ships.
    let plan = enumerate_default(&g, &ro, phon.as_ref());
    let built =
        build_controllable(&plan, &opts, &g, &alphabet, &ro).expect("plan-walk build must succeed");
    let built_net = built
        .net
        .clone()
        .expect("plan-walk build must produce a non-empty net");

    assert_eq!(
        direct.groups, built.groups,
        "direct-compile and plan-walk must agree on group count"
    );
    assert_eq!(
        direct.groups, 2,
        "fixture sanity: exactly 2 gating groups expected"
    );

    // Structural sanity, not a substitute for the apply comparison below: both paths run the same final minimize.
    assert_eq!(
        direct_net.statecount, built_net.statecount,
        "minimized state counts must match between direct compile and plan-walk build"
    );
    assert_eq!(
        direct_net.arccount, built_net.arccount,
        "minimized arc counts must match between direct compile and plan-walk build"
    );

    // apply_up on every distinguishing query word must be identical between the two nets.
    for word in ["p", "q"] {
        let want = apply_up_results(&direct_net, &alphabet, word);
        let got = apply_up_results(&built_net, &alphabet, word);
        assert!(
            !want.is_empty(),
            "sanity: {word:?} must actually analyze on the direct-compile net"
        );
        assert_eq!(
            got, want,
            "apply_up results for {word:?} must match EXACTLY between direct compile and \
             plan-walk build (want from direct compile, got from build_controllable)"
        );
    }

    // "p" and "q" must resolve to different results, proving the gate actually distinguishes the two groups.
    assert_ne!(
        apply_up_results(&direct_net, &alphabet, "p"),
        apply_up_results(&direct_net, &alphabet, "q"),
        "fixture sanity: \"p\" and \"q\" must resolve to different analyses on the direct-compile \
         net (otherwise the gate isn't actually being exercised)"
    );
}

/// Two differently-gated groups must get distinct Replace `NodeId`s, and that distinctness must not change the compiled relation (still apply-equivalent to direct compile).
#[test]
fn purity_differently_gated_groups_have_distinct_replace_node_ids_and_build_stays_apply_equivalent()
{
    let g = load(gated_two_group_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let opts = FomaOptions::default();
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let plan = enumerate_default(&g, &ro, phon.as_ref());

    // (a) node purity: the two gate groups must reference DISTINCT Replace NodeIds now.
    let gate_id = find_gate_node(&plan);
    let PlanNodeKind::Gate { children, .. } = plan.get(gate_id).unwrap() else {
        unreachable!("find_gate_node only ever returns the id of a Gate node")
    };
    assert_eq!(children.len(), 2, "fixture declares exactly 2 gate groups");
    let replace_ids: Vec<NodeId> = children
        .iter()
        .map(|&compose_id| gate_group_children(&plan, compose_id).1)
        .collect();
    assert_ne!(
        replace_ids[0], replace_ids[1],
        "task 1.4: two differently-gated groups must get DISTINCT Replace NodeIds -- a node's \
         compiled artifact is now a pure function of its own NodeId (design.md D1), so no two \
         groups needing different subrule_ok may share one Replace node"
    );

    // (b) that distinctness must not change the compiled relation.
    let direct =
        compile_gated_grammar(&opts, &g, &alphabet, &ro).expect("direct compile must succeed");
    let direct_net = direct
        .net
        .clone()
        .expect("direct compile must produce a non-empty net");
    let built =
        build_controllable(&plan, &opts, &g, &alphabet, &ro).expect("plan-walk build must succeed");
    let built_net = built
        .net
        .clone()
        .expect("plan-walk build must produce a non-empty net");

    for word in ["p", "q"] {
        let want = apply_up_results(&direct_net, &alphabet, word);
        let got = apply_up_results(&built_net, &alphabet, word);
        assert!(
            !want.is_empty(),
            "sanity: {word:?} must actually analyze on the direct-compile net"
        );
        assert_eq!(
            got, want,
            "apply_up results for {word:?} must match EXACTLY between direct compile and \
             plan-walk build despite the two groups now having distinct Replace NodeIds"
        );
    }
}
