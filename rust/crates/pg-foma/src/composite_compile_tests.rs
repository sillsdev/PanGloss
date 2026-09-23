use crate::compose_budget::{ApplyBudget, ApplyDimension};
use pg_parse::{Morpher, ParseOptions};

fn sample_path(name: &str) -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn load_sena() -> Option<Grammar> {
    let path = sample_path("sena-hc.xml")?;
    let xml = std::fs::read_to_string(&path).expect("read grammar");
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
}

/// A word with no proposed candidates at all returns an empty, non-panicking outcome.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn unknown_word_returns_empty_outcome() {
    let Some(g) = load_sena() else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let mut analyzer = compile_analyzer(&g).expect("sena compiles");
    let outcome = analyzer.analyze_word("zzzqxxxnonsense");
    assert!(outcome.structured.is_empty());
    assert!(outcome.analyses.is_empty());
    assert_eq!(outcome.confirmed, 0);
    assert!(!outcome.peel_used);
}

/// Sanity: `mbali` confirms to a non-empty outcome no larger than `candidates_generated` (confirm only prunes, never invents).
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn mbali_confirms_within_candidate_bound() {
    let Some(g) = load_sena() else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let mut analyzer = compile_analyzer(&g).expect("sena compiles");
    let outcome = analyzer.analyze_word("mbali");
    assert!(!outcome.structured.is_empty());
    assert!(outcome.confirmed <= outcome.candidates_generated);
    let morpher = Morpher::new(&g, usize::MAX);
    let engine = morpher.parse_word_opts("mbali", &ParseOptions::default());
    assert_eq!(outcome.structured.len(), engine.structured.len());
}

/// Regression guard: the parallel-confirm batch path must produce, per word, the exact same confirmed-analysis multiset as calling `analyze_word` alone, including a word with zero candidates.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn analyze_words_matches_analyze_word_per_word() {
    let Some(g) = load_sena() else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let words: Vec<String> = ["mbali", "zzzqxxxnonsense", "mbali"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let mut analyzer = compile_analyzer(&g).expect("sena compiles");
    let sequential: Vec<String> = words
        .iter()
        .map(|w| pg_parse::result_signature(&analyzer.analyze_word(w).analyses))
        .collect();

    let batched = analyzer.analyze_words(&words);
    assert_eq!(batched.len(), words.len());
    let parallel: Vec<String> = batched
        .iter()
        .map(|(outcome, _)| pg_parse::result_signature(&outcome.analyses))
        .collect();

    assert_eq!(
        sequential, parallel,
        "analyze_words must match analyze_word per word, in order"
    );
}

const DIAGNOSTICS_FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>CompositeDiagnosticsSmoke</Name>
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

#[test]
fn analyze_word_with_diagnostics_matches_normal_pipeline_and_accounts_exactly() {
    let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
    let mut normal = compile_analyzer(&g).expect("normal analyzer compiles");
    let expected = normal.analyze_word("ka");
    let mut diagnostic = compile_analyzer(&g).expect("diagnostic analyzer compiles");
    let profiled = diagnostic.analyze_word_with_diagnostics("ka");

    assert_eq!(
        pg_parse::result_signature(&profiled.outcome.analyses),
        pg_parse::result_signature(&expected.analyses)
    );
    assert_eq!(
        profiled.outcome.candidates_generated,
        expected.candidates_generated
    );
    assert_eq!(profiled.outcome.confirmed, expected.confirmed);
    assert_eq!(profiled.diagnostics.confirm_batch_calls, 1);
    assert_eq!(
        profiled.diagnostics.confirmation_calls,
        profiled.diagnostics.confirmation_groups
    );
    assert!(profiled.diagnostics.confirmation_groups <= profiled.outcome.candidates_generated);
    assert_eq!(
        profiled.diagnostics.confirmed_analyses,
        profiled.outcome.confirmed
    );
    assert_eq!(
        profiled.diagnostics.proposal.raw_paths,
        profiled.diagnostics.proposal.decoded_paths + profiled.diagnostics.proposal.malformed_paths
    );
}

