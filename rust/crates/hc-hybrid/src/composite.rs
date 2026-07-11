//! `composite.rs` (F6, HYBRID_FST_RUST_PLAN.md §8, **THE HEADLINE MILESTONE**) — port of C#
//! `CompositeProposer` (`CompositeProposer.cs`, `fst-oracle` branch): unions the candidate streams of
//! every proposer, deduped by signature (first-proposer-wins).
//!
//! ## Fixed proposer order (plan §4.2 — "the fixed proposer order matters and must match C# exactly")
//! Read directly from `CompositeProposer.ForLanguage` (`CompositeProposer.cs:79-105`), NOT assumed
//! from the plan's own paraphrase (which lists `FST, [forwardSynthesis], redup, infix, composed,
//! lockstep/chain` — confirmed correct by direct reading, but note the actual C# construction is
//! FST first (the composite's own ctor prepends it unconditionally), then a `generators` list built
//! as `[redup, infix, composed, phonology]` with `forwardSynthesis` (if enabled) `Insert(0, ...)`ed
//! into THAT list — i.e. forwardSynthesis lands between FST and redup, exactly where the plan says):
//! 1. `FstTemplateAnalyzer` (the bare walker, [`crate::walk::analyze_word`])
//! 2. `ForwardSynthesisProposer` (opt-in via `forward_synthesis`; DEFERRED STUB — see `proposers.rs`'s
//!    module doc for why)
//! 3. `ReduplicationProposer` ([`crate::proposers::ReduplicationProposer`], built for real)
//! 4. `InfixProposer` ([`crate::proposers::InfixProposer`], built for real)
//! 5. `ComposedPhonologyProposer` (DEFERRED STUB)
//! 6. `LockstepPhonologyProposer` (v1, DEFAULT phonology path; DEFERRED STUB) OR, when
//!    `useChainPhonology` is on ([`CompositeAnalyzer::with_chain_phonology`], F7),
//!    [`crate::proposers::ChainPhonologyProposer`] instead — the two are mutually exclusive at this
//!    one order position, matching C#'s `CompositeProposer.ForLanguage` (`useChainPhonology ?
//!    new ChainPhonologyProposer(...) : new LockstepPhonologyProposer(...)`, both landing at the
//!    SAME slot in the `generators` list).
//!
//! ## Dedup (plan §4.2 / `CompositeProposer.AnalyzeWord`, `:111-125`)
//! C# dedups by `Signature(candidate, ids)`, a PER-CALL `Dictionary<IMorpheme,int>` assigning
//! sequential ids to morphemes by first-seen OBJECT IDENTITY, then formats `join("+", ids) + ":" +
//! rootIndex`. Two candidates get the same signature iff their morpheme sequences are identical BY
//! IDENTITY (order-sensitive) and their root index matches — which is exactly what comparing this
//! port's raw `(Vec<MorphemeId>, root_index)` tuples directly achieves, with no per-call dictionary
//! needed at all (a `MorphemeId` already IS a stable identity within one `Grammar`). This is a
//! representation simplification, not a behavior change — see `repl ay.rs`'s identical argument for
//! why comparing raw ids suffices in place of C#'s object-identity dictionary.
//!
//! ## Corpus-empirical scope decision
//! See `proposers.rs`'s module doc: `ComposedPhonologyProposer`/`LockstepPhonologyProposer`/
//! `ForwardSynthesisProposer` are wired at their correct order position but always yield zero
//! candidates (an oracle-verified-safe stub for the corpus-level gates; NOT a substitute for their
//! real logic, which remains open work — see this crate's F6 commit message for the exact deferred
//! scope).

use rustc_hash::FxHashSet as HashSet;

use hc_grammar::model::{Grammar, MorphemeId};
use hc_parse::{Morpher, WordAnalysis as EngineAnalysis};

