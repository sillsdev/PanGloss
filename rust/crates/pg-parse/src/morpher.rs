//! The end-to-end parse pipeline: segment → analyze (unapply) → lexical lookup → synthesize
//! (confirm) → validity/surface filter → dedup → signature. Analysis chains strata
//! surface→deepest; synthesis chains them deepest→surface. Ports C# `Morpher.ParseWord`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use pg_featstruct::{FeatId, FeatureStruct, FeatureValue, FsId};
use pg_grammar::model::{
    AllomorphId, AllomorphOwner, Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId, MprSet,
    StratumId,
};
use pg_memo::AnalysisScope;
use pg_rules::cache::RuleCache;
use pg_rules::shape_feat::segment_with_features;
use pg_rules::stratum::{AnalyzerConfig, NonHeadRootFilter};
use pg_rules::trace::{FailureReason, NoopSink, TraceHandle, TraceSink};
use pg_rules::word::{
    MorphRecord, ResolvedRoot, RuntimeRoot, SuppliedAuthorityData, Word, WordKey,
};
use rustc_hash::FxHashMap as HashMap;

use crate::guess;
use crate::root_trie::{collect_lexical_patterns, RootAllomorphIndex};
use crate::{result_signature, surface, AnalysisProvenance, SuppliedRootOverlay, WordAnalysis};

/// The compiled parser for one grammar: the immutable grammar plus its per-stratum root-allomorph
/// tries (C# `Morpher`, built once, parses many words).
pub struct Morpher<'g> {
    g: &'g Grammar,
    root_index: RootAllomorphIndex,
    overlay: Option<&'g SuppliedRootOverlay>,
    /// Every `IsPattern` root allomorph across every stratum, in document order; read only by guess.
    lexical_patterns: Vec<(AllomorphId, LexEntryId)>,
    /// Global per-word step budget threaded through the cascades; `usize::MAX` means uncapped.
    cap: usize,
    /// The order-invariant analysis memo; `false` reproduces the unmemoized engine (`--memo=off`).
    /// Lives on `Morpher`, not `AnalyzerConfig`: full-literal construction there is pinned by `sena_analysis_stratum_terminates_on_short_words`.
    memo: bool,
    /// `--word-timeout-ms`: independent wall-clock deadline alongside `cap`, needed because per-step cost is not uniform.
    word_timeout: Option<Duration>,
    /// Every matcher this grammar's rules need, compiled once and shared read-only across `--threads=N` workers.
    cache: RuleCache,
    /// C#'s settable `Morpher.MaxStemCount`; default `2`, and raising it stays bounded by the shared step/timeout budget.
    max_stem_count: u32,
}

/// Shared instrumentation and hard limits for bounded synthesis across multiple derivations.
pub struct SynthesisBudget {
    steps: pg_rules::stratum::StepBudget,
    candidate_cap: usize,
    candidates: Cell<usize>,
    candidates_capped: Cell<bool>,
}

impl SynthesisBudget {
    pub fn new(step_cap: usize, candidate_cap: usize, timeout: Duration) -> Self {
        Self {
            steps: pg_rules::stratum::StepBudget::new(step_cap)
                .with_timeout(Some(timeout))
                .with_synthesis_counting(),
            candidate_cap,
            candidates: Cell::new(0),
            candidates_capped: Cell::new(false),
        }
    }
    fn admit_candidate(&self) -> bool {
        if self.candidates.get() >= self.candidate_cap {
            self.candidates_capped.set(true);
            return false;
        }
        self.candidates.set(self.candidates.get() + 1);
        true
    }
    pub fn steps(&self) -> usize {
        self.steps.steps()
    }
    pub fn candidates(&self) -> usize {
        self.candidates.get()
    }
    pub fn step_capped(&self) -> bool {
        self.steps.capped()
    }
    pub fn candidate_capped(&self) -> bool {
        self.candidates_capped.get()
    }
    pub fn timed_out(&self) -> bool {
        self.steps.timed_out()
    }
}

/// The outcome of parsing one word.
pub struct ParseOutcome {
    /// The `(morpheme-id-join, surface)` pairs, one per surviving analysis, ready for
    /// `result_signature`. Empty = no analyses (signature `-`).
    pub analyses: Vec<(String, String)>,
    /// The numeric-id mirror of `analyses`: same length, same index correspondence, built from the
    /// same `Morpher::allomorphs_in_morph_order` traversal, so the two views of the morpheme
    /// sequence cannot drift. `pg-ffi` re-sorts both views together before encoding.
    pub structured: Vec<WordAnalysis>,
    /// Whether the analysis step budget fired on any stratum (partial results possible).
    pub capped: bool,
    /// The surface word did not segment (C# `InvalidShapeException` → batch status `SKIPPED`).
    pub invalid_shape: bool,
    /// Diagnostic only, not part of any C# contract: the step budget's raw tick count for this
    /// call, independent of whether the cap was hit.
    pub steps: usize,
    /// Whether `--word-timeout-ms`'s wall-clock deadline fired for this word. Independent of
    /// `capped` — either can fire without the other. Always `false` for the `invalid_shape` early
    /// return, which happens before the budget is constructed.
    pub timed_out: bool,
    /// True iff these analyses came from the guess branch. That branch is all-or-nothing (it runs
    /// only on a total normal-lexicon miss), so one flag describes every returned analysis and
    /// each `structured[i].guessed` mirrors it.
    pub guessed: bool,
    /// Diagnostic only, not part of any C# contract: synthesis candidates yielded before the
    /// validity/match gate, across both the normal loop and the guess branch. `structured.len()`
    /// is the accepted counterpart; 0 on both early returns, which synthesize nothing.
    pub candidates_generated: usize,
}

impl ParseOutcome {
    /// The batch signature (`BatchCommand.BuildSignature`): sorted, `;`-joined, `-` when empty.
    pub fn signature(&self) -> String {
        result_signature(&self.analyses)
    }
}

/// The no-analyses outcome: every counter/flag except `invalid_shape` is zero/false.
fn empty_outcome(invalid_shape: bool) -> ParseOutcome {
    ParseOutcome {
        analyses: Vec::new(),
        structured: Vec::new(),
        capped: false,
        invalid_shape,
        steps: 0,
        timed_out: false,
        guessed: false,
        candidates_generated: 0,
    }
}

/// Per-call parse knobs — C#'s per-call `guessRoot` parameter rather than a construction-time
/// flag, because callers toggle it per word. `#[non_exhaustive]`: future per-call knobs land here
/// instead of more `parse_word_*` method variants.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseOptions {
    /// C#'s `guessRoot`: when true AND the normal (real-lexicon) synthesis pass returns zero
    /// results, retry via the lexical-pattern guesser.
    pub guess_root: bool,
    /// Run the lexical guesser without consulting real or supplied root lookup. Intended for
    /// orchestration after an independently confirmed total lexical miss.
    pub guess_only: bool,
}

