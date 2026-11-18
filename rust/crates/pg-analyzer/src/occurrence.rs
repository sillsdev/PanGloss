//! Checks one Occurrence: the primary split, then every Segmentation alternative.

use crate::alternatives::{generate, part_span};
use crate::dictionary::{DictionaryPack, Verdict};
use crate::text::{
    CheckStatus, EvidenceKind, LanguagePack, LimitReason, OccurrenceOutcome, OccurrenceResult,
    PackCheck, Part, Reading, ReadingKind, Span, SplitSupport,
};
use pg_parse::{ParseOptions, ParseOutcome};
use std::collections::BTreeMap;

pub(crate) fn check_occurrence(
    packs: &[LanguagePack<'_>],
    dictionaries: &[DictionaryPack],
    occurrence: crate::Occurrence,
    attempt: bool,
    max_alternatives: usize,
    all_readings: bool,
    max_suggestion_chars: usize,
) -> OccurrenceResult {
    let span = Span {
        code_points: occurrence.code_points,
        utf8: occurrence.utf8,
        utf16: occurrence.utf16,
    };
    let suggest = Some(max_suggestion_chars);
    let checks = pack_checks(packs, dictionaries, &occurrence.text, attempt, suggest);
    let primary = Reading {
        kind: ReadingKind::Primary,
        analysed: checks.iter().any(|c| c.status == CheckStatus::Valid),
        parts: vec![part(occurrence.text.clone(), span.clone(), checks.clone())],
        grammar_support: BTreeMap::new(),
    };
    let mut limits = Vec::new();
    let mut alternatives = Vec::new();
    if attempt
        && !(packs.is_empty() && dictionaries.is_empty())
        && (all_readings || !primary.analysed)
    {
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
                    let checks = pack_checks(packs, dictionaries, &word, true, None);
                    part(word, part_span(&occurrence.text, &span, range), checks)
                })
                .collect::<Vec<_>>();
            let analysed = parts.iter().all(|p| !p.valid_languages.is_empty());
            let grammar_support = if analysed {
                packs
                    .iter()
                    .map(|pack| {
                        let supported = parts
                            .iter()
                            .filter(|p| {
                                p.checks.iter().any(|c| {
                                    c.language == pack.language
                                        && c.evidence_kind == EvidenceKind::LanguagePackAnalysis
                                        && c.status == CheckStatus::Valid
                                })
                            })
                            .count();
                        let support = match supported {
                            0 => SplitSupport::AllContact,
                            n if n == parts.len() => SplitSupport::VernacularOnly,
                            _ => SplitSupport::Mixed,
                        };
                        (pack.language.clone(), support)
                    })
                    .collect()
            } else {
                BTreeMap::new()
            };
            alternatives.push(Reading {
                kind: ReadingKind::Alternative,
                parts,
                analysed,
                grammar_support,
            });
        }
        // Stable: generation order is kept inside each group.
        alternatives.sort_by_key(|r| !r.analysed);
    }
    let mut readings = vec![primary];
    readings.extend(alternatives);

    let primary_analysed = readings[0].analysed;
    let alternative_only_win = Reading::alternative_only_win(&readings);
    let analysed = primary_analysed || alternative_only_win;
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
    let mut valid_languages: Vec<String> = Vec::new();
    for (_, language) in valid {
        if !valid_languages.iter().any(|l| l == language) {
            valid_languages.push(language.to_owned());
        }
    }
    // Apply Q6′ and prefer a joined grammar miss to a different language's spelling flag.
    let mut misspelled_languages: Vec<String> = Vec::new();
    if !alternative_only_win {
        for check in &checks {
            let language = &check.language;
            if check.status == CheckStatus::Misspelled
                && !valid_languages.contains(language)
                && !misspelled_languages.contains(language)
                && (primary_analysed
                    || !checks.iter().any(|grammar| {
                        grammar.is_completed_grammar_miss() && grammar.language != *language
                    }))
            {
                misspelled_languages.push(language.clone());
            }
        }
    }
    // An unfinished grammar search outranks a spelling verdict: it is never reported as an error.
    let grammar_open = readings
        .iter()
        .flat_map(|r| &r.parts)
        .flat_map(|p| &p.checks)
        .any(|c| {
            c.evidence_kind == EvidenceKind::LanguagePackAnalysis
                && c.status == CheckStatus::Incomplete
        });
    let outcome = if analysed {
        OccurrenceOutcome::Valid
    } else if checks.is_empty() || checks.iter().any(|c| c.status == CheckStatus::Unattempted) {
        OccurrenceOutcome::Unattempted
    } else if !limits.is_empty() || grammar_open {
        OccurrenceOutcome::Incomplete
    } else if !misspelled_languages.is_empty() {
        OccurrenceOutcome::Misspelled
    } else if readings.iter().any(undecided) {
        OccurrenceOutcome::Incomplete
    } else {
        OccurrenceOutcome::NoAnalysis
    };
    OccurrenceResult {
        wordform: occurrence.text,
        span,
        outcome,
        valid_languages,
        misspelled_languages,
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

fn pack_checks(
    packs: &[LanguagePack<'_>],
    dictionaries: &[DictionaryPack],
    word: &str,
    attempt: bool,
    suggest: Option<usize>,
) -> Vec<PackCheck> {
    let grammar = packs.iter().enumerate().map(|(index, pack)| {
        if attempt {
            run_check(index + 1, pack, word)
        } else {
            unattempted(
                index + 1,
                &pack.language,
                EvidenceKind::LanguagePackAnalysis,
            )
        }
    });
    let dictionary = dictionaries.iter().enumerate().map(|(index, pack)| {
        let resource = 1 + packs.len() + index;
        if attempt {
            dictionary_check(resource, pack, word, suggest)
        } else {
            unattempted(
                resource,
                &pack.identity.language,
                EvidenceKind::DictionaryAcceptance,
            )
        }
    });
    grammar.chain(dictionary).collect()
}

fn unattempted(resource: usize, language: &str, evidence_kind: EvidenceKind) -> PackCheck {
    PackCheck {
        resource,
        language: language.to_owned(),
        evidence_kind,
        status: CheckStatus::Unattempted,
        analyses: 0,
        limits: Vec::new(),
        suggestions: Vec::new(),
        suggestion_methods: Vec::new(),
        unattempted_reason: Some("occurrence_limit".into()),
    }
}

/// `suggest` is `Some(max_chars)` only for the primary split: parts never yield misspellings.
fn dictionary_check(
    resource: usize,
    pack: &DictionaryPack,
    word: &str,
    suggest: Option<usize>,
) -> PackCheck {
    use EvidenceKind::{DictionaryAcceptance as Spelling, WordListMembership as Listed};
    let (evidence_kind, status, corrections, limits) = match pack.check(word, suggest) {
        Verdict::Accepted => (Spelling, CheckStatus::Valid, Vec::new(), Vec::new()),
        Verdict::Listed => (Listed, CheckStatus::Valid, Vec::new(), Vec::new()),
        Verdict::Rejected(corrections) => {
            let status = if corrections.is_empty() {
                CheckStatus::NoAnalysis
            } else {
                CheckStatus::Misspelled
            };
            (Spelling, status, corrections, Vec::new())
        }
        Verdict::SuggestionsCapped => (
            Spelling,
            CheckStatus::Incomplete,
            Vec::new(),
            vec![LimitReason::SuggestionsCapped],
        ),
    };
    let (suggestions, suggestion_methods) = corrections.into_iter().unzip();
    PackCheck {
        resource,
        language: pack.identity.language.clone(),
        evidence_kind,
        status,
        analyses: 0,
        limits,
        suggestions,
        suggestion_methods,
        unattempted_reason: None,
    }
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
        suggestions: Vec::new(),
        suggestion_methods: Vec::new(),
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
