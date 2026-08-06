//! Regression gate: analysis-side syntactic-FS accumulation must widen, not narrow.
//! See `docs/research/pg-rules-widening-gate-notes.md`.

use pg_featstruct::{add, unify, FeatureStruct, FeatureStructBuilder, FeatureValue, SymbolBits};
use pg_grammar::model::{Grammar, MorphRuleDef, StratumId};
use pg_rules::morph::analyze;
use pg_rules::Word;
use pg_shape::{NodeKind, ShapeBuilder};

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>WideningGate</Name>
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

fn load_widening_grammar() -> Grammar {
    pg_grammar::load(XML).expect("widening-gate grammar loads")
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

/// `{head: {num: symbol}}`, hand-built the same way the XML loader would.
/// See `docs/research/pg-rules-widening-gate-notes.md`.
fn num_fs(g: &Grammar, symbol_xml_id: &str) -> FeatureStruct {
    let feat_num = g
        .syn_features
        .feature_by_xml_id("featNum")
        .expect("featNum declared");
    let idx = g
        .syn_features
        .symbol_index(feat_num, symbol_xml_id)
        .unwrap_or_else(|| panic!("{symbol_xml_id} declared on featNum"));
    let mut inner = FeatureStructBuilder::new();
    inner.add(feat_num, FeatureValue::Symbolic(SymbolBits::single(idx)));
    let mut outer = FeatureStructBuilder::new();
    outer.add(
        g.syn_features.head.expect("HeadFeatures declared"),
        FeatureValue::Complex(inner.build()),
    );
    outer.build()
}

#[test]
fn analysis_chain_survives_only_because_add_widens_not_narrows() {
    let g = load_widening_grammar();
    let sg = num_fs(&g, "symSg");
    let pl = num_fs(&g, "symPl");

    // Control: confirms this is a genuine narrowing-vs-widening fork, not a vacuous fixture.
    assert_eq!(
        unify(&sg, &pl),
        None,
        "sanity: sg/pl must be disjoint for this gate to mean anything"
    );

    let mut w0 = Word::new(word_shape(&g, "cat"), StratumId(0));
    w0.syn_fs = sg;

    // Rule 1 ("inner"): its widened FS must retain both `sg` and `pl` as a real two-bit lane.
    // See `docs/research/pg-rules-widening-gate-notes.md`.
    let out1 = analyze(&g, &w0, &g.mrules[0]);
    assert_eq!(
        out1.len(),
        1,
        "the inner rule's LHS should match the whole word exactly once"
    );
    let expected_widened = add(&num_fs(&g, "symSg"), &num_fs(&g, "symPl"), &|f| {
        g.syn_features.mask(f)
    });
    assert_eq!(
        out1[0].syn_fs, expected_widened,
        "the accumulated FS must be the union {{sg, pl}}, matching pg_featstruct::add directly"
    );

    // Rule 2 ("outer"): under widening rule 1's output carries `{sg, pl}`, so this gate still passes.
    // See `docs/research/pg-rules-widening-gate-notes.md`.
    let out2 = analyze(&g, &out1[0], &g.mrules[1]);
    assert_eq!(
        out2.len(),
        1,
        "the outer rule must still apply: its OutputHeadFeatures=pl gate must see rule 1's widened \
         {{sg, pl}} FS (which overlaps pl), not a narrowed-to-sg or narrowing-failed value"
    );

    // Negative control: replaying rule 1's step with `unify` instead of `add` reproduces the failure the fix eliminates.
    let MorphRuleDef::AffixProcess(inner_def) = &g.mrules[0] else {
        panic!("expected affix rule")
    };
    let req = g.fs_interner.get(inner_def.required_syn_fs);
    assert_eq!(
        unify(&w0.syn_fs, req),
        None,
        "narrowing rule 1's Add site would have failed outright on this input"
    );
}
