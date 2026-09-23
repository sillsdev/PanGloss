//! Synthetic, delanguaged fixtures only (no natural-language names), mirroring
//! `capability.rs`'s own test-module convention.

use pg_grammar::model::{PhonRuleDef, RewriteMode};

use super::*;

fn load(xml: &str) -> pg_grammar::model::Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

const OVERLAP_LOWER_PROBE_XML: &str = r#"<HermitCrabInput><Language><Name>OverlapLowerProbe</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <PhonologicalFeatureSystem>
    <SymbolicFeature id="featPlace"><Name>place</Name>
      <Symbols>
        <Symbol id="symNeutral">neutral</Symbol>
        <Symbol id="symFront">front</Symbol>
        <Symbol id="symBack">back</Symbol>
      </Symbols>
    </SymbolicFeature>
  </PhonologicalFeatureSystem>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="cStop"><Representations><Representation>p</Representation></Representations>
        <FeatureValue feature="featPlace" symbolValues="symNeutral" />
      </SegmentDefinition>
      <SegmentDefinition id="cFront"><Representations><Representation>i</Representation></Representations>
        <FeatureValue feature="featPlace" symbolValues="symFront" />
      </SegmentDefinition>
      <SegmentDefinition id="cBack"><Representations><Representation>u</Representation></Representations>
        <FeatureValue feature="featPlace" symbolValues="symBack" />
      </SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses>
    <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cStop" /></SegmentNaturalClass>
    <FeatureNaturalClass id="ncFront"><Name>Front</Name>
      <FeatureValue feature="featPlace" symbolValues="symFront" />
    </FeatureNaturalClass>
    <FeatureNaturalClass id="ncBack"><Name>Back</Name>
      <FeatureValue feature="featPlace" symbolValues="symBack" />
    </FeatureNaturalClass>
  </NaturalClasses>
  <PhonologicalRuleDefinitions>
    <PhonologicalRule id="prNoOverlap" multipleApplicationOrder="simultaneous"><Name>noOverlap</Name>
      <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
      <PhonologicalSubrules>
        <PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
          <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncFront" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
        </PhonologicalSubrule>
        <PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
          <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
        </PhonologicalSubrule>
      </PhonologicalSubrules>
    </PhonologicalRule>
    <PhonologicalRule id="prOverlap" multipleApplicationOrder="simultaneous"><Name>overlap</Name>
      <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
      <PhonologicalSubrules>
        <PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
        </PhonologicalSubrule>
        <PhonologicalSubrule>
          <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticOutput>
        </PhonologicalSubrule>
      </PhonologicalSubrules>
    </PhonologicalRule>
  </PhonologicalRuleDefinitions>
</Language></HermitCrabInput>"#;

fn rewrite_rule<'g>(
    g: &'g pg_grammar::model::Grammar,
    xml_id: &str,
) -> &'g pg_grammar::model::RewriteRuleDef {
    for pr in &g.prules {
        if let PhonRuleDef::Rewrite(r) = pr {
            if r.xml_id == xml_id {
                return r;
            }
        }
    }
    panic!("rewrite rule {xml_id:?} not found");
}

/// Two subrules whose right environments are mutually exclusive natural classes must lower to spans whose focus+right intersection is empty — they cannot both hold at the same position.
#[test]
fn lower_span_disjoint_right_environments_do_not_overlap() {
    let g = load(OVERLAP_LOWER_PROBE_XML);
    let r = rewrite_rule(&g, "prNoOverlap");
    assert_eq!(r.mode, RewriteMode::Simultaneous);
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    let span_a = lower_span(
        &opts,
        &g,
        &alphabet,
        r.subrules[0].left_env.as_ref(),
        &r.lhs,
        r.subrules[0].right_env.as_ref(),
    )
    .expect("prNoOverlap subrule 0 must lower (no unsupported nodes)");
    let span_b = lower_span(
        &opts,
        &g,
        &alphabet,
        r.subrules[1].left_env.as_ref(),
        &r.lhs,
        r.subrules[1].right_env.as_ref(),
    )
    .expect("prNoOverlap subrule 1 must lower (no unsupported nodes)");

    assert!(
        !spans_overlap(&opts, &span_a, &span_b),
        "Front/Back-flanked subrules must NOT overlap"
    );
}

