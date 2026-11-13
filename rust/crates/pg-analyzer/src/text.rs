//! The analyzer entry point; the result format is `docs/text-analysis-format.md`.

use crate::alternatives::DEFAULT_MAX_ALTERNATIVES;
use crate::detect::{detect, Detection};
use crate::dictionary::DictionaryPack;
use crate::not_a_word::{self, NotAWordInfo, NotAWordReason, Rule, TokenFilter, Verdict};
use crate::occurrence::check_occurrence;
use crate::statistics::{summarize, Statistics};
use crate::{analyze, ProfileInput, SegmentationProfile, StyledRun};
use pg_parse::Morpher;
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
    /// Alternatives kept per Occurrence (default 16); past it, `LimitReason::AlternativesCapped`.
    pub max_alternatives: Option<usize>,
    /// Also enumerate alternatives when the primary split is already analysed.
    pub all_readings: bool,
    /// Longest form (default 64 chars) searched for suggestions; longer is `SuggestionsCapped`.
    pub max_suggestion_chars: Option<usize>,
    /// Switch the valid-character rule off; only the reject pattern and styles skip tokens.
    pub no_character_rule: bool,
    /// Extra reject regex; replaces the profile's. A token it matches is not a word.
    pub reject_pattern: Option<String>,
    /// Words that pass anyway, added to the profile's.
    pub accept_words: Vec<String>,
    /// Regexes whose matches pass anyway, added to the profile's.
    pub accept_patterns: Vec<String>,
    /// Return statistics, detection and resources only: `texts[].occurrences` stays empty.
    pub summary_only: bool,
}

/// Default for [`AnalyzeOptions::max_suggestion_chars`].
pub const DEFAULT_MAX_SUGGESTION_CHARS: usize = 64;

pub struct TextRequest<'a> {
    pub profile: ProfileInput<'a>,
    pub packs: &'a [LanguagePack<'a>],
    pub dictionaries: &'a [DictionaryPack],
    pub texts: &'a [TextInput<'a>],
    pub options: AnalyzeOptions,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    SegmentationProfile,
    LanguagePack,
    DictionaryPack,
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
    /// Version and licence of a Dictionary pack; absent for other resources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<String>,
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
    /// A Dictionary pack's Hunspell dictionary/affix acceptance. Never a parse.
    DictionaryAcceptance,
    /// Membership in a Dictionary pack's word list. Absence is no rejection.
    WordListMembership,
}

/// Why a check did not finish its search.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LimitReason {
    Capped,
    CaptureCapped,
    TimedOut,
    InvalidShape,
    /// The Occurrence has more Segmentation alternatives than the request allows.
    AlternativesCapped,
    /// A Dictionary pack did not search for suggestions: the form is over the length bound.
    SuggestionsCapped,
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
    /// A Dictionary pack rejected the Wordform and offered suggestions. Not valid.
    Misspelled,
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
    /// Correction suggestions behind a misspelled status. They never replace the text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    /// Why an unattempted check never ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unattempted_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceOutcome {
    /// At least one pack analysed it.
    Valid,
    /// No reading is analysed, and a Dictionary pack rejected the primary split with suggestions.
    Misspelled,
    /// Every check ran to completion and none analysed it.
    NoAnalysis,
    /// No pack analysed it and at least one search did not finish.
    Incomplete,
    /// No pack analysed it and a check never ran (no packs loaded counts).
    Unattempted,
    /// Rejected by the reject pattern; no check ran and no other count includes it.
    NotAWord,
}

/// The kind of character an analysed alternative splits at.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SplitCharacter {
    Hyphen,
    Apostrophe,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadingKind {
    /// The FieldWorks-rule split.
    Primary,
    /// A Segmentation alternative.
    Alternative,
}

/// One part of a reading: a piece of the Occurrence and every pack's evidence about it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Part {
    pub wordform: String,
    /// Source span in the same units as the Occurrence's.
    pub span: Span,
    pub valid_languages: Vec<String>,
    pub checks: Vec<PackCheck>,
}

