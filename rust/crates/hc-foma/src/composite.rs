//! `FomaAnalyzer` (plan §1 architecture / P2 "propose→confirm composite", gate F2): the public
//! product API tying [`crate::analyzer::FomaProposer`] (propose), [`crate::peel::ReduplicationPeeler`]
//! (D6), and [`crate::confirm`] (verify + D4 multiplicity recovery) into one `analyze_word` call
//! whose output shape mirrors `hc_parse::ParseOutcome`'s essentials — `analyses`/`structured`,
//! parallel by index, `hc-parse/src/morpher.rs:79-120` — plus diagnostics.
//!
//! Pipeline (plan §1's diagram): `propose(word)` UNION `peel_candidates(word, propose)`, deduped by
//! `(morphemes, root_index)` (plan §2: "Allomorph IDs are NOT part of candidate identity"), then
//! `confirm_all` on each surviving candidate, concatenating every match. Over-generation is pruned
//! silently by `confirm_all` (candidates that don't re-derive under restricted search simply
//! contribute zero matches); under-generation would be a recall bug in `propose`/`peel_candidates`
//! themselves (P1's job, already gated).

use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

use hc_grammar::model::Grammar;
use hc_parse::{Morpher, WordAnalysis};

use crate::analyzer::{FomaError, FomaProposer};
use crate::confirm::{self, MorphemeOwner};
use crate::peel::ReduplicationPeeler;
use crate::tags::Candidate;

/// The outcome of [`FomaAnalyzer::analyze_word`] — the `hc_parse::ParseOutcome`-compatible shape
/// plan P2 calls for (`analyses`/`structured`), plus diagnostics the P2 gate's numbers come from:
/// how many distinct candidates were proposed before confirm, how many survived confirm, and
/// whether the reduplication peel contributed any candidate for this particular word.
pub struct FomaOutcome {
    /// `(morpheme-join, surface)` pairs, one per confirmed analysis — parallel to `structured` by
    /// index, exactly like `hc_parse::ParseOutcome::analyses`/`structured`.
    pub analyses: Vec<(String, String)>,
    pub structured: Vec<WordAnalysis>,
    /// Distinct `(morphemes, root_index)` candidates offered to confirm (propose UNION peel,
    /// deduped) — the over-generation half of the P2 gate's headline number.
    pub candidates_generated: usize,
    /// `structured.len()` — kept as its own field (rather than making callers re-derive it) since
    /// it is the OTHER half of the same headline number (candidates_generated vs confirmed).
    pub confirmed: usize,
    /// Whether [`crate::peel::ReduplicationPeeler::peel_candidates`] returned at least one
    /// candidate for this word (regardless of whether it survived the union dedup against
    /// `propose`'s own output) — the redup gate's own diagnostic (plan P2 gate item "redup words
    /// round-trip").
    pub peel_used: bool,
}

/// One grammar's compiled foma proposer + uncapped verify [`Morpher`] + prebuilt morpheme-owner map
/// + redup peeler, owned together (plan §1: "propose→confirm composite"). `'g` ties this to the
/// same `&Grammar` borrow the verify `Morpher` itself needs.
pub struct FomaAnalyzer<'g> {
    g: &'g Grammar,
    proposer: FomaProposer,
    peeler: ReduplicationPeeler,
    morpher: Morpher<'g>,
    owners: Vec<Option<MorphemeOwner>>,
}

impl<'g> FomaAnalyzer<'g> {
    /// Emit + foma-compile `g` (via [`FomaProposer::new`]), build the redup peeler, an UNCAPPED
    /// verify `Morpher` (`Morpher::new(g, usize::MAX)` — see [`crate::confirm::confirm_all`]'s doc
    /// for why a cap here would be a silent parity bug, not a performance knob), and the
    /// morpheme-owner reverse map confirm needs. `Err` iff the grammar's emitted lexc source itself
    /// fails to foma-compile. Per the revised plan §0 there is no per-grammar fallback tier: this
    /// composite IS the mainline for every grammar, so a compile failure here is an emitter gap to
    /// fix (later plan stages), not a routing decision — the `Err` just surfaces it to the caller.
    pub fn new(g: &'g Grammar) -> Result<Self, FomaError> {
        let proposer = FomaProposer::new(g)?;
        Ok(FomaAnalyzer {
            g,
            proposer,
            peeler: ReduplicationPeeler::new(g),
            morpher: Morpher::new(g, usize::MAX),
            owners: confirm::build_morpheme_owners(g),
        })
    }

