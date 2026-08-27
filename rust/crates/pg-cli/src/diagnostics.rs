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
