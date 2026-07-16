//! Browser entry point for the PanGloss demo (`PanGloss-demo`, a sibling repo): a thin
//! `wasm-bindgen` wrapper over the existing `hc-grammar` / `hc-parse` / `hc-realize` pipeline.
//! Mirrors `hc-cli`'s `parse --gloss --natural-gloss=eng` glue (`hc-cli/src/main.rs`
//! `print_realize_lines`) but for a whole run of text at once, tokenized here rather than one
//! word at a time on a command line.
//!
//! P4 (`docs/fst-plan/foma-fst-plan.md` §4 P4, gate F4): `PanGlossGrammar::new` (and
//! `apply_user_lexicon`, which reloads the grammar) also builds an `hc_foma::composite::FomaAnalyzer`
//! for the grammar; `analyze_text` routes each word through it when present. A grammar whose
//! emitted lexc source fails to foma-compile falls back automatically to the full engine (logged,
//! see [`log_foma_fallback`]) — see [`FomaState`]'s doc for why the compiled proposer is stored as
//! its own owned pieces rather than as a `FomaAnalyzer<'g>` field.
#![forbid(unsafe_code)]

use std::collections::HashMap;

use hc_grammar::model::{Grammar, MorphRuleDef};
use hc_parse::{Morpher, ParseOptions, WordAnalysis};
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

/// The OWNED (non-borrowing) pieces of a compiled `hc_foma::composite::FomaAnalyzer` for one
/// grammar: the compiled foma net (`FomaProposer`, plan P4's expensive emit+foma-compile step),
/// the reduplication peeler, and the morpheme-owner map. Stored separately from a `FomaAnalyzer`
/// itself (which additionally borrows `&'g Grammar` and owns a `Morpher<'g>`) because
/// `PanGlossGrammar` also OWNS the `Grammar` these would borrow from — a `PanGlossGrammar` field
/// of type `FomaAnalyzer<'g>` tied to a sibling `grammar: Grammar` field is a self-referential
/// struct Rust cannot express directly. Instead this crate does what it already does for
/// `Morpher<'g>` (see [`PanGlossGrammar::analyze_text`]: never stored, always built fresh per call
/// from `&self.grammar`): [`PanGlossGrammar::analyze_text`] takes this out of `self.foma`,
/// reconstructs a short-lived `FomaAnalyzer` via `FomaAnalyzer::from_cached(&self.grammar, ...)`
/// for the duration of one call, then hands the (unchanged) owned pieces back via
/// `FomaAnalyzer::into_parts`.
struct FomaState {
    proposer: hc_foma::analyzer::FomaProposer,
    peeler: hc_foma::peel::ReduplicationPeeler,
    owners: Vec<Option<hc_foma::confirm::MorphemeOwner>>,
}

/// Emit + foma-compile `grammar`'s propose→confirm pieces. `Err` carries a human-readable message
/// (`hc_foma::analyzer::FomaError`'s `Display`) — a compiler-gap diagnostic, not a grammar-content
/// problem the caller can fix by editing their text.
fn build_foma_state(grammar: &Grammar) -> Result<FomaState, String> {
    let proposer =
        hc_foma::analyzer::FomaProposer::new(grammar).map_err(|e| e.to_string())?;
    Ok(FomaState {
        peeler: hc_foma::peel::ReduplicationPeeler::new(grammar),
        owners: hc_foma::confirm::build_morpheme_owners(grammar),
        proposer,
    })
}

/// Attempt to build [`FomaState`] for `grammar`; on failure, log the automatic fallback (plan P4:
/// "compile failure → automatic fallback to full engine, logged") and return the diagnostic
/// message alongside `None` so [`PanGlossGrammar::engine_diagnostic`] can surface it to JS without
/// the caller needing to inspect the browser console.
fn init_foma(grammar: &Grammar) -> (Option<FomaState>, Option<String>) {
    match build_foma_state(grammar) {
        Ok(state) => (Some(state), None),
        Err(msg) => {
            log_foma_fallback(&msg);
            (None, Some(msg))
        }
    }
}

/// `console.error` in a browser (wasm32) build, `eprintln!` natively (this crate's own `cargo
/// test` runs off the wasm32 target) — the "logged" half of plan P4's automatic-fallback
/// requirement. Deliberately independent of `console_error_panic_hook` (set up in [`start`]),
/// which only intercepts Rust panics; this is an ordinary `Err` return, not a panic.
fn log_foma_fallback(msg: &str) {
    let full = format!(
        "hc-wasm: foma proposer compile failed, falling back to the full engine for this grammar: {msg}"
    );
    #[cfg(target_arch = "wasm32")]
    web_sys::console::error_1(&JsValue::from_str(&full));
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{full}");
}

