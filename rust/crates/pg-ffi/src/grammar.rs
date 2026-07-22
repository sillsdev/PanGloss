//! Grammar handle: load, free, and the self-referential `Morpher` construction (plan §4.2).
//!
//! The handle is immutable and `Send + Sync` once built (compile-time-checked below): one
//! `hc_grammar_load` can back any number of concurrent `hc_parse_word` callers, or one internally
//! parallel `hc_parse_batch` call. **Safety contract the caller must uphold** (undocumentable in
//! the C signature itself, so stated here and in the crate docs): do not call `hc_grammar_free`
//! on a handle while any `hc_parse_word`/`hc_parse_batch` call using it is still in flight on
//! another thread. This is the same "read-only while shared, exclusive to free" contract as any
//! `Arc`-free C API; a future revision could enforce it with a refcount if FieldWorks needs it.

use pg_grammar::model::Grammar;
use pg_parse::Morpher;
use std::sync::{Arc, Mutex};

use crate::error::{
    clear_error, set_error, HcError, HC_ERR_GRAMMAR_LOAD, HC_ERR_NULL_ARG, HC_ERR_UTF8, HC_OK,
};

/// Opaque handle type in the C ABI (`typedef void* HcGrammarHandle;`). Actually a
/// `Box<GrammarHandle>` leaked via [`Box::into_raw`]; reclaimed by [`hc_grammar_free`].
pub type HcGrammarHandle = *mut std::ffi::c_void;

/// Matches the Indonesian parity-gate configuration already established by the M6/M7 work
/// (`--step-cap 500000 --memo=on`; see MEMORY `rust-parity-facts`): a bounded budget by default
/// so a native host cannot trigger the unbounded-memory runaway documented there (a prior session
/// hit 55GB+ on an unmemoized/uncapped Indonesian word) merely by loading a grammar and parsing.
/// Plan §4.2's ABI has no step-cap/memo parameter yet in `hc_grammar_load` — a candidate addition
/// for the #448 budgets port (M10) if a host ever needs a different budget.
///
/// `pub` so tests can build a plain in-process `pg_parse::Morpher` with the identical
/// configuration the FFI handle uses internally — the FFI-vs-in-process parity test needs both
/// sides run under the same budget/memo settings, or a mismatch there (not an encoding bug) could
/// masquerade as one.
pub const DEFAULT_ANALYSIS_POLICY: pg_lexicon::AnalysisPolicy =
    pg_lexicon::AnalysisPolicy::native_abi_v1();
pub const DEFAULT_STEP_CAP: usize = DEFAULT_ANALYSIS_POLICY.step_cap;
pub const DEFAULT_MEMO: bool = DEFAULT_ANALYSIS_POLICY.memo;

