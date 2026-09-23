//! Three outcomes the task requires, in this order: (1) two genuinely distinct SAME-relation
//! plans (`enumerate_default` vs. `permute_gate_groups` of it) -> `Agree`; (2) a deliberately
//! WRONG second plan (one gate group dropped, module doc's `drop_last_gate_group`) -> a real
//! `Disagree` naming a concrete word and a non-empty symmetric difference, proving the oracle is
//! not vacuous; (3) the shortest-witness tie-break, tested directly against
//! `resolve_verdict` with synthetic multi-length word data (this repo's tiny synthetic
//! fixtures only ever recognize single-segment surface forms, so exercising the length tie-break
//! through a real grammar+build would need a needlessly elaborate fixture -- testing the pure
//! selection function directly is the more direct proof of this specific claim).

use std::collections::HashSet;

use pg_grammar::model::{Grammar, PhonRuleDef};

use super::*;
use crate::enumerate::enumerate_default;
use crate::junctions::PhonologyProbe;
use crate::plan::ReplaceCascadeSpec;

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

/// One MPR-gated subrule and two entries realizing both truth values of that gate key: `e0` surfaces "p" unchanged, `e1` surfaces "p" as "q", so each word is producible by exactly one gate group.
fn oracle_gated_two_group_fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>OracleGatedTwoGroupFixture</Name>
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

/// Test-only deliberately-wrong second topology: drops the last gate group (the only entry that ever produces surface "q"), a real under-generating topology `differential_oracle` must catch.
fn drop_last_gate_group(plan: &Plan) -> Plan {
    copy_plan_transforming(
        plan,
        &mut |_id, partition, children| {
            assert_eq!(partition.groups.len(), children.len());
            assert!(
                partition.groups.len() >= 2,
                "drop_last_gate_group needs >=2 groups to drop one and still have a non-empty Gate"
            );
            let keep = partition.groups.len() - 1;
            let groups = partition.groups[..keep].to_vec();
            let kept_children = children[..keep].to_vec();
            (
                GatePartitionSpec {
                    gated_subrules: partition.gated_subrules.clone(),
                    groups,
                },
                kept_children,
            )
        },
        &mut |_id, children| children.to_vec(),
    )
}

/// A test-only `MutationStep` wrapping `drop_last_gate_group`, so `minimize_disagreement` can be proven to find exactly the real bug among genuinely sound mutations.
fn breakage_step() -> MutationStep {
    MutationStep {
        description: "BREAKAGE(test-only, deliberately unsound): drop_last_gate_group".to_string(),
        transform: Rc::new(drop_last_gate_group),
    }
}

/// A grammar with one ordinary, ungated rule whose `should_run=true` root is a `Union` wrapping a single-group `Gate` plus a marker leaf, the shape `permute_union_children` needs to exercise.
fn oracle_union_root_fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>OracleUnionRootFixture</Name>
<PartsOfSpeech>
  <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
</PartsOfSpeech>
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="c2"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses>
  <SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="c1" /></SegmentNaturalClass>
</NaturalClasses>
<PhonologicalRuleDefinitions>
  <PhonologicalRule id="pr1">
    <Name>PR</Name>
    <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
    <PhonologicalSubrules>
      <PhonologicalSubrule>
        <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
      </PhonologicalSubrule>
    </PhonologicalSubrules>
  </PhonologicalRule>
</PhonologicalRuleDefinitions>
<Strata>
  <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="pr1">
    <Name>S</Name>
    <LexicalEntries>
      <LexicalEntry id="e1" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
        <Gloss>e1</Gloss>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#
}

/// An ungated grammar whose single group holds three entries, the smallest fixture that can distinguish `Bisect` (2 sub-groups) from `FanOut` (3 singletons) from each other, not just from baseline.
fn oracle_three_entry_ungated_fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>OracleThreeEntryUngatedFixture</Name>
<PartsOfSpeech>
  <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
