//! Regression gate: analysis-side syntactic-FS accumulation follows `ana_syn_fs`'s "Exact" inverse of synthesis, never the old `Add`/`PriorityUnion` modes. See `docs/research/pg-rules-analysis-syn-fs-gate-notes.md`.

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

    // Rule 1 ("inner"): Required=pl, no Out. Exact's check=PU(pl,EMPTY)=pl is not unifiable with the accumulated sg, so it refuses where the old out-only gate (vacuous, since Out is empty) admitted and PriorityUnion overwrote sg with pl.
    let mut w0 = Word::new(word_shape(&g, "cat"), StratumId(0));
    w0.syn_fs = sg.clone();
    let inner = find_affix_rule(&g, "inner");
    let out1 = analyze(&g, &w0, inner);
    assert!(
        out1.is_empty(),
        "Exact: check=PU(required=pl, out=EMPTY)=pl is not unifiable with the accumulated sg, so \
         the strengthened gate refuses; the old out-only gate ignored `required` here and admitted \
         it, then PriorityUnion overwrote sg with pl"
    );
    assert!(
        is_unifiable(&FeatureStruct::EMPTY, &w0.syn_fs),
        "sanity: the old gate (is_unifiable(out=EMPTY, word)) is vacuously true, confirming it \
         really would have admitted this where Exact's stronger check does not"
    );

    // Rule 2 ("outer"): Out=pl, no Required. Fed a word carrying pl directly (rule 1 no longer produces one from sg under Exact) to isolate remove_paths: outer's own pl contribution is stripped, not left in place.
    let mut w_pl = Word::new(word_shape(&g, "cat"), StratumId(0));
    w_pl.syn_fs = pl.clone();
    let outer = find_affix_rule(&g, "outer");
    let out2 = analyze(&g, &w_pl, outer);
    assert_eq!(
        out2.len(),
        1,
        "outer's Out=pl is unifiable with the word's own pl, so the gate passes"
    );
    assert_eq!(
        out2[0].syn_fs,
        FeatureStruct::EMPTY,
        "Exact: remove_paths strips outer's own Out=pl contribution from the stem entirely; the \
         old PriorityUnion mode left pl in place (required was empty, so it fell through to \
         `word.syn_fs.clone()`)"
    );
    let old_priority_union_result = pl.clone();
    assert_ne!(
        out2[0].syn_fs, old_priority_union_result,
        "the old (pre-Exact) code kept pl on the stem here; Exact must not"
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

// Exact-only cases, porting `AnalysisSyntacticFeatureMergeTests.cs` (see the notes doc for the C# test-name mapping).

/// One grammar for every Exact-only case: `cat` (n/v), `num` (sg/du/pl), `tense` (pres/past), all under Head.
const EXACT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ExactSynFsGate</Name>
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
      <SymbolicFeature id="featNum">
        <Name>num</Name>
        <Symbols>
          <Symbol id="symSg">sg</Symbol>
          <Symbol id="symDu">du</Symbol>
          <Symbol id="symPl">pl</Symbol>
        </Symbols>
      </SymbolicFeature>
      <SymbolicFeature id="featTense">
        <Name>tense</Name>
        <Symbols>
          <Symbol id="symPres">pres</Symbol>
          <Symbol id="symPast">past</Symbol>
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
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrDisjReq mrStrongGate mrTwiceZ mrEmptyRule mrOlInner mrOlOuter">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrDisjReq">
            <Name>disjReq</Name>
            <RequiredHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symN symV" />
            </RequiredHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subDisjReq">
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
          <MorphologicalRule id="mrStrongGate">
            <Name>strongGate</Name>
            <RequiredHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symN" />
              <FeatureValue feature="featNum" symbolValues="symPl" />
            </RequiredHeadFeatures>
            <OutputHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symV" />
            </OutputHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subStrongGate">
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
          <MorphologicalRule id="mrTwiceZ" multipleApplication="2">
            <Name>twiceZ</Name>
            <RequiredHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symN" />
            </RequiredHeadFeatures>
            <OutputHeadFeatures>
              <FeatureValue feature="featCat" symbolValues="symV" />
            </OutputHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subTwiceZ">
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
          <MorphologicalRule id="mrEmptyRule">
            <Name>emptyRule</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subEmptyRule">
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
          <MorphologicalRule id="mrOlInner">
            <Name>olInner</Name>
            <OutputHeadFeatures>
              <FeatureValue feature="featTense" symbolValues="symPres" />
            </OutputHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subOlInner">
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
          <MorphologicalRule id="mrOlOuter">
            <Name>olOuter</Name>
            <OutputHeadFeatures>
              <FeatureValue feature="featTense" symbolValues="symPast" />
            </OutputHeadFeatures>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subOlOuter">
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

fn tense_fs(g: &Grammar, symbol_xml_id: &str) -> FeatureStruct {
    head_symbol_fs(g, "featTense", symbol_xml_id)
}

/// `symbol_xml_ids` all set on the same feature (disjunctive required value, e.g. `cat={n,v}`).
fn head_fs_multi(g: &Grammar, feature_xml_id: &str, symbol_xml_ids: &[&str]) -> FeatureStruct {
    let feat = g
        .syn_features
        .feature_by_xml_id(feature_xml_id)
        .unwrap_or_else(|| panic!("{feature_xml_id} declared"));
    let mut bits = SymbolBits::EMPTY;
    for sym in symbol_xml_ids {
        let idx = g
            .syn_features
            .symbol_index(feat, sym)
            .unwrap_or_else(|| panic!("{sym} declared on {feature_xml_id}"));
        bits.set(idx);
    }
    let mut inner = FeatureStructBuilder::new();
    inner.add(feat, FeatureValue::Symbolic(bits));
    let mut outer = FeatureStructBuilder::new();
    outer.add(
        g.syn_features.head.expect("HeadFeatures declared"),
        FeatureValue::Complex(inner.build()),
    );
    outer.build()
}

/// Multiple DIFFERENT features combined under one head (e.g. `{cat: n, num: pl}`).
fn head_fs_pairs(g: &Grammar, pairs: &[(&str, &str)]) -> FeatureStruct {
    let mut inner = FeatureStructBuilder::new();
    for (feat_xml, sym_xml) in pairs {
        let feat = g
            .syn_features
            .feature_by_xml_id(feat_xml)
            .unwrap_or_else(|| panic!("{feat_xml} declared"));
        let idx = g
            .syn_features
            .symbol_index(feat, sym_xml)
            .unwrap_or_else(|| panic!("{sym_xml} declared on {feat_xml}"));
        inner.add(feat, FeatureValue::Symbolic(SymbolBits::single(idx)));
    }
    let mut outer = FeatureStructBuilder::new();
    outer.add(
        g.syn_features.head.expect("HeadFeatures declared"),
        FeatureValue::Complex(inner.build()),
    );
    outer.build()
}

/// Direct-`ana_syn_fs` port of `OverrideLoss_TenseFlipFlop_AddAndPriorityUnionLoseTheParse_ExactFindsIt` (end-to-end port: `pg-parse`'s `exact_analysis_fs_recall.rs`).
#[test]
fn override_loss_tense_flip_flop_exact_finds_it() {
    let g = load_grammar(EXACT_XML);
    let pres = tense_fs(&g, "symPres");
    let past = tense_fs(&g, "symPast");
    assert_eq!(
        unify(&pres, &past),
        None,
        "sanity: pres/past must be disjoint for this gate to mean anything"
    );
    // Pre-existing master bug: with tense:past never removed from the stem, the old out-only gate blocks `inner` outright.
    assert!(
        !is_unifiable(&pres, &past),
        "old behaviour: with tense:past still on the stem, inner's Out=pres is not unifiable, so \
         the pre-Exact gate blocks the rule that should have been un-appliable here"
    );

    let mut surface = Word::new(word_shape(&g, "cat"), StratumId(0));
    surface.syn_fs = past.clone();

    let outer = find_affix_rule(&g, "olOuter");
    let out1 = analyze(&g, &surface, outer);
    assert_eq!(out1.len(), 1, "outer's Out=past is unifiable with the surface word's own past");
    assert_eq!(
        out1[0].syn_fs,
        FeatureStruct::EMPTY,
        "Exact: remove_paths strips outer's tense:past contribution entirely, leaving no tense at all"
    );

    let inner = find_affix_rule(&g, "olInner");
    let out2 = analyze(&g, &out1[0], inner);
    assert_eq!(
        out2.len(),
        1,
        "Exact: with no tense feature left on the stem, inner's Out=pres gate has nothing to \
         conflict with, so the rule that the pre-existing bug blocked now un-applies"
    );
}

/// Port of `DisjunctiveRequired_MeetingSinglePos_NarrowsOnlyUnderExact`.
#[test]
fn disjunctive_required_meeting_single_pos_narrows_under_exact() {
    let g = load_grammar(EXACT_XML);
    let n = cat_fs(&g, "symN");
    let n_or_v = head_fs_multi(&g, "featCat", &["symN", "symV"]);

    let mut w0 = Word::new(word_shape(&g, "cat"), StratumId(0));
    w0.syn_fs = n.clone();

    let rule = find_affix_rule(&g, "disjReq");
    let out = analyze(&g, &w0, rule);
    assert_eq!(out.len(), 1, "required={{n,v}} overlaps the word's n, so the gate passes");
    assert_eq!(
        out[0].syn_fs, n,
        "Exact: unify({{n,v}}, n) narrows to the intersection (n), not the disjunctive {{n,v}} itself"
    );
    assert_ne!(
        out[0].syn_fs, n_or_v,
        "a mode that merely folded the disjunctive required value in wholesale would keep {{n,v}}"
    );
}

/// Port of `EmptyRequiredAndOut_ClearsFS_ExceptUnderExact`.
#[test]
fn empty_required_and_out_leaves_fs_unchanged_under_exact() {
    let g = load_grammar(EXACT_XML);
    let n = cat_fs(&g, "symN");

    let mut w0 = Word::new(word_shape(&g, "cat"), StratumId(0));
    w0.syn_fs = n.clone();

    let rule = find_affix_rule(&g, "emptyRule");
    let out = analyze(&g, &w0, rule);
    assert_eq!(out.len(), 1, "empty required/out is vacuously unifiable with anything");
    assert_eq!(
        out[0].syn_fs, n,
        "Exact never Clear()s: a rule with neither required nor out features must leave the \
         accumulated FS exactly as it was; the old code cleared it to EMPTY"
    );
}

/// A rule whose Required/Out disagree on a feature the word already carries: Exact's check=PU(required,out) catches it where the old out-only gate would have admitted it.
#[test]
fn stronger_gate_rejects_conflicting_num_where_old_gate_would_admit() {
    let g = load_grammar(EXACT_XML);
    let out_fs = cat_fs(&g, "symV");
    let input = head_fs_pairs(&g, &[("featCat", "symV"), ("featNum", "symSg")]);

    let mut w0 = Word::new(word_shape(&g, "cat"), StratumId(0));
    w0.syn_fs = input.clone();

    let rule = find_affix_rule(&g, "strongGate");
    let out = analyze(&g, &w0, rule);
    assert!(
        out.is_empty(),
        "check=PU(required={{cat:n,num:pl}}, out={{cat:v}})={{cat:v,num:pl}} is not unifiable with \
         the word's num:sg, so Exact's strengthened gate refuses"
    );
    assert!(
        is_unifiable(&out_fs, &input),
        "sanity: the old gate (is_unifiable(out=cat:v, word)) ignores num entirely and would have \
         admitted this"
    );
}

/// Port of `SameRuleAppliedTwice_SecondApplicationGatedDifferentlyByMode` (the `Exact` row).
#[test]
fn same_rule_applied_twice_second_application_rejected_under_exact() {
    let g = load_grammar(EXACT_XML);
    let v = cat_fs(&g, "symV");
    let n = cat_fs(&g, "symN");

    let rule = find_affix_rule(&g, "twiceZ");
    if let MorphRuleDef::AffixProcess(def) = rule {
        assert_eq!(def.max_apps, 2, "sanity: multipleApplication=\"2\" loaded onto max_apps");
    } else {
        panic!("twiceZ must load as AffixProcess");
    }

    let mut w0 = Word::new(word_shape(&g, "cat"), StratumId(0));
    w0.syn_fs = v.clone();

    let out1 = analyze(&g, &w0, rule);
    assert_eq!(out1.len(), 1, "required=n, out=v: check=v is unifiable with the word's own v");
    assert_eq!(
        out1[0].syn_fs, n,
        "remove_paths strips out=v entirely, then unifies the empty stem with required=n"
    );

    let out2 = analyze(&g, &out1[0], rule);
    assert!(
        out2.is_empty(),
        "second application: check is still cat=v (independent of the word), which is not \
         unifiable with the narrowed stem cat=n -- Exact rejects the repeat"
    );
}