use crate::proposers::{ChainPhonologyProposer, InfixProposer, ReduplicationProposer};
use crate::replay::{self, MorphemeOwner};
use crate::surface::SurfacePhonology;
use crate::token::MorphOp;
use crate::trie::Trie;
use crate::walk::{self, WordAnalysis as Candidate};

/// One candidate as it leaves the composite, tagged with the name of the proposer that FIRST
/// produced it (C#'s `fst-candidates` dump column 3 — see `HYBRID_FST_RUST_PLAN.md` §6.1). Proposer
/// names are the C# CLASS names verbatim (the golden format's own vocabulary).
pub struct LabeledCandidate {
    pub proposer: &'static str,
    pub candidate: Candidate,
}

/// Dedup key: order-sensitive morpheme-identity sequence + root index (see module doc).
fn dedup_key(c: &Candidate) -> (Vec<u32>, i32) {
    (c.morphemes.iter().map(|m| m.0).collect(), c.root_index)
}

pub struct CompositeAnalyzer<'g> {
    g: &'g Grammar,
    trie: &'g Trie,
    max_beam_work: i64,
    redup: ReduplicationProposer,
    infix: InfixProposer,
    /// Opt-in flag (`ForwardSynthesisProposer`, default OFF, matching C#'s own default). Wired
    /// structurally (a caller CAN turn it on) but the proposer itself is a deferred stub — see
    /// `proposers.rs`'s module doc — so enabling it changes nothing observable yet; kept as a real
    /// knob (not silently dropped) so the composite's public shape already matches the plan's §3.6
    /// "knob parity" requirement once the real proposer lands.
    forward_synthesis: bool,
    /// `useChainPhonology` (F7): `None` (default, matching C#'s own default) keeps position 6 as the
    /// `LockstepPhonologyProposer` stub; `Some` (via [`CompositeAnalyzer::with_chain_phonology`])
    /// swaps in the real [`ChainPhonologyProposer`] instead.
    chain: Option<ChainPhonologyProposer>,
}

impl<'g> CompositeAnalyzer<'g> {
    /// `surface` must be the SAME [`SurfacePhonology`] instance the trie was built from (matches C#
    /// `CompositeProposer.ForLanguage`'s single shared `SurfacePhonology`/`Morpher` per language
    /// instance — memoization reuse, not merely convention).
    pub fn new(
        g: &'g Grammar,
        trie: &'g Trie,
        surface: &SurfacePhonology<'g>,
        max_beam_work: i64,
        forward_synthesis: bool,
    ) -> Self {
        CompositeAnalyzer {
            g,
            trie,
            max_beam_work,
            redup: ReduplicationProposer::new(g),
            infix: InfixProposer::new(g, surface),
            forward_synthesis,
            chain: None,
        }
    }

    /// Builder: enable `useChainPhonology` (F7) — builds [`ChainPhonologyProposer`]'s own
    /// underlying-only trie (see that type's doc) and swaps it in for position 6, replacing the
    /// `LockstepPhonologyProposer` stub. `surface`/`morpher` here are BUILD-time only (used once to
    /// construct the chain proposer's own trie/compiled rule chain, then discarded — mirrors every
    /// other build-time-only `Morpher` use in this crate, e.g. `Trie::build`'s own `morpher` param).
    pub fn with_chain_phonology(
        mut self,
        g: &'g Grammar,
        surface: &SurfacePhonology<'g>,
        morpher: &Morpher,
        max_states: usize,
        deriv_depth: usize,
    ) -> Self {
        self.chain = Some(ChainPhonologyProposer::new(g, surface, morpher, max_states, deriv_depth, self.max_beam_work));
        self
    }

