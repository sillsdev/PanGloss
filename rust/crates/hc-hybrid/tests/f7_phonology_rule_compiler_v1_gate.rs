//! Port of C# `PhonologyRuleCompilerTests.cs` (F7's "ported test classes" gate): does
//! `compiler_v1::compile` (v1's merged-automaton `PhonologyRuleCompiler`) auto-recover the same
//! shapes the plan's LEVER_2 spike hand-built, and does `LockstepPhonologyProposer` wire it
//! correctly.
//!
//! C#'s first five methods call `lex.AnalyzeComposed(word, pinv)` DIRECTLY -- `FstTemplateAnalyzer`'s
//! own chain-walk method taking an arbitrary `Pinv` -- NOT through `LockstepPhonologyProposer`'s
//! `HasNonIdentityArcs` pre-check (quirk 1: only inspects arcs from the START state; a
//! left-environment-gated branch can look all-identity from state 0 alone and get the whole Pinv
//! wrongly skipped -- documented at length in `proposers.rs` and pinned by `ChainDeletionEpenthesisTests`'s
//! own honesty note in the C# original). The direct Rust analog of `AnalyzeComposed` is
//! `walk::analyze_chain(g, &underlying_only_trie, &[pinv], word, ...)`, called directly here for
//! those five tests -- NOT via `LockstepPhonologyProposer::analyze_word` (found empirically: an
//! earlier draft used the wrapper for all six tests and
//! `compile_auto_recovers_left_context_substitution` failed silently on a correctly-compiled,
//! non-unsupported rule purely because of the wrapper's own quirk-1 gate). Only the LAST test
//! (`LockstepPhonologyProposer_CoversDeletion_WiredThroughComposite`) deliberately goes through the
//! real wrapper class + composite, matching its C# original exactly (that test's whole point IS the
//! wrapper's wiring, quirk included).
//!
//! C#'s six methods build each toy grammar IN CODE against one shared test base; this port
//! authors FOUR separate small XML fixtures instead of one shared one -- deliberately, not for
//! convenience: combining an UNCONDITIONED rule with a context-GATED rule of the same shape in one
//! stratum would let the unconditioned rule silently paper over the gated rule's own environment
//! check (the whole point of `Compile_AutoRecoversLeftContextSubstitution`/
//! `Compile_AutoRecoversUnconditionedSubstitution` is to exercise those two branches
//! DISTINGUISHABLY). The two deletion rules (right-context, left-context) do NOT have this problem
//! (verified: each rule's own environment never matches the other's target word), so those two plus
//! the composite-wiring test share one fixture (`PhonologyRuleCompilerDeletionToyGrammar.xml`).
//!
//! All four `compile_auto_recovers_*` fixtures use a SUFFIXED word, not a bare root (found
//! empirically, corrected from an earlier bare-root draft): `Trie::build_ex`'s own doc records a
//! known, already-scoped port gap -- `bare_root_surfaces` runs its full synthesis-aware path
//! regardless of `enable_variants`/`enable_junction_probing`, unlike C#'s truly-bare
//! `FstTemplateAnalyzer(language)` ctor -- so a bare root's underlying-only-trie baseline in this
//! port already bakes the rule's effect in, where C#'s baseline would not. A suffixed word sidesteps
//! this (the suffix's own arc has no baked variant under `enable_variants=false`).

use hc_hybrid::compiler_v1;
use hc_hybrid::composite::CompositeAnalyzer;
use hc_hybrid::proposers::LockstepPhonologyProposer;
use hc_hybrid::replay;
use hc_hybrid::surface::SurfacePhonology;
use hc_hybrid::trie::Trie;
use hc_hybrid::walk;
use hc_parse::Morpher;

