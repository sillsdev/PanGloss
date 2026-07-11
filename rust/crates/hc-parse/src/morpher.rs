//! The end-to-end Morpher pipeline (plan §2.1, §5): segment → analyze (unapply) → lexical lookup →
//! synthesize (confirm) → validity/surface filter → dedup → signature.
//!
//! Faithful port of `SIL.Machine.Morphology.HermitCrab.Morpher.ParseWord` (Morpher.cs:112-164) +
//! `Synthesize` (Morpher.cs:283-301, the `SINGLE_THREADED` branch) + `LexicalLookup`
//! (Morpher.cs:349-371) + `IsWordValid`/`IsMatch` (Morpher.cs:555-597), with the strata chaining of
//! `AnalysisLanguageRule` (surface→deepest, cs:22-49) and `Language.CompileSynthesisRule`'s
//! `PipelineRuleCascade` over strata (deepest→surface, Language.cs:145-151 + PipelineRuleCascade.cs).
//!
//! ## `MergeEquivalentAnalyses` = true (Tier-2 #14)
//! C# runs with `MergeEquivalentAnalyses = true` (ctor default, Morpher.cs:110): within each
//! stratum's analysis, shape-collided candidates are folded into one canonical word's `Alternatives`
//! (AnalysisStratumRule.cs:161-171) so only one word per shape flows into the deeper strata, and the
//! stashed alternatives are **re-expanded at synthesis** via `ExpandAlternatives` (Morpher.cs:478).
//! Both a correctness matter on the 3-stratum grammars (the merge keys on `Shape` alone and replays
//! deeper deltas onto the alternatives, so it is not generally identical to keeping every
//! distinct-history candidate) and the second Sena perf lever (fewer words descend). This is modelled
//! faithfully: `merge_equivalent = true` here folds alternatives in [`hc_rules::stratum`], the seed /
//! lexical-lookup clones drop alternatives ([`Word::clone_without_alternatives`]), and this module
//! expands them (via [`Word::expand_alternatives`]) between lexical lookup and synthesis, exactly as
//! `SynthesizeAnalysis` does (Morpher.cs:476-486).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use rustc_hash::FxHashMap as HashMap;
use hc_grammar::model::{AllomorphId, AllomorphOwner, Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId, StratumId};
use hc_memo::AnalysisScope;
use hc_rules::cache::RuleCache;
use hc_rules::shape_feat::segment_with_features;
use hc_rules::stratum::{analyze_stratum_scoped_filtered, AnalyzerConfig, NonHeadRootFilter};
use hc_rules::trace::{FailureReason, NoopSink, TraceHandle, TraceSink};
use hc_rules::word::{MorphRecord, Word, WordKey};
use hc_featstruct::{FeatId, FeatureStruct, FeatureValue};

use crate::guess;
use crate::root_trie::{collect_lexical_patterns, RootAllomorphIndex};
use crate::{result_signature, surface, WordAnalysis};

/// The compiled parser for one grammar: the immutable grammar plus its per-stratum root-allomorph
/// tries (C# `Morpher`, built once, parses many words).
pub struct Morpher<'g> {
    g: &'g Grammar,
    root_index: RootAllomorphIndex,
    /// P11 §4.3: every `IsPattern` root allomorph, flat across all strata, in document order
    /// (C#'s single `_lexicalPatterns` list, `Morpher.cs:74-85`) — the exact counterpart of the
    /// exclusion `RootAllomorphTrie::build` now applies. Inert until the guess subsystem
    /// (P11 chunks 3-5) reads it.
    lexical_patterns: Vec<(AllomorphId, LexEntryId)>,
    /// Global per-word step budget threaded through the cascades (analysis is the hot path).
    /// `usize::MAX` = uncapped (the C# behavior). With the memo on it should rarely fire.
    cap: usize,
    /// The #451 order-invariant analysis memo (M6). Default `true`; `false` reproduces the
    /// unmemoized engine — the fair-baseline knob (`--memo=off`, plan §6.3). This is the
    /// "AnalyzerConfig flag" the brief calls for, placed on `Morpher` instead of `AnalyzerConfig`
    /// because the latter's fields are set by a full struct literal in a frozen test
    /// (`stratum_gate.rs`), which an added field would break.
    memo: bool,
    /// `--word-timeout-ms`: an optional wall-clock deadline armed on the [`hc_rules::stratum::StepBudget`]
    /// alongside `cap`, independent of it — two orthogonal bounds on one `parse_word` call,
    /// whichever fires first wins (`docs/budget-model.md`'s addendum). `None` (the default) never
    /// reads the clock at all. Exists because per-step cost is not uniform — some pathological
    /// words legitimately spend far longer per step than others (heavier narrowing/expansion
    /// analysis), so a step-count cap alone cannot bound wall-clock time per word.
    word_timeout: Option<Duration>,
    /// The compile-once FST pattern cache (plan §13.2 step 5): every phonological/morphological
    /// matcher this grammar's rules need, compiled exactly once here (mirroring C#'s `Morpher`
    /// construction compile pass) rather than per-application inside the hot analysis/synthesis
    /// loops. Built once per `Morpher`, then shared read-only across every `--threads=N` worker
    /// (`hc_parse_batch` holds `&Morpher`, never a mutable one, once construction finishes). See
    /// `hc_rules::cache`'s module doc for the full rationale and per-site accounting.
    cache: RuleCache,
}

/// The outcome of parsing one word.
pub struct ParseOutcome {
    /// The `(morpheme-id-join, surface)` pairs, one per surviving analysis, ready for
    /// [`result_signature`]. Empty = no analyses (signature `-`).
    pub analyses: Vec<(String, String)>,
    /// The numeric-id mirror of `analyses`, same length, **same index correspondence**
    /// (`structured[i]` describes the same analysis as `analyses[i]`) — built from the *same*
    /// [`Morpher::allomorphs_in_morph_order`] traversal as `analyses[i].0`'s string join, so the
    /// two views of "the morpheme sequence" cannot drift apart (plan §4.2, M8: `hc-ffi`'s wire
    /// format needs numeric ids, not the display strings `result_signature` prints). Consumed by
    /// `hc-ffi`, which re-sorts both views together into canonical order before encoding.
    pub structured: Vec<WordAnalysis>,
    /// Whether the analysis step budget fired on any stratum (partial results possible).
    pub capped: bool,
    /// The surface word did not segment (C# `InvalidShapeException` → batch status `SKIPPED`).
    pub invalid_shape: bool,
    /// Diagnostic only (not part of any C# contract): the shared [`hc_rules::stratum::StepBudget`]'s
    /// raw tick count for this `parse_word` call. Lets callers (currently `hc-cli`'s `--step-stats`)
    /// report how many (un)application attempts a word actually consumed, independent of whether the
    /// cap was hit — used to compare against C#'s `--rule-stats` attempt counts on specific words.
    pub steps: usize,
    /// Whether `--word-timeout-ms`'s wall-clock deadline fired for this word (`docs/budget-model.md`'s
    /// addendum). Independent of `capped` — a word can time out with steps to spare, or hit the step
    /// cap well inside its deadline. `false` whenever no `--word-timeout-ms` was configured (the
    /// default) or the deadline never elapsed. Always `false` for the `invalid_shape` early return
    /// (segmentation happens before the budget is even constructed).
    pub timed_out: bool,
    /// P11 §4.1: true iff these analyses came from the guess branch (`ParseOptions.guess_root =
    /// true`, on a total normal-lexicon miss — C#'s "the caller passed `guessRoot` and got
    /// results with `RootAllomorph.Guessed`"). Per-analysis granularity is unnecessary: the guess
    /// branch is all-or-nothing (`syntheses.Count == 0` gate, §1.2), so one flag describes every
    /// returned analysis; each `structured[i].guessed` mirrors this same value. Always `false` on
    /// the `parse_word` default path (guess off) and on the `invalid_shape` early return.
    pub guessed: bool,
}

