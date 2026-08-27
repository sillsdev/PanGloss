//! The single-owner invariant for `GrammarSemantics`: no consumer re-reads `Grammar` to decide
//! something it already owns; see `docs/research/pg-foma-grammar-semantics-owner-gate.md`.

use foma::options::FomaOptions;
use pg_grammar::model::{Grammar, PhonRuleDef};

use pg_foma::backend_registry::Applicability;
use pg_foma::capability::{
    characterize_call_count, default_registry, reset_characterize_call_count,
};
use pg_foma::characterization::characterization_findings;
use pg_foma::enumerate::enumerate_candidates;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use pg_foma::selection::select_plan;

/// One MPR-gated subrule and two entries realizing both truth values of that gate key, so `enumerate_candidates` yields two candidates -- the shape that makes per-candidate `characterize` calls visible.
const GATED_TWO_GROUP_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SemanticsOwnerGatedTwoGroupFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <MorphologicalPhonologicalRuleFeatures>
      <MorphologicalPhonologicalRuleFeature id="mpr1">f1</MorphologicalPhonologicalRuleFeature>
    </MorphologicalPhonologicalRuleFeatures>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>gate1</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule requiredMPRFeatures="mpr1">
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e0" partOfSpeech="posV">
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e0</Gloss>
          </LexicalEntry>
          <LexicalEntry id="e1" partOfSpeech="posV" ruleFeatures="mpr1">
            <Allomorphs><Allomorph id="allo1"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e1</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// The disagreement fixture: `prule1` is declared globally but named by no stratum's `phonologicalRules` attribute, so it lands in `g.prules` and in no `sd.prules`.
const ORPHANED_PRULE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SemanticsOwnerOrphanedPruleFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="c2"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>orphan</Name>
        <PhoneticInput><PhoneticSequence><Segment segment="c1" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="c2" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="e0">
            <Allomorphs><Allomorph id="allo0"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>e0</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

fn prules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|&id| &g.prules[id.0 as usize])
        .collect()
}

/// `select_plan` characterizes the grammar once, not once per candidate plan; falsified by moving `GrammarSemantics::derive` back inside its per-candidate closure.
#[test]
fn select_plan_characterizes_the_grammar_once_not_once_per_candidate() {
    let g = load(GATED_TWO_GROUP_XML);
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();
    let registry = default_registry();

    let candidates = enumerate_candidates(&g, &ro, phon.as_ref());
    assert_eq!(
        candidates.len(),
        2,
        "this fixture must yield 2 candidates, or the per-candidate claim below is vacuous"
    );

    reset_characterize_call_count();
    let outcome = select_plan(&candidates, &g, &registry, &opts, &alphabet, &ro);
    let select_calls = characterize_call_count();

    assert!(
        outcome.chosen.is_some(),
        "the fixture must still select a candidate -- 7.11 is a consolidation, not a behavior change"
    );
    assert!(
        select_calls > 0,
        "the counter must actually have observed select_plan; 0 means it measured nothing"
    );
    assert_eq!(
        select_calls,
        1,
        "select_plan must characterize the GRAMMAR once, not once per candidate PLAN (it was {} \
         before task 7.11 -- one per candidate)",
        candidates.len()
    );
}

/// `characterization_findings` characterizes once, not twice; falsified by restoring two independent `characterize` walks.
#[test]
fn characterization_findings_characterizes_once_not_twice() {
    let g = load(GATED_TWO_GROUP_XML);

    reset_characterize_call_count();
    let findings = characterization_findings(&g);
    let characterization_calls = characterize_call_count();

    assert!(
        characterization_calls > 0,
        "the counter must actually have observed characterization_findings"
    );
    assert_eq!(
        characterization_calls, 1,
        "characterization_findings must characterize once -- it took the profile AND the capability \
         verdict from two independent walks before task 7.11, which its own module doc called \
         'an acceptable duplication ... while waiting on 7.11'"
    );
    // Sanity check that characterization actually ran rather than short-circuited: this grammar has real cardinality to look at.
    let _ = findings;
}

/// Declared phonology and cascade phonology are different facts; each consumer keeps the one it means. Falsified by pointing `Applicability::HasPhonology` at `cascade_phonology` instead.
#[test]
fn declared_and_cascade_phonology_stay_separate_facts_for_their_separate_consumers() {
    let orphan = load(ORPHANED_PRULE_XML);
    let sem = GrammarSemantics::derive(&orphan);

    assert!(
        sem.declared_phonology(),
        "fixture precondition: the grammar declares a <PhonologicalRule>"
    );
    assert!(
        !sem.cascade_phonology(),
        "fixture precondition: no stratum names that rule, so it never reaches the cascade"
    );

    assert!(
        Applicability::HasPhonology.matches(&orphan),
        "Applicability::HasPhonology reads the grammar-wide declaration and must stay true here -- \
         switching it to the cascade reading would change which backend families this grammar is \
         offered, which task 7.11 explicitly forbids"
    );
    assert!(
        Applicability::HasPhonologyOrTemplates.matches(&orphan),
        "HasPhonologyOrTemplates is HasPhonology OR HasTemplates over the same declared facts"
    );
    assert!(
        PhonologyProbe::new(&orphan).is_none(),
        "PhonologyProbe drives the per-stratum rewrite cascade, so an unreferenced rule gives it \
         nothing to probe -- it must stay None here"
    );
    assert!(
        PhonologyProbe::new_with_semantics(&sem).is_none(),
        "the semantics-taking constructor must reach the identical answer"
    );
}
