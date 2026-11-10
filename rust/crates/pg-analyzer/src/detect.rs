//! Language detection: a proposal over a text or corpus, never a verdict on an Occurrence.

use crate::dictionary::DictionaryPack;
use crate::statistics::Pair;
use crate::text::{OccurrenceOutcome, OccurrenceResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Untuned initial value: Occurrences only one language explains before it is proposed.
pub const MIN_DECISIVE_OCCURRENCES: usize = 5;
/// Untuned initial value: shorter forms never receive n-gram evidence.
pub const MIN_NGRAM_CHARS: usize = 4;
/// Untuned initial value: percent of a form's trigrams a profile must hold to vouch for it.
pub const MIN_NGRAM_COVERAGE_PERCENT: usize = 80;
/// Untuned initial value: percent points of lead over the runner-up for a clear n-gram winner.
pub const MIN_NGRAM_MARGIN_PERCENT: usize = 20;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// Enough Occurrences are explained by this language alone to suggest turning it on.
    Proposed,
    /// Some evidence, but too little or too shared to suggest anything.
    Ambiguous,
}

/// Evidence for one language over a text or corpus.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageProposal {
    pub language: String,
    pub status: ProposalStatus,
    /// Occurrences and Wordforms this language has any evidence for: `dictionary` plus `ngram`.
    pub occurrences: Pair,
    /// Valid by a Language Pack or Dictionary pack of this language.
    pub dictionary: Pair,
    /// Of `dictionary`, also valid in another language, so no support for this one over it.
    pub shared: Pair,
    /// Not valid anywhere, but this language's n-gram profile clearly covers the form.
    pub ngram: Pair,
}

/// What a text or corpus seems to contain. `sufficient` is false when no language is proposed.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Detection {
    /// Most Occurrences first, then by language code.
    pub proposals: Vec<LanguageProposal>,
    pub sufficient: bool,
}

/// Character trigrams of a pack's words, padded at both edges; memory is the set of distinct trigrams.
#[derive(Clone, Debug, Default)]
pub(crate) struct NgramProfile(BTreeSet<[char; 3]>);

fn trigrams(word: &str) -> Vec<[char; 3]> {
    let chars: Vec<char> = std::iter::once('^')
        .chain(word.to_lowercase().chars())
        .chain(std::iter::once('$'))
        .collect();
    chars.windows(3).map(|w| [w[0], w[1], w[2]]).collect()
}

impl NgramProfile {
    pub(crate) fn add(&mut self, word: &str) {
        self.0.extend(trigrams(word));
    }

    /// Whole percent of the form's trigrams this profile holds; 0 for forms too short to judge.
    fn coverage_percent(&self, word: &str) -> usize {
        if word.chars().count() < MIN_NGRAM_CHARS {
            return 0;
        }
        let grams = trigrams(word);
        grams.iter().filter(|g| self.0.contains(*g)).count() * 100 / grams.len()
    }
}

#[derive(Default)]
struct Tally<'a> {
    dictionary: (usize, BTreeSet<&'a str>),
    shared: (usize, BTreeSet<&'a str>),
    ngram: (usize, BTreeSet<&'a str>),
    any: (usize, BTreeSet<&'a str>),
    decisive: usize,
}

fn bump<'a>(slot: &mut (usize, BTreeSet<&'a str>), word: &'a str) {
    slot.0 += 1;
    slot.1.insert(word);
}

fn pair(slot: &(usize, BTreeSet<&str>)) -> Pair {
    Pair {
        occurrences: slot.0,
        wordforms: slot.1.len(),
    }
}

/// The language whose profile clearly covers `word`, if any.
fn ngram_winner<'a>(dictionaries: &'a [DictionaryPack], word: &str) -> Option<&'a str> {
    let mut best: BTreeMap<&str, usize> = BTreeMap::new();
    for pack in dictionaries {
        let coverage = pack.ngrams.coverage_percent(word);
        let slot = best.entry(pack.identity.language.as_str()).or_default();
        *slot = (*slot).max(coverage);
    }
    let mut ranked: Vec<(&str, usize)> = best.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let (top, second) = (ranked.first()?, ranked.get(1).map_or(0, |r| r.1));
    (top.1 >= MIN_NGRAM_COVERAGE_PERCENT && top.1 >= second + MIN_NGRAM_MARGIN_PERCENT)
        .then_some(top.0)
}

/// Count, per language, what the Occurrences' evidence supports. Unrun and unfinished checks vote for nothing.
pub(crate) fn detect<'a>(
    occurrences: impl IntoIterator<Item = &'a OccurrenceResult>,
    dictionaries: &[DictionaryPack],
) -> Detection {
    let mut tallies: BTreeMap<String, Tally<'a>> = BTreeMap::new();
    for item in occurrences {
        let word = item.wordform.as_str();
        let languages: BTreeSet<&str> = item.valid_languages.iter().map(String::as_str).collect();
        if !languages.is_empty() {
            for language in &languages {
                let tally = tallies.entry((*language).to_owned()).or_default();
                bump(&mut tally.dictionary, word);
                bump(&mut tally.any, word);
                if languages.len() > 1 {
                    bump(&mut tally.shared, word);
                } else {
                    tally.decisive += 1;
                }
            }
            continue;
        }
        let rejected = matches!(
            item.outcome,
            OccurrenceOutcome::NoAnalysis | OccurrenceOutcome::Misspelled
        );
        if rejected {
            if let Some(language) = ngram_winner(dictionaries, word) {
                let tally = tallies.entry(language.to_owned()).or_default();
                bump(&mut tally.ngram, word);
                bump(&mut tally.any, word);
                tally.decisive += 1;
            }
        }
    }
    let mut proposals: Vec<LanguageProposal> = tallies
        .into_iter()
        .map(|(language, t)| LanguageProposal {
            status: if t.decisive >= MIN_DECISIVE_OCCURRENCES {
                ProposalStatus::Proposed
            } else {
                ProposalStatus::Ambiguous
            },
            language,
            occurrences: pair(&t.any),
            dictionary: pair(&t.dictionary),
            shared: pair(&t.shared),
            ngram: pair(&t.ngram),
        })
        .collect();
    proposals.sort_by(|a, b| {
        b.occurrences
            .occurrences
            .cmp(&a.occurrences.occurrences)
            .then(a.language.cmp(&b.language))
    });
    let sufficient = proposals
        .iter()
        .any(|p| p.status == ProposalStatus::Proposed);
    Detection {
        proposals,
        sufficient,
    }
}