impl ParseOutcome {
    /// The batch signature (`BatchCommand.BuildSignature`): sorted, `;`-joined, `-` when empty.
    pub fn signature(&self) -> String {
        result_signature(&self.analyses)
    }
}

/// Per-call parse knobs (P11 §4.1) — the Rust mirror of C#'s per-call `guessRoot` parameter to
/// `Morpher.ParseWord`/`AnalyzeWord`, rather than a `Morpher`-construction-time flag (FieldWorks
/// toggles this per word). `#[non_exhaustive]`: future per-call knobs land here instead of more
/// `parse_word_*` method variants.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseOptions {
    /// C#'s `guessRoot`: when true AND the normal (real-lexicon) synthesis pass returns zero
    /// results, retry via the lexical-pattern guesser (§1.2's pseudocode). `false` (the `Default`)
    /// reproduces `parse_word`'s existing behavior byte-for-byte.
    pub guess_root: bool,
}

impl ParseOptions {
    /// Builder-style setter for [`Self::guess_root`] — `#[non_exhaustive]` blocks callers outside
    /// this crate from using struct-literal syntax (even with `..Default::default()`), so this is
    /// the public construction path.
    pub fn with_guess_root(mut self, guess_root: bool) -> Self {
        self.guess_root = guess_root;
        self
    }
}

impl<'g> Morpher<'g> {
    /// Build the parser: one root-allomorph trie per stratum (C# `Morpher` ctor, Morpher.cs:35-48).
    pub fn new(g: &'g Grammar, cap: usize) -> Self {
        Morpher {
            g,
            root_index: RootAllomorphIndex::build(g),
            lexical_patterns: collect_lexical_patterns(g),
            cap,
            memo: true,
            word_timeout: None,
            cache: RuleCache::build(g),
        }
    }

    /// Toggle the #451 analysis memo (default on). `false` = the unmemoized baseline (`--memo=off`).
    pub fn with_memo(mut self, memo: bool) -> Self {
        self.memo = memo;
        self
    }

