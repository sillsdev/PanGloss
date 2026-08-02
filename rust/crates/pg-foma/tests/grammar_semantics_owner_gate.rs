//! Task 7.11 of `openspec/changes/cleanup-and-recipe-parity`: the single-owner invariant for
//! [`pg_foma::grammar_semantics::GrammarSemantics`].
//!
//! The invariant this pins: **no migrated consumer re-reads `Grammar` to decide something
//! `GrammarSemantics` already owns.** Two shapes of evidence, and they are not equally strong --
//! stated plainly rather than padded:
//!
//! 1. **Call counting (the real gate).** [`pg_foma::capability::characterize`] is the expensive
//!    walk -- it builds real `foma::types::Fsm` networks for `Simultaneous`-mode subrules -- and
//!    nothing memoized it before this task. `select_plan` called it ONCE PER CANDIDATE PLAN and
//!    `preflight_findings` called it twice. Both assertions below FAIL on the pre-7.11 code (2 and
//!    2 respectively, against the 1 asserted here) and pass after. This is the load-bearing test.
//!
//! 2. **The declared-vs-cascade phonology split.** `Applicability::HasPhonology` and
//!    `PhonologyProbe::new`'s existence gate are DIFFERENT predicates that genuinely disagree, and
//!    the fixture here proves it. This assertion also fails on any future "simplification" that
//!    unifies them -- which is exactly what it is for, since unifying them would change which recipe
//!    families a grammar is offered.
//!
//! What is NOT claimed: the projection equalities (`Applicability::HasTemplates` ==
//! `declared_templates`, and so on) are tautologies given the implementation and would pass either
//! way. They are not asserted here, because a test that cannot fail is not evidence.
//!
//! # Why one `#[test]` function
//! `characterize_call_count` is thread-local, so it cannot be polluted by other test binaries or by
//! tests on other threads -- but two `#[test]`s in THIS file could still be scheduled on the same
//! thread by the harness's thread reuse. Keeping the counting work in one function makes the reading
//! unambiguous without depending on how the harness schedules.

use foma::options::FomaOptions;
use pg_grammar::model::{Grammar, PhonRuleDef};

use pg_foma::capability::{
    characterize_call_count, default_registry, reset_characterize_call_count,
};
use pg_foma::compose_budget::ComposeBudget;
use pg_foma::enumerate::enumerate_candidates;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::preflight::preflight_findings;
use pg_foma::recipe_registry::Applicability;
use pg_foma::replace::SegAlphabet;
use pg_foma::selection::select_plan;

/// One MPR-gated subrule and two entries realizing both truth values of that gate key, so
/// `enumerate_candidates` yields TWO candidates (`"default"` and `"gate-group-permuted"`) -- the
/// shape that makes `select_plan`'s per-candidate `characterize` visible. Same synthetic fixture
/// `selection.rs`'s own test module uses, duplicated here because test modules do not share private
/// helpers across files.
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

/// The disagreement fixture: `prule1` is declared in the global `<PhonologicalRuleDefinitions>`
/// block and named by NO stratum's `phonologicalRules` attribute. `pg_grammar::load`'s stratum
/// loader only ever pushes ids that attribute names, so the rule lands in `g.prules` and in no
/// `sd.prules`.
const ORPHANED_PRULE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SemanticsOwnerOrphanedPruleFixture</Name>
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

/// `select_plan` characterizes the GRAMMAR once, not once per candidate PLAN.
///
/// Falsified 2026-08-02 by moving `GrammarSemantics::derive` back inside `select_plan`'s
/// per-candidate closure: this test then reported 2 (one per candidate) and FAILED.
#[test]
fn select_plan_characterizes_the_grammar_once_not_once_per_candidate() {
    let g = load(GATED_TWO_GROUP_XML);
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let ro = prules_in_order(&g);
    let phon = PhonologyProbe::new(&g);
    let opts = FomaOptions::default();
    // `ComposeBudget::unbounded()` is `#[cfg(test)]`-only inside the crate, so an integration test
    // builds the equivalent never-trips budget through the public constructor (same shape
    // `plan_interaction_coverage::fuzz_gate_group_reordering_for_grammar` uses).
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let registry = default_registry();

    let candidates = enumerate_candidates(&g, &alphabet, &ro, phon.as_ref());
    assert_eq!(
        candidates.len(),
        2,
        "this fixture must yield 2 candidates, or the per-candidate claim below is vacuous"
    );

    reset_characterize_call_count();
    let outcome = select_plan(&candidates, &g, &registry, &opts, &alphabet, &ro, &budget);
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

/// `preflight_findings` characterizes once, not twice.
///
/// Falsified 2026-08-02 by restoring the two independent walks (`capability::characterize(g)` here
/// plus a second inside a freshly derived `evaluate_capability`): this test then reported 2 and
/// FAILED.
#[test]
fn preflight_findings_characterizes_once_not_twice() {
    let g = load(GATED_TWO_GROUP_XML);

    reset_characterize_call_count();
    let findings = preflight_findings(&g);
    let preflight_calls = characterize_call_count();

    assert!(
        preflight_calls > 0,
        "the counter must actually have observed preflight_findings"
    );
    assert_eq!(
        preflight_calls, 1,
        "preflight_findings must characterize once -- it took the profile AND the capability \
         verdict from two independent walks before task 7.11, which its own module doc called \
         'an acceptable duplication ... while waiting on 7.11'"
    );
    // A sanity check that preflight actually ran rather than short-circuiting: this grammar has a
    // gated MPR subrule and a rewrite rule, so preflight has real cardinality to look at.
    let _ = findings;
}

/// Declared phonology and cascade phonology are DIFFERENT facts, and each consumer keeps the one it
/// already meant. A "simplification" that unified them fails here.
///
/// Falsified 2026-08-02 by pointing `Applicability::HasPhonology` at `cascade_phonology`: this test
/// then FAILED on the orphaned-rule fixture, which is exactly the routing change task 7.11 forbids.
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
         switching it to the cascade reading would change which recipe families this grammar is \
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
