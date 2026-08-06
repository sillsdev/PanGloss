//! `pangloss fst-health <grammar> [<words.txt>] [<out.json>]` composes three sets of findings (preflight, a standalone profiled compile, and — only when `<words.txt>` is given — apply-side measurement) into one `HealthReport`, deduplicating `FomaOutcome::structured` by `WordAnalysis`'s structured identity rather than `result_signature`'s rendered-string equality, and computing rejection share as `(proposed - confirmed) / proposed` via `saturating_sub` to stay in `[0.0, 1.0]`.

use std::fs;

use pg_foma::analyzer::{FomaError, FomaProposer};
use pg_foma::composite::FomaAnalyzer;
use pg_foma::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
use pg_foma::health_evaluator::evaluate_health;
use pg_foma::preflight::preflight_findings;
use pg_grammar::model::Grammar;
use pg_parse::WordAnalysis;

/// Every word's `FomaOutcome::structured`, deduplicated by `WordAnalysis`'s derived equality, feeds `DuplicateAnalysisOverlap` plus the batch-level `ProposalVolume`/`ConfirmationWork` findings, always emitted once at least one word was measured — never gated behind "only report if something looks wrong".
fn measure_apply_side(grammar: &Grammar, words: &[String]) -> Result<Vec<HealthFinding>, String> {
    let mut analyzer =
        FomaAnalyzer::new(grammar).map_err(|e| format!("foma analyzer build failed: {e}"))?;

    let mut findings = Vec::new();
    let mut total_candidates: u64 = 0;
    let mut total_confirmed: u64 = 0;

    for word in words {
        let outcome = analyzer.analyze_word(word);
        total_candidates += outcome.candidates_generated as u64;
        total_confirmed += outcome.confirmed as u64;
        findings.extend(duplicate_analysis_findings(word, &outcome.structured));
    }

    if !words.is_empty() {
        findings.push(proposal_volume_finding(total_candidates));
        findings.extend(confirmation_work_findings(
            total_candidates,
            total_confirmed,
        ));
    }

    Ok(findings)
}

/// One word's pre-dedup duplicate-analysis evidence: many copies still mean one semantic answer but expose an FST design problem, so this emits `Severity::Info` count/ratio findings, both empty when `structured` has no duplicate at all.
fn duplicate_analysis_findings(word: &str, structured: &[WordAnalysis]) -> Vec<HealthFinding> {
    let total = structured.len();
    if total == 0 {
        return Vec::new();
    }
    let mut unique: Vec<&WordAnalysis> = Vec::with_capacity(structured.len());
    for wa in structured {
        if !unique.iter().any(|u| **u == *wa) {
            unique.push(wa);
        }
    }
    let duplicate_count = total - unique.len();
    if duplicate_count == 0 {
        return Vec::new();
    }
    let ratio = duplicate_count as f64 / total as f64;
    let explanation = format!(
        "{duplicate_count} of {total} confirmed analyses for {word:?} are pre-dedup duplicates of \
         another confirmed analysis under the SAME structured-analysis identity this repo already \
         uses (`pg_parse::WordAnalysis`'s own derived equality) -- {unique_len} distinct semantic \
         answer(s), {duplicate_count} redundant copy/copies. Per R6, duplicate copies still expose \
         an FST design problem even though the semantic result is correct.",
        unique_len = unique.len(),
    );
    vec![
        HealthFinding {
            code: FindingCode::DuplicateAnalysisOverlap,
            severity: Severity::Info,
            phase: Phase::Apply,
            affected: vec![word.to_string()],
            metric: Metric::DuplicateAnalysisCount,
            value: MetricValue::Count(duplicate_count as u64),
            provenance: ValueProvenance::Observed,
            threshold: None,
            explanation: explanation.clone(),
            remedies: Vec::new(),
            override_record: None,
        },
        HealthFinding {
            code: FindingCode::DuplicateAnalysisOverlap,
            severity: Severity::Info,
            phase: Phase::Apply,
            affected: vec![word.to_string()],
            metric: Metric::DuplicateAnalysisRatio,
            value: MetricValue::Ratio(ratio),
            provenance: ValueProvenance::Observed,
            threshold: None,
            explanation,
            remedies: Vec::new(),
            override_record: None,
        },
    ]
}

