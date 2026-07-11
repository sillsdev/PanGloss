//! Regression gate for plan §13.1.1 Tier-2 #9: analysis-side syntactic-FS accumulation must
//! **widen** (`FeatureStruct.Add`, a per-feature value-set union) rather than **narrow**
//! (`unify`, an intersection) at the three C# call sites (`AnalysisAffixProcessRule.cs:63-68`,
//! `AnalysisCompoundingRule.cs:133-138`, `AnalysisAffixTemplateRule.cs:66`).
//!
//! This end-to-end test models the motivating Amharic pattern directly: two chained
//! `MorphologicalRule`s, the first carrying a rule-level `<RequiredHeadFeatures>` (accumulated via
//! `Add` onto the analysis candidate's syntactic FS), the second gating its own `Apply` on
//! `OutSyntacticFeatureStruct.IsUnifiable(input.SyntacticFeatureStruct)`
//! (`AnalysisAffixProcessRule.cs:46-49`) against whatever the first rule left behind. A 3-symbol
//! `num` feature (`sg`/`du`/`pl`) makes the accumulation a genuine multi-bit (disjunctive) lane,
//! not merely the "delete when the union covers everything" corner case hc-featstruct's unit
//! tests already cover directly:
//!
//! - The root starts at `num=sg`; the inner rule requires `num=pl`.
//! - **Narrowing** (`unify(sg, pl)`) is disjoint and fails outright -- this is the pre-fix Rust
//!   behavior at `morph.rs`'s `ana_syn_fs` (it fell back to the *unchanged* `sg` value rather than
//!   rejecting the candidate, which is its own divergence from C#, but the practical effect on the
//!   chain below is the same: the outer rule's gate sees `sg`, not `pl`).
//! - **Widening** (`add(sg, pl)`) unions to `{sg, pl}` (a real two-bit lane) -- not disjoint from
//!   `pl`, so the outer rule's `is_unifiable` gate against `num=pl` still passes.
//!
//! The outer rule's `Apply` therefore produces zero candidates under narrowing and one under
//! widening: the chain "dies" without the fix and "survives" with it, exactly as
//! rust-conversion.md §13.1.1 describes.

use hc_featstruct::{add, unify, FeatureStruct, FeatureStructBuilder, FeatureValue, SymbolBits};
use hc_grammar::model::{Grammar, MorphRuleDef, StratumId};
use hc_rules::morph::analyze;
use hc_rules::Word;
use hc_shape::{NodeKind, ShapeBuilder};

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
    hc_grammar::load(XML).expect("widening-gate grammar loads")
}

fn word_shape(g: &Grammar, text: &str) -> hc_shape::Shape {
    let t = &g.char_tables[0];
    let seg = hc_grammar::segment::segment(t, text).expect("segments");
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

/// `{head: {num: symbol}}`, hand-built the same way the XML loader would (`build_syn_fs` /
/// `load_syn_fs`), so the test doesn't depend on any lexicon/`AssignedHeadFeatures` machinery --
/// just the bare feature system the two rules already reference.
fn num_fs(g: &Grammar, symbol_xml_id: &str) -> FeatureStruct {
    let feat_num = g.syn_features.feature_by_xml_id("featNum").expect("featNum declared");
    let idx = g
        .syn_features
        .symbol_index(feat_num, symbol_xml_id)
        .unwrap_or_else(|| panic!("{symbol_xml_id} declared on featNum"));
    let mut inner = FeatureStructBuilder::new();
    inner.add(feat_num, FeatureValue::Symbolic(SymbolBits::single(idx)));
    let mut outer = FeatureStructBuilder::new();
    outer.add(g.syn_features.head.expect("HeadFeatures declared"), FeatureValue::Complex(inner.build()));
    outer.build()
}

#[test]
fn analysis_chain_survives_only_because_add_widens_not_narrows() {
    let g = load_widening_grammar();
    let sg = num_fs(&g, "symSg");
    let pl = num_fs(&g, "symPl");

    // Control: confirm this is a genuine narrowing-vs-widening fork, not a vacuous fixture --
    // `sg` and `pl` are disjoint at `num`, so a real `unify` fails outright on this exact pair
    // (the operation the pre-fix Rust code used in `ana_syn_fs`).
    assert_eq!(unify(&sg, &pl), None, "sanity: sg/pl must be disjoint for this gate to mean anything");

    let mut w0 = Word::new(word_shape(&g, "cat"), StratumId(0));
    w0.syn_fs = sg;

    // Rule 1 ("inner"): rule-level `RequiredHeadFeatures` = num:pl, `Add`ed onto the candidate's
    // syntactic FS on unapply (AnalysisAffixProcessRule.cs:63-68). One candidate; its widened FS
    // must retain BOTH `sg` (from the input) and `pl` (from the requirement) as a real two-bit
    // lane -- not narrowed to just `pl`, and not silently left at just `sg`.
    let out1 = analyze(&g, &w0, &g.mrules[0]);
    assert_eq!(out1.len(), 1, "the inner rule's LHS should match the whole word exactly once");
    let expected_widened = add(&num_fs(&g, "symSg"), &num_fs(&g, "symPl"), &|f| g.syn_features.mask(f));
    assert_eq!(
        out1[0].syn_fs, expected_widened,
        "the accumulated FS must be the union {{sg, pl}}, matching hc_featstruct::add directly"
    );

    // Rule 2 ("outer"): gates its own `Apply` on `OutSyntacticFeatureStruct.IsUnifiable(input.
    // SyntacticFeatureStruct)` (AnalysisAffixProcessRule.cs:46-49) against `OutputHeadFeatures` =
    // num:pl. Under narrowing, rule 1's output would carry only `sg` (or, in the old code's actual
    // fallback-on-failure behavior, still just `sg`) and this gate -- `is_unifiable({pl}, {sg})`
    // -- fails, killing the chain. Under the fix, rule 1's output carries `{sg, pl}`, which
    // overlaps `{pl}`, so the gate passes and the chain survives.
    let out2 = analyze(&g, &out1[0], &g.mrules[1]);
    assert_eq!(
        out2.len(),
        1,
        "the outer rule must still apply: its OutputHeadFeatures=pl gate must see rule 1's widened \
         {{sg, pl}} FS (which overlaps pl), not a narrowed-to-sg or narrowing-failed value"
    );

    // And the negative control: replaying rule 1's step with `unify` instead of `add` (i.e. the
    // pre-fix narrowing operator) on this exact input reproduces the failure the fix eliminates,
    // pinning down *why* the old code could never have produced a survivable candidate here.
    let MorphRuleDef::AffixProcess(inner_def) = &g.mrules[0] else { panic!("expected affix rule") };
    let req = g.fs_interner.get(inner_def.required_syn_fs);
    assert_eq!(
        unify(&w0.syn_fs, req),
        None,
        "narrowing rule 1's Add site would have failed outright on this input"
    );
}
