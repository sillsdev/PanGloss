use super::*;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

// Fixtures.

/// An ordinary, ungated, single-stratum grammar: one rewrite rule, one lexical entry, no capability gaps at all.
fn ordinary_fixture() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PlanDiagramOrdinaryFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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
      <Stratum characterDefinitionTable="t1" phonologicalRules="pr1">
        <Name>OnlyStratum</Name>
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

/// A 2-group gated grammar plus an independent ungated stratum; `e0_has_mpr1` toggles 2 vs 1 partition groups, while the independent stratum's leaf must stay byte-identical either way.
fn gated_plus_independent_stratum_fixture(e0_has_mpr1: bool) -> String {
    let e0_attr = if e0_has_mpr1 {
        r#" ruleFeatures="mpr1""#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PlanDiagramContentAddressFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mpr1">f1</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c3"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c4"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
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
      <PhonologicalRule id="pruleIndep">
        <Name>indep</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c3" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="c4" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" phonologicalRules="prule1">
        <Name>Gated</Name>
        <LexicalEntries>
          <LexicalEntry id="e0" partOfSpeech="posV"{e0_attr}>
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e0</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e1" partOfSpeech="posV" ruleFeatures="mpr1">
            <Allomorphs><Allomorph id="allo1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <Stratum characterDefinitionTable="t1" phonologicalRules="pruleIndep">
        <Name>Independent</Name>
        <LexicalEntries>
          <!-- Always tagged ruleFeatures="mpr1": gate::partition_entries buckets every grammar-wide entry, so pinning this one keeps its contribution constant across the e0 toggle. -->
          <LexicalEntry id="e2" partOfSpeech="posV" ruleFeatures="mpr1">
            <Allomorphs><Allomorph id="allo2"><PhoneticShape>s</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e2</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    )
}

/// A 2-stratum grammar plus an `Overwrite` MPR group, the permanent carve-out; the render test asserts both stratum names appear and a refusal is visible.
fn multi_stratum_refused_fixture() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PlanDiagramMultiStratumRefusedFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mprA">A</MorphologicalPhonologicalRuleFeature>
      <MorphologicalPhonologicalRuleFeatureGroup matchType="all" outputType="overwrite" features="mprA"><Name>GOverwrite</Name></MorphologicalPhonologicalRuleFeatureGroup>
    </MorphologicalPhonologicalRuleFeatures>
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
        <Name>PR1</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncAll" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" phonologicalRules="pr1">
        <Name>StratumAlpha</Name>
        <LexicalEntries>
          <LexicalEntry id="e1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <Stratum characterDefinitionTable="t1">
        <Name>StratumBeta</Name>
        <LexicalEntries>
          <LexicalEntry id="e2" partOfSpeech="posV">
            <Allomorphs><Allomorph id="a2"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e2</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    .to_string()
}