/// Two subrules with identical (unconstrained) focus/environment lower to the same span, so their intersection is trivially non-empty.
#[test]
fn lower_span_identical_unconstrained_subrules_overlap() {
    let g = load(OVERLAP_LOWER_PROBE_XML);
    let r = rewrite_rule(&g, "prOverlap");
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    let span_a = lower_span(
        &opts,
        &g,
        &alphabet,
        r.subrules[0].left_env.as_ref(),
        &r.lhs,
        r.subrules[0].right_env.as_ref(),
    )
    .expect("prOverlap subrule 0 must lower");
    let span_b = lower_span(
        &opts,
        &g,
        &alphabet,
        r.subrules[1].left_env.as_ref(),
        &r.lhs,
        r.subrules[1].right_env.as_ref(),
    )
    .expect("prOverlap subrule 1 must lower");

    assert!(
        spans_overlap(&opts, &span_a, &span_b),
        "two unconstrained same-focus subrules must overlap"
    );
}

// A genuinely unbounded (max: None) quantifier compiles via foma's native E*/E^>N operator.

use foma::apply::{apply_init, apply_up};

/// One `CharacterDefinitionTable` and four `PhonologicalRule`s, each a bare Segment-focused LHS with one quantifier-bearing probe fed straight to `pattern_slots`, never compiled/composed.
const QUANTIFIER_SCOPE_PROBE_XML: &str = r#"<HermitCrabInput><Language><Name>QuantifierScopeProbe</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <PhonologicalFeatureSystem>
    <SymbolicFeature id="featA"><Name>a</Name>
      <Symbols><Symbol id="symX">x</Symbol><Symbol id="symY">y</Symbol></Symbols>
    </SymbolicFeature>
  </PhonologicalFeatureSystem>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="c1"><Representations><Representation>a</Representation></Representations>
        <FeatureValue feature="featA" symbolValues="symX" />
      </SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses>
    <SegmentNaturalClass id="ncC1"><Name>C1</Name><Segment segment="c1" /></SegmentNaturalClass>
  </NaturalClasses>
  <PhonologicalRuleDefinitions>
    <PhonologicalRule id="prUnboundedMinZero"><Name>demo0</Name>
      <PhoneticInput><PhoneticSequence>
        <OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
      </PhoneticSequence></PhoneticInput>
      <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
    </PhonologicalRule>
    <PhonologicalRule id="prUnboundedLargeMin"><Name>demo1</Name>
      <PhoneticInput><PhoneticSequence>
        <OptionalSegmentSequence min="1000" max="-1"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
      </PhoneticSequence></PhoneticInput>
      <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
    </PhonologicalRule>
    <PhonologicalRule id="prInvertedFinite"><Name>demo2</Name>
      <PhoneticInput><PhoneticSequence>
        <OptionalSegmentSequence min="5" max="2"><SimpleContext naturalClass="ncC1" /></OptionalSegmentSequence>
      </PhoneticSequence></PhoneticInput>
      <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
    </PhonologicalRule>
    <PhonologicalRule id="prAlphaNestedUnbounded"><Name>demo4</Name>
      <VariableFeatures><VariableFeature id="var1" name="a" phonologicalFeature="featA" /></VariableFeatures>
      <PhoneticInput><PhoneticSequence>
        <OptionalSegmentSequence min="1" max="-1">
          <SimpleContext naturalClass="ncC1"><AlphaVariables><AlphaVariable variableFeature="var1" /></AlphaVariables></SimpleContext>
        </OptionalSegmentSequence>
      </PhoneticSequence></PhoneticInput>
      <PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticOutput></PhonologicalSubrule></PhonologicalSubrules>
    </PhonologicalRule>
  </PhonologicalRuleDefinitions>
</Language></HermitCrabInput>"#;

fn quantifier_probe_rule<'g>(
    g: &'g pg_grammar::model::Grammar,
    xml_id: &str,
) -> &'g pg_grammar::model::RewriteRuleDef {
    for pr in &g.prules {
        if let PhonRuleDef::Rewrite(r) = pr {
            if r.xml_id == xml_id {
                return r;
            }
        }
    }
    panic!("rule {xml_id:?} not found");
}

/// Positive witness: a genuinely unbounded (`max="-1"`), `min="0"` quantifier lowers to `Some(Slot::Repeat { min: 0, max: None, .. })`.
#[test]
fn unbounded_quantifier_min_zero_is_accepted_not_refused() {
    let g = load(QUANTIFIER_SCOPE_PROBE_XML);
    let table = &g.char_tables[0];
    let rule = quantifier_probe_rule(&g, "prUnboundedMinZero");
    let mut next_occurrence = 0usize;
    let slots = pattern_slots(
        &g,
        table,
        &rule.lhs,
        &mut next_occurrence,
        PatternLowerScope::Baseline,
    )
    .expect("a well-formed unbounded (min=0), alpha-free quantifier must now lower");
    assert_eq!(slots.len(), 1);
    match &slots[0] {
        Slot::Repeat { min, max, .. } => {
            assert_eq!(*min, 0);
            assert_eq!(*max, None);
        }
        _ => panic!("expected a Slot::Repeat"),
    }
}

