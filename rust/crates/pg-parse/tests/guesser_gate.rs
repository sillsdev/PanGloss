//! Ports `MorpherTests.AnalyzeWord_CanGuess_ReturnsCorrectAnalysis` against a hand-transcribed grammar, since the C# CLI has no `--guess` flag to generate a golden TSV against; verified directly against the C# unit test's literal expected outcomes, checking the same semantic content (root position, morph count, sort order) through this port's own join format rather than C#'s `ToString()` byte-identical strings.

use pg_grammar::load;
use pg_parse::{AnalysisProvenance, Morpher, ParseOptions};

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>AnalyzeWordCanGuess</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrEd">
        <Name>Morphophonemic</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV">
            <Name>ed_suffix</Name>
            <MorphemeId>PAST</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subEd">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="ePattern">
            <Allomorphs><Allomorph id="aPattern"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pattern</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn grammar() -> pg_grammar::model::Grammar {
    load(XML).unwrap_or_else(|e| panic!("guesser fixture grammar failed to load: {e}"))
}

/// Sanity: the pattern entry really is a lexical pattern, and therefore excluded from ordinary lexical lookup -- the two preconditions this file depends on.
#[test]
fn fixture_sanity_pattern_entry_is_excluded_from_the_trie() {
    let g = grammar();
    assert_eq!(g.entries.len(), 1);
    assert!(
        g.entries[0].allomorphs[0].is_pattern,
        "[Any]* must classify as a pattern (chunk 1)"
    );
    assert!(
        g.entries[0].syn_fs == pg_featstruct::FsId(0),
        "the pattern entry's syn FS must be empty"
    );

    let m = Morpher::new(&g, usize::MAX);
    assert_eq!(
        m.lexical_patterns().len(),
        1,
        "the pattern must land in Morpher::lexical_patterns"
    );
}

/// PORT-CORRESPONDENCE: guess off, no real lexicon entry can ever match since the pattern is never trie-indexed.
#[test]
fn guess_off_both_words_have_no_analyses() {
    let g = grammar();
    let m = Morpher::new(&g, usize::MAX);
    assert_eq!(m.parse_word("gag").signature(), "-");
    assert_eq!(m.parse_word("gagd").signature(), "-");
    // parse_word_opts with guess_root=false must be byte-identical to parse_word (§4.1).
    let opts_off = ParseOptions::default().with_guess_root(false);
    assert_eq!(m.parse_word_opts("gag", &opts_off).signature(), "-");
    assert!(!m.parse_word_opts("gag", &opts_off).guessed);
}

/// PORT-CORRESPONDENCE: one guess, the root alone (no affix) at index 0, via this port's own join format rather than C#'s `ToString()`.
#[test]
fn guess_on_gag_has_exactly_one_analysis_root_only() {
    let g = grammar();
    let m = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default().with_guess_root(true);
    let outcome = m.parse_word_opts("gag", &opts);

    assert!(
        outcome.guessed,
        "the guess branch must have fired (normal path was empty)"
    );
    assert_eq!(
        outcome.analyses.len(),
        1,
        "exactly one guess for \"gag\": {:?}",
        outcome.analyses
    );
    assert_eq!(
        outcome.analyses[0].0, "gag",
        "root-only join: just the guessed root's rendered text"
    );
    assert_eq!(outcome.structured.len(), 1);
    assert_eq!(
        outcome.structured[0].root_morpheme_index, 0,
        "the root is morph index 0 (C#'s '*' marker)"
    );
    assert_eq!(outcome.structured[0].morpheme_ids.len(), 1);
    assert_eq!(
        outcome.structured[0].morpheme_ids[0],
        u32::MAX,
        "the sentinel MorphemeId::GUESSED value"
    );
    assert!(outcome.structured[0].guessed);
    assert_eq!(
        outcome.structured[0].provenance,
        AnalysisProvenance::Guessed
    );
}

/// PORT-CORRESPONDENCE: the 2-morph guess (`*gag`+`ed_suffix`) must sort before the 1-morph guess (bare "gagd"), by `parse_word_opts`'s descending-by-morph-count sort.
#[test]
fn guess_on_gagd_has_two_analyses_two_morph_first() {
    let g = grammar();
    let m = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default().with_guess_root(true);
    let outcome = m.parse_word_opts("gagd", &opts);

    assert!(outcome.guessed);
    assert_eq!(
        outcome.analyses.len(),
        2,
        "two coexisting guesses: {:?}",
        outcome.analyses
    );
    assert_eq!(outcome.structured.len(), 2);

    // [0]: the 2-morph guess -- root "gag" (index 0) + the PAST-suffix rule.
    assert_eq!(
        outcome.analyses[0].0, "gag+PAST",
        "2-morph join: guessed root text + the rule's MorphemeId"
    );
    assert_eq!(outcome.structured[0].morpheme_ids.len(), 2);
    assert_eq!(outcome.structured[0].root_morpheme_index, 0);
    assert_eq!(
        outcome.structured[0].morpheme_ids[0],
        u32::MAX,
        "the guessed root's sentinel id"
    );
    assert_ne!(
        outcome.structured[0].morpheme_ids[1],
        u32::MAX,
        "the PAST suffix is a REAL morpheme, not guessed"
    );

    // [1]: the 1-morph guess -- the whole surface word "gagd" guessed as one bare root.
    assert_eq!(
        outcome.analyses[1].0, "gagd",
        "1-morph join: just the guessed root's rendered text"
    );
    assert_eq!(outcome.structured[1].morpheme_ids.len(), 1);
    assert_eq!(outcome.structured[1].root_morpheme_index, 0);
    assert_eq!(outcome.structured[1].morpheme_ids[0], u32::MAX);

    // Both are marked guessed (the branch is all-or-nothing, §4.1).
    assert!(outcome.structured[0].guessed && outcome.structured[1].guessed);

    // Both surface renders match "gagd", but their underlying shapes differ: [0]'s affix-rule application leaves a residual boundary-optional marker in the display regex, [1]'s bare root has none.
    assert_eq!(
        outcome.analyses[0].1, "gag+?d",
        "the affix path leaves the inserted boundary visible in the display regex"
    );
    assert_eq!(
        outcome.analyses[1].1, "gagd",
        "the bare-root guess renders the surface plainly"
    );
}
