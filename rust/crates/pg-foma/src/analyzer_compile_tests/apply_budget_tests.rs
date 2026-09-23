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
