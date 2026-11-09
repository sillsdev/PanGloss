use crate::text::{OccurrenceOutcome, OccurrenceResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Occurrences and distinct Wordforms, always reported side by side and never merged.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Pair {
    pub occurrences: usize,
    pub wordforms: usize,
}

/// Valid-word counts for one language code.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageCounts {
    pub valid: Pair,
}

/// Counts for one text or a corpus; see `docs/text-analysis-format.md` for what each field counts.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Statistics {
    pub total: Pair,
    pub valid: Pair,
    pub no_analysis: Pair,
    pub incomplete: Pair,
    pub unattempted: Pair,
    pub ambiguous: Pair,
    pub languages: BTreeMap<String, LanguageCounts>,
}

#[derive(Default)]
struct Wordforms<'a> {
    total: BTreeSet<&'a str>,
    valid: BTreeSet<&'a str>,
    no_analysis: BTreeSet<&'a str>,
    incomplete: BTreeSet<&'a str>,
    unattempted: BTreeSet<&'a str>,
    ambiguous: BTreeSet<&'a str>,
    languages: BTreeMap<&'a str, BTreeSet<&'a str>>,
}

/// Counts each Occurrence once, however many packs, parser results or readings support it.
pub(crate) fn summarize<'a>(results: impl IntoIterator<Item = &'a OccurrenceResult>) -> Statistics {
    let mut stats = Statistics::default();
    let mut forms = Wordforms::default();
    for item in results {
        let word = item.wordform.as_str();
        stats.total.occurrences += 1;
        forms.total.insert(word);
        let (pair, set) = match item.outcome {
            OccurrenceOutcome::Valid => (&mut stats.valid, &mut forms.valid),
            OccurrenceOutcome::NoAnalysis => (&mut stats.no_analysis, &mut forms.no_analysis),
            OccurrenceOutcome::Incomplete => (&mut stats.incomplete, &mut forms.incomplete),
            OccurrenceOutcome::Unattempted => (&mut stats.unattempted, &mut forms.unattempted),
        };
        pair.occurrences += 1;
        set.insert(word);
        for language in &item.valid_languages {
            stats
                .languages
                .entry(language.clone())
                .or_default()
                .valid
                .occurrences += 1;
            forms.languages.entry(language).or_default().insert(word);
        }
        if standalone_languages(item) > 1 {
            stats.ambiguous.occurrences += 1;
            forms.ambiguous.insert(word);
        }
    }
    stats.total.wordforms = forms.total.len();
    stats.valid.wordforms = forms.valid.len();
    stats.no_analysis.wordforms = forms.no_analysis.len();
    stats.incomplete.wordforms = forms.incomplete.len();
    stats.unattempted.wordforms = forms.unattempted.len();
    stats.ambiguous.wordforms = forms.ambiguous.len();
    for (language, words) in forms.languages {
        if let Some(counts) = stats.languages.get_mut(language) {
            counts.valid.wordforms = words.len();
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
