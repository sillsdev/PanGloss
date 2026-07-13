//! Shared test support: a tiny hand-authored HermitCrab grammar (loaded through the real
//! `hc_grammar::load`, since `PhonFeatureSystem`/`CharDefTable` constructors are `pub(crate)`).
//!
//! Feature system (declaration order fixes `FlatIndex`):
//! - `FlatIndex(0)` = **cons** (consonantal): `sym_cp` (+, bit 0), `sym_cm` (−, bit 1)
//! - `FlatIndex(1)` = **voi** (voice): `sym_vp` (+, bit 0), `sym_vm` (−, bit 1)
//!
//! Segments (lanes are `[cons, voi]`):
//! - `a` = `[0b10, 0b01]`  (cons−, voi+ : a voiced vowel)
//! - `t` = `[0b01, 0b10]`  (cons+, voi− : a voiceless stop)
//! - `d` = `[0b01, 0b01]`  (cons+, voi+ : a voiced stop)
//! - `n` = `[0b01, 0b01]`  (cons+, voi+ : shares lanes with d; distinct char-def id) — for
//!   epenthesis targets we want a segment to insert.
//!
//! One boundary `+`.
//!
//! Natural classes:
//! - `nc_vowel` (Feature): cons− → matches `a`
//! - `nc_cons`  (Feature): cons+ → matches `t`, `d`, `n`
//! - `nc_voi`   (Feature): voi+  → the feature-change RHS "[+voice]"
//! - `nc_t`     (Segment): {t}
//! - `nc_d`     (Segment): {d}
//! - `nc_n`     (Segment): {n}

#![allow(dead_code)]

use hc_grammar::model::Grammar;

pub const GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Probe</Name>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_cons">
        <Name>cons</Name>
        <Symbols>
          <Symbol id="sym_cp">+</Symbol>
          <Symbol id="sym_cm">-</Symbol>
        </Symbols>
      </SymbolicFeature>
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
        <SegmentDefinition id="char_a">
          <Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_cons" symbolValues="sym_cm" />
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_t">
          <Representations><Representation>t</Representation></Representations>
          <FeatureValue feature="feat_cons" symbolValues="sym_cp" />
          <FeatureValue feature="feat_voi" symbolValues="sym_vm" />
        </SegmentDefinition>
        <SegmentDefinition id="char_d">
          <Representations><Representation>d</Representation></Representations>
          <FeatureValue feature="feat_cons" symbolValues="sym_cp" />
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_n">
          <Representations><Representation>n</Representation></Representations>
          <FeatureValue feature="feat_cons" symbolValues="sym_cp" />
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="char_bnd">
          <Representations><Representation>+</Representation></Representations>
        </BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="nc_vowel">
        <Name>V</Name>
        <FeatureValue feature="feat_cons" symbolValues="sym_cm" />
      </FeatureNaturalClass>
      <FeatureNaturalClass id="nc_cons">
        <Name>C</Name>
        <FeatureValue feature="feat_cons" symbolValues="sym_cp" />
      </FeatureNaturalClass>
      <FeatureNaturalClass id="nc_voi">
        <Name>Voiced</Name>
        <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
      </FeatureNaturalClass>
      <SegmentNaturalClass id="nc_t">
        <Name>t</Name>
        <Segment segment="char_t" />
      </SegmentNaturalClass>
      <SegmentNaturalClass id="nc_d">
        <Name>d</Name>
        <Segment segment="char_d" />
      </SegmentNaturalClass>
      <SegmentNaturalClass id="nc_n">
        <Name>n</Name>
        <Segment segment="char_n" />
      </SegmentNaturalClass>
    </NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

pub fn load_probe_grammar() -> Grammar {
    hc_grammar::load(GRAMMAR_XML).expect("probe grammar loads")
}

/// The single char-def table of the probe grammar.
pub fn table(g: &Grammar) -> &hc_grammar::chardef::CharDefTable {
    &g.char_tables[0]
}

/// Resolve a natural class by its XML id.
pub fn nat_class(g: &Grammar, xml_id: &str) -> hc_grammar::model::NatClassId {
    let i = g
        .natural_classes
        .iter()
        .position(|nc| nc.xml_id == xml_id)
        .unwrap_or_else(|| panic!("no natural class {xml_id}"));
    hc_grammar::model::NatClassId(i as u32)
}