    /// Arm (or leave unarmed) `--word-timeout-ms`'s wall-clock deadline. `None` (the default from
    /// `new`) is a complete no-op — `parse_word` behaves byte-identically to before this existed.
    pub fn with_word_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.word_timeout = timeout;
        self
    }

    /// P11 §4.3 introspection: the lexical-pattern root allomorphs partitioned out of the trie
    /// (`Morpher.cs:74-85`'s `_lexicalPatterns`), flat across all strata, document order. Inert
    /// until the guess subsystem (chunks 3-5) consumes it for real; exposed now so the chunk-2
    /// gate test can pin the partition directly.
    #[inline]
    pub fn lexical_patterns(&self) -> &[(AllomorphId, LexEntryId)] {
        &self.lexical_patterns
    }

    /// Parse one surface word (C# `Morpher.ParseWord`, guessRoot = false). A thin wrapper over
    /// [`Self::parse_word_opts`] — byte-identical to the pre-P11 behavior (§4.1).
    pub fn parse_word(&self, word: &str) -> ParseOutcome {
        self.parse_word_opts(word, &ParseOptions::default())
    }

    /// P11 §4.1: `Morpher.ParseWord`'s full per-call surface, including the `guessRoot` parameter
    /// C#'s `Morpher.ParseWord(string, out bool, bool)`/`AnalyzeWord` expose. See §1.2's
    /// pseudocode for the guess branch this adds after step 4.
    ///
    /// P12 chunk 2: a thin wrapper over [`Self::parse_word_core`] with a [`NoopSink`] — byte-
    /// identical to the pre-P12 behavior. See [`Self::parse_word_traced`] for the tracing entry
    /// point.
    pub fn parse_word_opts(&self, word: &str, opts: &ParseOptions) -> ParseOutcome {
        let sink = NoopSink;
        self.parse_word_core(word, opts, &sink)
    }

    /// P12 chunk 2 (design doc §4.3/§5): the tracing entry point — `hc-rs parse --trace` (chunk 7)
    /// is the intended caller. Deliberately the smallest possible end-to-end slice: mints the root
    /// `WordAnalysis` node (C# `AnalyzeWord`, called once at `ParseWord` entry) and wires
    /// [`Self::is_word_valid_traced`]'s two morpher-level `Failed` sites
    /// (`PartialParse`/`ObligatorySyntacticFeatures`) plus [`Self::is_match_traced`]'s
    /// `Successful`/`Failed(SurfaceFormMismatch)`. Does **not** yet reach into `hc_rules::validity`
    /// (chunk 3) or the analysis/synthesis rule cascades (chunks 4-6) — a candidate rejected by
    /// `allomorphs_valid_cached` produces no trace event yet (a known, temporary gap the later
    /// chunks close; see this method's body).
    pub fn parse_word_traced(&self, word: &str, opts: &ParseOptions, trace: &dyn TraceSink) -> ParseOutcome {
        self.parse_word_core(word, opts, trace)
    }

    /// The shared implementation behind [`Self::parse_word_opts`] (traced with a no-op [`NoopSink`])
    /// and [`Self::parse_word_traced`] (traced with a real sink) — one body, parameterized by
    /// `trace`, so the two paths cannot drift. Every trace call is guarded by
    /// `trace.is_tracing()` first (§4.1's zero-cost-when-off requirement); `NoopSink`'s own methods
    /// are `unreachable!()` and are never reached because of that guard.
    fn parse_word_core(&self, word: &str, opts: &ParseOptions, trace: &dyn TraceSink) -> ParseOutcome {
        let g = self.g;
        let n = g.strata.len();
        if n == 0 {
            return ParseOutcome {
                analyses: Vec::new(),
                structured: Vec::new(),
                capped: false,
                invalid_shape: false,
                steps: 0,
                timed_out: false,
                guessed: false,
            };
        }
        let surface_stratum = StratumId((n - 1) as u8);
        let surface_table = &g.char_tables[g.strata[surface_stratum.0 as usize].table.0 as usize];

        // 1. Segment the surface word against the surface stratum's table (Morpher.cs:115).
        let shape = match segment_with_features(g, surface_table, word) {
            Ok(s) => s,
            Err(_) => {
                return ParseOutcome {
                    analyses: Vec::new(),
                    structured: Vec::new(),
                    capped: false,
                    invalid_shape: true,
                    steps: 0,
                    timed_out: false,
                    guessed: false,
                }
            }
        };
        let mut input = Word::new(shape, surface_stratum);

        // P12 chunk 2: mint the root `WordAnalysis` node (C# `AnalyzeWord`, called once at
        // `ParseWord` entry — Morpher.cs). `input.trace` carries the root handle forward through
        // every later clone (chunk 1's derived-`Clone` threading), so every candidate reaching
        // `is_word_valid_traced`/`is_match_traced` below finds its own ancestor via `w.trace`
        // without this function touching `hc_rules` internals.
        let root = if trace.is_tracing() {
            let h = trace.analyze_word(&input);
            input.trace = Some(h);
            h
        } else {
            TraceHandle::DUMMY
        };

        // 2. Analysis: `AnalysisLanguageRule.Apply` — strata reversed (surface→deepest), the union of
        //    every stratum's unapplication outputs accumulated into `results` (AnalysisLanguageRule.cs).
        //
        // P12 chunk 5: tracing forces `merge_equivalent = false`, matching C#'s own
        // `_morpher.MergeEquivalentAnalyses && !_morpher.TraceManager.IsTracing` guard
        // (`AnalysisStratumRule.cs:152`, comment there: "Don't merge if tracing because it messes up
        // the tracing") -- a trace built while merging is active would silently omit the
        // shape-equivalent alternatives the merge folded away, showing a narrower rule-attempt set
        // than the engine actually explored with tracing off. This is a real, if rare,
        // control-flow-changing interaction (§2/§5 chunk 5 of the design doc), not just an
        // observability toggle: it must fire even though `hc-rules`' own `merge_equivalent` knob has
        // no other connection to tracing today.
        let cfg = AnalyzerConfig {
            merge_equivalent: !trace.is_tracing(), // Tier-2 #14 — see module docs
            max_unapplications: 0,
            max_stem_count: 2, // C# `Morpher.MaxStemCount` default (Morpher.cs:108)
        };
        // ONE step budget for this whole `parse_word` call (see `hc_rules::stratum::StepBudget`'s
        // doc / `docs/budget-model.md`): shared, by reference, across every stratum × every
        // candidate word's `analyze_stratum_scoped_filtered` call below. This is the fix for the
        // per-`StratumAnalyzer`-instance amplifier — previously each of those calls got its own
        // fresh `cap`-sized counter, so a multi-stratum, multi-candidate word could explore
        // `cap × (#such calls)` steps for a single `--step-cap` value.
        let budget = hc_rules::stratum::StepBudget::new(self.cap).with_timeout(self.word_timeout);
        // One memo scope per `parse_word` (C# `AnalysisScope`, one per `Morpher.ParseWord`): shared
        // across all strata + input words of this parse, never across parses. `None` when memo is
        // off, OR (P12 chunk 5) whenever tracing is on: `Morpher.cs:239-247` only constructs
        // `input.AnalysisScope` `if (!_traceManager.IsTracing)`, leaving it unset entirely while
        // tracing (comment there: "traces must stay byte-identical to the unmemoized engine" --
        // parse-optimization.md Phase 2 ground rules). This is the M6/Gate-B analog of the
        // `merge_equivalent` bypass just above: a trace built against a memoized replay would show a
        // narrower rule-attempt set than the real unmemoized engine explores, in exactly the cases a
        // human trusts the trace to explain.
        let scope_cell = (self.memo && !trace.is_tracing()).then(|| RefCell::new(AnalysisScope::new()));
        let scope = scope_cell.as_ref();
        // M5c: the non-head lexicon filter (`AnalysisCompoundingRule.Apply`'s root-allomorph-search
        // gate — see `hc_rules::stratum::NonHeadRootFilter`'s doc). `hc-parse` owns the
        // `RootAllomorphIndex`, so the closure body lives here; `hc-rules` only receives the search
        // results, never the index type itself (crate-boundary: `hc-rules` cannot depend on
        // `hc-parse`).
        let filter: NonHeadRootFilter = &|st: StratumId, shape: &hc_shape::Shape| self.root_index.search(g, st, shape);
        let mut input_set: HashMap<WordKey, Word> = HashMap::default();
        input_set.insert(input.dedup_key(), input);
        let mut results: HashMap<WordKey, Word> = HashMap::default();
        for s in (0..n).rev() {
            let mut output_set: HashMap<WordKey, Word> = HashMap::default();
            for w in input_set.values() {
                let res = analyze_stratum_scoped_filtered(
                    g,
                    StratumId(s as u8),
                    w.clone(),
                    &cfg,
                    scope,
                    Some(filter),
                    &self.cache,
                    &budget,
                );
                for o in res.words {
                    let k = o.dedup_key();
                    results.entry(k.clone()).or_insert_with(|| o.clone());
                    output_set.entry(k).or_insert(o);
                }
            }
            input_set = output_set;
            if input_set.is_empty() {
                break;
            }
        }

        // 3. + 4. Synthesis: per analysis candidate, lexical lookup → synthesis pipeline → filter.
        //    `Morpher.Synthesize` (SINGLE_THREADED branch, Morpher.cs:283-301).
        let mut matches: HashMap<WordKey, Word> = HashMap::default();
        for aw in results.values() {
            for syn_word in self.lexical_lookup(aw) {
                // `synthesisWord.ExpandAlternatives()` (Morpher.cs:478): recover the shape-equivalent
                // candidates `MergeEquivalentAnalyses` folded away, each with the deeper strata's
                // rules replayed, then synthesize every one of them.
                for alt in syn_word.expand_alternatives() {
                    for vw in self.synthesis_pipeline_traced(alt, trace, root) {
                        if self.is_word_valid_traced(&vw, trace, root) && self.is_match_traced(&vw, word, trace, root) {
                            matches.entry(vw.dedup_key()).or_insert(vw);
                        }
                    }
                }
            }
        }

        // P11 §1.2's guess branch: only on a TOTAL miss of the normal (real-lexicon) path, and
        // only when the caller opted in. `results.values()` is `origAnalyses` (the design doc's
        // §1.2 notes this is the exact same analysis-candidate set, no re-analysis/copy).
        let (ordered_matches, guessed): (Vec<Word>, bool) = if opts.guess_root && matches.is_empty() {
            let mut guess_matches: Vec<Word> = Vec::new();
            for aw in results.values() {
                // `LexicalGuess(analysisWord).Distinct()` — the `.Distinct()` is a documented C#
                // no-op (§1.2): every yielded `Word` is a fresh clone with no `Equals` override,
                // so consuming `guess::lexical_guess`'s `Vec<Word>` directly is faithful.
                for synthesis_word in guess::lexical_guess(g, &self.lexical_patterns, aw) {
                    for alt in synthesis_word.expand_alternatives() {
                        for vw in self.synthesis_pipeline_traced(alt, trace, root) {
                            if self.is_word_valid_traced(&vw, trace, root) && self.is_match_traced(&vw, word, trace, root) {
                                // No dedup here (§1.2: "unlike the normal path's HashSet/Distinct,
                                // duplicates ... survive into the output") — a plain `Vec`, not a
                                // `WordKey`-deduped `HashMap` like `matches` above.
                                guess_matches.push(vw);
                            }
                        }
                    }
                }
            }
            // Descending by morph count (`Morphs.Count()`), C#'s `List.Sort` — Rust's `sort_by` is
            // stable, a deliberate strengthening over C#'s unstable sort (design doc §7 open
            // question #4): tie order becomes deterministic instead of unspecified.
            guess_matches.sort_by_key(|w| std::cmp::Reverse(w.morphs.len()));
            (guess_matches, true)
        } else {
            (matches.into_values().collect(), false)
        };

        // 5. Build (morpheme-join, surface) pairs for each surviving word, plus the numeric-id
        //    mirror `structured` (same index correspondence — see `ParseOutcome` docs). Iterates
        //    `ordered_matches` in order so the guess branch's sort is reflected in `structured`'s
        //    enumeration order (§1.2: "this only matters for `AnalyzeWord` enumeration order").
        let mut analyses: Vec<(String, String)> = Vec::with_capacity(ordered_matches.len());
        let mut structured: Vec<WordAnalysis> = Vec::with_capacity(ordered_matches.len());
        for w in &ordered_matches {
            analyses.push((self.morpheme_join(w), self.surface_of(w)));
            structured.push(self.structured_analysis(w, guessed));
        }

        ParseOutcome {
            analyses,
            structured,
            capped: budget.capped(),
            invalid_shape: false,
            steps: budget.steps(),
            timed_out: budget.timed_out(),
            guessed,
        }
    }

    /// C# `Morpher.LexicalLookup` (Morpher.cs:349-371): search the candidate's stratum trie for
    /// matching root allomorphs, take the **distinct owning entries**, and for **each allomorph of
    /// each entry** clone the candidate and set the root allomorph on it (which replaces the shape
    /// with the root's underlying form, resets the syntactic FS / MPR / partial flag, and records the
    /// root morph — `Word.SetRootAllomorph`, Word.cs:137-147). The analysis history (`mrule_apps` /
    /// `mrule_app_index`) is carried over unchanged — it is what synthesis will confirm.
    fn lexical_lookup(&self, aw: &Word) -> Vec<Word> {
        let g = self.g;
        let matched = self.root_index.search(g, aw.stratum, &aw.shape);
        // Distinct entries in first-seen order (C# `.Distinct()` on the entry sequence).
        let mut entries: Vec<LexEntryId> = Vec::new();
        for (_, le) in &matched {
            if !entries.contains(le) {
                entries.push(*le);
            }
        }
        let mut out = Vec::new();
        for le in entries {
            let entry = &g.entries[le.0 as usize];
            for allo in &entry.allomorphs {
                // C# `Word newWord = input.Clone()` (Morpher.cs:509): the clone drops alternatives
                // and records `input` (= `aw`) as its `Source`, the boundary `expand_alternatives`
                // walks from at synthesis (Word.cs:86-87 + the synthesis `ExpandAlternatives` call).
                let mut nw = aw.clone_without_alternatives();
                nw.source = Some(Rc::new(aw.clone()));
                self.set_root_allomorph(&mut nw, le, allo.id, &allo.shape.text);
                out.push(nw);
            }
        }
        out
    }

    /// `Word.SetRootAllomorph` (Word.cs:137-147). The root allomorph's underlying shape is
    /// re-segmented *with phonological features* from its stored surface text (the C# `Segments`
    /// shape carries a per-node FeatureStruct; the loader-stored `RootAllomorphDef.shape` is
    /// feature-less), so synthesis and surface rendering see the same feature-bearing shape C# does.
    fn set_root_allomorph(&self, w: &mut Word, le: LexEntryId, allo: hc_grammar::model::AllomorphId, text: &str) {
        let g = self.g;
        let entry = &g.entries[le.0 as usize];
        let root_stratum = g.morphemes[entry.morpheme.0 as usize].stratum;
        let table = &g.char_tables[g.strata[root_stratum.0 as usize].table.0 as usize];
        let root_shape = segment_with_features(g, table, text)
            .unwrap_or_else(|_| entry.allomorphs.iter().find(|a| a.id == allo).map(|a| a.shape.shape.clone()).unwrap());
        w.shape = root_shape;
        w.stratum = root_stratum;
        w.syn_fs = g.fs_interner.get(entry.syn_fs).clone();
        w.mpr = entry.mpr;
        w.flags.is_partial = entry.partial;
        w.root_allomorph = Some(allo);
        // MarkMorph(shape, rootAllomorph, RootMorphID): the root is the base morph at order 0.
        w.morphs = vec![MorphRecord::new(allo, entry.morpheme, 0)];
    }

    /// The synthesis pipeline (`PipelineRuleCascade` over strata deepest→surface,
    /// Language.cs:145-151 + PipelineRuleCascade.cs:15-33): fold the candidate through every stratum's
    /// `synthesize_stratum` in document order, deduping by `WordKey` between strata. A thin
    /// NoopSink wrapper over [`Self::synthesis_pipeline_traced`] — see that method's doc.
    fn synthesis_pipeline(&self, syn_word: Word) -> Vec<Word> {
        let sink = NoopSink;
        self.synthesis_pipeline_traced(syn_word, &sink, TraceHandle::DUMMY)
    }

    /// P12 chunks 4/5 (the applied-event spine): [`Self::synthesis_pipeline`]'s traced sibling.
    /// Threads `trace`/`parent` into `hc_rules::stratum::synthesize_stratum_traced`, whose
    /// `guided_synth` fires `MorphologicalRuleApplied` on every successful rule confirmation and
    /// reassigns each output word's trace cursor -- the mechanism that makes a traced multi-rule
    /// synthesis render as a followable rule sequence (design doc §6's acceptance bar) rather than
    /// a single `Successful` leaf under the root.
    fn synthesis_pipeline_traced(&self, syn_word: Word, trace: &dyn TraceSink, parent: TraceHandle) -> Vec<Word> {
        let g = self.g;
        let n = g.strata.len();
        let mut cur: HashMap<WordKey, Word> = HashMap::default();
        cur.insert(syn_word.dedup_key(), syn_word);
        for s in 0..n {
            if cur.is_empty() {
                break; // PipelineRuleCascade stops once the working set empties (cs:20).
            }
            let mut next: HashMap<WordKey, Word> = HashMap::default();
            for w in cur.values() {
                let node_parent = w.trace.unwrap_or(parent);
                for o in hc_rules::stratum::synthesize_stratum_traced(
                    g,
                    StratumId(s as u8),
                    w.clone(),
                    self.cap,
                    &self.cache,
                    trace,
                    node_parent,
                ) {
                    next.entry(o.dedup_key()).or_insert(o);
                }
            }
            cur = next;
        }
        cur.into_values().collect()
    }

    /// C# `Morpher.IsWordValid` (Morpher.cs:555-582): the realizational FS unifies with the syntactic
    /// FS (always — realizational is empty in v1), every morphological rule has been re-applied
    /// (`IsAllMorphologicalRulesApplied` = `mrule_app_index == -1`), every obligatory syntactic
    /// feature is present, and (plan §13.1 Tier-1 #5, Morpher.cs:581) every distinct allomorph used
    /// in the word passes its own `Allomorph.IsWordValid` — environments, bound-root, and per-
    /// allomorph required-syntactic-FS gates (`hc_rules::validity::allomorphs_valid`).
    fn is_word_valid(&self, w: &Word) -> bool {
        let sink = NoopSink;
        self.is_word_valid_traced(w, &sink, TraceHandle::DUMMY)
    }

    /// P12 chunk 2/3: [`Self::is_word_valid`]'s traced sibling — the single source of truth both
    /// share (`is_word_valid` calls this with a [`NoopSink`]). Wires C#'s `Failed(PartialParse)`/
    /// `Failed(ObligatorySyntacticFeatures)` (Morpher.cs:723, the two morpher-level clauses ahead of
    /// the allomorph-validity gate — §3.1's direct 1:1 matches), then (chunk 3) delegates the final
    /// gate to [`hc_rules::validity::allomorphs_valid_cached_traced`], which reports exactly which
    /// of its own 11 `FailureReason`s rejected the word. `parent` is `w.trace.unwrap_or(parent)`:
    /// once later chunks set `Word::trace` as the parse progresses (mirroring C#'s `CurrentTrace`
    /// reassignment), this call site does not need to change to pick it up.
    fn is_word_valid_traced(&self, w: &Word, trace: &dyn TraceSink, parent: TraceHandle) -> bool {
        let parent = w.trace.unwrap_or(parent);
        if w.mrule_app_index != -1 {
            // partial parse — not every unapplied rule was confirmed
            if trace.is_tracing() {
                trace.failed(parent, w, FailureReason::PartialParse);
            }
            return false;
        }
        for &f in &w.obligatory {
            if !contains_feature(&w.syn_fs, f) {
                if trace.is_tracing() {
                    trace.failed(parent, w, FailureReason::ObligatorySyntacticFeatures);
                }
                return false;
            }
        }
        hc_rules::validity::allomorphs_valid_cached_traced(self.g, w, &self.cache, trace, parent)
    }

    /// C# `Morpher.IsMatch` (Morpher.cs:584): the synthesized shape's surface matches the input
    /// word. P12 chunk 2: the sole (traced) implementation — `trace.is_tracing()` guards every
    /// trace call, so the [`NoopSink`] path pays only the guard check, matching
    /// [`Self::is_word_valid_traced`]'s pattern. Wires C#'s `Successful`/
    /// `Failed(SurfaceFormMismatch)` (Morpher.cs:740, `IsMatch`'s own gate — §3.1's direct 1:1
    /// match). Only reached for candidates that already passed [`Self::is_word_valid_traced`] (the
    /// `&&` short-circuit at both call sites), matching C#'s `IsWordValid(word) && IsMatch(word,
    /// input)` order exactly.
    fn is_match_traced(&self, w: &Word, word: &str, trace: &dyn TraceSink, parent: TraceHandle) -> bool {
        let parent = w.trace.unwrap_or(parent);
        let g = self.g;
        let n = g.strata.len();
        let surface_table = &g.char_tables[g.strata[n - 1].table.0 as usize];
        let ok = surface::is_match(surface_table, &w.shape, word);
        if trace.is_tracing() {
            if ok {
                trace.successful(parent, w);
            } else {
                trace.failed(parent, w, FailureReason::SurfaceFormMismatch);
            }
        }
        ok
    }

    /// The signature's surface half (`Shape.ToRegexString(Stratum.CharacterDefinitionTable, true)`,
    /// BatchCommand.cs:235). Rendered against the word's own stratum table (= surface stratum for a
    /// fully-synthesized word).
    fn surface_of(&self, w: &Word) -> String {
        let g = self.g;
        let table = &g.char_tables[g.strata[w.stratum.0 as usize].table.0 as usize];
        surface::to_regex_display(table, &w.shape)
    }

    /// `Word.AllomorphsInMorphOrder` (Word.cs:119): distinct **allomorphs** in first-occurrence
    /// morph order. The single shared traversal that both [`Self::morpheme_join`] (the display
    /// string) and [`Self::structured_analysis`] (the FFI's numeric ids) are built from, so the
    /// two representations of "the morpheme sequence" cannot disagree on count, order, or dedup
    /// (flagged as a drift risk during M8 review — kept as one traversal, two projections).
    fn allomorphs_in_morph_order(
        &self,
        w: &Word,
    ) -> Vec<(hc_grammar::model::AllomorphId, hc_grammar::model::MorphemeId)> {
        let mut ms = w.morphs.clone();
        ms.sort_by_key(|m| m.order);
        let mut seen: Vec<hc_grammar::model::AllomorphId> = Vec::new();
        ms.iter()
            .filter(|m| {
                if seen.contains(&m.allomorph) {
                    false
                } else {
                    seen.push(m.allomorph);
                    true
                }
            })
            .map(|m| (m.allomorph, m.morpheme))
            .collect()
    }

    /// The signature's morpheme half: `join("+", AllomorphsInMorphOrder.Select(a => a.Morpheme.Id))`
    /// (BatchCommand.cs:233), mapped to each morpheme's `<MorphemeId>` string (empty when the
    /// morpheme declares none — every Indonesian morpheme). P11 §4.4-2: the sentinel
    /// `MorphemeId::GUESSED` has no `Grammar::morphemes` row — its join text is
    /// `Word::guessed_root`'s rendered `text` (C#'s fabricated `Morpheme.Id = shapeString`, which
    /// is exactly what `BatchCommand` prints for a guessed root).
    fn morpheme_join(&self, w: &Word) -> String {
        let g = self.g;
        self.allomorphs_in_morph_order(w)
            .into_iter()
            .map(|(_, morpheme)| {
                if morpheme == MorphemeId::GUESSED {
                    w.guessed_root.as_ref().map(|gr| gr.text.clone()).unwrap_or_default()
                } else {
                    g.morphemes[morpheme.0 as usize].morph_id.clone().unwrap_or_default()
                }
            })
            .collect::<Vec<String>>()
            .join("+")
    }

    /// The FFI's numeric mirror of one analysis (plan §4.2, M8): the same
    /// [`Self::allomorphs_in_morph_order`] sequence, projected to grammar-tier `MorphemeId`
    /// ordinals (dense indices into `Grammar::morphemes` — **not** the `<MorphemeId>` XML string
    /// `morpheme_join` prints, and not yet mapped to a managed `IMorpheme`; that mapping is the
    /// out-of-scope §4.1 C# facade), plus the root's position in that sequence and the word's POS
    /// symbol index (`Morpher.cs`'s `WordAnalysis`, cs:637). P11 §4.4-2: `morpheme_ids` needs no
    /// sentinel special-casing — `MorphemeId::GUESSED.0 == u32::MAX` passes through as a plain
    /// numeric id, exactly as the design calls for; `root_morpheme_index`'s
    /// `Some(*allo) == w.root_allomorph` comparison likewise works unmodified, since both sides
    /// carry the identical `AllomorphId::GUESSED` sentinel for a guessed word.
    fn structured_analysis(&self, w: &Word, guessed: bool) -> WordAnalysis {
        let seq = self.allomorphs_in_morph_order(w);
        let morpheme_ids: Vec<u32> = seq.iter().map(|(_, m)| m.0).collect();
        let root_morpheme_index = seq
            .iter()
            .position(|(allo, _)| Some(*allo) == w.root_allomorph)
            .map(|i| i as i32)
            .unwrap_or(-1);
        let pos_id = match w.syn_fs.get(self.g.syn_features.pos) {
            Some(FeatureValue::Symbolic(bits)) => bits.first(),
            _ => None,
        };
        WordAnalysis { morpheme_ids, root_morpheme_index, pos_id, guessed }
    }
}

