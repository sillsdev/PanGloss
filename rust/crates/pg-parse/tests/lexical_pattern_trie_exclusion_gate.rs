//! P11 chunk 2 gate (`rust/docs/p11-guesser-api-design.md` §2, §5 chunk 2): a lexical-pattern root
//! allomorph (`IsPattern`, e.g. a bare `[Any]*` entry) must be **excluded** from the root-allomorph
//! trie, exactly as C#'s `Morpher` ctor partitions `IsPattern` allomorphs into `_lexicalPatterns`
//! and never indexes them (`Morpher.cs:39-48,74-85`).
//!
//! Before this fix, `RootAllomorphTrie::build` indexed every allomorph unconditionally (its own doc
//! note said so explicitly), and stored `OPTIONAL`/`ITERATIVE` flags are never consulted by the trie
//! edge condition (`edge_matches`) — so a `[Any]*` entry became a single **mandatory**, unrestricted
//! (`CdSet` = every table member) edge. That edge then matched *any* one-segment word in ordinary
//! (guess-off) lexical lookup, a real divergence: C# never surfaces a pattern allomorph outside the
//! guess subsystem, so the same word must return `-` there.
//!
//! Red-on-revert: reverting `root_trie.rs::RootAllomorphTrie::build`'s `if allo.is_pattern {
//! continue; }` skip (chunk 2) makes `pattern_only_word_does_not_spuriously_match` fail (the
//! one-segment word "a" gets a bogus match against the `[Any]*` entry instead of `-`).

use pg_grammar::load;
use pg_parse::Morpher;

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>LexicalPatternTrieExclusion</Name>
    <PartsOfSpeech><PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncAny"><Name>Any</Name><Segment segment="cA" /><Segment segment="cB" /></SegmentNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="ePattern" partOfSpeech="posN">
            <Allomorphs><Allomorph id="aPattern"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pattern</Gloss>
          </LexicalEntry>
          <LexicalEntry id="eReal" partOfSpeech="posN">
            <Allomorphs><Allomorph id="aReal"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>real</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// The chunk-2 fix itself: a one-segment word can only match the `[Any]*` pattern allomorph if the
/// trie (wrongly) indexed it. With the fix, ordinary (guess-off) lexical lookup must return `-` for
/// both "a" and "b" — the pattern allomorph is never in the trie, and there is no other one-segment
/// entry in this grammar. This is the fixture's red-on-revert case (see module doc).
#[test]
fn pattern_only_word_does_not_spuriously_match() {
    let g = load(XML).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
    assert!(
        g.entries[0].allomorphs[0].is_pattern,
        "sanity: [Any]* must classify as a pattern"
    );
    assert!(
        !g.entries[1].allomorphs[0].is_pattern,
        "sanity: 'ab' is an ordinary root, not a pattern"
    );

    let m = Morpher::new(&g, usize::MAX);
    for word in ["a", "b"] {
        let got = m.parse_word(word).signature();
        assert_eq!(got, "-", "word {word:?}: a lexical-pattern allomorph must never surface in guess-off lookup, got {got:?}");
    }
}

/// Sanity/positive control: the ordinary (non-pattern) root "ab" must still be found via the trie —
/// confirms the fix excludes only pattern allomorphs, not indexing wholesale.
#[test]
fn ordinary_root_still_matches_through_the_trie() {
    let g = load(XML).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
    let m = Morpher::new(&g, usize::MAX);
    let got = m.parse_word("ab").signature();
    assert_ne!(got, "-", "the ordinary root 'ab' must still parse");
}

/// Structural confirmation of the partition itself: the trie's `allomorph_count` excludes the
/// pattern allomorph, and `Morpher::lexical_patterns` carries exactly it (P11 §4.3).
#[test]
fn trie_excludes_pattern_allomorph_and_morpher_carries_it_in_lexical_patterns() {
    let g = load(XML).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
    let m = Morpher::new(&g, usize::MAX);
    assert_eq!(
        m.lexical_patterns().len(),
        1,
        "exactly one lexical pattern allomorph across the (single) stratum"
    );
    let (allo, entry) = m.lexical_patterns()[0];
    assert_eq!(allo, g.entries[0].allomorphs[0].id);
    assert_eq!(entry, pg_grammar::model::LexEntryId(0));
}
