//! Projects `pg_parse::WordAnalysis` to a stable-key identity that survives a dense-ordinal reshuffle from an unrelated grammar edit (ADR 0006).

use pg_assess::{AnalysisIdentity, AnalysisSet};
use pg_grammar::model::Grammar;
use pg_parse::morpher::Morpher;
use pg_parse::ParseOptions;

/// Two entries sharing the surface form `ab`, differing only in part of speech.
const BASELINE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>IdentityFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryA" partOfSpeech="posN">
            <Allomorphs><Allomorph id="alloA"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>noun-root</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryB" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloB"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>verb-root</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// The same grammar with one part of speech and one lexical entry inserted *ahead* of the existing ones.
const AFTER_INSERTION_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>IdentityFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posAdj"><Name>Adj</Name></PartOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cZ"><Representations><Representation>z</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryNew" partOfSpeech="posAdj">
            <Allomorphs><Allomorph id="alloNew"><PhoneticShape>zz</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>adj-root</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryA" partOfSpeech="posN">
            <Allomorphs><Allomorph id="alloA"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>noun-root</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryB" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloB"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>verb-root</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("failed to load fixture: {e}"))
}

/// Parse `word` and project every analysis to a stable-key identity.
fn identities(grammar: &Grammar, word: &str) -> AnalysisSet {
    let morpher = Morpher::new(grammar, usize::MAX);
    let outcome = morpher.parse_word_opts(word, &ParseOptions::default());
    AnalysisSet::from_observed(
        outcome
            .structured
            .iter()
            .map(|a| AnalysisIdentity::project(a, grammar).expect("analysis projects")),
    )
}

/// The raw dense ordinals the projection exists to replace.
fn raw_ordinals(grammar: &Grammar, word: &str) -> Vec<(Vec<u32>, Option<u32>)> {
    let morpher = Morpher::new(grammar, usize::MAX);
    let outcome = morpher.parse_word_opts(word, &ParseOptions::default());
    let mut out: Vec<_> = outcome
        .structured
        .iter()
        .map(|a| (a.morpheme_ids.clone(), a.pos_id))
        .collect();
    out.sort();
    out
}

#[test]
fn identities_carry_authored_keys_not_ordinals() {
    let grammar = load(BASELINE_XML);
    let set = identities(&grammar, "ab");

    assert_eq!(set.len(), 2, "both entries should analyze `ab`");

    let mut seen: Vec<(Vec<Option<String>>, Option<String>)> = set
        .entries()
        .iter()
        .map(|e| (e.identity.morphemes.clone(), e.identity.category.clone()))
        .collect();
    seen.sort();

    assert_eq!(
        seen,
        vec![
            (vec![Some("entryA".to_string())], Some("posN".to_string())),
            (vec![Some("entryB".to_string())], Some("posV".to_string())),
        ],
        "morpheme and category keys must be the authored XML ids"
    );
}

#[test]
fn dense_ordinals_shift_but_identities_do_not() {
    // The document-order tables behind `MorphemeId` and `pos_id` both shift on insertion; `ab`'s analyses must come out identical anyway.
    let baseline = load(BASELINE_XML);
    let candidate = load(AFTER_INSERTION_XML);

    assert_ne!(
        raw_ordinals(&baseline, "ab"),
        raw_ordinals(&candidate, "ab"),
        "the fixture is pointless unless the insertion really does shift dense ordinals"
    );

    assert_eq!(
        identities(&baseline, "ab"),
        identities(&candidate, "ab"),
        "an unrelated insertion must not change the identity of an untouched analysis"
    );
}

#[test]
fn a_deleted_entry_is_ordinary_removed_evidence() {
    // The candidate no longer defines `entryB`, so its analysis is simply absent; a baseline identity holds its own keys and never consults the candidate model.
    let baseline = load(BASELINE_XML);
    let candidate = load(&BASELINE_XML.replace(
        r#"          <LexicalEntry id="entryB" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloB"><PhoneticShape>ab</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>verb-root</Gloss>
          </LexicalEntry>
"#,
        "",
    ));

    let before = identities(&baseline, "ab");
    let after = identities(&candidate, "ab");
    assert_eq!(before.len(), 2);
    assert_eq!(after.len(), 1);

    let removed: Vec<_> = before
        .entries()
        .iter()
        .filter(|e| !after.contains(&e.identity))
        .collect();
    assert_eq!(removed.len(), 1);
    assert_eq!(
        removed[0].identity.morphemes,
        vec![Some("entryB".to_string())],
        "the deleted entry's analysis is `removed`, not a comparison failure"
    );
}

#[test]
fn identities_survive_the_model_that_produced_them() {
    // A report outlives its grammar: once projected, an identity is a plain value comparable long after the `Grammar` is dropped.
    let retained = {
        let grammar = load(BASELINE_XML);
        identities(&grammar, "ab")
    };
    assert_eq!(retained.len(), 2);
    assert!(retained.contains(&AnalysisIdentity {
        morphemes: vec![Some("entryA".to_string())],
        root_index: 0,
        category: Some("posN".to_string()),
    }));
}