#[test]
fn from_precompiled_proposer_matches_normal_results_and_capped_diagnostics() {
    let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
    let mut normal = compile_analyzer(&g).expect("normal analyzer compiles");
    let expected = normal.analyze_word("ka");

    let proposer = compile_proposer(&g).expect("precompiled proposer compiles");
    let mut precompiled = FomaAnalyzer::from_precompiled_proposer(&g, proposer);
    let budget = ApplyBudget::with_caps(Some(100), Some(100));
    let profiled = match precompiled.analyze_word_with_diagnostics_budgeted("ka", &budget) {
        ProfiledFomaApplyOutcome::Complete(profiled) => profiled,
        ProfiledFomaApplyOutcome::Incomplete { dimension, .. } => {
            panic!("generous tiny-fixture budget tripped: {dimension:?}")
        }
    };

    assert_eq!(
        pg_parse::result_signature(&profiled.outcome.analyses),
        pg_parse::result_signature(&expected.analyses)
    );
    assert_eq!(
        profiled.outcome.candidates_generated,
        expected.candidates_generated
    );
    assert_eq!(profiled.outcome.confirmed, expected.confirmed);
    assert_eq!(
        profiled.diagnostics.proposal.raw_paths,
        profiled.diagnostics.proposal.decoded_paths + profiled.diagnostics.proposal.malformed_paths
    );
}

#[test]
fn analyze_word_with_diagnostics_budgeted_stops_before_confirming_partial_candidates() {
    let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
    let mut analyzer = compile_analyzer(&g).expect("analyzer compiles");
    let budget = crate::compose_budget::ApplyBudget::with_caps(Some(0), None);

    match analyzer.analyze_word_with_diagnostics_budgeted("ka", &budget) {
        ProfiledFomaApplyOutcome::Incomplete {
            dimension,
            value,
            limit,
            diagnostics,
        } => {
            assert_eq!(
                dimension,
                crate::compose_budget::ApplyDimension::DecodedPaths
            );
            assert_eq!(value, 1);
            assert_eq!(limit, 0);
            assert_eq!(diagnostics.confirm_batch_calls, 0);
            assert_eq!(diagnostics.confirmation_calls, 0);
            assert_eq!(diagnostics.confirmation_groups, 0);
            assert_eq!(
                diagnostics.proposal.raw_paths,
                diagnostics.proposal.decoded_paths + diagnostics.proposal.malformed_paths
            );
        }
        ProfiledFomaApplyOutcome::Complete(_) => {
            panic!("path-cap=0 must not confirm a partial proposal")
        }
    }
}

#[test]
fn an_unbounded_budget_leaves_analyze_word_unchanged() {
    // The whole safety argument for routing production through the budgeted entry point: every cap check is `Some(cap) if count > cap`, so `None` can never trip.
    let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
    let mut analyzer = compile_analyzer(&g).expect("analyzer compiles");
    let expected = analyzer.analyze_word("ka");

    match analyzer.analyze_word_budgeted("ka", &ApplyBudget::unbounded()) {
        FomaApplyOutcome::Complete(outcome) => {
            assert_eq!(
                pg_parse::result_signature(&outcome.analyses),
                pg_parse::result_signature(&expected.analyses)
            );
            assert_eq!(outcome.candidates_generated, expected.candidates_generated);
            assert_eq!(outcome.confirmed, expected.confirmed);
            assert_eq!(outcome.peel_used, expected.peel_used);
        }
        FomaApplyOutcome::Incomplete { dimension, .. } => {
            panic!("an unbounded budget tripped on {dimension:?}")
        }
    }
}

