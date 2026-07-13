//! Port of C# `BoundaryTapeChainTests.cs` (I4 of `FST_FULL_GRAMMAR_PLAN.md`, "the boundary tape"):
//! end-to-end toy tests of (a) a boundary-conditioned rule's env fragment gating correctly via the
//! chain's "insert boundary" move, and (b) that move's global per-word cap
//! (`max_boundary_insertions`) being structurally hang-proof.
//!
//! Authoring lesson applied from the start (see each fixture's own header): a natural-class
//! (`SimpleContext`) pattern node unconditionally pins the synthetic `Type` feature to `Segment`
//! (`F7LockstepComposedToyGrammar.xml`'s own established finding), so it can never match a real
//! `Boundary` node. Referencing a boundary in a rule's environment therefore needs a
//! `<BoundaryMarker boundary="...">` node (resolves to `PatternNode::CharDef`, which carries the
//! boundary char-def's own real lanes, Type=Boundary included) -- not a natural class. Both
//! fixtures below use `BoundaryMarker`, not `SimpleContext`, for the boundary reference.

use hc_hybrid::compiler::{self, RuleInverseTier};
use hc_hybrid::surface::SurfacePhonology;
use hc_hybrid::trie::Trie;
use hc_hybrid::walk;
use hc_parse::Morpher;

/// C# `Sig(WordAnalysis).IsSubsetOf(engine)` soundness check, ported via `xml_key` (same
/// convention `replay::signature`/`composite::candidate_signature` use).
fn engine_sigs(g: &hc_grammar::model::Grammar, word: &str) -> std::collections::HashSet<String> {
    Morpher::new(g, usize::MAX)
        .parse_word(word)
        .structured
        .iter()
        .map(|wa| {
            let keys: Vec<&str> = wa
                .morpheme_ids
                .iter()
                .map(|&id| g.morphemes[id as usize].xml_key.as_str())
                .collect();
            format!("{}:{}", keys.join("+"), wa.root_morpheme_index)
        })
        .collect()
}

fn chain_sigs_capped(
    g: &hc_grammar::model::Grammar,
    trie: &Trie,
    chain: &[hc_hybrid::inverse::InversePhonology],
    word: &str,
    max_boundary_insertions: i32,
) -> std::collections::HashSet<String> {
    walk::analyze_chain(
        g,
        trie,
        chain,
        word,
        walk::DEFAULT_MAX_BEAM_WORK,
        max_boundary_insertions,
    )
    .analyses
    .iter()
    .map(|wa| {
        let keys: Vec<&str> = wa
            .morphemes
            .iter()
            .map(|&hc_grammar::model::MorphemeId(id)| g.morphemes[id as usize].xml_key.as_str())
            .collect();
        format!("{}:{}", keys.join("+"), wa.root_index)
    })
    .collect()
}