fn native_v1_morpher(grammar: &'static Grammar) -> Morpher<'static> {
    Morpher::new(grammar, DEFAULT_ANALYSIS_POLICY.step_cap).with_memo(DEFAULT_ANALYSIS_POLICY.memo)
}

struct FomaState {
    proposer: pg_foma::analyzer::FomaProposer,
    peeler: pg_foma::peel::ReduplicationPeeler,
    owners: Vec<Option<pg_foma::confirm::MorphemeOwner>>,
}

enum OfficialBackend {
    Foma(Box<FomaState>),
    MorpherFallback { diagnostic: String },
}

impl FomaState {
    fn new(grammar: &Grammar) -> Result<Self, String> {
        Ok(Self {
            proposer: pg_foma::analyzer::FomaProposer::new(grammar).map_err(|e| e.to_string())?,
            peeler: pg_foma::peel::ReduplicationPeeler::new(grammar),
            owners: pg_foma::confirm::build_morpheme_owners(grammar),
        })
    }
}

/// The boxed, self-owning grammar + pre-built `Morpher`.
///
/// `Morpher<'g>` borrows the `Grammar` it parses against; to store both in one heap allocation
/// behind an opaque C pointer, `grammar` is boxed first (a stable heap address that does not move
/// even if this outer struct is moved — only the `Box`'s pointer moves), and `morpher` holds a
/// `'static`-lifetime-erased reference into it, constructed once in [`GrammarHandle::new`] and
/// never re-pointed. This is sound because:
/// - the boxed `Grammar` is never mutated or moved out of this struct after construction, so its
///   heap address is stable for the handle's whole lifetime;
/// - `morpher` (declared first, so it drops first — see field order below) never outlives
///   `grammar`, since both live and die inside the same `Box<GrammarHandle>`; and
/// - `HcGrammarHandle` never exposes the `Grammar` reference or the `Morpher` separately, only
///   this struct as a single opaque unit.
pub(crate) struct GrammarHandle {
    /// Declared before `grammar` so it drops first. Neither type's `Drop` impl (there isn't one)
    /// dereferences the other, so this ordering isn't load-bearing for *this* struct today — but
    /// it is the correct discipline for a self-referential struct regardless, and keeps this
    /// sound if either type ever gains a `Drop` impl later.
    pub(crate) morpher: Morpher<'static>,
    pub(crate) runtime: pg_lexicon::SuppliedLexiconRuntime,
    /// Owned, immutable-with-respect-to-the-grammar proposer pieces. The foma runtime itself
    /// needs mutable access while proposing, so calls briefly check the pieces out under a lock.
    official_backend: Mutex<OfficialBackend>,
    #[cfg(test)]
    force_next_foma_panic: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    force_next_pool_build_failure: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    pool_build_count: std::sync::atomic::AtomicUsize,
    /// Never read directly after construction — it exists purely to *own* the allocation
    /// `morpher` points into (dropping it is what actually frees the grammar). That's a real
    /// use the `dead_code` lint can't see, hence the explicit allow.
    #[allow(dead_code)]
    pub(crate) grammar: Arc<Grammar>,
}

impl GrammarHandle {
    fn new(grammar: Grammar, grammar_source: &str) -> Box<GrammarHandle> {
        let grammar = Arc::new(grammar);
        // SAFETY: `grammar` is heap-allocated and, per the struct docs above, never moved or
        // mutated again for the lifetime of the `GrammarHandle` being constructed — only this
        // function ever had `&mut` access to it, and that access ends here. The transmuted
        // lifetime therefore only asserts something true: the referent outlives every use of
        // `morpher`, because both are dropped together (as fields of the same struct) and
        // `morpher` (declared first) drops before `grammar` is freed.
        let grammar_ref: &'static Grammar = unsafe { &*(grammar.as_ref() as *const Grammar) };
        let morpher = native_v1_morpher(grammar_ref);
        let runtime = pg_lexicon::SuppliedLexiconRuntime::with_policy(
            grammar.clone(),
            grammar_source,
            DEFAULT_ANALYSIS_POLICY,
        )
        .expect("a successfully loaded named grammar initializes its runtime");
        let foma = FomaState::new(&grammar);
        Self::new_with_foma_result_inner(grammar, morpher, runtime, foma)
    }

    #[cfg(test)]
    fn new_with_foma_result(
        grammar: Grammar,
        grammar_source: &str,
        foma: Result<FomaState, String>,
    ) -> Box<GrammarHandle> {
        let grammar = Arc::new(grammar);
        // SAFETY: identical ownership/lifetime argument to `new` above.
        let grammar_ref: &'static Grammar = unsafe { &*(grammar.as_ref() as *const Grammar) };
        let morpher = native_v1_morpher(grammar_ref);
        let runtime = pg_lexicon::SuppliedLexiconRuntime::with_policy(
            grammar.clone(),
            grammar_source,
            DEFAULT_ANALYSIS_POLICY,
        )
        .expect("a successfully loaded named grammar initializes its runtime");
        Self::new_with_foma_result_inner(grammar, morpher, runtime, foma)
    }

    fn new_with_foma_result_inner(
        grammar: Arc<Grammar>,
        morpher: Morpher<'static>,
        runtime: pg_lexicon::SuppliedLexiconRuntime,
        foma: Result<FomaState, String>,
    ) -> Box<GrammarHandle> {
        let official_backend = match foma {
            Ok(state) => OfficialBackend::Foma(Box::new(state)),
            Err(diagnostic) => OfficialBackend::MorpherFallback { diagnostic },
        };
        Box::new(GrammarHandle {
            morpher,
            runtime,
            official_backend: Mutex::new(official_backend),
            #[cfg(test)]
            force_next_foma_panic: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            force_next_pool_build_failure: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            pool_build_count: std::sync::atomic::AtomicUsize::new(0),
            grammar,
        })
    }

    pub(crate) fn analyze_word(&self, word: &str) -> pg_lexicon::UnifiedAnalysis {
        let mut backend = self
            .official_backend
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let checked_out = std::mem::replace(
            &mut *backend,
            OfficialBackend::MorpherFallback {
                diagnostic: "foma state temporarily checked out".into(),
            },
        );
        let state = match checked_out {
            OfficialBackend::Foma(state) => *state,
            OfficialBackend::MorpherFallback { diagnostic } => {
                *backend = OfficialBackend::MorpherFallback { diagnostic };
                drop(backend);
                // `None` deliberately selects the unified runtime's authoritative grammar
                // Morpher, not an overlay-only path.
                return self.runtime.analyze_word(word, None);
            }
        };
        let attempted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.test_panic_if_requested();
            let mut analyzer = pg_foma::composite::FomaAnalyzer::from_cached(
                &self.grammar,
                state.proposer,
                state.peeler,
                state.owners,
            );
            let outcome = analyzer.analyze_word(word);
            let (proposer, peeler, owners) = analyzer.into_parts();
            (
                pg_lexicon::OfficialOutcome {
                    analyses: outcome.analyses,
                    structured: outcome.structured,
                    candidates_generated: outcome.candidates_generated,
                },
                FomaState {
                    proposer,
                    peeler,
                    owners,
                },
            )
        }));
        match attempted {
            Ok((official, state)) => {
                *backend = OfficialBackend::Foma(Box::new(state));
                drop(backend);
                self.runtime.analyze_word(word, Some(official))
            }
            Err(payload) => {
                *backend = match FomaState::new(&self.grammar) {
                    Ok(state) => OfficialBackend::Foma(Box::new(state)),
                    Err(diagnostic) => OfficialBackend::MorpherFallback { diagnostic },
                };
                drop(backend);
                std::panic::resume_unwind(payload)
            }
        }
    }

    /// Analyze a batch while holding the mutable foma backend lock only for serialized proposal.
    /// Confirmation (the dominant official cost) and overlay union run outside that lock with the
    /// caller's requested parallelism.
    pub(crate) fn analyze_words(
        &self,
        words: &[String],
        max_threads: usize,
    ) -> Result<Vec<(pg_lexicon::UnifiedAnalysis, std::time::Duration)>, ()> {
        for word in words {
            pg_parse::batch::test_panic_if_requested(word);
        }
        let pool = self.build_batch_pool(max_threads)?;
        let mut backend = self
            .official_backend
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let checked_out = std::mem::replace(
            &mut *backend,
            OfficialBackend::MorpherFallback {
                diagnostic: "foma state temporarily checked out".into(),
            },
        );
        let (proposed, owners) = match checked_out {
            OfficialBackend::MorpherFallback { diagnostic } => {
                *backend = OfficialBackend::MorpherFallback { diagnostic };
                drop(backend);
                return Ok(self.analyze_words_without_official(words, &pool));
            }
            OfficialBackend::Foma(state) => {
                let state = *state;
                let attempted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.test_panic_if_requested();
                    let mut analyzer = pg_foma::composite::FomaAnalyzer::from_cached(
                        &self.grammar,
                        state.proposer,
                        state.peeler,
                        state.owners,
                    );
                    let proposed = analyzer.propose_words(words);
                    let (proposer, peeler, owners) = analyzer.into_parts();
                    (proposed, proposer, peeler, owners)
                }));
                match attempted {
                    Ok((proposed, proposer, peeler, owners)) => {
                        let confirm_owners = owners.clone();
                        *backend = OfficialBackend::Foma(Box::new(FomaState {
                            proposer,
                            peeler,
                            owners,
                        }));
                        drop(backend);
                        (proposed, confirm_owners)
                    }
                    Err(payload) => {
                        *backend = match FomaState::new(&self.grammar) {
                            Ok(state) => OfficialBackend::Foma(Box::new(state)),
                            Err(diagnostic) => OfficialBackend::MorpherFallback { diagnostic },
                        };
                        drop(backend);
                        std::panic::resume_unwind(payload)
                    }
                }
            }
        };
        let official = pg_foma::composite::confirm_proposed_words_in_pool(
            &self.grammar,
            &owners,
            words,
            proposed,
            &pool,
        );
        Ok(self.union_official_batch(words, official, &pool))
    }

    fn build_batch_pool(&self, max_threads: usize) -> Result<rayon::ThreadPool, ()> {
        #[cfg(test)]
        if self
            .force_next_pool_build_failure
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(());
        }
        let mut builder = rayon::ThreadPoolBuilder::new().stack_size(1 << 30);
        if max_threads > 0 {
            builder = builder.num_threads(max_threads);
        }
        let pool = builder.build().map_err(|_| ())?;
        #[cfg(test)]
        self.pool_build_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(pool)
    }

    fn analyze_words_without_official(
        &self,
        words: &[String],
        pool: &rayon::ThreadPool,
    ) -> Vec<(pg_lexicon::UnifiedAnalysis, std::time::Duration)> {
        self.parallel_map(words, pool, |word| {
            let started = std::time::Instant::now();
            let outcome = self.runtime.analyze_word(word, None);
            (outcome, started.elapsed())
        })
    }

    fn union_official_batch(
        &self,
        words: &[String],
        official: Vec<(pg_foma::composite::FomaOutcome, std::time::Duration)>,
        pool: &rayon::ThreadPool,
    ) -> Vec<(pg_lexicon::UnifiedAnalysis, std::time::Duration)> {
        let inputs: Vec<_> = words.iter().zip(official).collect();
        self.parallel_map(&inputs, pool, |(word, (outcome, elapsed))| {
            let started = std::time::Instant::now();
            let unified = self.runtime.analyze_word(
                word,
                Some(pg_lexicon::OfficialOutcome {
                    analyses: outcome.analyses.clone(),
                    structured: outcome.structured.clone(),
                    candidates_generated: outcome.candidates_generated,
                }),
            );
            (unified, *elapsed + started.elapsed())
        })
    }

    fn parallel_map<T: Sync, R: Send>(
        &self,
        inputs: &[T],
        pool: &rayon::ThreadPool,
        operation: impl Fn(&T) -> R + Sync + Send,
    ) -> Vec<R> {
        use rayon::prelude::*;
        pool.install(|| inputs.par_iter().map(operation).collect())
    }

    #[cfg(test)]
    fn backend_kind(&self) -> &'static str {
        match &*self
            .official_backend
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            OfficialBackend::Foma(_) => "foma",
            OfficialBackend::MorpherFallback { diagnostic } => {
                assert!(!diagnostic.is_empty());
                "morpherFallback"
            }
        }
    }

    #[cfg(test)]
    fn force_next_foma_panic(&self) {
        self.force_next_foma_panic
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(test)]
    fn force_next_pool_build_failure_for_test(&self) {
        self.force_next_pool_build_failure
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(test)]
    fn pool_build_count_for_test(&self) -> usize {
        self.pool_build_count
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn test_panic_if_requested(&self) {
        #[cfg(test)]
        if self
            .force_next_foma_panic
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            panic!("forced native foma analyzer panic");
        }
    }
}