    /// `propose(word)` UNION `peel_candidates(word, propose)` (deduped by `(morphemes, root_index)`,
    /// first-seen order) → `confirm_all` on every surviving candidate → concatenate every match.
    /// Empty (never panics) for a word neither the proposer nor the peel can reach at all, and for
    /// a word the engine itself would only reach via `guess_root` (this crate never sets it —
    /// `confirm_all` always calls `parse_word_selected` with `ParseOptions::default()` — so the
    /// result here is consistent with `Morpher::parse_word_opts(word, &ParseOptions::default())`
    /// under the SAME options, matching P2's own gate requirement).
    pub fn analyze_word(&mut self, word: &str) -> FomaOutcome {
        let (candidates, peel_used) = self.propose_candidates(word);
        let candidates_generated = candidates.len();
        let mut analyses = Vec::new();
        let mut structured = Vec::new();
        // Batched confirm (John, 2026-07-15): ONE union re-parse routes every outcome analysis to
        // its candidate's bucket — content-identical to per-candidate confirm_all (soundness
        // argument in `confirm::confirm_batch`'s doc) at 1/N the re-parse cost. Buckets come back
        // in candidate order, each in outcome order, preserving the previous concatenation order.
        for bucket in confirm::confirm_batch(self.g, &self.owners, &self.morpher, &candidates, word)
        {
            for (wa, join, surface) in bucket {
                structured.push(wa);
                analyses.push((join, surface));
            }
        }

        FomaOutcome {
            confirmed: structured.len(),
            analyses,
            structured,
            candidates_generated,
            peel_used,
        }
    }

    /// `propose(word)` UNION `peel_candidates(word, propose)`, deduped by `(morphemes,
    /// root_index)` — the pre-confirm half of [`Self::analyze_word`]/[`Self::analyze_words`],
    /// factored out so the batch path can run this stage sequentially over every word (see
    /// [`Self::analyze_words`]'s doc for why it stays sequential) before handing the results to
    /// confirm. Returns the deduped candidate list plus whether the redup peel contributed
    /// anything for this word.
    fn propose_candidates(&mut self, word: &str) -> (Vec<Candidate>, bool) {
        let mut candidates: Vec<Candidate> = self.proposer.propose(word);

        // Disjoint field borrows: `proposer` borrows only `self.proposer` (mutably); the
        // `peel_candidates` call below borrows only `self.peeler` (immutably) and copies `self.g`
        // (a `&Grammar`) — no conflict, since neither touches the other's field.
        let peeled: Vec<Candidate> = {
            let proposer = &mut self.proposer;
            let mut propose_fn = |r: &str| proposer.propose(r);
            self.peeler.peel_candidates(self.g, word, &mut propose_fn)
        };
        let peel_used = !peeled.is_empty();

        for c in peeled {
            let already_present = candidates
                .iter()
                .any(|existing| existing.root_index == c.root_index && existing.morphemes == c.morphemes);
            if !already_present {
                candidates.push(c);
            }
        }

        // Plan §2/D4: distinct candidates yield disjoint matched sequences (confirm's
        // `analyses_match` is keyed on exactly a candidate's own `(morphemes, root_index)`), so no
        // cross-candidate double-count is possible once this list itself has no duplicate key —
        // asserted here (debug-only: a real invariant of the dedup above, not a runtime check meant
        // to fire in release).
        debug_assert!(
            {
                let mut seen: Vec<(Vec<u32>, i32)> = Vec::with_capacity(candidates.len());
                candidates.iter().all(|c| {
                    let key = (c.morphemes.iter().map(|m| m.0).collect::<Vec<_>>(), c.root_index);
                    if seen.contains(&key) {
                        false
                    } else {
                        seen.push(key);
                        true
                    }
                })
            },
            "propose UNION peel produced a duplicate (morphemes, root_index) candidate for {word:?}"
        );

        (candidates, peel_used)
    }