/// Positive witness: an unbounded quantifier's large `min` still lowers to `Some(_)`.
#[test]
fn unbounded_quantifier_large_min_is_accepted() {
    let g = load(QUANTIFIER_SCOPE_PROBE_XML);
    let table = &g.char_tables[0];
    let rule = quantifier_probe_rule(&g, "prUnboundedLargeMin");
    let mut next_occurrence = 0usize;
    let slots = pattern_slots(
        &g,
        table,
        &rule.lhs,
        &mut next_occurrence,
        PatternLowerScope::Baseline,
    )
    .expect("an unbounded quantifier with min=1000 must lower");
    match &slots[0] {
        Slot::Repeat { min, max, .. } => {
            assert_eq!(*min, 1000);
            assert_eq!(*max, None);
        }
        _ => panic!("expected a Slot::Repeat"),
    }
}

/// Negative witness: an inverted finite bound (`min=5 > max=2`) has no sound finite construction and must stay refused.
#[test]
fn inverted_finite_quantifier_still_unsupported() {
    let g = load(QUANTIFIER_SCOPE_PROBE_XML);
    let table = &g.char_tables[0];
    let rule = quantifier_probe_rule(&g, "prInvertedFinite");
    let mut next_occurrence = 0usize;
    assert!(
        pattern_slots(
            &g,
            table,
            &rule.lhs,
            &mut next_occurrence,
            PatternLowerScope::Baseline
        )
        .is_none(),
        "min=5 > max=2 (both concrete) must stay refused"
    );
}

/// Negative witness: an `AlphaVariable` occurrence inside a quantifier's own children is out of scope regardless of whether the quantifier itself is bounded or unbounded.
#[test]
fn alpha_nested_unbounded_quantifier_still_unsupported() {
    let g = load(QUANTIFIER_SCOPE_PROBE_XML);
    let table = &g.char_tables[0];
    let rule = quantifier_probe_rule(&g, "prAlphaNestedUnbounded");
    let mut next_occurrence = 0usize;
    assert!(
        pattern_slots(
            &g,
            table,
            &rule.lhs,
            &mut next_occurrence,
            PatternLowerScope::Baseline
        )
        .is_none(),
        "an AlphaVariable occurrence inside a quantifier's own children is out of scope \
         regardless of whether the quantifier itself is bounded or unbounded"
    );
}

/// Load-bearing off-by-one, pinned at the compiled FST level, not just the rendered text: `min=2` must render `^>1` and accept exactly 2 (and 3+) occurrences while rejecting 1.
/// See `docs/research/pg-foma-lower-design-notes.md` for why `^>min` (rather than `^>(min-1)`) would be wrong.
#[test]
fn render_slots_unbounded_min_off_by_one_boundary() {
    let g = load(QUANTIFIER_SCOPE_PROBE_XML);
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let (cd, _) = table
        .iter()
        .next()
        .expect("QUANTIFIER_SCOPE_PROBE_XML's table must have exactly 1 segment");
    let tok = alphabet.token(cd).to_string();

    let slots = vec![Slot::Repeat {
        min: 2,
        max: None,
        children: vec![Slot::Fixed(cd)],
    }];
    let asg = AlphaAssignment {
        values: std::collections::HashMap::new(),
    };
    let text = render_slots(&alphabet, &slots, &asg);
    assert_eq!(
        text,
        format!("[{tok}]^>1"),
        "min=2 (\"2 or more\") must render as ^>1 (min-1), never ^>2 (min)"
    );

    let net = fsm_parse_regex(&opts, &text, None, None)
        .expect("rendered unbounded-quantifier text must compile");
    let one = tok.clone();
    let two = format!("{tok}{tok}");
    let three = format!("{tok}{tok}{tok}");

    let mut h = apply_init(&net);
    assert_eq!(
        apply_up(&mut h, Some(&one)),
        None,
        "1 occurrence (below min=2) must NOT match"
    );
    let mut h = apply_init(&net);
    assert_eq!(
        apply_up(&mut h, Some(&two)),
        Some(two.clone()),
        "exactly min=2 occurrences must match -- the off-by-one this test pins"
    );
    let mut h = apply_init(&net);
    assert_eq!(
        apply_up(&mut h, Some(&three)),
        Some(three.clone()),
        "MORE than min (3) must ALSO match -- genuinely unbounded, not a min..min+1 accident"
    );
}

