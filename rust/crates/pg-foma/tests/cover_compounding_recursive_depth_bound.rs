//! The depth-BOUND half of `Compounding`'s recursive split (piece 1, already landed) plus the
//! depth-BUDGETED CONSTRUCTION and its containment proof (pieces 2/3 -- the part this file's own
//! tests originally found missing).
//!
//! Piece 1 ("bound the self-feeding depth... a max-cycle-length computation,
//! extending the existing classifier") was DONE first: `crate::capability::compounding_max_depth`
//! (`CompoundingDetail::max_depth`) turns the existing boolean `recursive` flag into an exact,
//! always-finite stem-count bound -- see that function's own doc and its unit tests in
//! `capability.rs` (`compounding_max_depth_scales_with_multiple_application`,
//! `..._scales_with_co_located_rule_count`, `..._is_asymmetric_across_strata`,
//! `..._matches_compounding_recursive_boolean_exactly`).
//!
//! Pieces 2/3 (a depth-budgeted faithful cross-product construction; a no-false-negative
//! containment proof) are now DONE too, in `crate::emit`: the "bounded compound loop" (that
//! module's own doc) no longer hardcodes exactly one extra root -- `build_compound_chain` unrolls
//! `max_depth - 1` extra (non-head) root LEVELS, consuming this predicate's own precomputed bound
//! directly (one source of truth), and `crate::capability::CompoundingRecursionSafePredicate` now
//! reaches `ConfirmOnly` unconditionally for every observed `Compounding` rule, recursive or not
//! (`capability.rs`'s own doc, "the recursive split is now closed too"). Growth is checked eagerly,
//! before any lexc text is written, against `crate::emit::DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET` -- a
//! genuinely oversized `multipleApplication` value gets a typed `FomaTier::Unsupported` outcome, not
//! a hang or an OOM (`compound_chain_depth_budget_trips_before_any_lexc_emitted`, below).
//!
//! Fixture: `conformance-staging/edge-cases/recursive-endocentric-compounding` (already staged;
//! reused as-is, not duplicated -- its own `STAGING.md`/`grammar.xml` document the shape: `cr1
//! multipleApplication="9"`, so `compounding_max_depth` bounds it at exactly 10 stems). The
//! bound-respected and budget tests below use small, self-contained inline fixtures instead of the
//! staged one (mirroring this crate's own `capability.rs` inline-XML convention) since they need
//! SPECIFIC, small, exactly-controlled depth numbers the staged fixture's own `multipleApplication="9"`
//! does not give them.

use std::fs;
use std::path::Path;

use pg_foma::analyzer::FomaProposer;
use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::evaluate_capability;
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

/// Piece 1, pinned against the real staged fixture (not just the synthetic unit-test
/// grammars in `capability.rs`): `cr1`'s `multipleApplication="9"` bounds at exactly `1 + 9 = 10`
/// stems -- still true and still checked directly, even though the number no longer surfaces via a
/// `Refuse` diagnostic (see the next test): `crate::capability::characterize` is the one source of
/// truth the construction itself now consumes, and this asserts the number an operator would read
/// there is the same one this module has always claimed.
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

/// This fixture's self-feeding `CompoundingRule` composes to `ConfirmOnly`: the construction gap
/// that would otherwise force `Refuse` is closed (`crate::capability::
/// CompoundingRecursionSafePredicate`'s own doc, "the recursive split is now closed too"). Kept as
/// its own test (not merged into the characterize test above) since it checks
/// a DIFFERENT public surface (`evaluate_capability`/`compose_envelope`, not `characterize` directly).
#[test]
fn capability_gate_is_now_confirm_only_for_the_computed_depth_bound() {
    let g = load();
    assert_eq!(
        evaluate_capability(&g),
        CompileDecision::ConfirmOnly,
        "a self-feeding CompoundingRule (multipleApplication > 1) must now evaluate to ConfirmOnly \
         -- crate::emit's depth-budgeted compound loop closes the construction gap that used to make \
         this Refuse"
    );
}