impl ParseOptions {
    /// Builder-style setter for `Self::guess_root` — `#[non_exhaustive]` blocks callers outside
    /// this crate from using struct-literal syntax (even with `..Default::default()`), so this is
    /// the public construction path.
    pub fn with_guess_root(mut self, guess_root: bool) -> Self {
        self.guess_root = guess_root;
        self
    }

    pub fn with_guess_only(mut self, guess_only: bool) -> Self {
        self.guess_only = guess_only;
        if guess_only {
            self.guess_root = true;
        }
        self
    }
}

impl<'g> Morpher<'g> {
    /// Build the parser: one root-allomorph trie per stratum (C# `Morpher` ctor, Morpher.cs:35-48).
    pub fn new(g: &'g Grammar, cap: usize) -> Self {
        Morpher {
            g,
            root_index: RootAllomorphIndex::build(g),
            overlay: None,
            lexical_patterns: collect_lexical_patterns(g),
            cap,
            memo: true,
            word_timeout: None,
            cache: RuleCache::build(g),
            max_stem_count: 2, // C# `Morpher.MaxStemCount` ctor default (Morpher.cs:56)
        }
    }

    pub fn new_with_overlay(g: &'g Grammar, cap: usize, overlay: &'g SuppliedRootOverlay) -> Self {
        let mut morpher = Self::new(g, cap);
        morpher.overlay = Some(overlay);
        morpher
    }

    fn search_roots(&self, stratum: StratumId, shape: &pg_shape::Shape) -> Vec<ResolvedRoot> {
        let mut roots = Vec::new();
        for (allo, entry) in self.root_index.search(self.g, stratum, shape) {
            let authored_id = &self.g.entries[entry.0 as usize].authored_id;
            if !self
                .overlay
                .is_some_and(|overlay| overlay.suppresses(authored_id))
            {
                roots.push(ResolvedRoot::Grammar(allo, entry));
            }
        }
        if let Some(overlay) = self.overlay {
            roots.extend(
                overlay
                    .search(self.g, stratum, shape)
                    .into_iter()
                    .map(ResolvedRoot::Supplied),
            );
        }
        roots
    }

    /// Toggle the order-invariant analysis memo (default on). `false` = the unmemoized baseline (`--memo=off`).
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

    /// Raise the compound stem cap (C#'s settable `Morpher.MaxStemCount`); `2` is the default.
    /// Raising it cannot turn into an unbounded search — see `Self::max_stem_count`.
    pub fn with_max_stem_count(mut self, max_stem_count: u32) -> Self {
        self.max_stem_count = max_stem_count;
        self
    }

    /// Introspection: the lexical-pattern root allomorphs partitioned out of the trie
    /// (mirrors C#'s `_lexicalPatterns`), flat across all strata, document order. Read by the
    /// guess subsystem; also exposed so a gate test can pin the partition directly.
    #[inline]
    pub fn lexical_patterns(&self) -> &[(AllomorphId, LexEntryId)] {
        &self.lexical_patterns
    }

    /// Parse one surface word (C# `Morpher.ParseWord`, guessRoot = false). A thin wrapper over
    /// `Self::parse_word_opts`.
    pub fn parse_word(&self, word: &str) -> ParseOutcome {
        self.parse_word_opts(word, &ParseOptions::default())
    }

    /// `Morpher.ParseWord`'s full per-call surface, including the `guessRoot` parameter. A thin
    /// wrapper over `Self::parse_word_core` with a `NoopSink`.
    pub fn parse_word_opts(&self, word: &str, opts: &ParseOptions) -> ParseOutcome {
        let sink = NoopSink;
        self.parse_word_core(word, opts, &sink)
    }

    /// The tracing entry point (`pangloss parse --trace`): `Self::parse_word_opts` with a real
    /// sink. This function mints only the root `WordAnalysis` node, as C# `AnalyzeWord` does at
    /// `ParseWord` entry; every node beneath it is emitted by the cascades and gates themselves.
    pub fn parse_word_traced(
        &self,
        word: &str,
        opts: &ParseOptions,
        trace: &dyn TraceSink,
    ) -> ParseOutcome {
        self.parse_word_core(word, opts, trace)
    }

    /// The restricted-analysis entry point: C#'s `LexEntrySelector`/`RuleSelector`, taken as
    /// per-call parameters rather than mutable instance state, so it is thread-safe by
    /// construction. `None` for either reproduces `Self::parse_word_opts`.
    pub fn parse_word_selected(
        &self,
        word: &str,
        opts: &ParseOptions,
        lex_entry_filter: Option<&dyn Fn(LexEntryId) -> bool>,
        rule_filter: Option<pg_rules::stratum::RuleFilter>,
    ) -> ParseOutcome {
        let sink = NoopSink;
        self.parse_word_core_selected(word, opts, &sink, lex_entry_filter, rule_filter, None)
    }

    /// `Self::parse_word_selected`'s traced sibling, on exactly the selector-restricted path
    /// `pg_foma::confirm` uses: it lets a caller classify WHY a restricted reparse produced no
    /// match — a validity-gate `FailureReason` versus no derivation at all.
    pub fn parse_word_selected_traced(
        &self,
        word: &str,
        opts: &ParseOptions,
        trace: &dyn TraceSink,
        lex_entry_filter: Option<&dyn Fn(LexEntryId) -> bool>,
        rule_filter: Option<pg_rules::stratum::RuleFilter>,
    ) -> ParseOutcome {
        self.parse_word_core_selected(word, opts, trace, lex_entry_filter, rule_filter, None)
    }

    /// `Self::parse_word_opts` plus a per-word `--stats` collector: gated collection, opt-in via
    /// this separate entry point so every other caller pays nothing. Returns the ordinary outcome
    /// alongside the collector's drained rows for a later task to persist.
    pub fn parse_word_with_stats(
        &self,
        word: &str,
        opts: &ParseOptions,
    ) -> (ParseOutcome, Vec<pg_rules::stats::StatsRow>) {
        let stats = pg_rules::stats::StatsCollector::new(self.g);
        let sink = NoopSink;
        let outcome = self.parse_word_core_selected(word, opts, &sink, None, None, Some(&stats));
        (outcome, stats.rows())
    }