/// One `Simultaneous` rule with genuinely-overlapping subrules plus an ordinary sibling rule in the same stratum, demonstrating a node-local refusal, contrasting `multi_stratum_refused_fixture`'s grammar-wide one.
fn mixed_node_local_refusal_fixture() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PlanDiagramMixedNodeLocalFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featVoice"><Name>voice</Name><Symbols>
        <Symbol id="symVless">vless</Symbol><Symbol id="symVd1">vd1</Symbol><Symbol id="symVd2">vd2</Symbol><Symbol id="symVoc">voc</Symbol>
      </Symbols></SymbolicFeature>
      <SymbolicFeature id="featPlace"><Name>place</Name><Symbols>
        <Symbol id="symFront">front</Symbol><Symbol id="symMid">mid</Symbol><Symbol id="symBack">back</Symbol><Symbol id="symNeutral">neutral</Symbol>
      </Symbols></SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVless" /><FeatureValue feature="featPlace" symbolValues="symNeutral" /></SegmentDefinition>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVd1" /><FeatureValue feature="featPlace" symbolValues="symNeutral" /></SegmentDefinition>
        <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVd2" /><FeatureValue feature="featPlace" symbolValues="symNeutral" /></SegmentDefinition>
        <SegmentDefinition id="ci"><Representations><Representation>i</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVoc" /><FeatureValue feature="featPlace" symbolValues="symFront" /></SegmentDefinition>
        <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVoc" /><FeatureValue feature="featPlace" symbolValues="symMid" /></SegmentDefinition>
        <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVoc" /><FeatureValue feature="featPlace" symbolValues="symBack" /></SegmentDefinition>
        <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVless" /><FeatureValue feature="featPlace" symbolValues="symNeutral" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncStop"><Name>Stop</Name><FeatureValue feature="featVoice" symbolValues="symVless" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncBackOrMid"><Name>BackOrMid</Name><FeatureValue feature="featPlace" symbolValues="symBack symMid" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncMidOrFront"><Name>MidOrFront</Name><FeatureValue feature="featPlace" symbolValues="symMid symFront" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncB"><Name>B</Name><FeatureValue feature="featVoice" symbolValues="symVd1" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncD"><Name>D</Name><FeatureValue feature="featVoice" symbolValues="symVd2" /></FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prOverlap" multipleApplicationOrder="simultaneous">
        <Name>simOverlap</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncB" /></PhoneticSequence></PhoneticOutput>
            <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBackOrMid" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
          </PhonologicalSubrule>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncD" /></PhoneticSequence></PhoneticOutput>
            <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncMidOrFront" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="prOrdinary">
        <Name>ordinaryRule</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="ct" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" phonologicalRules="prOverlap prOrdinary">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryPU" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloPU"><PhoneticShape>pu</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>PU</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#
    .to_string()
}

/// Three sibling, ungated, mutually-independent rules in one stratum with no shared leaves, so collapsing this one parent's leaf group unambiguously reduces the emitted node count.
fn three_rule_ungated_fixture() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PlanDiagramThreeRuleFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c3"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c4"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c5"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c6"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="pr1">
        <Name>PR1</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="pr2">
        <Name>PR2</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c3" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="c4" /></PhoneticSequence></PhoneticOutput>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
      <PhonologicalRule id="pr3">
        <Name>PR3</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c5" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules><PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><Segment segment="c6" /></PhoneticSequence></PhoneticOutput>
        </PhonologicalSubrule></PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" phonologicalRules="pr1 pr2 pr3">
        <Name>OnlyStratum</Name>
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

// 1. JSON: round trip, determinism.

#[test]
fn plan_diagram_round_trip() {
    let g = load(&ordinary_fixture());
    let doc = build_plan_document(&g);
    let json = doc.to_json().expect("serialize");
    let parsed = PlanDocument::from_json(&json).expect("deserialize");
    assert_eq!(
        parsed, doc,
        "round trip through canonical JSON must be lossless"
    );
}

#[test]
fn plan_diagram_schema_version_is_stamped() {
    let g = load(&ordinary_fixture());
    assert_eq!(
        build_plan_document(&g).schema_version,
        PLAN_DIAGRAM_SCHEMA_VERSION
    );
}

/// An unchanged grammar planned twice must produce identical serialized JSON, including node identities.
#[test]
fn plan_diagram_determinism() {
    let g = load(&gated_plus_independent_stratum_fixture(false));
    let doc_a = build_plan_document(&g);
    let doc_b = build_plan_document(&g);
    let json_a = doc_a.to_json().expect("serialize a");
    let json_b = doc_b.to_json().expect("serialize b");
    assert_eq!(
        json_a, json_b,
        "byte-identical JSON for two builds of the SAME grammar"
    );
}

// 2. The content-address property, pinned.