</PartsOfSpeech>
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="c2"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="c3"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<Strata>
  <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
    <Name>S</Name>
    <LexicalEntries>
      <LexicalEntry id="e0" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
        <Gloss>e0</Gloss>
      </LexicalEntry>
      <LexicalEntry id="e1" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>t</PhoneticShape></Allomorph></Allomorphs>
        <Gloss>e1</Gloss>
      </LexicalEntry>
      <LexicalEntry id="e2" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a2"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
        <Gloss>e2</Gloss>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#
}

/// Two ungated rules in one cascade, used to prove reversing `cascade.rules` makes `build_controllable` panic (it cross-validates positionally against `prules_in_order`).
fn oracle_two_rule_cascade_fixture_xml() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>OracleTwoRuleCascadeFixture</Name>
<PartsOfSpeech>
  <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
</PartsOfSpeech>
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="c2"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="c3"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<PhonologicalRuleDefinitions>
  <PhonologicalRule id="pr1">
    <Name>PR1</Name>
    <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
    <PhonologicalSubrules>
      <PhonologicalSubrule>
        <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
      </PhonologicalSubrule>
    </PhonologicalSubrules>
  </PhonologicalRule>
  <PhonologicalRule id="pr2">
    <Name>PR2</Name>
    <PhoneticInput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticInput>
    <PhonologicalSubrules>
      <PhonologicalSubrule>
        <PhoneticOutput><PhoneticSequence><Segment segment="c3" /></PhoneticSequence></PhoneticOutput>
      </PhonologicalSubrule>
    </PhonologicalSubrules>
  </PhonologicalRule>
</PhonologicalRuleDefinitions>
<Strata>
  <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="pr1 pr2">
    <Name>S</Name>
    <LexicalEntries>
      <LexicalEntry id="e1" partOfSpeech="posV">
        <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
        <Gloss>e1</Gloss>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>
"#
}

/// Test-only deliberately-wrong restructuring: swaps a gate group's `Compose` node's two children, since `copy_plan_transforming` has no `Compose` hook by design (no sound generator exists).
fn swap_compose_children(plan: &Plan) -> Plan {
    fn copy(old_plan: &Plan, old_id: NodeId, new_plan: &mut Plan) -> NodeId {
        match old_plan
            .get(old_id)
            .unwrap_or_else(|| panic!("dangling NodeId {old_id} while copying a Plan"))
        {
            PlanNodeKind::Leaf {
                fragment,
                provenance,
            } => new_plan.add_node(PlanNodeKind::Leaf {
                fragment: fragment.clone(),
                provenance: provenance.clone(),
            }),
            PlanNodeKind::Compose { children, strategy } => {
                let strategy = *strategy;
                assert_eq!(
                    children.len(),
                    2,
                    "swap_compose_children fixture must have a 2-child Compose node"
                );
                let mut swapped = children.clone();
                swapped.swap(0, 1);
                let new_children: Vec<NodeId> = swapped
                    .iter()
                    .map(|&c| copy(old_plan, c, new_plan))
                    .collect();
                new_plan.add_node(PlanNodeKind::Compose {
                    children: new_children,
                    strategy,
                })
            }
            PlanNodeKind::Union { children } => {
                let new_children: Vec<NodeId> = children
                    .iter()
                    .map(|&c| copy(old_plan, c, new_plan))
                    .collect();
                new_plan.add_node(PlanNodeKind::Union {
                    children: new_children,
                })
            }
            PlanNodeKind::Replace { cascade, children } => {
                let cascade = cascade.clone();
                let new_children: Vec<NodeId> = children
                    .iter()
                    .map(|&c| copy(old_plan, c, new_plan))
                    .collect();
                new_plan.add_node(PlanNodeKind::Replace {
                    cascade,
                    children: new_children,
                })
            }
            PlanNodeKind::Gate {
                partition,
                children,
            } => {
                let partition = partition.clone();
                let new_children: Vec<NodeId> = children
                    .iter()
                    .map(|&c| copy(old_plan, c, new_plan))
                    .collect();
                new_plan.add_node(PlanNodeKind::Gate {
                    partition,
                    children: new_children,
                })
            }
        }
    }
    let root = plan.root().expect("plan must have a root");
    let mut new_plan = Plan::new();
    let new_root = copy(plan, root, &mut new_plan);
    new_plan.set_root(new_root);
    new_plan
}

