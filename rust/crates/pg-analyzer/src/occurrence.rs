//! Checks one Occurrence: the primary split, then every Segmentation alternative.

use crate::alternatives::{generate, part_span};
use crate::text::{
    CheckStatus, EvidenceKind, LanguagePack, LimitReason, OccurrenceOutcome, OccurrenceResult,
    PackCheck, Part, Reading, ReadingKind, Span,
};
use pg_parse::{ParseOptions, ParseOutcome};

pub(crate) fn check_occurrence(
    packs: &[LanguagePack<'_>],
    occurrence: crate::Occurrence,
    attempt: bool,
    max_alternatives: usize,
    all_readings: bool,
) -> OccurrenceResult {
    let span = Span {
        code_points: occurrence.code_points,
        utf8: occurrence.utf8,
        utf16: occurrence.utf16,
    };
    let checks = pack_checks(packs, &occurrence.text, attempt);
    let primary = Reading {
        kind: ReadingKind::Primary,
        analysed: checks.iter().any(|c| c.status == CheckStatus::Valid),
        parts: vec![part(occurrence.text.clone(), span.clone(), checks.clone())],
    };
    let mut limits = Vec::new();
    let mut alternatives = Vec::new();
    if attempt && !packs.is_empty() && (all_readings || !primary.analysed) {
        let generated = generate(&occurrence.text, max_alternatives);
        if generated.capped {
            limits.push(LimitReason::AlternativesCapped);
        }
        let chars: Vec<char> = occurrence.text.chars().collect();
        for division in generated.divisions {
            let parts = division
                .into_iter()
                .map(|range| {
                    let word: String = chars[range.0..range.1].iter().collect();
                    let checks = pack_checks(packs, &word, true);
                    part(word, part_span(&occurrence.text, &span, range), checks)
                })
                .collect::<Vec<_>>();
            let analysed = parts.iter().all(|p| !p.valid_languages.is_empty());
            alternatives.push(Reading {
                kind: ReadingKind::Alternative,
                parts,
                analysed,
            });
        }
        // Stable: generation order is kept inside each group.
        alternatives.sort_by_key(|r| !r.analysed);
    }
    let mut readings = vec![primary];
    readings.extend(alternatives);

    let mut valid: Vec<(usize, &str)> = readings
        .iter()
        .filter(|r| r.analysed)
        .flat_map(|r| &r.parts)
        .flat_map(|p| &p.checks)
        .filter(|c| c.status == CheckStatus::Valid)
        .map(|c| (c.resource, c.language.as_str()))
        .collect();
    valid.sort();
    valid.dedup();
    let valid_languages: Vec<String> = valid.into_iter().map(|(_, l)| l.to_owned()).collect();
    let outcome = if readings.iter().any(|r| r.analysed) {
        OccurrenceOutcome::Valid
    } else if checks.is_empty() || checks.iter().any(|c| c.status == CheckStatus::Unattempted) {
        OccurrenceOutcome::Unattempted
    } else if !limits.is_empty() || readings.iter().any(undecided) {
        OccurrenceOutcome::Incomplete
    } else {
        OccurrenceOutcome::NoAnalysis
    };
    OccurrenceResult {
        wordform: occurrence.text,
        span,
        outcome,
        valid_languages,
        checks,
        readings,
        limits,
    }
}

/// A reading whose parts are all analysed or unfinished, with at least one unfinished.
fn undecided(reading: &Reading) -> bool {
    let open = |p: &Part| p.checks.iter().any(|c| c.status == CheckStatus::Incomplete);
    reading
        .parts
        .iter()
        .all(|p| !p.valid_languages.is_empty() || open(p))
        && reading.parts.iter().any(open)
}

fn part(wordform: String, span: Span, checks: Vec<PackCheck>) -> Part {
    let mut valid_languages: Vec<String> = Vec::new();
    for check in checks.iter().filter(|c| c.status == CheckStatus::Valid) {
        if !valid_languages.contains(&check.language) {
            valid_languages.push(check.language.clone());
        }
    }
    Part {
        wordform,
        span,
        valid_languages,
        checks,
    }
}

fn pack_checks(packs: &[LanguagePack<'_>], word: &str, attempt: bool) -> Vec<PackCheck> {
    packs
        .iter()
        .enumerate()
        .map(|(index, pack)| {
            if attempt {
                run_check(index + 1, pack, word)
            } else {
                PackCheck {
                    resource: index + 1,
                    language: pack.language.clone(),
                    evidence_kind: EvidenceKind::LanguagePackAnalysis,
                    status: CheckStatus::Unattempted,
                    analyses: 0,
                    limits: Vec::new(),
                    unattempted_reason: Some("occurrence_limit".into()),
                }
            }
        })
        .collect()
}

fn run_check(resource: usize, pack: &LanguagePack<'_>, word: &str) -> PackCheck {
    let outcome = pack.morpher.parse_word_opts(word, &ParseOptions::default());
    let limits = limits(&outcome);
    let analyses = outcome.structured.len();
    let status = if analyses > 0 {
        CheckStatus::Valid
    } else if limits.is_empty() {
        CheckStatus::NoAnalysis
    } else {
        CheckStatus::Incomplete
    };
    PackCheck {
        resource,
        language: pack.language.clone(),
        evidence_kind: EvidenceKind::LanguagePackAnalysis,
        status,
        analyses,
        limits,
        unattempted_reason: None,
    }
}

fn limits(outcome: &ParseOutcome) -> Vec<LimitReason> {
    [
        (outcome.capped, LimitReason::Capped),
        (outcome.capture_capped, LimitReason::CaptureCapped),
        (outcome.timed_out, LimitReason::TimedOut),
        (outcome.invalid_shape, LimitReason::InvalidShape),
    ]
    .into_iter()
    .filter_map(|(present, reason)| present.then_some(reason))
    .collect()
}
