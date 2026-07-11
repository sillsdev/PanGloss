//! `replay.rs` (F5, HYBRID_FST_RUST_PLAN.md §8): verification by restricted re-analysis
//! (`FstReplay.cs`, `fst-oracle` branch) + the propose→verify loop (`VerifiedFstAnalyzer.cs`).
//!
//! ## No `MorpherPool` in this port (a recorded, approved deviation)
//! C#'s `MorpherPool` (`MorpherPool.cs`) exists **solely** because `Morpher.LexEntrySelector`/
//! `RuleSelector` are mutable instance properties: two threads verifying different words on a
//! shared `Morpher` would race setting them, so C# rents an independent `Morpher` per verification
//! and resets its selectors on return. F1 (`hc-parse::Morpher::parse_word_selected`,
//! `docs/fst-plan/HYBRID_FST_RUST_PLAN.md` §7.1 item 1) already deleted that mutable state — the
//! selectors are per-call parameters, not fields — an approved deviation recorded at F1 time
//! ("Prefer per-call parameters over C#'s mutable instance state ... thread-safe by construction").
//! `hc_parse::Morpher` has no interior mutability at all (confirmed: `hc-parse/src/batch.rs`'s own
//! module doc — "no interior mutability shared across calls" — already shares one `&Morpher` across
//! a rayon pool for the plain engine batch). So the pool's entire reason to exist is gone: a shared
//! `&Morpher` (or a `&VerifiedFstAnalyzer` wrapping one) already IS the concurrency-safe equivalent,
//! with no rent/return bookkeeping to get wrong. [`verify_words_parallel`] below is the concrete
//! demonstration — it drives real concurrent verification over `&Morpher`/`&VerifiedFstAnalyzer`
//! with no pool, no lock, and no per-thread clone.
//!
//! ## Quirk 8 — `FstReplay` keeps templates/strata/ALL phonological rules open
//! (`F1_QUIRK_AUDIT.md` #8, `FstReplay.cs:73-79`):
//! ```csharp
//! morpher.LexEntrySelector = e => e == root || extraRoots.Contains(e);
//! morpher.RuleSelector = r =>
//!     r is AffixTemplate || r is Stratum || r is IPhonologicalRule
//!     || rules.Contains(r) || (extraRoots.Count > 0 && r is CompoundingRule);
//! ```
//! Mapped onto `hc_rules::stratum::RuleRef`'s F1 subset (`Stratum`/`Template`/`MRule` — see that
//! type's own doc for why `PRule` does not exist yet):
//! - `RuleRef::Stratum`/`RuleRef::Template` → **always admit** (`AffixTemplate`/`Stratum` are
//!   unconditionally `true` in the C# predicate).
//! - Phonological rules → **never gated at all** on the Rust side (`hc-rules::rewrite`/
//!   `metathesis` never consult `rule_filter` — confirmed by grep, F1_QUIRK_AUDIT.md's own
//!   `RuleRef` doc). That is the correct Rust encoding of "always open": there is no admission
//!   check to pass, so phonological rules apply unconditionally, exactly matching
//!   `r is IPhonologicalRule` being unconditionally `true` in C#. No `RuleRef::PRule` variant is
//!   needed for this milestone (or ever, unless some FUTURE gate needs to actually reject a
//!   phonological rule, which quirk 8 says never happens for verify).
//! - `RuleRef::MRule(id)` → admit iff `id` is one of the candidate's own morphological rules
//!   (`rules.Contains(r)`), **or** `g.mrules[id]` is a `Compounding` rule and the candidate has at
//!   least one extra (non-head) root. F1 unified `AffixProcessRule`/`CompoundingRule`/
//!   `RealizationalAffixProcessRule` under one `MRuleId` space (`RuleRef::MRule`), so the
//!   Compounding clause must inspect `g.mrules[id]`'s variant directly — it cannot be a type tag
//!   the way C#'s `r is CompoundingRule` is. A `CompoundingRule` never owns a morpheme (see
//!   [`MorphemeOwner`]'s doc / `token.rs`'s `owning_morpheme`), so it can never appear in the
//!   candidate's own `rules` set — `extra_roots.is_empty()` being false is therefore the *sole*
//!   condition that ever opens a `Compounding` `MRuleId`, matching C# exactly (there,
//!   `rules.Contains(compoundingRule)` is always false for the same structural reason, so the
//!   `||`'s left side never fires for a `CompoundingRule` either).

