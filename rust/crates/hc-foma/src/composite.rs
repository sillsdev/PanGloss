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

    pub fn grammar(&self) -> &'g Grammar {
        self.g
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
}
