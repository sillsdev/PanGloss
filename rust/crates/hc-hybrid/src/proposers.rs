//! `proposers.rs` (F6, HYBRID_FST_RUST_PLAN.md §8) — the sibling candidate generators that flank the
//! F4 bare walker inside [`crate::composite::CompositeAnalyzer`]: [`ReduplicationProposer`] (C#
//! `ReduplicationProposer.cs`) and [`InfixProposer`] (C# `InfixProposer.cs`), both read from
//! `C:\Users\johnm\Documents\repos\machine\.worktrees\fst-oracle\src\SIL.Machine.Morphology.HermitCrab\`
//! (the `fst-oracle` oracle branch).
//!
//! ## Scope decision (recorded, not silent — see this crate's F6 commit message)
//! `ComposedPhonologyProposer`, `LockstepPhonologyProposer` (+ its `PhonologyRuleCompiler` v1 +
//! `InversePhonology` substrate), and `ForwardSynthesisProposer` are wired into
//! [`crate::composite::CompositeAnalyzer`] at their correct FIXED ORDER position (matching C#
//! `CompositeProposer.ForLanguage`'s construction order exactly) but as DEFERRED STUBS that always
//! yield zero candidates. This is empirically justified, not a guess: diffing this milestone's own
//! frozen `candidates-composite.tsv` goldens against `candidates-bare.tsv` for ALL THREE grammars
//! (Indonesian, Sena, Amharic) shows the ONLY tag ever added beyond `FstTemplateAnalyzer` is
//! `ReduplicationProposer` — `InfixProposer`, `ComposedPhonologyProposer`, and
//! `LockstepPhonologyProposer` contribute a genuinely NEW signature on ZERO corpus words across all
//! three grammars, even though the C# oracle that generated those goldens ran the REAL
//! `ComposedPhonologyProposer`/`LockstepPhonologyProposer` (chain-off default composite). That
//! oracle fact is what makes the corpus-level headline gates (candidate parity, verified parity,
//! negatives) stub-safe for those three proposers specifically — Infix turns out to be corpus-inert
//! too (no infix rule in any of the three reference grammars), so it is built for REAL below (it is
//! cheap and self-contained) even though the corpus gate does not exercise it; only Composed/
//! Lockstep/ForwardSynthesis are deferred, because building them for real requires either a
//! standalone phonology-cascade-over-a-shape helper (`ComposedPhonologyProposer` — feasible, but not
//! on the critical path per an advisor review of this milestone) or an entirely new automaton
//! subsystem (`PhonologyRuleCompiler`/`InversePhonology`/the lockstep walker — confirmed
//! multi-day greenfield work, and the ONLY thing forcing it is `PhonologyRuleCompilerTests`, which
//! drives `AnalyzeComposed` end-to-end rather than asserting static compiler output alone). Deferred
//! precisely: `compiler_v1.rs`, `inverse.rs`, the lockstep proposer, `ComposedPhonologyProposer`'s
//! real logic, `ForwardSynthesisProposer`'s real logic, and their toy tests
//! (`PhonologyRuleCompilerTests`, `ComposedPhonology_CoversCrossBoundaryAlternation_...`,
//! `Composite_WithPhonologyAndReduplication_ParallelMatchesSequential`'s Composed-specific half).

use hc_grammar::chardef::{CharDefId, CharDefTable};
use hc_grammar::model::{Grammar, MRuleId, MorphRuleDef, MorphemeId, OutputAction};
use hc_shape::{NodeKind, Shape};

use crate::compiler::{self, RuleInverseTier};
use crate::inverse::InversePhonology;
use crate::surface::SurfacePhonology;
use crate::token::{classify_affix, MorphOp};
use crate::trie::{surface_table, Trie};
use crate::walk::{self, WordAnalysis};