/// `crate::emit::build_compound_chain`
/// unrolls enough extra non-head levels to realize this fixture's own computed bound (10 stems), so
/// the real, unmodified, production `FomaProposer` (exactly what `FomaAnalyzer`/`pangloss
/// --engine=foma` uses) DOES propose the genuine 3-stem self-feeding compound `tevimaflisra` --
/// an "exactly one extra root, regardless of any rule's bound" loop could not.
#[test]
fn depth_budgeted_compound_loop_now_proposes_the_bounded_recursive_shape() {
    let g = load();
    let mut proposer =
        FomaProposer::new(&g).expect("fixture must compile (a single, simple CompoundingRule)");

    // Sanity: the SAME proposer still proposes the ordinary depth-1 compounds (and bare roots)
    // fine -- proving the non-empty result below is a genuine EXTENSION, not a broken compile that
    // happens to propose garbage for everything.
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
    // Every proposed candidate must be the real 3-root sequence (ROOT1, ROOT2, ROOT3 -- morpheme
    // ids 0,1,2 in this fixture's own declaration order), never some OTHER, spurious morpheme
    // sequence the chain's own over-approximation should not be able to reach at all: the compound
    // loop only ever proposes an ENTRY from this grammar's own root list at each level, so the tag
    // sequence for a word segmenting into exactly three CVCV roots can only ever be this one triple.
    for c in &candidates {
        let ids: Vec<u32> = c.morphemes.iter().map(|m| m.0).collect();
        assert_eq!(
            ids,
            vec![0, 1, 2],
            "every tevimaflisra candidate must be the ROOT1+ROOT2+ROOT3 sequence: {candidates:?}"
        );
    }
}

