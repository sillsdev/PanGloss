//! Regression gate for the char-def-set fix: a class insertion must render/match its real members.
//! See `docs/research/pg-parse-cd-set-gate-notes.md`.

use pg_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, Grammar, MorphRuleDef, MorphemeId, MprSet,
    NatClassId, OutputAction, PartRef, Pattern, PatternNode, ReduplicationHint, SimpleContext,
    StratumId, VarTable,
};
use pg_rules::morph::synthesize;
use pg_rules::word::MorphRecord;
use pg_rules::Word;

// Shared hand-built-grammar plumbing, reproduced here since integration-test `tests/common` modules are not shared across crates.

fn nat_class(g: &Grammar, xml_id: &str) -> NatClassId {
    let i = g
        .natural_classes
        .iter()
        .position(|nc| nc.xml_id == xml_id)
        .unwrap_or_else(|| panic!("no natural class {xml_id}"));
    NatClassId(i as u32)
}

fn ctx(nc: NatClassId) -> SimpleContext {
    SimpleContext {
        nat_class: nc,
        vars: vec![],
    }
}

fn any_pattern(g: &Grammar) -> Pattern {
    Pattern {
        nodes: vec![PatternNode::Quantifier {
            min: 1,
            max: None,
            children: vec![PatternNode::Context(ctx(nat_class(g, "nc_any")))],
        }],
    }
}

fn allomorph(id: u32, lhs: Vec<Pattern>, rhs: Vec<OutputAction>) -> AffixAllomorphDef {
    AffixAllomorphDef {
        id: AllomorphId(id),
        environments: vec![],
        co_occurrence: vec![],
        required_syn_fs: pg_featstruct::FsId(0),
        vars: VarTable::default(),
        required_mpr: MprSet::EMPTY,
        excluded_mpr: MprSet::EMPTY,
        out_mpr: MprSet::EMPTY,
        redup_hint: ReduplicationHint::Prefix,
        lhs,
        rhs,
        properties: vec![],
    }
}

fn prefix_rule(morpheme: u32, insert_nc: &str, g: &Grammar) -> MorphRuleDef {
    MorphRuleDef::AffixProcess(AffixProcessRuleDef {
        morpheme: MorphemeId(morpheme),
        name: None,
        blockable: false,
        partial: false,
        max_apps: 1,
        required_syn_fs: pg_featstruct::FsId(0),
        out_syn_fs: pg_featstruct::FsId(0),
        obligatory_features: vec![],
        required_stem_name: None,
        is_template_rule: false,
        allomorphs: vec![allomorph(
            morpheme,
            vec![any_pattern(g)],
            vec![
                OutputAction::InsertContext(ctx(nat_class(g, insert_nc))),
                OutputAction::Copy(PartRef::Input(0)),
            ],
        )],
    })
}

/// Builds the root's shape the way production code actually does, via `segment_with_features`.
/// See `docs/research/pg-parse-cd-set-gate-notes.md`.
fn root_word(g: &Grammar, text: &str) -> Word {
    let shape = pg_rules::shape_feat::segment_with_features(g, &g.char_tables[0], text)
        .expect("root segments");
    let mut w = Word::new(shape, StratumId(0));
    w.morphs
        .push(MorphRecord::new(AllomorphId(100), MorphemeId(100), 0));
    w
}

/// Synthesize `rule` onto root `text`, returning the single output's rendered display signature.
fn synth_display(g: &Grammar, text: &str, rule: &MorphRuleDef) -> String {
    let word = root_word(g, text);
    let out = synthesize(g, &word, rule);
    assert_eq!(out.len(), 1, "expected exactly one synthesis result");
    pg_parse::surface::to_regex_display(&g.char_tables[0], &out[0].shape)
}

// (a) Zero-phonological-feature grammar (mirrors Sena exactly: no <PhonologicalFeatureSystem>).

const ZERO_FEAT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ZeroFeat</Name>
    <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_m"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_n"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_k"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_s"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_t"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_l"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_i"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="nc_any"><Name>Any</Name></FeatureNaturalClass>
      <SegmentNaturalClass id="nc_nasal">
        <Name>Nasal</Name>
        <Segment segment="char_m" />
        <Segment segment="char_n" />
      </SegmentNaturalClass>
    </NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

fn load_zero_feat_grammar() -> Grammar {
    pg_grammar::load(ZERO_FEAT_XML).expect("zero-feat grammar loads")
}

/// Mirrors Sena's actual "mbali" situation directly.
/// See `docs/research/pg-parse-cd-set-gate-notes.md`.
#[test]
fn zero_feat_segments_class_renders_only_its_members() {
    let g = load_zero_feat_grammar();
    // `len()` is never 0 post-fix (the always-appended synthetic `Type` feature); `is_empty()` is the correct check.
    assert!(
        g.phon_features.is_empty(),
        "sanity: this grammar has zero authored phonological features"
    );
    let rule = prefix_rule(200, "nc_nasal", &g);

    let sig = synth_display(&g, "bali", &rule);
    assert_eq!(sig, "[mn]bali");
}

// (b) Feature-bearing grammar (mirrors Indonesian/Amharic).
// See `docs/research/pg-parse-cd-set-gate-notes.md`.

const FEATURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Feat</Name>
    <PartsOfSpeech><PartOfSpeech id="p"><Name>P</Name></PartOfSpeech></PartsOfSpeech>
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
        <SegmentDefinition id="char_d"><Representations><Representation>d</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_g"><Representations><Representation>g</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_p"><Representations><Representation>p</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vm" />
        </SegmentDefinition>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="nc_any"><Name>Any</Name></FeatureNaturalClass>
      <FeatureNaturalClass id="nc_voiced">
        <Name>Voiced</Name>
        <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
      </FeatureNaturalClass>
      <SegmentNaturalClass id="nc_bd">
        <Name>BD</Name>
        <Segment segment="char_b" />
        <Segment segment="char_d" />
      </SegmentNaturalClass>
    </NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

fn load_feature_grammar() -> Grammar {
    pg_grammar::load(FEATURE_XML).expect("feature grammar loads")
}

/// `nc_bd` is `Segments`-kind with exactly `{b, d}`, but `g` and `a` share the same voice lane; the fix must render exactly `[bd]p`, not `[bdga]p`.
/// See `docs/research/pg-parse-cd-set-gate-notes.md`.
#[test]
fn feature_grammar_segments_class_narrows_tighter_than_lane_union() {
    let g = load_feature_grammar();
    assert!(
        !g.phon_features.is_empty(),
        "sanity: this grammar has phonological features"
    );
    let rule = prefix_rule(200, "nc_bd", &g);

    let sig = synth_display(&g, "p", &rule);
    assert_eq!(
        sig, "[bd]p",
        "Segments-kind class must narrow to its exact explicit members"
    );
}

/// `nc_voiced` is `Feature`-kind, matching b/d/g/a but excluding p; must render unchanged as `[bdga]`.
/// See `docs/research/pg-parse-cd-set-gate-notes.md`.
#[test]
fn feature_grammar_feature_class_behavior_is_unchanged() {
    let g = load_feature_grammar();
    let rule = prefix_rule(200, "nc_voiced", &g);

    let sig = synth_display(&g, "p", &rule);
    assert_eq!(
        sig, "[bdga]p",
        "Feature-kind class rendering must stay lane-unification-derived"
    );
}
