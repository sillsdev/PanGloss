//! Proposer-to-confirm containment for `MorphRuleOrder::Unordered`'s
//! `unordered-application.chain-depth-bounded` configuration predicate (target disposition:
//! `ConfirmOnly`), plus a deterministic `unordered-application.unbounded` budget-refusal witness.
//!
//! ## Synthetic, delanguaged fixture (synthetic data only -- invented CVC root, no
//! natural-language lexemes, named by construct only)
//! One stratum, `morphologicalRuleOrder="unordered"`, TWO loose suffix rules declared in document
//! order `mrP` (index 0) then `mrQ` (index 1) -- no `required_syn_fs`/feature interaction between
//! them at all, no phonological rules, no `Role::Infix` rule, no templates. Both suffix, so cascade
//! APPLICATION order directly determines surface CONCATENATION order (each new suffix appends at
//! the current end): applying `mrP` then `mrQ` yields `"kpq"`; applying `mrQ` then `mrP` yields
//! `"kqp"`.
//!
//! ## The distinguishing property this fixture pins (empirically verified against the real oracle,
//! `pg_parse::Morpher` -- NOT hand-derived, `docs/adr/0001`'s own discipline)
//! `pg_rules::cascade::Cascade::permutation` (`Linear`) only ever recurses to a NON-DECREASING rule
//! index (`permutation_rec`'s own doc: "never revisits an index behind the current one") -- so
//! under a HYPOTHETICAL `morphologicalRuleOrder="linear"` declaration of this SAME grammar,
//! `"kqp"` (rule index 1 firing before rule index 0) is NOT a reachable analysis at all: `Morpher::
//! parse_word_opts("kqp", ..)` returns an EMPTY `structured` set under `Linear` (verified directly
//! against this exact fixture, `linear_variant` below), while `"kpq"` (document order) IS reachable
//! under EITHER `mrule_order`. Declaring THIS grammar's own stratum `Unordered` is therefore the
//! MINIMAL change that makes `"kqp"` a genuine, oracle-confirmed analysis (`Cascade::combination`'s
//! any-order walk, `cascade.rs`'s own "k!-walk over rule subsets" doc) -- exactly the scenario
//! where "a word's analysis requires the stratum's rules to have applied in an order other than
//! their declared document order."
//!
//! ## The distinguishing witness against the pre-existing morphotactic-legality convention
//! This fixture has ZERO phonological rules and ZERO `Role::Infix` rules -- `crate::preexpand::
//! should_run(g, phon) = phon.is_some() || any_infix_rule(g)` is `false` for it (both `mrP`/`mrQ`
//! classify `Role::Suffix`: their RHS is `CopyFromInput` followed by a TRAILING `InsertSegments`,
//! the suffix shape `crate::emit::classify_affix` recognizes), so `crate::preexpand::
//! build_composites`/`crate::morphotactics::MorphotacticIndex::next_state` (the "Linear-as-Unordered"
//! pruning convention `morphotactics.rs`'s own module doc names) are NEVER CONSULTED for a single
//! (root, rule) pair on this grammar -- confirmed by `g.prules.is_empty()` below (the public proxy
//! this integration test can observe; `crate::preexpand`/`crate::morphotactics` are crate-internal).
//! The containment this file proves for `"kqp"` therefore comes ENTIRELY from
//! `crate::emit::build_deriv_chain`'s ordinary derivation-layer construction (this change's own
//! load-bearing finding, `crate::unordered`'s own module doc) -- not from that pruning automaton;
//! the two are NOT the same proof.

mod common;

use std::collections::HashSet;

use pg_foma::analyzer::FomaError;
use pg_foma::capability::{compose_envelope, default_registry, CompileDecision};
use pg_foma::composite::FomaAnalyzer;
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

/// `mrule_order`: `"unordered"` or `"linear"` -- the ONLY difference between the two fixture
/// variants this file compares (module doc's "distinguishing property").
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

/// A chain-depth-bounded `Unordered` stratum's OWN loose-rule count exceeding the calibrated
/// [`pg_foma::compose_budget`] default -- generated programmatically (never hand-typed), same
/// generator shape `crate::unordered`'s own test-only `stratum_xml` uses.
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

/// `(morpheme_ids, root_morpheme_index)` multiset key -- same shape `tests/cover_compounding.rs::
/// analysis_set` uses.
fn analysis_set(v: &[WordAnalysis]) -> HashSet<(Vec<u32>, i32)> {
    v.iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect()
}

/// Runs `word` through both the real propose→confirm composite and the full-HC oracle, and asserts
/// EXACT structured-set equality between them (never mere containment) -- same helper shape as
/// `cover_compounding.rs::assert_confirm_matches_oracle`.
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