/// `min == 0` renders as plain `*` (foma's native Kleene star), a distinct code path from the `min >= 1` `^>` case above.
#[test]
fn render_slots_unbounded_min_zero_is_kleene_star() {
    let g = load(QUANTIFIER_SCOPE_PROBE_XML);
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let (cd, _) = table
        .iter()
        .next()
        .expect("QUANTIFIER_SCOPE_PROBE_XML's table must have exactly 1 segment");
    let tok = alphabet.token(cd).to_string();

    let slots = vec![Slot::Repeat {
        min: 0,
        max: None,
        children: vec![Slot::Fixed(cd)],
    }];
    let asg = AlphaAssignment {
        values: std::collections::HashMap::new(),
    };
    let text = render_slots(&alphabet, &slots, &asg);
    assert_eq!(
        text,
        format!("[{tok}]*"),
        "min=0 or more must render as a plain Kleene star"
    );

    let net = fsm_parse_regex(&opts, &text, None, None)
        .expect("rendered unbounded-quantifier text must compile");
    let zero = String::new();
    let one = tok.clone();
    let two = format!("{tok}{tok}");

    let mut h = apply_init(&net);
    assert_eq!(
        apply_up(&mut h, Some(&zero)),
        Some(zero.clone()),
        "0 occurrences must match"
    );
    let mut h = apply_init(&net);
    assert_eq!(
        apply_up(&mut h, Some(&one)),
        Some(one.clone()),
        "1 occurrence must match"
    );
    let mut h = apply_init(&net);
    assert_eq!(
        apply_up(&mut h, Some(&two)),
        Some(two.clone()),
        "2 occurrences must match"
    );
}

/// Cross-table representation aliasing happens in render_slots's Fixed/Union arms, not class_members: an alphabet built with a table identity must render a shared atom as a bracketed union of both tables' tokens, while one without renders the same slot bare.
#[test]
fn render_slots_aliases_fixed_and_union_atoms_across_tables() {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>RenderSlotsAliasProbe</Name>
  <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t0"><Name>TableA</Name>
<SegmentDefinitions>
  <SegmentDefinition id="c0x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
</SegmentDefinitions>
  </CharacterDefinitionTable>
  <CharacterDefinitionTable id="t1"><Name>TableB</Name>
<SegmentDefinitions>
  <SegmentDefinition id="c1z"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
  <SegmentDefinition id="c1x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
  <SegmentDefinition id="c1y"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
</SegmentDefinitions>
  </CharacterDefinitionTable>
</Language></HermitCrabInput>"#;
    let g = load(XML);
    let table_a = &g.char_tables[0];
    let table_b = &g.char_tables[1];
    let cd_a_x = table_a.lookup_nfd("x").unwrap();
    let cd_b_x = table_b.lookup_nfd("x").unwrap();
    let cd_b_y = table_b.lookup_nfd("y").unwrap();
    assert_ne!(
        cd_a_x.0, cd_b_x.0,
        "the fixture's own misalignment must hold"
    );

    let alias_map = crate::replace::RepresentationAliasMap::build(&g);
    let aliased = SegAlphabet::with_table_id(table_b, pg_grammar::model::TableId(1), &alias_map);
    let bare = SegAlphabet::new(table_b);
    let asg = AlphaAssignment {
        values: std::collections::HashMap::new(),
    };

    // Slot::Fixed: the shared "x" atom aliases under `aliased`, stays bare under `bare`.
    let fixed_slots = vec![Slot::Fixed(cd_b_x)];
    let aliased_text = render_slots(&aliased, &fixed_slots, &asg);
    let bare_text = render_slots(&bare, &fixed_slots, &asg);
    assert_eq!(
        bare_text,
        bare.token(cd_b_x).to_string(),
        "unaliased rendering must stay exactly the single bare token"
    );
    assert_ne!(
        aliased_text, bare_text,
        "aliased rendering of a shared atom must differ from the unaliased rendering"
    );
    assert!(
        aliased_text.contains(&bare.token(cd_b_x).to_string())
            && aliased_text.contains(&SegAlphabet::new(table_a).token(cd_a_x).to_string()),
        "aliased rendering must contain BOTH tables' own tokens for the shared spelling: \
         {aliased_text:?}"
    );

    // Slot::Union: an unshared atom degenerates to the same bare rendering whether aliased or not, since aliasing only ever adds, never touches a spelling unique to its table.
    let union_slots = vec![Slot::Union(vec![cd_b_y])];
    assert_eq!(
        render_slots(&aliased, &union_slots, &asg),
        render_slots(&bare, &union_slots, &asg),
        "an unshared atom's rendering must be unaffected by aliasing"
    );
}
