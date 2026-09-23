use super::*;
use crate::plan::PlanNodeKind;

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

/// A bare, rule-free grammar (no phonological or morphological rules): `should_run` is `false`, and it is the ungated case, one partition group with an empty key.
fn ungated_no_composite_fixture() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>EnumerateUngatedFixture</Name>
<PartsOfSpeech>
  <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
</PartsOfSpeech>
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<Strata>
  <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
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
    .to_string()
}

/// A grammar with one real ungated phonological rewrite rule: `should_run` is `true`, the structural route is absent (no circumfix/dropped-material rule), and it is still ungated (1 group).
fn should_run_ordinary_phonology_fixture() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>EnumerateShouldRunFixture</Name>
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
    .to_string()
}

/// A grammar with one gated MPR-restricted subrule and two entries realizing both truth values of that gate key, so `partition_entries` must yield exactly 2 groups; the structural route stays absent, isolating the Gate seam from the other two.
fn gated_two_group_fixture() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
<Name>EnumerateGatedTwoGroupFixture</Name>
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
    .to_string()
}

/// Every `Leaf` node in `plan` whose fragment matches `pred`.
fn leaves_matching(plan: &Plan, pred: impl Fn(&FragmentSpec) -> bool) -> Vec<NodeId> {
    plan.iter()
        .filter_map(|(id, kind)| match kind {
            PlanNodeKind::Leaf { fragment, .. } if pred(fragment) => Some(id),
            _ => None,
        })
        .collect()
}

fn gate_of(plan: &Plan) -> (NodeId, GatePartitionSpec) {
    plan.iter()
        .find_map(|(id, kind)| match kind {
            PlanNodeKind::Gate { partition, .. } => Some((id, partition.clone())),
            _ => None,
        })
        .expect("plan must contain exactly one Gate node")
}

/// Row 3: the enumerated Plan's Gate `partition.groups.len()` equals the real `partition_entries` count; both must be 1 for the ungated fixture.
#[test]
fn ungated_fixture_collapses_to_single_group_gate_matching_real_seam() {
    let g = load(&ungated_no_composite_fixture());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    // Cross-check against the REAL seam functions, not a re-derivation.
    assert!(
        !preexpand::should_run(&g, phon.as_ref()),
        "fixture must NOT exercise should_run"
    );
    let gated = find_gated_subrules(&g, &ro);
    assert!(gated.is_empty(), "fixture must declare zero gated subrules");
    let real_groups = partition_entries(&g, &gated, &ro);
    assert_eq!(real_groups.len(), 1, "ungated grammar collapses to 1 group");

    let plan = enumerate_default(&g, &ro, phon.as_ref());
    let (_, partition) = gate_of(&plan);
    assert_eq!(
        partition.groups.len(),
        real_groups.len(),
        "enumerated Gate's group count must match the real seam's partition_entries count"
    );
    assert_eq!(partition.groups[0].key, Vec::<bool>::new());
    assert!(
        partition.gated_subrules.is_empty(),
        "no gated subrules declared"
    );

    // Row 1/2: neither composite marker should be present.
    assert!(leaves_matching(&plan, |f| matches!(
        f,
        FragmentSpec::CompositeEmissionMarker
    ))
    .is_empty());
    assert!(leaves_matching(&plan, |f| matches!(
        f,
        FragmentSpec::StructuralCompositeMarker
    ))
    .is_empty());
}

/// Row 1: the composite-emission subtree is present in the enumerated Plan iff `preexpand::should_run` says so.
#[test]
fn composite_subtree_present_iff_should_run() {
    let g = load(&should_run_ordinary_phonology_fixture());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let real_should_run = preexpand::should_run(&g, phon.as_ref());
    assert!(real_should_run, "fixture must exercise should_run");
    let real_refuses = emit::probe_would_refuse(&g);
    assert!(
        !real_refuses,
        "fixture's rule has a real LHS, not epenthesis/metathesis"
    );

    let plan = enumerate_default(&g, &ro, phon.as_ref());
    let composite_leaves = leaves_matching(&plan, |f| {
        matches!(f, FragmentSpec::CompositeEmissionMarker)
    });
    assert_eq!(
        !composite_leaves.is_empty(),
        real_should_run,
        "composite-emission subtree presence must match preexpand::should_run exactly"
    );

    // Row 2: the structural route must be absent (probe_would_refuse is false, no circumfix/dropped-material rule).
    let structural_leaves = leaves_matching(&plan, |f| {
        matches!(f, FragmentSpec::StructuralCompositeMarker)
    });
    assert_eq!(
        !structural_leaves.is_empty(),
        real_refuses,
        "structural-composite subtree presence must match probe_would_refuse for this fixture \
         (no non-refusing structural-rule construct is declared here)"
    );
}