    /// Shared body behind `Self::parse_word_opts`/`Self::parse_word_traced`; every trace call here must be guarded by `trace.is_tracing()`, since `NoopSink` panics otherwise.
    fn parse_word_core(
        &self,
        word: &str,
        opts: &ParseOptions,
        trace: &dyn TraceSink,
    ) -> ParseOutcome {
        self.parse_word_core_selected(word, opts, trace, None, None, None)
    }

    /// The actual shared implementation; `Self::parse_word_core` is a thin `(None, None, None)` wrapper over this.
    fn parse_word_core_selected(
        &self,
        word: &str,
        opts: &ParseOptions,
        trace: &dyn TraceSink,
        lex_entry_filter: Option<&dyn Fn(LexEntryId) -> bool>,
        rule_filter: Option<pg_rules::stratum::RuleFilter>,
        stats: Option<&pg_rules::stats::StatsCollector>,
    ) -> ParseOutcome {
        let g = self.g;
        let n = g.strata.len();
        if n == 0 {
            return empty_outcome(false);
        }
        let surface_stratum = StratumId((n - 1) as u8);
        let surface_table = &g.char_tables[g.strata[surface_stratum.0 as usize].table.0 as usize];

        // 1. Segment the surface word against the surface stratum's table (Morpher.cs:115).
        let shape = match segment_with_features(g, surface_table, word) {
            Ok(s) => s,
            Err(_) => return empty_outcome(true),
        };
        let mut input = Word::new(shape, surface_stratum);

        // `input.trace` carries the handle through every clone, so gates below find their ancestor via `w.trace`.
        let root = if trace.is_tracing() {
            let h = trace.analyze_word(&input);
            input.trace = Some(h);
            h
        } else {
            TraceHandle::DUMMY
        };

        // Analysis runs strata surface→deepest into `results`; tracing disables merging, since a merged trace would understate the search.
        let cfg = AnalyzerConfig {
            merge_equivalent: !trace.is_tracing(),
            max_unapplications: 0,
            max_stem_count: self.max_stem_count,
        };
        // One step budget shared by reference across every stratum × candidate; a per-instance counter would let one word explore `cap` steps per call.
        let budget = pg_rules::stratum::StepBudget::new(self.cap).with_timeout(self.word_timeout);
        // One memo scope per parse, never shared across parses; disabled while tracing for the same reason merging is, above.
        let scope_cell =
            (self.memo && !trace.is_tracing()).then(|| RefCell::new(AnalysisScope::new()));
        let scope = scope_cell.as_ref();
        // Closure lives here because `pg-parse` owns `RootAllomorphIndex` and `pg-rules` cannot depend on `pg-parse`.
        let filter: NonHeadRootFilter =
            &|st: StratumId, shape: &pg_shape::Shape| self.search_roots(st, shape);
        let mut input_set: HashMap<WordKey, Word> = HashMap::default();
        input_set.insert(input.dedup_key(), input);
        let mut results: HashMap<WordKey, Word> = HashMap::default();
        for s in (0..n).rev() {
            // A rejected stratum must not reassign `input_set`: the next stratum still gets the same candidates, and the rejected one contributes nothing to `results`.
            let stratum_ref = pg_rules::stratum::RuleRef::Stratum(StratumId(s as u8));
            if rule_filter.is_some_and(|f| !f(stratum_ref)) {
                continue;
            }
            let mut output_set: HashMap<WordKey, Word> = HashMap::default();
            for w in input_set.values() {
                // `w.trace.unwrap_or(root)` is the resolved-cursor idiom used throughout; an untraced parse pays nothing since `trace.is_tracing()` is false.
                let node_parent = w.trace.unwrap_or(root);
                let res = pg_rules::stratum::analyze_stratum_scoped_filtered_ruled_traced(
                    g,
                    StratumId(s as u8),
                    w.clone(),
                    &cfg,
                    scope,
                    Some(filter),
                    rule_filter,
                    Some(&self.cache),
                    &budget,
                    stats,
                    trace,
                    node_parent,
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

        // Per analysis candidate: lexical lookup → synthesis pipeline → filter (C# `Morpher.Synthesize`).
        let mut candidates_generated: usize = 0;
        let mut matches: HashMap<WordKey, Word> = HashMap::default();
        if !opts.guess_only {
            for aw in results.values() {
                let looked_up =
                    self.lexical_lookup_filtered(aw, lex_entry_filter, trace, root, stats);
                if looked_up.is_empty() {
                    self.record_no_root(stats, aw);
                }
                for syn_word in looked_up {
                    // Recovers the shape-equivalent candidates `merge_equivalent` folded away; skipping this loses real analyses whenever merging is on (the default).
                    for alt in syn_word.expand_alternatives() {
                        for vw in self.synthesis_pipeline_selected(
                            alt,
                            trace,
                            root,
                            rule_filter,
                            &budget,
                            stats,
                            None,
                        ) {
                            candidates_generated += 1;
                            if !self.is_word_valid_traced(&vw, trace, root) {
                                continue;
                            }
                            if !self.is_match_traced(&vw, word, trace, root) {
                                self.record_surface_mismatch(stats, &vw);
                                continue;
                            }
                            self.commit_uses(stats, &vw);
                            matches.entry(vw.dedup_key()).or_insert(vw);
                        }
                    }
                }
            }
        }

        // Guess branch: only on a total miss of the normal path and only when the caller opted in; reuses `results.values()` with no re-analysis.
        let (ordered_matches, guessed): (Vec<Word>, bool) = if opts.guess_root && matches.is_empty()
        {
            let mut guess_matches: Vec<Word> = Vec::new();
            for aw in results.values() {
                if let Some(stats) = stats {
                    stats.record_guesser_attempt(aw.stratum, aw.shape.len() as u64);
                }
                // C#'s `.Distinct()` here is a documented no-op (fresh clones, no `Equals` override), so consuming `guess::lexical_guess`'s output directly is faithful.
                for synthesis_word in
                    guess::lexical_guess(g, &self.lexical_patterns, aw, trace, root)
                {
                    for alt in synthesis_word.expand_alternatives() {
                        for vw in self.synthesis_pipeline_traced(alt, trace, root, &budget, stats) {
                            candidates_generated += 1;
                            if self.is_word_valid_traced(&vw, trace, root)
                                && self.is_match_traced(&vw, word, trace, root)
                            {
                                // No dedup here, unlike the normal path: a plain `Vec`, not a `WordKey`-deduped `HashMap`.
                                guess_matches.push(vw);
                            }
                        }
                    }
                }
            }
            // Descending by morph count; a stable sort deliberately strengthens C#'s unstable `List.Sort` into deterministic tie order.
            guess_matches.sort_by_key(|w| std::cmp::Reverse(w.morphs.len()));
            (guess_matches, true)
        } else {
            (matches.into_values().collect(), false)
        };

        // Both `analyses` and `structured` are pushed in `ordered_matches` order, keeping their indices in correspondence.
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
            candidates_generated,
        }
    }

    /// C# `Morpher.LexicalLookup`: one clone per allomorph of each matched entry; `lex_entry_filter` runs before the distinct-entry dedup.
    fn lexical_lookup_filtered(
        &self,
        aw: &Word,
        lex_entry_filter: Option<&dyn Fn(LexEntryId) -> bool>,
        trace: &dyn TraceSink,
        parent: TraceHandle,
        stats: Option<&pg_rules::stats::StatsCollector>,
    ) -> Vec<Word> {
        // Fires once per call before any root-allomorph search; `aw.trace` resolves to its own node, not the parse root.
        if trace.is_tracing() {
            let node_parent = aw.trace.unwrap_or(parent);
            trace.lexical_lookup(node_parent, aw.stratum, aw);
        }
        let g = self.g;
        if let Some(stats) = stats {
            stats.record_root_index_attempt(aw.stratum, aw.shape.len() as u64);
        }
        let matched = self.search_roots(aw.stratum, &aw.shape);
        // Distinct entries in first-seen order; `lex_entry_filter` runs before the dedup, mirroring C#'s `.Where().Distinct()` order.
        let mut entries: Vec<LexEntryId> = Vec::new();
        for root in &matched {
            let ResolvedRoot::Grammar(_, le) = root else {
                continue;
            };
            if lex_entry_filter.is_some_and(|f| !f(*le)) {
                continue;
            }
            if !entries.contains(le) {
                entries.push(*le);
            }
        }
        let mut out = Vec::new();
        for le in entries {
            let entry = &g.entries[le.0 as usize];
            for (allo_idx, allo) in entry.allomorphs.iter().enumerate() {
                if let Some(stats) = stats {
                    // +1: index 0 must not collide with the `ALLOMORPH_NONE` sentinel.
                    stats.record_lex_entry_attempt(aw.stratum, le, allo_idx as u32 + 1);
                }
                // Per-object self time is booked at one of the three object boundaries
                // `StatsCollector::time_enter` names.
                let _time_lex = stats.map(|stats| {
                    stats.time_enter(
                        pg_rules::stats::ObjectKind::LexEntry,
                        aw.stratum,
                        le.0,
                        allo_idx as u32 + 1,
                        pg_rules::stats::Direction::Analysis,
                    )
                });
                // The clone drops alternatives and records `aw` as its source — the boundary `expand_alternatives` walks back from.
                let mut nw = aw.clone_without_alternatives();
                nw.source = Some(Rc::new(aw.clone()));
                self.set_root_allomorph(&mut nw, le, allo.id, &allo.shape.text);
                out.push(nw);
            }
        }
        for root in matched {
            let ResolvedRoot::Supplied(root) = root else {
                continue;
            };
            let mut nw = aw.clone_without_alternatives();
            nw.source = Some(Rc::new(aw.clone()));
            let table = &g.char_tables[g.strata[root.stratum.0 as usize].table.0 as usize];
            let Ok(shape) = segment_with_features(g, table, &root.lexical_spelling) else {
                continue;
            };
            if let Some(stats) = stats {
                stats.record_overlay_attempt(aw.stratum, shape.len() as u64);
            }
            nw.shape = shape;
            nw.stratum = root.stratum;
            nw.syn_fs = root.syn_fs.clone();
            nw.mpr = root.mpr;
            nw.flags.is_partial = false;
            nw.root_allomorph = Some(AllomorphId::GUESSED);
            nw.root_runtime_id = Some(root.realization_id.clone());
            nw.morphs = vec![
                MorphRecord::new(AllomorphId::GUESSED, MorphemeId::GUESSED, 0)
                    .with_runtime_root(RuntimeRoot::Supplied(root)),
            ];
            out.push(nw);
        }
        out
    }

    /// `Word.SetRootAllomorph`: shape is re-segmented with phonological features because `RootAllomorphDef.shape` is feature-less.
    fn set_root_allomorph(
        &self,
        w: &mut Word,
        le: LexEntryId,
        allo: pg_grammar::model::AllomorphId,
        text: &str,
    ) {
        let g = self.g;
        let entry = &g.entries[le.0 as usize];
        let root_stratum = g.morphemes[entry.morpheme.0 as usize].stratum;
        let table = &g.char_tables[g.strata[root_stratum.0 as usize].table.0 as usize];
        let root_shape = segment_with_features(g, table, text).unwrap_or_else(|_| {
            entry
                .allomorphs
                .iter()
                .find(|a| a.id == allo)
                .map(|a| a.shape.shape.clone())
                .unwrap()
        });
        w.shape = root_shape;
        w.stratum = root_stratum;
        w.syn_fs = g.fs_interner.get(entry.syn_fs).clone();
        w.mpr = entry.mpr;
        w.flags.is_partial = entry.partial;
        w.root_allomorph = Some(allo);
        // MarkMorph(shape, rootAllomorph, RootMorphID): the root is the base morph at order 0.
        w.root_runtime_id = None;
        w.morphs = vec![MorphRecord::new(allo, entry.morpheme, 0)];
    }

    /// `no_root`: charged to the last rule applied on `aw`'s trail, or to the stratum's root lookup when none applied.
    fn record_no_root(&self, stats: Option<&pg_rules::stats::StatsCollector>, aw: &Word) {
        let Some(stats) = stats else { return };
        if let Some(Some(id)) = aw.mrule_apps.last() {
            if let Some(stratum) = self.mrule_stratum(*id) {
                stats.record_no_root_mrule(stratum, *id);
                return;
            }
        }
        stats.record_no_root_root_index(aw.stratum);
    }

    /// `surface_mismatch`: `vw` passed validity but its rendered surface did not match the input word.
    fn record_surface_mismatch(&self, stats: Option<&pg_rules::stats::StatsCollector>, vw: &Word) {
        let Some(stats) = stats else { return };
        if let Some((le, allo_idx)) = self.root_lex_entry(vw) {
            stats.record_surface_mismatch(vw.stratum, le, allo_idx);
        }
    }

    /// `uses`: commit every morphological rule and the root lexical entry on a surviving analysis.
    fn commit_uses(&self, stats: Option<&pg_rules::stats::StatsCollector>, vw: &Word) {
        let Some(stats) = stats else { return };
        let mut seen: Vec<MRuleId> = Vec::new();
        for id in vw.mrule_apps.iter().flatten() {
            if seen.contains(id) {
                continue;
            }
            seen.push(*id);
            if let Some(stratum) = self.mrule_stratum(*id) {
                stats.record_use_mrule(stratum, *id);
            }
        }
        if let Some((le, allo_idx)) = self.root_lex_entry(vw) {
            stats.record_use_lex_entry(vw.stratum, le, allo_idx);
        }
    }

    /// The grammar-resident `(LexEntryId, 1-based allomorph index)` behind `w.root_allomorph`, matching `Self::lexical_lookup_filtered`'s shift; `None` for a guessed or supplied root, which has no `LexEntry` row.
    fn root_lex_entry(&self, w: &Word) -> Option<(LexEntryId, u32)> {
        let id = w.root_allomorph?;
        if id == AllomorphId::GUESSED {
            return None;
        }
        match self.g.allomorph_owners[id.0 as usize] {
            AllomorphOwner::Root(le, idx) => Some((le, idx as u32 + 1)),
            AllomorphOwner::Affix(..) => None,
        }
    }

    /// The stratum owning `id`, by linear scan; stats-gated callers only, not the per-word hot path.
    fn mrule_stratum(&self, id: MRuleId) -> Option<StratumId> {
        let g = self.g;
        for (si, sd) in g.strata.iter().enumerate() {
            if sd.mrules.contains(&id) {
                return Some(StratumId(si as u8));
            }
            for &tid in &sd.templates {
                for slot in &g.templates[tid.0 as usize].slots {
                    if slot.rules.contains(&id) {
                        return Some(StratumId(si as u8));
                    }
                }
            }
        }
        None
    }

    /// Folds the candidate through every stratum deepest→surface, deduping by `WordKey`; the budget here is timeout-less since generation has no per-word deadline.
    fn synthesis_pipeline(&self, syn_word: Word) -> Vec<Word> {
        let sink = NoopSink;
        let budget = pg_rules::stratum::StepBudget::new(self.cap);
        self.synthesis_pipeline_selected(
            syn_word,
            &sink,
            TraceHandle::DUMMY,
            None,
            &budget,
            None,
            None,
        )
    }

    /// `Self::synthesis_pipeline`'s traced sibling; the callee reassigns each word's trace cursor per rule so a multi-rule synthesis renders as a followable sequence.
    fn synthesis_pipeline_traced(
        &self,
        syn_word: Word,
        trace: &dyn TraceSink,
        parent: TraceHandle,
        budget: &pg_rules::stratum::StepBudget,
        stats: Option<&pg_rules::stats::StatsCollector>,
    ) -> Vec<Word> {
        self.synthesis_pipeline_selected(syn_word, trace, parent, None, budget, stats, None)
    }

    /// `Self::synthesis_pipeline_traced`'s selector-restricted sibling; a rejected stratum passes the word through unchanged, and `budget` enforces only the wall-clock deadline.
    #[allow(clippy::too_many_arguments)]
    fn synthesis_pipeline_selected(
        &self,
        syn_word: Word,
        trace: &dyn TraceSink,
        parent: TraceHandle,
        rule_filter: Option<pg_rules::stratum::RuleFilter>,
        budget: &pg_rules::stratum::StepBudget,
        stats: Option<&pg_rules::stats::StatsCollector>,
        work_budget: Option<&SynthesisBudget>,
    ) -> Vec<Word> {
        let g = self.g;
        let n = g.strata.len();
        let mut cur: HashMap<WordKey, Word> = HashMap::default();
        cur.insert(syn_word.dedup_key(), syn_word);
        for s in 0..n {
            if cur.is_empty() {
                break; // PipelineRuleCascade stops once the working set empties (cs:20).
            }
            let stratum_ref = pg_rules::stratum::RuleRef::Stratum(StratumId(s as u8));
            let admitted = rule_filter.is_none_or(|f| f(stratum_ref));
            let mut next: HashMap<WordKey, Word> = HashMap::default();
            for w in cur.values() {
                if !admitted {
                    // SynthesisStratumRule.cs:51: `return input.ToEnumerable();` — pass through.
                    next.entry(w.dedup_key()).or_insert_with(|| w.clone());
                    continue;
                }
                let node_parent = w.trace.unwrap_or(parent);
                for o in pg_rules::stratum::synthesize_stratum_traced(
                    g,
                    StratumId(s as u8),
                    w.clone(),
                    self.cap,
                    &self.cache,
                    budget,
                    stats,
                    trace,
                    node_parent,
                ) {
                    if work_budget.is_some_and(|budget| !budget.admit_candidate()) {
                        return Vec::new();
                    }
                    next.entry(o.dedup_key()).or_insert(o);
                }
            }
            cur = next;
        }
        cur.into_values().collect()
    }

    /// Mirrors C#'s `Morpher.IsWordValid`: every rule re-applied, every obligatory feature present, every allomorph passing its own gates.
    fn is_word_valid(&self, w: &Word) -> bool {
        let sink = NoopSink;
        self.is_word_valid_traced(w, &sink, TraceHandle::DUMMY)
    }

    /// The implementation `Self::is_word_valid` also uses; owns only the two morpher-level failure clauses.
    /// Pinned by `partial_parse_is_reported_when_an_unapplied_rule_never_confirms`.
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
        pg_rules::validity::allomorphs_valid_cached_traced(self.g, w, &self.cache, trace, parent)
    }

    /// Mirrors C#'s `Morpher.IsMatch`; call sites `&&` this after `Self::is_word_valid_traced` in that order, which decides which `Failed` node fires.
    fn is_match_traced(
        &self,
        w: &Word,
        word: &str,
        trace: &dyn TraceSink,
        parent: TraceHandle,
    ) -> bool {
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

    /// The signature's surface half, rendered against the word's own stratum table (= surface stratum once fully synthesized).
    fn surface_of(&self, w: &Word) -> String {
        let g = self.g;
        let table = &g.char_tables[g.strata[w.stratum.0 as usize].table.0 as usize];
        surface::to_regex_display(table, &w.shape)
    }

    /// `Word.AllomorphsInMorphOrder`: distinct allomorphs in first-occurrence order; both the display string and the FFI ids project from this one traversal so they cannot disagree.
    fn allomorphs_in_morph_order(&self, w: &Word) -> Vec<MorphRecord> {
        let mut ms = w.morphs.clone();
        ms.sort_by_key(|m| m.order);
        let mut seen: Vec<(AllomorphId, Option<String>)> = Vec::new();
        ms.into_iter()
            .filter(|m| {
                let key = (
                    m.allomorph,
                    pg_rules::word::runtime_id(m.runtime_root.as_deref()).map(str::to_owned),
                );
                if seen.contains(&key) {
                    false
                } else {
                    seen.push(key);
                    true
                }
            })
            .collect()
    }

    /// The signature's morpheme half, `+`-joined; `MorphemeId::GUESSED` has no `Grammar::morphemes` row, so it resolves from the runtime root's own text instead.
    fn morpheme_join(&self, w: &Word) -> String {
        let g = self.g;
        self.allomorphs_in_morph_order(w)
            .into_iter()
            .map(|m| {
                if m.morpheme == MorphemeId::GUESSED {
                    m.runtime_root
                        .as_deref()
                        .map(|root| match root {
                            RuntimeRoot::Guessed(gr) => gr.text.clone(),
                            RuntimeRoot::Supplied(root) => root.lexical_spelling.clone(),
                        })
                        .unwrap_or_default()
                } else {
                    g.morphemes[m.morpheme.0 as usize]
                        .morph_id
                        .clone()
                        .unwrap_or_default()
                }
            })
            .collect::<Vec<String>>()
            .join("+")
    }

    /// The FFI's numeric mirror: `Self::allomorphs_in_morph_order` projected to dense ordinals rather than the `<MorphemeId>` strings `morpheme_join` prints.
    fn structured_analysis(&self, w: &Word, guessed: bool) -> WordAnalysis {
        let seq = self.allomorphs_in_morph_order(w);
        let morpheme_ids: Vec<u32> = seq.iter().map(|m| m.morpheme.0).collect();
        let root_morpheme_index = seq
            .iter()
            .position(|m| {
                Some(m.allomorph) == w.root_allomorph
                    && pg_rules::word::runtime_id(m.runtime_root.as_deref())
                        == w.root_runtime_id.as_deref()
            })
            .map(|i| i as i32)
            .unwrap_or(-1);
        let pos_id = match w.syn_fs.get(self.g.syn_features.pos) {
            Some(FeatureValue::Symbolic(bits)) => bits.first(),
            _ => None,
        };
        WordAnalysis {
            morpheme_ids,
            root_morpheme_index,
            pos_id,
            syn_fs: w.syn_fs.clone(),
            mpr: w.mpr,
            guessed,
            provenance: match w.root_runtime() {
                Some(RuntimeRoot::Guessed(_)) => AnalysisProvenance::Guessed,
                Some(RuntimeRoot::Supplied(root)) => match &root.authority {
                    SuppliedAuthorityData::Supplied => AnalysisProvenance::Supplied {
                        entry_id: root.entry_id.clone(),
                    },
                    SuppliedAuthorityData::Override { official_entry_id } => {
                        AnalysisProvenance::SuppliedOverride {
                            entry_id: root.entry_id.clone(),
                            overridden_grammar_entry_id: official_entry_id.clone(),
                        }
                    }
                },
                None => AnalysisProvenance::Grammar,
            },
            supplied_root: w.root_runtime().and_then(|root| match root {
                RuntimeRoot::Supplied(root) => Some(crate::SuppliedRoot::from_data(root)),
                RuntimeRoot::Guessed(_) => None,
            }),
            morpheme_roots: seq
                .iter()
                .map(|m| match m.runtime_root.as_deref() {
                    Some(RuntimeRoot::Supplied(root)) => Some(crate::SuppliedRoot::from_data(root)),
                    _ => None,
                })
                .collect(),
        }
    }
}

// Generation: `Morpher.GenerateWords`, the synthesis-only counterpart to `parse_word`.

/// One "other morpheme" in `Morpher::generate_words`'s direct API. A bare `CompoundingRule` is
/// never a `Morpheme` in C# and so has no variant here; only entries and affix/realizational rules
/// can appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenMorpheme {
    /// A known morphological rule to unapply (`AffixProcessRule`/`RealizationalAffixProcessRule`).
    Rule(MRuleId),
    /// A bare compounding non-head root entry; its owning `CompoundingRule` is discovered by the synthesis confirmation gate, not specified here (see `Word::mrule_apps`).
    NonHead(LexEntryId),
}

/// One resolved slot of a `permute_rules` permutation: a known rule, or a specific non-head root allomorph — every allomorph of a `NonHead` entry is tried.
#[derive(Clone, Copy, Debug)]
enum PermItem {
    Rule(MRuleId),
    NonHead(AllomorphId),
}

/// C# `Morpher.PermuteRules`: cross product of every `NonHead` entry's allomorphs, preserving input order; only the resulting set matters to callers.
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

/// `mrule is RealizationalAffixProcessRule`: the one rule kind that never occupies an `mrule_apps` trail slot (see that field's doc).
fn is_realizational_rule(g: &Grammar, id: MRuleId) -> bool {
    matches!(&g.mrules[id.0 as usize], MorphRuleDef::Realizational(_))
}

/// Which kind of `Morpheme` a grammar-tier `MorphemeId` names; `Compounding` rules are never `Morpheme`s in C#, so they don't resolve here.
enum MorphemeOwner {
    Root(LexEntryId),
    Rule(MRuleId),
}

/// Linear scan over `entries` + `mrules`; fine for generation, a rare caller-initiated call, not the per-word hot path.
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

/// `resolve_morpheme` projected to `GenMorpheme`; `None` if `id` cannot be resolved (malformed caller input).
fn resolve_other(g: &Grammar, id: MorphemeId) -> Option<GenMorpheme> {
    match resolve_morpheme(g, id)? {
        MorphemeOwner::Root(le) => Some(GenMorpheme::NonHead(le)),
        MorphemeOwner::Rule(r) => Some(GenMorpheme::Rule(r)),
    }
}

/// Every order-preserving merge of `left` and `right`; `left` must arrive root-outward (adjacent-to-root first) — callers reverse it before calling.
fn interleavings<T: Clone>(left: &[T], right: &[T]) -> Vec<Vec<T>> {
    fn go<T: Clone>(left: &[T], right: &[T], acc: &mut Vec<T>, out: &mut Vec<Vec<T>>) {
        if left.is_empty() && right.is_empty() {
            out.push(acc.clone());
            return;
        }
        if let Some((head, rest)) = left.split_first() {
            acc.push(head.clone());
            go(rest, right, acc, out);
            acc.pop();
        }
        if let Some((head, rest)) = right.split_first() {
            acc.push(head.clone());
            go(left, rest, acc, out);
            acc.pop();
        }
    }
    let mut out = Vec::new();
    go(left, right, &mut Vec::new(), &mut out);
    out
}

impl<'g> Morpher<'g> {
    /// C# `Morpher.GenerateWords`: one seed per `(root allomorph, other-morpheme permutation)`
    /// pair, kept if `Self::is_word_valid` passes. Must NOT apply the surface-match gate — there is
    /// no input word to match. Sorted rather than C#'s unordered set; only the set is guaranteed.
    pub fn generate_words(
        &self,
        root: LexEntryId,
        others: &[GenMorpheme],
        real_fs: FeatureStruct,
    ) -> Vec<String> {
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
                            seed.morphological_rule_unapplied(
                                is_realizational_rule(g, id),
                                Some(id),
                            );
                        }
                        PermItem::NonHead(allo_id) => {
                            // The non-head gets a fresh empty realizational FS, never the outer `real_fs` this call was given.
                            seed.morphological_rule_unapplied(false, None);
                            let nh = self.build_allomorph_seed(allo_id, FeatureStruct::EMPTY);
                            seed.non_head_unapplied(nh);
                        }
                    }
                }
                self.collect_valid_surfaces(self.synthesis_pipeline(seed), &mut words);
            }
        }
        words.into_iter().collect()
    }

    /// C# `Morpher.GenerateWords(WordAnalysis)`: union `Self::generate_words` over every left/right
    /// interleaving — the word-order recovery the direct API does not attempt. A malformed or
    /// foreign `WordAnalysis` yields no words rather than panicking as C#'s `(LexEntry)` cast would.
    pub fn generate_words_from_analysis(&self, wa: &WordAnalysis) -> Vec<String> {
        if wa.morpheme_ids.is_empty() {
            return Vec::new();
        }
        let g = self.g;
        if wa.root_morpheme_index < 0 || wa.root_morpheme_index as usize >= wa.morpheme_ids.len() {
            return Vec::new();
        }
        let root_idx = wa.root_morpheme_index as usize;
        if wa
            .morpheme_roots
            .iter()
            .enumerate()
            .any(|(i, root)| i != root_idx && root.is_some())
        {
            return self.generate_analysis_with_runtime_non_heads(wa, root_idx);
        }
        let resolve_side = |ids: &[u32]| -> Option<Vec<GenMorpheme>> {
            ids.iter()
                .map(|&id| resolve_other(g, MorphemeId(id)))
                .collect()
        };
        // `left` resolves outermost-prefix-first, but `interleavings` requires root-outward order — hence the reversal below.
        let Some(mut left) = resolve_side(&wa.morpheme_ids[..root_idx]) else {
            return Vec::new();
        };
        left.reverse();
        let Some(right) = resolve_side(&wa.morpheme_ids[root_idx + 1..]) else {
            return Vec::new();
        };

        let mut words: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for others in interleavings(&left, &right) {
            if let Some(root) = &wa.supplied_root {
                let Some(mut seed) = self.supplied_root_seed(root.to_data()) else {
                    continue;
                };
                for other in &others {
                    self.queue_other_morpheme(&mut seed, *other);
                }
                self.collect_valid_surfaces(self.synthesis_pipeline(seed), &mut words);
            } else {
                let Some(MorphemeOwner::Root(root_entry)) =
                    resolve_morpheme(g, MorphemeId(wa.morpheme_ids[root_idx]))
                else {
                    continue;
                };
                for w in self.generate_words(root_entry, &others, FeatureStruct::EMPTY) {
                    words.insert(w);
                }
            }
        }
        words.into_iter().collect()
    }

    /// `Self::generate_words_from_analysis` for runtime-rooted non-root slots, which have no `LexEntryId` to permute; replays the single recorded derivation instead.
    fn generate_analysis_with_runtime_non_heads(
        &self,
        wa: &WordAnalysis,
        root_idx: usize,
    ) -> Vec<String> {
        let g = self.g;
        let mut seed = if let Some(root) = wa.morpheme_roots.get(root_idx).and_then(Option::as_ref)
        {
            let Some(word) = self.supplied_root_seed(root.to_data()) else {
                return Vec::new();
            };
            word
        } else {
            let Some(MorphemeOwner::Root(entry)) =
                resolve_morpheme(g, MorphemeId(wa.morpheme_ids[root_idx]))
            else {
                return Vec::new();
            };
            self.build_root_seed(entry, 0, FeatureStruct::EMPTY)
        };

        let mut indices: Vec<usize> = (0..root_idx).rev().collect();
        indices.extend(root_idx + 1..wa.morpheme_ids.len());
        for i in indices {
            if let Some(root) = wa.morpheme_roots.get(i).and_then(Option::as_ref) {
                let Some(non_head) = self.supplied_root_seed(root.to_data()) else {
                    return Vec::new();
                };
                seed.morphological_rule_unapplied(false, None);
                seed.non_head_unapplied(non_head);
            } else {
                let Some(other) = resolve_other(g, MorphemeId(wa.morpheme_ids[i])) else {
                    return Vec::new();
                };
                self.queue_other_morpheme(&mut seed, other);
            }
        }
        let mut words = std::collections::BTreeSet::new();
        self.collect_valid_surfaces(self.synthesis_pipeline(seed), &mut words);
        words.into_iter().collect()
    }

    /// Seeds a `Word` on a runtime-supplied root (a `GUESSED` sentinel morph); `None` if segmentation fails, which callers treat as contributing nothing.
    fn supplied_root_seed(&self, data: pg_rules::word::SuppliedRootData) -> Option<Word> {
        let g = self.g;
        let table = &g.char_tables[g.strata[data.stratum.0 as usize].table.0 as usize];
        let shape = segment_with_features(g, table, &data.lexical_spelling).ok()?;
        let mut w = Word::new(shape, data.stratum);
        w.syn_fs = data.syn_fs.clone();
        w.mpr = data.mpr;
        w.root_allomorph = Some(AllomorphId::GUESSED);
        w.root_runtime_id = Some(data.realization_id.clone());
        w.morphs = vec![
            MorphRecord::new(AllomorphId::GUESSED, MorphemeId::GUESSED, 0)
                .with_runtime_root(RuntimeRoot::Supplied(data)),
        ];
        Some(w)
    }

    /// Queues one resolved "other morpheme" onto a generation seed; a non-head takes a wildcard (`None`) trail slot so synthesis discovers its owning rule.
    fn queue_other_morpheme(&self, seed: &mut Word, other: GenMorpheme) {
        match other {
            GenMorpheme::Rule(id) => {
                seed.morphological_rule_unapplied(is_realizational_rule(self.g, id), Some(id));
            }
            GenMorpheme::NonHead(le) => {
                seed.morphological_rule_unapplied(false, None);
                seed.non_head_unapplied(self.build_root_seed(le, 0, FeatureStruct::EMPTY));
            }
        }
    }

    /// Keeps every `Self::is_word_valid` survivor's rendered surface; generation has no surface-match gate.
    fn collect_valid_surfaces(
        &self,
        generated: Vec<Word>,
        out: &mut std::collections::BTreeSet<String>,
    ) {
        for w in generated {
            if self.is_word_valid(&w) {
                out.insert(self.generated_surface_of(&w));
            }
        }
    }

    /// A fresh root-level seed `Word` at `allo_idx`; explicit (unlike `pg_rules::morph::seed_from_entry`'s always-primary allomorph) because generation tries every allomorph.
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

    /// `Self::build_root_seed` for an `AllomorphId` already known to be a root allomorph, resolved via `Grammar::allomorph_owners`.
    fn build_allomorph_seed(&self, allo_id: AllomorphId, real_fs: FeatureStruct) -> Word {
        let g = self.g;
        let AllomorphOwner::Root(le, idx) = g.allomorph_owners[allo_id.0 as usize] else {
            unreachable!(
                "PermItem::NonHead is only ever built from a root allomorph (permute_rules)"
            )
        };
        self.build_root_seed(le, idx as usize, real_fs)
    }

    /// `Self::surface_of`'s sibling for generation output: the plain first-matching-representation renderer, not the bracket-alternation one.
    fn generated_surface_of(&self, w: &Word) -> String {
        let g = self.g;
        let table = &g.char_tables[g.strata[w.stratum.0 as usize].table.0 as usize];
        surface::to_plain_string(table, &w.shape, false)
    }

    /// Synthesize a supplied root whose grammatical class is already resolved. It can replay a
    /// whole multi-rule derivation; bounded exploration is deliberately left to the caller.
    pub fn synthesize_resolved_stem(
        &self,
        shape_text: &str,
        syn_fs: FsId,
        mpr: MprSet,
        stratum: StratumId,
        rules: &[MRuleId],
    ) -> Vec<String> {
        self.synthesize_resolved_stem_impl(shape_text, syn_fs, mpr, stratum, rules, None)
    }

    /// `Self::synthesize_resolved_stem` under a caller-owned `SynthesisBudget`, which caps
    /// candidates as well as steps and reports which limit truncated the run.
    pub fn synthesize_resolved_stem_bounded(
        &self,
        shape_text: &str,
        syn_fs: FsId,
        mpr: MprSet,
        stratum: StratumId,
        rules: &[MRuleId],
        budget: &SynthesisBudget,
    ) -> Vec<String> {
        self.synthesize_resolved_stem_impl(shape_text, syn_fs, mpr, stratum, rules, Some(budget))
    }

    fn synthesize_resolved_stem_impl(
        &self,
        shape_text: &str,
        syn_fs: FsId,
        mpr: MprSet,
        stratum: StratumId,
        rules: &[MRuleId],
        work_budget: Option<&SynthesisBudget>,
    ) -> Vec<String> {
        let g = self.g;
        if stratum.0 as usize >= g.strata.len() {
            return Vec::new();
        }
        let realization_id = format!("classification:{shape_text}");
        let supplied = pg_rules::word::SuppliedRootData {
            entry_id: realization_id.clone(),
            realization_id,
            authority: pg_rules::word::SuppliedAuthorityData::Supplied,
            lexical_spelling: shape_text.to_string(),
            gloss: String::new(),
            syn_fs: g.fs_interner.get(syn_fs).clone(),
            mpr,
            stratum,
        };
        let Some(mut word) = self.supplied_root_seed(supplied) else {
            return Vec::new();
        };
        for &rule in rules {
            if rule.0 as usize >= g.mrules.len() {
                return Vec::new();
            }
            word.morphological_rule_unapplied(is_realizational_rule(g, rule), Some(rule));
        }
        let generated = if let Some(work) = work_budget {
            let sink = NoopSink;
            self.synthesis_pipeline_selected(
                word,
                &sink,
                TraceHandle::DUMMY,
                None,
                &work.steps,
                None,
                Some(work),
            )
        } else {
            self.synthesis_pipeline(word)
        };
        let mut out = std::collections::BTreeSet::new();
        self.collect_valid_surfaces(generated, &mut out);
        out.into_iter().collect()
    }
}

