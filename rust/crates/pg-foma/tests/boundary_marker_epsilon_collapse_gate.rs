//! Regression pin for the `finish_controllable_net` precision regression
//! (`docs/fst-plan/large-lexicon-proposal-explosion.md`): a synthetic fixture reproducing the exact
//! boundary-token pathology that document's own commit trail flagged as "owed" ("A synthetic fixture
//! reproducing the boundary-token pathology is owed" -- no checked-in fixture exercised it before
//! this).
//!
//! # The shape this fixture reproduces
//! Sena's own explosion traced to ONE specific construction: an affix allomorph whose ENTIRE
//! underlying shape is composed only of `Boundary`-kind characters (Sena's compounding allomorph
//! `"^0+"` -- a null/zero-morph marker immediately followed by an ordinary separator, nothing else).
//! [`pg_foma::uflexc`]'s prefix/suffix continuation classes are deliberately self-looping (that
//! module's own doc), so once EVERY character of such an allomorph is deleted by the boundary
//! cleanup step, its lexc line degenerates to a bare, zero-width, epsilon-tagged entry whose own
//! continuation loops back to the state it came from -- a free, repeatable insertion point available
//! at every prefix juncture, not a one-off oddity. This fixture is the minimal grammar with that
//! shape: one ordinary ("p"-spelled) prefix, one all-`Boundary` ("marker + plain separator"-spelled)
//! prefix, and one bare root -- small enough to build and query in well under a second, but built
//! from the SAME two-kind `Boundary` split (one single-representation separator, one
//! multi-representation marker family) `boundary_cleanup_net` itself branches on.
//!
//! # What the fix actually is
//! NOT "exclude the marker family from cleanup" -- that was tried first and rejected, because
//! excluding ANY `Boundary` char-def from deletion makes every entry containing it unreachable by a
//! real surface query, which is a straight recall regression (the synthetic test below caught it
//! immediately: `MultiplicityMismatch { word: "s", expected: 2, actual: 1 }`). The cleanup still
//! blanket-deletes every `Boundary` char-def, exactly as it always did.
//!
//! The fix is `build::reroute_null_shaped_affix_chains`, applied to a group's RAW `uflexc` lexc source
//! BEFORE it is compiled, so a line whose entire underlying text is drawn only from boundary tokens
//! never reaches the compiled `Fsm` sitting on a self-looping continuation in the first place. That
//! mirrors what `crate::emit` already does successfully -- boundary characters never go onto the
//! queryable tape at all -- instead of emitting them and deleting them afterwards.
//!
//! # Which half of the gate each test actually pins -- measured, not assumed
//! The synthetic test below pins the RECALL half only. It does NOT pin precision: verified by
//! bypassing `reroute_null_shaped_affix_chains` at its call site with `pg-foma` genuinely rebuilt,
//! after which this fixture still reports `total_proposals <= 20` and PASSES. Its two words are too
//! short and it has too few root rules for the epsilon cycle to multiply past the ceiling. A gate
//! that cannot distinguish the fixed build from the broken one is not a precision gate, so this file
//! does not pretend otherwise.
//!
//! The precision half is pinned by [`corpus_large_lexicon_proposals_stay_bounded_after_the_reroute`],
//! on the real grammar where the defect manifests: 575 proposals with the fix, 53,992 with it
//! bypassed, over the same deterministic 5-word slice. That one has been confirmed to fail on the
//! broken build.
//!
//! Delanguaged per this repo's own conformance-grammar convention (`s`/`p` segments, no real
//! language's morphology): this is a synthetic construction pinning a specific FST-construction
//! defect, not a language sample.

use pg_conformance_fixtures::corpus;
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::recipe_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::Grammar;

