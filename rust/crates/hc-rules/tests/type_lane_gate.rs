//! Regression gate for plan §13.1 Tier-1 #1 (the `Type` / boundary-lane fix): a literal
//! `<BoundaryMarker>` pattern node must match only boundary shape nodes, and a literal `<Segment>`
//! / natural-class pattern node must match only segment shape nodes — never the reverse.
//!
//! Before this fix, `PatternBridge::char_def_lanes` (`hc-rules/src/bridge.rs`) returned a boundary
//! char-def's `feature_lanes()` as-is, which was an **empty** `Vec` (`hc-grammar/src/chardef.rs`
//! never attached a `Type` lane, or any lane at all, to boundaries). `hc_fst`'s `flat_unifiable`
//! treats an absent lane as unconstrained, so a length-0 constraint vector canonicalizes to
//! "matches any segment" — the confirmed root cause of the `meN-`/`peN-`-prefix boundary-environment
//! bug (`rust-conversion.md` §13.1 Tier-1 #1). These tests drive the real `PatternBridge` →
//! `hc_fst::Transduce` path end-to-end (not just inspecting stored lane bits), because the bug's
//! symptom lives in how those bits get consumed, not just how they are stored — a test that only
//! checked `feature_lanes()[type_idx]` would pass even if some consumer still special-cased
//! boundaries as unconstrained.
//!
//! Two grammars are exercised, spanning the two width regimes this fix changes:
//! - [`zero_feat_grammar`] mirrors Sena exactly (no `<PhonologicalFeatureSystem>` at all — the
//!   `phon_features.len()` 0→1 case).
//! - [`feature_grammar`] mirrors Indonesian/Amharic (one real symbolic feature — the `len()` N→N+1
//!   case), and additionally pins that a `FeatureNaturalClass` keyed on a real feature still
//!   matches exactly the right segments post-fix (no regression on real phonological matching).

use hc_fst::{Segment, Transduce};
use hc_grammar::chardef::CharDefId;
use hc_grammar::model::{Grammar, NatClassId, Pattern, PatternNode, SimpleContext};
use hc_rules::bridge::PatternBridge;

fn char_def(g: &Grammar, xml_id: &str) -> CharDefId {
    g.char_tables[0]
        .iter()
        .find(|(_, cd)| cd.xml_id() == xml_id)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no char def {xml_id}"))
}

fn nat_class(g: &Grammar, xml_id: &str) -> NatClassId {
    let i = g
        .natural_classes
        .iter()
        .position(|nc| nc.xml_id == xml_id)
        .unwrap_or_else(|| panic!("no natural class {xml_id}"));
    NatClassId(i as u32)
}

/// The real, concrete lane row a shape node backed by `cd` would carry (mirrors
/// `hc_rules::shape_feat::lanes_for` / `ShapeBuilder::push_segment_with_lanes`): exactly the
/// char-def's own `feature_lanes()` — no padding needed post-fix, since every char def (segment
/// or boundary) is already `phon_features.len()`-wide.
fn concrete_lanes(g: &Grammar, cd: CharDefId) -> Vec<u64> {
    g.char_tables[0].get(cd).feature_lanes().to_vec()
}

fn matches_single(pattern: &Pattern, g: &Grammar, lanes: Vec<u64>) -> bool {
    let compiled = PatternBridge::new(g)
        .compile_pattern(pattern)
        .expect("pattern compiles");
    let fst = compiled.input.compile();
    Transduce::new(&fst, vec![Segment::new(lanes)])
        .anchored(true, true)
        .accepts()
}

// =================================================================================================
// (a)/(d) Zero-phonological-feature grammar (mirrors Sena: no <PhonologicalFeatureSystem>).
// =================================================================================================

const ZERO_FEAT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ZeroFeatTypeLane</Name>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="char_plus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="nc_any"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

fn zero_feat_grammar() -> Grammar {
    hc_grammar::load(ZERO_FEAT_XML).expect("zero-feat grammar loads")
}

#[test]
fn zero_feat_grammar_phon_features_len_is_one_not_zero() {
    // The headline width-0-to-1 assertion (plan §13.1 Tier-1 #1 blast-radius note): a grammar with
    // zero authored phonological features (Sena's real situation) now reports `len() == 1` (the
    // synthetic `Type` feature), not 0 — `is_empty()` is the new spelling of the old "no features"
    // check.
    let g = zero_feat_grammar();
    assert!(
        g.phon_features.is_empty(),
        "zero *authored* phonological features"
    );
    assert_eq!(
        g.phon_features.len(),
        1,
        "Type is always appended, even at zero authored features"
    );
}

#[test]
fn zero_feat_boundary_marker_pattern_does_not_match_a_segment() {
    let g = zero_feat_grammar();
    let boundary_pattern = Pattern {
        nodes: vec![PatternNode::CharDef(char_def(&g, "char_plus"))],
    };
    let seg_lanes = concrete_lanes(&g, char_def(&g, "char_a"));
    let bnd_lanes = concrete_lanes(&g, char_def(&g, "char_plus"));

    assert!(
        matches_single(&boundary_pattern, &g, bnd_lanes),
        "a boundary-marker pattern must still match its own boundary node"
    );
    assert!(
        !matches_single(&boundary_pattern, &g, seg_lanes),
        "a boundary-marker pattern must NOT match a real segment node (the confirmed root-cause bug: \
         an empty/unpadded boundary lane row canonicalizes to hc_fst's match-any constraint)"
    );
}

