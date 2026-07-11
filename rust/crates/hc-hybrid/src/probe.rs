//! `probe.rs` (F8, HYBRID_FST_RUST_PLAN.md §8) — port of C# `FstCoverageProbe.cs`/`ProbeReport`/
//! `CoverageDiff`: run a wordlist through the FULL fast-path composite (bare FST +
//! `ReduplicationProposer` + `InfixProposer` + `ComposedPhonologyProposer` + the default v1
//! `LockstepPhonologyProposer`, or the opt-in `ChainPhonologyProposer` chain) and report coverage,
//! or diff coverage between two grammar versions — the "did my grammar edit make parsing better or
//! worse?" tool, never a soundness/completeness claim (every reported "parsed" word is confirmed
//! by real restricted re-analysis, [`crate::replay::confirm`]; an "unparsed" word may still be a
//! valid word the real engine can analyze — see `FstCoverageProbe.cs`'s own class doc, ported
//! verbatim in spirit above).
//!
//! ## Rust-idiom deviation: no `FstCoverageProbe` struct (approved per plan §7.1's convention)
//! C#'s `FstCoverageProbe.ForLanguage(language, ...)` returns a long-lived object wrapping a
//! `VerifiedFstAnalyzer` over a `CompositeProposer`, then `.Probe(words)` is called on it
//! (possibly repeatedly, accumulating `BeamOverflowCount` across calls — see [`ForLanguage`]'s own
//! doc). [`crate::composite::CompositeAnalyzer`] borrows its `Trie`/`SurfacePhonology` rather than
//! owning them (this crate's established pattern throughout — see `composite.rs`'s own module
//! doc), which would make an owning `CoverageProbe` struct self-referential. Since nothing in this
//! milestone's gate (the six `FstCoverageProbeTests` methods) needs cross-call overflow
//! accumulation, [`for_language`]/[`for_language_with_options`] instead build everything FRESH
//! per call (grammar → surface → trie → composite → probe, exactly matching
//! `FstCoverageProbe.ForLanguage(...).Probe(...)` chained as ONE call) — a representation
//! simplification, not a behavior change, matching the same argument `composite.rs`/`replay.rs`
//! already make for per-call parameters over C#'s mutable instance state.
//!
//! ## `ComposedPhonologyProposer` is UNCONDITIONAL here (unlike `CompositeAnalyzer`'s own default)
//! `CompositeAnalyzer::new` defaults position 5 (`ComposedPhonologyProposer`) to an empty stub
//! unless a caller opts in via `.with_composed_phonology(...)` (a Rust-port-only convenience
//! default, see `composite.rs`'s field doc). C#'s `FstCoverageProbe.ForLanguage` builds it for
//! REAL unconditionally (`FstCoverageProbe.cs:93`, no knob at all) — this module always calls
//! `.with_composed_phonology(g)` to match.
//!
//! ## The beam-overflow blind spot (C# `FstCoverageProbe.cs`'s own documented caveat, `:199-217`)
//! `BeamOverflows`/`LastBeamOverflowWord` only reflect the bare-walk `FstTemplateAnalyzer` instance
//! the probe itself owns (shared with `ComposedPhonologyProposer`) — NOT whichever chain/lockstep
//! phonology proposer's own PRIVATE trie/walk. `walk.rs`'s `analyze_word` doc names this exact gap
//! as "a future `fst-stats`-style diagnostic, F8's job": since `analyze_word` is a stateless
//! function (no shared instance to read an accumulated counter off), this module recomputes the
//! SAME bare walk once more per word purely to read its `overflowed` flag — cheap, and exactly the
//! bare-walk instance C# tracks (position 1 of the composite), so the blind spot is reproduced
//! faithfully rather than accidentally fixed.

use rustc_hash::FxHashSet as HashSet;

use hc_grammar::model::Grammar;
use hc_parse::Morpher;

use crate::compiler::{self, RuleInverseTier};
use crate::compiler_v1;
use crate::composite::CompositeAnalyzer;
use crate::replay;
use crate::surface::SurfacePhonology;
use crate::token::MorphOp;
use crate::trie::Trie;
use crate::walk::{self, DEFAULT_MAX_BEAM_WORK};

/// C# `FstTemplateAnalyzer`/`ChainPhonologyProposer` ctor defaults (`maxStates`, `derivDepth`) —
/// same constants `f3_trie_gate.rs`/`composite.rs`'s own tests already hardcode for "the default
/// two-arg ctor" shape.
const MAX_STATES: usize = 1_000_000;
const DERIV_DEPTH: usize = 2;