// =================================================================================================
// Generation (W7): `Morpher.GenerateWords` — the synthesis-only counterpart to `parse_word`.
// =================================================================================================

/// One "other morpheme" in [`Morpher::generate_words`]'s direct API — the Rust mirror of C#
/// `IEnumerable<Morpheme>`, where a `Morpheme` is either a `LexEntry` (a compounding non-head, whose
/// owning `CompoundingRule` is deliberately left unspecified — C#'s "unknown compounding rule" null,
/// Word.cs:317-327/`Morpher.cs:261`) or an `IMorphologicalRule`
/// (`AffixProcessRule`/`RealizationalAffixProcessRule`; a bare `CompoundingRule` is never a
/// `Morpheme` in C# and so can never appear here directly — `Morpher.PermuteRules`,
/// Morpher.cs:239-280, type-switches on exactly this LexEntry-vs-rule distinction).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenMorpheme {
    /// A known morphological rule to unapply (`AffixProcessRule`/`RealizationalAffixProcessRule`).
    Rule(MRuleId),
    /// A bare compounding non-head root entry — the owning `CompoundingRule` is discovered by the
    /// synthesis confirmation gate trying every compounding rule at that trail position (the `None`
    /// wildcard `hc_rules::word::Word::mrule_apps` slot; see that field's doc).
    NonHead(LexEntryId),
}