// Compile-time proof the handle can be shared across host threads, per plan §4.2 ("immutable and
// Send + Sync"). If a future field addition ever breaks this, it fails to compile right here
// instead of silently becoming a data race reachable only under concurrent FFI load.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GrammarHandle>();
};

/// `hc_grammar_load(const uint8_t* xml_utf8, size_t len, HcGrammarHandle* out, HcError* err)`
/// (plan §4.2). Entire body wrapped in `catch_unwind` (plan §8 layer 7) — no panic from XML
/// parsing, grammar compilation, or lint checking crosses this boundary.
///
/// # Safety
/// `xml_utf8` must point to `len` readable bytes (or `len == 0`, in which case it may be null);
/// `out` and `err` must each be a valid pointer to storage for their respective out-types, valid
/// for the duration of the call. On success, `*out` receives a handle that must eventually be
/// passed to [`hc_grammar_free`] exactly once.
#[no_mangle]
pub unsafe extern "C" fn hc_grammar_load(
    xml_utf8: *const u8,
    len: usize,
    out: *mut HcGrammarHandle,
    err: *mut HcError,
) -> i32 {
    let result = std::panic::catch_unwind(|| -> Result<(Grammar, String), (i32, String)> {
        if out.is_null() {
            return Err((HC_ERR_NULL_ARG, "hc_grammar_load: out is null".to_string()));
        }
        if len > 0 && xml_utf8.is_null() {
            return Err((
                HC_ERR_NULL_ARG,
                "hc_grammar_load: xml_utf8 is null but len > 0".to_string(),
            ));
        }
        // SAFETY: `xml_utf8`/`len` validity is this function's documented precondition; the
        // null+zero-length case is handled above without dereferencing anything.
        let bytes: &[u8] = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(xml_utf8, len) }
        };
        let xml = std::str::from_utf8(bytes).map_err(|e| {
            (
                HC_ERR_UTF8,
                format!("hc_grammar_load: invalid UTF-8 in grammar xml: {e}"),
            )
        })?;
        let grammar = pg_grammar::load(xml)
            .map_err(|e| (HC_ERR_GRAMMAR_LOAD, format!("hc_grammar_load: {e}")))?;
        Ok((grammar, xml.to_string()))
    });

    match result {
        Ok(Ok((grammar, xml))) => {
            let handle = GrammarHandle::new(grammar, &xml);
            // SAFETY: `out` non-null already checked above.
            unsafe {
                *out = Box::into_raw(handle) as HcGrammarHandle;
            }
            clear_error(err);
            HC_OK
        }
        Ok(Err((code, msg))) => {
            set_error(err, code, &msg);
            code
        }
        Err(payload) => {
            let msg = crate::error::panic_message(payload);
            let full = format!("hc_grammar_load: caught panic: {msg}");
            set_error(err, crate::error::HC_ERR_PANIC, &full);
            crate::error::HC_ERR_PANIC
        }
    }
}

