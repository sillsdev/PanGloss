//! Proposer-to-confirm containment for `MorphRuleOrder::Unordered`'s chain-depth-bounded configuration (target disposition `ConfirmOnly`), plus a deterministic unbounded-budget-refusal witness.
//! See docs/research/pg-foma-cover-unordered-morph-rules-notes.md for the fixture and the two distinguishing-witness arguments.

mod common;

use std::collections::HashSet;

use pg_foma::analyzer::{FomaError, FomaProposer};
use pg_foma::backend_selection::select_backends_for_grammar;
use pg_foma::capability::{compose_envelope, default_registry, CompileDecision};
use pg_foma::composite::FomaAnalyzer;
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

/// `mrule_order`: `"unordered"` or `"linear"`, the only difference between the two fixture variants this file compares.
fn fixture_xml(mrule_order: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>CoverUnorderedMorphRulesFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="{mrule_order}" morphologicalRules="mrP mrQ">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrP" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>p</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subP">
                <MorphologicalInput><PhoneticSequence id="stemP"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemP" /><InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>P</MorphemeId>
          </MorphologicalRule>
          <MorphologicalRule id="mrQ" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>q</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subQ">
                <MorphologicalInput><PhoneticSequence id="stemQ"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stemQ" /><InsertSegments><PhoneticShape>q</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>Q</MorphemeId>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#
    )
}

/// A chain-depth-bounded `Unordered` stratum whose loose-rule count exceeds the calibrated `pg_foma::compose_budget` default, generated programmatically rather than hand-typed.
fn unbounded_fixture_xml(rule_count: u32) -> String {
    let mut rules = String::new();
    let mut segs = String::new();
    for i in 0..rule_count {
        segs.push_str(&format!(
            r#"<SegmentDefinition id="cx{i}"><Representations><Representation>x{i}</Representation></Representations></SegmentDefinition>"#
        ));
        rules.push_str(&format!(
            r#"<MorphologicalRule id="mr{i}" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
                 <Name>r{i}</Name>
                 <MorphologicalSubrules>
                   <MorphologicalSubrule id="sub{i}">
                     <MorphologicalInput><PhoneticSequence id="stem{i}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                     <MorphologicalOutput><CopyFromInput index="stem{i}" /><InsertSegments><PhoneticShape>x{i}</PhoneticShape></InsertSegments></MorphologicalOutput>
                   </MorphologicalSubrule>
                 </MorphologicalSubrules>
                 <MorphemeId>R{i}</MorphemeId>
               </MorphologicalRule>"#
        ));
    }
    let rule_ids: Vec<String> = (0..rule_count).map(|i| format!("mr{i}")).collect();
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>CoverUnorderedMorphRulesUnbounded</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        {segs}
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="{rule_ids}">
        <Name>Main</Name>
        <MorphologicalRuleDefinitions>{rules}</MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="eK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <MorphemeId>K</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#,
        rule_ids = rule_ids.join(" "),
    )
}

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// `(morpheme_ids, root_morpheme_index)` multiset key, same shape `tests/cover_compounding.rs::analysis_set` uses.
fn analysis_set(v: &[WordAnalysis]) -> HashSet<(Vec<u32>, i32)> {
    v.iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect()
}

/// Runs `word` through both the real propose-confirm composite and the full-HC oracle, and asserts exact structured-set equality (never mere containment).
fn assert_confirm_matches_oracle(
    analyzer: &mut FomaAnalyzer,
    morpher: &Morpher,
    word: &str,
    expect_nonempty: bool,
) -> pg_foma::composite::FomaOutcome {
    let oracle = morpher.parse_word_opts(word, &ParseOptions::default());
    let outcome = analyzer.analyze_word(word);

    assert_eq!(
        !oracle.structured.is_empty(),
        expect_nonempty,
        "oracle precondition for {word:?}: expected non-empty={expect_nonempty}, got {:?}",
        oracle.structured
    );
    assert_eq!(
        outcome.confirmed,
        oracle.structured.len(),
        "confirmed count must equal the oracle's exact analysis count for {word:?}"
    );
    assert_eq!(
        analysis_set(&outcome.structured),
        analysis_set(&oracle.structured),
        "FST-confirmed set must equal the oracle's own set for {word:?}"
    );
    outcome
}

/// This fixture's `Unordered` stratum must characterize as chain-depth-bounded and compose to `ConfirmOnly`, proving the containment tests below exercise its resting disposition, not an accident.
#[test]
fn fixture_is_chain_depth_bounded_and_confirm_only() {
    let g = load(&fixture_xml("unordered"));
    let ro: Vec<&PhonRuleDef> = g
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|&id| &g.prules[id.0 as usize])
        .collect();
    let phon = PhonologyProbe::new(&g);
    let alphabet = SegAlphabet::new(&g.char_tables[0]);
    let plan = enumerate_default(&g, &alphabet, &ro, phon.as_ref());
    let registry = default_registry();

    assert_eq!(
        compose_envelope(&g, &plan, &registry),
        CompileDecision::ConfirmOnly,
        "a chain-depth-bounded Unordered fixture must compose to ConfirmOnly, never Refuse"
    );
}

