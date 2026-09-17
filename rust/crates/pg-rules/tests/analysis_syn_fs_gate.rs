//! Regression gate: analysis-side syntactic-FS accumulation narrows with `PriorityUnion`, not `Add`.
//! See `docs/research/pg-rules-analysis-syn-fs-gate-notes.md`.

use pg_featstruct::{
    add, is_unifiable, unify, FeatureStruct, FeatureStructBuilder, FeatureValue, SymbolBits,
};
use pg_grammar::model::{Grammar, MorphRuleDef, StratumId};
use pg_rules::morph::analyze;
use pg_rules::Word;
use pg_shape::{NodeKind, ShapeBuilder};

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>AnalysisSynFsGate</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures>
      <SymbolicFeature id="featNum">
        <Name>num</Name>
        <Symbols>
          <Symbol id="symSg">sg</Symbol>
          <Symbol id="symDu">du</Symbol>
          <Symbol id="symPl">pl</Symbol>
        </Symbols>
      </SymbolicFeature>
    </HeadFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cC"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll">
        <Name>All</Name>
        <Segment segment="cC" /><Segment segment="cA" /><Segment segment="cT" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrInner mrOuter">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrInner">
            <Name>inner</Name>
            <RequiredHeadFeatures>
              <FeatureValue feature="featNum" symbolValues="symPl" />
            </RequiredHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subInner">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrOuter">
            <Name>outer</Name>
            <OutputHeadFeatures>
              <FeatureValue feature="featNum" symbolValues="symPl" />
            </OutputHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subOuter">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// Category-change chain, porting the upstream `AnalysisAffixProcessRule_CategoryChangeChain_RequiredOverridesAccumulatedPos`, via a head feature rather than `<RequiredPartsOfSpeech>` (same `ana_syn_fs` semantics either way).
const CATEGORY_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CategoryChangeChain</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <HeadFeatures>
      <SymbolicFeature id="featCat">
        <Name>cat</Name>
        <Symbols>
          <Symbol id="symN">n</Symbol>
          <Symbol id="symV">v</Symbol>
        </Symbols>
      </SymbolicFeature>
    </HeadFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cC"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll">
        <Name>All</Name>
        <Segment segment="cC" /><Segment segment="cA" /><Segment segment="cT" />
        <Segment segment="cU" /><Segment segment="cI" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrN2V mrV2N mrThird">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrN2V">
            <Name>n2v</Name>
            <RequiredHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symN" />
            </RequiredHeadFeatures>
            <OutputHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symV" />
            </OutputHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subN2V">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>u</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrV2N">
            <Name>v2n</Name>
            <RequiredHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symV" />
            </RequiredHeadFeatures>
            <OutputHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symN" />
            </OutputHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subV2N">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>i</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
          <MorphologicalRule id="mrThird">
            <Name>third</Name>
            <RequiredHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symN" />
            </RequiredHeadFeatures>
            <OutputHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symV" />
            </OutputHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subThird">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>a</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn load_grammar(xml: &str) -> Grammar {
    pg_grammar::load(xml).expect("gate grammar loads")
}

fn word_shape(g: &Grammar, text: &str) -> pg_shape::Shape {
    let t = &g.char_tables[0];
    let seg = pg_grammar::segment::segment(t, text).expect("segments");
    let mut b = ShapeBuilder::with_features_capacity(0, seg.len());
    for (_, kind, cd, _) in seg.interior() {
        match kind {
            NodeKind::Segment => b.push_segment_with_lanes(cd, &[]),
            NodeKind::Boundary => b.push_boundary_with_lanes(cd, &[]),
            _ => {}
        }
    }
    b.finish()
}

/// `{head: {feature: symbol}}`, hand-built the same way the XML loader would.
fn head_symbol_fs(g: &Grammar, feature_xml_id: &str, symbol_xml_id: &str) -> FeatureStruct {
    let feat = g
        .syn_features
        .feature_by_xml_id(feature_xml_id)
        .unwrap_or_else(|| panic!("{feature_xml_id} declared"));
    let idx = g
        .syn_features
        .symbol_index(feat, symbol_xml_id)
        .unwrap_or_else(|| panic!("{symbol_xml_id} declared on {feature_xml_id}"));
    let mut inner = FeatureStructBuilder::new();
    inner.add(feat, FeatureValue::Symbolic(SymbolBits::single(idx)));
    let mut outer = FeatureStructBuilder::new();
    outer.add(
        g.syn_features.head.expect("HeadFeatures declared"),
        FeatureValue::Complex(inner.build()),
    );
    outer.build()
}

