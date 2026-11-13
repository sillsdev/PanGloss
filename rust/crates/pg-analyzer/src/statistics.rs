use crate::not_a_word::NotAWordReason;
use crate::text::{CheckStatus, OccurrenceOutcome, OccurrenceResult, SplitCharacter};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Occurrences and distinct Wordforms, always reported side by side and never merged.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pair {
    pub occurrences: usize,
    pub wordforms: usize,
}

/// Counts for one language code; valid beats misspelled beats unfinished beats no-analysis.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageCounts {
    pub valid: Pair,
    /// Rejected with suggestions by a pack of this language; counted even if valid elsewhere.
    #[serde(default)]
    pub misspelled: Pair,
    /// Valid in this language and in at least one other.
    #[serde(default)]
    pub ambiguous: Pair,
    /// Every check for this language finished and found nothing.
    #[serde(default)]
    pub no_analysis: Pair,
}

/// One distinct not-a-word form and how often it occurs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotAWordForm {
    pub wordform: String,
    pub reason: NotAWordReason,
    /// The first offending character; absent for a style exclusion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    pub occurrences: usize,
}

/// Tokens that are not words. They are outside `total` and every other count.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotAWordCounts {
    pub total: Pair,
    /// Skipped for a character outside the valid set, grouped by the first offending character.
    pub by_character: BTreeMap<String, Pair>,
    /// Skipped because the extra reject regex matched.
    pub reject_pattern: Pair,
    /// Skipped because they sit in a Chapter Number or Verse Number styled run.
    pub style: Pair,
    /// Every distinct (wordform, reason, character), most frequent first.
    pub forms: Vec<NotAWordForm>,
}

/// Valid Occurrences whose primary split is not analysed but a hyphen or apostrophe alternative is.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AlternativeCounts {
    pub total: Pair,
    /// Split at a hyphen. An Occurrence split at both counts in both and once in `total`.
    pub hyphen: Pair,
    pub apostrophe: Pair,
}

/// Counts for one text or a corpus; see `docs/text-analysis-format.md` for what each field counts.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Statistics {
    pub total: Pair,
    pub valid: Pair,
    /// No valid evidence, and a Dictionary pack offered suggestions. Part of the partition.
    #[serde(default)]
    pub misspelled: Pair,
    pub no_analysis: Pair,
    pub incomplete: Pair,
    pub unattempted: Pair,
    pub ambiguous: Pair,
    pub languages: BTreeMap<String, LanguageCounts>,
    /// Not-a-word Occurrences; `total` and the outcome counts above exclude them.
    #[serde(default)]
    pub not_a_word: NotAWordCounts,
    /// Part of `valid`: analysed only through a segmentation alternative.
    #[serde(default)]
    pub valid_via_alternative: AlternativeCounts,
}

#[derive(Default)]
struct Wordforms<'a> {
    total: BTreeSet<&'a str>,
    valid: BTreeSet<&'a str>,
    misspelled: BTreeSet<&'a str>,
    no_analysis: BTreeSet<&'a str>,
    incomplete: BTreeSet<&'a str>,
    unattempted: BTreeSet<&'a str>,
    ambiguous: BTreeSet<&'a str>,
    languages: BTreeMap<&'a str, LanguageWordforms<'a>>,
    not_a_word: BTreeMap<(&'a str, NotAWordReason, Option<&'a str>), usize>,
    not_a_word_total: BTreeSet<&'a str>,
    by_character: BTreeMap<&'a str, BTreeSet<&'a str>>,
    by_reason: [BTreeSet<&'a str>; 2],
    alt_total: BTreeSet<&'a str>,
    alt_hyphen: BTreeSet<&'a str>,
    alt_apostrophe: BTreeSet<&'a str>,
}

#[derive(Default)]
struct LanguageWordforms<'a> {
    valid: BTreeSet<&'a str>,
    misspelled: BTreeSet<&'a str>,
    ambiguous: BTreeSet<&'a str>,
    no_analysis: BTreeSet<&'a str>,
}

