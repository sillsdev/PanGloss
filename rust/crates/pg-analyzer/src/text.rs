//! The analyzer entry point; the result format is `docs/text-analysis-format.md`.

use crate::statistics::{summarize, Statistics};
use crate::{analyze, ProfileInput, SegmentationProfile, StyledRun};
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Schema identifier written to every result.
pub const SCHEMA: &str = "pangloss-text-analysis/v1";

/// A loaded grammar and the identity its evidence is attributed to.
pub struct LanguagePack<'a> {
    pub language: String,
    pub source_id: String,
    pub source_sha256: String,
    pub morpher: &'a Morpher<'a>,
}

/// One text of a corpus. Spans are relative to this text.
pub struct TextInput<'a> {
    pub id: String,
    pub text: &'a str,
    pub styles: &'a [StyledRun],
}

/// Bounds on one request.
#[derive(Clone, Debug, Default)]
pub struct AnalyzeOptions {
    /// Occurrences past this count, over all texts, are reported unattempted.
    pub max_checked_occurrences: Option<usize>,
}

pub struct TextRequest<'a> {
    pub profile: ProfileInput<'a>,
    pub packs: &'a [LanguagePack<'a>],
    pub texts: &'a [TextInput<'a>],
    pub options: AnalyzeOptions,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    SegmentationProfile,
    LanguagePack,
}

/// A resource that shaped the result, with enough identity to reproduce it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceIdentity {
    pub kind: ResourceKind,
    pub source_id: String,
    pub source_sha256: String,
    /// The language code of a Language Pack; absent for a Segmentation profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Half-open source range in each explicit unit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Span {
    pub code_points: [usize; 2],
    pub utf8: [usize; 2],
    pub utf16: [usize; 2],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// A HermitCrab structured analysis from a Language Pack grammar.
    LanguagePackAnalysis,
}

/// Why a check did not finish its search.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LimitReason {
    Capped,
    CaptureCapped,
    TimedOut,
    InvalidShape,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// The pack analysed the Wordform.
    Valid,
    /// The search finished and found nothing.
    NoAnalysis,
    /// The search did not finish and found nothing; this is not a failure.
    Incomplete,
    /// The check was never run.
    Unattempted,
}

/// One pack's evidence about one Occurrence.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackCheck {
    /// Index into [`TextAnalysis::resources`].
    pub resource: usize,
    pub language: String,
    pub evidence_kind: EvidenceKind,
    pub status: CheckStatus,
    /// Parser analyses behind a valid status. Not a count of anything else.
    pub analyses: usize,
    /// Limits that stopped the search; a valid check can still carry them.
    pub limits: Vec<LimitReason>,
    /// Why an unattempted check never ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unattempted_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceOutcome {
    /// At least one pack analysed it.
    Valid,
    /// Every check ran to completion and none analysed it.
    NoAnalysis,
    /// No pack analysed it and at least one search did not finish.
    Incomplete,
    /// No pack analysed it and a check never ran (no packs loaded counts).
    Unattempted,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OccurrenceResult {
    pub wordform: String,
    pub span: Span,
    pub outcome: OccurrenceOutcome,
    /// Language codes of the packs that analysed it, in pack order.
    pub valid_languages: Vec<String>,
    pub checks: Vec<PackCheck>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextResult {
    pub id: String,
    pub occurrences: Vec<OccurrenceResult>,
    pub statistics: Statistics,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextAnalysis {
    pub schema: String,
    pub profile: SegmentationProfile,
    /// The profile, then each pack in request order.
    pub resources: Vec<ResourceIdentity>,
    pub texts: Vec<TextResult>,
    pub corpus: Statistics,
}

/// Segment every text, check each Occurrence against every pack, and count.
pub fn analyze_text(request: &TextRequest<'_>) -> Result<TextAnalysis, String> {
    let mut languages = BTreeSet::new();
    for pack in request.packs {
        if pack.language.is_empty() || pack.source_id.is_empty() || pack.source_sha256.is_empty() {
            return Err("a Language Pack needs a language, source identity and hash".into());
        }
        if !languages.insert(pack.language.as_str()) {
            return Err(format!(
                "language {} has more than one Language Pack",
                pack.language
            ));
        }
    }
    let mut texts = BTreeSet::new();
    if !request.texts.iter().all(|t| texts.insert(t.id.as_str())) {
        return Err("text ids must be unique".into());
    }

    // Resolve the profile once so LDML is read once and every text shares one identity.
    let profile = analyze("", clone_input(&request.profile), &[])?.profile;
    let mut remaining = request.options.max_checked_occurrences;
    let mut results = Vec::new();
    for input in request.texts {
        let segmented = analyze(input.text, ProfileInput::Profile(&profile), input.styles)?;
        let occurrences: Vec<OccurrenceResult> = segmented
            .occurrences
            .into_iter()
            .map(|o| {
                let attempt = match remaining.as_mut() {
                    Some(0) => false,
                    Some(left) => {
                        *left -= 1;
                        true
                    }
                    None => true,
                };
                check_occurrence(request.packs, o, attempt)
            })
            .collect();
        results.push(TextResult {
            id: input.id.clone(),
            statistics: summarize(&occurrences),
            occurrences,
        });
    }
    let mut resources = vec![ResourceIdentity {
        kind: ResourceKind::SegmentationProfile,
        source_id: profile.source_id.clone(),
        source_sha256: profile.source_sha256.clone(),
        language: None,
    }];
    resources.extend(request.packs.iter().map(|p| ResourceIdentity {
        kind: ResourceKind::LanguagePack,
        source_id: p.source_id.clone(),
        source_sha256: p.source_sha256.clone(),
        language: Some(p.language.clone()),
    }));
    let corpus = summarize(results.iter().flat_map(|t| t.occurrences.iter()));
    Ok(TextAnalysis {
        schema: SCHEMA.into(),
        profile,
        resources,
        texts: results,
        corpus,
    })
}

fn clone_input<'a>(input: &ProfileInput<'a>) -> ProfileInput<'a> {
    match input {
        ProfileInput::Ldml { identity, xml } => ProfileInput::Ldml { identity, xml },
        ProfileInput::Json(json) => ProfileInput::Json(json),
        ProfileInput::Profile(p) => ProfileInput::Profile(p),
    }
}

fn check_occurrence(
    packs: &[LanguagePack<'_>],
    occurrence: crate::Occurrence,
    attempt: bool,
) -> OccurrenceResult {
    let checks: Vec<PackCheck> = packs
        .iter()
        .enumerate()
        .map(|(index, pack)| {
            if attempt {
                run_check(index + 1, pack, &occurrence.text)
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
        .collect();
    let valid_languages: Vec<String> = checks
        .iter()
        .filter(|c| c.status == CheckStatus::Valid)
        .map(|c| c.language.clone())
        .collect();
    let any = |status| checks.iter().any(|c| c.status == status);
    let outcome = if !valid_languages.is_empty() {
        OccurrenceOutcome::Valid
    } else if checks.is_empty() || any(CheckStatus::Unattempted) {
        OccurrenceOutcome::Unattempted
    } else if any(CheckStatus::Incomplete) {
        OccurrenceOutcome::Incomplete
    } else {
        OccurrenceOutcome::NoAnalysis
    };
    OccurrenceResult {
        wordform: occurrence.text,
        span: Span {
            code_points: occurrence.code_points,
            utf8: occurrence.utf8,
            utf16: occurrence.utf16,
        },
        outcome,
        valid_languages,
        checks,
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
