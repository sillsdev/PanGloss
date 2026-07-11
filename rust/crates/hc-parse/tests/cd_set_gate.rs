//! Regression gate for plan §13.1 Tier-1 #3 (the char-def-set / `StrRep`-analog fix): an
//! `InsertSimpleContext` (`OutputAction::InsertContext`) natural-class insertion must render/match
//! as exactly the class's real members, never the whole char-def table.
//!
//! Two hand-built grammars, spanning `hc-rules` (synthesis) and `hc-parse` (surface rendering):
//! - [`zero_feat_segments_class_renders_only_its_members`] mirrors Sena's actual situation: a
//!   grammar with **zero phonological features**, where the pre-fix lane-only representation was
//!   *no constraint at all* (every char-def's lanes are `&[]`, so `flat_unifiable(&[],&[])` is
//!   vacuously true for the entire table) — the confirmed mechanism behind the Sena "mbali"
//!   full-inventory-bracket bug (`rust-conversion.md` §13.1 Tier-1 #3, `parity-out/audit/
//!   C-loader-pipeline.md` Detail #1).
//! - [`feature_grammar_segments_class_narrows_tighter_than_lane_union`] and
//!   [`feature_grammar_feature_class_behavior_is_unchanged`] mirror Indonesian/Amharic: a
//!   phonological-feature-bearing table where a `Segments`-kind class must narrow to *exactly* its
//!   explicit members (tighter than the old lane-union over-approximation, which would have also
//!   admitted a same-lane non-member), while a `Feature`-kind class's lane-unification rendering is
//!   preserved unchanged (its char-def set is derived *from* the lanes, not an independent
//!   constraint).

use hc_grammar::model::{
    AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, Grammar, MorphRuleDef, MorphemeId,
    MprSet, NatClassId, OutputAction, PartRef, Pattern, PatternNode, ReduplicationHint,
    SimpleContext, StratumId, VarTable,
};
use hc_rules::morph::synthesize;
use hc_rules::word::MorphRecord;
use hc_rules::Word;

// =================================================================================================
// Shared hand-built-grammar plumbing (the `common::load_alpha_grammar()` pattern used across
// `hc-rules/tests/*.rs`, reproduced here since integration-test `tests/common` modules are not
// shared across crates).
// =================================================================================================

fn nat_class(g: &Grammar, xml_id: &str) -> NatClassId {
    let i = g
        .natural_classes
        .iter()
        .position(|nc| nc.xml_id == xml_id)
        .unwrap_or_else(|| panic!("no natural class {xml_id}"));
    NatClassId(i as u32)
}

fn ctx(nc: NatClassId) -> SimpleContext {
    SimpleContext { nat_class: nc, vars: vec![] }
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
        required_syn_fs: hc_featstruct::FsId(0),
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
        required_syn_fs: hc_featstruct::FsId(0),
        out_syn_fs: hc_featstruct::FsId(0),
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

/// Builds the root's shape the way production code actually does (`Morpher::set_root_allomorph`,
/// `hc-parse/src/morpher.rs:226-241`) — via `segment_with_features`, which attaches each node's
/// real per-char-def phonological lanes, not the bare feature-less `hc_grammar::segment::segment`.
/// This matters for the feature-bearing test below: with unfilled (unconstrained) lanes the root's
/// *own* concrete segment would misrender regardless of this milestone's fix, which would test the
/// wrong thing. At `feat_width == 0` (the zero-feature test) the two are identical by construction
/// (`segment_with_features`'s own doc comment) — the char-def-set fix is the *only* discriminator
/// available there, which is exactly the case this milestone addresses.
fn root_word(g: &Grammar, text: &str) -> Word {
    let shape = hc_rules::shape_feat::segment_with_features(g, &g.char_tables[0], text)
        .expect("root segments");
    let mut w = Word::new(shape, StratumId(0));
    w.morphs.push(MorphRecord::new(AllomorphId(100), MorphemeId(100), 0));
    w
}

/// Synthesize `rule` onto root `text`, returning the single output's rendered display signature.
fn synth_display(g: &Grammar, text: &str, rule: &MorphRuleDef) -> String {
    let word = root_word(g, text);
    let out = synthesize(g, &word, rule);
    assert_eq!(out.len(), 1, "expected exactly one synthesis result");
    hc_parse::surface::to_regex_display(&g.char_tables[0], &out[0].shape)
}

// =================================================================================================
// (a) Zero-phonological-feature grammar (mirrors Sena exactly: no <PhonologicalFeatureSystem>).
// =================================================================================================

const ZERO_FEAT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ZeroFeat</Name>
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
    hc_grammar::load(ZERO_FEAT_XML).expect("zero-feat grammar loads")
}

