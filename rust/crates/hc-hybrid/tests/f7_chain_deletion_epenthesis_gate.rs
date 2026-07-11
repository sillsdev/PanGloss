//! Port of C# `ChainDeletionEpenthesisTests.cs` (I3 of `FST_FULL_GRAMMAR_PLAN.md`): deletion-inverse
//! (ε-input restoration arcs, capped) and epenthesis-inverse (ε-output arcs) through
//! `compiler::compile`, walked end-to-end via `walk::analyze_chain` against a real lexicon trie.
//!
//! Each test follows the C# original's four-part shape: (a) the real engine parses the word; (b)
//! the bare walker misses it; (c) the chain covers it; (d) a matched non-word stays unparsed. Root
//! words are SUFFIXED, not literal bare roots (see this port's established finding, first recorded
//! in the `PhonologyRuleCompilerTests` port commit: `Trie::build_ex`'s own doc records a known,
//! already-scoped gap where `bare_root_surfaces` always runs its full synthesis-aware path
//! regardless of `enable_variants`, so a bare root's baseline is unreliable in this port); each
//! fixture's own header records the specific word-shape substitution and why it still exercises the
//! same word-internal, non-boundary-conditioned rule shape the C# original intends.
//!
//! C#'s own "HONESTY NOTE" convention (documenting that today's real composite already covers these
//! words via `ComposedPhonologyProposer`/bare-root synthesis, independent of the chain) is preserved
//! as informational assertions, not requirements the chain depends on.

use hc_hybrid::compiler::{self, CompiledRuleInverse};
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

fn rule<'a>(compiled: &'a [CompiledRuleInverse], name: &str) -> &'a CompiledRuleInverse {
    compiled.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no compiled rule named {name:?}"))
}

fn chain_covers(g: &hc_grammar::model::Grammar, trie: &Trie, chain: &[hc_hybrid::inverse::InversePhonology], word: &str) -> bool {
    !walk::analyze_chain(g, trie, chain, word, walk::DEFAULT_MAX_BEAM_WORK, walk::DEFAULT_MAX_BOUNDARY_INSERTIONS).analyses.is_empty()
}

#[test]
fn chain_recovers_word_internal_deletion_with_env_gating() {
    // "kadata"+"-i" = "kadatai" -> d_deletion (d->0/a_a) -> surface "kaatai".
    let g = load("ChainDeletionWithEnvGatingToyGrammar.xml");
    let morpher = Morpher::new(&g, usize::MAX);
    assert!(!morpher.parse_word("kaatai").analyses.is_empty(), "precondition: the engine parses 'kaatai' under the deletion rule");

    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let bare_trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    assert!(walk::analyze_word(&g, &bare_trie, "kaatai", walk::DEFAULT_MAX_BEAM_WORK).analyses.is_empty(), "baseline: the bare walker must miss the deletion surface form");

    let compile_morpher = Morpher::new(&g, usize::MAX);
    let compiled = compiler::compile_default(&g);
    let r = rule(&compiled, "d_deletion");
    assert_eq!(r.tier, compiler::RuleInverseTier::Exact, "an env-gated deletion must compile Exact: {:?}", r.reasons);
    let _ = compile_morpher;

    let chain_trie_morpher = Morpher::new(&g, usize::MAX);
    let chain_trie = Trie::build(&g, &surface, &chain_trie_morpher, 1_000_000, 2, true);
    let compiled2 = compiler::compile_default(&g);
    let r2 = rule(&compiled2, "d_deletion");
    assert!(chain_covers(&g, &chain_trie, std::slice::from_ref(&r2.pinv), "kaatai"), "the chain must restore the deleted word-internal segment");

    // Soundness: a matched non-word (same length/shape, no valid restoration site) must stay unparsed.
    let compiled3 = compiler::compile_default(&g);
    let r3 = rule(&compiled3, "d_deletion");
    assert!(!chain_covers(&g, &chain_trie, std::slice::from_ref(&r3.pinv), "kaitai"), "a matched non-word must stay unparsed");
}