/// `hc_grammar_free(HcGrammarHandle)` (plan §4.2). Reclaims the handle built by
/// [`hc_grammar_load`]. A null handle is a no-op. Wrapped in `catch_unwind` per plan §8 layer 7
/// even though this path is not expected to panic — every entry point gets the same treatment,
/// not just the ones judged risky, since that judgment is exactly what a regression would
/// invalidate.
///
/// # Safety
/// `handle` must be either null or a value previously returned via `*out` by a successful
/// `hc_grammar_load` call, not yet freed, and not in concurrent use by any in-flight
/// `hc_parse_word`/`hc_parse_batch` call (see module docs).
#[no_mangle]
pub unsafe extern "C" fn hc_grammar_free(handle: HcGrammarHandle) {
    let result = std::panic::catch_unwind(|| {
        if handle.is_null() {
            return;
        }
        // SAFETY: per this function's documented precondition, `handle` is a still-valid,
        // not-yet-freed value produced by `Box::into_raw` in `hc_grammar_load`.
        unsafe {
            drop(Box::from_raw(handle as *mut GrammarHandle));
        }
    });
    if let Err(payload) = result {
        // No error channel on this signature (plan §4.2: `void` return) — the panic is still
        // caught (never crosses the boundary), just not reportable beyond a diagnostic.
        eprintln!(
            "hc_grammar_free: caught panic: {}",
            crate::error::panic_message(payload)
        );
    }
}