use rustc_hash::FxHashSet as HashSet;

use hc_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef, MorphemeId};
use hc_parse::{Morpher, ParseOptions, WordAnalysis as EngineAnalysis};
use hc_rules::stratum::RuleRef;

use crate::trie::Trie;
use crate::walk::{self, WordAnalysis as Candidate};

/// Which grammar object owns a given [`MorphemeId`] — the reverse of `token.rs`'s
/// `owning_morpheme` (allomorph → morpheme). `confirm` needs this to classify each of a
/// candidate's non-root morphemes as either a compound's extra root (a `LexEntry`) or a
/// morphological rule (an `MRuleId`), exactly as C#'s `FstReplay.Confirm` does via
/// `morphemes[i] is LexEntry nonHeadRoot` / `morphemes[i] is IHCRule rule`
/// (`FstReplay.cs:45-60`). A `CompoundingRule` owns no morpheme at all (mirrors
/// `trie.rs::owning_morpheme_of_mrule`'s `unreachable!` for `Compounding` — a compounding rule is
/// never itself a token in a derivation, only its two roots are), so it can never be the `MRule`
/// variant here; that asymmetry is exactly what makes quirk 8's Compounding clause fire purely off
/// `extra_roots`, never off `rules`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MorphemeOwner {
    LexEntry(LexEntryId),
    MRule(MRuleId),
}

/// Build the `MorphemeId -> owner` reverse map once per grammar (`O(entries + mrules)`, reused
/// across every `confirm` call — the C# analog has no equivalent cost at all since `IMorpheme`
/// objects carry their own identity; this is the Rust-side one-time price of using plain integer
/// ids). `None` for a morpheme no `LexEntry`/`AffixProcessRule`/`RealizationalRule` owns (a
/// `CompoundingRule`'s "morpheme", or any id that should never appear inside a candidate's own
/// morpheme list at all).
pub fn build_morpheme_owners(g: &Grammar) -> Vec<Option<MorphemeOwner>> {
    let mut owners = vec![None; g.morphemes.len()];
    for (i, e) in g.entries.iter().enumerate() {
        owners[e.morpheme.0 as usize] = Some(MorphemeOwner::LexEntry(LexEntryId(i as u32)));
    }
    for (i, r) in g.mrules.iter().enumerate() {
        let morpheme = match r {
            MorphRuleDef::AffixProcess(def) => Some(def.morpheme),
            MorphRuleDef::Realizational(def) => Some(def.morpheme),
            MorphRuleDef::Compounding(_) => None,
        };
        if let Some(m) = morpheme {
            owners[m.0 as usize] = Some(MorphemeOwner::MRule(MRuleId(i as u32)));
        }
    }
    owners
}

