//! Dictionary packs: Hunspell evidence behind a PanGloss interface; no Spellbook type escapes.

use spellbook::Dictionary;
use std::collections::BTreeSet;

/// Who a Dictionary pack is: language, source, content hash, version and licence.
#[derive(Clone, Debug)]
pub struct DictionaryIdentity {
    pub language: String,
    pub source_id: String,
    pub source_sha256: String,
    pub version: String,
    pub licence: String,
}

/// A loaded Dictionary pack: Hunspell `.aff`/`.dic` data and an optional word list.
pub struct DictionaryPack {
    pub(crate) identity: DictionaryIdentity,
    hunspell: Dictionary,
    word_list: BTreeSet<String>,
}

/// What one pack says about one Wordform.
pub(crate) enum Verdict {
    Accepted,
    /// In the pack's word list; the Hunspell dictionary did not accept it.
    Listed,
    /// Hunspell rejected it and offered corrections.
    Misspelled(Vec<String>),
    /// Rejected, and the suggestion search was not run because the word is over the bound.
    SuggestionsCapped,
    /// Rejected without corrections: a miss alone, which is no misspelling.
    Unknown,
}

impl DictionaryPack {
    /// Parse Hunspell data; malformed data is refused, never loaded partially.
    pub fn new(identity: DictionaryIdentity, aff: &str, dic: &str) -> Result<Self, String> {
        if identity.language.is_empty()
            || identity.source_id.is_empty()
            || identity.source_sha256.is_empty()
            || identity.version.is_empty()
            || identity.licence.is_empty()
        {
            return Err(
                "a Dictionary pack needs a language, source, hash, version and licence".into(),
            );
        }
        let hunspell = Dictionary::new(aff, dic)
            .map_err(|e| format!("Dictionary pack {}: {e}", identity.source_id))?;
        Ok(Self {
            identity,
            hunspell,
            word_list: BTreeSet::new(),
        })
    }

    /// Add word-list membership evidence. A word missing from the list is never rejected for it.
    pub fn with_word_list<I, S>(mut self, words: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.word_list.extend(words.into_iter().map(Into::into));
        self
    }

    /// `suggestions` is `Some(max_chars)` when corrections are wanted; a longer word is not searched.
    pub(crate) fn check(&self, word: &str, suggestions: Option<usize>) -> Verdict {
        if self.hunspell.check(word) {
            return Verdict::Accepted;
        }
        if self.word_list.contains(word) || self.word_list.contains(&word.to_lowercase()) {
            return Verdict::Listed;
        }
        let Some(max_chars) = suggestions else {
            return Verdict::Unknown;
        };
        if word.chars().count() > max_chars {
            return Verdict::SuggestionsCapped;
        }
        let mut found = Vec::new();
        self.hunspell.suggest(word, &mut found);
        found.retain(|s| s != word);
        if found.is_empty() {
            Verdict::Unknown
        } else {
            Verdict::Misspelled(found)
        }
    }
}
