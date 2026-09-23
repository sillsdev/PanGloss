//! Natural-phrases N2 (`docs/natural-phrases-plan.md` N2): `TableRealizer`, the one
//! `crate::Realizer` implementation this milestone ships — a compile-time-authored English
//! construction table (Architecture B, `docs/natural-glosses-plan.md` §8) loaded from two
//! `include_str!`-embedded assets under `rust/crates/pg-realize/assets/eng/`:
//!
//! - `templates.toml`'s `[cells]` section: one entry per `(CaseRole, Poss, Num)` construction
//!   cell (4 × 9 × 3 = 108 cells, every one enumerated explicitly — see that file's own header
//!   comment for the mechanical rule that generated it), key syntax `"Loc.P1Sg.Pl" = "in my
//!   {n:pl}"`. Each value contains exactly one `{n:sg}` or `{n:pl}` slot.
//! - `lexicon.toml`'s `[plural_exceptions]` section: the standard English irregular plurals,
//!   consulted before the regular `-s`/`-es`/`-ies` rules in `regular_plural`.
//!
//! Both assets reuse `crate::map`'s restricted-TOML-subset reader
//! (`crate::map::parse_section`) rather than a third hand-rolled parser — same rejection-with-
//! line-number behavior a bad committed asset gets caught by (below, in a unit test, not a
//! runtime panic: `TableRealizer::new` returns a `Result`, and `assets_load_and_cover_all_108_cells`
//! is the test that would fail CI on a bad asset).
#![forbid(unsafe_code)]

use std::collections::HashMap;

use crate::ir::{CaseRole, Concept, GlossIr, Num, Poss};
use crate::map::{parse_section, MapError};
use crate::realize::{Realization, Realizer};

const TEMPLATES_TOML: &str = include_str!("../assets/eng/templates.toml");
const LEXICON_TOML: &str = include_str!("../assets/eng/lexicon.toml");

/// A loaded, validated English construction table: the 108-cell `(CaseRole, Poss, Num)` ->
/// template map, plus the irregular-plural exceptions table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRealizer {
    cells: HashMap<(CaseRole, Poss, Num), String>,
    plural_exceptions: HashMap<String, String>,
}

impl TableRealizer {
    /// Load and validate the two embedded English assets. Fails (rather than panicking) on
    /// anything the restricted-TOML-subset reader rejects, an unrecognized cell key (a
    /// `Case.Poss.Num` component that doesn't parse), a template missing its required `{n:sg}`/
    /// `{n:pl}` slot (or carrying both / neither), or incomplete 108-cell coverage. A committed
    /// bad asset is meant to fail *this constructor* — the unit test
    /// `assets_load_and_cover_all_108_cells` below is what turns that into a CI failure instead
    /// of a field-discovered runtime gap (per the milestone task: "IN A TEST, not a runtime
    /// panic").
    pub fn new() -> Result<Self, MapError> {
        let cells = load_cells(TEMPLATES_TOML)?;
        let plural_exceptions = load_lexicon(LEXICON_TOML)?;
        validate_coverage(&cells)?;
        Ok(TableRealizer {
            cells,
            plural_exceptions,
        })
    }

    /// Resolves `citation`'s plural form: the exceptions table first, then `regular_plural` for a plain ASCII-alphabetic word; `None` for anything else, so the caller falls back to the unchanged citation form and marks the realization incomplete.
    fn plural_form(&self, word: &str) -> Option<String> {
        if let Some(p) = self.plural_exceptions.get(word) {
            return Some(p.clone());
        }
        if is_plain_ascii_word(word) {
            Some(regular_plural(word))
        } else {
            None
        }
    }

    /// Pluralizes a citation form already split into words: only the final word is inflected, for multi-word/dotted glosses. Returns the joined phrase and whether the inflection was cleanly derivable.
    fn pluralize_phrase(&self, words: &[&str]) -> (String, bool) {
        match words.split_last() {
            Some((last, prefix)) => match self.plural_form(last) {
                Some(plural_last) => {
                    let text = if prefix.is_empty() {
                        plural_last
                    } else {
                        format!("{} {}", prefix.join(" "), plural_last)
                    };
                    (text, true)
                }
                None => (words.join(" "), false),
            },
            None => (String::new(), false), // defensive: empty citation form
        }
    }
}