/// One ordinary prefix (`mrRealPfx`, inserts literal `p`), one all-`Boundary` prefix (`mrNullPfx`,
/// inserts `^0+` -- a two-representation marker char-def, `cNull`, immediately followed by a
/// single-representation separator, `cPlus` -- the EXACT shape Sena's own `"^0+"` allomorphs have),
/// and one bare root (`s`). Both rules are stratum-level standalone rules (`morphologicalRules`
/// id-list), no `AffixTemplate` needed -- `uflexc`'s own model reads every `AffixProcess` rule
/// directly (module doc), template membership is irrelevant to it.
const FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>BoundaryMarkerEpsilonCollapseFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
        <BoundaryDefinition id="cNull"><Representations><Representation>^0</Representation><Representation>*0</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrRealPfx mrNullPfx">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrRealPfx" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>RealPrefix</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="mrRealPfxS">
                <MorphologicalInput>
                  <PhoneticSequence id="stem1">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <InsertSegments><PhoneticShape>p</PhoneticShape></InsertSegments>
                  <CopyFromInput index="stem1" />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <Gloss>RPX</Gloss>
          </MorphologicalRule>
          <MorphologicalRule id="mrNullPfx" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>NullPrefix</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="mrNullPfxS">
                <MorphologicalInput>
                  <PhoneticSequence id="stem2">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <InsertSegments><PhoneticShape>^0+</PhoneticShape></InsertSegments>
                  <CopyFromInput index="stem2" />
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <Gloss>NPX</Gloss>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
        <LexicalEntries>
          <LexicalEntry id="root1" partOfSpeech="posV">
            <Allomorphs><Allomorph id="root1a0"><PhoneticShape>s</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>root</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// Mirrors `recipe_runtime_net_is_queryable_gate.rs`'s own `materialize_and_evaluate` helper
/// (duplicated rather than shared across a test-module boundary, matching that file's own
/// convention for its synthetic fixtures) -- drives ONLY public API, never `recipe_runtime.rs`/
/// `recipe_optimize.rs` internals.
fn materialize_and_evaluate(
    grammar: &Grammar,
    words: &[String],
) -> Vec<pg_foma::recipe_runtime::RuntimeEvaluation> {
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(grammar);
    let baseline = enumerate_default(grammar, &alphabet, &prules, phonology.as_ref());
    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");
    let plans: Vec<_> = candidates.into_iter().map(|(_, p)| p).collect();
    assert!(!plans.is_empty(), "must materialize at least one candidate");
    evaluate_plans(grammar, &plans, words, RuntimeBudget::default())
}

/// The bare root and one-real-prefix words: both must still fully confirm (recall preserved by the
/// fix -- the ordinary "p" prefix is untouched), and their SUMMED proposal count must stay small.
/// Pinned threshold: measured `<= 20` total after the fix (typically single digits per word on a
/// grammar this size); measured far beyond that (hundreds, on a fixture two orders of magnitude
/// smaller than Sena) with `boundary_cleanup_net` reverted to blanket-deleting every `Boundary`
/// char-def identically -- see this test file's own module doc for the mechanism.
#[test]
fn null_morph_prefix_does_not_collapse_to_a_free_epsilon_loop() {
    let grammar = pg_grammar::load(FIXTURE_XML)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{FIXTURE_XML}"));

    let words: Vec<String> = vec!["s".to_string(), "ps".to_string()];
    let evaluations = materialize_and_evaluate(&grammar, &words);

    let confirmed: Vec<_> = evaluations
        .iter()
        .filter(|e| e.certification.selectable())
        .collect();
    assert!(
        !confirmed.is_empty(),
        "no candidate reached FullHcConfirmed on this fixture -- recall regressed (the ordinary \
         real-text prefix path must still work): {:?}",
        evaluations
            .iter()
            .map(|e| &e.certification)
            .collect::<Vec<_>>()
    );

    let total_proposals: u64 = confirmed.iter().map(|e| e.score.proposals).sum();
    eprintln!(
        "null_morph_prefix_does_not_collapse_to_a_free_epsilon_loop: total_proposals={total_proposals} \
         certifications={:?}",
        evaluations.iter().map(|e| &e.certification).collect::<Vec<_>>()
    );
    assert!(
        total_proposals > 0,
        "confirmed with zero proposals is a vacuous pass"
    );
    assert!(
        total_proposals <= 20,
        "proposal count {total_proposals} far exceeds what two short words should cost on a \
         2-root-rule fixture -- the all-Boundary null-morph prefix (`^0+`) has collapsed to a free \
         epsilon self-loop through `PrefixOrRoot` (this test file's own module doc explains the \
         mechanism)"
    );
}

