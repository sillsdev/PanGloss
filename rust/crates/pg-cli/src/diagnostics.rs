//! `pangloss diagnose`: assesses a grammar against a word list, producing `pg_assess::AssessmentReport` (the repo's one canonical artifact); apply-time analysis is contained by cooperative magnitude budgets, never a watchdog.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use pg_assess::{
    AnalysisIdentity, AnalysisSet, AssessmentReport, CaseOutcome, CaseRecord, Diagnostic,
    Execution, IncompleteReason, Provenance, ReportDraft, Severity, SuiteRef, IDENTITY_PROFILE,
};
use pg_foma::compose_budget::ApplyBudget;
use pg_foma::composite::{FomaAnalyzer, FomaApplyOutcome};
use pg_grammar::model::Grammar;

/// This module's own schema version, written into every `BuildReport`/`AssessmentReport`; bump only on a wire-incompatible change to either type.
pub const DIAGNOSTICS_SCHEMA_VERSION: u32 = 2;

/// The build-side report, kept separate and immutable from `assessment.json`: produced once per grammar load, independent of any word list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildReport {
    pub schema_version: u32,
    /// `Grammar::name`, verbatim; never invents a name when the grammar declares none.
    pub grammar_name: Option<String>,
    /// `Grammar::entries.len()`, part of this report's lightweight build-identity fingerprint.
    pub lex_entry_count: usize,
    pub morpheme_count: usize,
    pub stratum_count: usize,
    /// `crate::load_grammar`'s own compile/import warnings, recorded here so a `build.json` consumer has them without re-running the load.
    pub load_warnings: Vec<String>,
}

/// Builds a `BuildReport` from an already-loaded grammar plus `crate::load_grammar`'s own warnings; pure, so directly unit-testable.
pub fn build_report(grammar: &Grammar, load_warnings: Vec<String>) -> BuildReport {
    BuildReport {
        schema_version: DIAGNOSTICS_SCHEMA_VERSION,
        grammar_name: grammar.name.clone(),
        lex_entry_count: grammar.entries.len(),
        morpheme_count: grammar.morphemes.len(),
        stratum_count: grammar.strata.len(),
        load_warnings,
    }
}

/// Assess `words` against `grammar`'s production pipeline, producing the repo's one canonical assessment artifact; `grammar` is compiled to foma exactly once, so the recorded apply status can never describe a run other than the one that produced the analyses.
pub fn assess_words(
    grammar: &Grammar,
    grammar_path: &str,
    words: &[String],
    apply_budget: &ApplyBudget,
    warnings: &[pg_snapshot::Warning],
) -> Result<AssessmentReport, String> {
    let mut analyzer =
        FomaAnalyzer::new(grammar).map_err(|e| format!("foma analyzer build failed: {e}"))?;

    let mut cases = Vec::with_capacity(words.len());
    let mut per_word_diagnostics = serde_json::Map::new();

    for (index, word) in words.iter().enumerate() {
        // Deterministic and positional, matching `assess --words`'s convention, so runs over the same list join on the same IDs.
        let case_id = format!("w{index}:{word}");

        let (outcome, candidates_generated, gloss_signature) =
            match analyzer.analyze_word_budgeted(word, apply_budget) {
                FomaApplyOutcome::Complete(found) => {
                    let pairs: Vec<(pg_parse::WordAnalysis, String)> = found
                        .structured
                        .iter()
                        .cloned()
                        .zip(
                            found
                                .analyses
                                .iter()
                                .map(|(_join, surface)| surface.clone()),
                        )
                        .collect();
                    let gloss = pg_realize::word_gloss_signature(grammar, &pairs);

                    let mut annotated = Vec::with_capacity(found.structured.len());
                    for analysis in &found.structured {
                        let identity = AnalysisIdentity::project(analysis, grammar)
                            .map_err(|e| format!("project analysis identity for {word}: {e}"))?;
                        annotated.push((identity, analysis.guessed));
                    }
                    (
                        CaseOutcome::Complete(AnalysisSet::from_annotated(annotated)),
                        found.candidates_generated,
                        gloss,
                    )
                }
                FomaApplyOutcome::Incomplete {
                    dimension,
                    value,
                    limit,
                } => (
                    CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
                        dimension: crate::assess::budget_dimension(dimension),
                        value: value as u64,
                        limit: limit as u64,
                    }),
                    // A tripped budget confirms nothing, so there is no candidate count or gloss to report.
                    0,
                    String::new(),
                ),
            };

        per_word_diagnostics.insert(
            case_id.clone(),
            serde_json::json!({
                "candidatesGenerated": candidates_generated,
                "glossSignature": gloss_signature,
            }),
        );
        cases.push(CaseRecord {
            case_id,
            input: word.clone(),
            outcome,
            supersedes: Vec::new(),
        });
    }

    let source =
        fs::read_to_string(grammar_path).map_err(|e| format!("read {grammar_path}: {e}"))?;
    let source_kind = crate::assess::source_kind_of(grammar_path);
    let version = env!("CARGO_PKG_VERSION");
    let digest = pg_assess::sha256_bytes(source.as_bytes());

    ReportDraft {
        generated_at: crate::assess::now_rfc3339(),
        suite: SuiteRef {
            // `diagnose` runs a bare word list, not a caller-authored suite, so the "suite" is the list itself.
            suite_id: format!("diagnose:{grammar_path}"),
            suite_revision: digest.clone(),
            semantic_digest: digest,
            analysis_identity_profile: IDENTITY_PROFILE.to_string(),
        },
        execution: Execution {
            pipeline: "foma-confirm".to_string(),
            budgets: crate::assess::recorded_budgets(apply_budget),
            wall_clock_limit_us: None,
        },
        provenance: Provenance {
            source_sha256: pg_assess::source_sha256(source.as_bytes()),
            source_kind: source_kind.as_str().to_string(),
            model_fingerprint: pg_assess::model_fingerprint(source_kind, &source, version)
                .map_err(|e| format!("model fingerprint: {e}"))?,
            importer_version: version.to_string(),
            compiler_version: version.to_string(),
        },
        // Codes carried through from the importer rather than flattened to one bucket, so `compare` can distinguish a count change from a reworded message.
        diagnostics: warnings
            .iter()
            .map(|w| Diagnostic {
                code: w.code.to_string(),
                severity: Severity::Warning,
                message: w.message.clone(),
            })
            .collect(),
        cases,
        failure: None,
        extensions: Some(serde_json::json!({
            "org.sil.pangloss.diagnose": { "perCase": per_word_diagnostics }
        })),
    }
    .finish()
    .map_err(|e| format!("finish assessment report: {e}"))
}