/// One resolved slot of a `PermuteRules` permutation (C# `Tuple<IMorphologicalRule, RootAllomorph>`,
/// Morpher.cs:185): either a known rule, or a *specific* non-head root allomorph (C#'s
/// `foreach (RootAllomorph allo in entry.Allomorphs)`, Morpher.cs:252 — every allomorph of a
/// `NonHead` entry is tried, not just the primary one).
#[derive(Clone, Copy, Debug)]
enum PermItem {
    Rule(MRuleId),
    NonHead(AllomorphId),
}

/// C# `Morpher.PermuteRules` (Morpher.cs:239-280): the cross product of every `NonHead` entry's
/// allomorphs (a `Rule` slot has no choice — branching factor 1), preserving `morphemes`' own input
/// order in every output permutation. Faithful to C#'s recursion in *content* (same combinatorial
/// space) though not in per-permutation construction order, which is unobservable — the caller
/// dedupes the union of every permutation's generated words (C# `results.UnionWith`/
/// `words.Distinct()`), never inspects an individual permutation.
fn permute_rules(g: &Grammar, morphemes: &[GenMorpheme]) -> Vec<Vec<PermItem>> {
    if morphemes.is_empty() {
        return vec![Vec::new()];
    }
    let tails = permute_rules(g, &morphemes[1..]);
    let mut out = Vec::new();
    match morphemes[0] {
        GenMorpheme::Rule(id) => {
            for tail in &tails {
                let mut v = Vec::with_capacity(tail.len() + 1);
                v.push(PermItem::Rule(id));
                v.extend_from_slice(tail);
                out.push(v);
            }
        }
        GenMorpheme::NonHead(le) => {
            for allo in &g.entries[le.0 as usize].allomorphs {
                for tail in &tails {
                    let mut v = Vec::with_capacity(tail.len() + 1);
                    v.push(PermItem::NonHead(allo.id));
                    v.extend_from_slice(tail);
                    out.push(v);
                }
            }
        }
    }
    out
}