/// C# `ProbeReport` (`FstCoverageProbe.cs:173-265`).
pub struct ProbeReport {
    pub total_words: usize,
    pub parsed_words: usize,
    pub total_analyses: usize,
    pub unparsed_words: Vec<String>,
    /// C# `CoversAllConstructs` (`CompositeProposer.CoversAllConstructs`, computed via the
    /// composite, NOT equal to `uncovered_constructs.is_empty()` in general -- see that field's
    /// own doc: siblings can cover a `MorphOp` the bare FST left uncovered).
    pub covers_all_constructs: bool,
    /// C# `UncoveredConstructs` = `fst.UncoveredOps` VERBATIM (the bare FST proposer's own raw
    /// list, NOT filtered by what siblings cover -- that filtering is what `covers_all_constructs`
    /// computes separately).
    pub uncovered_constructs: Vec<MorphOp>,
    pub unsupported_phonology_rule_count: usize,
    pub beam_overflows: usize,
    pub last_beam_overflow_word: Option<String>,
}

impl ProbeReport {
    pub fn coverage_rate(&self) -> f64 {
        if self.total_words == 0 {
            0.0
        } else {
            self.parsed_words as f64 / self.total_words as f64
        }
    }

    pub fn average_analyses_per_parsed_word(&self) -> f64 {
        if self.parsed_words == 0 {
            0.0
        } else {
            self.total_analyses as f64 / self.parsed_words as f64
        }
    }
}

impl std::fmt::Display for ProbeReport {
    /// C# `ProbeReport.ToString` (`:254-264`) — not gated byte-identical anywhere (no golden
    /// prints it), ported for fidelity/future CLI use.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{} words parsed ({:.1}%), {:.2} analyses/parsed word, {:.0} ms",
            self.parsed_words,
            self.total_words,
            self.coverage_rate() * 100.0,
            self.average_analyses_per_parsed_word(),
            0.0, // C#'s Elapsed.TotalMilliseconds -- this port doesn't time the probe (no gate needs it).
        )?;
        if !self.covers_all_constructs {
            write!(f, ", uncovered constructs: [{}]", {
                let names: Vec<String> = self.uncovered_constructs.iter().map(|op| format!("{op:?}")).collect();
                names.join(",")
            })?;
        }
        if self.unsupported_phonology_rule_count > 0 {
            write!(f, ", {} unsupported phonology rule(s)", self.unsupported_phonology_rule_count)?;
        }
        if self.beam_overflows > 0 {
            write!(
                f,
                ", {} beam-cap overflow(s) (last: {})",
                self.beam_overflows,
                self.last_beam_overflow_word.as_deref().unwrap_or("")
            )?;
        }
        Ok(())
    }
}

/// C# `CoverageDiff` (`FstCoverageProbe.cs:270-298`).
pub struct CoverageDiff {
    pub before: ProbeReport,
    pub after: ProbeReport,
    /// Unparsed under `before`, parsed under `after`, sorted ordinal.
    pub gained: Vec<String>,
    /// Parsed under `before`, unparsed under `after`, sorted ordinal.
    pub lost: Vec<String>,
}

