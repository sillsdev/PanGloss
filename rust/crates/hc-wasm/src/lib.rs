//! Browser entry point for the PanGloss demo (`PanGloss-demo`, a sibling repo): a thin
//! `wasm-bindgen` wrapper over the existing `hc-grammar` / `hc-parse` / `hc-realize` pipeline.
//! Mirrors `hc-cli`'s `parse --gloss --natural-gloss=eng` glue (`hc-cli/src/main.rs`
//! `print_realize_lines`) but for a whole run of text at once, tokenized here rather than one
//! word at a time on a command line.
#![forbid(unsafe_code)]

use hc_grammar::model::Grammar;
use hc_parse::{Morpher, ParseOptions};
use hc_realize::{gloss_bundle, leipzig, to_ir, RealizeMap, Realizer, TableRealizer};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Call once from JS before anything else — routes Rust panics to `console.error` instead of a
/// silent abort, the only setup a `--target web` module needs beyond `init()`.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[derive(Serialize)]
struct AnalysisOut {
    /// Leipzig-style gloss, e.g. `1-child` (`hc_realize::leipzig`).
    leipzig: String,
    /// Natural-English realization text, e.g. `child` (`hc_realize::Realizer::realize`).
    gloss: String,
    /// False when the realizer only partially covered the analysis's features.
    complete: bool,
    /// Leftover feature tags the realizer couldn't render into `gloss` (empty when `complete`).
    residue: Vec<String>,
    /// True if this analysis came from the guess-root fallback (word absent from the lexicon).
    guessed: bool,
}

#[derive(Serialize)]
struct TokenOut {
    /// `"word"` for a token that went through morphological analysis, `"other"` for whitespace/
    /// punctuation/digits passed through verbatim (see [`tokenize`]).
    kind: &'static str,
    /// Original surface text, unchanged — concatenating every token's `text` in order reconstructs
    /// the input exactly.
    text: String,
    /// Every surviving analysis, in `ParseOutcome.structured` order (first is the one the view
    /// stacks under the word; the rest are what a tooltip lists as alternate readings). Empty for
    /// `"other"` tokens and for words with no surviving analysis at all.
    analyses: Vec<AnalysisOut>,
    /// `ParseOutcome.capped` — the analysis cascade hit its step budget before exhausting search.
    capped: bool,
    /// `ParseOutcome.invalid_shape` — the word contains characters outside the grammar's
    /// orthography, so it was never actually run through the cascade.
    invalid_shape: bool,
}

/// A loaded grammar plus the (grammar-independent) English realization pipeline, kept together so
/// JS makes one object per grammar and calls [`PanGlossGrammar::analyze_text`] on it repeatedly.
#[wasm_bindgen]
pub struct PanGlossGrammar {
    grammar: Grammar,
    realize_map: RealizeMap,
    realizer: TableRealizer,
}

#[wasm_bindgen]
impl PanGlossGrammar {
    /// `xml` is a HermitCrab grammar document (`<HermitCrabInput>...`, e.g. the output of
    /// FieldWorks' `GenerateHCConfig.exe`, or `hc_grammar`'s own test fixtures). `realize_toml` is
    /// the optional per-grammar `hc_realize::RealizeMap` sidecar (`samples/data/*-realize.toml`'s
    /// format) — pass `None`/empty when a grammar has no sidecar; `hc_realize` already degrades
    /// gracefully to Leipzig-only glosses in that case (e.g. today's Sena sample).
    #[wasm_bindgen(constructor)]
    pub fn new(xml: &str, realize_toml: Option<String>) -> Result<PanGlossGrammar, JsValue> {
        let grammar = hc_grammar::load(xml).map_err(|e| js_err("load grammar", &e))?;
        let realizer =
            TableRealizer::new().map_err(|e| js_err("load embedded English table", &e))?;
        let realize_map = match realize_toml {
            Some(text) if !text.trim().is_empty() => {
                RealizeMap::parse(&text).map_err(|e| js_err("parse realize-map", &e))?
            }
            _ => RealizeMap::empty(),
        };
        Ok(PanGlossGrammar {
            grammar,
            realize_map,
            realizer,
        })
    }