/// Total distinct FST-propose candidates across every word measured; `Severity::Info` since proposal volume is evidence, not itself a problem.
fn proposal_volume_finding(total_candidates: u64) -> HealthFinding {
    HealthFinding {
        code: FindingCode::ProposalVolume,
        severity: Severity::Info,
        phase: Phase::Apply,
        affected: Vec::new(),
        metric: Metric::ProposalCandidateCount,
        value: MetricValue::Count(total_candidates),
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: format!(
            "This word set proposed {total_candidates} distinct FST-propose candidate(s) in total \
             across every word measured. Candidate volume remains first-class health evidence \
             independent of whether every confirmed result was ultimately correct (R6)."
        ),
        remedies: Vec::new(),
        override_record: None,
    }
}

/// Total confirmed analyses plus the rejection share; the rejection finding is omitted entirely when `total_candidates` is zero, never a fabricated `0.0`.
fn confirmation_work_findings(total_candidates: u64, total_confirmed: u64) -> Vec<HealthFinding> {
    let mut findings = vec![HealthFinding {
        code: FindingCode::ConfirmationWork,
        severity: Severity::Info,
        phase: Phase::Apply,
        affected: Vec::new(),
        metric: Metric::ConfirmationCount,
        value: MetricValue::Count(total_confirmed),
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: format!(
            "This word set confirmed {total_confirmed} analysis/analyses in total (HermitCrab \
             confirmation over {total_candidates} proposed candidate(s)). Confirmation work remains \
             first-class health evidence independent of correctness (R6)."
        ),
        remedies: Vec::new(),
        override_record: None,
    }];

    if total_candidates > 0 {
        let rejected = total_candidates.saturating_sub(total_confirmed);
        let share = rejected as f64 / total_candidates as f64;
        findings.push(HealthFinding {
            code: FindingCode::ConfirmationWork,
            severity: Severity::Info,
            phase: Phase::Apply,
            affected: Vec::new(),
            metric: Metric::RejectionShare,
            value: MetricValue::Ratio(share),
            provenance: ValueProvenance::Observed,
            threshold: None,
            explanation: format!(
                "Of {total_candidates} proposed candidate(s), {rejected} did not survive HermitCrab \
                 confirmation ({:.1}% rejection share). PanGloss is deliberately propose-and-confirm \
                 (R6): a high rejection share is expected overapproximation evidence, not itself a \
                 correctness problem, but remains first-class health evidence.",
                share * 100.0,
            ),
            remedies: Vec::new(),
            override_record: None,
        });
    }
    findings
}

/// Compile-time observed health via a standalone profiled compile; duplicated rather than shared with `pack.rs::run_pack`'s own equivalent section since each composes it into a different report shape.
fn compile_time_findings(grammar: &Grammar) -> Vec<HealthFinding> {
    let (proposer_result, compile_profile) = FomaProposer::new_with_profile(grammar);
    let report = match &proposer_result {
        Ok(proposer) => evaluate_health(
            None,
            Some(&proposer.report),
            &[],
            &[],
            Some(&compile_profile),
        ),
        Err(FomaError::LexcCompileFailed(report)) => {
            evaluate_health(None, Some(report), &[], &[], Some(&compile_profile))
        }
        Err(_) => evaluate_health(None, None, &[], &[], Some(&compile_profile)),
    };
    report.findings
}

/// Composes preflight + compile-time findings, plus apply-side findings only when `words` is `Some`; factored out from `run_fst_health` so the honest no-words contract is directly unit-testable without file I/O.
fn build_health_report(
    grammar: &Grammar,
    words: Option<&[String]>,
) -> Result<HealthReport, String> {
    let mut findings = preflight_findings(grammar);
    findings.extend(compile_time_findings(grammar));
    if let Some(words) = words {
        findings.extend(measure_apply_side(grammar, words)?);
    }
    Ok(HealthReport::new(findings))
}