/// Build the full composite fast path for `g` (matching `FstCoverageProbe.ForLanguage`'s
/// unconditional wiring: bare FST + Redup + Infix + ComposedPhonology, always; the phonology slot
/// is Lockstep unless `use_chain_phonology`) and probe `words` — this is
/// `FstCoverageProbe.ForLanguage(language, forwardSynthesis, useChainPhonology).Probe(words)`
/// collapsed into one call (see this module's doc for why no owning struct sits between the two).
pub fn for_language_with_options(
    g: &Grammar,
    words: &[&str],
    forward_synthesis: bool,
    use_chain_phonology: bool,
) -> ProbeReport {
    let build_morpher = Morpher::new(g, usize::MAX);
    let surface = SurfacePhonology::new(g);
    let trie = Trie::build(g, &surface, &build_morpher, MAX_STATES, DERIV_DEPTH, true);
    let verify_morpher = Morpher::new(g, usize::MAX);

    let mut composite = CompositeAnalyzer::new(g, &trie, &surface, DEFAULT_MAX_BEAM_WORK, forward_synthesis)
        .with_composed_phonology(g);
    composite = if use_chain_phonology {
        composite.with_chain_phonology(g, &surface, &build_morpher, MAX_STATES, DERIV_DEPTH)
    } else {
        composite.with_lockstep_phonology(g, &surface, &build_morpher, MAX_STATES, DERIV_DEPTH)
    };

    let owners = replay::build_morpheme_owners(g);

    let mut total = 0usize;
    let mut parsed = 0usize;
    let mut total_analyses = 0usize;
    let mut unparsed: Vec<String> = Vec::new();
    let mut beam_overflows = 0usize;
    let mut last_beam_overflow_word: Option<String> = None;

    for &word in words {
        total += 1;

        // See module doc: recompute the bare walk once more, purely for its `overflowed` flag --
        // the same bare-walk instance C#'s probe tracks (position 1 of the composite).
        let bare = walk::analyze_word(g, &trie, word, DEFAULT_MAX_BEAM_WORK);
        if bare.overflowed {
            beam_overflows += 1;
            last_beam_overflow_word = Some(word.to_string());
        }

        let verified = composite.analyze_word_verified(&verify_morpher, &owners, word);
        if verified.is_empty() {
            unparsed.push(word.to_string());
        } else {
            parsed += 1;
            total_analyses += verified.len();
        }
    }

    let covers_all_constructs = composite.covers_all_constructs();
    let uncovered_constructs = trie.uncovered_ops();
    let unsupported_phonology_rule_count = if use_chain_phonology {
        // C# `ChainPhonologyProposer.UnsupportedRuleCount` (`ChainPhonologyProposer.cs:70`):
        // count of `IdentitySkip`-tier compiled rules.
        compiler::compile_default(g).iter().filter(|c| c.tier == RuleInverseTier::IdentitySkip).count()
    } else {
        // C# `LockstepPhonologyProposer.UnsupportedRuleCount` (`LockstepPhonologyProposer.cs:35`):
        // the v1 compiler's own unsupported-subrule count.
        compiler_v1::compile(g).unsupported_rule_count
    };

    ProbeReport {
        total_words: total,
        parsed_words: parsed,
        total_analyses,
        unparsed_words: unparsed,
        covers_all_constructs,
        uncovered_constructs,
        unsupported_phonology_rule_count,
        beam_overflows,
        last_beam_overflow_word,
    }
}

/// C# `FstCoverageProbe.ForLanguage(language).Probe(words)` with default knobs (`forwardSynthesis
/// = false`, `useChainPhonology = false`) -- the shape every `FstCoverageProbeTests` method uses.
pub fn for_language(g: &Grammar, words: &[&str]) -> ProbeReport {
    for_language_with_options(g, words, false, false)
}

/// C# `FstCoverageProbe.CompareGrammars` (`:158-168`): probe `before`/`after` independently over
/// the SAME corpus (default knobs, no `forwardSynthesis`/`useChainPhonology` parameters -- C#'s own
/// static method signature has none either) and diff which words flipped parse status.
pub fn compare_grammars(before: &Grammar, after: &Grammar, words: &[&str]) -> CoverageDiff {
    let before_report = for_language(before, words);
    let after_report = for_language(after, words);

    let before_unparsed: HashSet<&str> = before_report.unparsed_words.iter().map(String::as_str).collect();
    let after_unparsed: HashSet<&str> = after_report.unparsed_words.iter().map(String::as_str).collect();

    let mut gained: Vec<String> = before_unparsed
        .iter()
        .filter(|w| !after_unparsed.contains(*w))
        .map(|s| s.to_string())
        .collect();
    gained.sort();

    let mut lost: Vec<String> = after_unparsed
        .iter()
        .filter(|w| !before_unparsed.contains(*w))
        .map(|s| s.to_string())
        .collect();
    lost.sort();

    CoverageDiff {
        before: before_report,
        after: after_report,
        gained,
        lost,
    }
}

#[cfg(test)]
mod tests {
    //! Port of `FstCoverageProbeTests.cs` (all six methods -- the plan's F8 gate names three
    //! "edit-loop" tests specifically, but the other three are the only coverage this crate has
    //! for `compare_grammars`/the base coverage-rate contract/the soundness "never over-generate"
    //! claim, so all six are ported, per an independent Fable review of this milestone's draft).
    //!
    //! Toy fixtures (`tests/fixtures/fst-advisor-toys/FstCoverageProbeToyGrammar*.xml`) are
    //! HAND-AUTHORED (not a C# `XmlLanguageWriter` export), following the established
    //! `F6ProposersToyGrammar.xml` precedent (HYBRID_FST_RUST_PLAN.md §9's escape hatch): two bare
    //! roots "sag"/"dat" (mirroring `HermitCrabTestBase`'s own shared roots the C# tests reuse),
    //! category V, in an otherwise rule-free base grammar -- each "after" variant is a byte-for-byte
    //! copy of the base with exactly ONE rule block added (plus the one stratum attribute wiring
    //! it in), so the `after == before + 1` arithmetic the C# tests assert is provably isolated to
    //! that one edit, not confounded by incidental fixture drift between files.

    use super::*;

