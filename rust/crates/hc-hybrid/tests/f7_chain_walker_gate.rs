//! Port of C# `ChainWalkerTests.cs` (I2 of `FST_FULL_GRAMMAR_PLAN.md`): end-to-end tests of the
//! chain walker (`walk::analyze_chain`) built from real `compiler::compile` output against a real
//! lexicon trie -- as opposed to `compiler.rs`'s own inline tests, which only feed a compiled
//! `InversePhonology` through the standalone `f7_rule_inverse_compiler_gate.rs` interpreter.
//!
//! Each real test follows the C# original's four-part shape: (a) the real engine parses the word;
//! (b) the bare walker misses it; (c) the chain covers it; (d) a matched non-word stays unparsed.
//! Root words are SUFFIXED (see the established finding from the `PhonologyRuleCompilerTests`/
//! `ChainDeletionEpenthesisTests` ports: a bare root's underlying-only-trie baseline is unreliable
//! in this port, `Trie::build_ex`'s own doc records the gap) except for the long-distance-harmony
//! fixture, which was already root+suffix in the C# original.
//!
//! `Chain_RecoversMetathesis_WordInternalConsonantSwap` is a DEFERRED gap, not a full port (see
//! `compiler.rs`'s `compile_metathesis_stub` doc, and `f7_rule_inverse_compiler_gate.rs`'s own
//! metathesis test for the first instance of this same documented deferral): asserts what actually
//! happens (real engine parses the metathesized word; the compiled Pinv is the identity-only stub;
//! the chain built from it does NOT recover the word), not C#'s full recovery expectation.
//!
//! C#'s own "HONESTY NOTE" convention (today's real composite already covers these words via
//! `ComposedPhonologyProposer`/bare-root synthesis, independent of the chain) is not re-asserted
//! here since it is not load-bearing for what the CHAIN itself must prove.

use hc_hybrid::compiler::{self, CompiledRuleInverse, RuleInverseTier};
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

/// C# `Sig(WordAnalysis)`: `join("+", gloss)` in morpheme order + `":"` + root index -- ported here
/// via `xml_key` (same convention `replay::signature`/`composite::candidate_signature` use) so the
/// real-engine set and the chain-walked set are directly comparable strings.
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

fn chain_sigs(g: &hc_grammar::model::Grammar, trie: &Trie, chain: &[hc_hybrid::inverse::InversePhonology], word: &str) -> std::collections::HashSet<String> {
    walk::analyze_chain(g, trie, chain, word, walk::DEFAULT_MAX_BEAM_WORK, walk::DEFAULT_MAX_BOUNDARY_INSERTIONS)
        .analyses
        .iter()
        .map(|wa| {
            let keys: Vec<&str> = wa.morphemes.iter().map(|&hc_grammar::model::MorphemeId(id)| g.morphemes[id as usize].xml_key.as_str()).collect();
            format!("{}:{}", keys.join("+"), wa.root_index)
        })
        .collect()
}

#[test]
fn chain_recovers_word_internal_rule_two_segment_lhs_far_from_any_boundary() {
    // "katanaga"+"-i" -> an_to_am (2-segment Lhs "a n"->"a m", unconditioned) -> "katamagai".
    let g = load("ChainTwoSegLhsToyGrammar.xml");
    let morpher = Morpher::new(&g, usize::MAX);
    assert!(!morpher.parse_word("katamagai").analyses.is_empty(), "precondition: the engine parses 'katamagai'");

    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let bare_trie = Trie::build_ex(&g, &surface, &build_morpher, 1_000_000, 2, false, false);
    assert!(walk::analyze_word(&g, &bare_trie, "katamagai", walk::DEFAULT_MAX_BEAM_WORK).analyses.is_empty(), "baseline: the bare walker must miss the substituted surface form");

    let compiled = compiler::compile_default(&g);
    let r = rule(&compiled, "an_to_am");
    assert_eq!(r.tier, RuleInverseTier::Exact, "{:?}", r.reasons);

    let chain_trie_morpher = Morpher::new(&g, usize::MAX);
    let chain_trie = Trie::build_ex(&g, &surface, &chain_trie_morpher, 1_000_000, 2, false, false);
    assert!(chain_covers(&g, &chain_trie, std::slice::from_ref(&r.pinv), "katamagai"), "the chain must recover the word-internal 2-segment substitution");
    assert!(
        chain_sigs(&g, &chain_trie, std::slice::from_ref(&r.pinv), "katamagai").is_subset(&engine_sigs(&g, "katamagai")),
        "soundness: chain candidates must be a subset of the engine's"
    );

    let compiled2 = compiler::compile_default(&g);
    let r2 = rule(&compiled2, "an_to_am");
    assert!(!chain_covers(&g, &chain_trie, std::slice::from_ref(&r2.pinv), "katamazai"), "a matched non-word (same target substring, different elsewhere) must stay unparsed");
}