/// **The oracle-side half of the same finding, made non-vacuous per this task's own caveat.** At
/// `Morpher`'s DEFAULT `max_stem_count` (2, `Morpher::new`'s own ctor default), `tevimaflisra`
/// confirms ZERO analyses -- but that zero is a SEPARATE, independent resource ceiling (the default
/// cap itself), not evidence about the FST proposer's own capability. A containment check run only
/// at the default cap would therefore be VACUOUSLY true (propose returns >=1, confirm returns 0 --
/// EITHER direction of containment against an empty set is trivially true, proving nothing) and
/// would not exercise the real recall claim at all. This test raises the cap via
/// `Morpher::with_max_stem_count(3)` (mirroring C#'s own `CompoundingRuleTests.SimpleRules`
/// reconfiguration, `Morpher.cs:72`/cs:87,105) so the oracle genuinely accepts the 3-stem analysis --
/// setting up `depth_budgeted_compound_loop_contains_the_raised_cap_oracle_analysis` (below) to make
/// a REAL, non-vacuous containment claim.
#[test]
fn raised_cap_oracle_finds_the_recursive_analysis_confirm_at_default_would_miss() {
    let g = load();

    // Default max_stem_count (2 -- `Morpher::new`'s own ctor default; the `usize::MAX` argument
    // here is the UNRELATED step-budget `cap` parameter, left uncapped so it never interferes):
    // reproduces the STAGING.md-pinned zero, confirming the vacuity concern is real for THIS
    // fixture, not hypothetical.
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

/// **The load-bearing containment proof (pieces 2/3, the gap the module doc's own history
/// records).** Propose (the real, unmodified, production `FomaProposer`) must CONTAIN the oracle's
/// own raised-cap analysis (`Morpher::with_max_stem_count(3)`, non-vacuous per the previous test) --
/// not merely propose SOMETHING, but propose the EXACT morpheme sequence confirm independently
/// accepts. This is the proposer-to-confirm containment proof this module's own promotion criteria
/// requires, now checked against the REAL depth-budgeted compound loop rather than merely argued.
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

/// Builds a small, self-contained (not the staged fixture) `CompoundingRule` grammar with an exact,
/// small, hand-picked depth bound: one isolated rule, `multipleApplication` set to
/// `extra_levels - 1`... actually stated directly as the max_apps value, since (isolated rule,
/// `max_depth = 1 + max_apps`) is this module's own established equivalence
/// (`compounding_max_depth`'s own doc). `root_count` distinct CVCV roots are declared, freely
/// licensed on both head and non-head sides (no MPR/PoS restriction at all) -- enough to build a
/// word requiring any number of stems up to `root_count`.
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

/// [`small_bound_grammar_xml`]'s sibling for the OTHER way `max_depth` grows: `rule_count` DISTINCT
/// `CompoundingRule`s in one stratum, every one at the DTD default `multipleApplication="1"`.
///
/// This is the real `sena` shape (8 such rules, measured), and the shape that makes
/// `compounding_max_depth` a rule COUNT rather than a nesting DEPTH: `1 + 1 + (rule_count - 1)`.
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

/// **THE RULE-COUNT-VERSUS-DEPTH CONFLATION, proven by collision rather than described.**
///
/// `compounding_max_depth` sums `max_apps` across the transitive closure of rules that COULD feed a
/// rule. That makes it blind to a distinction which decides how big a construction gets:
///
/// - **ONE** rule at `multipleApplication="4"` -- a rule that genuinely may re-apply to its own
///   output four times, so a single derivation really can reach five stems;
/// - **FOUR** rules at the DTD default `multipleApplication="1"` -- four ALTERNATIVE ways to
///   compound, none of which may apply twice at all.
///
/// Both compute `max_depth == 5`. The formula cannot tell them apart, because the first quantity is
/// typology and the second is grammar-counting. Eight ways to compound is not nine levels of nesting,
/// and this test is that sentence as an executable collision -- it needs no FST, no corpus and no
/// emitter, so nothing else can confound it.
///
/// That multiplier is live, not hypothetical: the private `sena` grammar declares **8**
/// `CompoundingRule`s, none with `multipleApplication`, so it lands on `max_depth = 9` and
/// `crate::emit::compound_extra_levels_checked` unrolls **8** non-head root levels for it.
///
/// The OPERATIVE bound is much smaller and lives elsewhere entirely: C#'s `Morpher.MaxStemCount`
/// (ctor default **2**), ported as [`pg_rules::stratum::AnalyzerConfig::max_stem_count`], gates
/// `Compounding` rule application as soon as `non_heads.len() + 1 >= max_stem_count`, so a
/// DEFAULT-configured engine confirms at most two stems.
/// `raised_cap_oracle_finds_the_recursive_analysis_confirm_at_default_would_miss` in this same
/// file already pins that half against the staged fixture: at the default cap the 3-stem compound
/// confirms ZERO analyses, and only `with_max_stem_count(3)` makes it one.
///
/// **Deliberately a PIN, not a behavior change.** The over-approximating direction is sound, and the
/// deeper levels are real recall for a raised-cap caller --
/// `depth_budgeted_compound_loop_contains_the_raised_cap_oracle_analysis` (this file) would break if
/// the construction were simply clamped to the default. Sizing the construction by
/// `min(ceiling, operative stem bound)` requires the operative bound to travel from whoever sets
/// `max_stem_count` into `crate::emit`, with that test moving in lockstep; see
/// `crate::capability::compounding_max_depth`'s own doc for the full write-up. What this guarantees
/// meanwhile is that the conflation is ASSERTED, so it cannot be quietly re-argued away.
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

/// **The depth-BOUND-respected gate.** A grammar whose computed `max_depth` bound is exactly `k`
/// (here `k = 3`: one isolated `CompoundingRule`, `multipleApplication="2"`, so
/// `max_depth = 1 + 2 = 3` -- this module's own established isolated-rule equivalence) must propose
/// a `k`-stem word (3 roots concatenated) but must NOT propose a `k+1`-stem word (4 roots
/// concatenated) -- over-approximation is licensed up to the computed bound, never past it.
/// `build_compound_chain` only ever unrolls `max_depth - 1 = 2` extra non-head levels for this
/// grammar, so a 4-root word is structurally unreachable through it, exactly mirroring the
/// pre-task-4.1 "one extra root" shape's own bound at k=2.
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

/// **The budget gate.** A `CompoundingRuleDef` with a `multipleApplication` value far beyond the
/// DTD's practical ceiling (9) computes an enormous `max_depth` bound -- `crate::emit`'s own
/// `DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET` (200) must refuse this grammar with a typed
/// `FomaTier::Unsupported` outcome, checked BEFORE any lexc text is written, rather than unrolling
/// 60,000 chain levels (a hang/OOM risk this task's own brief names as the real cost concern). No
/// env var mutation needed (unlike `tests/cover_compounding_budget.rs`'s own `HC_COMPOUND_PAIR_BUDGET`
/// convention): the DEFAULT budget itself is what this grammar is deliberately built to exceed, so
/// this test needs no process-global state and can run safely alongside every other test in this
/// crate's default parallel test execution.
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