/// One division of the Occurrence's source span.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reading {
    pub kind: ReadingKind,
    pub parts: Vec<Part>,
    /// The primary reading when a pack analysed it; an alternative when every part is analysed.
    pub analysed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OccurrenceResult {
    pub wordform: String,
    pub span: Span,
    pub outcome: OccurrenceOutcome,
    /// Why this is not a word; set exactly when `outcome` is `not_a_word`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_a_word: Option<NotAWordInfo>,
    /// Language codes of the packs that analysed any analysed reading, in pack order.
    pub valid_languages: Vec<String>,
    /// Dictionary languages that rejected the primary split with suggestions; never a valid one.
    #[serde(default)]
    pub misspelled_languages: Vec<String>,
    /// The primary reading's checks.
    pub checks: Vec<PackCheck>,
    /// Primary first, then alternatives with every part analysed, then the rest.
    #[serde(default)]
    pub readings: Vec<Reading>,
    /// Characters the first analysed alternative splits at, when only an alternative is analysed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternative_split: Vec<SplitCharacter>,
    /// Occurrence-level limits; pack limits sit on their checks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limits: Vec<LimitReason>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextResult {
    pub id: String,
    pub occurrences: Vec<OccurrenceResult>,
    pub statistics: Statistics,
    /// Languages this text seems to contain: a proposal, not a verdict on any Occurrence.
    #[serde(default)]
    pub detection: Detection,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextAnalysis {
    pub schema: String,
    pub profile: SegmentationProfile,
    /// The profile, then each pack, then each Dictionary pack, in request order.
    pub resources: Vec<ResourceIdentity>,
    pub texts: Vec<TextResult>,
    pub corpus: Statistics,
    /// The default rule that decided which tokens are not words.
    #[serde(default)]
    pub not_a_word_rule: Rule,
    /// Language detection over every text together.
    #[serde(default)]
    pub detection: Detection,
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
    let filter = TokenFilter::new(&profile, &request.options)?;
    let mut remaining = request.options.max_checked_occurrences;
    let max_alternatives = request
        .options
        .max_alternatives
        .unwrap_or(DEFAULT_MAX_ALTERNATIVES);
    let max_suggestion_chars = request
        .options
        .max_suggestion_chars
        .unwrap_or(DEFAULT_MAX_SUGGESTION_CHARS);
    let mut results = Vec::new();
    for input in request.texts {
        let segmented = analyze(input.text, ProfileInput::Profile(&profile), input.styles)?;
        let mut occurrences: Vec<OccurrenceResult> = segmented
            .occurrences
            .into_iter()
            .map(|o| {
                if let Some(verdict) = filter.verdict(&o.text) {
                    let span = span_of(&o);
                    return not_a_word::result(o.text, span, verdict);
                }
                let attempt = match remaining.as_mut() {
                    Some(0) => false,
                    Some(left) => {
                        *left -= 1;
                        true
                    }
                    None => true,
                };
                check_occurrence(
                    request.packs,
                    request.dictionaries,
                    o,
                    attempt,
                    max_alternatives,
                    request.options.all_readings,
                    max_suggestion_chars,
                )
            })
            .collect();
        occurrences.extend(segmented.labels.into_iter().map(|o| {
            let style = Verdict {
                reason: NotAWordReason::Style,
                character: None,
            };
            not_a_word::result(o.text.clone(), span_of(&o), style)
        }));
        occurrences.sort_by_key(|o| o.span.code_points);
        let statistics = summarize(&occurrences);
        let detection = detect(&occurrences, request.dictionaries);
        results.push(TextResult {
            id: input.id.clone(),
            statistics,
            detection,
            occurrences,
        });
    }
    let mut resources = vec![ResourceIdentity {
        kind: ResourceKind::SegmentationProfile,
        source_id: profile.source_id.clone(),
        source_sha256: profile.source_sha256.clone(),
        language: None,
        version: None,
        licence: None,
    }];
    resources.extend(request.packs.iter().map(|p| ResourceIdentity {
        kind: ResourceKind::LanguagePack,
        source_id: p.source_id.clone(),
        source_sha256: p.source_sha256.clone(),
        language: Some(p.language.clone()),
        version: None,
        licence: None,
    }));
    resources.extend(request.dictionaries.iter().map(|d| ResourceIdentity {
        kind: ResourceKind::DictionaryPack,
        source_id: d.identity.source_id.clone(),
        source_sha256: d.identity.source_sha256.clone(),
        language: Some(d.identity.language.clone()),
        version: Some(d.identity.version.clone()),
        licence: Some(d.identity.licence.clone()),
    }));
    let corpus = summarize(results.iter().flat_map(|t| t.occurrences.iter()));
    let detection = detect(
        results.iter().flat_map(|t| t.occurrences.iter()),
        request.dictionaries,
    );
    if request.options.summary_only {
        for text in &mut results {
            text.occurrences = Vec::new();
        }
    }
    Ok(TextAnalysis {
        schema: SCHEMA.into(),
        profile,
        resources,
        texts: results,
        corpus,
        not_a_word_rule: filter.rule,
        detection,
    })
}

fn clone_input<'a>(input: &ProfileInput<'a>) -> ProfileInput<'a> {
    match input {
        ProfileInput::Ldml { identity, xml } => ProfileInput::Ldml { identity, xml },
        ProfileInput::Json(json) => ProfileInput::Json(json),
        ProfileInput::Profile(p) => ProfileInput::Profile(p),
    }
}

fn span_of(o: &crate::Occurrence) -> Span {
    Span {
        code_points: o.code_points,
        utf8: o.utf8,
        utf16: o.utf16,
    }
}