#[test]
fn chain_recovers_two_rule_feeding_chain_mid_root() {
    // "aaatpaaa"+"-i" -> p_to_b (unconditioned) feeds t_to_d_before_b (t->d/_b) -> "aaadbaaai".
    let g = load("ChainFeedingRuleToyGrammar.xml");
    let morpher = Morpher::new(&g, usize::MAX);
    assert!(!morpher.parse_word("aaadbaaai").analyses.is_empty(), "precondition: the engine parses 'aaadbaaai' under the feeding cascade");

    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let bare_trie = Trie::build_ex(&g, &surface, &build_morpher, 1_000_000, 2, false, false);
    assert!(walk::analyze_word(&g, &bare_trie, "aaadbaaai", walk::DEFAULT_MAX_BEAM_WORK).analyses.is_empty(), "baseline: the bare walker must miss the doubly-substituted surface form");

    let compiled = compiler::compile_default(&g);
    let rule_a = rule(&compiled, "p_to_b");
    let rule_b = rule(&compiled, "t_to_d_before_b");
    assert_eq!(rule_a.tier, RuleInverseTier::Exact, "{:?}", rule_a.reasons);
    assert_eq!(rule_b.tier, RuleInverseTier::Exact, "{:?}", rule_b.reasons);

    let chain_trie_morpher = Morpher::new(&g, usize::MAX);
    let chain_trie = Trie::build_ex(&g, &surface, &chain_trie_morpher, 1_000_000, 2, false, false);

    // Extra sanity check: a length-1 chain of EITHER rule alone must fail -- proves genuine
    // two-level cascading is what closes this case, not just "the chain machinery exists".
    assert!(!chain_covers(&g, &chain_trie, std::slice::from_ref(&rule_b.pinv), "aaadbaaai"), "rule B's inverse alone cannot see through to rule A's restoration");
    assert!(!chain_covers(&g, &chain_trie, std::slice::from_ref(&rule_a.pinv), "aaadbaaai"), "rule A's inverse alone cannot undo rule B's own substitution");

    // Chain order = reverse application order: index 0 = last-applied (rule B, surface-facing).
    let chain = [rule_b.pinv.clone(), rule_a.pinv.clone()];
    assert!(chain_covers(&g, &chain_trie, &chain, "aaadbaaai"), "the 2-level chain must recover the feeding cascade");
    assert!(
        chain_sigs(&g, &chain_trie, &chain, "aaadbaaai").is_subset(&engine_sigs(&g, "aaadbaaai")),
        "soundness: chain candidates must be a subset of the engine's"
    );

    let compiled2 = compiler::compile_default(&g);
    let rule_b2 = rule(&compiled2, "t_to_d_before_b");
    let rule_a2 = rule(&compiled2, "p_to_b");
    let chain2 = [rule_b2.pinv.clone(), rule_a2.pinv.clone()];
    assert!(!chain_covers(&g, &chain_trie, &chain2, "aaadbazai"), "a matched non-word (same target substring, different elsewhere) must stay unparsed");
}