/// MEASURED LIMITATION, stated rather than left implied: the synthetic test above does NOT pin the
/// precision fix. Verified by mutation — with `reroute_null_shaped_affix_chains` bypassed at its call
/// site in `build.rs`, and `pg-foma` genuinely rebuilt, this fixture still reports
/// `total_proposals <= 20` and passes. Its two words are too short and it has too few root rules for
/// the epsilon cycle to multiply into anything the ceiling would catch, so it cannot distinguish the
/// fixed build from the broken one.
///
/// What it IS still worth keeping for: it pins the RECALL half (a marker-bearing entry must stay
/// reachable — it is what caught the earlier attempt that excluded multi-representation boundary
/// char-defs from cleanup, `MultiplicityMismatch { word: "s", expected: 2, actual: 1 }`), and its
/// sibling pins the fixture's own structural assumption. Neither is nothing; neither is the
/// precision pin.
///
/// So the precision pin lives HERE, on the real grammar, where the defect actually manifests. The
/// A/B is unambiguous on a deterministic 5-word slice:
///   with the fix     ->    575 proposals total (dominant word: 499)
///   fix bypassed     -> 53,992 proposals total (dominant word: 53,720)
/// The ceiling below sits an order of magnitude clear of both, so it cannot pass on the broken build
/// and cannot flake on the fixed one.
///
/// The slice is DERIVED from the corpus at run time rather than hardcoded, for two reasons: the words
/// are real-language data and must not enter the repository, and deriving them keeps the slice honest
/// if the corpus changes. Hyphenated entries are dropped because this grammar's char table has no
/// hyphen, so they can only ever be `SKIPPED` (`docs/fst-plan/corpus-word-list-hazards.md`).
#[test]
#[ignore = "needs the private corpus at samples/data/sena-hc.xml; run with --include-ignored"]
fn corpus_large_lexicon_proposals_stay_bounded_after_the_reroute() {
    let grammar_path = corpus::require("sena-hc.xml");
    let words_path = corpus::require("sena-words.txt");
    let grammar = pg_grammar::load(&std::fs::read_to_string(&grammar_path).expect("read grammar"))
        .expect("grammar must load");

    let words: Vec<String> = std::fs::read_to_string(&words_path)
        .expect("read words")
        .replace('\r', "")
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty() && !w.contains('-'))
        .take(5)
        .map(str::to_owned)
        .collect();
    assert_eq!(words.len(), 5, "expected a 5-word slice, got {words:?}");

    let evaluations = materialize_and_evaluate(&grammar, &words);
    let total_proposals: u64 = evaluations.iter().map(|e| e.score.proposals).sum();
    let confirmed = evaluations
        .iter()
        .filter(|e| e.certification.selectable())
        .count();

    // Recall half: at least one candidate must still confirm. A "precision win" that stopped
    // proposing the right analysis would satisfy the ceiling below trivially and be worthless.
    assert!(
        confirmed > 0,
        "no candidate confirmed on the real grammar -- the reroute must not cost recall: {:?}",
        evaluations
            .iter()
            .map(|e| &e.certification)
            .collect::<Vec<_>>()
    );
    assert!(
        total_proposals > 0,
        "confirmed with zero proposals is a vacuous pass"
    );
    // Precision half.
    assert!(
        total_proposals < 5_000,
        "proposal total {total_proposals} on the 5-word slice indicates the null-shaped affix chain \
         is collapsing to a free epsilon self-loop again (bypassing \
         `build::reroute_null_shaped_affix_chains` measures 53,992 here; the fix measures 575). \
         Recall is fine — this is precision."
    );
    corpus::record_cases(
        "corpus_large_lexicon_proposals_stay_bounded_after_the_reroute",
        words.len(),
    );
}

#[test]
fn sanity_marker_family_is_the_multi_representation_boundary_and_plain_is_single() {
    // A cheap, independent structural check that this fixture actually carries the split
    // `boundary_cleanup_net` branches on -- if a future change to the loader ever collapsed
    // `cNull`'s two representations into one (or `cPlus` grew a second), the test above would stop
    // meaning what its own doc says it means, silently. This makes that loudly visible instead.
    let grammar = pg_grammar::load(FIXTURE_XML)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{FIXTURE_XML}"));
    let table = &grammar.char_tables[0];
    let mut single_rep_boundaries = 0usize;
    let mut multi_rep_boundaries = 0usize;
    for (_, cd) in table.iter() {
        if cd.kind() == pg_grammar::chardef::CharDefKind::Boundary {
            match cd.representations().len() {
                0 => panic!("a char-def with zero representations should be unreachable"),
                1 => single_rep_boundaries += 1,
                _ => multi_rep_boundaries += 1,
            }
        }
    }
    assert_eq!(
        single_rep_boundaries, 1,
        "expected exactly one plain (single-rep) boundary (`+`)"
    );
    assert_eq!(
        multi_rep_boundaries, 1,
        "expected exactly one marker-family (multi-rep) boundary (`^0`/`*0`)"
    );
}