/// `mrule is RealizationalAffixProcessRule` (Word.cs:321) — the one rule kind that never occupies an
/// `mrule_apps` trail slot (see that field's doc); consulted by
/// [`Morpher::morphological_rule_unapplied`]'s call in [`Morpher::generate_words`].
fn is_realizational_rule(g: &Grammar, id: MRuleId) -> bool {
    matches!(&g.mrules[id.0 as usize], MorphRuleDef::Realizational(_))
}

/// Which kind of `Morpheme` a grammar-tier [`MorphemeId`] names — `LexEntry` or a `Morpheme`-bearing
/// rule (`AffixProcess`/`Realizational`; `Compounding` rules are never `Morpheme`s in C#, so never
/// resolve here — Morpher.cs:50-52's `_morphemes` construction only gathers entries and
/// `AffixProcessRule`s, and `RealizationalAffixProcessRule` is reachable the same way any
/// `WordAnalysis`-listed morpheme is: via `AllomorphsInMorphOrder`, Word.cs:119).
enum MorphemeOwner {
    Root(LexEntryId),
    Rule(MRuleId),
}

/// Linear scan over `entries` + `mrules` — fine for generation (a rare, caller-initiated call, not
/// the per-word analysis/synthesis hot path).
fn resolve_morpheme(g: &Grammar, id: MorphemeId) -> Option<MorphemeOwner> {
    if let Some(idx) = g.entries.iter().position(|e| e.morpheme == id) {
        return Some(MorphemeOwner::Root(LexEntryId(idx as u32)));
    }
    for (idx, r) in g.mrules.iter().enumerate() {
        let m = match r {
            MorphRuleDef::AffixProcess(d) => Some(d.morpheme),
            MorphRuleDef::Realizational(d) => Some(d.morpheme),
            MorphRuleDef::Compounding(_) => None,
        };
        if m == Some(id) {
            return Some(MorphemeOwner::Rule(MRuleId(idx as u32)));
        }
    }
    None
}

/// [`resolve_morpheme`] projected to [`GenMorpheme`] (only meaningful for a non-root "other"
/// morpheme slot); `None` if `id` cannot be resolved at all (malformed caller input).
fn resolve_other(g: &Grammar, id: MorphemeId) -> Option<GenMorpheme> {
    match resolve_morpheme(g, id)? {
        MorphemeOwner::Root(le) => Some(GenMorpheme::NonHead(le)),
        MorphemeOwner::Rule(r) => Some(GenMorpheme::Rule(r)),
    }
}

/// Every way to merge `left` and `right` into one sequence while preserving each side's own
/// relative order — C# `Morpher.PermuteOtherMorphemes`'s combinatorial space (Morpher.cs:681-711):
/// `C(|left|+|right|, |left|)` interleavings, byte-for-byte the same SET C# produces (verified by
/// hand-deriving C#'s stack-push recursion for `left=[A,B], right=[C]` — see the commit message —
/// and confirming it always emits `B` before `A`, i.e. root-outward, never the reverse).
///
/// **Callers must pass `left` in root-outward order** (the element adjacent to the root first, the
/// outermost prefix last) — the reverse of `left`'s natural left-to-right position order in the
/// word. `right` is passed in ordinary (root-outward-is-also-forward) position order; no reversal
/// needed there. [`Morpher::generate_words_from_analysis`] does this reversal at its call site, not
/// here, so this function's own contract stays a plain "two already-ordered sequences" merge with
/// no morpheme-specific direction knowledge baked in.
fn interleavings(left: &[GenMorpheme], right: &[GenMorpheme]) -> Vec<Vec<GenMorpheme>> {
    fn go(left: &[GenMorpheme], right: &[GenMorpheme], acc: &mut Vec<GenMorpheme>, out: &mut Vec<Vec<GenMorpheme>>) {
        if left.is_empty() && right.is_empty() {
            out.push(acc.clone());
            return;
        }
        if let Some((&head, rest)) = left.split_first() {
            acc.push(head);
            go(rest, right, acc, out);
            acc.pop();
        }
        if let Some((&head, rest)) = right.split_first() {
            acc.push(head);
            go(left, rest, acc, out);
            acc.pop();
        }
    }
    let mut out = Vec::new();
    go(left, right, &mut Vec::new(), &mut out);
    out
}