/// C# `FstReplay.Confirm` (`FstReplay.cs:34-96`): confirm one FST [`Candidate`] by running the
/// engine's own restricted analysis of `word`, pinned to exactly this candidate's root(s) and
/// morphological rules (module doc's quirk-8 mapping). Returns the matched genuine engine
/// [`EngineAnalysis`] (carrying the real category/pos, unlike the proposer's bare candidate) or
/// `None` if the engine's restricted search does not reproduce it.
///
/// The verify `Morpher` must be built **uncapped** (`Morpher::new(g, usize::MAX)`) — C#'s verify
/// has no work budget of its own (feasibility report §10.7, an acknowledged open architectural
/// gap, not something to "fix" here by capping); a Rust-side cap could silently drop a result the
/// C# golden contains, which would look like a parity bug rather than the deliberate absence of a
/// budget it actually is.
///
/// Match test: C#'s `Signature` keys by per-morpheme *object identity* (a first-seen
/// `Dictionary<IMorpheme,int>`, `FstReplay.cs:98-113`) because `Morpheme.Id` is empty in these
/// grammars. The Rust equivalent is exact sequence equality of the SAME `Grammar`'s raw
/// [`MorphemeId`] values plus root index — both the candidate and the engine's
/// [`EngineAnalysis::morpheme_ids`] index the very same `g.morphemes`, so comparing the id
/// sequences directly *is* comparing by identity, with no dictionary needed.
pub fn confirm(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidate: &Candidate,
    word: &str,
) -> Option<EngineAnalysis> {
    confirm_checked(g, owners, morpher, candidate, word).0
}

/// [`confirm`], plus a second return value reporting whether the underlying restricted
/// `parse_word_selected` call hit `Morpher::with_word_timeout`'s wall-clock deadline
/// (`ParseOutcome::timed_out`) — added in F9 (`HYBRID_FST_RUST_PLAN.md` §8's full-corpus Sena
/// watchdog gate) so a full-corpus run can distinguish "this word's restricted verify did not
/// finish in the configured budget" (a pathological word to record explicitly, per the plan's own
/// instruction) from "this word's restricted verify finished and genuinely found no matching
/// analysis" (an ordinary, correct `None`). `confirm` itself is unchanged behaviorally — it just
/// discards the new second value — so every existing caller/gate is unaffected.
pub fn confirm_checked(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidate: &Candidate,
    word: &str,
) -> (Option<EngineAnalysis>, bool) {
    if candidate.root_index < 0 || candidate.root_index as usize >= candidate.morphemes.len() {
        return (None, false);
    }
    let root_index = candidate.root_index as usize;
    let root_entry = match owner_of(owners, candidate.morphemes[root_index]) {
        Some(MorphemeOwner::LexEntry(le)) => le,
        _ => return (None, false), // FstReplay.cs:38-41: the designated root must be a LexEntry.
    };

    let mut rules: HashSet<MRuleId> = HashSet::default();
    let mut extra_roots: HashSet<LexEntryId> = HashSet::default();
    for (i, &m) in candidate.morphemes.iter().enumerate() {
        if i == root_index {
            continue;
        }
        match owner_of(owners, m) {
            Some(MorphemeOwner::LexEntry(le)) => {
                extra_roots.insert(le);
            }
            Some(MorphemeOwner::MRule(mid)) => {
                rules.insert(mid);
            }
            None => return (None, false), // FstReplay.cs:56-59: neither a LexEntry nor an IHCRule -> null.
        }
    }

    let lex_entry_filter = |le: LexEntryId| le == root_entry || extra_roots.contains(&le);
    let rule_filter = |r: RuleRef| match r {
        RuleRef::Stratum(_) | RuleRef::Template(_) => true,
        RuleRef::MRule(id) => {
            rules.contains(&id)
                || (!extra_roots.is_empty()
                    && matches!(g.mrules[id.0 as usize], MorphRuleDef::Compounding(_)))
        }
    };

    let outcome = morpher.parse_word_selected(
        word,
        &ParseOptions::default(),
        Some(&lex_entry_filter),
        Some(&rule_filter),
    );
    let timed_out = outcome.timed_out;

    let found = outcome
        .structured
        .into_iter()
        .find(|wa| analyses_match(wa, candidate));
    (found, timed_out)
}

fn owner_of(owners: &[Option<MorphemeOwner>], m: MorphemeId) -> Option<MorphemeOwner> {
    owners.get(m.0 as usize).copied().flatten()
}