/// The morpheme a morphological rule (`AffixProcess`/`Realizational`) owns. A `CompoundingRule`
/// never owns a single morpheme (see `token.rs::owning_morpheme`'s identical `unreachable!` arm) —
/// neither sibling generator in this file ever discovers a `MRuleId` that resolves to one (both
/// filter to `MorphRuleDef::AffixProcess`/the `affix_allomorphs()` accessor before ever recording a
/// rule id), so this only needs to handle the two kinds that actually reach it.
fn mrule_morpheme(g: &Grammar, id: MRuleId) -> MorphemeId {
    match &g.mrules[id.0 as usize] {
        MorphRuleDef::AffixProcess(def) => def.morpheme,
        MorphRuleDef::Realizational(def) => def.morpheme,
        MorphRuleDef::Compounding(_) => {
            unreachable!("proposers.rs never records a MRuleId for a CompoundingRule")
        }
    }
}

/// C# `ReduplicationProposer.RenderSurfaceOnly` (`ReduplicationProposer.cs:113-130`): render only the
/// Segment-kind nodes of `shape` through `table`'s first representation, `None` the instant any
/// Segment node has no representation (C#'s `return null` on a missing/empty `rep`) — the underlying
/// representation may carry boundary characters (e.g. Indonesian's `-i` LOC suffix is underlyingly
/// `+i`) that must not appear in the rendered surface text.
fn render_surface_only(table: &CharDefTable, shape: &Shape) -> Option<String> {
    let mut out = String::new();
    for (_, kind, cd, _flags) in shape.interior() {
        if kind != NodeKind::Segment {
            continue;
        }
        match table.get(CharDefId(cd)).representations().first() {
            Some(rep) if !rep.is_empty() => out.push_str(rep),
            _ => return None,
        }
    }
    Some(out)
}

/// C# `ReduplicationProposer.IsReduplication` (`:233-247`): **only** an `AffixProcessRule` is ever
/// checked (a `RealizationalAffixProcessRule` is never considered for reduplication classification
/// at all, even if one of its allomorphs happens to classify as `MorphOp::Reduplication` — a real,
/// faithfully-preserved C# quirk, not an oversight in this port).
fn is_reduplication_rule(def: &MorphRuleDef) -> bool {
    match def {
        MorphRuleDef::AffixProcess(d) => d
            .allomorphs
            .iter()
            .any(|a| classify_affix(&a.rhs) == MorphOp::Reduplication),
        _ => false,
    }
}

/// C# `ReduplicationProposer` (`ReduplicationProposer.cs`): full/partial-copy scan, suffix-copy scan,
/// separator+tail-copy scan, and separator+suffix-peel scan (four scan kinds total — see the module
/// doc). Recurses every residual through the BARE FST walker (the same `fst` instance the C#
/// constructor's `baseProposer` parameter names — never the composite itself, avoiding recursion
/// through the sibling generators).
pub struct ReduplicationProposer {
    /// `AffixProcessRule`s whose RHS classifies as reduplication, in grammar document order
    /// (stratum order, then `stratum.mrules` order).
    redup_rules: Vec<MRuleId>,
    /// `(suffix surface text, owning rule)` pairs for every ordinary SUFFIX-classified allomorph in
    /// the grammar (`AffixProcess` or `Realizational`), document order — the Phase G1 suffix-peel
    /// scan's search list.
    suffix_surfaces: Vec<(String, MRuleId)>,
}

impl ReduplicationProposer {
    pub const COVERED_OPS: [MorphOp; 1] = [MorphOp::Reduplication];

