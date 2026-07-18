//! Natural-phrases N2 (`docs/natural-phrases-plan.md` N2): [`TableRealizer`], the one
//! [`crate::Realizer`] implementation this milestone ships — a compile-time-authored English
//! construction table (Architecture B, `docs/natural-glosses-plan.md` §8) loaded from two
//! `include_str!`-embedded assets under `rust/crates/pg-realize/assets/eng/`:
//!
//! - `templates.toml`'s `[cells]` section: one entry per `(CaseRole, Poss, Num)` construction
//!   cell (4 × 9 × 3 = 108 cells, every one enumerated explicitly — see that file's own header
//!   comment for the mechanical rule that generated it), key syntax `"Loc.P1Sg.Pl" = "in my
//!   {n:pl}"`. Each value contains exactly one `{n:sg}` or `{n:pl}` slot.
//! - `lexicon.toml`'s `[plural_exceptions]` section: the standard English irregular plurals,
//!   consulted before the regular `-s`/`-es`/`-ies` rules in [`regular_plural`].
//!
//! Both assets reuse `crate::map`'s restricted-TOML-subset reader
//! ([`crate::map::parse_section`]) rather than a third hand-rolled parser — same rejection-with-
//! line-number behavior a bad committed asset gets caught by (below, in a unit test, not a
//! runtime panic: [`TableRealizer::new`] returns a `Result`, and `assets_load_and_cover_all_108_cells`
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

    /// Resolve `citation`'s plural form: the exceptions table first, then the regular mechanical
    /// rules ([`regular_plural`]) for any plain ASCII-alphabetic word. `None` when neither source
    /// can handle it cleanly (a non-alphabetic token — digits, punctuation, empty) — the caller
    /// falls back to the unchanged citation form and marks the realization incomplete, per the
    /// milestone task's "else use the citation form unchanged and mark the realization
    /// incomplete" rule.
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

    /// Pluralize a citation form already split into whitespace-separated words (dots already
    /// turned to spaces by the caller): only the final word is inflected, matching the milestone
    /// task's "pluralize ONLY the final token" rule for multi-word/dotted glosses. Returns the
    /// joined phrase and whether the inflection was cleanly derivable.
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
    /// Look up the `(case, poss, num)` cell and fill its noun-form slot. Total: every branch
    /// produces a non-empty `text` (barring a genuinely empty `GlossIr`), never panics.
    ///
    /// - `Concept::Guessed(surface)`: always renders `*{surface}*` uninflected in the slot (never
    ///   pluralized — a guessed root has no known plural form) and is always `complete: false`,
    ///   even when the cell itself matched and residue is empty, per the milestone task's
    ///   explicit guessed-concept rule.
    /// - `Concept::Lex(citation)`: dots become spaces; only the final word is pluralized when the
    ///   matched cell's slot is `{n:pl}`. `complete` requires an empty `residue`, a matched cell,
    ///   and (when the cell needed a plural) a cleanly derivable one.
    /// - Missing cell (defensive only — impossible while `TableRealizer::new`'s coverage check
    ///   holds): falls back to the bare citation form, `complete: false`.
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

/// Fill a template's single `{n:sg}`/`{n:pl}` slot with `form`, leaving the rest of the template
/// unchanged. `templates.toml`'s own load-time validation guarantees exactly one of the two
/// placeholders is present, so checking for `{n:sg}` and falling back to `{n:pl}` unconditionally
/// is exhaustive, not a guess.
fn fill_template(template: &str, form: &str) -> String {
    if template.contains("{n:sg}") {
        template.replace("{n:sg}", form)
    } else {
        template.replace("{n:pl}", form)
    }
}

/// A plain (non-empty, all-ASCII-alphabetic) word — the class [`regular_plural`]'s mechanical
/// suffix rules are meaningful over. Anything else (digits, punctuation, empty) is left alone by
/// [`TableRealizer::plural_form`] and marks the realization incomplete instead.
fn is_plain_ascii_word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic())
}

/// Regular English pluralization (`docs/natural-phrases-plan.md` N2): default `-s`; `-es` after a
/// word ending in s/x/z/ch/sh; a final consonant + `y` becomes `-ies`. Only ever called on an
/// [`is_plain_ascii_word`] input.
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

/// Parse `templates.toml`'s `[cells]` section into the `(CaseRole, Poss, Num) -> template`
/// table. Each key must be exactly three dot-separated components, each parsing via the
/// corresponding enum's `parse`; each value must contain exactly one of `{n:sg}`/`{n:pl}` (not
/// both, not neither) — both are load-time errors carrying the entry's source line number.
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