/// Counts each Occurrence once, however many packs, parser results or readings support it.
pub(crate) fn summarize<'a>(results: impl IntoIterator<Item = &'a OccurrenceResult>) -> Statistics {
    let mut stats = Statistics::default();
    let mut forms = Wordforms::default();
    for item in results {
        let word = item.wordform.as_str();
        if let (OccurrenceOutcome::NotAWord, Some(info)) = (item.outcome, &item.not_a_word) {
            let character = info.character.as_deref();
            *forms
                .not_a_word
                .entry((word, info.reason, character))
                .or_default() += 1;
            forms.not_a_word_total.insert(word);
            let counts = &mut stats.not_a_word;
            counts.total.occurrences += 1;
            match (info.reason, character) {
                (NotAWordReason::Character, Some(c)) => {
                    counts
                        .by_character
                        .entry(c.to_owned())
                        .or_default()
                        .occurrences += 1;
                    forms.by_character.entry(c).or_default().insert(word);
                }
                (NotAWordReason::RejectPattern, _) => {
                    counts.reject_pattern.occurrences += 1;
                    forms.by_reason[0].insert(word);
                }
                _ => {
                    counts.style.occurrences += 1;
                    forms.by_reason[1].insert(word);
                }
            }
            continue;
        }
        if !item.alternative_split.is_empty() {
            stats.valid_via_alternative.total.occurrences += 1;
            forms.alt_total.insert(word);
            if item.alternative_split.contains(&SplitCharacter::Hyphen) {
                stats.valid_via_alternative.hyphen.occurrences += 1;
                forms.alt_hyphen.insert(word);
            }
            if item.alternative_split.contains(&SplitCharacter::Apostrophe) {
                stats.valid_via_alternative.apostrophe.occurrences += 1;
                forms.alt_apostrophe.insert(word);
            }
        }
        stats.total.occurrences += 1;
        forms.total.insert(word);
        let (pair, set) = match item.outcome {
            OccurrenceOutcome::Valid => (&mut stats.valid, &mut forms.valid),
            OccurrenceOutcome::Misspelled => (&mut stats.misspelled, &mut forms.misspelled),
            OccurrenceOutcome::NoAnalysis => (&mut stats.no_analysis, &mut forms.no_analysis),
            OccurrenceOutcome::Incomplete => (&mut stats.incomplete, &mut forms.incomplete),
            OccurrenceOutcome::Unattempted => (&mut stats.unattempted, &mut forms.unattempted),
            OccurrenceOutcome::NotAWord => continue,
        };
        pair.occurrences += 1;
        set.insert(word);
        let languages: BTreeSet<&str> = item
            .checks
            .iter()
            .map(|c| c.language.as_str())
            .chain(item.valid_languages.iter().map(String::as_str))
            .collect();
        for language in languages {
            let counts = stats.languages.entry(language.to_owned()).or_default();
            let words = forms.languages.entry(language).or_default();
            let primary = |status| {
                item.checks
                    .iter()
                    .any(|c| c.language == language && c.status == status)
            };
            let primary_analysed = item.readings.first().is_none_or(|r| r.analysed);
            let valid = item.valid_languages.iter().any(|l| l == language);
            let misspelled = item.misspelled_languages.iter().any(|l| l == language);
            // Valid wins over misspelled for one language; other languages keep their own state.
            if valid {
                counts.valid.occurrences += 1;
                words.valid.insert(word);
                if standalone_languages(item) > 1 {
                    counts.ambiguous.occurrences += 1;
                    words.ambiguous.insert(word);
                }
            } else if misspelled {
                counts.misspelled.occurrences += 1;
                words.misspelled.insert(word);
            } else if primary(CheckStatus::Unattempted)
                || primary(CheckStatus::Incomplete)
                || (item.outcome == OccurrenceOutcome::Valid && !primary_analysed)
            {
                // Unfinished checks and primary misses behind an analysed alternative are no miss.
                continue;
            } else {
                counts.no_analysis.occurrences += 1;
                words.no_analysis.insert(word);
            }
        }
        if standalone_languages(item) > 1 {
            stats.ambiguous.occurrences += 1;
            forms.ambiguous.insert(word);
        }
    }
    stats.total.wordforms = forms.total.len();
    stats.valid.wordforms = forms.valid.len();
    stats.misspelled.wordforms = forms.misspelled.len();
    stats.no_analysis.wordforms = forms.no_analysis.len();
    stats.incomplete.wordforms = forms.incomplete.len();
    stats.unattempted.wordforms = forms.unattempted.len();
    stats.ambiguous.wordforms = forms.ambiguous.len();
    stats.valid_via_alternative.total.wordforms = forms.alt_total.len();
    stats.valid_via_alternative.hyphen.wordforms = forms.alt_hyphen.len();
    stats.valid_via_alternative.apostrophe.wordforms = forms.alt_apostrophe.len();
    stats.not_a_word.total.wordforms = forms.not_a_word_total.len();
    stats.not_a_word.reject_pattern.wordforms = forms.by_reason[0].len();
    stats.not_a_word.style.wordforms = forms.by_reason[1].len();
    for (character, words) in &forms.by_character {
        if let Some(pair) = stats.not_a_word.by_character.get_mut(*character) {
            pair.wordforms = words.len();
        }
    }
    for (&(word, reason, character), &count) in &forms.not_a_word {
        stats.not_a_word.forms.push(NotAWordForm {
            wordform: word.to_owned(),
            reason,
            character: character.map(str::to_owned),
            occurrences: count,
        });
    }
    stats.not_a_word.forms.sort_by(|a, b| {
        b.occurrences
            .cmp(&a.occurrences)
            .then_with(|| a.wordform.cmp(&b.wordform))
    });
    for (language, words) in forms.languages {
        if let Some(counts) = stats.languages.get_mut(language) {
            counts.valid.wordforms = words.valid.len();
            counts.misspelled.wordforms = words.misspelled.len();
            counts.ambiguous.wordforms = words.ambiguous.len();
            counts.no_analysis.wordforms = words.no_analysis.len();
        }
    }
    stats
}

/// Languages that analyse some reading by themselves; a mixed-language reading adds none.
fn standalone_languages(item: &OccurrenceResult) -> usize {
    item.valid_languages
        .iter()
        .filter(|language| {
            item.readings.is_empty()
                || item
                    .readings
                    .iter()
                    .filter(|r| r.analysed)
                    .any(|r| r.parts.iter().all(|p| p.valid_languages.contains(language)))
        })
        .count()
}
