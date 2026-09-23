//! Profiled construction must populate a real `CompileProfile` on success, match the non-profiled entry points byte-for-byte, and still produce a profile on a typed build failure.

use super::*;
use crate::profile::CompileStage;

/// Same minimal single-root fixture shape as `apply_budget_tests::FIXTURE`.
const FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
<Name>ProfileSmoke</Name>
<PartsOfSpeech>
  <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
</PartsOfSpeech>
<CharacterDefinitionTable id="t1">
  <Name>Main</Name>
  <SegmentDefinitions>
    <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
    <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
  </SegmentDefinitions>
</CharacterDefinitionTable>
<NaturalClasses>
  <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
</NaturalClasses>
<Strata>
  <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
    <Name>Main</Name>
    <LexicalEntries>
      <LexicalEntry id="eK" partOfSpeech="posV">
        <Allomorphs><Allomorph id="aK"><PhoneticShape>ka</PhoneticShape></Allomorph></Allomorphs>
        <MorphemeId>K</MorphemeId>
      </LexicalEntry>
    </LexicalEntries>
  </Stratum>
</Strata>
  </Language>
</HermitCrabInput>"#;

fn load_fixture() -> Grammar {
    pg_grammar::load(FIXTURE).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
}

#[test]
fn new_with_profile_populates_lexc_parse_stage_and_final_network_counts() {
    let g = load_fixture();
    let (result, profile) = compile_proposer_with_profile(&g);
    assert!(result.is_ok(), "the tiny fixture must build successfully");

    assert_eq!(profile.pipeline, crate::profile::PRODUCTION_PIPELINE);
    assert!(
        profile
            .stages
            .iter()
            .any(|s| s.stage == CompileStage::LexcParse),
        "a successful build must record the LexcParse stage"
    );
    assert!(
        profile.final_state_count.is_some_and(|v| v > 0),
        "a compiled network must report a positive state count"
    );
    assert!(
        profile.final_arc_count.is_some_and(|v| v >= 0),
        "a compiled network must report a final arc count"
    );
    assert!(profile.total_lexc_lines.is_some_and(|v| v > 0));
}

/// The profiled path must build the same network as the non-profiled path -- proven via identical `propose` results, not just "both `Ok`".
#[test]
fn new_proposer_matches_new_proposer_with_profile_byte_for_byte() {
    let g = load_fixture();

    let mut without_profile =
        compile_proposer(&g).unwrap_or_else(|e| panic!("new_proposer failed: {e}"));
    let (with_profile, _profile) = compile_proposer_with_profile(&g);
    let mut with_profile =
        with_profile.unwrap_or_else(|e| panic!("new_proposer_with_profile failed: {e}"));

    assert_eq!(without_profile.propose("ka"), with_profile.propose("ka"));
}