/// Test-only deliberately-wrong restructuring: reverses a `Replace` node's `cascade.rules` and its rule-leaf `children` together, keeping the node's own internal consistency intact so only the cascade order itself is under test.
fn reverse_replace_cascade(plan: &Plan) -> Plan {
    fn copy(old_plan: &Plan, old_id: NodeId, new_plan: &mut Plan) -> NodeId {
        match old_plan
            .get(old_id)
            .unwrap_or_else(|| panic!("dangling NodeId {old_id} while copying a Plan"))
        {
            PlanNodeKind::Leaf {
                fragment,
                provenance,
            } => new_plan.add_node(PlanNodeKind::Leaf {
                fragment: fragment.clone(),
                provenance: provenance.clone(),
            }),
            PlanNodeKind::Compose { children, strategy } => {
                let strategy = *strategy;
                let new_children: Vec<NodeId> = children
                    .iter()
                    .map(|&c| copy(old_plan, c, new_plan))
                    .collect();
                new_plan.add_node(PlanNodeKind::Compose {
                    children: new_children,
                    strategy,
                })
            }
            PlanNodeKind::Union { children } => {
                let new_children: Vec<NodeId> = children
                    .iter()
                    .map(|&c| copy(old_plan, c, new_plan))
                    .collect();
                new_plan.add_node(PlanNodeKind::Union {
                    children: new_children,
                })
            }
            PlanNodeKind::Replace { cascade, children } => {
                assert_eq!(
                    cascade.rules.len(),
                    children.len(),
                    "reverse_replace_cascade requires the Replace invariant to hold going in"
                );
                assert!(
                    cascade.rules.len() >= 2,
                    "reverse_replace_cascade needs >=2 rules for a reversal to be a genuine \
                     reordering, not a no-op"
                );
                let mut rules = cascade.rules.clone();
                rules.reverse();
                let mut old_children = children.clone();
                old_children.reverse();
                let new_cascade = ReplaceCascadeSpec {
                    rules,
                    gated_subrules: cascade.gated_subrules.clone(),
                    group_key: cascade.group_key.clone(),
                };
                let new_children: Vec<NodeId> = old_children
                    .iter()
                    .map(|&c| copy(old_plan, c, new_plan))
                    .collect();
                new_plan.add_node(PlanNodeKind::Replace {
                    cascade: new_cascade,
                    children: new_children,
                })
            }
            PlanNodeKind::Gate {
                partition,
                children,
            } => {
                let partition = partition.clone();
                let new_children: Vec<NodeId> = children
                    .iter()
                    .map(|&c| copy(old_plan, c, new_plan))
                    .collect();
                new_plan.add_node(PlanNodeKind::Gate {
                    partition,
                    children: new_children,
                })
            }
        }
    }
    let root = plan.root().expect("plan must have a root");
    let mut new_plan = Plan::new();
    let new_root = copy(plan, root, &mut new_plan);
    new_plan.set_root(new_root);
    new_plan
}