/// `pangloss diagnose <grammar> <words.txt> <out-dir>`: writes `<out-dir>/build.json` and `<out-dir>/assessment.json`, always both, always separate files, never a combined artifact.
pub fn run_diagnose(args: &[String]) -> Result<(), String> {
    let [grammar_path, words_path, out_dir] = args else {
        return Err("usage: diagnose <grammar> <words.txt> <out-dir>".to_string());
    };

    let (grammar, coded_warnings) = crate::load_grammar_coded(grammar_path)?;
    // `build.json` keeps its prose-only shape; only the assessment artifact needs the codes.
    let load_warnings: Vec<String> = coded_warnings.iter().map(|w| w.to_string()).collect();
    crate::print_grammar_warnings(&load_warnings);

    let words: Vec<String> = fs::read_to_string(words_path)
        .map_err(|e| format!("read {words_path}: {e}"))?
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect();

    let build = build_report(&grammar, load_warnings);
    let assessment = assess_words(
        &grammar,
        grammar_path,
        &words,
        &ApplyBudget::from_env(),
        &coded_warnings,
    )?;

    fs::create_dir_all(out_dir).map_err(|e| format!("create {out_dir}: {e}"))?;
    let build_path = Path::new(out_dir).join("build.json");
    let assessment_path = Path::new(out_dir).join("assessment.json");

    fs::write(
        &build_path,
        serde_json::to_string_pretty(&build).map_err(|e| format!("serialize build.json: {e}"))?,
    )
    .map_err(|e| format!("write {}: {e}", build_path.display()))?;
    fs::write(
        &assessment_path,
        assessment
            .to_canonical_json()
            .map_err(|e| format!("serialize assessment.json: {e}"))?,
    )
    .map_err(|e| format!("write {}: {e}", assessment_path.display()))?;

    // Counts with their denominator, no rate; `status` is the run's verdict on execution, never on the grammar.
    let complete = assessment
        .cases()
        .iter()
        .filter(|c| c.outcome.is_complete())
        .count();
    eprintln!(
        "diagnose {status:?}: {complete}/{total} cases complete -> {out_dir}",
        status = assessment.status(),
        total = assessment.cases().len(),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic, tiny fixture: bare root `eRoot` (gloss `GX`, surface `kal`) and root `eBare` (no gloss, surface `tuz`), enough gloss variety to exercise both `g:`/`m:` gloss tags.
    const FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>DiagnosticsFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="n"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segL"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="bdry1"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="eRoot" partOfSpeech="n">
            <Allomorphs><Allomorph id="eRoot-1"><PhoneticShape>kal</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>GX</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eBare" partOfSpeech="n">
            <Allomorphs><Allomorph id="eBare-1"><PhoneticShape>tuz</PhoneticShape></Allomorph></Allomorphs>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    fn grammar() -> Grammar {
        pg_grammar::load(FIXTURE_XML)
            .unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
    }

    #[test]
    fn build_report_reflects_grammar_counts_and_warnings() {
        let g = grammar();
        let report = build_report(&g, vec!["warn: synthetic".to_string()]);
        assert_eq!(report.schema_version, DIAGNOSTICS_SCHEMA_VERSION);
        assert_eq!(report.grammar_name.as_deref(), Some("DiagnosticsFixture"));
        assert_eq!(report.lex_entry_count, 2);
        assert_eq!(report.stratum_count, 1);
        assert_eq!(report.load_warnings, vec!["warn: synthetic".to_string()]);
    }

    fn grammar_file() -> std::path::PathBuf {
        // `assess_words` hashes the grammar's exact source bytes, so the fixture must exist on disk.
        let dir = std::env::temp_dir().join("pg-diagnose-fixture");
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        let path = dir.join("diagnostics-fixture.xml");
        std::fs::write(&path, FIXTURE_XML).expect("write fixture");
        path
    }

    fn assess(words: &[&str], budget: &ApplyBudget) -> AssessmentReport {
        let path = grammar_file();
        let words: Vec<String> = words.iter().map(|w| (*w).to_string()).collect();
        assess_words(
            &grammar(),
            path.to_str().expect("fixture path is UTF-8"),
            &words,
            budget,
            &[],
        )
        .expect("assessment must succeed")
    }

    fn diagnose_extension(
        report: &AssessmentReport,
        case_id: &str,
        field: &str,
    ) -> serde_json::Value {
        report
            .draft()
            .extensions
            .as_ref()
            .expect("diagnose always attaches its extension")["org.sil.pangloss.diagnose"]
            ["perCase"][case_id][field]
            .clone()
    }

    #[test]
    fn diagnose_emits_the_one_canonical_assessment_artifact() {
        let report = assess(&["kal", "tuz", "zzzz"], &ApplyBudget::unbounded());
        let value = report.to_value();

        assert_eq!(value["schema"], "pangloss.assessment-report");
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["execution"]["pipeline"], "foma-confirm");
        assert_eq!(value["status"], "complete");
        assert_eq!(report.cases().len(), 3);
        // Every case completed, including the unanalyzable word, whose set is empty -- a positive claim, not a failure.
        assert!(report.cases().iter().all(|c| c.outcome.is_complete()));
        assert_eq!(
            report.cases()[2].outcome.analyses().unwrap().len(),
            0,
            "an unanalyzable word completes with an empty set"
        );
    }

    #[test]
    fn per_word_diagnostics_survive_in_the_reports_extensions() {
        // The canonical report has no field for propose-side over-generation or gloss rendering, so they live in the namespaced extension slot instead.
        let report = assess(&["kal", "tuz", "zzzz"], &ApplyBudget::unbounded());

        assert_eq!(
            diagnose_extension(&report, "w0:kal", "glossSignature"),
            serde_json::json!(r#"g:"GX"|s:"kal""#),
            "must match pg_realize::signature's own documented encoding exactly, not a re-derived one"
        );
        assert_eq!(
            diagnose_extension(&report, "w1:tuz", "glossSignature"),
            serde_json::json!(r#"m:""|s:"tuz""#)
        );
        assert_eq!(
            diagnose_extension(&report, "w2:zzzz", "glossSignature"),
            serde_json::json!("-"),
            "zero confirmed analyses must render the same '-' literal pg_realize::signature \
             documents for an empty entry set"
        );
        assert_eq!(
            diagnose_extension(&report, "w0:kal", "candidatesGenerated"),
            serde_json::json!(1)
        );
    }

    #[test]
    fn extensions_do_not_move_either_semantic_projection() {
        // The guarantee that lets diagnose carry extra evidence without changing what the report means: two runs differing only in the extension agree semantically.
        let with = assess(&["kal"], &ApplyBudget::unbounded());
        let mut without = with.draft().clone();
        without.extensions = None;
        let without = without.finish().expect("digests");

        assert_eq!(with.semantic_digest(), without.semantic_digest());
        assert_eq!(with.outcome_digest(), without.outcome_digest());
        assert_ne!(with.report_id(), without.report_id());
    }

    #[test]
    fn a_tripped_budget_is_a_typed_incomplete_not_an_empty_analysis_set() {
        // cap=0 on decoded paths: the bare root must still decode at least one apply_up result, tripping the budget on the first one.
        let report = assess(&["kal"], &ApplyBudget::with_caps(Some(0), None));

        assert_eq!(report.status(), pg_assess::AssessmentStatus::Failed);
        match &report.cases()[0].outcome {
            CaseOutcome::Incomplete(IncompleteReason::LogicalBudget {
                dimension,
                value,
                limit,
            }) => {
                assert_eq!(*dimension, pg_assess::BudgetDimension::DecodedPaths);
                assert_eq!(*value, 1);
                assert_eq!(*limit, 0);
            }
            other => panic!("expected a typed logical-budget incomplete, got {other:?}"),
        }
        assert!(
            report.cases()[0].outcome.analyses().is_none(),
            "a tripped budget must not present an authoritative set"
        );
    }

    #[test]
    fn the_recorded_budget_is_the_one_actually_in_force() {
        // The envelope reaches the report from `ApplyBudget` itself, so a budget from `from_env` is recorded as faithfully as one from the command line.
        let unbounded = assess(&["kal"], &ApplyBudget::unbounded());
        assert!(
            unbounded.draft().execution.budgets.is_empty(),
            "unbounded is recorded as no envelope, not as a zero"
        );

        let capped = assess(&["kal"], &ApplyBudget::with_caps(Some(0), None));
        assert_eq!(
            capped.draft().execution.budgets.get("decodedPaths"),
            Some(&0u64)
        );
    }

    #[test]
    fn the_assessment_artifact_round_trips_and_reproduces_its_digests() {
        // Not a golden-string test: `generatedAt` and the digests change per run by design; what needs guarding is that the artifact parses back to the same evidence.
        let report = assess(&["kal"], &ApplyBudget::unbounded());
        let json = report.to_canonical_json().expect("canonicalize");
        let read = pg_assess::parse_report(&json).expect(
            "a diagnose artifact must parse as the
             canonical report — that is the whole point of having one type",
        );

        assert_eq!(read.report_id(), report.report_id());
        assert_eq!(read.semantic_digest(), report.semantic_digest());
        assert_eq!(read.outcome_digest(), report.outcome_digest());
        assert_eq!(read.cases(), report.cases());
    }

    #[test]
    fn importer_warning_codes_reach_the_report_rather_than_one_bucket() {
        // `compare` diffs diagnostics by code and count, so a caller must be able to tell a data change from a reworded message.
        let path = grammar_file();
        let warnings = vec![
            pg_snapshot::Warning::new(
                "fwdata.dangling-reference",
                "entry 4 refers to a missing MSA",
            ),
            pg_snapshot::Warning::new("fwdata.unsupported-morph-type", "morph type not supported"),
            pg_snapshot::Warning::new(
                "fwdata.dangling-reference",
                "entry 9 refers to a missing MSA",
            ),
        ];
        let report = assess_words(
            &grammar(),
            path.to_str().unwrap(),
            &["kal".to_string()],
            &ApplyBudget::unbounded(),
            &warnings,
        )
        .expect("assessment must succeed");

        let codes: Vec<&str> = report
            .draft()
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(
            codes,
            vec![
                "fwdata.dangling-reference",
                "fwdata.unsupported-morph-type",
                "fwdata.dangling-reference"
            ],
            "codes must survive to the report, with their multiplicity — count per code is what              `compare` diffs"
        );
        assert!(
            !codes.contains(&"importer.warning"),
            "a single catch-all bucket is exactly what this task replaced"
        );
    }

    #[test]
    fn two_diagnose_runs_of_the_same_words_agree_on_behaviour() {
        let first = assess(&["kal", "tuz"], &ApplyBudget::unbounded());
        let second = assess(&["kal", "tuz"], &ApplyBudget::unbounded());
        assert_eq!(first.outcome_digest(), second.outcome_digest());
        assert_eq!(first.semantic_digest(), second.semantic_digest());
    }
}