    /// Batch entry point (perf pass, 2026-07-16): analyze every word in `words`, running PROPOSE
    /// sequentially (unchanged) but CONFIRM in parallel across words.
    ///
    /// **Why propose stays sequential:** [`FomaProposer::propose`] takes `&mut self` because it
    /// drives the single foma `ApplyHandle` this analyzer owns — that handle is deliberately
    /// built ONCE per grammar and reused (`analyzer.rs`'s own doc: `apply_init` deep-clones the
    /// whole compiled network, so rebuilding or cloning one per worker thread would cost far more
    /// than the propose stage itself saves). There is exactly one handle, so propose for N words
    /// cannot run on more than one thread without either a lock (serializing it anyway) or N
    /// redundant network clones — neither is a real win, so this stage stays a plain sequential
    /// loop over `words`, identical in content and cost to N calls to [`Self::analyze_word`]'s own
    /// propose half.
    ///
    /// **Why confirm parallelizes across words:** a tracer measured `hc_foma::confirm::confirm_batch`
    /// as the dominant cost of `analyze_word` on real corpora, and it is safe to run concurrently,
    /// one word per task: [`Morpher`] is `Sync` (its only interior-mutable state, an
    /// `AnalysisScope` behind a `RefCell`, is created fresh inside `parse_word_core_selected` per
    /// call — never a shared field), `RuleCache` and `hc_fst::Fst` hold no interior mutability at
    /// all, and `hc-parse`/`hc-rules`/`hc-fst` are all `#![forbid(unsafe_code)]` — so two words'
    /// confirm calls touch no shared mutable state. This runs on a DEDICATED rayon pool (not the
    /// global default pool) with large worker stacks
    /// ([`crate::emit::PROBE_STACK_BYTES`] — same size [`crate::preexpand::build_composites`] and
    /// [`crate::junctions::PhonologyProbe`] already use, and for the same reason: the analysis
    /// cascade `confirm_batch`'s pinned `parse_word_selected` recurses through can overflow
    /// rayon's default 2-8MB stacks on heavy words). `wasm32-unknown-unknown` cannot spawn OS
    /// threads (a rayon pool ctor would panic there), so that target keeps the plain sequential
    /// loop instead — matching this crate's existing convention
    /// (`junctions.rs`/`preexpand.rs`'s own `#[cfg(target_arch = "wasm32")]` fallback arms).
    ///
    /// Returns one `(`[`FomaOutcome`]`, elapsed)` pair per input word, in the SAME order as
    /// `words` — independent of dispatch/completion order or thread count
    /// (`par_iter().map(..).collect::<Vec<_>>()` over an `IndexedParallelIterator`, like
    /// [`crate::preexpand::build_composites`]'s own doc notes, preserves original input order),
    /// each `FomaOutcome` content-identical to calling [`Self::analyze_word`] on that word alone.
    /// `elapsed` is that word's own propose (stage 1) plus confirm (stage 2) wall time — timed
    /// separately per word in each stage (stage 2's timer runs inside that word's own parallel
    /// task, so it reflects real per-word cost, not a share of the pool's total wall time) and
    /// summed, mirroring `hc_parse::batch::BatchWordOutcome::elapsed`'s own per-word-not-per-batch
    /// convention.
    pub fn analyze_words(&mut self, words: &[String]) -> Vec<(FomaOutcome, Duration)> {
        if words.is_empty() {
            return Vec::new();
        }

        // Stage 1 (sequential): propose + peel per word.
        let per_word: Vec<(Vec<Candidate>, bool, Duration)> = words
            .iter()
            .map(|word| {
                let t0 = std::time::Instant::now();
                let (candidates, peel_used) = self.propose_candidates(word);
                (candidates, peel_used, t0.elapsed())
            })
            .collect();

        // Stage 2 (parallel across words): confirm — see doc above for the soundness argument.
        #[cfg(target_arch = "wasm32")]
        let buckets_per_word: Vec<(Vec<Vec<(WordAnalysis, String, String)>>, Duration)> = words
            .iter()
            .zip(per_word.iter())
            .map(|(word, (candidates, _, _))| {
                let t0 = std::time::Instant::now();
                let buckets =
                    confirm::confirm_batch(self.g, &self.owners, &self.morpher, candidates, word);
                (buckets, t0.elapsed())
            })
            .collect();

        #[cfg(not(target_arch = "wasm32"))]
        let buckets_per_word: Vec<(Vec<Vec<(WordAnalysis, String, String)>>, Duration)> = {
            let pool = rayon::ThreadPoolBuilder::new()
                .stack_size(crate::emit::PROBE_STACK_BYTES)
                .build()
                .expect("build FomaAnalyzer::analyze_words confirm rayon pool");
            pool.install(|| {
                words
                    .par_iter()
                    .zip(per_word.par_iter())
                    .map(|(word, (candidates, _, _))| {
                        let t0 = std::time::Instant::now();
                        let buckets = confirm::confirm_batch(
                            self.g,
                            &self.owners,
                            &self.morpher,
                            candidates,
                            word,
                        );
                        (buckets, t0.elapsed())
                    })
                    .collect()
            })
        };

        per_word
            .into_iter()
            .zip(buckets_per_word)
            .map(|((candidates, peel_used, propose_elapsed), (buckets, confirm_elapsed))| {
                let candidates_generated = candidates.len();
                let mut analyses = Vec::new();
                let mut structured = Vec::new();
                for bucket in buckets {
                    for (wa, join, surface) in bucket {
                        structured.push(wa);
                        analyses.push((join, surface));
                    }
                }
                let outcome = FomaOutcome {
                    confirmed: structured.len(),
                    analyses,
                    structured,
                    candidates_generated,
                    peel_used,
                };
                (outcome, propose_elapsed + confirm_elapsed)
            })
            .collect()
    }

