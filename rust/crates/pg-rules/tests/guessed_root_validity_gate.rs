//! Unit tests for `allomorphs_valid_impl`'s sentinel-delegation branch for a guessed root, built by hand against a real grammar since nothing in the matcher/wire-up yet produces such a `Word`.

use pg_grammar::model::{AllomorphId, Grammar, LexEntryId, MorphemeId};
use pg_rules::validity::allomorphs_valid;
use pg_rules::word::{GuessedRoot, MorphRecord};
use pg_rules::Word;
use pg_shape::{NodeKind, ShapeBuilder};

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>GuessedRootValidity</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <HeadFeatures>
      <SymbolicFeature id="featPers"><Name>pers</Name>
        <Symbols><Symbol id="symP1">p1</Symbol><Symbol id="symP2">p2</Symbol></Symbols>
      </SymbolicFeature>
    </HeadFeatures>
    <StemNames>
      <StemName id="sn1" partsOfSpeech="posV">
        <Name>sn1</Name>
        <Regions>
          <Region><AssignedHeadFeatures><FeatureValue feature="featPers" symbolValues="symP1" /></AssignedHeadFeatures></Region>
        </Regions>
      </StemName>
    </StemNames>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
      <SegmentNaturalClass id="ncX"><Name>XClass</Name><Segment segment="cX" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="ePattern" partOfSpeech="posV">
            <Allomorphs>
              <Allomorph id="aPattern" isBound="true"><PhoneticShape>[Any]*</PhoneticShape>
                <RequiredEnvironments>
                  <Environment>
                    <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncX" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
                  </Environment>
                </RequiredEnvironments>
              </Allomorph>
              <Allomorph id="aSibling" stemName="sn1"><PhoneticShape>zzzz</PhoneticShape></Allomorph>
            </Allomorphs>
            <MorphemeId>PATTERN</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eOther" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aOther"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>OTHER</MorphemeId>
          </LexicalEntry>
          <LexicalEntry id="eExcl" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aExcl"><PhoneticShape>x</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>EXCL</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
    <AllomorphCoOccurrenceRules>
      <AllomorphCoOccurrenceRule type="exclude" primaryAllomorph="aPattern" otherAllomorphs="aExcl" adjacency="anywhere" />
    </AllomorphCoOccurrenceRules>
    <MorphemeCoOccurrenceRules>
      <MorphemeCoOccurrenceRule type="exclude" primaryMorpheme="ePattern" otherMorphemes="eExcl" adjacency="anywhere" />
    </MorphemeCoOccurrenceRules>
  </Language>
</HermitCrabInput>
"#;

fn load_gate_grammar() -> Grammar {
    pg_grammar::load(XML).expect("guessed-root validity-gate grammar loads")
}

fn find_entry<'g>(
    g: &'g Grammar,
    xml_id_allomorph_text: &str,
) -> &'g pg_grammar::model::LexEntryDef {
    g.entries
        .iter()
        .find(|e| {
            e.allomorphs
                .iter()
                .any(|a| a.shape.text == xml_id_allomorph_text)
        })
        .unwrap_or_else(|| panic!("no entry with an allomorph surface {xml_id_allomorph_text:?}"))
}

fn find_entry_id(g: &Grammar, allomorph_text: &str) -> LexEntryId {
    let idx = g
        .entries
        .iter()
        .position(|e| e.allomorphs.iter().any(|a| a.shape.text == allomorph_text))
        .unwrap_or_else(|| panic!("no entry with an allomorph surface {allomorph_text:?}"));
    LexEntryId(idx as u32)
}