#[test]
fn the_budgeted_production_path_agrees_with_the_diagnostic_one() {
    // Two budgeted paths are two contracts, so they must agree on results.
    let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
    let mut analyzer = compile_analyzer(&g).expect("analyzer compiles");
    let budget = ApplyBudget::with_caps(Some(100), Some(100));

    let production = match analyzer.analyze_word_budgeted("ka", &budget) {
        FomaApplyOutcome::Complete(outcome) => outcome,
        FomaApplyOutcome::Incomplete { dimension, .. } => {
            panic!("generous tiny-fixture budget tripped: {dimension:?}")
        }
    };
    let diagnostic = match analyzer.analyze_word_with_diagnostics_budgeted("ka", &budget) {
        ProfiledFomaApplyOutcome::Complete(profiled) => profiled.outcome,
        ProfiledFomaApplyOutcome::Incomplete { dimension, .. } => {
            panic!("generous tiny-fixture budget tripped: {dimension:?}")
        }
    };

    assert_eq!(
        pg_parse::result_signature(&production.analyses),
        pg_parse::result_signature(&diagnostic.analyses)
    );
    assert_eq!(
        production.candidates_generated,
        diagnostic.candidates_generated
    );
}

#[test]
fn a_tripped_production_budget_reports_the_dimension_and_confirms_nothing() {
    let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
    let mut analyzer = compile_analyzer(&g).expect("analyzer compiles");

    match analyzer.analyze_word_budgeted("ka", &ApplyBudget::with_caps(Some(0), None)) {
        FomaApplyOutcome::Incomplete {
            dimension,
            value,
            limit,
        } => {
            assert_eq!(dimension, ApplyDimension::DecodedPaths);
            assert_eq!(value, 1);
            assert_eq!(limit, 0);
        }
        // Must not come back as a complete outcome with an empty analysis list, which would read as "no analysis exists" rather than "we stopped looking".
        FomaApplyOutcome::Complete(_) => {
            panic!("path-cap=0 must not confirm a partial proposal")
        }
    }
}

#[test]
fn a_candidate_cap_trip_reports_the_candidate_dimension() {
    let g = pg_grammar::load(DIAGNOSTICS_FIXTURE)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}"));
    let mut analyzer = compile_analyzer(&g).expect("analyzer compiles");

    match analyzer.analyze_word_budgeted("ka", &ApplyBudget::with_caps(None, Some(0))) {
        FomaApplyOutcome::Incomplete {
            dimension, limit, ..
        } => {
            assert_eq!(dimension, ApplyDimension::Candidates);
            assert_eq!(limit, 0);
        }
        FomaApplyOutcome::Complete(_) => {
            panic!("candidate-cap=0 must not confirm a partial proposal")
        }
    }
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_diagnostics_preserve_results_and_report_real_confirmation_topology() {
    let Some(g) = load_sena() else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let mut normal = compile_analyzer(&g).expect("sena compiles");
    let expected = normal.analyze_word("mbali");
    let mut diagnostic = compile_analyzer(&g).expect("sena compiles");
    let profiled = diagnostic.analyze_word_with_diagnostics("mbali");

    assert_eq!(
        pg_parse::result_signature(&profiled.outcome.analyses),
        pg_parse::result_signature(&expected.analyses)
    );
    assert_eq!(profiled.diagnostics.confirm_batch_calls, 1);
    assert_eq!(
        profiled.diagnostics.confirmation_calls,
        profiled.diagnostics.confirmation_groups
    );
    assert!(
            profiled.diagnostics.confirmation_groups
                <= profiled.outcome.candidates_generated,
            "confirm_batch may fuse candidates, so group count is bounded by, not equal to, candidate count"
        );
    assert_eq!(
        profiled.diagnostics.confirmed_analyses,
        profiled.outcome.confirmed
    );
    assert_eq!(
        profiled.diagnostics.proposal.raw_paths,
        profiled.diagnostics.proposal.decoded_paths + profiled.diagnostics.proposal.malformed_paths
    );
}