/// The positive witness: `"kqp"` (`mrQ` before `mrP`, reverse of document order) is genuinely oracle-confirmed and proposed under `Unordered`; `"kpq"` still works too, so ordinary recall is unaffected.
#[test]
fn non_document_order_analysis_is_proposed_and_confirmed() {
    let g = load(&fixture_xml("unordered"));
    let mut analyzer = FomaAnalyzer::new(&g).expect(
        "fixture must compile: a chain-depth-bounded Unordered stratum, no phonology, no templates",
    );
    let morpher = Morpher::new(&g, usize::MAX);

    let document_order = assert_confirm_matches_oracle(&mut analyzer, &morpher, "kpq", true);
    assert!(
        document_order.candidates_generated > 0,
        "document-order kpq must still be proposed"
    );

    let reverse_order = assert_confirm_matches_oracle(&mut analyzer, &morpher, "kqp", true);
    assert!(
        reverse_order.candidates_generated > 0,
        "the FST proposer must PROPOSE kqp (crate::emit::build_deriv_chain offers every rule at \
         every derivation-chain level, unconditional on rule order)"
    );
    assert_eq!(
        reverse_order.confirmed, 1,
        "kqp must confirm to exactly one analysis under Unordered's any-order combination cascade"
    );
}

/// The identical grammar, differing only in `mrule_order="linear"`, must not confirm `"kqp"` at all: firing `mrQ` before `mrP` is out of scope for `Cascade::permutation`'s non-decreasing-index restriction.
#[test]
fn linear_variant_of_the_same_grammar_does_not_confirm_the_reverse_order() {
    let g = load(&fixture_xml("linear"));
    let morpher = Morpher::new(&g, usize::MAX);

    let kpq = morpher.parse_word_opts("kpq", &ParseOptions::default());
    assert!(
        !kpq.structured.is_empty(),
        "document-order kpq must still confirm under Linear"
    );

    let kqp = morpher.parse_word_opts("kqp", &ParseOptions::default());
    assert!(
        kqp.structured.is_empty(),
        "kqp (reverse of document order) must NOT confirm under Linear -- \
         Cascade::permutation never revisits an index behind the current one"
    );

    // The FST proposer still proposes kqp (order-blind at propose time); confirm alone draws the Linear/Unordered distinction.
    let mut analyzer = FomaAnalyzer::new(&g).expect("Linear fixture must compile too");
    let outcome = analyzer.analyze_word("kqp");
    assert!(
        outcome.candidates_generated > 0,
        "the FST proposer must still propose kqp even though this grammar is Linear"
    );
    assert_eq!(
        outcome.confirmed, 0,
        "confirm must prune kqp to zero under Linear, matching the oracle exactly"
    );
}

/// The negative witness: `"kpp"`/`"kqq"` (the same rule applied twice, over `multipleApplication = 1`) are over-proposed by `build_deriv_chain` but confirm's `MaxApplicationCount` gate prunes both to zero.
#[test]
fn same_rule_reapplication_is_over_proposed_and_confirm_pruned() {
    let g = load(&fixture_xml("unordered"));
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture must compile");
    let morpher = Morpher::new(&g, usize::MAX);

    for word in ["kpp", "kqq"] {
        let outcome = assert_confirm_matches_oracle(&mut analyzer, &morpher, word, false);
        assert_eq!(
            outcome.confirmed, 0,
            "{word} must confirm zero analyses (max_apps = 1)"
        );
        assert!(
            outcome.candidates_generated > 0,
            "the FST proposer must still PROPOSE {word} (build_deriv_chain never checks \
             multipleApplication) for confirm's own gate to have anything to prune"
        );
    }
}

/// Zero phonological rules is the public proxy for "`should_run` is false, so `MorphotacticIndex`'s consumers never run", proving the containment above comes from `build_deriv_chain`, not that pruning convention.
#[test]
fn no_phonology_isolates_build_deriv_chain_from_the_legality_pruning_convention() {
    let g = load(&fixture_xml("unordered"));
    assert!(
        g.prules.is_empty(),
        "this fixture must have zero phonological rules, so crate::preexpand::should_run is false \
         and the morphotactic-legality pruning convention's own consumers never run"
    );
}

/// A stratum whose loose-rule count exceeds the calibrated budget must deterministically fail to compile with a typed refusal, never a silent truncation or an attempt to build the oversized network.
#[test]
fn unbounded_unordered_stratum_deterministically_refuses_to_compile() {
    let xml = unbounded_fixture_xml(101);
    let g = load(&xml);

    match pg_foma::analyzer::FomaProposer::new(&g) {
        Err(FomaError::UnorderedOrderingMultiplicityExceeded { rule_count, limit }) => {
            assert_eq!(rule_count, 101);
            assert_eq!(limit, 100);
        }
        Err(other) => panic!("expected UnorderedOrderingMultiplicityExceeded, got {other}"),
        Ok(_) => panic!(
            "expected a 101-rule Unordered stratum to exceed the calibrated default budget (100)"
        ),
    }

    // The same refusal surfaces through the public product API, never a panic or a hang building a 101-level chain.
    match FomaAnalyzer::new(&g) {
        Err(FomaError::UnorderedOrderingMultiplicityExceeded { .. }) => {}
        Err(other) => panic!("expected UnorderedOrderingMultiplicityExceeded, got {other}"),
        Ok(_) => panic!("expected FomaAnalyzer::new to propagate the same refusal"),
    }
}

/// The capability characterization for the same unbounded grammar must independently decline it for the backend that would compile it -- read from the selector's per-backend report rather than a whole-grammar join, since the join is the best ANY backend offers and a backend that builds no derivation layers has nothing to say about the budget bounding them.
#[test]
fn unbounded_unordered_stratum_composes_to_refuse() {
    let xml = unbounded_fixture_xml(101);
    let g = load(&xml);
    let selection = select_backends_for_grammar(&g);

    let backend = FomaProposer::EMISSION_STRATEGY;
    let report = selection
        .report_for(backend)
        .expect("every backend must be reported");
    assert!(
        report
            .declined_on()
            .iter()
            .any(|d| d.construct.contains("Unordered")),
        "expected a diagnostic naming the Unordered stratum: {:?}",
        report.declined_on()
    );

}
