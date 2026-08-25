//! Browser entry point for the PanGloss demo (`PanGloss-demo`, a sibling repo): a thin
//! `wasm-bindgen` wrapper over the existing `pg-grammar` / `pg-parse` / `pg-realize` pipeline.
//! Mirrors `pg-cli`'s `parse --gloss --natural-gloss=eng` glue (`pg-cli/src/main.rs`
//! `print_realize_lines`) but for a whole run of text at once, tokenized here rather than one
//! word at a time on a command line.
//!
//! `PanGlossGrammar::new` also builds a `pg_foma::composite::FomaAnalyzer`
//! for the grammar; `analyze_text` routes each word through it when present. A grammar whose
//! emitted lexc source fails to foma-compile falls back automatically to the full engine (logged,
//! see `log_foma_fallback`) — see `FomaState`'s doc for why the compiled proposer is stored as
//! its own owned pieces rather than as a `FomaAnalyzer<'g>` field.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;

use pg_grammar::model::{Grammar, MorphRuleDef};
use pg_parse::WordAnalysis;
use pg_realize::{gloss_bundle, leipzig, to_ir, RealizeMap, Realizer, TableRealizer};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_time::Instant;

/// `.pgpack` load-time compatibility (ADR 0004 `required ⊆ provided` containment) and the ADR
/// 0005 capability-trust stamp -- see `pack`'s own module doc. `PgPack` below is this module's
/// wasm-bindgen-facing wrapper.
pub mod pack;

/// Call once from JS before anything else — routes Rust panics to `console.error` instead of a
/// silent abort, the only setup a `--target web` module needs beyond `init()`.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AnalysisOut {
    /// Leipzig-style gloss, e.g. `1-child` (`pg_realize::leipzig`).
    leipzig: String,
    /// Natural-English realization text, e.g. `child` (`pg_realize::Realizer::realize`).
    gloss: String,
    /// False when the realizer only partially covered the analysis's features.
    complete: bool,
    /// Leftover feature tags the realizer couldn't render into `gloss` (empty when `complete`).
    residue: Vec<String>,
    /// True if this analysis came from the guess-root fallback (word absent from the lexicon).
    guessed: bool,
    provenance: pg_parse::AnalysisProvenance,
    /// One entry per surface morpheme, same order as `pg_realize::GlossBundle.tokens`: each morpheme's `<Property name="ID">` value if it has one, `None` otherwise; for a FieldWorks-converted grammar this is the LCM Hvo the `LexicalDataDump` sidecar is keyed by.
    morpheme_ids: Vec<Option<String>>,
}

/// Everything about a parsed word that's deterministic for a given (grammar, exact authored word) pair, safe to cache and replay without re-running the morpher; kept separate from `TokenOut`, which also carries call-specific bookkeeping that must not be cached.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CachedWord {
    /// Overlay revision that produced this record; a caller-provided record is reusable only when it exactly matches the handle's current revision.
    overlay_revision: pg_lexicon::Revision,
    /// Every surviving analysis, in `ParseOutcome.structured` order (first is what the view stacks under the word); empty for words with no surviving analysis at all.
    analyses: Vec<AnalysisOut>,
    /// `ParseOutcome.capped`: the analysis cascade hit its step budget before exhausting search.
    capped: bool,
    /// `ParseOutcome.invalid_shape`: the word contains characters outside the grammar's orthography, so it was never run through the cascade.
    invalid_shape: bool,
    /// `ParseOutcome.candidates_generated`: total synthesis candidates the parser produced before the validity/match gate, win or lose.
    candidates_generated: usize,
    /// `ParseOutcome.structured.len()`: the subset of `candidates_generated` that survived the gate and became an entry in `analyses`.
    candidates_accepted: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenOut {
    /// `"word"` for a token that went through morphological analysis, `"other"` for whitespace/punctuation/digits passed through verbatim.
    kind: &'static str,
    /// Original surface text, unchanged; concatenating every token's `text` in order reconstructs the input exactly.
    text: String,
    /// Every surviving analysis, in `ParseOutcome.structured` order; empty for `"other"` tokens and for words with no surviving analysis at all.
    analyses: Vec<AnalysisOut>,
    /// `ParseOutcome.capped`: the analysis cascade hit its step budget before exhausting search.
    capped: bool,
    /// `ParseOutcome.invalid_shape`: the word contains characters outside the grammar's orthography, so it was never run through the cascade.
    invalid_shape: bool,
    /// See `CachedWord::candidates_generated`. `0` for `"other"` tokens.
    candidates_generated: usize,
    /// See `CachedWord::candidates_accepted`. `0` for `"other"` tokens.
    candidates_accepted: usize,
    /// Wall-clock milliseconds this call spent in `Morpher::parse_word_opts` for this word; `0.0` for a cache hit or an `"other"` token.
    parse_ms: f64,
    /// True if this word's result came from the `cache` argument to `PanGlossGrammar::analyze_text` rather than a fresh parse; always `false` for `"other"` tokens.
    from_cache: bool,
}