/// Resolve a char-def by its XML id (in table 0).
pub fn char_def(g: &Grammar, xml_id: &str) -> hc_grammar::chardef::CharDefId {
    table(g)
        .iter()
        .find(|(_, cd)| cd.xml_id() == xml_id)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no char def {xml_id}"))
}

/// A `SimpleContext` with no alpha variables over the given natural class.
pub fn ctx(nat_class: hc_grammar::model::NatClassId) -> hc_grammar::model::SimpleContext {
    hc_grammar::model::SimpleContext {
        nat_class,
        vars: vec![],
    }
}

/// Resolve a phonological feature's `FlatIndex` by XML id.
pub fn feat(g: &Grammar, xml_id: &str) -> hc_grammar::featsys::FlatIndex {
    g.phon_features
        .flat_index(xml_id)
        .unwrap_or_else(|| panic!("no feature {xml_id}"))
}

/// A `SimpleContext` over `nat_class` carrying one alpha variable `VarId(var)` governing feature
/// `feature` with the given polarity (`plus` = agree).
pub fn ctx_var(
    nat_class: hc_grammar::model::NatClassId,
    feature: hc_grammar::featsys::FlatIndex,
    var: u16,
    plus: bool,
) -> hc_grammar::model::SimpleContext {
    hc_grammar::model::SimpleContext {
        nat_class,
        vars: vec![hc_grammar::model::AlphaVar {
            feature,
            var: hc_grammar::model::VarId(var),
            plus,
        }],
    }
}

// -------------------------------------------------------------------------------------------------
// A second grammar for alpha-variable tests, adding a `poa` (place) feature. Kept separate so the
// primary probe grammar (and its 16 tests) stay byte-for-byte unchanged.
//
// Features (declaration order): FlatIndex(0)=cons, (1)=voi, (2)=poa (lab bit0 / vel bit1).
// Segments (lanes [cons, voi, poa]):
//   a=[0b10,0b01,0b11] (vowel), p=[0b01,0b10,0b01] (voiceless labial), b=[0b01,0b01,0b01] (voiced
//   labial), k=[0b01,0b10,0b10] (voiceless velar), g=[0b01,0b01,0b10] (voiced velar),
//   n=[0b01,0b01,0b11] (voiced, placeless nasal — poa unmentioned => full mask, so it assimilates).

pub const ALPHA_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>Alpha</Name>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_cons"><Name>cons</Name>
        <Symbols><Symbol id="sym_cp">+</Symbol><Symbol id="sym_cm">-</Symbol></Symbols>
      </SymbolicFeature>
      <SymbolicFeature id="feat_voi"><Name>voi</Name>
        <Symbols><Symbol id="sym_vp">+</Symbol><Symbol id="sym_vm">-</Symbol></Symbols>
      </SymbolicFeature>
      <SymbolicFeature id="feat_poa"><Name>poa</Name>
        <Symbols><Symbol id="sym_lab">lab</Symbol><Symbol id="sym_vel">vel</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations>
          <FeatureValue feature="feat_cons" symbolValues="sym_cm" />
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
        <SegmentDefinition id="char_p"><Representations><Representation>p</Representation></Representations>
          <FeatureValue feature="feat_cons" symbolValues="sym_cp" />
          <FeatureValue feature="feat_voi" symbolValues="sym_vm" />
          <FeatureValue feature="feat_poa" symbolValues="sym_lab" />
        </SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations>
          <FeatureValue feature="feat_cons" symbolValues="sym_cp" />
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
          <FeatureValue feature="feat_poa" symbolValues="sym_lab" />
        </SegmentDefinition>
        <SegmentDefinition id="char_k"><Representations><Representation>k</Representation></Representations>
          <FeatureValue feature="feat_cons" symbolValues="sym_cp" />
          <FeatureValue feature="feat_voi" symbolValues="sym_vm" />
          <FeatureValue feature="feat_poa" symbolValues="sym_vel" />
        </SegmentDefinition>
        <SegmentDefinition id="char_g"><Representations><Representation>g</Representation></Representations>
          <FeatureValue feature="feat_cons" symbolValues="sym_cp" />
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
          <FeatureValue feature="feat_poa" symbolValues="sym_vel" />
        </SegmentDefinition>
        <SegmentDefinition id="char_n"><Representations><Representation>n</Representation></Representations>
          <FeatureValue feature="feat_cons" symbolValues="sym_cp" />
          <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="nc_any"><Name>Any</Name></FeatureNaturalClass>
      <FeatureNaturalClass id="nc_cons"><Name>C</Name>
        <FeatureValue feature="feat_cons" symbolValues="sym_cp" />
      </FeatureNaturalClass>
      <FeatureNaturalClass id="nc_voiced"><Name>Voiced</Name>
        <FeatureValue feature="feat_voi" symbolValues="sym_vp" />
      </FeatureNaturalClass>
      <SegmentNaturalClass id="nc_n"><Name>n</Name><Segment segment="char_n" /></SegmentNaturalClass>
    </NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