impl<'g> Morpher<'g> {
    /// C# `Morpher.GenerateWords(LexEntry, IEnumerable<Morpheme>, FeatureStruct)` (Morpher.cs:169-237
    /// — the trace-less overload; this port has no `ITraceManager`, plan rust-conversion.md §7, and
    /// no parallel-cascade mode, same §7). Builds one seed [`Word`] per `(root allomorph,
    /// otherMorphemes permutation)` pair — `rootEntry.Allomorphs × PermuteRules(otherMorphemes)`
    /// (Morpher.cs:185,195-198) — runs each through [`Self::synthesis_pipeline`], and keeps the
    /// [`Self::is_word_valid`] survivors' rendered surface forms, deduplicated (C# `words.Distinct()`,
    /// Morpher.cs:236). Returned in sorted order (a `BTreeSet`, not C#'s unordered `HashSet`) — a
    /// strengthening, not a behavior difference: the returned *set* of words is what C# guarantees.
    ///
    /// Deliberately **not** filtered by [`Self::is_match`]: generation has no input surface string to
    /// match against — that gate is `Morpher.Synthesize`'s (the parse path, Morpher.cs:294/323), never
    /// `Morpher.GenerateWords`'s (Morpher.cs:218 calls only `IsWordValid`).
    ///
    /// Validity gates that DO run — the same [`Self::is_word_valid`] the parse path uses, so blocking
    /// (`hc_rules::morph::check_blocking`/`apply_blocking`, threaded inside `synthesize`/
    /// `synthesize_cached` and hence inside [`Self::synthesis_pipeline`]), stem names and
    /// co-occurrence rules (`hc_rules::validity::allomorphs_valid_cached`), environments, and the
    /// per-allomorph required-syntactic-FS/bound-root gates are all exactly as parse-time (Morpher.cs
    /// itself calls the identical private `IsWordValid`, Morpher.cs:555, for both `Synthesize` and
    /// `GenerateWords`) — see [`Self::is_word_valid`]'s own doc for the C# citations.
    pub fn generate_words(&self, root: LexEntryId, others: &[GenMorpheme], real_fs: FeatureStruct) -> Vec<String> {
        let g = self.g;
        let permutations = permute_rules(g, others);
        let mut words: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let n_allomorphs = g.entries[root.0 as usize].allomorphs.len();
        for allo_idx in 0..n_allomorphs {
            for perm in &permutations {
                let mut seed = self.build_root_seed(root, allo_idx, real_fs.clone());
                for item in perm {
                    match *item {
                        PermItem::Rule(id) => {
                            seed.morphological_rule_unapplied(is_realizational_rule(g, id), Some(id));
                        }
                        PermItem::NonHead(allo_id) => {
                            // C# `synthesisWord.MorphologicalRuleUnapplied(rule.Item1)` with
                            // `rule.Item1 == null` (Morpher.cs:206), then, since `rule.Item2 != null`
                            // (Morpher.cs:207-208), `NonHeadUnapplied(new Word(rule.Item2, new
                            // FeatureStruct()))` — a FRESH empty realizational FS for the non-head,
                            // regardless of the outer `real_fs` this call was given.
                            seed.morphological_rule_unapplied(false, None);
                            let nh = self.build_allomorph_seed(allo_id, FeatureStruct::EMPTY);
                            seed.non_head_unapplied(nh);
                        }
                    }
                }
                for vw in self.synthesis_pipeline(seed) {
                    if self.is_word_valid(&vw) {
                        words.insert(self.generated_surface_of(&vw));
                    }
                }
            }
        }
        words.into_iter().collect()
    }

    /// C# `Morpher.GenerateWords(WordAnalysis)` (Morpher.cs:659-679): recover the root + its
    /// left/right "other morphemes" from a [`WordAnalysis`] (typically `Morpher::parse_word`'s own
    /// `structured` output, round-tripped), try every left/right [`interleavings`] (C#
    /// `PermuteOtherMorphemes`, Morpher.cs:681-711 — the *word-order* recovery `generate_words`'s
    /// direct API does not attempt on its own), and union the direct API's results over all of them
    /// (C# `results.UnionWith`, Morpher.cs:676) with an always-empty realizational FS (C# `new
    /// FeatureStruct()`, Morpher.cs:666 — the direct API is the only way to supply a non-empty one).
    /// Empty input or an unresolvable morpheme id (a malformed/foreign `WordAnalysis`) yields no
    /// words rather than panicking — C#'s `(LexEntry)morphemes[...]` cast would throw here instead;
    /// reporting no results is the safer Rust-idiomatic analog for input this port cannot validate
    /// upfront.
    pub fn generate_words_from_analysis(&self, wa: &WordAnalysis) -> Vec<String> {
        if wa.morpheme_ids.is_empty() {
            return Vec::new();
        }
        let g = self.g;
        if wa.root_morpheme_index < 0 || wa.root_morpheme_index as usize >= wa.morpheme_ids.len() {
            return Vec::new();
        }
        let root_idx = wa.root_morpheme_index as usize;
        let root_entry = match resolve_morpheme(g, MorphemeId(wa.morpheme_ids[root_idx])) {
            Some(MorphemeOwner::Root(le)) => le,
            _ => return Vec::new(),
        };
        let resolve_side = |ids: &[u32]| -> Option<Vec<GenMorpheme>> {
            ids.iter().map(|&id| resolve_other(g, MorphemeId(id))).collect()
        };
        // `left` resolves in the word's left-to-right position order (outermost prefix first);
        // reverse it to root-outward order before interleaving — `interleavings`' documented
        // contract, matching C# `PermuteOtherMorphemes`'s trail convention exactly (Morpher.cs:702-
        // 710: `leftIndex` walks from adjacent-to-root outward, so the element closest to the root
        // is always pushed, and therefore confirmed, first).
        let Some(mut left) = resolve_side(&wa.morpheme_ids[..root_idx]) else {
            return Vec::new();
        };
        left.reverse();
        let Some(right) = resolve_side(&wa.morpheme_ids[root_idx + 1..]) else {
            return Vec::new();
        };

        let mut words: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for others in interleavings(&left, &right) {
            for w in self.generate_words(root_entry, &others, FeatureStruct::EMPTY) {
                words.insert(w);
            }
        }
        words.into_iter().collect()
    }

