//! P11 chunk 5: the end-to-end guesser wire-up, ported against a hand-transcribed grammar rather
//! than an oracle-verified fixture — the C# CLI (`hc.dll`'s `BatchCommand`) has **no `--guess`
//! flag** at all (`rust/docs/p11-guesser-api-design.md` §6's open question #2), so there is no
//! upstream tool to generate a golden TSV against, the same situation P9's Generation API faced
//! (`csharp_port_generation.rs`'s own module doc). This file is verified directly against the C#
//! unit test's own literal expected outcomes, not a byte-parity oracle diff.
//!
//! Ports `MorpherTests.AnalyzeWord_CanGuess_ReturnsCorrectAnalysis`
//! (`.worktrees/parse-opt/tests/SIL.Machine.Morphology.HermitCrab.Tests/MorpherTests.cs:242-274`).
//! The grammar: one V-requiring `ed_suffix` rule (`+d`, `<MorphemeId>PAST</MorphemeId>`, mirroring
//! the C# rule's `Gloss = "PAST"`) and one lexical-pattern entry `[Any]*` whose `Any` natural class
//! has an EMPTY feature struct (`new FeatureStruct()` in C#, `<FeatureNaturalClass><Name>Any</Name>
//! </FeatureNaturalClass>` here — no constraints, matches every segment) and NO `partOfSpeech`
//! attribute (empty syntactic FS, matching C#'s `AddEntry("pattern", new FeatureStruct(), ...)` —
//! `RootAllomorphDef::is_pattern` is true for it per P11 chunk 1, so it is excluded from the trie
//! per chunk 2 and lands in `Morpher::lexical_patterns` instead).
//!
//! **Rendering caveat (why this test doesn't compare literal strings to the C# assertions):** C#'s
//! `analyses[0].ToString()` is `WordAnalysis.ToString()` (`SIL.Machine/Morphology/WordAnalysis.cs:
//! 63-69`), a display formatter this port has no equivalent of — it prints an ordinary rule
//! morpheme's `Name` field (`HCRuleBase.ToString()`, "ed_suffix") and a `LexEntry`'s
//! `PrimaryAllomorph.Segments.ToString()` (the surface text, "gag"/"gagd" for the guessed root —
//! NOT its `Id`/`<MorphemeId>`). This is a *different* string format from `BatchCommand.
//! BuildSignature`'s `.Id`-based join, which is what `pg_parse::Morpher::parse_word`'s
//! `analyses`/`result_signature` mirrors (§4.5: the guess feature deliberately does not perturb
//! that format). So this test checks the SAME semantic content the C# assertions pin — root
//! position (the design doc's own footnote: "the `*` ... is `WordAnalysis.ToString`'s root-*index*
//! marker, not a guessed marker" — exactly `structured[i].root_morpheme_index` here), morph count,
//! and sort order — through this port's own join format (`guessed_root.text` for the root slot,
//! `<MorphemeId>` for the rule slot, joined with `+` — see `Morpher::morpheme_join`'s P11 sentinel
//! branch), not literal byte-identical strings to the C# `ToString()` output.

use pg_grammar::load;
use pg_parse::{Morpher, ParseOptions};

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

/// Sanity: the pattern entry really is a lexical pattern (chunk 1) and therefore excluded from
/// ordinary lexical lookup (chunk 2) — the two preconditions this whole test depends on.
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

/// C#: `Assert.That(morpher.AnalyzeWord("gag"), Is.Empty)` and `AnalyzeWord("gagd")` likewise —
/// guess OFF, no real lexicon entry can ever match (the pattern is never trie-indexed).
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

/// C#: `AnalyzeWord("gag", true)[0].ToString() == "[*gag]"` — one guess, the root alone (no
/// affix), root at index 0. Ported via this port's own join format (see module doc): the root
/// slot's morpheme_join text is the guessed root's rendered surface ("gag"), and `guessed` is set.
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
}

/// C#: `AnalyzeWord("gagd", true)[0].ToString() == "[*gag ed_suffix]"` — the design doc's
/// headline ordering pin: the 2-morph guess (`*gag` + `ed_suffix`, from analysis-unapplying
/// `ed_suffix` before guessing the residual root "gag") must sort BEFORE the also-produced
/// 1-morph guess (the whole "gagd" guessed as one bare root, from the un-unapplied analysis
/// candidate) — `parse_word_opts`'s descending-by-morph-count sort (§1.2).
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

    // Both analyses' surface renders MATCH the input word "gagd" (`Morpher::is_match` already
    // guarantees this structurally -- both survived that filter -- but the two underlying shapes
    // are genuinely different: [0]'s came from a real affix-rule application (leaving a residual
    // "+?" boundary-optional marker in the display regex), [1]'s is a bare guessed root with no
    // affix at all, so their `to_regex_display` strings differ even though both denote "gagd".
    assert_eq!(
        outcome.analyses[0].1, "gag+?d",
        "the affix path leaves the inserted boundary visible in the display regex"
    );
    assert_eq!(
        outcome.analyses[1].1, "gagd",
        "the bare-root guess renders the surface plainly"
    );
}
