//! Browser entry point for the PanGloss demo (`PanGloss-demo`, a sibling repo): a thin
//! `wasm-bindgen` wrapper over the existing `hc-grammar` / `hc-parse` / `hc-realize` pipeline.
//! Mirrors `hc-cli`'s `parse --gloss --natural-gloss=eng` glue (`hc-cli/src/main.rs`
//! `print_realize_lines`) but for a whole run of text at once, tokenized here rather than one
//! word at a time on a command line.
#![forbid(unsafe_code)]

use std::collections::HashMap;

use hc_grammar::model::{Grammar, MorphRuleDef};
use hc_parse::{Morpher, ParseOptions};
use hc_realize::{gloss_bundle, leipzig, to_ir, RealizeMap, Realizer, TableRealizer};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_time::Instant;

/// Call once from JS before anything else — routes Rust panics to `console.error` instead of a
/// silent abort, the only setup a `--target web` module needs beyond `init()`.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[derive(Serialize, Deserialize, Clone)]
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
    /// One entry per surface morpheme, same order as `hc_realize::GlossBundle.tokens` (which is
    /// itself `WordAnalysis.morpheme_ids` order) — each morpheme's `<Property name="ID">` value
    /// from the HermitCrab XML if it has one, `None` otherwise (guessed roots and any morpheme
    /// the grammar didn't tag). For a FieldWorks-converted grammar this is the LCM Hvo (as a
    /// string) `GenerateHCConfig.exe`'s `LexicalDataDump` sidecar is keyed by — the caller looks
    /// each one up there for headword/POS/definition; for the toy/hand-built grammars there's no
    /// sidecar and every entry here is simply `None`.
    morpheme_ids: Vec<Option<String>>,
}

/// Everything about a parsed word that's deterministic for a given (grammar, lowercased word)
/// pair — i.e. safe to cache and replay without re-running the morpher. Kept separate from
/// [`TokenOut`] because [`TokenOut`] also carries call-specific bookkeeping (`parse_ms`,
/// `from_cache`) that must NOT be cached (a cache hit's `parse_ms` is always ~0, not the original
/// call's timing).
#[derive(Serialize, Deserialize, Clone)]
struct CachedWord {
    /// Every surviving analysis, in `ParseOutcome.structured` order (first is the one the view
    /// stacks under the word; the rest are what a tooltip lists as alternate readings). Empty for
    /// words with no surviving analysis at all.
    analyses: Vec<AnalysisOut>,
    /// `ParseOutcome.capped` — the analysis cascade hit its step budget before exhausting search.
    capped: bool,
    /// `ParseOutcome.invalid_shape` — the word contains characters outside the grammar's
    /// orthography, so it was never actually run through the cascade.
    invalid_shape: bool,
    /// `ParseOutcome.candidates_generated` — total synthesis candidates the parser produced
    /// before the validity/match gate, win or lose (see that field's doc in `hc-parse`).
    candidates_generated: usize,
    /// `ParseOutcome.structured.len()` — the subset of `candidates_generated` that survived the
    /// gate and became an entry in `analyses`.
    candidates_accepted: usize,
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
    /// See [`CachedWord::candidates_generated`]. `0` for `"other"` tokens.
    candidates_generated: usize,
    /// See [`CachedWord::candidates_accepted`]. `0` for `"other"` tokens.
    candidates_accepted: usize,
    /// Wall-clock milliseconds this call spent in `Morpher::parse_word_opts` for this word — `0.0`
    /// for a cache hit (nothing was re-parsed) and for `"other"` tokens (never parsed at all).
    parse_ms: f64,
    /// True if this word's result came from the `cache` argument to [`PanGlossGrammar::analyze_text`]
    /// rather than a fresh parse this call. Always `false` for `"other"` tokens.
    from_cache: bool,
}