    pub fn new(g: &Grammar) -> Self {
        let (table, _w) = surface_table(g);
        let mut redup_rules = Vec::new();
        let mut suffix_surfaces = Vec::new();
        for stratum in &g.strata {
            for &mrule_id in &stratum.mrules {
                let def = &g.mrules[mrule_id.0 as usize];
                if is_reduplication_rule(def) {
                    redup_rules.push(mrule_id);
                    continue;
                }
                let Some(allomorphs) = def.affix_allomorphs() else {
                    continue; // CompoundingRule: not a MorphemicMorphologicalRule in C# either.
                };
                for allomorph in allomorphs {
                    if classify_affix(&allomorph.rhs) != MorphOp::Suffix {
                        continue;
                    }
                    let Some(insert_shape) = allomorph.rhs.iter().find_map(|a| match a {
                        OutputAction::InsertSegments { shape, .. } => Some(shape),
                        _ => None,
                    }) else {
                        continue;
                    };
                    if let Some(surface_text) = render_surface_only(table, &insert_shape.shape) {
                        if !surface_text.is_empty() {
                            suffix_surfaces.push((surface_text, mrule_id));
                        }
                    }
                }
            }
        }
        ReduplicationProposer {
            redup_rules,
            suffix_surfaces,
        }
    }

    /// C# `AnalyzeWord` (`:134-209`). Operates on `char`s (Rust's `char` == a Unicode scalar value;
    /// every reference grammar's alphabet is BMP-only, where C#'s UTF-16 `string.Length`/`Substring`
    /// indexing and a `Vec<char>`'s indexing coincide exactly) rather than raw bytes, so this never
    /// panics on a non-ASCII grammar's multi-byte UTF-8 word (Amharic).
    pub fn analyze_word(
        &self,
        g: &Grammar,
        trie: &Trie,
        word: &str,
        max_beam_work: i64,
    ) -> Vec<WordAnalysis> {
        let mut out = Vec::new();
        if self.redup_rules.is_empty() {
            return out;
        }
        let chars: Vec<char> = word.chars().collect();
        let len = chars.len();
        let max_copy_len = len / 2;

        for l in 1..=max_copy_len {
            // Prefix copy: chars[0..l] repeats immediately (chars[l..2l]) -- strip it.
            if chars[0..l] == chars[l..2 * l] {
                let residual: String = chars[l..len].iter().collect();
                self.propose_for_residual(g, trie, &residual, None, max_beam_work, &mut out);
            }
            // Suffix copy: the last l chars repeat the l chars before them -- strip the trailing copy.
            if chars[len - l..len] == chars[len - 2 * l..len - l] {
                let residual: String = chars[0..len - l].iter().collect();
                self.propose_for_residual(g, trie, &residual, None, max_beam_work, &mut out);
            }
        }

        // Separator + tail copy, and (Phase G1) separator + suffix-peel + tail copy.
        for sep_pos in 1..len.saturating_sub(1) {
            let before = &chars[0..sep_pos];
            let copy = &chars[sep_pos + 1..len];
            if copy.is_empty() {
                continue;
            }
            if before.len() >= copy.len() && before[before.len() - copy.len()..] == *copy {
                let residual: String = before.iter().collect();
                self.propose_for_residual(g, trie, &residual, None, max_beam_work, &mut out);
                continue; // plain tail matched -- do not also try the suffix-peel fallback.
            }
            for (suffix_text, suffix_rule) in &self.suffix_surfaces {
                let suffix_chars: Vec<char> = suffix_text.chars().collect();
                if suffix_chars.len() > copy.len() {
                    continue;
                }
                if copy[copy.len() - suffix_chars.len()..] != suffix_chars[..] {
                    continue;
                }
                let stripped_len = copy.len() - suffix_chars.len();
                if stripped_len == 0 {
                    continue;
                }
                let stripped_copy = &copy[..stripped_len];
                if before.len() >= stripped_copy.len()
                    && before[before.len() - stripped_copy.len()..] == *stripped_copy
                {
                    let residual: String = before.iter().collect();
                    self.propose_for_residual(
                        g,
                        trie,
                        &residual,
                        Some(*suffix_rule),
                        max_beam_work,
                        &mut out,
                    );
                }
            }
        }
        out
    }