fn load(fixture: &str) -> hc_grammar::model::Grammar {
    let xml = std::fs::read_to_string(format!(
        "{}/tests/fixtures/fst-advisor-toys/{fixture}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("read {fixture}: {e}"));
    hc_grammar::load(&xml).unwrap_or_else(|e| panic!("{fixture} failed to load: {e}"))
}

/// C# `lex.AnalyzeComposed(word, pinv)`: the underlying-only trie plus a direct length-1 chain walk
/// of `pinv` (bypassing `LockstepPhonologyProposer`'s own quirk-1 gate entirely -- see module doc).
fn analyze_composed(g: &hc_grammar::model::Grammar, surface: &SurfacePhonology, pinv: hc_hybrid::inverse::InversePhonology, word: &str) -> Vec<hc_hybrid::walk::WordAnalysis> {
    let morpher = Morpher::new(g, usize::MAX);
    let underlying_trie = Trie::build_ex(g, surface, &morpher, 1_000_000, 2, false, false);
    walk::analyze_chain(g, &underlying_trie, std::slice::from_ref(&pinv), word, walk::DEFAULT_MAX_BEAM_WORK, walk::DEFAULT_MAX_BOUNDARY_INSERTIONS).analyses
}

fn bare_underlying_walk_is_empty(g: &hc_grammar::model::Grammar, surface: &SurfacePhonology, word: &str) -> bool {
    let morpher = Morpher::new(g, usize::MAX);
    let underlying_trie = Trie::build_ex(g, surface, &morpher, 1_000_000, 2, false, false);
    walk::analyze_word(g, &underlying_trie, word, walk::DEFAULT_MAX_BEAM_WORK).analyses.is_empty()
}

/// C# `composed.IsSubsetOf(engine)` soundness check, ported via `xml_key` (same convention
/// `replay::signature`/`composite::candidate_signature` use).
fn engine_sigs(g: &hc_grammar::model::Grammar, word: &str) -> std::collections::HashSet<String> {
    Morpher::new(g, usize::MAX)
        .parse_word(word)
        .structured
        .iter()
        .map(|wa| {
            let keys: Vec<&str> = wa.morpheme_ids.iter().map(|&id| g.morphemes[id as usize].xml_key.as_str()).collect();
            format!("{}:{}", keys.join("+"), wa.root_morpheme_index)
        })
        .collect()
}

fn composed_sigs(g: &hc_grammar::model::Grammar, analyses: &[hc_hybrid::walk::WordAnalysis]) -> std::collections::HashSet<String> {
    analyses
        .iter()
        .map(|wa| {
            let keys: Vec<&str> = wa.morphemes.iter().map(|&hc_grammar::model::MorphemeId(id)| g.morphemes[id as usize].xml_key.as_str()).collect();
            format!("{}:{}", keys.join("+"), wa.root_index)
        })
        .collect()
}

#[test]
fn compile_auto_recovers_boundary_deletion() {
    // "sag"+KD = "sagkd" -> k_deletion_right (k->0/_d) -> surface "sagd".
    let g = load("PhonologyRuleCompilerDeletionToyGrammar.xml");
    let result = compiler_v1::compile(&g);
    assert_eq!(result.unsupported_rule_count, 0, "this rule is entirely within the v1 supported shape");

    let surface = SurfacePhonology::new(&g);
    assert!(bare_underlying_walk_is_empty(&g, &surface, "sagd"), "baseline: the underlying-only walk alone must miss 'sagd'");

    let composed = analyze_composed(&g, &surface, result.pinv, "sagd");
    assert!(composed.iter().any(|c| c.morphemes.len() == 2), "auto-compiled Pinv must recover the deletion form: {composed:?}");
    assert!(composed_sigs(&g, &composed).is_subset(&engine_sigs(&g, "sagd")), "soundness: composed candidates must be a subset of the engine's");

    let result2 = compiler_v1::compile(&g);
    assert!(analyze_composed(&g, &surface, result2.pinv, "saga").is_empty(), "a non-word must yield nothing");
}

#[test]
fn compile_auto_recovers_left_context_deletion() {
    // "sag"+DK = "sagdk" -> k_deletion_left (k->0/d_) -> surface "sagd".
    let g = load("PhonologyRuleCompilerDeletionToyGrammar.xml");
    let result = compiler_v1::compile(&g);
    assert_eq!(result.unsupported_rule_count, 0, "left-context-only deletion is within the supported shape");

    let surface = SurfacePhonology::new(&g);
    let composed = analyze_composed(&g, &surface, result.pinv, "sagd");
    assert!(composed.iter().any(|c| c.morphemes.len() == 2), "auto-compiled Pinv must recover the left-context deletion form: {composed:?}");
    assert!(composed_sigs(&g, &composed).is_subset(&engine_sigs(&g, "sagd")), "soundness: composed candidates must be a subset of the engine's");

    let result2 = compiler_v1::compile(&g);
    assert!(analyze_composed(&g, &surface, result2.pinv, "saga").is_empty(), "a non-word must yield nothing");
}

#[test]
fn compile_auto_recovers_unconditioned_substitution() {
    // "da"+"t" = "dat" -> t_to_d (unconditioned) -> surface "dad".
    let g = load("PhonologyRuleCompilerUnconditionedSubToyGrammar.xml");
    let result = compiler_v1::compile(&g);
    assert_eq!(result.unsupported_rule_count, 0);

    let surface = SurfacePhonology::new(&g);
    assert!(bare_underlying_walk_is_empty(&g, &surface, "dad"), "baseline: the underlying-only walk alone must miss 'dad'");

    let composed = analyze_composed(&g, &surface, result.pinv, "dad");
    assert!(!composed.is_empty(), "auto-compiled Pinv must recover the substituted form: {composed:?}");
    assert!(composed_sigs(&g, &composed).is_subset(&engine_sigs(&g, "dad")), "soundness: composed candidates must be a subset of the engine's");

    let result2 = compiler_v1::compile(&g);
    assert!(analyze_composed(&g, &surface, result2.pinv, "zzz").is_empty(), "a non-word must yield nothing");
}

#[test]
fn compile_skips_unconditioned_deletion_as_unsupported() {
    let g = load("PhonologyRuleCompilerUnsupportedDeletionToyGrammar.xml");
    let result = compiler_v1::compile(&g);
    assert_eq!(result.unsupported_rule_count, 1, "unconditioned deletion must be marked unsupported, not compiled");
}

#[test]
fn compile_auto_recovers_left_context_substitution() {
    // "kag"+"t" = "kagt" -> t_to_d_left (t->d/g_) -> surface "kagd".
    let g = load("PhonologyRuleCompilerLeftContextSubToyGrammar.xml");
    let result = compiler_v1::compile(&g);
    assert_eq!(result.unsupported_rule_count, 0);

    let surface = SurfacePhonology::new(&g);
    assert!(bare_underlying_walk_is_empty(&g, &surface, "kagd"), "baseline: the underlying-only walk alone must miss 'kagd'");

    let composed = analyze_composed(&g, &surface, result.pinv, "kagd");
    assert!(!composed.is_empty(), "auto-compiled Pinv must recover the left-conditioned substitution: {composed:?}");
    assert!(composed_sigs(&g, &composed).is_subset(&engine_sigs(&g, "kagd")), "soundness: composed candidates must be a subset of the engine's");

    let result2 = compiler_v1::compile(&g);
    assert!(analyze_composed(&g, &surface, result2.pinv, "kagz").is_empty(), "a non-word must yield nothing");
}

/// `LockstepPhonologyProposer_CoversDeletion_WiredThroughComposite`: the composite (default
/// Lockstep at position 6) must match the unrestricted engine's own verified analysis set for
/// "sagd", end to end -- through the REAL wrapper class (quirk 1 included), not the direct
/// `AnalyzeComposed` bypass the other five tests use.
#[test]
fn lockstep_phonology_proposer_covers_deletion_wired_through_composite() {
    let g = load("PhonologyRuleCompilerDeletionToyGrammar.xml");
    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let lockstep_morpher = Morpher::new(&g, usize::MAX);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false)
        .with_lockstep_phonology(&g, &surface, &lockstep_morpher, 1_000_000, 2);

    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);
    let got = composite.analyze_word_verified(&verify_morpher, &owners, "sagd");
    assert!(!got.is_empty(), "composite (via the lockstep proposer) must recover 'sagd'");

    // Also confirm the wrapper itself sees non-identity arcs for this rule (the k_deletion_right
    // rule's RightEnvironment gate does not trip quirk 1 the way a left-environment gate can).
    let has_arcs_morpher = Morpher::new(&g, usize::MAX);
    let lockstep = LockstepPhonologyProposer::new(&g, &surface, &has_arcs_morpher, 1_000_000, 2, walk::DEFAULT_MAX_BEAM_WORK);
    assert!(lockstep.has_arcs(), "the merged Pinv must have real non-identity arcs for this grammar");

    // Soundness: a non-word must still yield nothing.
    let none = composite.analyze_word_verified(&verify_morpher, &owners, "zzz");
    assert!(none.is_empty(), "soundness: a non-word must still yield nothing");
}