/// Return value of [`PanGlossGrammar::analyze_text`]: the token stream to render, plus every
/// newly-parsed (not-a-cache-hit) word's [`CachedWord`], keyed by its lowercased surface form, for
/// the caller to merge into whatever persistent cache it keeps across calls. Only *new* entries are
/// returned — words already present in the `cache` argument aren't echoed back, since the caller
/// already has them.
#[derive(Serialize)]
struct AnalyzeTextResult {
    tokens: Vec<TokenOut>,
    new_cache_entries: HashMap<String, CachedWord>,
}

/// A loaded grammar plus the (grammar-independent) English realization pipeline, kept together so
/// JS makes one object per grammar and calls [`PanGlossGrammar::analyze_text`] on it repeatedly.
#[wasm_bindgen]
pub struct PanGlossGrammar {
    /// The PRISTINE grammar XML text this instance was constructed from — never mutated, never
    /// replaced with an augmented copy. [`PanGlossGrammar::apply_user_lexicon`] always re-augments
    /// from this original text (via `hc_lexicon::augment_xml`), so accumulated user-lexicon
    /// entries are re-spliced from scratch on every call rather than compounding onto a
    /// previously-augmented document.
    xml: String,
    /// The optional per-grammar `hc_realize::RealizeMap` sidecar this instance was constructed
    /// with, kept so [`build_realize_map`] can be re-run (with the same sidecar) after
    /// [`PanGlossGrammar::apply_user_lexicon`] reloads the grammar.
    realize_toml: Option<String>,
    grammar: Grammar,
    realize_map: RealizeMap,
    realizer: TableRealizer,
}

/// Every affix-morpheme `<Gloss>` string in `grammar` — the `AffixProcess`/`Realizational`
/// morphological rules' own [`hc_grammar::model::MorphemeInfo::gloss`] (resolved through each
/// rule's `morpheme: MorphemeId`), as opposed to lexical-ENTRY (root) glosses. `CompoundingRule`
/// carries no `MorphemeId` at all (`hc_grammar::model::CompoundingRuleDef`'s own doc: "Not a
/// morpheme") so it contributes nothing here. This is the gloss vocabulary
/// [`hc_realize::infer_english`] matches its built-in English alias table against — root glosses
/// (e.g. "house") are never affix category labels ("pl", "1sg.poss", ...) so including them would
/// only ever add noise, never a match.
fn affix_glosses(grammar: &Grammar) -> Vec<String> {
    grammar
        .mrules
        .iter()
        .filter_map(|mrule| match mrule {
            MorphRuleDef::AffixProcess(def) => Some(def.morpheme),
            MorphRuleDef::Realizational(def) => Some(def.morpheme),
            MorphRuleDef::Compounding(_) => None,
        })
        .filter_map(|morpheme_id| grammar.morphemes.get(morpheme_id.0 as usize))
        .filter_map(|info| info.gloss.clone())
        .collect()
}