/// Builds a feature-less shape (root allomorphs are stored feature-less), matching `validity_gate.rs`'s `entry_shape` helper.
fn shape_of(g: &Grammar, text: &str) -> pg_shape::Shape {
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

/// A one-morph guessed word: shape `text`, the sole morph record carrying the GUESSED sentinels, `guessed_root` pointing at `aPattern`/`ePattern`.
fn guessed_word(g: &Grammar, text: &str) -> Word {
    let pattern_entry = find_entry_id(g, "[Any]*");
    let pattern_allo = find_entry(g, "[Any]*").allomorphs[0].id;
    let mut w = Word::new(shape_of(g, text), pg_grammar::model::StratumId(0));
    let runtime = pg_rules::word::RuntimeRoot::Guessed(GuessedRoot {
        pattern_allo,
        pattern_entry,
        text: text.to_string(),
    });
    w.root_runtime_id = Some(text.to_string());
    w.morphs = vec![
        MorphRecord::new(AllomorphId::GUESSED, MorphemeId::GUESSED, 0).with_runtime_root(runtime),
    ];
    w
}

/// A guessed word with a second, real morph appended right after it: pushes `distinct_count` to 2, isolating checks past the bound-root-alone gate, and gives the guessed span a definite right edge.
fn guessed_word_plus(
    g: &Grammar,
    text: &str,
    second_allo: AllomorphId,
    second_morpheme: MorphemeId,
) -> Word {
    let mut w = guessed_word(g, text);
    w.morphs
        .push(MorphRecord::new(second_allo, second_morpheme, 1));
    w
}

fn other_allo_and_morpheme(g: &Grammar) -> (AllomorphId, MorphemeId) {
    let e = find_entry(g, "b");
    (e.allomorphs[0].id, e.morpheme)
}

fn excl_allo_and_morpheme(g: &Grammar) -> (AllomorphId, MorphemeId) {
    let e = find_entry(g, "x");
    (e.allomorphs[0].id, e.morpheme)
}

// Bound-root gate: copied verbatim from the pattern allomorph's `is_bound`.

#[test]
fn guessed_root_alone_is_rejected_by_the_bound_gate() {
    let g = load_gate_grammar();
    assert!(
        find_entry(&g, "[Any]*").allomorphs[0].is_bound,
        "sanity: aPattern is isBound=\"true\""
    );
    // Two-segment shape "za" so the RightEnvironment finds something (though not ncX, so it would also reject) -- the bound gate fires first in the checked order either way.
    let w = guessed_word(&g, "za");
    assert!(
        !allomorphs_valid(&g, &w),
        "a bound guessed root alone (distinct_count=1) must be rejected"
    );
}

#[test]
fn guessed_root_with_a_second_distinct_allomorph_is_not_rejected_by_the_bound_gate() {
    let g = load_gate_grammar();
    let (other_allo, other_morpheme) = other_allo_and_morpheme(&g);
    // "distinct_count" cares only about allomorph identity, not span content; "ax" puts real material after the guessed span, isolating the bound-gate result from the environments gate.
    let w = guessed_word_plus(&g, "ax", other_allo, other_morpheme);
    assert!(
        allomorphs_valid(&g, &w),
        "bound guessed root + a second distinct allomorph must not be rejected"
    );
}

// Stem-name gate: the PRIMARY clause delegates; the sibling-exclusion loop must not run.

#[test]
fn guessed_root_stem_name_sibling_exclusion_is_a_no_op() {
    let g = load_gate_grammar();
    let (other_allo, other_morpheme) = other_allo_and_morpheme(&g);
    let mut w = guessed_word_plus(&g, "ax", other_allo, other_morpheme);
    // aPattern carries no stem name itself; only its real sibling aSibling does (sn1, pers=p1). Setting syn_fs to pers=p1 would fail if the exclusion loop incorrectly ran against real siblings.
    w.syn_fs = pers_p1_fs(&g);
    assert!(
        allomorphs_valid(&g, &w),
        "the pattern's REAL sibling's stem name must NOT be checked against a guessed root \
         (the fabricated entry has exactly one allomorph -- itself)"
    );
}

fn pers_p1_fs(g: &Grammar) -> pg_featstruct::FeatureStruct {
    // No fixture entry carries pers=p1 to unify against, so this reads the {pers=p1} FS straight off sn1's own StemName region instead.
    let sn1 = g
        .stem_names
        .iter()
        .find(|s| s.name.as_deref() == Some("sn1"))
        .expect("sn1 exists");
    g.fs_interner.get(sn1.regions[0]).clone()
}

// Allomorph co-occurrence: keyed on the GUESSED sentinel id as primary.

#[test]
fn guessed_root_allomorph_co_occurrence_exclude_rejects_when_the_excluded_allomorph_co_occurs() {
    let g = load_gate_grammar();
    let (excl_allo, excl_morpheme) = excl_allo_and_morpheme(&g);
    // "ax" also satisfies the RightEnvironment, isolating this test to the co-occurrence gate alone.
    let w = guessed_word_plus(&g, "ax", excl_allo, excl_morpheme);
    assert!(
        !allomorphs_valid(&g, &w),
        "the pattern's own AllomorphCoOccurrenceRule must reject co-occurrence with aExcl"
    );
}

#[test]
fn guessed_root_allomorph_co_occurrence_exclude_passes_when_the_excluded_allomorph_is_absent() {
    let g = load_gate_grammar();
    let (other_allo, other_morpheme) = other_allo_and_morpheme(&g);
    let w = guessed_word_plus(&g, "ax", other_allo, other_morpheme);
    assert!(
        allomorphs_valid(&g, &w),
        "no violation of the AllomorphCoOccurrenceRule when aExcl never occurs"
    );
}

// Morpheme co-occurrence: keyed on the GUESSED sentinel id as primary.

#[test]
fn guessed_root_morpheme_co_occurrence_exclude_rejects_when_the_excluded_morpheme_co_occurs() {
    let g = load_gate_grammar();
    let (excl_allo, excl_morpheme) = excl_allo_and_morpheme(&g);
    let w = guessed_word_plus(&g, "ax", excl_allo, excl_morpheme);
    assert!(
        !allomorphs_valid(&g, &w),
        "the pattern's own MorphemeCoOccurrenceRule must reject co-occurrence with eExcl's morpheme"
    );
}

#[test]
fn guessed_root_morpheme_co_occurrence_exclude_passes_when_the_excluded_morpheme_is_absent() {
    let g = load_gate_grammar();
    let (other_allo, other_morpheme) = other_allo_and_morpheme(&g);
    let w = guessed_word_plus(&g, "ax", other_allo, other_morpheme);
    assert!(
        allomorphs_valid(&g, &w),
        "no violation of the MorphemeCoOccurrenceRule when eExcl's morpheme never occurs"
    );
}

// Environments: delegated to the pattern allomorph's own `environments` field.

/// The positive direction is already exercised by `guessed_root_with_a_second_distinct_allomorph_is_not_rejected_by_the_bound_gate`; this isolates the negative one directly.
#[test]
fn guessed_root_environments_delegate_to_the_pattern_allomorphs_own_environments() {
    let g = load_gate_grammar();
    let (other_allo, other_morpheme) = other_allo_and_morpheme(&g);
    assert!(
        !find_entry(&g, "[Any]*").allomorphs[0]
            .environments
            .is_empty(),
        "sanity: aPattern declares a RequiredEnvironments block"
    );
    let w = guessed_word_plus(&g, "ab", other_allo, other_morpheme);
    assert!(
        !allomorphs_valid(&g, &w),
        "\"a\" followed by \"b\" (not in ncX) must fail the pattern's RequiredEnvironments, \
         confirming environments delegate to the pattern allomorph's own field"
    );
}
