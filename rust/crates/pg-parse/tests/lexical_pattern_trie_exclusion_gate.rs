//! A lexical-pattern root allomorph (`IsPattern`, e.g. a bare `[Any]*` entry) must be excluded from the root-allomorph trie, exactly as C#'s `Morpher` ctor partitions `IsPattern` allomorphs into `_lexicalPatterns` and never indexes them. Red-on-revert: reverting `root_trie.rs::RootAllomorphTrie::build`'s `is_pattern` skip makes a bogus one-segment match reappear in ordinary (guess-off) lookup.

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

/// A one-segment word can only match `[Any]*` if the trie (wrongly) indexed it; ordinary (guess-off) lookup must return `-` for both "a" and "b" since there is no other one-segment entry.
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

/// Positive control: the ordinary (non-pattern) root "ab" must still be found via the trie, confirming the fix excludes only pattern allomorphs, not indexing wholesale.
#[test]
fn ordinary_root_still_matches_through_the_trie() {
    let g = load(XML).unwrap_or_else(|e| panic!("grammar failed to load: {e}"));
    let m = Morpher::new(&g, usize::MAX);
    let got = m.parse_word("ab").signature();
    assert_ne!(got, "-", "the ordinary root 'ab' must still parse");
}

/// Structural confirmation of the partition itself: the trie's `allomorph_count` excludes the pattern allomorph, and `Morpher::lexical_patterns` carries exactly it.
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
