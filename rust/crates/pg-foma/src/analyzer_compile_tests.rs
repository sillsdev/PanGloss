#[cfg(test)]
mod budget_tests {
    //! Trust classification regression tests for Foma proposal construction.

    use super::*;

    #[test]
    fn partial_emission_requires_the_explicit_unproven_constructor() {
        assert!(tier_requires_unproven_build(&FomaTier::Partial {
            uncovered: 1
        }));
        assert!(!tier_requires_unproven_build(&FomaTier::Full));
        assert!(!tier_requires_unproven_build(&FomaTier::Unsupported {
            reason: "synthetic refusal".to_string()
        }));
    }
}

#[cfg(test)]
mod apply_budget_tests {
    //! `FomaProposer::propose_budgeted` must behave byte-for-byte identically to `propose` when unbounded, and trip each dimension deterministically and cheaply, in-process.

    use super::*;
    use crate::compose_budget::{ApplyBudget, ApplyDimension, ApplyOutcome};

    /// A single-root, no-affix, no-rule fixture: `propose("ka")` finds exactly the bare root candidate, so a cap of 0 on either dimension trips on the very first decoded path/candidate.
    const FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>ApplyBudgetSmoke</Name>
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

    fn proposer() -> FomaProposer {
        let g = pg_grammar::load(FIXTURE).unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
        compile_proposer(&g).unwrap_or_else(|e| panic!("proposer build failed: {e}"))
    }

    #[test]
    fn propose_budgeted_unbounded_matches_plain_propose_exactly() {
        let mut p = proposer();
        let via_budgeted = match p.propose_budgeted("ka", &ApplyBudget::unbounded()) {
            ApplyOutcome::Complete(candidates) => candidates,
            ApplyOutcome::Incomplete { .. } => {
                panic!("ApplyBudget::unbounded() must never report Incomplete")
            }
        };
        let mut p2 = proposer();
        let via_plain = p2.propose("ka");
        assert_eq!(
            via_budgeted.len(),
            via_plain.len(),
            "propose_budgeted(unbounded) must find exactly as many candidates as propose()"
        );
        assert!(
            !via_plain.is_empty(),
            "the fixture's bare root must propose at least one candidate for this test to be \
             meaningful"
        );
    }

    #[test]
    fn propose_budgeted_path_cap_zero_trips_on_first_decoded_path() {
        let mut p = proposer();
        let budget = ApplyBudget::with_caps(Some(0), None);
        match p.propose_budgeted("ka", &budget) {
            ApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
            } => {
                assert_eq!(dimension, ApplyDimension::DecodedPaths);
                assert_eq!(value, 1, "must trip at exactly one past the cap, not later");
                assert_eq!(limit, 0);
            }
            ApplyOutcome::Complete(candidates) => panic!(
                "expected a path-cap=0 trip on a word with at least one apply_up result, got \
                 Complete({candidates:?})"
            ),
        }
    }

    #[test]
    fn propose_budgeted_candidate_cap_zero_trips_on_first_candidate() {
        let mut p = proposer();
        let budget = ApplyBudget::with_caps(None, Some(0));
        match p.propose_budgeted("ka", &budget) {
            ApplyOutcome::Incomplete {
                dimension,
                value,
                limit,
            } => {
                assert_eq!(dimension, ApplyDimension::Candidates);
                assert_eq!(value, 1);
                assert_eq!(limit, 0);
            }
            ApplyOutcome::Complete(candidates) => panic!(
                "expected a candidate-cap=0 trip on a word with at least one candidate, got \
                 Complete({candidates:?})"
            ),
        }
    }

    #[test]
    fn propose_budgeted_generous_caps_never_trip() {
        let mut p = proposer();
        let budget = ApplyBudget::with_caps(Some(1_000_000), Some(1_000_000));
        match p.propose_budgeted("ka", &budget) {
            ApplyOutcome::Complete(candidates) => assert!(!candidates.is_empty()),
            ApplyOutcome::Incomplete { dimension, .. } => {
                panic!("a generous cap must not trip on a tiny fixture (dimension: {dimension:?})")
            }
        }
    }

    #[test]
    fn propose_with_diagnostics_budgeted_preserves_the_first_path_budget_trip() {
        let mut p = proposer();
        let budget = ApplyBudget::with_caps(Some(0), None);
        let (outcome, diagnostics) = p.propose_with_diagnostics_budgeted("ka", &budget);

        assert!(matches!(
            outcome,
            ApplyOutcome::Incomplete {
                dimension: ApplyDimension::DecodedPaths,
                value: 1,
                limit: 0,
            }
        ));
        assert_eq!(diagnostics.raw_paths, 1);
        assert_eq!(
            diagnostics.raw_paths,
            diagnostics.decoded_paths + diagnostics.malformed_paths
        );
        assert_eq!(diagnostics.unique_candidates, 0);
    }
}

#[cfg(test)]
mod profile_tests {
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
}