    const BASE: &str = include_str!("../tests/fixtures/fst-advisor-toys/FstCoverageProbeToyGrammar.xml");
    const AFTER_SUFFIX: &str =
        include_str!("../tests/fixtures/fst-advisor-toys/FstCoverageProbeToyGrammar.AfterSuffix.xml");
    const AFTER_PHONOLOGY: &str =
        include_str!("../tests/fixtures/fst-advisor-toys/FstCoverageProbeToyGrammar.AfterPhonology.xml");
    const AFTER_REDUP: &str =
        include_str!("../tests/fixtures/fst-advisor-toys/FstCoverageProbeToyGrammar.AfterRedup.xml");

    fn load(xml: &str) -> Grammar {
        hc_grammar::load(xml).unwrap_or_else(|e| panic!("failed to load toy grammar: {e}"))
    }

    /// Port of `Probe_ReportsCoverageAndUnparsedWords`.
    #[test]
    fn probe_reports_coverage_and_unparsed_words() {
        let g = load(BASE);
        let corpus = ["sag", "dat", "zzz"]; // two bare roots, one non-word
        let report = for_language(&g, &corpus);

        assert_eq!(report.total_words, 3);
        assert_eq!(report.parsed_words, 2);
        assert_eq!(report.unparsed_words, vec!["zzz".to_string()]);
        assert!((report.coverage_rate() - 2.0 / 3.0).abs() < 0.0001);
    }

    /// Port of `Probe_NeverReportsANonWordAsParsed` (soundness contract).
    #[test]
    fn probe_never_reports_a_non_word_as_parsed() {
        let g = load(BASE);
        let report = for_language(&g, &["sagg"]);

        assert_eq!(report.parsed_words, 0);
        assert_eq!(report.unparsed_words, vec!["sagg".to_string()]);
    }

    /// Port of `CompareGrammars_SameGrammarTwice_NoGainedOrLost`.
    #[test]
    fn compare_grammars_same_grammar_twice_no_gained_or_lost() {
        let g = load(BASE);
        let corpus = ["sag", "dat", "zzz"];
        let diff = compare_grammars(&g, &g, &corpus);

        assert!(diff.gained.is_empty());
        assert!(diff.lost.is_empty());
        assert_eq!(diff.before.parsed_words, diff.after.parsed_words);
    }

    /// Port of `Probe_DetectsGainedCoverage_AfterAddingSuffixRule` (the affix-rule edit class of
    /// FST_FAST_PATH_PLAN.md's Phase 5.4 edit-loop promise).
    #[test]
    fn probe_detects_gained_coverage_after_adding_suffix_rule() {
        let before_g = load(BASE);
        let corpus = ["sag", "sags", "dat"];
        let before = for_language(&before_g, &corpus);
        assert!(
            before.unparsed_words.contains(&"sags".to_string()),
            "precondition: sags not yet coverable"
        );

        let after_g = load(AFTER_SUFFIX);
        let after = for_language(&after_g, &corpus);
        assert!(!after.unparsed_words.contains(&"sags".to_string()));
        assert_eq!(after.parsed_words, before.parsed_words + 1);
    }

    /// Port of `Probe_DetectsGainedCoverage_AfterAddingPhonologicalRule`: an unconditional t->d
    /// rule means bare root "dat" now surfaces only as "dad" (see the toy grammar's own comment
    /// for why "dat" itself is deliberately excluded from this corpus, mirroring the C# original).
    #[test]
    fn probe_detects_gained_coverage_after_adding_phonological_rule() {
        let before_g = load(BASE);
        let corpus = ["sag", "dad"];
        let before = for_language(&before_g, &corpus);
        assert!(
            before.unparsed_words.contains(&"dad".to_string()),
            "precondition: dad not yet coverable"
        );

        let after_g = load(AFTER_PHONOLOGY);
        let after = for_language(&after_g, &corpus);
        assert!(!after.unparsed_words.contains(&"dad".to_string()));
        assert_eq!(after.parsed_words, before.parsed_words + 1);
    }

    /// Port of `Probe_DetectsGainedCoverage_AfterAddingReduplicationRule`: a full-copy rule means
    /// "sagsag" (RED('sag')) is only coverable once the rule exists.
    #[test]
    fn probe_detects_gained_coverage_after_adding_reduplication_rule() {
        let before_g = load(BASE);
        let corpus = ["sag", "sagsag", "dat"];
        let before = for_language(&before_g, &corpus);
        assert!(
            before.unparsed_words.contains(&"sagsag".to_string()),
            "precondition: sagsag not yet coverable"
        );

        let after_g = load(AFTER_REDUP);
        let after = for_language(&after_g, &corpus);
        assert!(!after.unparsed_words.contains(&"sagsag".to_string()));
        assert_eq!(after.parsed_words, before.parsed_words + 1);
    }
}