    /// C# `ProposeForResidual` (`:211-231`): recurse `residual` through the bare FST walker, then
    /// wrap every returned base analysis with the reduplication morpheme (and, for the Phase G1
    /// suffix-peel path, the peeled suffix morpheme afterward) -- `root_index` is unchanged (the
    /// added morphemes are appended after the base's own morphemes, matching HC's `root … RED
    /// suffix` application order).
    fn propose_for_residual(
        &self,
        g: &Grammar,
        trie: &Trie,
        residual: &str,
        extra_suffix: Option<MRuleId>,
        max_beam_work: i64,
        out: &mut Vec<WordAnalysis>,
    ) {
        let base_outcome = walk::analyze_word(g, trie, residual, max_beam_work);
        for base in &base_outcome.analyses {
            for &redup in &self.redup_rules {
                let mut morphemes = base.morphemes.clone();
                morphemes.push(mrule_morpheme(g, redup));
                if let Some(suf) = extra_suffix {
                    morphemes.push(mrule_morpheme(g, suf));
                }
                out.push(WordAnalysis {
                    morphemes,
                    root_index: base.root_index,
                });
            }
        }
    }
}

/// C# `InfixProposer.InfixString` (`InfixProposer.cs:93-115`): the infix's inserted material iff it
/// is a SINGLE contiguous run of `InsertSegments` actions in the allomorph's RHS; `None` for a
/// templatic multi-slot infix (left to the engine, per the class doc's own scope note). Uses the RAW
/// authored representation (`SegmentedText::text`, C#'s `insert.Segments.Representation`) -- unlike
/// [`render_surface_only`] above, this is deliberately NOT filtered to segment-only nodes (the C#
/// source does not filter here either).
fn infix_string(rhs: &[OutputAction]) -> Option<String> {
    let mut runs: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for action in rhs {
        if let OutputAction::InsertSegments { shape, .. } = action {
            current
                .get_or_insert_with(String::new)
                .push_str(&shape.text);
        } else if let Some(run) = current.take() {
            runs.push(run);
        }
    }
    if let Some(run) = current {
        runs.push(run);
    }
    if runs.len() == 1 {
        runs.into_iter().next()
    } else {
        None
    }
}

/// First index `>= start` (in `char` units) at which `needle` occurs in `haystack`, or `None`. C#
/// `string.IndexOf(needle, start, StringComparison.Ordinal)`'s search semantics, restricted to a
/// non-empty `needle` (this port's only caller already guards that case -- see
/// [`InfixProposer::analyze_word`]).
fn index_of_chars(haystack: &[char], needle: &[char], start: usize) -> Option<usize> {
    if needle.is_empty() || start > haystack.len() || needle.len() > haystack.len() {
        return None;
    }
    (start..=haystack.len() - needle.len()).find(|&i| haystack[i..i + needle.len()] == *needle)
}

/// C# `InfixProposer` (`InfixProposer.cs`): remove-and-recurse over every interior occurrence of
/// every infix rule's surface-phonology variants (the underlying form is always included in
/// [`SurfacePhonology::variants`]'s own return, so a 0-phonology grammar is unaffected).
pub struct InfixProposer {
    /// `(owning rule, surface variants)` pairs, grammar document order. Only plain `AffixProcessRule`
    /// allomorphs are considered (C#'s `!(mrule is AffixProcessRule rule)` guard excludes
    /// `RealizationalAffixProcessRule` here, unlike [`ReduplicationProposer`]'s suffix-surfaces scan).
    infixes: Vec<(MRuleId, Vec<String>)>,
}

impl InfixProposer {
    pub const COVERED_OPS: [MorphOp; 1] = [MorphOp::Infix];

    pub fn new(g: &Grammar, surface: &SurfacePhonology) -> Self {
        let mut infixes = Vec::new();
        for stratum in &g.strata {
            for &mrule_id in &stratum.mrules {
                let MorphRuleDef::AffixProcess(def) = &g.mrules[mrule_id.0 as usize] else {
                    continue;
                };
                for allomorph in &def.allomorphs {
                    if classify_affix(&allomorph.rhs) != MorphOp::Infix {
                        continue;
                    }
                    let Some(infix) = infix_string(&allomorph.rhs) else {
                        continue;
                    };
                    if infix.is_empty() {
                        continue;
                    }
                    infixes.push((mrule_id, surface.variants(&infix)));
                }
            }
        }
        InfixProposer { infixes }
    }