/// Row 3 on a real gated multi-group grammar: the two groups realize different gate keys, so each must get a distinct `Replace` `NodeId` (the soundness invariant); the companion test below proves the other half, that identically-gated groups still dedup to the same `NodeId`.
#[test]
fn gated_two_group_fixture_matches_real_partition_and_gives_distinct_per_group_replace_nodes() {
    let g = load(&gated_two_group_fixture());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let gated = find_gated_subrules(&g, &ro);
    assert_eq!(gated.len(), 1, "fixture declares exactly 1 gated subrule");
    let real_groups = partition_entries(&g, &gated, &ro);
    assert_eq!(
        real_groups.len(),
        2,
        "fixture's 2 entries realize both gate-key values"
    );

    let plan = enumerate_default(&g, &ro, phon.as_ref());
    let (gate_id, partition) = gate_of(&plan);
    assert_eq!(
        partition.groups.len(),
        real_groups.len(),
        "enumerated Gate's group count must match the real seam's partition_entries count"
    );
    assert_eq!(
        partition.gated_subrules.len(),
        gated.len(),
        "one GatedSubruleRef per find_gated_subrules entry"
    );
    assert_eq!(partition.gated_subrules[0].rule_pos, gated[0].rule_pos);
    assert_eq!(partition.gated_subrules[0].sub_idx, gated[0].sub_idx);

    // Both possible keys (true and false) must be realized, since the fixture's 2 entries split exactly that way.
    let mut keys: Vec<Vec<bool>> = partition.groups.iter().map(|gr| gr.key.clone()).collect();
    keys.sort();
    assert_eq!(keys, vec![vec![false], vec![true]]);

    // The soundness invariant: these two groups gate differently ([true] vs [false]), so sharing one Replace node between them would be unsound.
    let PlanNodeKind::Gate { children, .. } = plan.get(gate_id).unwrap() else {
        unreachable!("gate_of only ever returns a Gate node")
    };
    assert_eq!(children.len(), 2, "one Compose child per partition group");
    let replace_ids: Vec<NodeId> = children
        .iter()
        .map(|&compose_id| {
            let PlanNodeKind::Compose { children, .. } = plan.get(compose_id).unwrap() else {
                panic!("each Gate child must be a Compose node")
            };
            children[1]
        })
        .collect();
    assert_ne!(
        replace_ids[0], replace_ids[1],
        "two DIFFERENTLY-gated groups must get DISTINCT Replace NodeIds (task 1.4: a node's \
         compiled artifact must be a pure function of its own NodeId, so no two groups needing \
         different subrule_ok may share one Replace node)"
    );
    let replace_node_count = plan
        .iter()
        .filter(|(_, kind)| kind.kind_name() == "Replace")
        .count();
    assert_eq!(
        replace_node_count, 2,
        "two differently-gated groups must be stored as two DISTINCT Replace nodes, not one \
         shared/deduped node"
    );

    // The two groups' own LexiconFragment leaves must differ (different entries subsets), the real per-group filtering.
    let lexicon_leaves =
        leaves_matching(&plan, |f| matches!(f, FragmentSpec::LexiconFragment { .. }));
    assert_eq!(
        lexicon_leaves.len(),
        2,
        "the 2 groups' lexicon fragments carry different entry subsets, so must NOT dedup \
         against each other"
    );
}

/// The other half of the soundness invariant: groups that gate identically still dedup to the same `Replace` `NodeId` across two independent `enumerate_default` calls, since content addressing dedups by content, never by which `Plan` built a node.
#[test]
fn identically_gated_groups_across_independent_plans_share_the_same_replace_node_id() {
    let g = load(&gated_two_group_fixture());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let plan_1 = enumerate_default(&g, &ro, phon.as_ref());
    let plan_2 = enumerate_default(&g, &ro, phon.as_ref());

    fn replace_ids_by_key(plan: &Plan) -> std::collections::BTreeMap<Vec<bool>, NodeId> {
        let (gate_id, partition) = gate_of(plan);
        let PlanNodeKind::Gate { children, .. } = plan.get(gate_id).unwrap() else {
            unreachable!("gate_of only ever returns a Gate node")
        };
        partition
            .groups
            .iter()
            .zip(children.iter())
            .map(|(group, &compose_id)| {
                let PlanNodeKind::Compose { children, .. } = plan.get(compose_id).unwrap() else {
                    panic!("each Gate child must be a Compose node")
                };
                (group.key.clone(), children[1])
            })
            .collect()
    }

    let ids_1 = replace_ids_by_key(&plan_1);
    let ids_2 = replace_ids_by_key(&plan_2);
    assert_eq!(
        ids_1.keys().collect::<Vec<_>>(),
        ids_2.keys().collect::<Vec<_>>(),
        "both independently-built plans must realize the same set of gate keys"
    );
    for (key, id_1) in &ids_1 {
        let id_2 = ids_2[key];
        assert_eq!(
            *id_1, id_2,
            "the SAME gating key ({key:?}), built in two INDEPENDENT Plans, must yield the \
             SAME Replace NodeId -- content addressing dedups by content, never by which Plan \
             built a node"
        );
    }
}