#[test]
fn zero_feat_segment_literal_pattern_does_not_match_a_boundary() {
    let g = zero_feat_grammar();
    let segment_pattern = Pattern {
        nodes: vec![PatternNode::CharDef(char_def(&g, "char_a"))],
    };
    let seg_lanes = concrete_lanes(&g, char_def(&g, "char_a"));
    let bnd_lanes = concrete_lanes(&g, char_def(&g, "char_plus"));

    assert!(
        matches_single(&segment_pattern, &g, seg_lanes),
        "a segment-literal pattern must match its own segment"
    );
    assert!(
        !matches_single(&segment_pattern, &g, bnd_lanes),
        "a segment-literal pattern must NOT match a boundary node"
    );
}

#[test]
fn zero_feat_feature_natural_class_matches_segment_not_boundary() {
    // `nc_any` is a `FeatureNaturalClass` with zero authored `<FeatureValue>`s -- before this fix it
    // matched literally everything (lane-unconstrained on every real feature, and no Type dimension
    // at all). Post-fix it must still match the segment (Type=Segment is injected automatically,
    // per architecture point 3) but reject the boundary.
    let g = zero_feat_grammar();
    let pattern = Pattern {
        nodes: vec![PatternNode::Context(SimpleContext {
            nat_class: nat_class(&g, "nc_any"),
            vars: vec![],
        })],
    };
    let seg_lanes = concrete_lanes(&g, char_def(&g, "char_a"));
    let bnd_lanes = concrete_lanes(&g, char_def(&g, "char_plus"));

    assert!(
        matches_single(&pattern, &g, seg_lanes),
        "an unconstrained FeatureNaturalClass must still match segments"
    );
    assert!(
        !matches_single(&pattern, &g, bnd_lanes),
        "an unconstrained FeatureNaturalClass must NOT match a boundary (implicit Type=Segment pin)"
    );
}

// =================================================================================================
// (b)/(c) Feature-bearing grammar (mirrors Indonesian/Amharic): one real symbolic feature, so the
// `Type` lane sits at FlatIndex(1), not FlatIndex(0), and real phonological feature matching must
// be unaffected by the fix.
// =================================================================================================

const FEATURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>FeatureTypeLane</Name>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_voi">
        <Name>voi</Name>
        <Symbols>
          <Symbol id="sym_vp">+</Symbol>
          <Symbol id="sym_vm">-</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_p"><Representations><Representation>p</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vm" />
        </SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="char_plus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="nc_voiced">
        <Name>Voiced</Name>
        <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
      </FeatureNaturalClass>
    </NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

fn feature_grammar() -> Grammar {
    hc_grammar::load(FEATURE_XML).expect("feature grammar loads")
}

#[test]
fn feature_grammar_phon_features_len_includes_type_appended_last() {
    let g = feature_grammar();
    assert!(
        !g.phon_features.is_empty(),
        "sanity: this grammar has one authored phonological feature"
    );
    assert_eq!(
        g.phon_features.len(),
        2,
        "1 authored feature + the always-appended Type feature"
    );
    assert_eq!(
        g.phon_features.type_flat(),
        hc_grammar::featsys::FlatIndex(1)
    );
}

#[test]
fn feature_grammar_boundary_marker_pattern_does_not_match_a_segment() {
    let g = feature_grammar();
    let boundary_pattern = Pattern {
        nodes: vec![PatternNode::CharDef(char_def(&g, "char_plus"))],
    };
    let seg_lanes = concrete_lanes(&g, char_def(&g, "char_b"));
    let bnd_lanes = concrete_lanes(&g, char_def(&g, "char_plus"));

    assert!(matches_single(&boundary_pattern, &g, bnd_lanes));
    assert!(
        !matches_single(&boundary_pattern, &g, seg_lanes),
        "a boundary-marker pattern must NOT match a real (voiced) segment even though the grammar \
         carries real phonological features"
    );
}

#[test]
fn feature_grammar_voiced_class_matches_only_voiced_segment_never_boundary() {
    // (c) No regression on real phonological feature matching: `nc_voiced` (feat_voi=+) must still
    // match the voiced segment `b`, must still reject the voiceless segment `p`, and — the new
    // assertion this fix adds — must reject the boundary too.
    let g = feature_grammar();
    let pattern = Pattern {
        nodes: vec![PatternNode::Context(SimpleContext {
            nat_class: nat_class(&g, "nc_voiced"),
            vars: vec![],
        })],
    };
    let voiced_lanes = concrete_lanes(&g, char_def(&g, "char_b"));
    let voiceless_lanes = concrete_lanes(&g, char_def(&g, "char_p"));
    let bnd_lanes = concrete_lanes(&g, char_def(&g, "char_plus"));

    assert!(
        matches_single(&pattern, &g, voiced_lanes),
        "must still match the voiced segment"
    );
    assert!(
        !matches_single(&pattern, &g, voiceless_lanes),
        "must still reject the voiceless segment"
    );
    assert!(
        !matches_single(&pattern, &g, bnd_lanes),
        "must reject the boundary (implicit Type=Segment pin)"
    );
}