    pub fn grammar(&self) -> &'g Grammar {
        self.g
    }

    /// Arm (or leave unarmed) the internal confirming [`Morpher`]'s `--word-timeout-ms` deadline
    /// (`hc_parse::Morpher::with_word_timeout`). `None` (also [`Self::new`]/[`Self::from_cached`]'s
    /// implicit default) is a complete no-op — behavior stays byte-identical to before this
    /// existed. NOTE: this only threads through the `Morpher` [`Self::new`]/[`Self::from_cached`]
    /// just built for THIS instance — [`Self::into_parts`]/[`Self::from_cached`]'s cached-pieces
    /// round trip does not persist the timeout (the `Morpher<'g>` it rebuilds is never one of the
    /// cached pieces, per that method's own doc), so a caller using that path must call this again
    /// after every `from_cached`.
    pub fn with_word_timeout(self, timeout: Option<Duration>) -> Self {
        FomaAnalyzer {
            morpher: self.morpher.with_word_timeout(timeout),
            ..self
        }
    }

    /// Rehydrate a `FomaAnalyzer` from previously-built OWNED pieces (a compiled [`FomaProposer`]
    /// — the expensive `emit`+foma-compile step — a [`ReduplicationPeeler`], and an owners map)
    /// plus a fresh borrow of `g` for this call. Plan P4 (`docs/fst-plan/foma-fst-plan.md`
    /// "`PanGlossGrammar::new` builds `FomaAnalyzer`"): a long-lived host (`hc-wasm`'s
    /// `PanGlossGrammar`) that also OWNS the `Grammar` these borrow from can't store a
    /// `FomaAnalyzer<'g>` as a sibling field of that same `Grammar` (that would be a
    /// self-referential struct) — but it CAN store the three owned pieces here and reconstruct a
    /// short-lived `FomaAnalyzer` from them plus `&self.grammar` for the duration of one call,
    /// exactly the way `Morpher<'g>` itself is never stored, only ever built fresh per call from
    /// an owned `&Grammar`. Pair with [`Self::into_parts`] to hand the (unchanged) owned pieces
    /// back to long-term storage once the call is done.
    pub fn from_cached(
        g: &'g Grammar,
        proposer: FomaProposer,
        peeler: ReduplicationPeeler,
        owners: Vec<Option<MorphemeOwner>>,
    ) -> Self {
        FomaAnalyzer {
            g,
            proposer,
            peeler,
            morpher: Morpher::new(g, usize::MAX),
            owners,
        }
    }

    /// The inverse of [`Self::from_cached`]: reclaim the three owned pieces this analyzer was
    /// built from (or built fresh in [`Self::new`]), discarding only the transient `Morpher<'g>`
    /// borrow (recreated for free on the next [`Self::from_cached`] call).
    pub fn into_parts(self) -> (FomaProposer, ReduplicationPeeler, Vec<Option<MorphemeOwner>>) {
        (self.proposer, self.peeler, self.owners)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hc_parse::ParseOptions;

    fn sample_path(name: &str) -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn load_sena() -> Option<Grammar> {
        let path = sample_path("sena-hc.xml")?;
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
    }

    /// A word with no proposed candidates at all returns an empty, non-panicking outcome.
    #[test]
    fn unknown_word_returns_empty_outcome() {
        let Some(g) = load_sena() else {
            eprintln!("skipping: sena-hc.xml not present on disk");
            return;
        };
        let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
        let outcome = analyzer.analyze_word("zzzqxxxnonsense");
        assert!(outcome.structured.is_empty());
        assert!(outcome.analyses.is_empty());
        assert_eq!(outcome.confirmed, 0);
        assert!(!outcome.peel_used);
    }

    /// Sanity: `mbali` confirms to a non-empty outcome whose size does not exceed
    /// `candidates_generated` (confirm only prunes, never invents).
    #[test]
    fn mbali_confirms_within_candidate_bound() {
        let Some(g) = load_sena() else {
            eprintln!("skipping: sena-hc.xml not present on disk");
            return;
        };
        let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
        let outcome = analyzer.analyze_word("mbali");
        assert!(!outcome.structured.is_empty());
        assert!(outcome.confirmed <= outcome.candidates_generated);
        let morpher = Morpher::new(&g, usize::MAX);
        let engine = morpher.parse_word_opts("mbali", &ParseOptions::default());
        assert_eq!(outcome.structured.len(), engine.structured.len());
    }

    /// Perf pass regression guard (2026-07-16): [`FomaAnalyzer::analyze_words`]'s parallel-confirm
    /// batch path must produce, per word, the exact same confirmed-analysis multiset as calling
    /// [`FomaAnalyzer::analyze_word`] on that word alone — compared via `hc_parse::result_signature`
    /// (order-independent over the analysis set), the same fingerprint the CLI's own TSV rows use.
    /// Includes a word with zero candidates (`"zzzqxxxnonsense"`) alongside real corpus words so the
    /// batch path's empty-outcome handling is covered too, not just the confirms-something case.
    #[test]
    fn analyze_words_matches_analyze_word_per_word() {
        let Some(g) = load_sena() else {
            eprintln!("skipping: sena-hc.xml not present on disk");
            return;
        };
        let words: Vec<String> = ["mbali", "zzzqxxxnonsense", "mbali"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
        let sequential: Vec<String> = words
            .iter()
            .map(|w| hc_parse::result_signature(&analyzer.analyze_word(w).analyses))
            .collect();

        let batched = analyzer.analyze_words(&words);
        assert_eq!(batched.len(), words.len());
        let parallel: Vec<String> = batched
            .iter()
            .map(|(outcome, _)| hc_parse::result_signature(&outcome.analyses))
            .collect();

        assert_eq!(
            sequential, parallel,
            "analyze_words must match analyze_word per word, in order"
        );
    }
}
