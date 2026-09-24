//! Tests closure-aware representation matching for feature-equivalent char defs.
use super::*;
use pg_grammar_model::chardef::CharDefId;
use pg_shape::ShapeBuilder;

const FEATURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SurfaceP5</Name>
    <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voi">
        <Name>voi</Name>
        <Symbols><Symbol id="sym_vp">+</Symbol><Symbol id="sym_vm">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_x"><Representations><Representation>x</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_y"><Representations><Representation>y</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_z"><Representations><Representation>z</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vm" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;

const ZERO_FEAT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SurfaceP5Zero</Name>
    <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_y"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
  </Language>
</HermitCrabInput>
"#;

fn find_cd(table: &CharDefTable, xml_id: &str) -> CharDefId {
    table
        .iter()
        .find(|(_, cd)| cd.xml_id() == xml_id)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no char def {xml_id}"))
}

/// A single concrete `Segment` node whose lanes are exactly `cd`'s own, mimicking real segmentation which stamps a node's lanes from its char-def.
fn one_segment_shape(table: &CharDefTable, cd: CharDefId) -> Shape {
    let lanes = table.get(cd).feature_lanes().to_vec();
    let mut b = ShapeBuilder::with_features(lanes.len() as u32);
    b.push_segment_with_lanes(cd.0, &lanes);
    b.finish()
}

#[test]
fn closure_sibling_renders_in_to_regex_display_table_order() {
    let g = pg_grammar::load(FEATURE_XML).expect("grammar loads");
    let table = &g.char_tables[0];
    let shape = one_segment_shape(table, find_cd(table, "char_x"));
    // x and y are the P5 closure-sibling pair; z (voi-) is excluded; document order is x,y,z.
    assert_eq!(to_regex_display(table, &shape), "[xy]");
}

#[test]
fn closure_sibling_first_match_wins_to_plain_string() {
    let g = pg_grammar::load(FEATURE_XML).expect("grammar loads");
    let table = &g.char_tables[0];
    let shape = one_segment_shape(table, find_cd(table, "char_x"));
    // `to_plain_string` takes only the FIRST matching rep (table order): x's own spelling.
    assert_eq!(to_plain_string(table, &shape, false), "x");
}

#[test]
fn closure_sibling_spelling_is_accepted_by_is_match() {
    let g = pg_grammar::load(FEATURE_XML).expect("grammar loads");
    let table = &g.char_tables[0];
    let shape = one_segment_shape(table, find_cd(table, "char_x"));
    // The node is concretely "x", but "y" (its closure sibling) must also confirm: C#'s IsMatch is pure FeatureStruct unification for a feature-bearing table, no separate StrRep gate.
    assert!(
        is_match(table, &shape, "y"),
        "closure-sibling spelling must confirm"
    );
    assert!(
        is_match(table, &shape, "x"),
        "own spelling must still confirm"
    );
    assert!(
        !is_match(table, &shape, "z"),
        "non-unifying (voi-) spelling must still reject"
    );
}

#[test]
fn zero_feature_table_is_unaffected_by_closure_gate() {
    let g = pg_grammar::load(ZERO_FEAT_XML).expect("grammar loads");
    let table = &g.char_tables[0];
    assert!(
        g.phon_features.is_empty(),
        "sanity: zero authored phon features (Sena regime)"
    );
    let shape = one_segment_shape(table, find_cd(table, "char_x"));
    // No closure exists here, so rendering/matching stay identity-only: "y" would trivially lane-unify with "x" (both zero authored lanes) but must NOT confirm.
    assert_eq!(to_regex_display(table, &shape), "x");
    assert_eq!(to_plain_string(table, &shape, false), "x");
    assert!(is_match(table, &shape, "x"));
    assert!(
        !is_match(table, &shape, "y"),
        "zero-feature table must stay identity-gated (no closure)"
    );
}