fn hs(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// Outcome 1: two genuinely distinct, SAME-relation plans -> Agree.

#[test]
fn permuted_gate_groups_is_a_genuinely_different_plan() {
    let g = load(oracle_gated_two_group_fixture_xml());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let plan_a = enumerate_default(&g, &ro, phon.as_ref());
    let plan_b = permute_gate_groups(&plan_a);

    assert_ne!(
        plan_a.root(),
        plan_b.root(),
        "permute_gate_groups must produce a plan with a different root NodeId (module doc: \
         group order is part of the Gate node's content address) -- otherwise this would not \
         be a real second topology for the oracle to diff"
    );
}

#[test]
fn differential_oracle_agrees_on_permuted_gate_groups_of_the_same_grammar() {
    let g = load(oracle_gated_two_group_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();

    let plan_a = enumerate_default(&g, &ro, phon.as_ref());
    let plan_b = permute_gate_groups(&plan_a);

    let result = differential_oracle(
        &plan_a,
        &plan_b,
        ("enumerate_default", "permute_gate_groups"),
        &opts,
        &g,
        &alphabet,
        &ro,
        &["p", "q"],
    )
    .expect("both plans must build successfully on this fixture");

    match result {
        OracleResult::Agree => {}
        OracleResult::Disagree {
            word,
            only_in_a,
            only_in_b,
            ..
        } => panic!(
            "two same-relation topologies (a grammar's default enumeration and its gate-group- \
             permuted twin) must Agree, not Disagree -- got a real divergence at {word:?}: \
             only_in_a={only_in_a:?}, only_in_b={only_in_b:?}. Per this task's own instruction, \
             this must be reported as a genuine finding, never papered over."
        ),
    }
}

// Outcome 2: a deliberately WRONG second plan -> the oracle actually catches it.

#[test]
fn differential_oracle_catches_a_dropped_gate_group_as_a_real_disagreement() {
    let g = load(oracle_gated_two_group_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();

    let plan_correct = enumerate_default(&g, &ro, phon.as_ref());
    let plan_wrong = drop_last_gate_group(&plan_correct);
    assert_ne!(
        plan_correct.root(),
        plan_wrong.root(),
        "the truncated plan must be a genuinely different Plan"
    );

    let result = differential_oracle(
        &plan_correct,
        &plan_wrong,
        (
            "enumerate_default",
            "drop_last_gate_group (deliberately wrong)",
        ),
        &opts,
        &g,
        &alphabet,
        &ro,
        &["p", "q"],
    )
    .expect(
        "both plans must build successfully on this fixture (the truncated plan still has \
              1 non-empty group left)",
    );

    match result {
        OracleResult::Agree => panic!(
            "dropping an entire gate group (entry e1, the only entry that ever produces \
             surface \"q\") changes the relation -- the oracle returning Agree here would mean \
             it is VACUOUS, which is exactly what this test exists to rule out"
        ),
        OracleResult::Disagree {
            word,
            only_in_a,
            only_in_b,
            plan_a_label,
            plan_b_label,
        } => {
            assert_eq!(
                word, "q",
                "the dropped group is exactly what makes surface \"q\" analyzable -- \"q\" must \
                 be the (only) disagreeing word here"
            );
            assert!(
                !only_in_a.is_empty() || !only_in_b.is_empty(),
                "a real disagreement must carry a non-empty symmetric difference, got \
                 only_in_a={only_in_a:?} only_in_b={only_in_b:?}"
            );
            assert!(
                only_in_b.is_empty(),
                "the truncated (wrong) plan must UNDER-generate on \"q\" -- nothing should be \
                 unique to it; got only_in_b={only_in_b:?}"
            );
            assert_eq!(plan_a_label, "enumerate_default");
            assert_eq!(plan_b_label, "drop_last_gate_group (deliberately wrong)");
        }
    }
}

// Outcome 3: shortest-witness tie-break, tested directly against the pure selection core.

#[test]
fn resolve_verdict_reports_the_shortest_disagreeing_word_regardless_of_input_order() {
    let per_word = vec![
        // A LONGER disagreement, listed FIRST, proves selection is by length, not order.
        ("longerword".to_string(), hs(&["X"]), hs(&["Y"])),
        ("hi".to_string(), hs(&["A"]), hs(&["B"])),
        // An agreeing word, to prove agreements are simply excluded, not treated as a tie.
        ("z".to_string(), hs(&["same"]), hs(&["same"])),
    ];

    match resolve_verdict(per_word, "planA", "planB") {
        OracleResult::Disagree { word, .. } => {
            assert_eq!(word, "hi", "the SHORTEST disagreeing word must be reported")
        }
        OracleResult::Agree => panic!("expected a Disagree (two words genuinely differ)"),
    }
}

#[test]
fn resolve_verdict_breaks_same_length_ties_lexicographically() {
    let per_word = vec![
        ("zz".to_string(), hs(&["X"]), hs(&["Y"])),
        ("aa".to_string(), hs(&["A"]), hs(&["B"])),
    ];

    match resolve_verdict(per_word, "planA", "planB") {
        OracleResult::Disagree { word, .. } => {
            assert_eq!(word, "aa", "same-length ties must break lexicographically")
        }
        OracleResult::Agree => panic!("expected a Disagree (two words genuinely differ)"),
    }
}

#[test]
fn resolve_verdict_agrees_when_every_word_matches() {
    let per_word = vec![
        ("p".to_string(), hs(&["e0"]), hs(&["e0"])),
        ("q".to_string(), hs(&["e1"]), hs(&["e1"])),
    ];
    assert_eq!(
        resolve_verdict(per_word, "planA", "planB"),
        OracleResult::Agree
    );
}

// The `Union` second-topology generator, on a real grammar.

#[test]
fn enumerate_default_on_union_fixture_has_a_union_root_with_two_children() {
    let g = load(oracle_union_root_fixture_xml());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let plan = enumerate_default(&g, &ro, phon.as_ref());

    let root = plan.root().expect("root must be set");
    match plan.get(root).unwrap() {
        PlanNodeKind::Union { children } => {
            assert_eq!(
                children.len(),
                2,
                "fixture sanity: Gate + composite-emission marker"
            )
        }
        other => panic!(
            "fixture sanity: expected a Union root, got {}",
            other.kind_name()
        ),
    }
}

#[test]
fn permute_union_children_is_a_genuinely_different_plan() {
    let g = load(oracle_union_root_fixture_xml());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let plan_a = enumerate_default(&g, &ro, phon.as_ref());
    let plan_b = permute_union_children(&plan_a);

    assert_ne!(
        plan_a.root(),
        plan_b.root(),
        "permute_union_children must produce a plan with a different root NodeId -- otherwise \
         this would not be a real second topology for the oracle to diff"
    );
}

#[test]
fn differential_oracle_agrees_on_permuted_union_children_of_the_same_grammar() {
    let g = load(oracle_union_root_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();

    let plan_a = enumerate_default(&g, &ro, phon.as_ref());
    let plan_b = permute_union_children(&plan_a);

    let result = differential_oracle(
        &plan_a,
        &plan_b,
        ("enumerate_default", "permute_union_children"),
        &opts,
        &g,
        &alphabet,
        &ro,
        &["p"],
    )
    .expect("both plans must build successfully on this fixture");

    match result {
        OracleResult::Agree => {}
        OracleResult::Disagree {
            word,
            only_in_a,
            only_in_b,
            ..
        } => panic!(
            "two same-relation topologies (a grammar's default enumeration and its root-Union- \
             permuted twin) must Agree, not Disagree -- got a real divergence at {word:?}: \
             only_in_a={only_in_a:?}, only_in_b={only_in_b:?}."
        ),
    }
}

// `Compose`/`Replace` have NO sound generator, confirmed empirically: reordering either is mechanically rejected by build_controllable.

#[test]
#[should_panic(expected = "expected a Leaf node as a gate-group Compose node's first child")]
fn swapping_compose_children_is_mechanically_rejected_by_build_controllable() {
    let g = load(oracle_gated_two_group_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();

    let plan = enumerate_default(&g, &ro, phon.as_ref());
    let swapped = swap_compose_children(&plan);

    // build_controllable enforces the Compose children positionally, so swapping them must panic before any apply_up comparison.
    let _ = build_controllable(&swapped, &opts, &g, &alphabet, &ro);
}

#[test]
#[should_panic(expected = "does not match the plan's Replace cascade at that position")]
fn reversing_replace_cascade_is_mechanically_rejected_by_build_controllable() {
    let g = load(oracle_two_rule_cascade_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();

    let plan = enumerate_default(&g, &ro, phon.as_ref());
    let reversed = reverse_replace_cascade(&plan);

    // validate_replace_cascade cross-checks cascade.rules against prules_in_order positionally, so a reversed cascade must panic.
    let _ = build_controllable(&reversed, &opts, &g, &alphabet, &ro);
}

// Partition refinement, proven by the same two bars as the module's other generators: apply-based agreement with baseline, and a root NodeId that actually differs.

fn gate_group_count(plan: &Plan) -> usize {
    plan.iter()
        .find_map(|(_, kind)| match kind {
            PlanNodeKind::Gate { partition, .. } => Some(partition.groups.len()),
            _ => None,
        })
        .expect("plan must contain exactly one Gate node")
}

#[test]
fn refine_gate_partition_bisect_and_fan_out_are_each_genuinely_different_plans() {
    let g = load(oracle_three_entry_ungated_fixture_xml());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let baseline = enumerate_default(&g, &ro, phon.as_ref());
    let bisected = refine_gate_partition(&baseline, PartitionGranularity::Bisect);
    let fanned_out = refine_gate_partition(&baseline, PartitionGranularity::FanOut);

    assert_eq!(
        gate_group_count(&baseline),
        1,
        "fixture sanity: ungated grammar collapses to 1 group before refinement"
    );
    assert_eq!(
        gate_group_count(&bisected),
        2,
        "bisecting a 3-entry group must yield 2 sub-groups (sizes 2 and 1)"
    );
    assert_eq!(
        gate_group_count(&fanned_out),
        3,
        "fanning out a 3-entry group must yield 3 singleton sub-groups"
    );

    assert_ne!(
        baseline.root(),
        bisected.root(),
        "bisection must change the plan's root NodeId -- otherwise materialize_distinct would \
         dedup it straight back to baseline and nothing was actually added"
    );
    assert_ne!(
        baseline.root(),
        fanned_out.root(),
        "fan-out must change the plan's root NodeId -- same content-distinctness bar as above"
    );
    assert_ne!(
        bisected.root(),
        fanned_out.root(),
        "bisect and fan-out must be TWO DIFFERENT topologies (different group counts, 2 vs 3), \
         not the same restructuring wearing two names -- exactly the bar this task's own \
         instruction sets for 'genuinely distinct'"
    );
}

#[test]
fn refine_gate_partition_agrees_with_baseline_by_apply_for_both_granularities() {
    let g = load(oracle_three_entry_ungated_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();

    let baseline = enumerate_default(&g, &ro, phon.as_ref());
    for (label, granularity) in [
        ("partition-bisect", PartitionGranularity::Bisect),
        ("partition-fan-out", PartitionGranularity::FanOut),
    ] {
        let refined = refine_gate_partition(&baseline, granularity);
        let result = differential_oracle(
            &baseline,
            &refined,
            ("enumerate_default", label),
            &opts,
            &g,
            &alphabet,
            &ro,
            &["p", "t", "k"],
        )
        .unwrap_or_else(|e| panic!("both plans must build for {label}: {e:?}"));
        match result {
            OracleResult::Agree => {}
            OracleResult::Disagree {
                word,
                only_in_a,
                only_in_b,
                ..
            } => panic!(
                "partition refinement ({label}) must not change the accepted relation -- got a \
                 real divergence at {word:?}: only_in_a={only_in_a:?}, only_in_b={only_in_b:?}. \
                 Per this task's own instruction, this must be reported as a genuine finding, \
                 never papered over."
            ),
        }
    }
}

#[test]
fn refine_gate_partition_is_a_no_op_when_no_group_has_two_or_more_entries() {
    let g = load(oracle_union_root_fixture_xml());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let baseline = enumerate_default(&g, &ro, phon.as_ref());
    let bisected = refine_gate_partition(&baseline, PartitionGranularity::Bisect);
    let fanned_out = refine_gate_partition(&baseline, PartitionGranularity::FanOut);

    assert_eq!(
        baseline.root(),
        bisected.root(),
        "a single-entry group has nothing to bisect -- this must be a true no-op (content- \
         identical, same NodeId), never forced churn on a fixture with nothing eligible"
    );
    assert_eq!(
        baseline.root(),
        fanned_out.root(),
        "a single-entry group is already maximally fanned out -- same true-no-op bar as above"
    );
}

// Seeded random subtree mutation.

#[test]
fn mutate_plan_seeded_is_deterministic_for_the_same_seed() {
    let g = load(oracle_gated_two_group_fixture_xml());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let plan = enumerate_default(&g, &ro, phon.as_ref());

    let outcome_1 = mutate_plan_seeded(&plan, 42);
    let outcome_2 = mutate_plan_seeded(&plan, 42);

    assert_eq!(
        outcome_1.recipe, outcome_2.recipe,
        "the SAME seed against the SAME plan must draw the SAME recipe"
    );
    assert_eq!(
        outcome_1.plan.root(),
        outcome_2.plan.root(),
        "the SAME seed against the SAME plan must produce the SAME mutated plan"
    );
}

#[test]
fn different_seeds_produce_different_topologies() {
    let g = load(oracle_gated_two_group_fixture_xml());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let plan = enumerate_default(&g, &ro, phon.as_ref());

    let roots: std::collections::BTreeSet<Option<NodeId>> = (0u64..30)
        .map(|seed| mutate_plan_seeded(&plan, seed).plan.root())
        .collect();

    assert!(
        roots.len() >= 2,
        "a genuinely random generator must produce more than one distinct topology across a \
         seed sweep -- a generator that always echoes its input (or always applies the \
         identical permutation) would fail this, which is exactly the non-vacuity failure mode \
         this test rules out; got {} distinct root(s)",
        roots.len()
    );
}

#[test]
fn mutate_plan_seeded_exercises_the_gate_generator_and_agrees() {
    let g = load(oracle_gated_two_group_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();
    let plan = enumerate_default(&g, &ro, phon.as_ref());

    // This fixture has 2 eligible targets (its Gate and the root Union); scan seeds for one that draws the Gate.
    let mut exercised = false;
    for seed in 0u64..20 {
        let outcome = mutate_plan_seeded(&plan, seed);
        let recipe = outcome
            .recipe
            .clone()
            .expect("this fixture has eligible targets");
        if recipe.target_kind == "Gate" {
            exercised = true;
            let result = differential_oracle(
                &plan,
                &outcome.plan,
                ("enumerate_default", "mutate_plan_seeded(gate permute)"),
                &opts,
                &g,
                &alphabet,
                &ro,
                &["p", "q"],
            )
            .expect("both plans must build successfully on this fixture");
            assert_eq!(
                result,
                OracleResult::Agree,
                "a mutation drawn from the sound Gate-group generator must Agree with the \
                 original plan, got {result:?}"
            );
            break;
        }
    }
    assert!(
        exercised,
        "expected at least one seed in 0..20 to draw the Gate target -- if none did, \
         mutate_plan_seeded's Gate path was never actually exercised by this test"
    );
}

#[test]
fn mutate_plan_seeded_exercises_the_union_generator_and_agrees() {
    let g = load(oracle_union_root_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();
    let plan = enumerate_default(&g, &ro, phon.as_ref());

    // This fixture's only eligible target is the root Union; scan seeds for one that draws the non-identity permutation.
    let mut exercised = false;
    for seed in 0u64..20 {
        let outcome = mutate_plan_seeded(&plan, seed);
        let recipe = outcome
            .recipe
            .clone()
            .expect("this fixture has one eligible Union target");
        assert_eq!(recipe.target_kind, "Union");
        if outcome.plan.root() != plan.root() {
            exercised = true;
            let result = differential_oracle(
                &plan,
                &outcome.plan,
                ("enumerate_default", "mutate_plan_seeded(union swap)"),
                &opts,
                &g,
                &alphabet,
                &ro,
                &["p"],
            )
            .expect("both plans must build successfully on this fixture");
            assert_eq!(
                result,
                OracleResult::Agree,
                "a mutation drawn from the sound Union generator must Agree with the original \
                 plan, got {result:?}"
            );
            break;
        }
    }
    assert!(
        exercised,
        "expected at least one seed in 0..20 to draw the non-identity Union permutation -- if \
         none did, mutate_plan_seeded's Union path was never actually exercised by this test"
    );
}

// Failure minimisation to a named recipe.

#[test]
#[should_panic(expected = "does not actually disagree")]
fn minimize_disagreement_panics_when_the_full_sequence_agrees() {
    let g = load(oracle_gated_two_group_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();
    let plan = enumerate_default(&g, &ro, phon.as_ref());

    // Every step here is sound, so the full chain must Agree; minimize_disagreement must refuse to "minimise" a non-disagreement.
    let steps = vec![MutationStep::from_seed(1), MutationStep::from_seed(2)];
    let _ = minimize_disagreement(
        &plan,
        steps,
        ("base", "mutated"),
        &opts,
        &g,
        &alphabet,
        &ro,
        &["p", "q"],
    );
}

#[test]
fn minimization_converges_to_the_injected_breakage() {
    let g = load(oracle_gated_two_group_fixture_xml());
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();
    let plan = enumerate_default(&g, &ro, phon.as_ref());

    // Four sound, harmless steps with one real bug spliced into the middle; minimisation must discard every sound step and converge on that one.
    let steps = vec![
        MutationStep::from_seed(1),
        MutationStep::from_seed(2),
        breakage_step(),
        MutationStep::from_seed(3),
    ];

    let recipe = minimize_disagreement(
        &plan,
        steps,
        ("enumerate_default", "mutated"),
        &opts,
        &g,
        &alphabet,
        &ro,
        &["p", "q"],
    )
    .expect("both plans must build successfully on this fixture");

    assert_eq!(
        recipe.steps.len(),
        1,
        "minimisation must converge to a SINGLE step, the injected breakage -- got {:?}",
        recipe.steps
    );
    assert!(
        recipe.steps[0].contains("BREAKAGE"),
        "the surviving step must be the injected breakage, not one of the sound mutations -- \
         got {:?}",
        recipe.steps
    );
    assert_eq!(
        recipe.word, "q",
        "the dropped gate group is exactly what makes surface \"q\" analyzable, same as the \
         oracle's own non-vacuity test"
    );

    // Replaying only the surviving step from the base plan must still disagree, with the same witness word.
    let replayed = apply_step_chain(&plan, &[breakage_step()]);
    let replay_result = differential_oracle(
        &plan,
        &replayed,
        ("enumerate_default", "replayed breakage"),
        &opts,
        &g,
        &alphabet,
        &ro,
        &["p", "q"],
    )
    .expect("both plans must build successfully on this fixture");
    match replay_result {
        OracleResult::Disagree { word, .. } => assert_eq!(
            word, "q",
            "the replayed minimal recipe must reproduce the SAME witness word"
        ),
        OracleResult::Agree => {
            panic!("the minimised recipe must actually reproduce the disagreement")
        }
    }

    // Printed so a test failure or `--nocapture` run shows exactly what a human would paste into a bug report.
    println!("{recipe}");
}
