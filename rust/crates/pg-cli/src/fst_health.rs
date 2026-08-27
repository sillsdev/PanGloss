//! `pangloss fst-health <grammar> [<words.txt>] [<out.json>]` composes characterization findings and — only when `<words.txt>` is given — apply-side measurement into one `HealthReport`, deduplicating `FomaOutcome::structured` by `WordAnalysis`'s structured identity rather than `result_signature`'s rendered-string equality, and computing rejection share as `(proposed - confirmed) / proposed` via `saturating_sub` to stay in `[0.0, 1.0]`.

use std::fs;

use pg_foma::characterization::characterization_findings;
use pg_foma::composite::FomaAnalyzer;
use pg_foma::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase, Severity, ValueProvenance,
};
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

/// One word's pre-dedup duplicate-analysis evidence: many copies still mean one semantic answer but expose an FST design problem, so this emits `Severity::Elevated` count/ratio findings, both empty when `structured` has no duplicate at all.
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
            severity: Severity::Elevated,
            phase: Phase::Apply,
            affected: vec![word.to_string()],
            metric: Metric::DuplicateAnalysisCount,
            value: MetricValue::Count(duplicate_count as u64),
            provenance: ValueProvenance::Observed,
            threshold: None,
            explanation: explanation.clone(),
            remedies: Vec::new(),
        },
        HealthFinding {
            code: FindingCode::DuplicateAnalysisOverlap,
            severity: Severity::Elevated,
            phase: Phase::Apply,
            affected: vec![word.to_string()],
            metric: Metric::DuplicateAnalysisRatio,
            value: MetricValue::Ratio(ratio),
            provenance: ValueProvenance::Observed,
            threshold: None,
            explanation,
            remedies: Vec::new(),
        },
    ]
}

/// Total distinct FST-propose candidates across every word measured; `Severity::Elevated` since proposal volume is evidence, not itself a problem.
fn proposal_volume_finding(total_candidates: u64) -> HealthFinding {
    HealthFinding {
        code: FindingCode::ProposalVolume,
        severity: Severity::Elevated,
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
    }
}

/// Total confirmed analyses plus the rejection share; the rejection finding is omitted entirely when `total_candidates` is zero, never a fabricated `0.0`.
fn confirmation_work_findings(total_candidates: u64, total_confirmed: u64) -> Vec<HealthFinding> {
    let mut findings = vec![HealthFinding {
        code: FindingCode::ConfirmationWork,
        severity: Severity::Elevated,
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
    }];

    if total_candidates > 0 {
        let rejected = total_candidates.saturating_sub(total_confirmed);
        let share = rejected as f64 / total_candidates as f64;
        findings.push(HealthFinding {
            code: FindingCode::ConfirmationWork,
            severity: Severity::Elevated,
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
        });
    }
    findings
}

/// Composes characterization findings plus apply-side findings only when `words` is `Some`; factored out from `run_fst_health` so the honest no-words contract is directly unit-testable without file I/O.
fn build_health_report(
    grammar: &Grammar,
    words: Option<&[String]>,
) -> Result<HealthReport, String> {
    let mut findings = characterization_findings(grammar);
    if let Some(words) = words {
        findings.extend(measure_apply_side(grammar, words)?);
    }
    Ok(HealthReport::new(findings))
}

/// The `admission (per-axis breakdown)` fragment of `run_fst_health`'s completion message.
fn render_admission_summary(report: &HealthReport) -> String {
    format!(
        "{:?} ({})",
        report.admission(),
        report.admission_by_class().render()
    )
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
        "fst-health complete: {} finding(s), admission={}.{}",
        report.findings.len(),
        render_admission_summary(&report),
        if words.is_some() {
            String::new()
        } else {
            " No <words.txt> given: apply-side proposal/confirmation/duplicate-analysis findings \
              were NOT measured (characterization findings only)."
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
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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

    fn run_fst_health_raw(tag: &str, grammar_xml: &str) -> Result<(), String> {
        let dir = scratch_dir(tag);
        let grammar_path = dir.join("grammar.xml");
        std::fs::write(&grammar_path, grammar_xml).expect("write grammar");

        let args: Vec<String> = vec![grammar_path.to_string_lossy().into_owned()];
        run_fst_health(&args)
    }

    #[test]
    fn fst_health_no_words_writes_no_apply_side_findings() {
        let result = run_fst_health_raw("no-words", CLEAN_GRAMMAR_XML);
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

    /// The printed fragment must name all four axes, not just the collapsed severity band.
    #[test]
    fn admission_summary_names_all_four_axes() {
        let g = grammar(CLEAN_GRAMMAR_XML);
        let report = build_health_report(&g, None).expect("build_health_report must succeed");
        let summary = render_admission_summary(&report);
        assert!(summary.contains("representability="), "{summary}");
        assert!(summary.contains("readiness="), "{summary}");
        assert!(summary.contains("containment="), "{summary}");
        assert!(summary.contains("process="), "{summary}");
    }

}