fn num_fs(g: &Grammar, symbol_xml_id: &str) -> FeatureStruct {
    head_symbol_fs(g, "featNum", symbol_xml_id)
}

fn cat_fs(g: &Grammar, symbol_xml_id: &str) -> FeatureStruct {
    head_symbol_fs(g, "featCat", symbol_xml_id)
}

fn find_affix_rule<'a>(g: &'a Grammar, name: &str) -> &'a MorphRuleDef {
    g.mrules
        .iter()
        .find(|r| matches!(r, MorphRuleDef::AffixProcess(def) if def.name.as_deref() == Some(name)))
        .unwrap_or_else(|| panic!("rule {name} declared"))
}

#[test]
fn analysis_required_fs_overrides_accumulated_value_priority_union() {
    let g = load_grammar(XML);
    let sg = num_fs(&g, "symSg");
    let pl = num_fs(&g, "symPl");

    // Control: confirms this is a genuine narrowing-vs-widening fork, not a vacuous fixture.
    assert_eq!(
        unify(&sg, &pl),
        None,
        "sanity: sg/pl must be disjoint for this gate to mean anything"
    );

    let mut w0 = Word::new(word_shape(&g, "cat"), StratumId(0));
    w0.syn_fs = sg.clone();

    // Rule 1 ("inner"): RequiredHeadFeatures=pl priority-unions onto the accumulated `sg`, overwriting it.
    let inner = find_affix_rule(&g, "inner");
    let out1 = analyze(&g, &w0, inner);
    assert_eq!(
        out1.len(),
        1,
        "the inner rule's LHS should match the whole word exactly once"
    );
    assert_eq!(
        out1[0].syn_fs, pl,
        "PriorityUnion must replace sg with pl entirely, not accumulate both"
    );
    let old_add_result = add(&sg, &pl, &|f| g.syn_features.mask(f));
    assert_ne!(
        out1[0].syn_fs, old_add_result,
        "the old Add semantics would have produced the widened {{sg, pl}} value; PriorityUnion must not"
    );

    // Rule 2 ("outer"): OutputHeadFeatures=pl gates on is_unifiable(out, word.syn), no widened lane needed.
    let outer = find_affix_rule(&g, "outer");
    let out2 = analyze(&g, &out1[0], outer);
    assert_eq!(
        out2.len(),
        1,
        "the outer rule's is_unifiable(out=pl, word.syn=pl) gate must pass under the narrowed value"
    );
}

#[test]
fn analysis_affix_process_rule_category_change_chain_required_overrides_accumulated_pos() {
    // Port of the upstream `AnalysisAffixProcessRule_CategoryChangeChain_RequiredOverridesAccumulatedPos`.
    let g = load_grammar(CATEGORY_XML);
    let n = cat_fs(&g, "symN");
    let v = cat_fs(&g, "symV");
    assert_eq!(
        unify(&n, &v),
        None,
        "sanity: N/V must be disjoint for this gate to mean anything"
    );

    // The surface word's POS after synthesis (root --n2v--> V --v2n--> N).
    let mut w0 = Word::new(word_shape(&g, "catui"), StratumId(0));
    w0.syn_fs = n.clone();

    let v2n = find_affix_rule(&g, "v2n");
    let out1 = analyze(&g, &w0, v2n);
    assert_eq!(out1.len(), 1, "v2n must unapply exactly once");
    assert_eq!(
        out1[0].syn_fs, v,
        "unapplying v2n (RequiredHeadFeatures=V) must leave the FS at exactly V"
    );

    let n2v = find_affix_rule(&g, "n2v");
    let out2 = analyze(&g, &out1[0], n2v);
    assert_eq!(out2.len(), 1, "n2v must unapply exactly once");
    let old_add_result = add(&v, &n, &|f| g.syn_features.mask(f));
    assert_eq!(
        out2[0].syn_fs, n,
        "unapplying n2v (RequiredHeadFeatures=N) must leave the FS at exactly N; Add would have produced {{N, V}}"
    );
    assert_ne!(
        out2[0].syn_fs, old_add_result,
        "the old Add semantics would have retained V alongside N; PriorityUnion must not"
    );

    // `third`'s Out=V must fail is_unifiable against the narrowed N (old Add's {N, V} would wrongly pass).
    let third = find_affix_rule(&g, "third");
    let out3 = analyze(&g, &out2[0], third);
    assert!(
        out3.is_empty(),
        "third's Out=V must not be unifiable with the narrowed N; a nonempty result means the old \
         widened {{N, V}} value leaked through"
    );
    assert!(
        is_unifiable(&v, &old_add_result),
        "sanity: V really would have been unifiable against the old widened {{N, V}} value"
    );
}
