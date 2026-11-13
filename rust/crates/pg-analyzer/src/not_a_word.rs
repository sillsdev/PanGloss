//! "Not a word" tokens: segmented as words (ADR 0011), then filtered out before any check (ADR 0016).

use crate::text::{OccurrenceOutcome, OccurrenceResult, Span};
use crate::SegmentationProfile;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;

/// Which default rule decided, so a summary says whether the writing system's letters were known.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// Every character must be in the main or auxiliary exemplar set, and one must be a letter.
    #[default]
    ValidCharacters,
    /// The writing system has no exemplar sets: a token with a numeric character or no letter.
    FallbackDigitOrNoLetter,
    /// The default rule is switched off; only the reject pattern and style exclusion apply.
    Off,
}

/// Why a token is not a word.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum NotAWordReason {
    /// It has a character outside the valid set (or, in the fallback, a digit or no letter).
    Character,
    /// The extra reject regex matched it.
    RejectPattern,
    /// It sits in a Chapter Number or Verse Number styled run.
    Style,
}

/// The verdict on one token: the reason and the offending character, when there is one.
pub(crate) struct Verdict {
    pub(crate) reason: NotAWordReason,
    pub(crate) character: Option<char>,
}

pub(crate) struct TokenFilter {
    pub(crate) rule: Rule,
    valid: BTreeSet<char>,
    reject: Option<Regex>,
    accept_words: BTreeSet<String>,
    accept_patterns: Vec<Regex>,
}

fn fold(word: &str) -> String {
    word.nfc().collect::<String>().to_lowercase()
}

fn compile(name: &str, pattern: &str) -> Result<Regex, String> {
    Regex::new(pattern).map_err(|e| format!("invalid {name} pattern: {e}"))
}

impl TokenFilter {
    /// Run lists and the run's reject pattern add to the profile's; accept regexes are anchored.
    pub(crate) fn new(
        profile: &SegmentationProfile,
        options: &crate::AnalyzeOptions,
    ) -> Result<Self, String> {
        let valid: BTreeSet<char> = profile
            .word_forming
            .iter()
            .chain(&profile.auxiliary)
            .flat_map(|c| fold(&c.to_string()).chars().collect::<Vec<_>>())
            .collect();
        let rule = if options.no_character_rule {
            Rule::Off
        } else if profile.word_forming.is_empty() && profile.auxiliary.is_empty() {
            Rule::FallbackDigitOrNoLetter
        } else {
            Rule::ValidCharacters
        };
        let rejects: Vec<String> = [&profile.reject_pattern, &options.reject_pattern]
            .into_iter()
            .flatten()
            .filter(|p| !p.is_empty())
            .map(|p| format!("(?:{p})"))
            .collect();
        let reject = (!rejects.is_empty())
            .then(|| compile("reject", &rejects.join("|")))
            .transpose()?;
        let accept_words = profile
            .accept_words
            .iter()
            .chain(&options.accept_words)
            .map(|w| fold(w))
            .collect();
        let accept_patterns = profile
            .accept_patterns
            .iter()
            .chain(&options.accept_patterns)
            .map(|p| compile("accept", &format!("^(?:{p})$")))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            rule,
            valid,
            reject,
            accept_words,
            accept_patterns,
        })
    }

    /// `None` when the token is a word. Accept entries win over every other rule.
    pub(crate) fn verdict(&self, word: &str) -> Option<Verdict> {
        let folded = fold(word);
        if self.accept_words.contains(&folded)
            || self.accept_patterns.iter().any(|p| p.is_match(word))
        {
            return None;
        }
        if let Some(found) = self.reject.as_ref().and_then(|r| r.find(word)) {
            return Some(Verdict {
                reason: NotAWordReason::RejectPattern,
                character: found.as_str().chars().next(),
            });
        }
        // A token with no letter (a lone apostrophe or hyphen the set allows) is never a word.
        let no_letter = || {
            (!word.chars().any(char::is_alphabetic))
                .then(|| word.chars().next())
                .flatten()
        };
        let character = match self.rule {
            Rule::Off => None,
            Rule::FallbackDigitOrNoLetter => {
                word.chars().find(|c| c.is_numeric()).or_else(no_letter)
            }
            Rule::ValidCharacters => word
                .nfc()
                .find(|c| !c.to_lowercase().all(|l| self.is_valid(l)))
                .or_else(no_letter),
        };
        character.map(|c| Verdict {
            reason: NotAWordReason::Character,
            character: Some(c),
        })
    }

    /// A character is valid when it, or every piece of its decomposition, is in the set.
    fn is_valid(&self, c: char) -> bool {
        self.valid.contains(&c) || {
            let mut parts = std::iter::once(c).nfd().peekable();
            parts.peek().is_some() && parts.all(|p| self.valid.contains(&p))
        }
    }
}

/// The result for a token that is not a word: no check ran and no other count includes it.
pub(crate) fn result(wordform: String, span: Span, verdict: Verdict) -> OccurrenceResult {
    OccurrenceResult {
        wordform,
        span,
        outcome: OccurrenceOutcome::NotAWord,
        not_a_word: Some(NotAWordInfo {
            reason: verdict.reason,
            character: verdict.character.map(String::from),
        }),
        valid_languages: Vec::new(),
        misspelled_languages: Vec::new(),
        checks: Vec::new(),
        readings: Vec::new(),
        alternative_split: Vec::new(),
        limits: Vec::new(),
    }
}

/// Why one Occurrence is not a word.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotAWordInfo {
    pub reason: NotAWordReason,
    /// The offending character; absent for a style exclusion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
}