/// Build the [`RealizeMap`] a [`PanGlossGrammar`] should use: always start from
/// [`hc_realize::infer_english`] over the grammar's own affix-morpheme glosses (see
/// [`affix_glosses`]) as the base, then, if `realize_toml` is `Some` and non-empty, parse it and
/// let it override the base per-key (`RealizeMap::extend_overriding` — sidecar wins). Shared by
/// [`PanGlossGrammar::new`] and [`PanGlossGrammar::apply_user_lexicon`] (the latter re-runs this
/// against the freshly-reloaded grammar, with the same original sidecar text, after every
/// add-to-dictionary splice).
fn build_realize_map(grammar: &Grammar, realize_toml: Option<&str>) -> Result<RealizeMap, JsValue> {
    let glosses = affix_glosses(grammar);
    let mut map = hc_realize::infer_english(glosses.iter().map(String::as_str));
    if let Some(text) = realize_toml {
        if !text.trim().is_empty() {
            let sidecar = RealizeMap::parse(text).map_err(|e| js_err("parse realize-map", &e))?;
            map.extend_overriding(sidecar);
        }
    }
    Ok(map)
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
        let realize_map = build_realize_map(&grammar, realize_toml.as_deref())?;
        Ok(PanGlossGrammar {
            xml: xml.to_string(),
            realize_toml,
            grammar,
            realize_map,
            realizer,
        })
    }

    /// Tokenizes `text` and runs every word token through the full analyze -> gloss -> realize
    /// pipeline, returning `{ tokens, newCacheEntries }` (see [`AnalyzeTextResult`]). Unknown words
    /// still produce a guessed-root analysis (`ParseOptions::with_guess_root(true)`) rather than an
    /// empty `analyses` array — showing the guess path is part of what the demo is for, not a
    /// fallback to hide.
    ///
    /// `cache` is a JS object (or `undefined`/`null`) mapping a lowercased word to a previously
    /// returned [`CachedWord`] (i.e. the accumulated `newCacheEntries` of every prior call, merged
    /// by the caller) — words present there skip `Morpher::parse_word_opts` entirely and are
    /// replayed verbatim, so re-analyzing the same chapter (or any text sharing vocabulary with
    /// one already seen) only pays the parse cost for genuinely new words. The cache is keyed
    /// per-grammar by construction (it's only ever passed to the same `PanGlossGrammar` instance
    /// the caller got it from), so callers don't need to namespace it themselves.
    #[wasm_bindgen(js_name = analyzeText)]
    pub fn analyze_text(&self, text: &str, cache: JsValue) -> Result<JsValue, JsValue> {
        let cache: HashMap<String, CachedWord> = if cache.is_undefined() || cache.is_null() {
            HashMap::new()
        } else {
            serde_wasm_bindgen::from_value(cache).map_err(|e| JsValue::from_str(&e.to_string()))?
        };

        let morpher = Morpher::new(&self.grammar, usize::MAX);
        let opts = ParseOptions::default().with_guess_root(true);
        let mut new_cache_entries: HashMap<String, CachedWord> = HashMap::new();

        let tokens: Vec<TokenOut> = tokenize(text)
            .into_iter()
            .map(|piece| match piece {
                Piece::Other(s) => TokenOut {
                    kind: "other",
                    text: s.to_string(),
                    analyses: Vec::new(),
                    capped: false,
                    invalid_shape: false,
                    candidates_generated: 0,
                    candidates_accepted: 0,
                    parse_ms: 0.0,
                    from_cache: false,
                },
                Piece::Word(word) => {
                    // Field-linguistics orthography tables are typically lowercase-only (verified
                    // against a real FieldWorks-exported grammar: capitalized sentence-initial
                    // words otherwise come back `invalid_shape`), so analyze the lowercased form
                    // but keep `text` as the original surface casing for display.
                    let lower = word.to_lowercase();

                    let (cached, from_cache, parse_ms) = if let Some(hit) = cache.get(&lower) {
                        (hit.clone(), true, 0.0)
                    } else {
                        let start = Instant::now();
                        let outcome = morpher.parse_word_opts(&lower, &opts);
                        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                        let analyses = outcome
                            .structured
                            .iter()
                            .map(|wa| {
                                let bundle = gloss_bundle(&self.grammar, wa);
                                let leipzig_tag = leipzig(&bundle, &lower);
                                let ir = to_ir(&bundle, &self.realize_map, &lower);
                                let realization = self.realizer.realize(&ir);
                                let morpheme_ids = bundle
                                    .tokens
                                    .iter()
                                    .map(|t| {
                                        t.properties
                                            .iter()
                                            .find(|(k, _)| k == "ID")
                                            .map(|(_, v)| v.clone())
                                    })
                                    .collect();
                                AnalysisOut {
                                    leipzig: leipzig_tag,
                                    gloss: realization.text,
                                    complete: realization.complete,
                                    residue: realization.residue,
                                    guessed: wa.guessed,
                                    morpheme_ids,
                                }
                            })
                            .collect();
                        let fresh = CachedWord {
                            analyses,
                            capped: outcome.capped,
                            invalid_shape: outcome.invalid_shape,
                            candidates_generated: outcome.candidates_generated,
                            candidates_accepted: outcome.structured.len(),
                        };
                        new_cache_entries.insert(lower.clone(), fresh.clone());
                        (fresh, false, elapsed)
                    };

                    TokenOut {
                        kind: "word",
                        text: word.to_string(),
                        analyses: cached.analyses,
                        capped: cached.capped,
                        invalid_shape: cached.invalid_shape,
                        candidates_generated: cached.candidates_generated,
                        candidates_accepted: cached.candidates_accepted,
                        parse_ms,
                        from_cache,
                    }
                }
            })
            .collect();

        let result = AnalyzeTextResult {
            tokens,
            new_cache_entries,
        };
        // `serialize_maps_as_objects`: default serde-wasm-bindgen serializes a Rust `HashMap` as a
        // JS `Map`, not a plain object -- but `new_cache_entries` is meant to be merged into the
        // caller's plain-object cache via `Object.assign` (and re-fed straight back into this same
        // method's `cache` parameter next call), so it must come back as a plain object instead.
        let serializer = serde_wasm_bindgen::Serializer::json_compatible();
        result
            .serialize(&serializer)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Enumerate this grammar's distinct `(POS, inflection-class)` candidate groups
    /// (`hc_lexicon::candidate_classes`) — the "add to dictionary" flow's class picker.
    #[wasm_bindgen(js_name = candidateClasses)]
    pub fn candidate_classes(&self) -> Result<JsValue, JsValue> {
        let classes = hc_lexicon::candidate_classes(&self.grammar);
        let serializer = serde_wasm_bindgen::Serializer::json_compatible();
        classes
            .serialize(&serializer)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Synthesize comparison forms (bare stem + a few inflected forms) for `shape` against each of
    /// `classKeys` (a JS array of [`hc_lexicon::ClassCandidate::key`] strings), so the user can
    /// compare against real text they've seen and pick the right inflection class. Throws a
    /// friendly message (via `hc_lexicon::validate_shape`) if `shape` contains characters outside
    /// this grammar's writing system.
    #[wasm_bindgen(js_name = disambiguatingForms)]
    pub fn disambiguating_forms(
        &self,
        shape: &str,
        class_keys: JsValue,
        max_per_class: usize,
    ) -> Result<JsValue, JsValue> {
        hc_lexicon::validate_shape(&self.grammar, shape).map_err(|msg| JsValue::from_str(&msg))?;

        let requested: Vec<String> = serde_wasm_bindgen::from_value(class_keys)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let all_candidates = hc_lexicon::candidate_classes(&self.grammar);
        let filtered: Vec<_> = requested
            .iter()
            .filter_map(|key| all_candidates.iter().find(|c| &c.key == key).cloned())
            .collect();

        let morpher = Morpher::new(&self.grammar, usize::MAX);
        let forms = hc_lexicon::disambiguating_forms(
            &self.grammar,
            &morpher,
            shape,
            &filtered,
            max_per_class,
        );

        let serializer = serde_wasm_bindgen::Serializer::json_compatible();
        forms
            .serialize(&serializer)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Splice every entry in `lexiconJson` (a JS-serialized [`hc_lexicon::UserLexicon`]) into a
    /// fresh copy of the ORIGINAL grammar XML (`self.xml`, never the previously-augmented text —
    /// see [`PanGlossGrammar::xml`]'s doc), reload it, rebuild the realize map against the
    /// reloaded grammar, and replace `self.grammar`/`self.realize_map` in place so future
    /// [`PanGlossGrammar::analyze_text`] calls recognize the new words. Returns the
    /// [`hc_lexicon::AugmentReport`] (`{ skipped: string[] }`) so the caller can surface any
    /// entries that couldn't be spliced in (stale class key, invalid shape, missing exemplar).
    #[wasm_bindgen(js_name = applyUserLexicon)]
    pub fn apply_user_lexicon(&mut self, lexicon_json: JsValue) -> Result<JsValue, JsValue> {
        let lexicon: hc_lexicon::UserLexicon = serde_wasm_bindgen::from_value(lexicon_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let candidates = hc_lexicon::candidate_classes(&self.grammar);
        let (new_xml, report) =
            hc_lexicon::augment_xml(&self.xml, &self.grammar, &lexicon, &candidates)
                .map_err(|msg| JsValue::from_str(&msg))?;

        let new_grammar = hc_grammar::load(&new_xml).map_err(|e| js_err("load grammar", &e))?;
        let new_realize_map = build_realize_map(&new_grammar, self.realize_toml.as_deref())?;

        self.grammar = new_grammar;
        self.realize_map = new_realize_map;

        let serializer = serde_wasm_bindgen::Serializer::json_compatible();
        report
            .serialize(&serializer)
            .map_err(|e| JsValue::from_str(&e.to_string()))
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

    // A small, hand-built, ORIGINAL HermitCrab XML fixture (not derived from any real language
    // project) with one noun lexical entry ("house", gloss "house") and one affix rule (plural,
    // gloss "pl") -- just enough to exercise `affix_glosses`/`build_realize_map` without a real
    // grammar. `PanGlossGrammar`'s wasm-bindgen methods can't easily be driven from a plain
    // `cargo test` (their JS-boundary types are meant to be called across the wasm-bindgen glue,
    // not constructed natively), so these tests target the plain-Rust helpers directly, per this
    // phase's testing guidance.
    const TEST_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>WasmWiringTest</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>n</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cH"><Representations><Representation>h</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cO"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cU"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAll">
        <Name>All</Name>
        <Segment segment="cH" /><Segment segment="cO" /><Segment segment="cU" />
        <Segment segment="cS" /><Segment segment="cE" />
      </SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrPl">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrPl" requiredPartsOfSpeech="posN" outputPartOfSpeech="posN">
            <Name>plural</Name>
            <MorphemeId>PL</MorphemeId>
            <Gloss>pl</Gloss>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subPl">
                <MorphologicalInput>
                  <PhoneticSequence id="stem1">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem1" />
                  <InsertSegments><PhoneticShape>+es</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eHouse" partOfSpeech="posN">
            <Allomorphs><Allomorph id="aHouse"><PhoneticShape>house</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>house</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    #[test]
    fn affix_glosses_includes_rule_glosses_but_not_root_glosses() {
        let g = hc_grammar::load(TEST_XML).expect("test fixture loads");
        let glosses = affix_glosses(&g);
        assert!(glosses.iter().any(|s| s == "pl"), "the plural rule's gloss must be included: {glosses:?}");
        assert!(
            !glosses.iter().any(|s| s == "house"),
            "a lexical-entry (root) gloss must not be treated as an affix gloss: {glosses:?}"
        );
    }

    #[test]
    fn build_realize_map_infers_from_affix_glosses_when_no_sidecar() {
        let g = hc_grammar::load(TEST_XML).expect("test fixture loads");
        let map = build_realize_map(&g, None).expect("builds base map");
        assert_eq!(
            map.lookup("pl"),
            Some(hc_realize::FeatureAssignment::Num(hc_realize::Num::Pl)),
            "the built-in English alias table must recognize the affix rule's 'pl' gloss"
        );
    }

    #[test]
    fn build_realize_map_lets_sidecar_override_the_inferred_base() {
        let g = hc_grammar::load(TEST_XML).expect("test fixture loads");
        let sidecar = "[features]\n\"pl\" = \"Ignore\"\n";
        let map = build_realize_map(&g, Some(sidecar)).expect("builds overridden map");
        assert_eq!(
            map.lookup("pl"),
            Some(hc_realize::FeatureAssignment::Ignore),
            "an explicit sidecar mapping must win over the inferred base for the same gloss key"
        );
    }

    #[test]
    fn build_realize_map_treats_blank_sidecar_the_same_as_none() {
        let g = hc_grammar::load(TEST_XML).expect("test fixture loads");
        let with_none = build_realize_map(&g, None).expect("builds with None");
        let with_blank = build_realize_map(&g, Some("   \n")).expect("builds with blank sidecar");
        assert_eq!(with_none, with_blank);
    }
}