impl Realizer for TableRealizer {
    /// Looks up the `(case, poss, num)` cell and fills its noun-form slot; a guessed root always renders `*{surface}*` uninflected and `complete: false`, a lexical citation pluralizes only the final word when needed, and a missing cell (impossible while `new`'s coverage check holds) falls back to the bare citation form.
    fn realize(&self, ir: &GlossIr) -> Realization {
        let residue = ir.extras.clone();

        match &ir.concept {
            Concept::Guessed(surface) => {
                let starred = format!("*{surface}*");
                let text = match self.cells.get(&(ir.case, ir.poss, ir.num)) {
                    Some(template) => fill_template(template, &starred),
                    None => starred,
                };
                Realization {
                    text,
                    complete: false,
                    residue,
                }
            }
            Concept::Lex(citation) => {
                let spaced = citation.replace('.', " ");
                let words: Vec<&str> = spaced.split_whitespace().collect();

                match self.cells.get(&(ir.case, ir.poss, ir.num)) {
                    Some(template) => {
                        let needs_plural = template.contains("{n:pl}");
                        let (form, derivable) = if needs_plural {
                            self.pluralize_phrase(&words)
                        } else {
                            (spaced.clone(), true)
                        };
                        let text = fill_template(template, &form);
                        Realization {
                            text,
                            complete: residue.is_empty() && derivable,
                            residue,
                        }
                    }
                    None => Realization {
                        text: spaced,
                        complete: false,
                        residue,
                    },
                }
            }
        }
    }
}

/// Fills a template's single `{n:sg}`/`{n:pl}` slot with `form`; load-time validation guarantees exactly one placeholder is present, so falling back to `{n:pl}` unconditionally is exhaustive, not a guess.
fn fill_template(template: &str, form: &str) -> String {
    if template.contains("{n:sg}") {
        template.replace("{n:sg}", form)
    } else {
        template.replace("{n:pl}", form)
    }
}

/// A plain (non-empty, all-ASCII-alphabetic) word, the class `regular_plural`'s suffix rules are meaningful over; anything else is left alone and marks the realization incomplete.
fn is_plain_ascii_word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic())
}

/// Regular English pluralization: default `-s`; `-es` after a word ending in s/x/z/ch/sh; a final consonant + `y` becomes `-ies`. Only ever called on an `is_plain_ascii_word` input.
fn regular_plural(word: &str) -> String {
    if word.ends_with('s')
        || word.ends_with('x')
        || word.ends_with('z')
        || word.ends_with("ch")
        || word.ends_with("sh")
    {
        format!("{word}es")
    } else if word.len() >= 2 {
        let mut chars = word.chars();
        let last = chars.next_back().expect("len >= 2");
        let prev = chars.next_back().expect("len >= 2");
        if last == 'y' && !is_vowel(prev) {
            format!("{}ies", &word[..word.len() - 1])
        } else {
            format!("{word}s")
        }
    } else {
        format!("{word}s")
    }
}

fn is_vowel(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u')
}

/// Parses `templates.toml`'s `[cells]` section into the `(CaseRole, Poss, Num) -> template` table; each value must contain exactly one of `{n:sg}`/`{n:pl}`, a load-time error carrying the source line number if not.
fn load_cells(text: &str) -> Result<HashMap<(CaseRole, Poss, Num), String>, MapError> {
    let mut cells = HashMap::new();
    for (line_no, key, value) in parse_section(text, "cells")? {
        let parts: Vec<&str> = key.split('.').collect();
        let [case_s, poss_s, num_s] = parts.as_slice() else {
            return Err(MapError {
                line: line_no,
                message: format!(
                    "cell key {key:?} must have exactly 3 dot-separated components (Case.Poss.Num)"
                ),
            });
        };
        let case = CaseRole::parse(case_s).ok_or_else(|| MapError {
            line: line_no,
            message: format!("cell key {key:?}: unknown CaseRole component {case_s:?}"),
        })?;
        let poss = Poss::parse(poss_s).ok_or_else(|| MapError {
            line: line_no,
            message: format!("cell key {key:?}: unknown Poss component {poss_s:?}"),
        })?;
        let num = Num::parse(num_s).ok_or_else(|| MapError {
            line: line_no,
            message: format!("cell key {key:?}: unknown Num component {num_s:?}"),
        })?;

        let has_sg = value.contains("{n:sg}");
        let has_pl = value.contains("{n:pl}");
        if has_sg == has_pl {
            return Err(MapError {
                line: line_no,
                message: format!(
                    "cell key {key:?}: value {value:?} must contain exactly one of {{n:sg}}/{{n:pl}}"
                ),
            });
        }

        cells.insert((case, poss, num), value);
    }
    Ok(cells)
}

/// Parses `lexicon.toml`'s `[plural_exceptions]` section into the singular -> irregular-plural table; last entry for a duplicate key wins.
fn load_lexicon(text: &str) -> Result<HashMap<String, String>, MapError> {
    let mut lexicon = HashMap::new();
    for (_line_no, key, value) in parse_section(text, "plural_exceptions")? {
        lexicon.insert(key, value);
    }
    Ok(lexicon)
}

/// Confirms `cells` has an entry for every one of the 4 x 9 x 3 = 108 `(CaseRole, Poss, Num)` combinations; line `0` since this checks the assembled map, not one entry.
fn validate_coverage(cells: &HashMap<(CaseRole, Poss, Num), String>) -> Result<(), MapError> {
    for &case in &CaseRole::ALL {
        for &poss in &Poss::ALL {
            for &num in &Num::ALL {
                if !cells.contains_key(&(case, poss, num)) {
                    return Err(MapError {
                        line: 0,
                        message: format!(
                            "templates.toml is missing cell \"{case:?}.{poss:?}.{num:?}\""
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