/// Mirrors Sena's actual "mbali" situation directly: a zero-phonological-feature grammar (so the
/// pre-fix lane-only check was vacuously true for every table entry, `hc-rules/src/morph.rs`'s
/// `InsertSimpleContext` handling — see that module and `hc_parse::surface::matching_str_reps`).
/// Root "bali" + a prefix that inserts the 2-member `Segments`-kind `nc_nasal` class must render as
/// `[mn]bali` — exactly the class's members, not the whole 9-char-def table (`[mn]` vs
/// `[abiklmnst]`-style full inventory).
#[test]
fn zero_feat_segments_class_renders_only_its_members() {
    let g = load_zero_feat_grammar();
    // Plan §13.1 Tier-1 #1: `len()` is never 0 post-fix (the always-appended synthetic `Type`
    // feature) — `is_empty()` is the correct "zero *authored* phonological features" check now.
    assert!(g.phon_features.is_empty(), "sanity: this grammar has zero authored phonological features");
    let rule = prefix_rule(200, "nc_nasal", &g);

    let sig = synth_display(&g, "bali", &rule);
    assert_eq!(sig, "[mn]bali");
}

// =================================================================================================
// (b) Feature-bearing grammar (mirrors Indonesian/Amharic): one symbolic feature (voice), five
// segments -- b/d/g voiced, p voiceless, a voiced vowel -- so a Segments-kind class of {b,d} shares
// its lane-union with g and a (also voiced) without being identical to the whole voiced set.
// =================================================================================================

const FEATURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Feat</Name>
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
    hc_grammar::load(FEATURE_XML).expect("feature grammar loads")
}

/// `nc_bd` is `Segments`-kind with exactly `{b, d}`, but `g` and `a` share the *same* voice lane
/// (`voi+`) as `b`/`d`. The pre-fix lane-union representation would have admitted `g` and `a` too
/// (over-approximation flagged in `parity-out/audit/C-loader-pipeline.md` row 1 as "narrower but
/// still real" on feature-bearing grammars); the fix must render exactly `[bd]p`, not `[bdga]p`.
///
/// Root text is `"p"`, not `"a"` (P5, `docs/p5-crosstable-featurestruct-design.md`): `p` is the
/// table's only `voi-` segment, so it is FeatureStruct-unique and its own rendering stays a plain
/// `"p"`. Rooting on `"a"` would (correctly, post-P5) also render the root's OWN segment as
/// `[bdga]` -- confirmed against the C# oracle (`CharacterDefinitionTable.cs:125`,
/// `new ShapeNode(cd.FeatureStruct.Clone())`: a feature-bearing char-def's segmented node carries
/// no `StrRep` at all, so `GetMatchingStrReps` genuinely unifies `a` against `b`/`d`/`g` too, since
/// this minimal fixture gives all four an identical `Type+voi+` FeatureStruct) -- that's the P5
/// fix working as designed, not a bug, but it would conflate the assertion below (about the
/// INSERTED class node) with an unrelated (also-correct) change to the root node's own rendering.
/// `p` isolates the assertion to just the inserted class's narrowing, this test's actual intent.
#[test]
fn feature_grammar_segments_class_narrows_tighter_than_lane_union() {
    let g = load_feature_grammar();
    assert!(!g.phon_features.is_empty(), "sanity: this grammar has phonological features");
    let rule = prefix_rule(200, "nc_bd", &g);

    let sig = synth_display(&g, "p", &rule);
    assert_eq!(sig, "[bd]p", "Segments-kind class must narrow to its exact explicit members");
}

/// `nc_voiced` is `Feature`-kind (`voi+`), matching `b`, `d`, `g`, `a` but excluding `p` (voiceless)
/// -- this must render as the full lane-unifying set, `[bdga]`, **unchanged** by the fix (a
/// Feature-kind class's char-def set is derived *from* the lanes, not an independent restriction).
///
/// Root text is `"p"` for the same P5 reason as the sibling test above (see its doc comment): `p`
/// is FeatureStruct-unique in this table, so its own rendering is unaffected by the P5 closure
/// fallback, isolating this assertion to the inserted class's (unchanged) lane-union rendering.
#[test]
fn feature_grammar_feature_class_behavior_is_unchanged() {
    let g = load_feature_grammar();
    let rule = prefix_rule(200, "nc_voiced", &g);

    let sig = synth_display(&g, "p", &rule);
    assert_eq!(sig, "[bdga]p", "Feature-kind class rendering must stay lane-unification-derived");
}