/// Borrow the `GrammarHandle` behind an opaque `HcGrammarHandle`, or `None` if null.
///
/// # Safety
/// `handle` must be either null or a value returned by a successful `hc_grammar_load` and not
/// yet freed via `hc_grammar_free`.
pub(crate) unsafe fn borrow<'a>(handle: HcGrammarHandle) -> Option<&'a GrammarHandle> {
    if handle.is_null() {
        None
    } else {
        // SAFETY: precondition documented above.
        Some(unsafe { &*(handle as *const GrammarHandle) })
    }
}

#[cfg(test)]
mod runtime_backend_tests {
    use super::*;
    use crate::{hc_analyze_word_json, hc_buf_free, hc_lexicon_add_json, HcResultBuf, HC_OK};

    const XML: &str = r#"<HermitCrabInput><Language><Name>BackendTest</Name><PartsOfSpeech><PartOfSpeech id="p"><Name>N</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions><SegmentDefinition id="a"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries><LexicalEntry id="official-a" partOfSpeech="p"><Allomorphs><Allomorph id="aa"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs></LexicalEntry></LexicalEntries></Stratum></Strata></Language></HermitCrabInput>"#;

    #[test]
    fn foma_initialization_failure_explicitly_falls_back_to_official_morpher_analysis() {
        let grammar = pg_grammar::load(XML).unwrap();
        let handle = GrammarHandle::new_with_foma_result(grammar, XML, Err("forced".into()));
        let result = handle.analyze_word("a");
        assert!(result
            .structured
            .iter()
            .any(|analysis| matches!(analysis.provenance, pg_parse::AnalysisProvenance::Grammar)));
        assert_eq!(handle.backend_kind(), "morpherFallback");
        assert_eq!(
            handle.runtime.analysis_policy(),
            pg_lexicon::AnalysisPolicy::native_abi_v1()
        );
    }