#[test]
fn plan_diagram_content_address_property_moves_affected_nodes_not_unrelated_siblings() {
    let baseline = load(&gated_plus_independent_stratum_fixture(false));
    let changed = load(&gated_plus_independent_stratum_fixture(true));

    let doc_baseline = build_plan_document(&baseline);
    let doc_changed = build_plan_document(&changed);

    // Sanity: the change actually altered the Gate's own shape (2 groups -> 1).
    let gate_group_count = |doc: &PlanDocument| -> usize {
        doc.nodes
            .iter()
            .find_map(|n| match &n.payload {
                NodePayload::Gate { group_keys, .. } => Some(group_keys.len()),
                _ => None,
            })
            .expect("plan must contain exactly one Gate node")
    };
    assert_eq!(
        gate_group_count(&doc_baseline),
        2,
        "baseline must realize 2 gate groups"
    );
    assert_eq!(
        gate_group_count(&doc_changed),
        1,
        "changed fixture must collapse to 1 group"
    );

    // The affected node (root) and its ancestors move.
    assert_ne!(
        doc_baseline.root, doc_changed.root,
        "the Gate/root identity must move when the grammar's gating structure changes"
    );

    // The unrelated, ungated stratum's leaf is untouched: its content address is the `PRuleId` alone, never a function of the other stratum's gating.
    let indep_leaf_id = |doc: &PlanDocument| -> String {
        doc.nodes
            .iter()
            .find(|n| n.label.contains("'indep'"))
            .unwrap_or_else(|| panic!("must find the independent rule's own leaf: {doc:?}"))
            .id
            .clone()
    };
    assert_eq!(
        indep_leaf_id(&doc_baseline),
        indep_leaf_id(&doc_changed),
        "an unrelated sibling subtree's identity must NOT move when a different construct's \
         content changes"
    );
}

// 3. Capability verdicts: real evaluation, whole-plan AND node-local shapes.

#[test]
fn plan_diagram_ordinary_grammar_admits_every_node() {
    let g = load(&ordinary_fixture());
    let doc = build_plan_document(&g);
    assert_eq!(doc.overall_verdict, NodeVerdict::Admit);
    for node in &doc.nodes {
        assert_eq!(
            node.verdict,
            NodeVerdict::Admit,
            "node {node:?} must read Admit"
        );
    }
}

/// The blind per-node mirror is faithful to the one compiler still gated by every predicate, not to the JOIN `overall_verdict` reports.
/// See docs/research/pg-foma-capability-design-notes.md.
#[test]
fn plan_diagram_root_verdict_matches_the_fully_constrained_strategys_envelope() {
    for xml in [
        ordinary_fixture(),
        gated_plus_independent_stratum_fixture(false),
        multi_stratum_refused_fixture(),
        mixed_node_local_refusal_fixture(),
    ] {
        let strategy = crate::enumerate::EmissionStrategy::TemplatedUnderlyingTokens;
        let g = load(&xml);
        let semantics = GrammarSemantics::derive(&g);
        let registry = default_registry();
        assert_eq!(
            registry
                .predicates()
                .iter()
                .filter(|p| p.constrains_strategies().contains(&strategy))
                .count(),
            registry.predicates().len(),
            "this test's comparand is only valid while TemplatedUnderlyingTokens is constrained \
             by every registered predicate"
        );

        let plan = plan_for_semantics(&semantics);
        let doc = build_plan_document(&g);
        let root_id = doc.root.clone().expect("root must be set");
        let root_verdict = doc.node(&root_id).unwrap().verdict.clone();
        let fully_constrained = crate::capability::compose_envelope_for_strategy(
            &semantics, &plan, strategy, &registry,
        );
        assert_eq!(
            root_verdict,
            NodeVerdict::from_decision(&fully_constrained),
            "the root node's own mirrored verdict must match the fully-constrained compiler's \
             verdict for {xml}"
        );
    }
}

/// A grammar-wide characteristic with no distinct `PlanNodeKind` legitimately marks every node refused; the real algorithm's answer, not a rendering bug.
#[test]
fn plan_diagram_grammar_wide_confirm_only_marks_every_node_confirm_only() {
    let g = load(&multi_stratum_refused_fixture());
    let doc = build_plan_document(&g);
    assert_eq!(doc.overall_verdict, NodeVerdict::ConfirmOnly);
    assert!(
        doc.nodes
            .iter()
            .all(|n| n.verdict == NodeVerdict::ConfirmOnly),
        "every node must read Refuse when a grammar-wide, node-agnostic characteristic is \
         observed (mpr-group.overwrite-output has no distinct PlanNodeKind to localize to)"
    );
}