/// Pins quirk 1 BEHAVIORALLY (not just by module-doc assertion): a left-environment-gated
/// substitution's real arc lives one hop past state 0 (`chain_left_environment_v1` routes it
/// through an intermediate state), so `arcs_from(0)` is identity-only even though the compiled
/// Pinv demonstrably recovers the word via a direct chain walk (`analyze_composed`, same as the
/// `compile_auto_recovers_left_context_substitution` test above). `LockstepPhonologyProposer`'s
/// `has_arcs()` must therefore be wrongly `false` here -- the exact C# bug (`HasNonIdentityArcs`
/// scans only state 0), not a hypothetical.
#[test]
fn quirk1_lockstep_proposer_misses_left_environment_gated_rule_from_state_zero() {
    let g = load("PhonologyRuleCompilerLeftContextSubToyGrammar.xml");

    // Ground truth: the compiled Pinv genuinely recovers "kagd" via a direct chain walk.
    let surface = SurfacePhonology::new(&g);
    let result = compiler_v1::compile(&g);
    assert_eq!(result.unsupported_rule_count, 0);
    let composed = analyze_composed(&g, &surface, result.pinv, "kagd");
    assert!(!composed.is_empty(), "the compiled Pinv genuinely has a usable non-identity arc (one hop past state 0)");

    // But the wrapper's own start-state-only scan misses it entirely.
    let morpher = Morpher::new(&g, usize::MAX);
    let lockstep = LockstepPhonologyProposer::new(&g, &surface, &morpher, 1_000_000, 2, walk::DEFAULT_MAX_BEAM_WORK);
    assert!(!lockstep.has_arcs(), "quirk 1: a left-environment-gated rule's arc is invisible to a state-0-only scan");
    assert!(lockstep.analyze_word(&g, "kagd").is_empty(), "quirk 1's downstream effect: the wrapper silently contributes nothing for this word");
}

/// Pins quirk 2 BEHAVIORALLY: v1's `_alphabet` is Segment-type char defs only (see module doc), so
/// a rule whose environment references a `BoundaryMarker` (Type=Boundary) can never find an
/// alphabet representative for that environment node -- `build_probe_representative_v1` returns
/// `None`, and `try_compile_subrule_v1` reports the whole subrule unsupported, even though
/// `compiler.rs`'s sibling `RuleInverseCompiler` (Segment ∪ Boundary alphabet) compiles the exact
/// same rule shape `Exact` (see `f7_boundary_tape_chain_gate.rs`'s
/// `chain_recovers_boundary_conditioned_substitution_after_morpheme_junction`, same fixture).
#[test]
fn quirk2_boundary_conditioned_rule_is_unsupported_by_v1_segment_only_alphabet() {
    let g = load("BoundaryTapeChainSubstitutionToyGrammar.xml");
    let result = compiler_v1::compile(&g);
    assert_eq!(result.unsupported_rule_count, 1, "v1's Segment-only alphabet cannot find a representative for a BoundaryMarker-gated environment");
}