pub fn load_alpha_grammar() -> Grammar {
    hc_grammar::load(ALPHA_XML).expect("alpha grammar loads")
}

// -------------------------------------------------------------------------------------------------
// A third grammar with ZERO phonological features (the Sena shape): every segment's lanes are
// identical (just the always-appended Type lane), so segment identity lives entirely in the
// char-def/`StrRep` dimension — C#'s `CharacterDefinitionTable.Add` `fs == null` branch
// (CharacterDefinitionTable.cs:68-76) gives each char-def a `StrRep`-only FeatureStruct on such
// grammars (XmlLanguageLoader.cs:670-673). Used to pin comparisons that MUST distinguish segments
// by that dimension when the lanes cannot.

pub const ZERO_FEAT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>ZeroFeat</Name>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_a"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_b"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_c"><Representations><Representation>c</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="char_x"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="nc_all"><Name>Any</Name>
        <Segment segment="char_a" /><Segment segment="char_b" /><Segment segment="char_c" /><Segment segment="char_x" />
      </SegmentNaturalClass>
    </NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

pub fn load_zero_feat_grammar() -> Grammar {
    hc_grammar::load(ZERO_FEAT_XML).expect("zero-feature grammar loads")
}

// -------------------------------------------------------------------------------------------------
// A fourth grammar with a single **3-symbol** feature (`place`: lab/cor/vel). Every feature in the
// probe/alpha grammars above is 2-valued, which is exactly why Tier-2 #11's `ana_feature`
// anti-FeatureStruct-negation bug (full-unconstrain vs. `L ∪ R`) was undetectable there: on a
// 2-symbol feature the two formulas coincide. `place` gives the analysis-reversal fixture a 3rd
// symbol ("cor") that neither an LHS nor an RHS pin ever mentions, so it can distinguish them.

pub const ANTI_FS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>AntiFS</Name>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="feat_place">
        <Name>place</Name>
        <Symbols>
          <Symbol id="sym_lab">lab</Symbol>
          <Symbol id="sym_cor">cor</Symbol>
          <Symbol id="sym_vel">vel</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="table1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="char_p">
          <Representations><Representation>p</Representation></Representations>
          <FeatureValue feature="feat_place" symbolValues="sym_lab" />
        </SegmentDefinition>
        <SegmentDefinition id="char_t">
          <Representations><Representation>t</Representation></Representations>
          <FeatureValue feature="feat_place" symbolValues="sym_cor" />
        </SegmentDefinition>
        <SegmentDefinition id="char_k">
          <Representations><Representation>k</Representation></Representations>
          <FeatureValue feature="feat_place" symbolValues="sym_vel" />
        </SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="nc_vel">
        <Name>Vel</Name>
        <FeatureValue feature="feat_place" symbolValues="sym_vel" />
      </FeatureNaturalClass>
    </NaturalClasses>
  </Language>
</HermitCrabInput>
"#;

pub fn load_anti_fs_grammar() -> Grammar {
    hc_grammar::load(ANTI_FS_XML).expect("anti-fs grammar loads")
}