/// A regression pin for `rule_id_of`: every rewrite-rule Leaf's `PRuleId` must equal the one `prules_in_order` was itself built from, not merely "some id or other".
#[test]
fn rewrite_rule_leaves_carry_the_correct_prule_id() {
    let g = load(&should_run_ordinary_phonology_fixture());
    let ro = prules_in_order(&g);
    assert_eq!(ro.len(), 1, "fixture declares exactly 1 phonological rule");
    let phon = PhonologyProbe::new(&g);

    let expected_id = g
        .prules
        .iter()
        .position(|candidate| std::ptr::eq(candidate, ro[0]))
        .map(|i| PRuleId(i as u32))
        .unwrap();

    let plan = enumerate_default(&g, &ro, phon.as_ref());
    let rule_leaves = leaves_matching(&plan, |f| matches!(f, FragmentSpec::RewriteRule { .. }));
    assert_eq!(rule_leaves.len(), 1);
    let PlanNodeKind::Leaf {
        fragment,
        provenance,
    } = plan.get(rule_leaves[0]).unwrap()
    else {
        unreachable!()
    };
    assert_eq!(*fragment, FragmentSpec::RewriteRule { rule: expected_id });
    assert_eq!(*provenance, Provenance::RewriteRule(expected_id));
}

/// `Plan::root` resolves to a `Union` when both a Gate node and a composite marker are present.
#[test]
fn root_is_union_when_composite_marker_and_gate_both_present() {
    let g = load(&should_run_ordinary_phonology_fixture());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let plan = enumerate_default(&g, &ro, phon.as_ref());

    let root = plan.root().expect("root must be set");
    match plan.get(root).unwrap() {
        PlanNodeKind::Union { children } => {
            assert_eq!(children.len(), 2, "Gate node + composite-emission marker");
        }
        other => panic!("expected Union at root, got {}", other.kind_name()),
    }
}

/// Companion to the above: when neither composite marker is present, the root collapses directly to the Gate node, no pointless one-child `Union`.
#[test]
fn root_is_gate_directly_when_no_composite_markers_present() {
    let g = load(&ungated_no_composite_fixture());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let plan = enumerate_default(&g, &ro, phon.as_ref());

    let root = plan.root().expect("root must be set");
    assert_eq!(
        plan.get(root).unwrap().kind_name(),
        "Gate",
        "no composite markers present -- root must collapse directly to the Gate node"
    );
}

/// Determinism: building the same fixture's Plan twice yields the same root NodeId and node count.
#[test]
fn enumerate_default_is_deterministic_across_independent_calls() {
    let g = load(&gated_two_group_fixture());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let plan_a = enumerate_default(&g, &ro, phon.as_ref());
    let plan_b = enumerate_default(&g, &ro, phon.as_ref());

    assert_eq!(plan_a.root(), plan_b.root());
    assert_eq!(plan_a.len(), plan_b.len());
}

// enumerate_candidates

/// A grammar with ≥2 gate groups must yield 2 candidates, `"default"` and `"gate-group-permuted"`, with genuinely different root NodeIds.
#[test]
fn enumerate_candidates_yields_two_distinct_candidates_for_a_multi_group_gated_fixture() {
    let g = load(&gated_two_group_fixture());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let candidates = enumerate_candidates(&g, &ro, phon.as_ref());
    assert_eq!(
        candidates.len(),
        2,
        "a ≥2-group gated grammar must yield exactly 2 candidates"
    );
    assert_eq!(candidates[0].label, "default");
    assert_eq!(candidates[1].label, "gate-group-permuted");
    assert_ne!(
        candidates[0].plan.root(),
        candidates[1].plan.root(),
        "the two candidates must be genuinely distinct topologies (different root NodeIds)"
    );
}

/// An ungated (single-group) grammar must yield exactly 1 candidate: permuting a single-element group list is a no-op, so `enumerate_candidates` must not append a relabeled copy.
#[test]
fn enumerate_candidates_yields_one_candidate_for_an_ungated_fixture() {
    let g = load(&ungated_no_composite_fixture());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let candidates = enumerate_candidates(&g, &ro, phon.as_ref());
    assert_eq!(
        candidates.len(),
        1,
        "a single-group (ungated) grammar must yield exactly 1 candidate -- permuting 1 group \
         is a no-op, not a genuine second topology"
    );
    assert_eq!(candidates[0].label, "default");
}

/// Determinism across independent calls: building the same fixture's candidates twice yields the same root NodeIds in the same order.
#[test]
fn enumerate_candidates_is_deterministic_across_independent_calls() {
    let g = load(&gated_two_group_fixture());
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);

    let candidates_a = enumerate_candidates(&g, &ro, phon.as_ref());
    let candidates_b = enumerate_candidates(&g, &ro, phon.as_ref());

    let roots_a: Vec<_> = candidates_a
        .iter()
        .map(|c| (c.label, c.plan.root()))
        .collect();
    let roots_b: Vec<_> = candidates_b
        .iter()
        .map(|c| (c.label, c.plan.root()))
        .collect();
    assert_eq!(roots_a, roots_b);
}