/// Node-local refusal: only the overlapping rule's leaf refuses; the ordinary sibling leaf stays Admit, proving verdicts are never inferred from a node merely existing.
#[test]
fn plan_diagram_node_local_refusal_leaves_unrelated_sibling_rule_admitted() {
    let g = load(&mixed_node_local_refusal_fixture());
    let doc = build_plan_document(&g);
    let root_id = doc.root.clone().expect("root must be set");
    assert!(
        doc.node(&root_id).unwrap().verdict.is_refused(),
        "the overlapping rule must refuse the whole plan's node-local walk"
    );
    assert!(
        !doc.overall_verdict.is_refused(),
        "the whole-grammar JOIN must NOT refuse: the mainline compiler composes no cascade, so \
         this rule's overlap is not its limit"
    );

    let overlap_leaf = doc
        .nodes
        .iter()
        .find(|n| n.label.contains("'simOverlap'"))
        .expect("must find the overlapping rule's own leaf");
    let ordinary_leaf = doc
        .nodes
        .iter()
        .find(|n| n.label.contains("'ordinaryRule'"))
        .expect("must find the ordinary rule's own leaf");

    assert!(
        overlap_leaf.verdict.is_refused(),
        "the overlapping rule's own leaf must refuse"
    );
    assert_eq!(
        ordinary_leaf.verdict,
        NodeVerdict::Admit,
        "the unrelated, ordinary sibling rule's own leaf must stay Admit -- never inferred from \
         merely being a RewriteRule leaf in a plan that has SOME refusal somewhere"
    );
}

// 4. Linguistic labelling.

#[test]
fn plan_diagram_labels_name_stratum_and_rule_not_only_node_kind() {
    let g = load(&ordinary_fixture());
    let doc = build_plan_document(&g);
    let rule_leaf = doc
        .nodes
        .iter()
        .find(|n| matches!(&n.payload, NodePayload::Leaf { fragment, .. } if fragment == "rewrite_rule"))
        .expect("must find the rewrite-rule leaf");
    assert!(
        rule_leaf.label.contains("OnlyStratum"),
        "label must name the owning stratum"
    );
    assert!(rule_leaf.label.contains("'PR'"), "label must name the rule");
    assert_eq!(
        rule_leaf.kind, "Leaf",
        "node kind is carried, but as secondary detail"
    );
}

// 5. Mermaid rendering.

/// A multi-stratum fixture's diagram distinguishes the strata, and a refused construct is marked refused.
#[test]
fn plan_diagram_render_distinguishes_strata_and_marks_refusal() {
    let g = load(&multi_stratum_refused_fixture());
    let doc = build_plan_document(&g);
    let render = render_mermaid(&doc, RenderMode::default());

    assert!(
        render.mermaid.contains("StratumAlpha"),
        "must distinguish the first stratum"
    );
    assert!(
        render.mermaid.contains("StratumBeta"),
        "must distinguish the second stratum"
    );
    assert!(
        render.mermaid.contains("ConfirmOnly"),
        "the overwrite construct must be visibly marked ConfirmOnly"
    );
    assert!(
        !render.summarized,
        "this small fixture must not need any collapsing"
    );
    assert_eq!(render.emitted_node_count, render.total_node_count);
}

#[test]
fn plan_diagram_render_reports_summarization_facts_in_text_and_struct() {
    let g = load(&three_rule_ungated_fixture());
    let doc = build_plan_document(&g);

    // The single Replace node's 3 sibling rewrite-rule leaves exceed a threshold of 2.
    let render = render_mermaid(&doc, RenderMode::Summarized { threshold: 2 });
    assert!(
        render.summarized,
        "3 sibling leaves must exceed a threshold of 2"
    );
    assert_eq!(render.threshold, Some(2));
    assert!(render.mermaid.contains("summarization: applied"));
    assert!(render.mermaid.contains("3 x rewrite_rule leaves collapsed"));
    assert!(render.mermaid.contains(&format!(
        "nodes emitted: {} of {}",
        render.emitted_node_count, render.total_node_count
    )));
    assert!(
        render.emitted_node_count < render.total_node_count,
        "collapsing 3 exclusively-owned sibling leaves into 1 summary node must reduce the \
         emitted count"
    );
}