fn load(fixture: &str) -> hc_grammar::model::Grammar {
    let xml = std::fs::read_to_string(format!(
        "{}/tests/fixtures/fst-advisor-toys/{fixture}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("read {fixture}: {e}"));
    hc_grammar::load(&xml).unwrap_or_else(|e| panic!("{fixture} failed to load: {e}"))
}

#[test]
fn chain_recovers_boundary_conditioned_substitution_after_morpheme_junction() {
    // "sa+"+"pata" = "sa+pata" -> voice_after_boundary (p->b / +_) -> surface "sabata".
    let g = load("BoundaryTapeChainSubstitutionToyGrammar.xml");
    let morpher = Morpher::new(&g, usize::MAX);
    assert!(
        !morpher.parse_word("sabata").analyses.is_empty(),
        "precondition: the toy engine parses 'sabata' as SA-PFX + pata, voiced after the boundary"
    );

    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let bare_trie = Trie::build_ex(&g, &surface, &build_morpher, 1_000_000, 2, false, false);
    assert!(
        walk::analyze_word(&g, &bare_trie, "sabata", walk::DEFAULT_MAX_BEAM_WORK)
            .analyses
            .is_empty(),
        "baseline: the bare walker must miss the boundary-voiced surface form"
    );

    let compiled = compiler::compile_default(&g);
    let r = compiled
        .iter()
        .find(|r| r.name == "voice_after_boundary")
        .expect("rule compiled");
    assert_eq!(
        r.tier,
        RuleInverseTier::Exact,
        "a boundary-only-gated substitution must compile Exact: {:?}",
        r.reasons
    );

    let chain_trie_morpher = Morpher::new(&g, usize::MAX);
    let chain_trie = Trie::build_ex(
        &g,
        &surface,
        &chain_trie_morpher,
        1_000_000,
        2,
        false,
        false,
    );
    let covered = walk::analyze_chain(
        &g,
        &chain_trie,
        std::slice::from_ref(&r.pinv),
        "sabata",
        walk::DEFAULT_MAX_BEAM_WORK,
        walk::DEFAULT_MAX_BOUNDARY_INSERTIONS,
    );
    assert!(
        !covered.analyses.is_empty(),
        "the chain must recover the boundary-conditioned voicing via the insert-boundary move"
    );
    assert!(
        chain_sigs_capped(
            &g,
            &chain_trie,
            std::slice::from_ref(&r.pinv),
            "sabata",
            walk::DEFAULT_MAX_BOUNDARY_INSERTIONS
        )
        .is_subset(&engine_sigs(&g, "sabata")),
        "soundness: chain candidates must be a subset of the engine's"
    );

    // A matched non-word (differs from the lexicon entry "pata" after the shared prefix) must stay
    // unparsed. NOTE (matching the C# original's own note): "sapata" (the literal unvoiced
    // concatenation) is deliberately NOT used -- the compiled Pinv is a sound superset whose
    // identity self-loop also accepts an unvoiced "p" as a spurious reading, which a real verify
    // step prunes, not a bare chain-walk call.
    let compiled2 = compiler::compile_default(&g);
    let r2 = compiled2
        .iter()
        .find(|r| r.name == "voice_after_boundary")
        .expect("rule compiled");
    let non_word = walk::analyze_chain(
        &g,
        &chain_trie,
        std::slice::from_ref(&r2.pinv),
        "sabala",
        walk::DEFAULT_MAX_BEAM_WORK,
        walk::DEFAULT_MAX_BOUNDARY_INSERTIONS,
    );
    assert!(non_word.analyses.is_empty(), "a matched non-word (differs from the lexicon entry after the shared prefix) must stay unparsed");
}

#[test]
fn chain_respects_insertion_cap_two_boundary_crossings_never_hangs() {
    // "sa+ti+"+"pata" = "sa+ti+pata" -> surface "satibata" -- TWO boundary crossings needed.
    let g = load("BoundaryTapeChainCapToyGrammar.xml");
    let morpher = Morpher::new(&g, usize::MAX);
    assert!(!morpher.parse_word("satibata").analyses.is_empty(), "precondition: the toy engine parses 'satibata' = sa+ti+pata, voiced after the 2nd boundary");

    let surface = SurfacePhonology::new(&g);
    let compiled = compiler::compile_default(&g);
    let r = compiled
        .iter()
        .find(|r| r.name == "voice_after_boundary_cap")
        .expect("rule compiled");
    assert_eq!(r.tier, RuleInverseTier::Exact, "{:?}", r.reasons);

    let chain_trie_morpher = Morpher::new(&g, usize::MAX);
    let chain_trie = Trie::build_ex(
        &g,
        &surface,
        &chain_trie_morpher,
        1_000_000,
        2,
        false,
        false,
    );

    // Cap 1: cannot cross both boundary arcs -- must fall to unparsed FAST, never hang.
    let start = std::time::Instant::now();
    let under_capped = walk::analyze_chain(
        &g,
        &chain_trie,
        std::slice::from_ref(&r.pinv),
        "satibata",
        walk::DEFAULT_MAX_BEAM_WORK,
        1,
    );
    let elapsed = start.elapsed();
    assert!(
        under_capped.analyses.is_empty(),
        "cap 1 cannot pay for both boundary crossings -- must fall to unparsed"
    );
    assert!(
        elapsed.as_secs() < 5,
        "the cap must turn a would-be-hang into a fast 'unparsed', not a slow one"
    );

    // A generous cap (4) covers it.
    let compiled2 = compiler::compile_default(&g);
    let r2 = compiled2
        .iter()
        .find(|r| r.name == "voice_after_boundary_cap")
        .expect("rule compiled");
    let covered = walk::analyze_chain(
        &g,
        &chain_trie,
        std::slice::from_ref(&r2.pinv),
        "satibata",
        walk::DEFAULT_MAX_BEAM_WORK,
        4,
    );
    assert!(
        !covered.analyses.is_empty(),
        "a generous cap (4) covers both boundary crossings"
    );
    assert!(
        chain_sigs_capped(
            &g,
            &chain_trie,
            std::slice::from_ref(&r2.pinv),
            "satibata",
            4
        )
        .is_subset(&engine_sigs(&g, "satibata")),
        "soundness: chain candidates must be a subset of the engine's"
    );

    // A matched non-word must stay unparsed even under the generous cap.
    let compiled3 = compiler::compile_default(&g);
    let r3 = compiled3
        .iter()
        .find(|r| r.name == "voice_after_boundary_cap")
        .expect("rule compiled");
    let non_word = walk::analyze_chain(
        &g,
        &chain_trie,
        std::slice::from_ref(&r3.pinv),
        "satibala",
        walk::DEFAULT_MAX_BEAM_WORK,
        4,
    );
    assert!(non_word.analyses.is_empty(), "a matched non-word (differs from the lexicon entry after the shared prefix) must stay unparsed");
}