    /// C# `AnalyzeWord` (`InfixProposer.cs:69-89`). See [`ReduplicationProposer::analyze_word`]'s
    /// doc for why this operates on `Vec<char>` rather than raw bytes.
    pub fn analyze_word(
        &self,
        g: &Grammar,
        trie: &Trie,
        word: &str,
        max_beam_work: i64,
    ) -> Vec<WordAnalysis> {
        let chars: Vec<char> = word.chars().collect();
        let mut out = Vec::new();
        for (rule, surfaces) in &self.infixes {
            for infix in surfaces {
                let infix_chars: Vec<char> = infix.chars().collect();
                if infix_chars.is_empty() {
                    continue;
                }
                // C#'s `while (i >= 1 && i + infix.Length < word.Length)` is a WHILE-loop condition,
                // not a per-occurrence `continue`: the scan for THIS infix stops the instant an
                // occurrence fails the interior-position test, even if a later occurrence would have
                // passed. Mirrored literally (`break`, not `continue`) rather than "improved" to scan
                // every occurrence.
                let mut i = index_of_chars(&chars, &infix_chars, 1);
                while let Some(pos) = i {
                    if !(pos >= 1 && pos + infix_chars.len() < chars.len()) {
                        break;
                    }
                    let mut residual_chars = chars.clone();
                    residual_chars.drain(pos..pos + infix_chars.len());
                    let residual: String = residual_chars.iter().collect();
                    let base_outcome = walk::analyze_word(g, trie, &residual, max_beam_work);
                    for base in &base_outcome.analyses {
                        let mut morphemes = base.morphemes.clone();
                        morphemes.push(mrule_morpheme(g, *rule));
                        out.push(WordAnalysis {
                            morphemes,
                            root_index: base.root_index,
                        });
                    }
                    i = index_of_chars(&chars, &infix_chars, pos + 1);
                }
            }
        }
        out
    }
}

/// `ChainPhonologyProposer` (F7, HYBRID_FST_RUST_PLAN.md §8): port of C# `ChainPhonologyProposer.cs`
/// -- phonology coverage via the GENERAL rule-inverse chain (`compiler::compile_default`, I1/I3)
/// walked by [`walk::analyze_chain`] (I2), the opt-in replacement candidate for the v1 lockstep
/// compiler (`compiler_v1.rs`, still F7's other job).
///
/// Owns its OWN "underlying-only acceptor" trie (`Trie::build_ex` with `enable_variants: false`,
/// `enable_junction_probing: false`), mirroring C#'s `_underlyingOnlyFst = new
/// FstTemplateAnalyzer(language)` -- composing against the surface-precompiled trie the bare
/// walker/composite share would apply phonology twice (LEVER_2.md's original finding); see
/// `trie.rs`'s `Trie::build_ex` doc for the exact C# ctor evidence this mirrors.
///
/// **Chain order.** `compiler::compile_default` enumerates strata/rules in forward (synthesis)
/// document order; `walk::analyze_chain` wants reverse-application order (index 0 = surface-facing,
/// the inverse of the LAST rule HC applied). A single flat `.rev()` of the compiled list reaches
/// that order (reversing a concatenation of per-stratum groups yields the reversed groups in
/// reversed order, i.e. exactly "strata reversed, and within each stratum the rule list reversed") --
/// the same argument C#'s own `ChainPhonologyProposer.cs` module doc makes for why
/// `ComposedPhonologyProposer`'s already-correct `Strata.Reverse().SelectMany(s =>
/// s.PhonologicalRules.Reverse())` order and this proposer's single flat `.Reverse()` land on the
/// identical order.
///
/// **IdentitySkip rules dropped before reversal** -- a PERFORMANCE choice: an `IdentitySkip` rule's
/// `Pinv` is identity-only by [`RuleInverseTier`]'s own contract, so stacking it into the chain would
/// be harmless (every symbol passes its self-loops unchanged) but adds a pure extra walk-time level
/// with zero coverage gain.
pub struct ChainPhonologyProposer {
    underlying_trie: Trie,
    /// Reverse-application order, `IdentitySkip` entries already dropped. Empty means "no rule
    /// contributed a non-identity branch" (a Sena-like no-phonology grammar) -- [`AnalyzeWord`]
    /// mirrors C#'s own early-out for that case rather than paying the chain-walk setup cost for a
    /// guaranteed-empty result.
    ///
    /// [`AnalyzeWord`]: ChainPhonologyProposer::analyze_word
    chain: Vec<InversePhonology>,
    max_beam_work: i64,
    max_boundary_insertions: i32,
}