/// Byte-for-byte identity comparison (module doc): same root index, same morpheme sequence, both
/// indexing the same `Grammar`.
fn analyses_match(wa: &EngineAnalysis, candidate: &Candidate) -> bool {
    wa.root_morpheme_index == candidate.root_index
        && wa.morpheme_ids.len() == candidate.morphemes.len()
        && wa
            .morpheme_ids
            .iter()
            .zip(candidate.morphemes.iter())
            .all(|(&a, &b)| a == b.0)
}

/// C# `VerifiedFstAnalyzer.AnalyzeWord` (`VerifiedFstAnalyzer.cs:38-48`): propose (the bare walker,
/// F4) → verify (`confirm`, above) — **no dedup at this level**, matching C# exactly (a candidate
/// that verifies is yielded regardless of whether an earlier candidate already verified to the
/// identical engine analysis; `FstBatchCommand.Run` sorts+joins the raw signature list with no
/// `Distinct()` anywhere in the chain, so duplicate confirmations must survive into the golden
/// comparison, not be silently collapsed here).
pub struct VerifiedFstAnalyzer<'g> {
    g: &'g Grammar,
    trie: &'g Trie,
    morpher: &'g Morpher<'g>,
    owners: Vec<Option<MorphemeOwner>>,
    max_beam_work: i64,
}

impl<'g> VerifiedFstAnalyzer<'g> {
    /// `morpher` must be built uncapped (`Morpher::new(g, usize::MAX)`) — see [`confirm`]'s doc.
    pub fn new(
        g: &'g Grammar,
        trie: &'g Trie,
        morpher: &'g Morpher<'g>,
        max_beam_work: i64,
    ) -> Self {
        VerifiedFstAnalyzer {
            g,
            trie,
            morpher,
            owners: build_morpheme_owners(g),
            max_beam_work,
        }
    }

    /// Every verified analysis for `word`, in the proposer's own candidate emission order (F4's
    /// own determinism guarantee — see `walk.rs`'s module doc), each entry a genuine engine
    /// [`EngineAnalysis`]. Threads and reruns of this method never touch any shared mutable state
    /// (see this module's own doc) — safe to call from many threads at once over the same
    /// `&VerifiedFstAnalyzer`.
    pub fn analyze_word(&self, word: &str) -> Vec<EngineAnalysis> {
        let outcome = walk::analyze_word(self.g, self.trie, word, self.max_beam_work);
        outcome
            .analyses
            .iter()
            .filter_map(|c| confirm(self.g, &self.owners, self.morpher, c, word))
            .collect()
    }

    pub fn grammar(&self) -> &'g Grammar {
        self.g
    }
}

/// F0's frozen composite signature format (`HYBRID_FST_RUST_PLAN.md` §6.2, `MANIFEST.txt` §1):
/// `join("+", xml_key)` in morpheme order + `":"` + `root_index`.
pub fn signature(g: &Grammar, wa: &EngineAnalysis) -> String {
    let keys: Vec<&str> = wa
        .morpheme_ids
        .iter()
        .map(|&id| g.morphemes[id as usize].xml_key.as_str())
        .collect();
    format!("{}:{}", keys.join("+"), wa.root_morpheme_index)
}

/// C# `SignatureFormat.JoinSorted`: sorted ordinal (Rust `String`'s byte-wise `Ord` is
/// ordinal-equivalent for these ASCII grammar ids — same convention `canon.rs` already uses),
/// `;`-joined, `-` if empty. Deliberately **no dedup** — see [`VerifiedFstAnalyzer::analyze_word`]'s
/// doc.
pub fn join_sorted(mut sigs: Vec<String>) -> String {
    sigs.sort();
    if sigs.is_empty() {
        "-".to_string()
    } else {
        sigs.join(";")
    }
}