/// Return value of `PanGlossGrammar::analyze_text`: the token stream to render, plus every newly-parsed word's `CachedWord`, keyed by surface form, for the caller to merge into its persistent cache; words already present in the `cache` argument aren't echoed back.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalyzeTextResult {
    tokens: Vec<TokenOut>,
    new_cache_entries: HashMap<String, CachedWord>,
}

/// The owned (non-borrowing) pieces of a compiled `pg_foma::composite::FomaAnalyzer` for one grammar, stored separately because `PanGlossGrammar` owns the `Grammar` a `FomaAnalyzer<'g>` field would need to borrow from, which Rust cannot express as a self-referential struct; `FomaCheckout` reconstructs a short-lived `FomaAnalyzer` and its `Drop` returns the unchanged compiled pieces on every return path.
struct FomaState {
    proposer: pg_foma::analyzer::FomaProposer,
    peeler: pg_foma::peel::ReduplicationPeeler,
    owners: Vec<Option<pg_foma::confirm::MorphemeOwner>>,
    /// Carried across every checkout rather than rebuilt, so nothing silently resets it.
    filter: pg_foma::candidate_filter::CandidateFilterSettings,
}

struct FomaCheckout<'slot, 'grammar> {
    slot: &'slot mut Option<FomaState>,
    analyzer: Option<pg_foma::composite::FomaAnalyzer<'grammar>>,
}

impl<'slot, 'grammar> FomaCheckout<'slot, 'grammar> {
    fn new(slot: &'slot mut Option<FomaState>, grammar: &'grammar Grammar) -> Self {
        let analyzer = slot.take().map(|state| {
            pg_foma::composite::FomaAnalyzer::from_cached(
                grammar,
                state.proposer,
                state.peeler,
                state.owners,
                state.filter,
            )
        });
        Self { slot, analyzer }
    }

    fn analyzer_mut(&mut self) -> Option<&mut pg_foma::composite::FomaAnalyzer<'grammar>> {
        self.analyzer.as_mut()
    }
}

impl Drop for FomaCheckout<'_, '_> {
    fn drop(&mut self) {
        if let Some(analyzer) = self.analyzer.take() {
            let (proposer, peeler, owners, filter) = analyzer.into_parts();
            *self.slot = Some(FomaState {
                proposer,
                peeler,
                owners,
                filter,
            });
        }
    }
}

/// Emit + foma-compile `grammar`'s propose→confirm pieces; `Err` carries a human-readable compiler-gap diagnostic, not a grammar-content problem the caller can fix by editing their text.
fn build_foma_state(grammar: &Grammar) -> Result<FomaState, String> {
    let proposer = pg_foma::analyzer::FomaProposer::new(grammar).map_err(|e| e.to_string())?;
    Ok(FomaState {
        peeler: pg_foma::peel::ReduplicationPeeler::new(grammar),
        owners: pg_foma::confirm::build_morpheme_owners(grammar),
        proposer,
        filter: pg_foma::candidate_filter::CandidateFilterSettings::off(),
    })
}

/// Attempt to build `FomaState` for `grammar`; on failure, log the fallback and return the diagnostic message alongside `None` so `PanGlossGrammar::engine_diagnostic` can surface it to JS.
fn init_foma(grammar: &Grammar) -> (Option<FomaState>, Option<String>) {
    match build_foma_state(grammar) {
        Ok(state) => (Some(state), None),
        Err(msg) => {
            log_foma_fallback(&msg);
            (None, Some(msg))
        }
    }
}

/// `console.error` in a browser (wasm32) build, `eprintln!` natively; deliberately independent of `console_error_panic_hook`, which only intercepts Rust panics, since this is an ordinary `Err` return.
fn log_foma_fallback(msg: &str) {
    let full = format!(
        "pg-wasm: foma proposer compile failed, falling back to the full engine for this grammar: {msg}"
    );
    #[cfg(target_arch = "wasm32")]
    web_sys::console::error_1(&JsValue::from_str(&full));
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{full}");
}

/// Build one `CachedWord` from a (possibly foma- or full-engine-sourced) list of confirmed analyses plus the diagnostic fields both engine paths carry under different names, so the gloss/leipzig/realize construction is written once.
struct CacheAnalysis {
    structured: Vec<WordAnalysis>,
    capped: bool,
    invalid_shape: bool,
    candidates_generated: usize,
    overlay_revision: pg_lexicon::Revision,
}

fn build_cached_word(
    grammar: &Grammar,
    realize_map: &RealizeMap,
    realizer: &TableRealizer,
    lower: &str,
    analysis: CacheAnalysis,
) -> CachedWord {
    let analyses: Vec<AnalysisOut> = analysis
        .structured
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
                provenance: wa.provenance.clone(),
                morpheme_ids,
            }
        })
        .collect();
    CachedWord {
        overlay_revision: analysis.overlay_revision,
        candidates_accepted: analyses.len(),
        analyses,
        capped: analysis.capped,
        invalid_shape: analysis.invalid_shape,
        candidates_generated: analysis.candidates_generated,
    }
}