/// C# `Morpher.ContainsFeature`: true if the feature is present at the top level or inside any nested feature value.
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
    //! Unit tests rather than integration ones: `is_word_valid_traced` never reads
    //! `Grammar::strata`, so a hand-built `Word` against a zero-stratum grammar drives the gate
    //! directly, where a natural repro would need a multi-stratum/template scenario.

    use super::*;
    use pg_rules::trace::{FailureReason, TraceType, TreeTraceSink};
    use pg_shape::ShapeBuilder;

    /// The smallest grammar `pg_grammar::load` accepts; sufficient since `is_word_valid_traced` doesn't read `Grammar::strata`.
    fn minimal_grammar() -> pg_grammar::model::Grammar {
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
        pg_grammar::load(XML).unwrap_or_else(|e| panic!("minimal_grammar failed to load: {e}"))
    }

    fn w() -> Word {
        Word::new(ShapeBuilder::new().finish(), StratumId(0))
    }

    #[test]
    fn partial_parse_is_reported_when_an_unapplied_rule_never_confirms() {
        let g = minimal_grammar();
        let m = Morpher::new(&g, usize::MAX);
        let mut word = w();
        // A leftover unapplied rule never re-confirmed by synthesis (C#'s `mruleAppIndex != -1`).
        word.mrule_apps = vec![Some(pg_grammar::model::MRuleId(0))];
        word.mrule_app_index = 0;

        let sink = TreeTraceSink::new();
        let root = sink.analyze_word(&word);
        let ok = m.is_word_valid_traced(&word, &sink, root);
        assert!(!ok);

        let child = *sink
            .node(root)
            .children
            .first()
            .expect("Failed must be appended under root");
        assert_eq!(sink.node(child).type_, TraceType::Failed);
        assert_eq!(
            sink.node(child).failure_reason,
            Some(FailureReason::PartialParse)
        );
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
        assert!(
            sink.node(root).children.is_empty(),
            "no Failed event should fire for a valid word"
        );
    }

    #[test]
    fn noop_sink_path_is_unaffected_by_is_word_valid_traced() {
        // Confirms the untraced `is_word_valid` wrapper (NoopSink + DUMMY handle) shares one implementation with the traced path.
        let g = minimal_grammar();
        let m = Morpher::new(&g, usize::MAX);
        let mut word = w();
        word.mrule_apps = vec![Some(pg_grammar::model::MRuleId(0))];
        word.mrule_app_index = 0;
        assert!(!m.is_word_valid(&word));
    }
}