    #[test]
    fn analyzer_panic_is_enveloped_and_the_same_handle_remains_usable() {
        let grammar = pg_grammar::load(XML).unwrap();
        let handle = GrammarHandle::new(grammar, XML);
        handle.force_next_foma_panic();
        let raw = Box::into_raw(handle).cast();

        let request = br#"{"word":"a"}"#;
        let mut out = HcResultBuf::EMPTY;
        assert_eq!(
            unsafe { hc_analyze_word_json(raw, request.as_ptr(), request.len(), &mut out) },
            HC_OK
        );
        let first: serde_json::Value =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) })
                .unwrap();
        assert_eq!(first["error"]["code"], "panic");
        unsafe { hc_buf_free(&mut out) };

        assert_eq!(
            unsafe { hc_analyze_word_json(raw, request.as_ptr(), request.len(), &mut out) },
            HC_OK
        );
        let second: serde_json::Value =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) })
                .unwrap();
        assert_eq!(second["ok"], true);
        assert_eq!(
            second["value"]["structured"][0]["provenance"]["kind"],
            "grammar"
        );
        unsafe {
            hc_buf_free(&mut out);
            crate::hc_grammar_free(raw)
        };
    }

    #[test]
    fn injected_analyzer_panic_is_scoped_to_one_handle() {
        let first = GrammarHandle::new(pg_grammar::load(XML).unwrap(), XML);
        let second = GrammarHandle::new(pg_grammar::load(XML).unwrap(), XML);
        first.force_next_foma_panic();
        assert!(!second.analyze_word("a").structured.is_empty());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| first.analyze_word("a")))
                .is_err()
        );
        assert!(!first.analyze_word("a").structured.is_empty());
    }

    #[test]
    fn mutation_panic_is_structured_and_same_handle_remains_mutable_and_analyzable() {
        let handle = GrammarHandle::new(pg_grammar::load(XML).unwrap(), XML);
        let signature = handle.runtime.catalog().signatures()[0].id.as_str();
        let request = serde_json::json!({"stem":"a", "gloss":"", "signatures":[signature]});
        let bytes = serde_json::to_vec(&request).unwrap();
        handle.runtime.force_next_mutation_panic_for_test();
        let raw = Box::into_raw(handle).cast();
        let mut out = HcResultBuf::EMPTY;
        assert_eq!(
            unsafe { hc_lexicon_add_json(raw, bytes.as_ptr(), bytes.len(), &mut out) },
            HC_OK
        );
        let first: serde_json::Value =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) })
                .unwrap();
        assert_eq!(first["error"]["code"], "panic");
        unsafe { hc_buf_free(&mut out) };

        assert_eq!(
            unsafe { hc_lexicon_add_json(raw, bytes.as_ptr(), bytes.len(), &mut out) },
            HC_OK
        );
        let second: serde_json::Value =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) })
                .unwrap();
        assert_eq!(second["ok"], true);
        unsafe { hc_buf_free(&mut out) };

        let analyze = br#"{"word":"a"}"#;
        assert_eq!(
            unsafe { hc_analyze_word_json(raw, analyze.as_ptr(), analyze.len(), &mut out) },
            HC_OK
        );
        let third: serde_json::Value =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(out.data, out.len) })
                .unwrap();
        assert_eq!(third["ok"], true);
        unsafe {
            hc_buf_free(&mut out);
            crate::hc_grammar_free(raw)
        };
    }

    #[test]
    fn batch_confirmation_uses_requested_parallelism_outside_backend_lock() {
        let handle = GrammarHandle::new(pg_grammar::load(XML).unwrap(), XML);
        let words = vec!["a".to_string(); 8];
        pg_foma::composite::test_confirmation_concurrency::arm();
        let sequential = handle.analyze_words(&words, 1).unwrap();
        assert_eq!(
            pg_foma::composite::test_confirmation_concurrency::max_active(),
            1
        );
        pg_foma::composite::test_confirmation_concurrency::arm();
        let pools_before = handle.pool_build_count_for_test();
        let outcomes = handle.analyze_words(&words, 2).unwrap();
        assert_eq!(outcomes.len(), words.len());
        assert_eq!(
            pg_foma::composite::test_confirmation_concurrency::max_active(),
            2
        );
        assert_eq!(handle.pool_build_count_for_test() - pools_before, 1);
        assert!(outcomes
            .iter()
            .all(|(outcome, _)| !outcome.structured.is_empty()));
        for ((expected, _), (actual, _)) in sequential.iter().zip(&outcomes) {
            assert_eq!(expected.analyses, actual.analyses);
            assert_eq!(expected.structured, actual.structured);
            assert_eq!(expected.guessed, actual.guessed);
            assert_eq!(expected.capped, actual.capped);
        }
    }

    #[test]
    fn batch_pool_build_failure_maps_to_invalid_argument_and_handle_is_reusable() {
        let handle = GrammarHandle::new(pg_grammar::load(XML).unwrap(), XML);
        handle.force_next_pool_build_failure_for_test();
        let raw = Box::into_raw(handle).cast();
        let mut out = HcResultBuf::EMPTY;
        assert_eq!(
            unsafe { crate::hc_parse_batch(raw, std::ptr::null(), 0, 2, &mut out) },
            crate::HC_ERR_INVALID_ARG
        );
        assert!(out.data.is_null());
        assert_eq!(
            unsafe { crate::hc_parse_word(raw, b"a".as_ptr(), 1, &mut out) },
            HC_OK
        );
        unsafe {
            hc_buf_free(&mut out);
            crate::hc_grammar_free(raw)
        };
    }
}