/// A loaded grammar plus the (grammar-independent) English realization pipeline, kept together so
/// JS makes one object per grammar and calls `PanGlossGrammar::analyze_text` on it repeatedly.
#[wasm_bindgen]
pub struct PanGlossGrammar {
    grammar: Arc<Grammar>,
    runtime: pg_lexicon::SuppliedLexiconRuntime,
    realize_map: RealizeMap,
    realizer: TableRealizer,
    /// `Some` iff this grammar's foma propose→confirm proposer compiled successfully; `None` means every word routes through the full engine (see `foma_diagnostic`). A transient checkout is protected by `FomaCheckout`, whose destructor restores this slot even while unwinding.
    foma: Option<FomaState>,
    /// `Some` iff construction failed to build `foma`, surfaced to JS via `PanGlossGrammar::engine_diagnostic`; `None` once foma is active.
    foma_diagnostic: Option<String>,
}

/// Every affix-morpheme `<Gloss>` string in `grammar` (`AffixProcess`/`Realizational` rules' own morpheme gloss, never lexical-entry root glosses); this is the vocabulary `pg_realize::infer_english` matches its English alias table against, where a root gloss like "house" would only ever add noise.
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

/// Build the `RealizeMap` a `PanGlossGrammar` should use: start from `pg_realize::infer_english` over the grammar's affix-morpheme glosses, then, if `realize_toml` is non-empty, parse it and let it override the base per-key (sidecar wins).
fn build_realize_map(grammar: &Grammar, realize_toml: Option<&str>) -> Result<RealizeMap, JsValue> {
    let glosses = affix_glosses(grammar);
    let mut map = pg_realize::infer_english(glosses.iter().map(String::as_str));
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
    /// FieldWorks' `GenerateHCConfig.exe`, or `pg_grammar`'s own test fixtures). `realize_toml` is
    /// the optional per-grammar `pg_realize::RealizeMap` sidecar (`samples/data/*-realize.toml`'s
    /// format) — pass `None`/empty when a grammar has no sidecar; `pg_realize` already degrades
    /// gracefully to Leipzig-only glosses in that case (e.g. today's Sena sample).
    #[wasm_bindgen(constructor)]
    pub fn new(xml: &str, realize_toml: Option<String>) -> Result<PanGlossGrammar, JsValue> {
        let grammar = Arc::new(pg_grammar::load(xml).map_err(|e| js_err("load grammar", &e))?);
        let realizer =
            TableRealizer::new().map_err(|e| js_err("load embedded English table", &e))?;
        let realize_map = build_realize_map(&grammar, realize_toml.as_deref())?;
        let (foma, foma_diagnostic) = init_foma(&grammar);
        let runtime = pg_lexicon::SuppliedLexiconRuntime::new(grammar.clone(), xml)
            .map_err(|e| js_err("initialize supplied lexicon", &e))?;
        Ok(PanGlossGrammar {
            grammar,
            runtime,
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
        if self.foma_diagnostic.is_none() {
            "foma"
        } else {
            "engine"
        }
        .to_string()
    }

    /// `Some` iff the most recent attempt to compile this grammar's foma proposer failed — the
    /// reason `PanGlossGrammar::engine_kind` reports `"engine"`. `None` once foma is active.
    #[wasm_bindgen(js_name = engineDiagnostic)]
    pub fn engine_diagnostic(&self) -> Option<String> {
        self.foma_diagnostic.clone()
    }

    /// Tokenizes `text` and runs every word token through the full analyze -> gloss -> realize
    /// pipeline, returning `{ tokens, newCacheEntries }` (see `AnalyzeTextResult`). Unknown words
    /// still produce a guessed-root analysis (`ParseOptions::with_guess_root(true)`) rather than an
    /// empty `analyses` array — showing the guess path is part of what the demo is for, not a
    /// fallback to hide.
    ///
    /// `cache` is a JS object (or `undefined`/`null`) mapping an exact surface word to a previously
    /// returned `CachedWord` (i.e. the accumulated `newCacheEntries` of every prior call, merged
    /// by the caller) — words present there skip re-analysis entirely and are replayed verbatim,
    /// so re-analyzing the same chapter (or any text sharing vocabulary with one already seen)
    /// only pays the parse cost for genuinely new words. The cache is keyed per-grammar by
    /// construction (it's only ever passed to the same `PanGlossGrammar` instance the caller got
    /// it from), so callers don't need to namespace it themselves.
    ///
    /// Routes each new word through `self.foma` (plan P4) when this grammar has one; a word that
    /// engine confirms nothing for still gets the full engine's own `guess_root` retry (below),
    /// exactly as it always did on the full-engine-only path — `FomaAnalyzer` deliberately never
    /// sets `guess_root` itself (`pg_foma::composite`'s own doc), so unrecognized-word display
    /// (part of what this demo is for, not a fallback to hide) still needs this one call.
    #[wasm_bindgen(js_name = analyzeText)]
    pub fn analyze_text(&mut self, text: &str, cache: JsValue) -> Result<JsValue, JsValue> {
        let cache: HashMap<String, CachedWord> = if cache.is_undefined() || cache.is_null() {
            HashMap::new()
        } else {
            serde_wasm_bindgen::from_value(cache).map_err(|e| JsValue::from_str(&e.to_string()))?
        };

        // Never stored as a field, same "build fresh per call from an owned `&Grammar`" shape as `FomaState`'s rehydrated `FomaAnalyzer` below.
        let mut new_cache_entries: HashMap<String, CachedWord> = HashMap::new();
        self.ensure_foma();

        // Rehydrate a `FomaAnalyzer` borrowing `&self.grammar`; the checkout guard restores the compiled pieces on every exit path, including panic unwinding.
        let mut foma = FomaCheckout::new(&mut self.foma, &self.grammar);

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
                    // Machine/LibLCM lexical identity is writing-system aware; preserve the authored token exactly.
                    let lexical = word.to_string();

                    let current_revision = self.runtime.snapshot().revision().clone();
                    let (cached, from_cache, parse_ms) = if let Some(hit) = cache
                        .get(&lexical)
                        .filter(|hit| hit.overlay_revision == current_revision)
                    {
                        (hit.clone(), true, 0.0)
                    } else {
                        let start = Instant::now();
                        let official = foma.analyzer_mut().map(|analyzer| {
                            let outcome = analyzer.analyze_word(&lexical);
                            pg_lexicon::OfficialOutcome {
                                analyses: outcome.analyses,
                                structured: outcome.structured,
                                candidates_generated: outcome.candidates_generated,
                            }
                        });
                        // `guess_fallback: true`, explicit and load-bearing: unlike the FFI wire format (which can't mark a guessed analysis), this demo always presents a guess as a guess, so the retry stays on here.
                        let outcome = self.runtime.analyze_word_opts(&lexical, official, true);
                        let structured = outcome.structured;
                        let capped = outcome.capped;
                        let invalid_shape = outcome.invalid_shape;
                        let candidates_generated = outcome.candidates_generated;
                        let overlay_revision = outcome.revision;
                        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                        let fresh = build_cached_word(
                            &self.grammar,
                            &self.realize_map,
                            &self.realizer,
                            &lexical,
                            CacheAnalysis {
                                structured,
                                capped,
                                invalid_shape,
                                candidates_generated,
                                overlay_revision,
                            },
                        );
                        new_cache_entries.insert(lexical.clone(), fresh.clone());
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

        let result = AnalyzeTextResult {
            tokens,
            new_cache_entries,
        };
        // Default serde-wasm-bindgen serializes a Rust `HashMap` as a JS `Map`, but `new_cache_entries` must come back as a plain object so the caller can `Object.assign` it into its own cache.
        let serializer = serde_wasm_bindgen::Serializer::json_compatible();
        result
            .serialize(&serializer)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    #[wasm_bindgen(js_name = classCatalog)]
    pub fn class_catalog(&self) -> Result<JsValue, JsValue> {
        let snapshot = self.runtime.snapshot();
        to_js(&CatalogOut {
            signatures: self.runtime.catalog().signatures(),
            revision: snapshot.revision(),
        })
    }

    #[wasm_bindgen(js_name = addSuppliedEntry)]
    pub fn add_supplied_entry(&self, request: JsValue) -> Result<JsValue, JsValue> {
        to_js(&self.runtime.add(from_js(request)?).map_err(structured_js)?)
    }

    #[wasm_bindgen(js_name = getSuppliedEntry)]
    pub fn get_supplied_entry(&self, id: &str) -> Result<JsValue, JsValue> {
        let id = pg_lexicon::EntryId::parse(id).map_err(structured_js)?;
        let entry = self
            .runtime
            .get(&id)
            .ok_or_else(|| structured_js(api_error("entry_not_found", "entry not found")))?;
        to_js(&entry)
    }

    #[wasm_bindgen(js_name = listSuppliedEntries)]
    pub fn list_supplied_entries(&self) -> Result<JsValue, JsValue> {
        to_js(&self.runtime.list())
    }

    #[wasm_bindgen(js_name = searchSuppliedEntries)]
    pub fn search_supplied_entries(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: pg_lexicon::SearchRequest = from_js(request)?;
        to_js(&self.runtime.search(&request))
    }

    #[wasm_bindgen(js_name = updateSuppliedEntry)]
    pub fn update_supplied_entry(&self, request: JsValue) -> Result<JsValue, JsValue> {
        to_js(
            &self
                .runtime
                .update(from_js(request)?)
                .map_err(structured_js)?,
        )
    }

    #[wasm_bindgen(js_name = removeSuppliedEntry)]
    pub fn remove_supplied_entry(&self, request: JsValue) -> Result<JsValue, JsValue> {
        to_js(
            &self
                .runtime
                .remove(from_js(request)?)
                .map_err(structured_js)?,
        )
    }

    #[wasm_bindgen(js_name = clearSuppliedEntries)]
    pub fn clear_supplied_entries(&self, request: JsValue) -> Result<JsValue, JsValue> {
        to_js(
            &self
                .runtime
                .clear(from_js(request)?)
                .map_err(structured_js)?,
        )
    }

    #[wasm_bindgen(js_name = setGlossLanguage)]
    pub fn set_gloss_language(&self, request: JsValue) -> Result<JsValue, JsValue> {
        to_js(
            &self
                .runtime
                .set_gloss_language(from_js(request)?)
                .map_err(structured_js)?,
        )
    }

    #[wasm_bindgen(js_name = setEntryAuthority)]
    pub fn set_entry_authority(&self, request: JsValue) -> Result<JsValue, JsValue> {
        to_js(
            &self
                .runtime
                .set_authority(from_js(request)?)
                .map_err(structured_js)?,
        )
    }

    #[wasm_bindgen(js_name = exportSuppliedLexicon)]
    pub fn export_supplied_lexicon(&self) -> Result<JsValue, JsValue> {
        to_js(&self.runtime.export_document())
    }

    #[wasm_bindgen(js_name = importSuppliedLexicon)]
    pub fn import_supplied_lexicon(&self, request: JsValue) -> Result<JsValue, JsValue> {
        to_js(
            &self
                .runtime
                .import(from_js(request)?)
                .map_err(structured_js)?,
        )
    }

    #[wasm_bindgen(js_name = classificationMatrix)]
    pub fn classification_matrix(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request = from_js(request)?;
        let matrix = pg_lexicon::classify(&self.grammar, self.runtime.catalog(), request)
            .map_err(structured_js)?;
        to_js(&matrix)
    }

    #[wasm_bindgen(js_name = analyzeWord)]
    pub fn analyze_word(&mut self, word: &str) -> Result<JsValue, JsValue> {
        let outcome = self.analyze_unified(word);
        to_js(&UnifiedAnalysisOut::from(outcome))
    }
}

impl PanGlossGrammar {
    fn ensure_foma(&mut self) {
        if self.foma.is_none() && self.foma_diagnostic.is_none() {
            match build_foma_state(&self.grammar) {
                Ok(state) => self.foma = Some(state),
                Err(message) => {
                    log_foma_fallback(&message);
                    self.foma_diagnostic = Some(message);
                }
            }
        }
    }

    /// `guess_fallback: true`, same rationale as the twin call site in `analyze_text`: every guess this crate returns reaches JS carrying `UnifiedAnalysisOut`'s own `guessed` flag.
    fn analyze_unified(&mut self, word: &str) -> pg_lexicon::UnifiedAnalysis {
        self.ensure_foma();
        let mut foma = FomaCheckout::new(&mut self.foma, &self.grammar);
        let official = foma.analyzer_mut().map(|analyzer| {
            let outcome = analyzer.analyze_word(word);
            pg_lexicon::OfficialOutcome {
                analyses: outcome.analyses,
                structured: outcome.structured,
                candidates_generated: outcome.candidates_generated,
            }
        });
        self.runtime.analyze_word_opts(word, official, true)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogOut<'a> {
    signatures: &'a [pg_lexicon::ClassSignature],
    revision: &'a pg_lexicon::Revision,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnifiedAnalysisOut {
    analyses: Vec<(String, String)>,
    structured: Vec<StructuredAnalysisOut>,
    capped: bool,
    invalid_shape: bool,
    timed_out: bool,
    guessed: bool,
    candidates_generated: usize,
    revision: pg_lexicon::Revision,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StructuredAnalysisOut {
    morpheme_ids: Vec<u32>,
    root_morpheme_index: i32,
    pos_id: Option<u32>,
    guessed: bool,
    provenance: pg_parse::AnalysisProvenance,
    supplied_root: Option<pg_parse::SuppliedRoot>,
}

impl From<pg_lexicon::UnifiedAnalysis> for UnifiedAnalysisOut {
    fn from(value: pg_lexicon::UnifiedAnalysis) -> Self {
        Self {
            analyses: value.analyses,
            structured: value
                .structured
                .into_iter()
                .map(|analysis| StructuredAnalysisOut {
                    morpheme_ids: analysis.morpheme_ids,
                    root_morpheme_index: analysis.root_morpheme_index,
                    pos_id: analysis.pos_id,
                    guessed: analysis.guessed,
                    provenance: analysis.provenance,
                    supplied_root: analysis.supplied_root,
                })
                .collect(),
            capped: value.capped,
            invalid_shape: value.invalid_shape,
            timed_out: value.timed_out,
            guessed: value.guessed,
            candidates_generated: value.candidates_generated,
            revision: value.revision,
        }
    }
}

fn from_js<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(|error| {
        structured_js(pg_lexicon::StructuredError {
            code: "invalid_json".into(),
            message: "request does not match the PanGloss JSON schema".into(),
            details: serde_json::json!({"error":error.to_string()}),
        })
    })
}

fn to_js<T: Serialize + ?Sized>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

fn structured_js(error: pg_lexicon::StructuredError) -> JsValue {
    to_js(&error).unwrap_or_else(|serialization| serialization)
}

fn api_error(code: &str, message: &str) -> pg_lexicon::StructuredError {
    pg_lexicon::StructuredError {
        code: code.into(),
        message: message.into(),
        details: serde_json::Value::Null,
    }
}

/// The wasm-bindgen-facing handle for a validated `.pgpack` artifact.
/// Construction runs `pack::load_pack`: the
/// container's own structural validation, then ADR 0004's `required ⊆ provided` runtime-feature
/// containment check against this build's own `pack::provided_runtime_features` -- replacing
/// what would otherwise be a monolithic engine-compatibility-identifier equality check. A pack
/// requiring a runtime feature this build does not provide is refused here with a typed
/// `pack_incompatible_runtime_features` diagnostic (see `pack_load_err_to_js`), never a crash.
///
/// Every getter below is a read-only view over the manifest `pack::load_pack` already accepted;
/// this handle constructs no working analyzer from the packaged runtime/foma payload
/// bytes -- see `pack`'s own module doc "Analysis-only boundary" section for that scope
/// boundary. Loading a pack here performs zero FST/lexc compilation.
#[wasm_bindgen]
pub struct PgPack {
    loaded: pack::LoadedPack,
}

#[wasm_bindgen]
impl PgPack {
    /// `bytes` is a complete `.pgpack` container (see `pg_pack::format`'s own byte-layout doc).
    /// `Err` iff the container itself is structurally invalid (bad magic/version, oversize or
    /// truncated section, digest or fingerprint mismatch, ...) OR iff its
    /// `required_runtime_features` is not a subset of this build's `provided` set -- both fail
    /// closed with a typed, JS-inspectable diagnostic rather than a panic.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<PgPack, JsValue> {
        let loaded = pack::load_pack(bytes).map_err(pack_load_err_to_js)?;
        Ok(PgPack { loaded })
    }

    /// `pg_pack::PackManifest::grammar_id` -- this pack's stable grammar/package identity.
    #[wasm_bindgen(js_name = grammarId)]
    pub fn grammar_id(&self) -> String {
        self.loaded.manifest.grammar_id.clone()
    }

    /// ADR 0005's pack-level degraded-trust signal: `true` iff this pack was force-compiled past
    /// a characteristics-check refusal (`pg_pack::CapabilityTrust::Overridden`) and is therefore
    /// indelibly unproven/recall-unsafe. A consuming application keys its warning banner off this
    /// at load time; the pack still loaded and may still be analyzed -- the signal, not a refusal,
    /// is the safety mechanism (ADR 0005).
    #[wasm_bindgen(js_name = isUnproven)]
    pub fn is_unproven(&self) -> bool {
        self.loaded.is_unproven()
    }

    /// The same ADR 0005 signal as `PgPack::is_unproven`, exposed under the name a per-analysis-
    /// result flag on packaged-artifact analysis should also use: every
    /// result drawn from an unproven pack must carry this flag.
    #[wasm_bindgen(js_name = analysisTrustFlag)]
    pub fn analysis_trust_flag(&self) -> bool {
        self.loaded.analysis_trust_flag()
    }

    /// `"unsigned"`, `"valid"`, or `"invalid"` (`pg_pack::SignatureState`). Reported for the
    /// caller's information only -- R2A: signature state never gates a load or analysis, so this
    /// is meaningful regardless of its value.
    #[wasm_bindgen(js_name = signatureState)]
    pub fn signature_state(&self) -> String {
        match self.loaded.signature_state {
            pg_pack::SignatureState::Unsigned => "unsigned",
            pg_pack::SignatureState::Valid => "valid",
            pg_pack::SignatureState::Invalid => "invalid",
        }
        .to_string()
    }

    /// The FST-health "admission result" (`pg_foma::health::HealthReport::admission`, reused
    /// verbatim -- see `pack::LoadedPack::fst_health_admission`'s doc), as its lowercase
    /// `Severity` name (`"within_limits"`, `"elevated"`, `"large_multiplier"`,
    /// `"not_production_ready"`, `"machine_limit"`, or `"cannot_represent"`).
    #[wasm_bindgen(js_name = fstHealthAdmission)]
    pub fn fst_health_admission(&self) -> String {
        match self.loaded.fst_health_admission() {
            pg_foma::health::Severity::WithinLimits => "within_limits",
            pg_foma::health::Severity::Elevated => "elevated",
            pg_foma::health::Severity::LargeMultiplier => "large_multiplier",
            pg_foma::health::Severity::NotProductionReady => "not_production_ready",
            pg_foma::health::Severity::MachineLimit => "machine_limit",
            pg_foma::health::Severity::CannotRepresent => "cannot_represent",
        }
        .to_string()
    }

    /// The stable boolean publication gate: `false` iff `fstHealthAdmission` has reached the
    /// tier that blocks publication (`NotProductionReady` or worse), `true` otherwise. Add this
    /// where `fstHealthAdmission`'s exact string is not needed -- its meaning cannot rot the way
    /// a string-equality check against `fstHealthAdmission` can across a schema rename.
    #[wasm_bindgen(js_name = fstHealthIsPublishable)]
    pub fn fst_health_is_publishable(&self) -> bool {
        self.loaded.fst_health_is_publishable()
    }

    /// The complete FST-health report (`pg_foma::health::HealthReport`, reused verbatim) as its
    /// own canonical JSON shape -- every finding, not just the aggregated admission severity.
    #[wasm_bindgen(js_name = fstHealthReport)]
    pub fn fst_health_report(&self) -> Result<JsValue, JsValue> {
        to_js(&self.loaded.manifest.fst_health)
    }

    /// This pack's required-runtime-feature set (`pg_pack::RequiredRuntimeFeatures`), the same
    /// value `PgPack::new` already checked against this build's provided set.
    #[wasm_bindgen(js_name = requiredRuntimeFeatures)]
    pub fn required_runtime_features(&self) -> Result<JsValue, JsValue> {
        to_js(&self.loaded.manifest.required_runtime_features)
    }
}

/// Maps `pack::PackLoadError` to this crate's usual `StructuredError` JSON shape, so JS callers get one consistent `{code, message, details}` diagnostic regardless of which layer refused the pack.
fn pack_load_err_to_js(err: pack::PackLoadError) -> JsValue {
    let structured = match err {
        pack::PackLoadError::Container(inner) => pg_lexicon::StructuredError {
            code: "pack_container_invalid".into(),
            message: inner.to_string(),
            details: serde_json::Value::Null,
        },
        pack::PackLoadError::IncompatibleRuntimeFeatures { required, provided } => {
            pg_lexicon::StructuredError {
                code: "pack_incompatible_runtime_features".into(),
                message: "pack requires a runtime feature this Runtime build does not provide \
                    (ADR 0004): upgrade PanGloss to run this grammar"
                    .into(),
                details: serde_json::json!({
                    "required": required,
                    "provided": {
                        "payloadFormatVersions": provided.payload_format_versions,
                        "runtimeOperations": provided.runtime_operations,
                        "fomaFeatureLevel": provided.foma_feature_level,
                        "hcPortSemver": [
                            provided.hc_port_semver.0,
                            provided.hc_port_semver.1,
                            provided.hc_port_semver.2,
                        ],
                        "extensions": provided.extensions,
                    },
                }),
            }
        }
    };
    structured_js(structured)
}

#[wasm_bindgen]
pub struct ClassificationGuide {
    inner: pg_lexicon::ClassificationGuide,
}

#[wasm_bindgen]
impl ClassificationGuide {
    #[wasm_bindgen(constructor)]
    pub fn new(matrix: JsValue) -> Result<ClassificationGuide, JsValue> {
        Ok(Self {
            inner: pg_lexicon::ClassificationGuide::new(from_js(matrix)?),
        })
    }

    pub fn answer(&mut self, form_id: &str, judgment: JsValue) -> Result<JsValue, JsValue> {
        self.inner
            .answer(form_id, from_js(judgment)?)
            .map_err(structured_js)?;
        to_js(&serde_json::json!({"answered":true}))
    }

    pub fn undo(&mut self) -> bool {
        self.inner.undo()
    }

    #[wasm_bindgen(js_name = remainingSignatures)]
    pub fn remaining_signatures(&self) -> Result<JsValue, JsValue> {
        to_js(&self.inner.remaining_signatures())
    }

    #[wasm_bindgen(js_name = nextForm)]
    pub fn next_form(&self) -> Result<JsValue, JsValue> {
        to_js(&self.inner.next_form())
    }

    #[wasm_bindgen(js_name = allUsefulForms)]
    pub fn all_useful_forms(&self) -> Result<JsValue, JsValue> {
        to_js(&self.inner.all_useful_forms())
    }

    #[wasm_bindgen(js_name = finalSelection)]
    pub fn final_selection(&self) -> Result<JsValue, JsValue> {
        to_js(&self.inner.final_selection())
    }

    pub fn matrix(&self) -> Result<JsValue, JsValue> {
        to_js(self.inner.matrix())
    }
}

fn js_err(action: &str, e: &dyn std::fmt::Debug) -> JsValue {
    JsValue::from_str(&format!("{action}: {e:?}"))
}

enum Piece<'a> {
    Word(&'a str),
    Other(&'a str),
}

/// Splits `text` into alternating word/other runs; a "word" run is a maximal span of alphabetic-or-apostrophe characters (apostrophe included since it's phonemic in some orthographies), anything else is an "other" run passed through unanalyzed. Concatenating every piece's text reconstructs `text` exactly.
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
        // "hi" / ", 16" / "th" / "!": punctuation, whitespace, and digits merge into one "other" run; only word-vs-other transitions split pieces.
        let pieces = tokenize("hi, 16th!");
        let kinds: Vec<bool> = pieces.iter().map(|p| matches!(p, Piece::Word(_))).collect();
        assert_eq!(kinds, vec![true, false, true, false]);
    }

    // A small, hand-built, original HermitCrab XML fixture, just enough to exercise affix_glosses/build_realize_map; PanGlossGrammar's wasm-bindgen methods can't easily be driven from a plain cargo test, so these tests target the plain-Rust helpers directly.
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
        let g = pg_grammar::load(TEST_XML).expect("test fixture loads");
        let glosses = affix_glosses(&g);
        assert!(
            glosses.iter().any(|s| s == "pl"),
            "the plural rule's gloss must be included: {glosses:?}"
        );
        assert!(
            !glosses.iter().any(|s| s == "house"),
            "a lexical-entry (root) gloss must not be treated as an affix gloss: {glosses:?}"
        );
    }

    #[test]
    fn build_realize_map_infers_from_affix_glosses_when_no_sidecar() {
        let g = pg_grammar::load(TEST_XML).expect("test fixture loads");
        let map = build_realize_map(&g, None).expect("builds base map");
        assert_eq!(
            map.lookup("pl"),
            Some(pg_realize::FeatureAssignment::Num(pg_realize::Num::Pl)),
            "the built-in English alias table must recognize the affix rule's 'pl' gloss"
        );
    }

    #[test]
    fn build_realize_map_lets_sidecar_override_the_inferred_base() {
        let g = pg_grammar::load(TEST_XML).expect("test fixture loads");
        let sidecar = "[features]\n\"pl\" = \"Ignore\"\n";
        let map = build_realize_map(&g, Some(sidecar)).expect("builds overridden map");
        assert_eq!(
            map.lookup("pl"),
            Some(pg_realize::FeatureAssignment::Ignore),
            "an explicit sidecar mapping must win over the inferred base for the same gloss key"
        );
    }

    #[test]
    fn build_realize_map_treats_blank_sidecar_the_same_as_none() {
        let g = pg_grammar::load(TEST_XML).expect("test fixture loads");
        let with_none = build_realize_map(&g, None).expect("builds with None");
        let with_blank = build_realize_map(&g, Some("   \n")).expect("builds with blank sidecar");
        assert_eq!(with_none, with_blank);
    }

    #[test]
    fn proposer_panic_or_trap_preserves_logical_backend_and_official_analysis_recovers() {
        let grammar = Arc::new(pg_grammar::load(TEST_XML).expect("test fixture loads"));
        let (foma, foma_diagnostic) = init_foma(&grammar);
        let mut wrapper = PanGlossGrammar {
            runtime: pg_lexicon::SuppliedLexiconRuntime::new(grammar.clone(), TEST_XML)
                .expect("runtime initializes"),
            realize_map: build_realize_map(&grammar, None).expect("realize map builds"),
            realizer: TableRealizer::new().expect("embedded realizer loads"),
            grammar,
            foma,
            foma_diagnostic,
        };
        assert_eq!(wrapper.engine_kind(), "foma");
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut checkout = FomaCheckout::new(&mut wrapper.foma, &wrapper.grammar);
            assert!(checkout.analyzer_mut().is_some());
            panic!("injected proposer panic");
        }));
        assert!(panic.is_err());
        assert_eq!(wrapper.engine_kind(), "foma");
        let checkout = FomaCheckout::new(&mut wrapper.foma, &wrapper.grammar);
        std::mem::forget(checkout);
        assert!(
            wrapper.foma.is_none(),
            "simulated trap strands the checkout"
        );
        assert_eq!(wrapper.engine_kind(), "foma");
        let recovered = wrapper.analyze_unified("house");
        assert!(
            recovered
                .structured
                .iter()
                .any(|analysis| analysis.provenance == pg_parse::AnalysisProvenance::Grammar),
            "the same compiled backend must still produce official analyses after unwind"
        );
    }

    // Native parity smoke: not a browser round-trip, but the identical Rust functions PanGlossGrammar::new/analyze_text call, compiled natively, run over real corpus words and compared against the full engine as multisets.

    /// Loads `grammar_file`/`words_file` from `samples/data/`, skipping quietly if either is absent, takes the first `sample` non-empty lines, and asserts the foma path's confirmed analyses exactly multiset-match the full engine's.
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
        let grammar =
            pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {grammar_file}: {e}"));
        let foma_state = build_foma_state(&grammar)
            .unwrap_or_else(|e| panic!("{grammar_file} must foma-compile (gate F1): {e}"));
        let mut analyzer = pg_foma::composite::FomaAnalyzer::from_cached(
            &grammar,
            foma_state.proposer,
            foma_state.peeler,
            foma_state.owners,
            foma_state.filter,
        );
        let morpher = pg_parse::Morpher::new(&grammar, usize::MAX);
        let opts = pg_parse::ParseOptions::default();

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
    #[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
    fn foma_path_matches_full_engine_on_sena_sample() {
        assert_foma_matches_engine("sena-hc.xml", "sena-words.txt", 40);
    }

    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
    fn foma_path_matches_full_engine_on_indonesian_corpus() {
        // Indonesian's whole corpus file is only 121 words, small enough to run in full rather than sampling.
        assert_foma_matches_engine("indonesian-hc.xml", "indonesian-words.txt", usize::MAX);
    }
}
