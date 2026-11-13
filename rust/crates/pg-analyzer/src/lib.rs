#![forbid(unsafe_code)]

mod alternatives;
mod detect;
mod dictionary;
mod not_a_word;
mod occurrence;
mod profile;
mod statistics;
mod text;
mod unicode;
mod usfm;

pub use detect::{
    Detection, LanguageProposal, ProposalStatus, MIN_DECISIVE_OCCURRENCES, MIN_NGRAM_CHARS,
    MIN_NGRAM_COVERAGE_PERCENT, MIN_NGRAM_MARGIN_PERCENT,
};
pub use dictionary::{DictionaryIdentity, DictionaryPack};
pub use not_a_word::{NotAWordInfo, NotAWordReason, Rule};
use serde::{Deserialize, Serialize};
pub use statistics::{
    AlternativeCounts, LanguageCounts, NotAWordCounts, NotAWordForm, Pair, Statistics,
};
pub use text::{
    analyze_text, AnalyzeOptions, CheckStatus, EvidenceKind, LanguagePack, LimitReason,
    OccurrenceOutcome, OccurrenceResult, PackCheck, Part, Reading, ReadingKind, ResourceIdentity,
    ResourceKind, Span, SplitCharacter, TextAnalysis, TextInput, TextRequest, TextResult, SCHEMA,
};
pub use usfm::usfm_styles;

/// A profile derived from LDML or supplied as its serialized representation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SegmentationProfile {
    pub version: u32,
    pub source_id: String,
    pub source_sha256: String,
    pub word_forming: Vec<char>,
    /// The auxiliary exemplar set: valid letters that do not join runs by themselves.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auxiliary: Vec<char>,
    /// Extra reject regex: a token it matches is not a word, unless an accept entry matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_pattern: Option<String>,
    /// Words that pass the valid-character check anyway (case-folded, NFC).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accept_words: Vec<String>,
    /// Regexes whose matches pass the valid-character check anyway.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accept_patterns: Vec<String>,
}

/// Source for the profile used by an analysis.
pub enum ProfileInput<'a> {
    Ldml { identity: &'a str, xml: &'a str },
    Json(&'a str),
    Profile(&'a SegmentationProfile),
}

/// Style applied to a half-open source span, in Unicode scalar-value offsets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StyledRun {
    pub start: usize,
    pub end: usize,
    pub style: String,
    #[serde(default)]
    pub profile: Option<SegmentationProfile>,
}

/// An occurrence's half-open offsets in all three explicit units.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Occurrence {
    pub text: String,
    pub code_points: [usize; 2],
    pub utf8: [usize; 2],
    pub utf16: [usize; 2],
}

/// Segmentation result, including the exact profile that produced it.
#[derive(Debug, Serialize, Deserialize)]
pub struct Analysis {
    pub profile: SegmentationProfile,
    pub occurrences: Vec<Occurrence>,
    /// Runs of non-space characters inside Chapter Number and Verse Number styled runs. They are
    /// never occurrences; they are returned so a caller can report them as not-a-word.
    #[serde(default)]
    pub labels: Vec<Occurrence>,
}

/// Style names whose characters are never word-forming.
const NON_WORD_STYLES: [&str; 3] = ["Chapter Number", "Verse Number", usfm::MARKER_STYLE];

/// Segment original text into maximal WordMaker runs. Styles are non-overlapping scalar ranges.
/// Chapter Number and Verse Number runs exclude all characters, including letters.
pub fn analyze(
    text: &str,
    input: ProfileInput<'_>,
    styles: &[StyledRun],
) -> Result<Analysis, String> {
    let profile = match input {
        ProfileInput::Ldml { identity, xml } => profile::from_ldml(identity, xml)?,
        ProfileInput::Json(json) => serde_json::from_str(json).map_err(|e| e.to_string())?,
        ProfileInput::Profile(p) => p.clone(),
    };
    validate_profile(&profile)?;
    let length = text.chars().count();
    let mut end = 0;
    for run in styles {
        if run.start < end || run.end <= run.start || run.end > length {
            return Err(
                "style ranges must be nonempty, ordered, non-overlapping, and within text".into(),
            );
        }
        if let Some(p) = &run.profile {
            validate_profile(p)?;
        }
        end = run.end;
    }
    let mut occurrences = Vec::new();
    let mut labels = Vec::new();
    let mut label_start: Option<(usize, usize, usize)> = None;
    let mut start: Option<(usize, usize, usize)> = None;
    let mut utf16 = 0;
    let mut style_index = 0;
    for (cp, (byte, ch)) in text
        .char_indices()
        .chain(std::iter::once((text.len(), '\0')))
        .enumerate()
    {
        while style_index < styles.len() && styles[style_index].end <= cp {
            style_index += 1;
        }
        let label = styles
            .get(style_index)
            .is_some_and(|r| r.start <= cp && NON_WORD_STYLES.contains(&r.style.as_str()));
        let number = label
            && styles
                .get(style_index)
                .is_some_and(|r| r.style != usfm::MARKER_STYLE)
            && byte < text.len()
            && !ch.is_whitespace();
        if number && label_start.is_none() {
            label_start = Some((cp, byte, utf16));
        }
        if !number {
            if let Some((c, b, u)) = label_start.take() {
                labels.push(Occurrence {
                    text: text[b..byte].to_owned(),
                    code_points: [c, cp],
                    utf8: [b, byte],
                    utf16: [u, utf16],
                });
            }
        }
        let active_profile = styles
            .get(style_index)
            .filter(|r| r.start <= cp)
            .and_then(|r| r.profile.as_ref())
            .unwrap_or(&profile);
        let word = byte < text.len()
            && !label
            && ch != '\u{200b}'
            && (active_profile.word_forming.contains(&ch) || unicode::word_category(ch));
        if word && start.is_none() {
            start = Some((cp, byte, utf16));
        }
        if !word {
            if let Some((c, b, u)) = start.take() {
                occurrences.push(Occurrence {
                    text: text[b..byte].to_owned(),
                    code_points: [c, cp],
                    utf8: [b, byte],
                    utf16: [u, utf16],
                });
            }
        }
        utf16 += ch.len_utf16();
    }
    Ok(Analysis {
        profile,
        occurrences,
        labels,
    })
}

#[cfg(test)]
mod tests;

fn validate_profile(profile: &SegmentationProfile) -> Result<(), String> {
    if profile.version != 1
        || profile.source_id.is_empty()
        || profile.source_sha256.len() != 64
        || !profile.source_sha256.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("invalid segmentation profile version, source identity, or SHA-256".into());
    }
    Ok(())
}