/// Build one [`CachedWord`] from a (possibly foma- or full-engine-sourced) list of confirmed
/// analyses plus the diagnostic fields `ParseOutcome`/`FomaOutcome` each carry under different
/// names — shared by both engine paths in [`PanGlossGrammar::analyze_text`] so the
/// gloss/leipzig/realize construction (which neither knows nor cares which engine produced
/// `structured`) is written once.
fn build_cached_word(
    grammar: &Grammar,
    realize_map: &RealizeMap,
    realizer: &TableRealizer,
    lower: &str,
    structured: Vec<WordAnalysis>,
    capped: bool,
    invalid_shape: bool,
    candidates_generated: usize,
) -> CachedWord {
    let analyses: Vec<AnalysisOut> = structured
        .iter()
        .map(|wa| {
            let bundle = gloss_bundle(grammar, wa);
            let leipzig_tag = leipzig(&bundle, lower);
            let ir = to_ir(&bundle, realize_map, lower);
            let realization = realizer.realize(&ir);
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
    CachedWord {
        candidates_accepted: analyses.len(),
        analyses,
        capped,
        invalid_shape,
        candidates_generated,
    }
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
    /// `Some` iff this grammar's foma propose→confirm proposer compiled successfully (plan P4) —
    /// see [`FomaState`]'s doc for why these are owned pieces rather than a stored `FomaAnalyzer`.
    /// `None` means every word in this grammar routes through the full engine, either because
    /// compilation failed (see `foma_diagnostic`) or (transiently, mid-`analyze_text`) while the
    /// pieces are checked out to build this call's `FomaAnalyzer`.
    foma: Option<FomaState>,
    /// `Some` iff the most recent attempt to build `foma` (construction, or the last
    /// `apply_user_lexicon` reload) failed — the human-readable reason, surfaced to JS via
    /// [`PanGlossGrammar::engine_diagnostic`]. `None` once foma is active.
    foma_diagnostic: Option<String>,
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
        let (foma, foma_diagnostic) = init_foma(&grammar);
        Ok(PanGlossGrammar {
            xml: xml.to_string(),
            realize_toml,
            grammar,
            realize_map,
            realizer,
            foma,
            foma_diagnostic,
        })
    }

    /// `"foma"` when this grammar's compiled propose→confirm proposer is active (plan P4's
    /// mainline), `"engine"` when it isn't — either the emitted lexc source failed to foma-compile
    /// at construction/reload time (`engineDiagnostic()` carries the reason) or this grammar has
    /// never had one built. This reports the GRAMMAR-level engine `PanGlossGrammar` was built
    /// with; it does NOT reflect the separate per-word guess-root retry `analyzeText` performs
    /// through the full engine when the foma path itself confirms nothing for a given word (see
    /// that method's doc) — that retry is a per-word display fallback, not a change of which
    /// engine this instance is on.
    #[wasm_bindgen(js_name = engineKind)]
    pub fn engine_kind(&self) -> String {
        if self.foma.is_some() {
            "foma"
        } else {
            "engine"
        }
        .to_string()
    }

    /// `Some` iff the most recent attempt to compile this grammar's foma proposer failed — the
    /// reason [`PanGlossGrammar::engine_kind`] reports `"engine"`. `None` once foma is active.
    #[wasm_bindgen(js_name = engineDiagnostic)]
    pub fn engine_diagnostic(&self) -> Option<String> {
        self.foma_diagnostic.clone()
    }

    /// Tokenizes `text` and runs every word token through the full analyze -> gloss -> realize
    /// pipeline, returning `{ tokens, newCacheEntries }` (see [`AnalyzeTextResult`]). Unknown words
    /// still produce a guessed-root analysis (`ParseOptions::with_guess_root(true)`) rather than an
    /// empty `analyses` array — showing the guess path is part of what the demo is for, not a
    /// fallback to hide.
    ///
    /// `cache` is a JS object (or `undefined`/`null`) mapping a lowercased word to a previously
    /// returned [`CachedWord`] (i.e. the accumulated `newCacheEntries` of every prior call, merged
    /// by the caller) — words present there skip re-analysis entirely and are replayed verbatim,
    /// so re-analyzing the same chapter (or any text sharing vocabulary with one already seen)
    /// only pays the parse cost for genuinely new words. The cache is keyed per-grammar by
    /// construction (it's only ever passed to the same `PanGlossGrammar` instance the caller got
    /// it from), so callers don't need to namespace it themselves.
    ///
    /// Routes each new word through `self.foma` (plan P4) when this grammar has one; a word that
    /// engine confirms nothing for still gets the full engine's own `guess_root` retry (below),
    /// exactly as it always did on the full-engine-only path — `FomaAnalyzer` deliberately never
    /// sets `guess_root` itself (`hc_foma::composite`'s own doc), so unrecognized-word display
    /// (part of what this demo is for, not a fallback to hide) still needs this one call.
    #[wasm_bindgen(js_name = analyzeText)]
    pub fn analyze_text(&mut self, text: &str, cache: JsValue) -> Result<JsValue, JsValue> {
        let cache: HashMap<String, CachedWord> = if cache.is_undefined() || cache.is_null() {
            HashMap::new()
        } else {
            serde_wasm_bindgen::from_value(cache).map_err(|e| JsValue::from_str(&e.to_string()))?
        };

        // Never stored as a field (same "build fresh per call from an owned `&Grammar`" shape as
        // `FomaState`'s rehydrated `FomaAnalyzer` below) — needed unconditionally, both as the
        // sole engine when `self.foma` is `None` and as the guess-root retry when foma confirms
        // nothing for a particular word.
        let morpher = Morpher::new(&self.grammar, usize::MAX);
        let opts = ParseOptions::default().with_guess_root(true);
        let mut new_cache_entries: HashMap<String, CachedWord> = HashMap::new();

        // Check the compiled foma pieces (if any) out of `self.foma` and rehydrate a
        // `FomaAnalyzer` borrowing `&self.grammar` for the rest of this call — see [`FomaState`]'s
        // doc for why this can't just be a stored field.
        let mut foma_analyzer = self.foma.take().map(|state| {
            hc_foma::composite::FomaAnalyzer::from_cached(
                &self.grammar,
                state.proposer,
                state.peeler,
                state.owners,
            )
        });

        let mut tokens: Vec<TokenOut> = Vec::new();
        for piece in tokenize(text) {
            let token = match piece {
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
                        let (structured, capped, invalid_shape, candidates_generated) =
                            match foma_analyzer.as_mut() {
                                Some(analyzer) => {
                                    let outcome = analyzer.analyze_word(&lower);
                                    if outcome.structured.is_empty() {
                                        // No confirmed foma candidate -- fall back to the full
                                        // engine's own guess-root path for JUST this word (see
                                        // this method's doc).
                                        let fallback = morpher.parse_word_opts(&lower, &opts);
                                        (
                                            fallback.structured,
                                            fallback.capped,
                                            fallback.invalid_shape,
                                            fallback.candidates_generated,
                                        )
                                    } else {
                                        // `FomaAnalyzer` has no notion of a step-budget cascade
                                        // (confirm is always uncapped by design) or of orthography
                                        // validity (that's a property of the word/grammar, not the
                                        // engine) -- `capped` is honestly always false here, and
                                        // `invalid_shape` is answered independently via the same
                                        // segmentation check `hc_lexicon` already uses elsewhere in
                                        // this crate (`disambiguating_forms`).
                                        let invalid_shape =
                                            hc_lexicon::validate_shape(&self.grammar, &lower)
                                                .is_err();
                                        (
                                            outcome.structured,
                                            false,
                                            invalid_shape,
                                            outcome.candidates_generated,
                                        )
                                    }
                                }
                                None => {
                                    let outcome = morpher.parse_word_opts(&lower, &opts);
                                    (
                                        outcome.structured,
                                        outcome.capped,
                                        outcome.invalid_shape,
                                        outcome.candidates_generated,
                                    )
                                }
                            };
                        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                        let fresh = build_cached_word(
                            &self.grammar,
                            &self.realize_map,
                            &self.realizer,
                            &lower,
                            structured,
                            capped,
                            invalid_shape,
                            candidates_generated,
                        );
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
            };
            tokens.push(token);
        }

        // Hand the (content-unchanged) compiled pieces back to long-term storage -- the inverse
        // of the check-out above. No-op (`self.foma` stays `None`) when this grammar has none.
        if let Some(analyzer) = foma_analyzer.take() {
            let (proposer, peeler, owners) = analyzer.into_parts();
            self.foma = Some(FomaState {
                proposer,
                peeler,
                owners,
            });
        }

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
        // The spliced-in entries change the lexicon the foma net was compiled from, so the
        // proposer must be recompiled from scratch here too (plan P4) -- otherwise newly-added
        // words would confirm via the full-engine guess-root retry forever, never via foma.
        let (new_foma, new_foma_diagnostic) = init_foma(&new_grammar);

        self.grammar = new_grammar;
        self.realize_map = new_realize_map;
        self.foma = new_foma;
        self.foma_diagnostic = new_foma_diagnostic;

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

    // --- P4 gate F4: native parity smoke (docs/fst-plan/foma-fst-plan.md §4 P4) ---------------
    //
    // Not a browser round-trip, but the IDENTICAL Rust functions `PanGlossGrammar::new`/
    // `analyze_text` call (`build_foma_state`, `hc_foma::composite::FomaAnalyzer::from_cached`/
    // `analyze_word`), compiled natively, run over real corpus words from `samples/data/` and
    // compared against the full engine's own `Morpher::parse_word_opts` as multisets keyed by
    // `(morpheme_ids, root_index)` -- exactly the parity contract plan §P3/§D7 already gates the
    // underlying `hc-foma` crate on; this test just confirms the wasm-facing wiring didn't lose
    // anything in translation.

    /// Loads `grammar_file`/`words_file` from `samples/data/` (skipping quietly if either is
    /// absent, matching this workspace's usual "sample data may not be checked out" convention),
    /// takes the first `sample` non-empty lines of the word list, and asserts the foma path's
    /// confirmed analyses exactly multiset-match the full engine's for every one of them.
    fn assert_foma_matches_engine(grammar_file: &str, words_file: &str, sample: usize) {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let samples_dir = manifest_dir.join("../../../samples/data");
        let grammar_path = samples_dir.join(grammar_file);
        let words_path = samples_dir.join(words_file);
        if !grammar_path.exists() || !words_path.exists() {
            eprintln!("skipping {grammar_file}: sample data not present on disk");
            return;
        }
        let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
        let grammar = hc_grammar::load(&xml)
            .unwrap_or_else(|e| panic!("failed to load {grammar_file}: {e}"));
        let foma_state = build_foma_state(&grammar)
            .unwrap_or_else(|e| panic!("{grammar_file} must foma-compile (gate F1): {e}"));
        let mut analyzer = hc_foma::composite::FomaAnalyzer::from_cached(
            &grammar,
            foma_state.proposer,
            foma_state.peeler,
            foma_state.owners,
        );
        let morpher = Morpher::new(&grammar, usize::MAX);
        let opts = ParseOptions::default();

        let words_text = std::fs::read_to_string(&words_path).expect("read words file");
        let words: Vec<&str> = words_text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .take(sample)
            .collect();
        assert!(!words.is_empty(), "{words_file} produced no sample words");

        let mut checked = 0usize;
        for &word in &words {
            let foma_outcome = analyzer.analyze_word(word);
            let engine_outcome = morpher.parse_word_opts(word, &opts);

            let mut foma_keys: Vec<(Vec<u32>, i32)> = foma_outcome
                .structured
                .iter()
                .map(|wa| (wa.morpheme_ids.clone(), wa.root_morpheme_index))
                .collect();
            let mut engine_keys: Vec<(Vec<u32>, i32)> = engine_outcome
                .structured
                .iter()
                .map(|wa| (wa.morpheme_ids.clone(), wa.root_morpheme_index))
                .collect();
            foma_keys.sort();
            engine_keys.sort();
            assert_eq!(
                foma_keys, engine_keys,
                "{grammar_file}: foma vs full-engine mismatch for {word:?}"
            );
            checked += 1;
        }
        eprintln!("{grammar_file}: foma path matched the full engine on {checked} corpus word(s)");
    }

    #[test]
    fn foma_path_matches_full_engine_on_sena_sample() {
        assert_foma_matches_engine("sena-hc.xml", "sena-words.txt", 40);
    }

    #[test]
    fn foma_path_matches_full_engine_on_indonesian_corpus() {
        // Indonesian's whole corpus file is only 121 words (plan §P3: "all 121 corpus words
        // required 100%") -- small enough to run in full rather than sampling.
        assert_foma_matches_engine("indonesian-hc.xml", "indonesian-words.txt", usize::MAX);
    }
}