/// Parse `lexicon.toml`'s `[plural_exceptions]` section into the singular -> irregular-plural
/// table (last entry for a duplicate key wins, same authoring-config posture as
/// `RealizeMap::parse`).
fn load_lexicon(text: &str) -> Result<HashMap<String, String>, MapError> {
    let mut lexicon = HashMap::new();
    for (_line_no, key, value) in parse_section(text, "plural_exceptions")? {
        lexicon.insert(key, value);
    }
    Ok(lexicon)
}

/// Confirm `cells` has an entry for every one of the 4 × 9 × 3 = 108 `(CaseRole, Poss, Num)`
/// combinations. Line `0` (no single source line owns a whole-file coverage gap) since this
/// checks the assembled map, not one entry.
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
mod tests {
    use super::*;

    fn realizer() -> TableRealizer {
        TableRealizer::new().unwrap_or_else(|e| panic!("embedded assets failed to load: {e}"))
    }

    fn ir(concept: Concept, case: CaseRole, poss: Poss, num: Num) -> GlossIr {
        GlossIr {
            concept,
            num,
            poss,
            case,
            extras: Vec::new(),
        }
    }

    // --- Load-time validation: the whole point of doing this in a test, not a runtime panic ---

    #[test]
    fn assets_load_and_cover_all_108_cells() {
        let r = realizer();
        assert_eq!(r.cells.len(), 108, "4 CaseRole x 9 Poss x 3 Num");
        for &case in &CaseRole::ALL {
            for &poss in &Poss::ALL {
                for &num in &Num::ALL {
                    let template = r
                        .cells
                        .get(&(case, poss, num))
                        .unwrap_or_else(|| panic!("missing cell {case:?}.{poss:?}.{num:?}"));
                    let has_sg = template.contains("{n:sg}");
                    let has_pl = template.contains("{n:pl}");
                    assert!(
                        has_sg != has_pl,
                        "{case:?}.{poss:?}.{num:?} template {template:?} must have exactly one slot"
                    );
                }
            }
        }
    }

    #[test]
    fn lexicon_exceptions_load() {
        let r = realizer();
        assert_eq!(
            r.plural_exceptions.get("child").map(String::as_str),
            Some("children")
        );
        assert_eq!(
            r.plural_exceptions.get("man").map(String::as_str),
            Some("men")
        );
        assert_eq!(
            r.plural_exceptions.get("sheep").map(String::as_str),
            Some("sheep")
        );
        assert_eq!(
            r.plural_exceptions.get("house"),
            None,
            "house is a regular plural"
        );
    }

    // --- The flagship assertion (task brief's own words) ---------------------------------------

    #[test]
    fn flagship_loc_p1sg_pl_house_renders_in_my_houses() {
        let r = realizer();
        let g = ir(
            Concept::Lex("house".to_string()),
            CaseRole::Loc,
            Poss::P1Sg,
            Num::Pl,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, "in my houses");
        assert!(out.complete);
        assert!(out.residue.is_empty());
    }

    // --- Irregular plural via the exceptions table ----------------------------------------------

    #[test]
    fn irregular_child_becomes_his_children_via_p3sgm_pl() {
        let r = realizer();
        let g = ir(
            Concept::Lex("child".to_string()),
            CaseRole::None,
            Poss::P3SgM,
            Num::Pl,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, "his children");
        assert!(out.complete);
    }

    // --- Regular -es/-ies rules ------------------------------------------------------------------

    #[test]
    fn regular_es_after_sibilant() {
        let r = realizer();
        for (word, expected_plural) in [
            ("box", "boxes"),
            ("buzz", "buzzes"),
            ("church", "churches"),
            ("bush", "bushes"),
        ] {
            let g = ir(
                Concept::Lex(word.to_string()),
                CaseRole::None,
                Poss::None,
                Num::Pl,
            );
            let out = r.realize(&g);
            assert_eq!(out.text, expected_plural, "{word}");
            assert!(out.complete, "{word}");
        }
    }

    #[test]
    fn regular_ies_after_consonant_plus_y() {
        let r = realizer();
        let g = ir(
            Concept::Lex("city".to_string()),
            CaseRole::None,
            Poss::None,
            Num::Pl,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, "cities");
        assert!(out.complete);
    }

