//! The compounding recursion depth bound and its depth-budgeted construction; see
//! `docs/research/pg-foma-cover-compounding-recursive-depth-bound-design-notes.md` for the argument.

use std::fs;
use std::path::Path;

use pg_foma::analyzer::FomaProposer;
use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::best_case_across_backends_for_grammar;
use pg_foma::emit;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn fixture_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../../conformance-staging/edge-cases/recursive-endocentric-compounding/grammar.xml",
    )
}

fn load() -> Grammar {
    let path = fixture_path();
    let xml = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

/// `cr1`'s `multipleApplication="9"` bounds at exactly `1 + 9 = 10` stems, pinned against the real staged fixture rather than a synthetic grammar.
#[test]
fn characterize_reports_the_computed_depth_bound_for_the_staged_fixture() {
    let g = load();
    let max_depth = pg_foma::capability::characterize(&g)
        .compounding_details()
        .map(|d| d.max_depth)
        .max()
        .unwrap_or(0);
    assert_eq!(
        max_depth, 10,
        "cr1's multipleApplication=\"9\" must bound at exactly 10 stems (1 base + 9)"
    );
}

/// This fixture's self-feeding `CompoundingRule` composes to `ConfirmOnly` rather than `Refuse`, checked via the whole-grammar join rather than `characterize` directly.
#[test]
fn capability_gate_is_now_confirm_only_for_the_computed_depth_bound() {
    let g = load();
    assert_eq!(
        best_case_across_backends_for_grammar(&g),
        CompileDecision::ConfirmOnly,
        "a self-feeding CompoundingRule (multipleApplication > 1) must now evaluate to ConfirmOnly \
         -- crate::emit's depth-budgeted compound loop closes the construction gap that used to make \
         this Refuse"
    );
}

/// `build_compound_chain` unrolls enough extra non-head levels to realize the fixture's computed bound (10 stems), so the production proposer now proposes the genuine 3-stem self-feeding compound `tevimaflisra`.
#[test]
fn depth_budgeted_compound_loop_now_proposes_the_bounded_recursive_shape() {
    let g = load();
    let mut proposer =
        FomaProposer::new(&g).expect("fixture must compile (a single, simple CompoundingRule)");

    // Sanity: the same proposer still proposes ordinary depth-1 compounds and bare roots fine.
    for word in ["tevi", "mafl", "isra", "tevimafl", "maflisra"] {
        assert!(
            !proposer.propose(word).is_empty(),
            "sanity: {word:?} (bare root / depth-1 compound) must still propose at least one \
             candidate on this same compiled network"
        );
    }

    let candidates = proposer.propose("tevimaflisra");
    assert!(
        !candidates.is_empty(),
        "the depth-budgeted compound loop (task 4.1 pieces 2/3) must now propose at least one \
         candidate for the genuine 3-stem self-feeding compound tevimaflisra"
    );
    // Every candidate must be the real ROOT1+ROOT2+ROOT3 sequence (ids 0,1,2), never a spurious one.
    for c in &candidates {
        let ids: Vec<u32> = c.morphemes.iter().map(|m| m.0).collect();
        assert_eq!(
            ids,
            vec![0, 1, 2],
            "every tevimaflisra candidate must be the ROOT1+ROOT2+ROOT3 sequence: {candidates:?}"
        );
    }
}

/// Raises the oracle's `max_stem_count` cap so it non-vacuously accepts the 3-stem analysis; see
/// docs/research/pg-foma-cover-compounding-recursive-depth-bound-design-notes.md.
#[test]
fn raised_cap_oracle_finds_the_recursive_analysis_confirm_at_default_would_miss() {
    let g = load();

    // `usize::MAX` here is the unrelated step-budget `cap` param, left uncapped so it never interferes.
    let default_morpher = Morpher::new(&g, usize::MAX);
    let default_outcome = default_morpher.parse_word_opts("tevimaflisra", &ParseOptions::default());
    assert_eq!(
        default_outcome.analyses.len(),
        0,
        "at the default max_stem_count (2), tevimaflisra must still confirm zero analyses -- a \
         containment check against this default would be vacuously true, proving nothing"
    );

    // Raised cap (3): the oracle DOES accept the 3-stem analysis once non-vacuous.
    let raised_morpher = Morpher::new(&g, usize::MAX).with_max_stem_count(3);
    let raised_outcome = raised_morpher.parse_word_opts("tevimaflisra", &ParseOptions::default());
    assert_eq!(
        raised_outcome.analyses.len(),
        1,
        "with max_stem_count raised to 3, tevimaflisra (a genuine ROOT1+ROOT2+ROOT3 self-feeding \
         compound) must confirm exactly one analysis -- the non-vacuous ground truth propose must \
         now contain for compounding.recursive's ConfirmOnly promotion to be honest"
    );
}

/// The load-bearing proposer-to-confirm containment proof; see
/// docs/research/pg-foma-cover-compounding-recursive-depth-bound-design-notes.md.
#[test]
fn depth_budgeted_compound_loop_contains_the_raised_cap_oracle_analysis() {
    let g = load();
    let mut proposer = FomaProposer::new(&g).expect("fixture must compile");
    let candidates = proposer.propose("tevimaflisra");
    assert!(
        !candidates.is_empty(),
        "propose must offer at least one candidate for tevimaflisra now that the compound loop is \
         depth-budgeted"
    );

    let raised_morpher = Morpher::new(&g, usize::MAX).with_max_stem_count(3);
    let outcome = raised_morpher.parse_word_opts("tevimaflisra", &ParseOptions::default());
    assert_eq!(
        outcome.structured.len(),
        1,
        "non-vacuity precondition (see raised_cap_oracle_finds_the_recursive_analysis_confirm_at_\
         default_would_miss): the raised-cap oracle must accept exactly one analysis"
    );
    let oracle_morphemes = outcome.structured[0].morpheme_ids.clone();

    let contained = candidates.iter().any(|c| {
        c.morphemes
            .iter()
            .map(|m| m.0)
            .eq(oracle_morphemes.iter().copied())
    });
    assert!(
        contained,
        "propose's candidate set must CONTAIN the oracle's raised-cap analysis (exact morpheme-id \
         sequence match) -- proposed candidates: {candidates:?}, oracle morpheme_ids: \
         {oracle_morphemes:?}"
    );
}

/// One isolated `CompoundingRule` at `multipleApplication = max_apps`, so `max_depth = 1 + max_apps`; `roots.len()` freely-licensed CVCV roots let a word need up to that many stems.
fn small_bound_grammar_xml(max_apps: u32, roots: &[&str]) -> String {
    let mut entries = String::new();
    for (i, root) in roots.iter().enumerate() {
        entries.push_str(&format!(
            r#"<LexicalEntry id="eRoot{i}" partOfSpeech="posRoot">
                 <Allomorphs><Allomorph id="aRoot{i}"><PhoneticShape>{root}</PhoneticShape></Allomorph></Allomorphs>
                 <MorphemeId>ROOT{i}</MorphemeId>
               </LexicalEntry>"#
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>SmallBoundFixture</Name>
  <PartsOfSpeech><PartOfSpeech id="posRoot"><Name>root</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ci"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="co"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cl"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cm"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cn"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="linear" morphologicalRules="cr1">
      <Name>Main</Name>
      <MorphologicalRuleDefinitions>
        <CompoundingRule id="cr1" multipleApplication="{max_apps}" headPartsOfSpeech="posRoot" nonHeadPartsOfSpeech="posRoot" outputPartOfSpeech="posRoot">
          <Name>Compound</Name>
          <CompoundingSubrules>
            <CompoundingSubrule>
              <HeadMorphologicalInput>
                <PhoneticSequence id="h0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
              </HeadMorphologicalInput>
              <NonHeadMorphologicalInput>
                <PhoneticSequence id="n0"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
              </NonHeadMorphologicalInput>
              <MorphologicalOutput>
                <CopyFromInput index="h0" />
                <CopyFromInput index="n0" />
              </MorphologicalOutput>
            </CompoundingSubrule>
          </CompoundingSubrules>
        </CompoundingRule>
      </MorphologicalRuleDefinitions>
      <LexicalEntries>{entries}</LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#
    )
}

/// `rule_count` distinct `CompoundingRule`s at the default `multipleApplication="1"`, the other way `max_depth` grows: as a rule count, `1 + 1 + (rule_count - 1)`, not a nesting depth.
fn many_rule_grammar_xml(rule_count: usize, roots: &[&str]) -> String {
    let mut entries = String::new();
    for (i, root) in roots.iter().enumerate() {
        entries.push_str(&format!(
            r#"<LexicalEntry id="eRoot{i}" partOfSpeech="posRoot">
                 <Allomorphs><Allomorph id="aRoot{i}"><PhoneticShape>{root}</PhoneticShape></Allomorph></Allomorphs>
                 <MorphemeId>ROOT{i}</MorphemeId>
               </LexicalEntry>"#
        ));
    }
    let mut rules = String::new();
    let mut rule_ids = Vec::new();
    for i in 0..rule_count {
        rule_ids.push(format!("cr{i}"));
        rules.push_str(&format!(
            r#"<CompoundingRule id="cr{i}" multipleApplication="1" headPartsOfSpeech="posRoot" nonHeadPartsOfSpeech="posRoot" outputPartOfSpeech="posRoot">
                 <Name>Compound{i}</Name>
                 <CompoundingSubrules>
                   <CompoundingSubrule>
                     <HeadMorphologicalInput>
                       <PhoneticSequence id="h{i}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                     </HeadMorphologicalInput>
                     <NonHeadMorphologicalInput>
                       <PhoneticSequence id="n{i}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence>
                     </NonHeadMorphologicalInput>
                     <MorphologicalOutput>
                       <CopyFromInput index="h{i}" />
                       <CopyFromInput index="n{i}" />
                     </MorphologicalOutput>
                   </CompoundingSubrule>
                 </CompoundingSubrules>
               </CompoundingRule>"#
        ));
    }
    let rule_list = rule_ids.join(" ");
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput><Language><Name>ManyRuleFixture</Name>
  <PartsOfSpeech><PartOfSpeech id="posRoot"><Name>root</Name></PartOfSpeech></PartsOfSpeech>
  <CharacterDefinitionTable id="t1"><Name>Main</Name>
    <SegmentDefinitions>
      <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ci"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="co"><Representations><Representation>o</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cf"><Representations><Representation>f</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cl"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cm"><Representations><Representation>m</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cn"><Representations><Representation>n</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="cs"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
      <SegmentDefinition id="ct"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
    </SegmentDefinitions>
  </CharacterDefinitionTable>
  <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
  <Strata>
    <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="linear" morphologicalRules="{rule_list}">
      <Name>Main</Name>
      <MorphologicalRuleDefinitions>{rules}</MorphologicalRuleDefinitions>
      <LexicalEntries>{entries}</LexicalEntries>
    </Stratum>
  </Strata>
</Language></HermitCrabInput>"#
    )
}

/// The rule-count-versus-depth conflation, proven by collision rather than described; see
/// docs/research/pg-foma-cover-compounding-recursive-depth-bound-design-notes.md.
#[test]
fn max_depth_cannot_distinguish_four_ways_to_compound_from_four_levels_of_nesting() {
    let roots = ["tevi", "mafl", "isra", "kopu", "nalt"];

    let depth_of = |xml: &str, what: &str| -> usize {
        let g = pg_grammar::load(xml).unwrap_or_else(|e| panic!("{what} fixture must load: {e}"));
        pg_foma::capability::characterize(&g)
            .compounding_details()
            .map(|d| d.max_depth)
            .max()
            .unwrap_or_else(|| panic!("{what}: a CompoundingRule must yield a depth detail"))
    };

    // Genuinely five-deep: one rule that may re-apply four times.
    let genuinely_deep = depth_of(
        &small_bound_grammar_xml(4, &roots),
        "one rule at multipleApplication=4",
    );
    // Four alternatives, none repeatable: nothing here nests deeper than head + non-head per rule.
    let merely_four_ways = depth_of(
        &many_rule_grammar_xml(4, &roots),
        "four non-repeatable rules",
    );

    eprintln!(
        "compounding depth conflation: one-rule-x4 => max_depth={genuinely_deep}; \
         four-rules-x1 => max_depth={merely_four_ways}; operative max_stem_count default = 2"
    );
    assert_eq!(
        genuinely_deep, 5,
        "one isolated rule at multipleApplication=4 must compute 1 + 4 = 5"
    );
    assert_eq!(
        merely_four_ways, 5,
        "four rules each at multipleApplication=1 must compute 1 + 1 + 3 = 5 -- the rule-count sum"
    );
    assert_eq!(
        genuinely_deep, merely_four_ways,
        "THE FINDING: the bound is identical for a grammar that genuinely nests five stems and one \
         that offers four non-repeatable alternatives. It is a rule-count ceiling, not a nesting \
         depth, and `crate::emit::compound_extra_levels_checked` sizes an unrolled construction from \
         it either way"
    );
}

/// The depth-bound-respected gate: over-approximation is licensed up to the computed bound; see
/// docs/research/pg-foma-cover-compounding-recursive-depth-bound-design-notes.md.
#[test]
fn depth_bound_is_respected_a_k_plus_one_stem_word_is_never_proposed() {
    let roots = ["pafu", "kilo", "setu", "namo"];
    let xml = small_bound_grammar_xml(2, &roots);
    let g = pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"));

    let max_depth = pg_foma::capability::characterize(&g)
        .compounding_details()
        .map(|d| d.max_depth)
        .max()
        .unwrap_or(0);
    assert_eq!(
        max_depth, 3,
        "isolated multipleApplication=\"2\" rule must bound at 1 + 2 = 3 stems"
    );

    let mut proposer = FomaProposer::new(&g).expect("small bound fixture must compile");

    let word_k = "pafukilosetu"; // 3 roots -- within the k=3 bound.
    assert!(
        !proposer.propose(word_k).is_empty(),
        "a {k}-stem word (the computed bound itself) must be proposable: {word_k:?}",
        k = 3
    );

    let word_k_plus_one = "pafukilosetunamo"; // 4 roots -- one past the k=3 bound.
    assert!(
        proposer.propose(word_k_plus_one).is_empty(),
        "a (k+1)-stem word must NEVER be proposed once the compound loop's own depth bound is k=3 \
         -- got {:?}",
        proposer.propose(word_k_plus_one)
    );
}

/// The budget gate: an oversized `max_depth` must refuse via `FomaTier::Unsupported`, not unroll; see
/// docs/research/pg-foma-cover-compounding-recursive-depth-bound-design-notes.md.
#[test]
fn compound_chain_depth_budget_trips_before_any_lexc_emitted() {
    let roots = ["pafu", "kilo"];
    // max_apps = 60_000 -> max_depth = 60_001 -- far past DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET (200).
    let xml = small_bound_grammar_xml(60_000, &roots);
    let g = pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"));

    let result = emit::emit(&g);
    assert!(
        result.lexc_source.is_empty(),
        "an over-budget compound-chain-depth grammar must never emit a partial/unsound network"
    );
    match result.report.tier {
        emit::FomaTier::Unsupported { ref reason } => {
            assert!(
                reason.contains("compound chain depth"),
                "the refusal reason must name the compound chain-depth dimension: {reason:?}"
            );
        }
        other => panic!("expected FomaTier::Unsupported, got {other:?}"),
    }
    let exceeded = result
        .report
        .enum_budget_exceeded
        .expect("must report a typed budget-exceeded outcome for the compound chain-depth measure");
    assert_eq!(
        exceeded.measure,
        "compound chain depth (extra non-head root levels)"
    );
    assert!(
        exceeded.value > exceeded.limit,
        "reported value {} must exceed the limit {} it tripped",
        exceeded.value,
        exceeded.limit
    );
}