#[test]
fn plan_diagram_render_full_mode_never_collapses_and_reports_no_threshold() {
    let g = load(&ordinary_fixture());
    let doc = build_plan_document(&g);
    let render = render_mermaid(&doc, RenderMode::Full);
    assert!(!render.summarized);
    assert_eq!(render.threshold, None);
    assert_eq!(render.emitted_node_count, render.total_node_count);
    assert!(render.mermaid.contains("summarization: not applied"));
}

/// Collapsing a large sibling-leaf group never erases the surrounding structure: non-`Leaf` nodes still render individually alongside the summary node.
#[test]
fn plan_diagram_render_collapsing_leaves_non_leaf_structure_intact() {
    let g = load(&three_rule_ungated_fixture());
    let doc = build_plan_document(&g);
    let render = render_mermaid(&doc, RenderMode::Summarized { threshold: 2 });
    assert!(render.summarized);
    assert!(render.emitted_node_count < render.total_node_count);
    assert!(
        render.mermaid.contains("Gate:"),
        "non-leaf nodes must still render individually"
    );
    assert!(
        render.mermaid.contains("Rewrite cascade:"),
        "the Replace node itself still renders"
    );
    assert!(
        render.mermaid.contains("Lexicon fragment:"),
        "the lexicon leaf (a different fragment kind, group size 1) is not swept into the \
         rewrite_rule leaves' own summary group"
    );
}

#[test]
fn plan_diagram_render_empty_plan_is_handled() {
    let empty = PlanDocument {
        schema_version: PLAN_DIAGRAM_SCHEMA_VERSION,
        root: None,
        overall_verdict: NodeVerdict::Admit,
        nodes: Vec::new(),
    };
    let render = render_mermaid(&empty, RenderMode::default());
    assert_eq!(render.emitted_node_count, 0);
    assert_eq!(render.total_node_count, 0);
    assert!(render.mermaid.contains("flowchart TD"));
}

// 6. Golden rendered diagram for one small synthetic fixture.

/// The single small synthetic fixture the golden mermaid diagram is regenerated from, deliberately tiny so the golden text stays short and reviewable.
fn golden_fixture_grammar() -> Grammar {
    load(&ordinary_fixture())
}

#[track_caller]
fn assert_plan_diagram_golden(actual: &str, expected: &str) {
    crate::test_support::assert_rendered_text_eq(actual, expected);
}

#[test]
fn plan_diagram_raw_golden_boundary_would_reject_crlf_materialized_fixture() {
    let actual = "flowchart TD\n";
    let expected = "flowchart TD\r\n";
    assert_ne!(actual, expected);
    assert_plan_diagram_golden(actual, expected);
}

/// Regeneration helper: run with `--ignored` after a reviewed rendering change, never hand-edit `plan_diagram_golden.mmd`.
#[test]
#[ignore = "regeneration helper, not a gate: run with --ignored to rewrite the golden from this \
            test's own computation after a reviewed rendering change"]
fn regenerate_plan_diagram_golden_mermaid() {
    let g = golden_fixture_grammar();
    let doc = build_plan_document(&g);
    let render = render_mermaid(&doc, RenderMode::default());
    std::fs::write(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/plan_diagram_golden.mmd"),
        &render.mermaid,
    )
    .expect("golden must be writable");
}

#[test]
fn plan_diagram_golden_mermaid() {
    let g = golden_fixture_grammar();
    let doc = build_plan_document(&g);
    let render = render_mermaid(&doc, RenderMode::default());
    assert_plan_diagram_golden(&render.mermaid, GOLDEN_MERMAID);
}

const GOLDEN_MERMAID: &str = include_str!("../plan_diagram_golden.mmd");