/// Deliverable 3 / capability.rs judgment call check: this fixture's OWN `Unordered` stratum must
/// characterize `unordered-application.chain-depth-bounded` and compose to `ConfirmOnly` -- proving
/// the containment tests below exercise this construct's own resting disposition, not an accident
/// of some other predicate meeting it down.
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

/// **The positive witness.** `"kqp"` (rule index 1 --
/// `mrQ` -- firing before rule index 0 -- `mrP`, the REVERSE of document order) is a genuine,
/// oracle-confirmed analysis under `Unordered`, and the FST proposer (via `crate::emit::
/// build_deriv_chain`'s existing derivation-layer construction, not a new mechanism -- module doc)
/// PROPOSES it, and confirm accepts it exactly. `"kpq"` (document order) is proposed/confirmed too,
/// proving ordinary (Linear-reachable) recall is unaffected by this change.
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

/// **The distinguishing property witness (module doc).** The IDENTICAL grammar, differing ONLY in
/// `mrule_order="linear"`, must NOT confirm `"kqp"` at all: this test pins that firing `mrQ`
/// before `mrP` is unreachable under [`pg_rules::cascade::Cascade::permutation`]'s own
/// non-decreasing-index restriction (rule index 1 before rule index 0) -- the real semantic
/// difference `Unordered`'s promotion depends on.
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

    // The FST proposer still PROPOSES kqp (build_deriv_chain is order-blind at propose time) --
    // confirm alone is what draws the Linear/Unordered distinction. This is this change's own
    // negative-witness shape, mirrored here for the Linear grammar specifically.
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

/// **The negative witness: an ordering the union proposal licenses but the exact
/// combination-cascade fold at confirm prunes to zero.** `mrP`/`mrQ` both default to
/// `multipleApplication = 1` (DTD default) -- `"kpp"`/`"kqq"` (the SAME rule applied twice) are
/// over-proposed by `build_deriv_chain` (every level offers every rule, unconditional on a rule's
/// own re-application cap) but confirm's `apply_one_mrule`/`MaxApplicationCount` gate prunes both
/// to zero, exactly matching the oracle.
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

/// **The distinguishing-from-legality-convention witness.** This fixture has zero phonological
/// rules -- the public proxy for
/// "`crate::preexpand::should_run` is false, so `crate::morphotactics::MorphotacticIndex`'s
/// consuming callers never run for a single (root, rule) pair on this grammar" (module doc) --
/// proving the containment proven above comes from `crate::emit::build_deriv_chain`, not the
/// pre-existing "Linear-as-Unordered" pruning convention.
#[test]
fn no_phonology_isolates_build_deriv_chain_from_the_legality_pruning_convention() {
    let g = load(&fixture_xml("unordered"));
    assert!(
        g.prules.is_empty(),
        "this fixture must have zero phonological rules, so crate::preexpand::should_run is false \
         and the morphotactic-legality pruning convention's own consumers never run"
    );
}

/// **Task 7's other half: `unordered-application.unbounded` stays refused.** A stratum whose own
/// loose-rule count exceeds `pg_foma::compose_budget::DEFAULT_ORDERING_MULTIPLICITY_BUDGET` must
/// deterministically fail to compile via `FomaAnalyzer::new` -- an honest, typed refusal (never a
/// silent truncation, never an attempt to actually build the oversized network).
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

    // The SAME refusal surfaces through the public product API (`FomaAnalyzer::new`, which builds
    // a `FomaProposer` internally) -- never a panic, never a hang building a 101-level chain.
    match FomaAnalyzer::new(&g) {
        Err(FomaError::UnorderedOrderingMultiplicityExceeded { .. }) => {}
        Err(other) => panic!("expected UnorderedOrderingMultiplicityExceeded, got {other}"),
        Ok(_) => panic!("expected FomaAnalyzer::new to propagate the same refusal"),
    }
}

/// The capability characterization for the SAME unbounded grammar must independently report
/// `unordered-application.unbounded` (`Refuse`), agreeing with the real compile-time refusal above
/// (both read the SAME calibrated constant -- `crate::unordered`'s own module doc) rather than being
/// a second, silently-divergent source of truth.
#[test]
fn unbounded_unordered_stratum_composes_to_refuse() {
    let xml = unbounded_fixture_xml(101);
    let g = load(&xml);
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

    match compose_envelope(&g, &plan, &registry) {
        CompileDecision::Refuse(diags) => {
            assert!(
                diags.iter().any(|d| d.construct.contains("Unordered")),
                "expected a diagnostic naming the Unordered stratum: {diags:?}"
            );
        }
        other => panic!("expected Refuse, got {other:?}"),
    }
}
