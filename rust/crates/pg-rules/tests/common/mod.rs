//! Shared test support: a tiny hand-authored HermitCrab grammar, loaded through the real `pg_grammar::load` since `PhonFeatureSystem`/`CharDefTable` constructors are `pub(crate)`; segment/feature layout is in `GRAMMAR_XML` below.

#![allow(dead_code)]

use pg_grammar::model::Grammar;

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
    pg_grammar::load(GRAMMAR_XML).expect("probe grammar loads")
}

/// The single char-def table of the probe grammar.
pub fn table(g: &Grammar) -> &pg_grammar::chardef::CharDefTable {
    &g.char_tables[0]
}

/// Resolve a natural class by its XML id.
pub fn nat_class(g: &Grammar, xml_id: &str) -> pg_grammar::model::NatClassId {
    let i = g
        .natural_classes
        .iter()
        .position(|nc| nc.xml_id == xml_id)
        .unwrap_or_else(|| panic!("no natural class {xml_id}"));
    pg_grammar::model::NatClassId(i as u32)
}

/// Resolve a char-def by its XML id (in table 0).
pub fn char_def(g: &Grammar, xml_id: &str) -> pg_grammar::chardef::CharDefId {
    table(g)
        .iter()
        .find(|(_, cd)| cd.xml_id() == xml_id)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no char def {xml_id}"))
}

/// A `SimpleContext` with no alpha variables over the given natural class.
pub fn ctx(nat_class: pg_grammar::model::NatClassId) -> pg_grammar::model::SimpleContext {
    pg_grammar::model::SimpleContext {
        nat_class,
        vars: vec![],
    }
}

/// Resolve a phonological feature's `FlatIndex` by XML id.
pub fn feat(g: &Grammar, xml_id: &str) -> pg_grammar::featsys::FlatIndex {
    g.phon_features
        .flat_index(xml_id)
        .unwrap_or_else(|| panic!("no feature {xml_id}"))
}

/// A `SimpleContext` over `nat_class` carrying one alpha variable governing `feature` with the given polarity (`plus` = agree).
pub fn ctx_var(
    nat_class: pg_grammar::model::NatClassId,
    feature: pg_grammar::featsys::FlatIndex,
    var: u16,
    plus: bool,
) -> pg_grammar::model::SimpleContext {
    pg_grammar::model::SimpleContext {
        nat_class,
        vars: vec![pg_grammar::model::AlphaVar {
            feature,
            var: pg_grammar::model::VarId(var),
            plus,
        }],
    }
}

// A second grammar for alpha-variable tests, adding a `poa` (place) feature; kept separate so the primary probe grammar's tests stay byte-for-byte unchanged. Layout is in `ALPHA_XML` below.

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
    pg_grammar::load(ALPHA_XML).expect("alpha grammar loads")
}

// A third grammar with zero phonological features (the Sena shape): every segment's lanes are identical, so segment identity lives entirely in the char-def/`StrRep` dimension, mirroring C#'s `fs == null` branch.

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
    pg_grammar::load(ZERO_FEAT_XML).expect("zero-feature grammar loads")
}

// A fourth grammar with a single 3-symbol feature (`place`): every feature above is 2-valued, on which full-unconstrain and `L ∪ R` coincide, so this fixture's 3rd symbol is what can tell those two formulas apart.

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
    pg_grammar::load(ANTI_FS_XML).expect("anti-fs grammar loads")
}