    /// C# `CompositeProposer.AnalyzeWord` (`:111-125`): the deduped, labeled candidate stream, in
    /// FIXED proposer order (module doc). This is the `fst-candidates --composite`-equivalent
    /// output (pre-verify).
    pub fn analyze_word_labeled(&self, word: &str) -> Vec<LabeledCandidate> {
        let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::default();
        let mut out: Vec<LabeledCandidate> = Vec::new();

        let extend = |proposer: &'static str,
                      candidates: Vec<Candidate>,
                      out: &mut Vec<LabeledCandidate>,
                      seen: &mut HashSet<(Vec<u32>, i32)>| {
            for candidate in candidates {
                if seen.insert(dedup_key(&candidate)) {
                    out.push(LabeledCandidate {
                        proposer,
                        candidate,
                    });
                }
            }
        };

        // 1. FstTemplateAnalyzer (bare walker).
        extend(
            "FstTemplateAnalyzer",
            walk::analyze_word(self.g, self.trie, word, self.max_beam_work).analyses,
            &mut out,
            &mut seen,
        );
        // 2. ForwardSynthesisProposer (opt-in; deferred stub -- always empty regardless of the flag,
        //    see this module's doc and `proposers.rs`'s).
        if self.forward_synthesis {
            extend("ForwardSynthesisProposer", Vec::new(), &mut out, &mut seen);
        }
        // 3. ReduplicationProposer (real).
        extend(
            "ReduplicationProposer",
            self.redup
                .analyze_word(self.g, self.trie, word, self.max_beam_work),
            &mut out,
            &mut seen,
        );
        // 4. InfixProposer (real).
        extend(
            "InfixProposer",
            self.infix
                .analyze_word(self.g, self.trie, word, self.max_beam_work),
            &mut out,
            &mut seen,
        );
        // 5. ComposedPhonologyProposer (deferred stub).
        extend("ComposedPhonologyProposer", Vec::new(), &mut out, &mut seen);
        // 6. LockstepPhonologyProposer (v1 default; deferred stub) OR ChainPhonologyProposer
        //    (useChainPhonology opt-in, F7) -- mutually exclusive at this one slot, see module doc.
        match &self.chain {
            Some(chain) => extend("ChainPhonologyProposer", chain.analyze_word(self.g, word), &mut out, &mut seen),
            None => extend("LockstepPhonologyProposer", Vec::new(), &mut out, &mut seen),
        }

        out
    }

    /// The unlabeled candidate stream (verify doesn't care which proposer contributed a candidate).
    pub fn analyze_word(&self, word: &str) -> Vec<Candidate> {
        self.analyze_word_labeled(word)
            .into_iter()
            .map(|lc| lc.candidate)
            .collect()
    }

    /// C# `CoversAllConstructs` (`CompositeProposer.cs:45,109`): every `MorphOp` the bare FST proposer
    /// left uncovered is claimed by some sibling generator's `CoveredOps`. `ComposedPhonologyProposer`/
    /// `LockstepPhonologyProposer`/`ForwardSynthesisProposer` all report an EMPTY `CoveredOps` in C#
    /// (phonology completeness is not a per-construct `MorphOp`), so their deferred-stub status here
    /// changes nothing about this diagnostic's correctness.
    pub fn covers_all_constructs(&self) -> bool {
        let mut covered: HashSet<MorphOp> = HashSet::default();
        for op in ReduplicationProposer::COVERED_OPS {
            covered.insert(op);
        }
        for op in InfixProposer::COVERED_OPS {
            covered.insert(op);
        }
        self.trie
            .uncovered_ops()
            .iter()
            .all(|op| covered.contains(op))
    }

    /// Propose (this composite) -> verify ([`replay::confirm`]) -- C# `VerifiedFstAnalyzer.AnalyzeWord`
    /// wired to a `CompositeProposer` instead of the bare `FstTemplateAnalyzer`. No dedup at THIS
    /// level either (matches `replay::VerifiedFstAnalyzer::analyze_word`'s own doc: a candidate that
    /// verifies is yielded regardless of whether an earlier one already verified to the identical
    /// engine analysis -- the composite's OWN dedup already ran during `analyze_word`, so this can
    /// never actually re-verify a duplicate candidate object, but it never additionally COLLAPSES two
    /// distinct verified engine analyses either, matching C# exactly).
    pub fn analyze_word_verified(
        &self,
        morpher: &Morpher,
        owners: &[Option<MorphemeOwner>],
        word: &str,
    ) -> Vec<EngineAnalysis> {
        self.analyze_word(word)
            .iter()
            .filter_map(|c| replay::confirm(self.g, owners, morpher, c, word))
            .collect()
    }
}

