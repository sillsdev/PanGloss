//! F6 toy-grammar tests (HYBRID_FST_RUST_PLAN.md §8/§9): ports of the SHAPE of
//! `VerifiedFstAnalyzerTests.cs`'s `Composite_CoversFullReduplication_WhereFstAloneMisses`,
//! `Composite_CoversSeparatorReduplication_WhereFstAloneMisses`,
//! `Composite_CoversSuffixStackedOutsideReduplication_WhereSeparatorScanAloneMisses`, and
//! `Composite_CoversInfixation_WhereFstAloneMisses` (`fst-oracle` branch), run against a
//! hand-authored minimal grammar (`fixtures/fst-advisor-toys/F6ProposersToyGrammar.xml` -- see that
//! file's own header comment for why it is hand-authored rather than a C#-exported round-trip).
//!
//! Each test follows the same shape as its C# original: (1) confirm the PRECONDITION -- the
//! unrestricted engine (`Morpher::parse_word`) actually analyzes the target word as the intended
//! derivation; (2) confirm the BARE FST proposer alone misses it (the construct the sibling
//! proposer exists for); (3) confirm the composite (bare FST + siblings) DOES cover it, with the
//! verified analysis matching the engine; (4) a soundness check -- a near-miss non-word must still
//! verify empty.

use hc_grammar::model::Grammar;
use hc_hybrid::composite::CompositeAnalyzer;
use hc_hybrid::replay;
use hc_hybrid::surface::SurfacePhonology;
use hc_hybrid::trie::Trie;
use hc_hybrid::walk;
use hc_parse::Morpher;

const FIXTURE: &str = include_str!("fixtures/fst-advisor-toys/F6ProposersToyGrammar.xml");

fn load() -> Grammar {
    hc_grammar::load(FIXTURE)
        .unwrap_or_else(|e| panic!("F6ProposersToyGrammar.xml failed to load: {e}"))
}

fn build(g: &Grammar) -> (SurfacePhonology<'_>, Trie) {
    let surface = SurfacePhonology::new(g);
    let build_morpher = Morpher::new(g, usize::MAX);
    let trie = Trie::build(g, &surface, &build_morpher, 1_000_000, 2, true);
    (surface, trie)
}

/// C# `Composite_CoversFullReduplication_WhereFstAloneMisses`: "sagsag" = RED("sag"). The bare FST
/// cannot represent full reduplication at all; the composite covers it via `ReduplicationProposer`'s
/// full/partial-copy scan.
#[test]
fn full_reduplication_covered_by_composite_not_bare_fst() {
    let g = load();
    let (surface, trie) = build(&g);
    let search = Morpher::new(&g, usize::MAX);

    let precondition = search.parse_word("sagsag");
    assert!(
        !precondition.structured.is_empty(),
        "precondition: 'sagsag' must analyze as RED(sag)"
    );

    let bare = walk::analyze_word(&g, &trie, "sagsag", walk::DEFAULT_MAX_BEAM_WORK);
    assert!(
        bare.analyses.is_empty(),
        "baseline: the bare FST alone cannot represent reduplication"
    );

    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);
    let verified = composite.analyze_word_verified(&verify_morpher, &owners, "sagsag");
    assert!(
        !verified.is_empty(),
        "reduplication not covered by the composite"
    );

    // Soundness: "sasag" has an incidental short prefix repeat ("sa"+"sag") but this grammar's RED
    // rule only ever produces base+base -- a coincidental prefix repeat must not verify.
    let unsound = composite.analyze_word_verified(&verify_morpher, &owners, "sasag");
    assert!(
        unsound.is_empty(),
        "a coincidental short prefix repeat must not verify: {unsound:?}"
    );
}