    /// Tokenizes `text` and runs every word token through the full analyze -> gloss -> realize
    /// pipeline, returning a JS array of token objects (see [`TokenOut`]). Unknown words still
    /// produce a guessed-root analysis (`ParseOptions::with_guess_root(true)`) rather than an
    /// empty `analyses` array — showing the guess path is part of what the demo is for, not a
    /// fallback to hide.
    #[wasm_bindgen(js_name = analyzeText)]
    pub fn analyze_text(&self, text: &str) -> Result<JsValue, JsValue> {
        let morpher = Morpher::new(&self.grammar, usize::MAX);
        let opts = ParseOptions::default().with_guess_root(true);

        let tokens: Vec<TokenOut> = tokenize(text)
            .into_iter()
            .map(|piece| match piece {
                Piece::Other(s) => TokenOut {
                    kind: "other",
                    text: s.to_string(),
                    analyses: Vec::new(),
                    capped: false,
                    invalid_shape: false,
                },
                Piece::Word(word) => {
                    // Field-linguistics orthography tables are typically lowercase-only (verified
                    // against a real FieldWorks-exported grammar: capitalized sentence-initial
                    // words otherwise come back `invalid_shape`), so analyze the lowercased form
                    // but keep `text` as the original surface casing for display.
                    let lower = word.to_lowercase();
                    let outcome = morpher.parse_word_opts(&lower, &opts);
                    let analyses = outcome
                        .structured
                        .iter()
                        .map(|wa| {
                            let bundle = gloss_bundle(&self.grammar, wa);
                            let leipzig_tag = leipzig(&bundle, &lower);
                            let ir = to_ir(&bundle, &self.realize_map, &lower);
                            let realization = self.realizer.realize(&ir);
                            AnalysisOut {
                                leipzig: leipzig_tag,
                                gloss: realization.text,
                                complete: realization.complete,
                                residue: realization.residue,
                                guessed: wa.guessed,
                            }
                        })
                        .collect();
                    TokenOut {
                        kind: "word",
                        text: word.to_string(),
                        analyses,
                        capped: outcome.capped,
                        invalid_shape: outcome.invalid_shape,
                    }
                }
            })
            .collect();

        serde_wasm_bindgen::to_value(&tokens).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

fn js_err(action: &str, e: &dyn std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{action}: {e:?}"))
}

enum Piece<'a> {
    Word(&'a str),
    Other(&'a str),
}

/// Splits `text` into alternating word/other runs. A "word" run is a maximal span of
/// alphabetic-or-apostrophe characters (apostrophe included because it's phonemic in several
/// sample languages' orthographies, e.g. Sena `m'phole`, not just an English quote mark) — anything
/// else (whitespace, digits, punctuation) is an "other" run, passed straight through for display
/// with no analysis attempted. Concatenating every returned piece's text, in order, reconstructs
/// `text` exactly (needed so View mode never silently drops or reorders characters Edit mode has).
fn tokenize(text: &str) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut in_word: Option<bool> = None; // None = no run yet; Some(is_word) = current run's kind

    let is_word_char = |c: char| c.is_alphabetic() || c == '\'';

    for (i, c) in text.char_indices() {
        let this_is_word = is_word_char(c);
        match in_word {
            Some(cur) if cur == this_is_word => {} // extend current run
            Some(cur) => {
                push_piece(&mut pieces, &text[start..i], cur);
                start = i;
                in_word = Some(this_is_word);
            }
            None => {
                in_word = Some(this_is_word);
            }
        }
    }
    if let Some(cur) = in_word {
        push_piece(&mut pieces, &text[start..], cur);
    }
    pieces
}

fn push_piece<'a>(pieces: &mut Vec<Piece<'a>>, s: &'a str, is_word: bool) {
    if s.is_empty() {
        return;
    }
    pieces.push(if is_word {
        Piece::Word(s)
    } else {
        Piece::Other(s)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_reconstructs_input_exactly() {
        let text = "Mwana ali na nyumba, m'phole-m'phole.\n16 Iwe.";
        let pieces = tokenize(text);
        let rejoined: String = pieces
            .iter()
            .map(|p| match p {
                Piece::Word(s) => *s,
                Piece::Other(s) => *s,
            })
            .collect();
        assert_eq!(rejoined, text);
    }

    #[test]
    fn tokenize_splits_words_from_punctuation_and_digits() {
        // "hi" / ", 16" / "th" / "!" -- punctuation, whitespace, and digits are all "other" and
        // merge into one run; only word-vs-other transitions split pieces.
        let pieces = tokenize("hi, 16th!");
        let kinds: Vec<bool> = pieces.iter().map(|p| matches!(p, Piece::Word(_))).collect();
        assert_eq!(kinds, vec![true, false, true, false]);
    }
}