/// C# `fst-batch`'s per-word dump shape, sourced from the COMPOSITE instead of the bare proposer
/// (`{idx}\t{word}\tSTARTED` then `{idx}\t{word}\tok\t{sig}`) -- mirrors `replay::batch_lines`
/// exactly, see that function's doc for the `status` field's own "always ok, never SKIPPED" note.
pub fn batch_lines(
    g: &Grammar,
    composite: &CompositeAnalyzer,
    morpher: &Morpher,
    owners: &[Option<MorphemeOwner>],
    idx: usize,
    word: &str,
) -> [String; 2] {
    let started = format!("{idx}\t{word}\tSTARTED");
    let sigs: Vec<String> = composite
        .analyze_word_verified(morpher, owners, word)
        .iter()
        .map(|wa| replay::signature(g, wa))
        .collect();
    let result = format!("{idx}\t{word}\tok\t{}", replay::join_sorted(sigs));
    [started, result]
}

/// C# `fst-candidates`'s per-word-per-candidate dump shape: `{idx}\t{word}\t{proposer}\t{signature}`,
/// one line per surviving candidate, in composite emission order (NOT sorted -- the golden itself is
/// emission-order, e.g. `mengamat-amati`'s 9 `ReduplicationProposer` lines are grouped by which
/// suffix-peel matched first, not alphabetically).
pub fn candidate_lines(
    g: &Grammar,
    composite: &CompositeAnalyzer,
    idx: usize,
    word: &str,
) -> Vec<String> {
    composite
        .analyze_word_labeled(word)
        .into_iter()
        .map(|lc| {
            let sig = candidate_signature(g, &lc.candidate);
            format!("{idx}\t{word}\t{}\t{sig}", lc.proposer)
        })
        .collect()
}

/// The frozen composite signature format (§6.2) applied directly to a pre-verify [`Candidate`]
/// (candidate `morphemes`/`root_index` already index `g.morphemes` exactly like a verified
/// [`EngineAnalysis`] does -- see `replay::signature`'s identical construction).
fn candidate_signature(g: &Grammar, c: &Candidate) -> String {
    let keys: Vec<&str> = c
        .morphemes
        .iter()
        .map(|&MorphemeId(id)| g.morphemes[id as usize].xml_key.as_str())
        .collect();
    format!("{}:{}", keys.join("+"), c.root_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path(name: &str) -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    /// Smoke test: the composite's redup-driven "membagi-bagi" candidate survives dedup and verifies.
    #[test]
    fn composite_covers_known_reduplicated_corpus_word() {
        let Some(path) = sample_path("indonesian-hc.xml") else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        let g = hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
        let build_morpher = hc_parse::Morpher::new(&g, usize::MAX);
        let surface = SurfacePhonology::new(&g);
        let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
        let composite =
            CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);

        let labeled = composite.analyze_word_labeled("membagi-bagi");
        assert!(
            labeled
                .iter()
                .any(|lc| lc.proposer == "ReduplicationProposer"),
            "expected at least one ReduplicationProposer candidate for membagi-bagi"
        );

        let verify_morpher = hc_parse::Morpher::new(&g, usize::MAX);
        let owners = replay::build_morpheme_owners(&g);
        let verified = composite.analyze_word_verified(&verify_morpher, &owners, "membagi-bagi");
        assert!(
            !verified.is_empty(),
            "membagi-bagi must verify to at least one analysis"
        );
    }
}
