use super::*;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// A grammar whose single `<PhonologicalRule>` is declared globally but named by no stratum's `phonologicalRules` attribute: the declared-vs-cascade split, made concrete.
const ORPHANED_PRULE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>OrphanedPruleFixture</Name>
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

/// The two phonology facts are genuinely distinct: without the split, whichever single predicate survived would change one of the two consumers' answers on this shape.
#[test]
fn declared_and_cascade_phonology_are_distinct_facts() {
    let g = load(ORPHANED_PRULE_XML);
    let sem = GrammarSemantics::derive(&g);

    assert!(
        sem.declared_phonology(),
        "the grammar declares a <PhonologicalRule>, so declared_phonology must be true"
    );
    assert!(
        !sem.cascade_phonology(),
        "no stratum names that rule in its phonologicalRules list, so it never reaches the \
         per-stratum cascade and cascade_phonology must be false"
    );
    assert!(
        sem.prules_in_order().is_empty(),
        "prules_in_order walks strata, so an unreferenced rule contributes nothing"
    );
}

/// `derive` is pure: two derivations from the same load, and one from a second independent load, agree on every eager and memoized fact.
#[test]
fn derivation_is_deterministic_across_independent_loads() {
    let g1 = load(ORPHANED_PRULE_XML);
    let g2 = load(ORPHANED_PRULE_XML);
    let a = GrammarSemantics::derive(&g1);
    let b = GrammarSemantics::derive(&g2);

    assert_eq!(a.declared_phonology(), b.declared_phonology());
    assert_eq!(a.cascade_phonology(), b.cascade_phonology());
    assert_eq!(a.declared_templates(), b.declared_templates());
    assert_eq!(a.has_morphology(), b.has_morphology());
    assert_eq!(a.has_reduplication(), b.has_reduplication());
    assert_eq!(a.has_metathesis(), b.has_metathesis());
    assert_eq!(a.entry_count(), b.entry_count());
    assert_eq!(a.stratum_count(), b.stratum_count());
    assert_eq!(a.ordered_operations(), b.ordered_operations());
    assert_eq!(a.ordering_dependencies(), b.ordering_dependencies());
    assert_eq!(a.gated_subrules(), b.gated_subrules());
    assert_eq!(a.entry_partition(), b.entry_partition());
    // Reading twice must give the same memoized answer, not a recomputed one.
    assert_eq!(a.entry_partition(), a.entry_partition());
    assert_eq!(a.partition_count(), 1);
}