/// `pangloss fst-health <grammar> [<words.txt>] [<out.json>]`; `<out.json>` omitted writes the canonical JSON to stdout instead of a file, matching this crate's stdout/stderr split.
pub fn run_fst_health(args: &[String]) -> Result<(), String> {
    let (grammar_path, words_path, out_path): (&str, Option<&str>, Option<&str>) = match args {
        [g] => (g.as_str(), None, None),
        [g, w] => (g.as_str(), Some(w.as_str()), None),
        [g, w, o] => (g.as_str(), Some(w.as_str()), Some(o.as_str())),
        _ => {
            return Err("usage: fst-health <grammar> [<words.txt>] [<out.json>]".to_string());
        }
    };

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);

    let words: Option<Vec<String>> = match words_path {
        Some(words_path) => Some(
            fs::read_to_string(words_path)
                .map_err(|e| format!("read {words_path}: {e}"))?
                .lines()
                .map(|w| w.trim().to_string())
                .filter(|w| !w.is_empty())
                .collect(),
        ),
        None => None,
    };
    let report = build_health_report(&grammar, words.as_deref())?;
    let json = report
        .to_json()
        .map_err(|e| format!("serialize health report: {e}"))?;

    match out_path {
        Some(path) => {
            fs::write(path, &json).map_err(|e| format!("write {path}: {e}"))?;
        }
        None => println!("{json}"),
    }

    eprintln!(
        "fst-health complete: {} finding(s), admission={:?}.{}",
        report.findings.len(),
        report.admission(),
        if words.is_some() {
            String::new()
        } else {
            " No <words.txt> given: apply-side proposal/confirmation/duplicate-analysis findings \
              were NOT measured (preflight + compile-time findings only)."
                .to_string()
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "pangloss-cli-fst-health-test-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// Same clean, `Admit`-verdict shape `pack.rs`'s own `CLEAN_GRAMMAR_XML` uses.
    const CLEAN_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>FstHealthCleanFixture</Name>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="e1">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    fn grammar(xml: &str) -> Grammar {
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture grammar failed to load: {e}"))
    }

    fn run_fst_health_raw(
        tag: &str,
        grammar_xml: &str,
        words: Option<&[&str]>,
    ) -> (Result<(), String>, std::path::PathBuf, std::path::PathBuf) {
        let dir = scratch_dir(tag);
        let grammar_path = dir.join("grammar.xml");
        let words_path = dir.join("words.txt");
        let out_path = dir.join("health.json");
        std::fs::write(&grammar_path, grammar_xml).expect("write grammar");

        let mut args: Vec<String> = vec![grammar_path.to_string_lossy().into_owned()];
        if let Some(words) = words {
            std::fs::write(&words_path, words.join("\n")).expect("write words");
            args.push(words_path.to_string_lossy().into_owned());
            args.push(out_path.to_string_lossy().into_owned());
        }
        (run_fst_health(&args), out_path, words_path)
    }

    #[test]
    fn fst_health_no_words_writes_no_apply_side_findings() {
        let (result, _out_path, _words_path) =
            run_fst_health_raw("no-words", CLEAN_GRAMMAR_XML, None);
        assert!(
            result.is_ok(),
            "no-words invocation must succeed: {result:?}"
        );

        // Precise honesty check: `words: None` must produce no apply-side finding at all, exercised directly against `build_health_report` rather than by scraping stdout.
        let g = grammar(CLEAN_GRAMMAR_XML);
        let report = build_health_report(&g, None).expect("build_health_report must succeed");
        assert!(
            !report.findings.iter().any(|f| {
                matches!(
                    f.code,
                    FindingCode::ProposalVolume
                        | FindingCode::ConfirmationWork
                        | FindingCode::DuplicateAnalysisOverlap
                )
            }),
            "a no-words report must never contain a proposal/confirmation/duplicate-analysis \
             finding: {:?}",
            report.findings
        );
    }

    #[test]
    fn fst_health_writes_valid_json_that_round_trips() {
        let (result, out_path, _words_path) =
            run_fst_health_raw("roundtrip", CLEAN_GRAMMAR_XML, Some(&["kat", "zzzz"]));
        assert!(result.is_ok(), "fst-health must succeed: {result:?}");

        let json = std::fs::read_to_string(&out_path).expect("read health.json");
        let report = HealthReport::from_json(&json).expect("health.json must parse");
        assert_eq!(
            report.schema_version,
            pg_foma::health::HEALTH_SCHEMA_VERSION
        );

        // Round-trip: re-serializing the parsed report must reproduce the same JSON exactly.
        let reserialized = report.to_json().expect("re-serialize");
        assert_eq!(reserialized, json, "health.json must round-trip losslessly");
    }

    #[test]
    fn fst_health_with_words_populates_proposal_and_confirmation_findings() {
        let (result, out_path, _words_path) =
            run_fst_health_raw("with-words", CLEAN_GRAMMAR_XML, Some(&["kat"]));
        assert!(result.is_ok(), "fst-health must succeed: {result:?}");
        let json = std::fs::read_to_string(&out_path).expect("read health.json");
        let report = HealthReport::from_json(&json).expect("health.json must parse");

        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == FindingCode::ProposalVolume
                    && f.metric == Metric::ProposalCandidateCount),
            "expected a populated ProposalVolume finding: {:?}",
            report.findings
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == FindingCode::ConfirmationWork
                    && f.metric == Metric::ConfirmationCount),
            "expected a populated ConfirmationWork/ConfirmationCount finding: {:?}",
            report.findings
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == FindingCode::ConfirmationWork
                    && f.metric == Metric::RejectionShare),
            "expected a populated ConfirmationWork/RejectionShare finding: {:?}",
            report.findings
        );
    }

    /// Direct unit coverage of the dedup logic: three identical `WordAnalysis` values, constructed directly rather than through a compiled grammar, must yield one finding with ratio `2/3`.
    #[test]
    fn duplicate_analysis_findings_reports_real_ratio_for_repeated_structured_analyses() {
        let wa = WordAnalysis {
            morpheme_ids: vec![7],
            root_morpheme_index: 0,
            pos_id: None,
            syn_fs: pg_featstruct::FeatureStruct::EMPTY,
            mpr: pg_grammar::model::MprSet::default(),
            guessed: false,
            provenance: pg_parse::AnalysisProvenance::Grammar,
            supplied_root: None,
            morpheme_roots: vec![None],
        };
        let structured = vec![wa.clone(), wa.clone(), wa];
        let findings = duplicate_analysis_findings("synthetic-word", &structured);

        let ratio_finding = findings
            .iter()
            .find(|f| f.metric == Metric::DuplicateAnalysisRatio)
            .expect("expected a DuplicateAnalysisRatio finding");
        assert_eq!(ratio_finding.code, FindingCode::DuplicateAnalysisOverlap);
        assert_eq!(ratio_finding.value, MetricValue::Ratio(2.0 / 3.0));
        assert_eq!(ratio_finding.affected, vec!["synthetic-word".to_string()]);

        let count_finding = findings
            .iter()
            .find(|f| f.metric == Metric::DuplicateAnalysisCount)
            .expect("expected a DuplicateAnalysisCount finding");
        assert_eq!(count_finding.value, MetricValue::Count(2));
    }

    #[test]
    fn duplicate_analysis_findings_empty_for_all_distinct_analyses() {
        let mk = |id: u32| WordAnalysis {
            morpheme_ids: vec![id],
            root_morpheme_index: 0,
            pos_id: None,
            syn_fs: pg_featstruct::FeatureStruct::EMPTY,
            mpr: pg_grammar::model::MprSet::default(),
            guessed: false,
            provenance: pg_parse::AnalysisProvenance::Grammar,
            supplied_root: None,
            morpheme_roots: vec![None],
        };
        let structured = vec![mk(1), mk(2), mk(3)];
        assert!(duplicate_analysis_findings("synthetic-word", &structured).is_empty());
    }
}