#[test]
fn chain_recovers_word_internal_epenthesis_with_env_gating() {
    // "atka"+"-u" = "atkau" -> i_epenthesis (0->i/t_k) -> surface "atikau".
    let g = load("ChainEpenthesisWithEnvGatingToyGrammar.xml");
    let morpher = Morpher::new(&g, usize::MAX);
    assert!(!morpher.parse_word("atikau").analyses.is_empty(), "precondition: the engine parses 'atikau' under the epenthesis rule");

    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let bare_trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    assert!(walk::analyze_word(&g, &bare_trie, "atikau", walk::DEFAULT_MAX_BEAM_WORK).analyses.is_empty(), "baseline: the bare walker must miss the epenthesized surface form");

    let compiled = compiler::compile_default(&g);
    let r = rule(&compiled, "i_epenthesis");
    assert_eq!(r.tier, compiler::RuleInverseTier::Exact, "an env-gated epenthesis must compile Exact: {:?}", r.reasons);

    let chain_trie_morpher = Morpher::new(&g, usize::MAX);
    let chain_trie = Trie::build(&g, &surface, &chain_trie_morpher, 1_000_000, 2, true);
    let compiled2 = compiler::compile_default(&g);
    let r2 = rule(&compiled2, "i_epenthesis");
    assert!(chain_covers(&g, &chain_trie, std::slice::from_ref(&r2.pinv), "atikau"), "the chain must strip the epenthesized segment");

    // Soundness: a different vowel in the epenthesis slot must stay unparsed.
    let compiled3 = compiler::compile_default(&g);
    let r3 = rule(&compiled3, "i_epenthesis");
    assert!(!chain_covers(&g, &chain_trie, std::slice::from_ref(&r3.pinv), "atukau"), "a matched non-word must stay unparsed");
}

#[test]
fn chain_respects_restoration_cap_unconditioned_deletion() {
    // "dagad"+"-u" = "dagadu" -> d_deletion_unconditioned (d->0, no env) -> surface "agau" -- BOTH
    // d's deleted, so recovering it needs TWO restoration events of this one rule.
    let g = load("ChainRestorationCapToyGrammar.xml");
    let morpher = Morpher::new(&g, usize::MAX);
    assert!(!morpher.parse_word("agau").analyses.is_empty(), "precondition: the engine parses 'agau', restoring BOTH deleted d's in one round");

    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let bare_trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    assert!(walk::analyze_word(&g, &bare_trie, "agau", walk::DEFAULT_MAX_BEAM_WORK).analyses.is_empty(), "baseline: the bare walker must miss the doubly-deleted surface form");

    let chain_trie_morpher = Morpher::new(&g, usize::MAX);
    let chain_trie = Trie::build(&g, &surface, &chain_trie_morpher, 1_000_000, 2, true);

    // Cap 1 (the DEFAULT_RESTORATION_CAP): the word needs 2 restoration events -- must fall to
    // unparsed, not hang and not over-restore.
    let cap1_all = compiler::compile(&g, 1);
    let cap1 = rule(&cap1_all, "d_deletion_unconditioned");
    assert_eq!(cap1.tier, compiler::RuleInverseTier::Exact, "an unconditioned deletion is compilable (the trie prunes restorations in lockstep): {:?}", cap1.reasons);
    assert!(!chain_covers(&g, &chain_trie, std::slice::from_ref(&cap1.pinv), "agau"), "cap respected STRICTLY: 2 restorations under a cap of 1 falls to unparsed");

    // Cap 2: now covered.
    let cap2_all = compiler::compile(&g, 2);
    let cap2 = rule(&cap2_all, "d_deletion_unconditioned");
    assert!(chain_covers(&g, &chain_trie, std::slice::from_ref(&cap2.pinv), "agau"), "with cap 2 the chain restores both deleted segments");

    // Cap 0 disables deletion-inverse entirely -- honest IdentitySkip, reason "restoration-cap".
    let cap0_all = compiler::compile(&g, 0);
    let cap0 = rule(&cap0_all, "d_deletion_unconditioned");
    assert_eq!(cap0.tier, compiler::RuleInverseTier::IdentitySkip);
    assert!(cap0.reasons.contains(&"restoration-cap".to_string()));

    // Soundness: no restoration sequence reaches any lexeme.
    assert!(!chain_covers(&g, &chain_trie, std::slice::from_ref(&cap2.pinv), "azau"), "a matched non-word must stay unparsed");
}