/// C# `Composite_CoversSeparatorReduplication_WhereFstAloneMisses`: "sagzsag" = CONT("sag") (a
/// literal-separator copy). Exercises the separator+tail-copy scan.
#[test]
fn separator_reduplication_covered_by_composite() {
    let g = load();
    let (surface, trie) = build(&g);
    let search = Morpher::new(&g, usize::MAX);

    assert!(
        !search.parse_word("sagzsag").structured.is_empty(),
        "precondition: 'sagzsag' must analyze as CONT(sag)"
    );
    let bare = walk::analyze_word(&g, &trie, "sagzsag", walk::DEFAULT_MAX_BEAM_WORK);
    assert!(
        bare.analyses.is_empty(),
        "baseline: the bare FST alone cannot represent separator reduplication"
    );

    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);
    let verified = composite.analyze_word_verified(&verify_morpher, &owners, "sagzsag");
    assert!(
        !verified.is_empty(),
        "separator reduplication not covered by the composite"
    );

    // Soundness: "sagzag" looks like a tail-copy candidate (the separator scan proposes residual
    // "sag") but this grammar's CONT rule only ever produces a FULL copy -- must not verify.
    let unsound = composite.analyze_word_verified(&verify_morpher, &owners, "sagzag");
    assert!(
        unsound.is_empty(),
        "a tail-copy candidate must not verify against a full-copy-only rule: {unsound:?}"
    );
}

/// C# `Composite_CoversSuffixStackedOutsideReduplication_WhereSeparatorScanAloneMisses`:
/// "sagzsags" = TRL(CONT("sag")) -- a suffix stacked OUTSIDE the reduplicated form, landing on the
/// tail of the copy. Exercises the Phase G1 separator+suffix-peel scan specifically.
#[test]
fn suffix_stacked_outside_reduplication_covered_by_composite() {
    let g = load();
    let (surface, trie) = build(&g);
    let search = Morpher::new(&g, usize::MAX);

    assert!(
        !search.parse_word("sagzsags").structured.is_empty(),
        "precondition: 'sagzsags' must analyze as TRL(CONT(sag))"
    );

    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);
    let verified = composite.analyze_word_verified(&verify_morpher, &owners, "sagzsags");
    assert!(
        !verified.is_empty(),
        "suffix stacked outside reduplication not covered (Phase G1 suffix-peel scan)"
    );
}

/// C# `Composite_CoversInfixation_WhereFstAloneMisses`: "saag" = INF("sag") (s . a . ag). The bare
/// FST recognizes but does not build infix slots; `InfixProposer` covers it.
#[test]
fn infixation_covered_by_composite_not_bare_fst() {
    let g = load();
    let (surface, trie) = build(&g);
    let search = Morpher::new(&g, usize::MAX);

    assert!(
        !search.parse_word("saag").structured.is_empty(),
        "precondition: 'saag' must analyze as INF(sag)"
    );
    let bare = walk::analyze_word(&g, &trie, "saag", walk::DEFAULT_MAX_BEAM_WORK);
    assert!(
        bare.analyses.is_empty(),
        "baseline: the bare FST alone does not build infix slots"
    );

    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);
    let verified = composite.analyze_word_verified(&verify_morpher, &owners, "saag");
    assert!(
        !verified.is_empty(),
        "infixation not covered by the composite"
    );

    let unsound = composite.analyze_word_verified(&verify_morpher, &owners, "zzz");
    assert!(
        unsound.is_empty(),
        "soundness: a non-word must still verify empty: {unsound:?}"
    );
}

/// C# `Composite_WiresGenerators_ReduplicatingGrammarMatchesEngine`-style integration check + the
/// `CompositeProposer.CoversAllConstructs` assertions from the redup/infix unit tests: this grammar's
/// FST alone leaves Reduplication/Infix uncovered, and the composite (with both sibling proposers
/// wired) covers both.
#[test]
fn composite_covers_all_constructs_this_grammar_uses() {
    let g = load();
    let (surface, trie) = build(&g);
    assert!(
        !trie.covers_all_constructs(),
        "the bare FST alone must NOT cover redup/infix in this grammar"
    );

    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);
    assert!(
        composite.covers_all_constructs(),
        "the composite must cover every construct this grammar uses"
    );
}