/// The one word→signature-line computation both the sequential and parallel gate paths share
/// (`{idx}\t{word}\tSTARTED\n{idx}\t{word}\tok\t{sig}`, C# `FstBatchCommand.Run`'s `--bare` shape —
/// status is unconditionally `"ok"` on this path, never `SKIPPED`: the bare proposer swallows a
/// segmentation failure internally and yields zero candidates, which verifies to the empty set,
/// which renders as `sig = "-"`, not as an exception the batch loop's `catch` ever sees — same
/// empirical finding `walk.rs`'s own `analyze_word` doc records for `--bare` candidates).
pub fn batch_lines(
    g: &Grammar,
    analyzer: &VerifiedFstAnalyzer,
    idx: usize,
    word: &str,
) -> [String; 2] {
    let started = format!("{idx}\t{word}\tSTARTED");
    let sigs: Vec<String> = analyzer
        .analyze_word(word)
        .iter()
        .map(|wa| signature(g, wa))
        .collect();
    let result = format!("{idx}\t{word}\tok\t{}", join_sorted(sigs));
    [started, result]
}

/// Verify every word in `words` against `analyzer`, using up to `threads` worker threads (`1` =
/// sequential, no thread pool spun up at all). Demonstrates the concurrency this module's doc
/// argues no longer needs a pool: every worker shares the SAME `&VerifiedFstAnalyzer` (in turn
/// sharing one `&Morpher`), no clone, no lock, no rent/return — `hc-parse::batch`'s own
/// already-established convention for the plain engine, extended here to the verify path. Returns
/// one two-line `[STARTED, result]` pair per word, in original `words` order (order is independent
/// of `threads`/scheduling — the thread-invariance gate's own requirement).
pub fn verify_words_parallel(
    g: &Grammar,
    analyzer: &VerifiedFstAnalyzer,
    words: &[String],
    threads: usize,
) -> Vec<[String; 2]> {
    if threads <= 1 {
        return words
            .iter()
            .enumerate()
            .map(|(i, w)| batch_lines(g, analyzer, i, w))
            .collect();
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("failed to build rayon thread pool");
    pool.install(|| {
        use rayon::prelude::*;
        words
            .par_iter()
            .enumerate()
            .map(|(i, w)| batch_lines(g, analyzer, i, w))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path(name: &str) -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn build_indonesian() -> Option<(Grammar, Trie)> {
        let path = sample_path("indonesian-hc.xml")?;
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        let g = hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
        let build_morpher = hc_parse::Morpher::new(&g, usize::MAX);
        let surface = crate::surface::SurfacePhonology::new(&g);
        let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
        Some((g, trie))
    }

    /// Sanity: a bare-root word ("ajar", entry25/entry26 homograph per the F4/F1 goldens) confirms
    /// to a non-empty, root-only signature with no morphological rules admitted spuriously.
    #[test]
    fn confirm_bare_root_word_verifies() {
        let Some((g, trie)) = build_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let morpher = Morpher::new(&g, usize::MAX);
        let analyzer = VerifiedFstAnalyzer::new(&g, &trie, &morpher, walk::DEFAULT_MAX_BEAM_WORK);
        let verified = analyzer.analyze_word("ajar");
        assert!(
            !verified.is_empty(),
            "\"ajar\" must verify to at least one analysis"
        );
        for wa in &verified {
            let sig = signature(&g, wa);
            assert!(
                sig.starts_with("entry25:0") || sig.starts_with("entry26:0"),
                "unexpected signature {sig}"
            );
        }
    }

    /// A candidate whose designated "root" position is not actually a `LexEntry` (e.g. an empty
    /// analysis, or a malformed root index) must confirm to `None`, never panic.
    #[test]
    fn confirm_rejects_out_of_range_root_index() {
        let Some((g, trie)) = build_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let morpher = Morpher::new(&g, usize::MAX);
        let owners = build_morpheme_owners(&g);
        let bogus = Candidate {
            morphemes: vec![],
            root_index: 0,
        };
        assert!(confirm(&g, &owners, &morpher, &bogus, "ajar").is_none());
        let _ = &trie; // trie unused by this particular assertion, kept for parity with the fixture builder
    }
}