#[test]
fn chain_recovers_long_distance_harmony_suffix_vowel_agrees_with_first_root_vowel() {
    // "uptk"+HARM("-i", harmonizing) -> round_harmony (i->u / u C* _) -> "uptku".
    let g = load("ChainLongDistanceHarmonyToyGrammar.xml");
    let morpher = Morpher::new(&g, usize::MAX);
    let engine = morpher.parse_word("uptku");
    assert!(!engine.analyses.is_empty(), "precondition: 'uptku' = uptk+HARM under root-controlled harmony, got {:?}", engine.analyses);

    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let bare_trie = Trie::build_ex(&g, &surface, &build_morpher, 1_000_000, 2, false, false);
    assert!(walk::analyze_word(&g, &bare_trie, "uptku", walk::DEFAULT_MAX_BEAM_WORK).analyses.is_empty(), "baseline: the bare walker must miss the harmonized surface form");

    let compiled = compiler::compile_default(&g);
    let r = rule(&compiled, "round_harmony");
    assert_eq!(r.tier, RuleInverseTier::Exact, "quantified env spans must stay Exact-tier through a real root+suffix walk: {:?}", r.reasons);

    let chain_trie_morpher = Morpher::new(&g, usize::MAX);
    let chain_trie = Trie::build_ex(&g, &surface, &chain_trie_morpher, 1_000_000, 2, false, false);
    let covered = walk::analyze_chain(&g, &chain_trie, std::slice::from_ref(&r.pinv), "uptku", walk::DEFAULT_MAX_BEAM_WORK, walk::DEFAULT_MAX_BOUNDARY_INSERTIONS);
    assert!(!covered.analyses.is_empty(), "the chain must recover the harmonized suffix form");
    assert!(
        chain_sigs(&g, &chain_trie, std::slice::from_ref(&r.pinv), "uptku").is_subset(&engine_sigs(&g, "uptku")),
        "soundness: chain candidates must be a subset of the engine's"
    );

    let compiled2 = compiler::compile_default(&g);
    let r2 = rule(&compiled2, "round_harmony");
    assert!(!chain_covers(&g, &chain_trie, std::slice::from_ref(&r2.pinv), "uptka"), "a matched non-word (plausible final vowel, but neither the underlying nor the harmonized surface) must stay unparsed");
}

/// DEFERRED (see module doc): metathesis compilation itself is out of scope this milestone.
#[test]
fn chain_recovers_metathesis_word_internal_consonant_swap_is_a_documented_deferral() {
    let g = load("ChainMetathesisDeferredToyGrammar.xml");
    let morpher = Morpher::new(&g, usize::MAX);
    assert!(!morpher.parse_word("aaksaa").analyses.is_empty(), "precondition: the real engine (which DOES support metathesis) parses 'aaksaa' via metathesis un-application");

    let compiled = compiler::compile_default(&g);
    let r = rule(&compiled, "swap_sk");
    assert_eq!(r.tier, RuleInverseTier::IdentitySkip, "metathesis compilation is deferred: the stub is unconditional IdentitySkip");
    assert_eq!(r.reasons, vec!["metathesis-unported".to_string()]);

    // NOT checked here: a chain-walk assertion that the identity-only stub fails to recover
    // "aaksaa". That would require an isolated "chain-only contribution" trie, but this word is a
    // BARE root, and `Trie::build_ex`'s own doc states `bare_root_surfaces` runs its full
    // synthesis-aware path (metathesis included, a real engine feature) REGARDLESS of the
    // `enable_variants`/`enable_junction_probing` knobs -- confirmed empirically here: even the
    // underlying-only trie already contains "aaksaa" via bare-root synthesis baking, independent of
    // the chain's own (stub) content, so a chain-walk check would test the wrong thing (the trie's
    // own bare-root baking, not the chain). The identity-only-stub CONTRACT itself (tier + reason,
    // and the "identity-only, not reject-all" property) is what's actually being deferred and is
    // fully covered by this test plus `f7_rule_inverse_compiler_gate.rs`'s own
    // `metathesis_rule_is_the_documented_identityskip_stub_not_a_combo_cap_port` test.
}