    /// Build a fresh root-level seed [`Word`] from `le`'s allomorph at `allo_idx` — C#'s
    /// `Word(RootAllomorph, FeatureStruct)` constructor + implicit `SetRootAllomorph`
    /// (Word.cs:36-51,137-147). Unlike [`hc_rules::morph::seed_from_entry`] (which the blocking/
    /// `ChooseInflectionalStem` sites use and which always takes the entry's *primary* allomorph),
    /// this takes an explicit index — `Morpher.GenerateWords` tries **every** allomorph of the root
    /// entry (`rootEntry.Allomorphs.SelectMany(...)`, Morpher.cs:195), not just the first.
    fn build_root_seed(&self, le: LexEntryId, allo_idx: usize, real_fs: FeatureStruct) -> Word {
        let g = self.g;
        let entry = &g.entries[le.0 as usize];
        let allo = &entry.allomorphs[allo_idx];
        let stratum = g.morphemes[entry.morpheme.0 as usize].stratum;
        let table = &g.char_tables[g.strata[stratum.0 as usize].table.0 as usize];
        let shape = segment_with_features(g, table, &allo.shape.text)
            .unwrap_or_else(|_| allo.shape.shape.clone());
        let mut w = Word::new(shape, stratum);
        w.syn_fs = g.fs_interner.get(entry.syn_fs).clone();
        w.mpr = entry.mpr;
        w.flags.is_partial = entry.partial;
        w.root_allomorph = Some(allo.id);
        w.real_fs = real_fs;
        w.morphs = vec![MorphRecord::new(allo.id, entry.morpheme, 0)];
        w
    }

    /// [`Self::build_root_seed`] for a *specific* [`AllomorphId`] already known to be a root
    /// allomorph (a compounding non-head, `PermItem::NonHead` — C#'s `new Word(rule.Item2,
    /// new FeatureStruct())`, Morpher.cs:208, where `rule.Item2` is a `RootAllomorph`, not a
    /// `LexEntry`). Resolves back to its owning entry + index via `Grammar::allomorph_owners`.
    fn build_allomorph_seed(&self, allo_id: AllomorphId, real_fs: FeatureStruct) -> Word {
        let g = self.g;
        let AllomorphOwner::Root(le, idx) = g.allomorph_owners[allo_id.0 as usize] else {
            unreachable!("PermItem::NonHead is only ever built from a root allomorph (permute_rules)")
        };
        self.build_root_seed(le, idx as usize, real_fs)
    }

    /// C# `validWord.Shape.ToString(_lang.SurfaceStratum.CharacterDefinitionTable, false)`
    /// (Morpher.cs:222) — `Self::surface_of`'s sibling for generation output, rendered with
    /// [`surface::to_plain_string`] (the first-matching-representation, no-brackets renderer) instead
    /// of [`surface::to_regex_display`] (the signature's bracket-alternation renderer). Uses the
    /// word's own `stratum` table exactly as `Self::surface_of` does — see that method's doc for why
    /// this equals the surface stratum for a fully-synthesized word.
    fn generated_surface_of(&self, w: &Word) -> String {
        let g = self.g;
        let table = &g.char_tables[g.strata[w.stratum.0 as usize].table.0 as usize];
        surface::to_plain_string(table, &w.shape, false)
    }
}

/// C# `Morpher.ContainsFeature` (Morpher.cs:599-611): the feature is present at the top level or
/// inside any complex (nested) feature value.
fn contains_feature(fs: &FeatureStruct, feat: FeatId) -> bool {
    if fs.get(feat).is_some() {
        return true;
    }
    fs.entries().iter().any(|(_, v)| match v {
        FeatureValue::Complex(inner) => contains_feature(inner, feat),
        FeatureValue::Symbolic(_) => false,
    })
}

#[cfg(test)]
mod trace_tests {
    //! P12 chunk 2: `is_word_valid_traced`'s `PartialParse` clause is exercised here (rather than in
    //! `hc-parse/tests/trace_gate.rs`) because a natural repro needs a multi-stratum/template
    //! scenario this crate's shared integration-test grammar helper does not cheaply build (see
    //! `docs/p12-tracemanager-design.md` §5 chunk 2's own note); a hand-built `Word` against a
    //! minimal (zero-stratum) grammar exercises the exact gate directly and deterministically --
    //! `is_word_valid_traced` never reads `Grammar::strata` at all, only `w.mrule_app_index`/
    //! `w.obligatory`/`w.syn_fs` plus `hc_rules::validity::allomorphs_valid_cached` (a no-op on a
    //! `Word` with no morphs), so a minimal grammar is sufficient.

    use super::*;
    use hc_rules::trace::{FailureReason, TraceType, TreeTraceSink};
    use hc_shape::ShapeBuilder;

    /// The smallest grammar `hc_grammar::load` accepts (no strata, no rules) -- sufficient because
    /// `is_word_valid_traced` never reads `Grammar::strata`.
    fn minimal_grammar() -> hc_grammar::model::Grammar {
        const XML: &str = r#"<HermitCrabInput><Language><Name>X</Name>
          <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
          <HeadFeatures />
          <MorphologicalPhonologicalRuleFeatures>
            <MorphologicalPhonologicalRuleFeature id="mprA">Alpha</MorphologicalPhonologicalRuleFeature>
            <MorphologicalPhonologicalRuleFeatureGroup features="mprA"><Name>G</Name></MorphologicalPhonologicalRuleFeatureGroup>
          </MorphologicalPhonologicalRuleFeatures>
          <CharacterDefinitionTable id="t1">
            <Name>Main</Name>
            <SegmentDefinitions><SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
          </CharacterDefinitionTable>
          <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="cA" /></SegmentNaturalClass></NaturalClasses>
        </Language></HermitCrabInput>"#;
        hc_grammar::load(XML).unwrap_or_else(|e| panic!("minimal_grammar failed to load: {e}"))
    }

    fn w() -> Word {
        Word::new(ShapeBuilder::new().finish(), StratumId(0))
    }

    #[test]
    fn partial_parse_is_reported_when_an_unapplied_rule_never_confirms() {
        let g = minimal_grammar();
        let m = Morpher::new(&g, usize::MAX);
        let mut word = w();
        // A leftover unapplied rule that was never re-confirmed by synthesis (C#'s
        // `mruleAppIndex != -1`, Morpher.cs:723's first clause).
        word.mrule_apps = vec![Some(hc_grammar::model::MRuleId(0))];
        word.mrule_app_index = 0;

        let sink = TreeTraceSink::new();
        let root = sink.analyze_word(&word);
        let ok = m.is_word_valid_traced(&word, &sink, root);
        assert!(!ok);

        let child = *sink.node(root).children.first().expect("Failed must be appended under root");
        assert_eq!(sink.node(child).type_, TraceType::Failed);
        assert_eq!(sink.node(child).failure_reason, Some(FailureReason::PartialParse));
    }

    #[test]
    fn valid_word_with_no_pending_rules_and_no_obligatory_features_passes() {
        let g = minimal_grammar();
        let m = Morpher::new(&g, usize::MAX);
        let word = w(); // mrule_app_index == -1, obligatory empty, no morphs.

        let sink = TreeTraceSink::new();
        let root = sink.analyze_word(&word);
        let ok = m.is_word_valid_traced(&word, &sink, root);
        assert!(ok, "a fresh Word with nothing pending must be valid");
        assert!(sink.node(root).children.is_empty(), "no Failed event should fire for a valid word");
    }

    #[test]
    fn noop_sink_path_is_unaffected_by_is_word_valid_traced() {
        // The untraced `is_word_valid` wrapper must still work (NoopSink + DUMMY handle), proving
        // the traced/untraced code paths share one implementation without regressing the plain one.
        let g = minimal_grammar();
        let m = Morpher::new(&g, usize::MAX);
        let mut word = w();
        word.mrule_apps = vec![Some(hc_grammar::model::MRuleId(0))];
        word.mrule_app_index = 0;
        assert!(!m.is_word_valid(&word));
    }
}