impl ChainPhonologyProposer {
    pub fn new(g: &Grammar, surface: &SurfacePhonology, morpher: &hc_parse::Morpher, max_states: usize, deriv_depth: usize, max_beam_work: i64) -> Self {
        let underlying_trie = Trie::build_ex(g, surface, morpher, max_states, deriv_depth, false, false);
        let compiled = compiler::compile_default(g);
        let chain: Vec<InversePhonology> = compiled
            .into_iter()
            .filter(|c| c.tier != RuleInverseTier::IdentitySkip)
            .rev()
            .map(|c| c.pinv)
            .collect();
        ChainPhonologyProposer { underlying_trie, chain, max_beam_work, max_boundary_insertions: walk::DEFAULT_MAX_BOUNDARY_INSERTIONS }
    }

    /// How many walk-chain rules (after dropping `IdentitySkip`) this proposer's chain stacks --
    /// mirrors C#'s `ChainLength` (an I7 measurement-battery diagnostic).
    pub fn chain_length(&self) -> usize {
        self.chain.len()
    }

    pub fn analyze_word(&self, g: &Grammar, word: &str) -> Vec<WordAnalysis> {
        if self.chain.is_empty() {
            return Vec::new();
        }
        walk::analyze_chain(g, &self.underlying_trie, &self.chain, word, self.max_beam_work, self.max_boundary_insertions).analyses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path(name: &str) -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../../samples/data").join(name);
        path.exists().then_some(path)
    }

    fn load_indonesian() -> Option<Grammar> {
        let path = sample_path("indonesian-hc.xml")?;
        let xml = std::fs::read_to_string(&path).expect("read grammar");
        Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
    }

    fn build_trie(g: &Grammar) -> Trie {
        let build_morpher = hc_parse::Morpher::new(g, usize::MAX);
        let surface = SurfacePhonology::new(g);
        Trie::build(g, &surface, &build_morpher, 1_000_000, 2, true)
    }

    /// Sanity: the Indonesian grammar's redup rules recover "membagi-bagi" (a known corpus word)
    /// with the residual "membagi" analyzed by the bare walker.
    #[test]
    fn reduplication_recovers_known_corpus_word() {
        let Some(g) = load_indonesian() else {
            eprintln!("skipping: indonesian-hc.xml not present on disk");
            return;
        };
        let trie = build_trie(&g);
        let redup = ReduplicationProposer::new(&g);
        assert!(
            !redup.redup_rules.is_empty(),
            "Indonesian must have at least one redup rule"
        );
        let out = redup.analyze_word(&g, &trie, "membagi-bagi", walk::DEFAULT_MAX_BEAM_WORK);
        assert!(
            !out.is_empty(),
            "expected at least one reduplication candidate for membagi-bagi"
        );
    }

    /// `index_of_chars` basic behavior: interior-only, ordinal, no false match at/after the end.
    #[test]
    fn index_of_chars_finds_interior_occurrences_only() {
        let haystack: Vec<char> = "saag".chars().collect();
        let needle: Vec<char> = "a".chars().collect();
        assert_eq!(index_of_chars(&haystack, &needle, 1), Some(1));
        assert_eq!(index_of_chars(&haystack, &needle, 2), Some(2));
        assert_eq!(index_of_chars(&haystack, &needle, 3), None);
    }
}