    #[test]
    fn vowel_plus_y_is_not_ies() {
        let r = realizer();
        let g = ir(
            Concept::Lex("toy".to_string()),
            CaseRole::None,
            Poss::None,
            Num::Pl,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, "toys");
        assert!(out.complete);
    }

    // --- Multi-word / dotted glosses: only the final token is pluralized -----------------------

    #[test]
    fn multi_word_dotted_gloss_pluralizes_only_final_token() {
        let r = realizer();
        let g = ir(
            Concept::Lex("treat.someone".to_string()),
            CaseRole::None,
            Poss::None,
            Num::Sg,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, "a treat someone");
        assert!(out.complete);

        let g_pl = ir(
            Concept::Lex("treat.someone".to_string()),
            CaseRole::None,
            Poss::None,
            Num::Pl,
        );
        let out_pl = r.realize(&g_pl);
        assert_eq!(
            out_pl.text, "treat someones",
            "only 'someone' inflects, not 'treat'"
        );
        assert!(out_pl.complete);
    }

    #[test]
    fn already_spaced_multi_word_gloss_works_the_same_as_dotted() {
        let r = realizer();
        let g = ir(
            Concept::Lex("soar skyward".to_string()),
            CaseRole::None,
            Poss::None,
            Num::Pl,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, "soar skywards");
        assert!(out.complete);
    }

    // --- Guessed concept: never inflected, always incomplete, template still filled --------------

    #[test]
    fn guessed_concept_uninflected_in_plural_slot_and_marked_incomplete() {
        let r = realizer();
        let g = ir(
            Concept::Guessed("mbal".to_string()),
            CaseRole::None,
            Poss::P1Sg,
            Num::Pl,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, "my *mbal*", "uninflected -- NOT '*mbal*s'");
        assert!(!out.complete, "guessed concepts are always incomplete");
    }

    #[test]
    fn guessed_concept_singular_cell_also_incomplete() {
        let r = realizer();
        let g = ir(
            Concept::Guessed("mbal".to_string()),
            CaseRole::None,
            Poss::None,
            Num::Sg,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, "a *mbal*");
        assert!(!out.complete);
    }

    // --- Missing-plural incompleteness: a non-alphabetic final token can't be inflected ----------

    #[test]
    fn non_alphabetic_final_token_leaves_citation_form_unchanged_and_marks_incomplete() {
        let r = realizer();
        let g = ir(
            Concept::Lex("x1".to_string()),
            CaseRole::None,
            Poss::P3SgF,
            Num::Pl,
        );
        let out = r.realize(&g);
        assert_eq!(
            out.text, "her x1",
            "unpluralized citation form used verbatim in the slot"
        );
        assert!(!out.complete);
    }

    // --- Residue / partial flagging ---------------------------------------------------------------

    #[test]
    fn nonempty_residue_marks_incomplete_even_with_a_matched_cell_and_clean_plural() {
        let r = realizer();
        let mut g = ir(
            Concept::Lex("house".to_string()),
            CaseRole::Loc,
            Poss::P1Sg,
            Num::Pl,
        );
        g.extras = vec!["obj".to_string()];
        let out = r.realize(&g);
        assert_eq!(out.text, "in my houses", "text still renders normally");
        assert!(!out.complete, "nonempty residue forces incomplete");
        assert_eq!(out.residue, vec!["obj".to_string()]);
    }

    #[test]
    fn residue_mirrors_ir_extras_verbatim_regardless_of_concept_kind() {
        let r = realizer();
        let mut g = ir(
            Concept::Guessed("mbal".to_string()),
            CaseRole::None,
            Poss::None,
            Num::Unspec,
        );
        g.extras = vec!["foo".to_string(), "bar".to_string()];
        let out = r.realize(&g);
        assert_eq!(out.residue, vec!["foo".to_string(), "bar".to_string()]);
    }

    // --- Sg vs Unspec both use the singular slot, with the Unspec/Sg article distinction ---------

    #[test]
    fn unspec_num_uses_singular_form_with_no_indefinite_article() {
        let r = realizer();
        let g = ir(
            Concept::Lex("house".to_string()),
            CaseRole::None,
            Poss::None,
            Num::Unspec,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, "house");
        assert!(out.complete);
    }

    #[test]
    fn sg_num_gets_indefinite_article_when_unpossessed() {
        let r = realizer();
        let g = ir(
            Concept::Lex("house".to_string()),
            CaseRole::None,
            Poss::None,
            Num::Sg,
        );
        let out = r.realize(&g);
        assert_eq!(out.text, "a house");
        assert!(out.complete);
    }
}
