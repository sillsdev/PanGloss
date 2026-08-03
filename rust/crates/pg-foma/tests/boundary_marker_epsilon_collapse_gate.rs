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
//! # The SECOND regression of this class (see the section further down)
//! The fix above is name-scoped: `reroute_null_shaped_affix_chains` matches the two literal lexicon
//! names `PrefixChain`/`SuffixChain`. The bounded compound loop later added a per-level self-looping
//! prefix lexicon of its own (`UCmpPfx0`, `UCmp2Pfx0`, ...) built by re-emitting every prefix line --
//! null-shaped ones included -- and the guard could not see those names, so the same epsilon cycle came
//! back once per compound level. That class is now pinned by
//! [`compound_level_null_shaped_prefix_is_not_a_free_epsilon_loop`], and pinned STRUCTURALLY (on the
//! emitted lexc text) rather than by a proposal ceiling, precisely because the measured limitation
//! recorded above means a ceiling on a fixture this small cannot discriminate. The fix for it is
//! structural too, in `uflexc`'s own `prefix_hop` at emission time -- a name-based guard cannot defend
//! a lexicon that did not exist when the guard was written.
//!
//! Delanguaged per this repo's own conformance-grammar convention (`s`/`p` segments, no real
//! language's morphology): this is a synthetic construction pinning a specific FST-construction
//! defect, not a language sample.

use std::collections::HashSet;

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
        .expect("the oracle liveness net / memory ceiling must not trip on this fixture")
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
    // Printed unconditionally, not only inside the failure message: these are the one set of numbers
    // in this file that has ever discriminated a fixed build from a broken one, so a passing run
    // should still say what it measured (that is how the 575-vs-53,992 A/B in this test's own doc was
    // obtained, and how the next one will be). All three are DETERMINISTIC -- proposals, states, arcs.
    // Wall-clock is deliberately not reported: this machine runs several worktrees' builds
    // concurrently, so elapsed time cannot separate a real effect from neighbouring load.
    //
    // The FIRE COUNT for the compound-level at-most-once discipline is printed on the SAME input, from
    // a second, independent `emit_underlying` call, so "the mechanism engaged" and "the proposal count
    // is what it is" are two facts about one named grammar rather than two separate anecdotes.
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let emit = pg_foma::uflexc::emit_underlying(&grammar, &alphabet).expect("uflexc emit");
    eprintln!(
        "corpus_large_lexicon_proposals_stay_bounded_after_the_reroute: \
         total_proposals={total_proposals} confirmed_candidates={confirmed} \
         per_candidate_proposals={:?} per_candidate_states={:?} per_candidate_arcs={:?} \
         FIRE_COUNT_compound_null_shaped_prefix_hops_suppressed={} \
         prefix_entries={} root_entries={}",
        evaluations
            .iter()
            .map(|e| e.score.proposals)
            .collect::<Vec<_>>(),
        evaluations
            .iter()
            .map(|e| e.score.states)
            .collect::<Vec<_>>(),
        evaluations.iter().map(|e| e.score.arcs).collect::<Vec<_>>(),
        emit.compound_null_shaped_prefix_hops_suppressed,
        emit.prefix_entries,
        emit.root_entries
    );

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

// -------------------------------------------------------------------------------------------------
// The SECOND regression of this class: a compounding-licensed null-shaped prefix
// -------------------------------------------------------------------------------------------------
//
// `reroute_null_shaped_affix_chains` de-loops the two lexicons it knows BY NAME (`PrefixChain`,
// `SuffixChain`). The bounded compound loop (`uflexc`'s own "Bounded compound loop" section) then
// added a per-level self-looping prefix lexicon of its own -- `UCmpPfx0`, `UCmp2Pfx0`, ... -- built by
// re-emitting EVERY line in `prefix_lines`, null-shaped ones included, with the level's own lexicon as
// the continuation. The guard cannot see those names, so the epsilon cycle it had already closed once
// was reopened, once per compound level. **A name-based guard cannot defend a lexicon that was added
// after it**, which is why the fix is structural (`uflexc`'s `prefix_hop`, at emission time) and this
// section pins the structure rather than only a proposal count.
//
// The pin below is deliberately STRUCTURAL, not a proposal ceiling, and that is a lesson from the
// first regression rather than a shortcut: this file's own module doc records that the synthetic
// recall test above CANNOT distinguish the fixed build from the broken one (measured: it still passes
// with the fix bypassed), because a small fixture's epsilon cycle does not multiply past any ceiling
// worth pinning. A structural assertion has no such threshold to be under: a self-looping null-shaped
// line either is or is not in the emitted lexc text.

/// The minimal COMPOUNDING-licensed shape of the same pathology: one `CompoundingRule` with no MPR
/// restrictions at all (so `emit::compound_license` admits every entry as both head and non-head, and
/// the compound levels are genuinely emitted), two bare roots, one ordinary prefix (inserts `p`) and
/// one all-`Boundary` prefix (inserts `^0+`, the exact shape Sena's seven null-shaped allomorphs
/// have). Both affix rules are ordinary stratum-level `MorphologicalRule`s -- which is what Sena's
/// seven are too (all seven live on `MorphologicalRule` elements, NOT on the `CompoundingRule`s), so
/// they classify `Role::Prefix` and land in `prefix_lines`, which is precisely how they end up
/// re-emitted into every `UCmp{k}Pfx0`.
///
/// Delanguaged per this repo's own conformance-grammar convention (`s`/`p`/`t` segments).
const COMPOUND_FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>CompoundNullShapedPrefixEpsilonFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
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
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="cr1 mrRealPfx mrNullPfx">
        <Name>S</Name>
        <MorphologicalRuleDefinitions>
          <CompoundingRule id="cr1">
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
            <Gloss>rootS</Gloss>
          </LexicalEntry>
          <LexicalEntry id="root2" partOfSpeech="posV">
            <Allomorphs><Allomorph id="root2a0"><PhoneticShape>t</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>rootT</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// One entry line of `uflexc`-shaped lexc text, split the way
/// `build::reroute_line_if_null_shaped` splits it (first `:` not preceded by `%`).
fn split_entry_line(line: &str) -> Option<(&str, &str)> {
    let mut prev = '\0';
    for (i, c) in line.char_indices() {
        if c == ':' && prev != '%' {
            return Some((&line[..i], &line[i + 1..]));
        }
        prev = c;
    }
    None
}

/// Every `(lexicon, underlying, continuation)` entry line in `lexc_source`, in emission order.
fn entry_lines(lexc_source: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in lexc_source.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("LEXICON ") {
            current = Some(name.trim().to_string());
            continue;
        }
        let Some(lexicon) = current.as_deref() else {
            continue;
        };
        let Some((_tag, rest)) = split_entry_line(trimmed) else {
            continue;
        };
        let mut fields = rest.split_whitespace();
        let Some(underlying) = fields.next() else {
            continue;
        };
        let Some(continuation) = fields.next() else {
            continue;
        };
        out.push((
            lexicon.to_string(),
            underlying.to_string(),
            continuation.to_string(),
        ));
    }
    out
}

/// The `SegAlphabet` token characters standing for this grammar's `Boundary`-kind char-defs -- the
/// same set `build::boundary_tokens` computes in-crate, recomputed here because this is an
/// integration test and that function is `pub(crate)`.
fn boundary_token_set(grammar: &Grammar, alphabet: &SegAlphabet) -> HashSet<char> {
    grammar.char_tables[0]
        .iter()
        .filter(|(_, cd)| cd.kind() == pg_grammar::chardef::CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect()
}

/// THE NEW GATE CASE. A null-shaped prefix line inside any of the bounded compound loop's own
/// per-level prefix-hop lexicons (`UCmpPfx0`, `UCmp2Pfx0`, ...) must not continue back to the lexicon
/// it sits in. That self-loop is a free epsilon cycle for exactly the reason
/// `build::reroute_null_shaped_affix_chains`'s own doc records for `PrefixChain` (measured there:
/// 127 -> 53,992 proposals, 425x), and the guard that closed it for `PrefixChain` is name-scoped, so it
/// never covered these lexicons at all.
///
/// GENUINE BEFORE/AFTER: with `uflexc`'s `prefix_hop` restored to its pre-fix body (every line in
/// `prefix_lines` written with `entry` as its continuation), this test fails on the emitted text with
/// `UCmpPfx0` carrying a self-looping `^0+` line. It is not a threshold that a small fixture might
/// slip under -- there is no threshold.
#[test]
fn compound_level_null_shaped_prefix_is_not_a_free_epsilon_loop() {
    let grammar = pg_grammar::load(COMPOUND_FIXTURE_XML)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{COMPOUND_FIXTURE_XML}"));
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let boundary = boundary_token_set(&grammar, &alphabet);
    assert!(
        !boundary.is_empty(),
        "fixture must declare Boundary char-defs, or this test is vacuous"
    );

    let report = pg_foma::uflexc::emit_underlying(&grammar, &alphabet)
        .expect("uflexc emission must succeed on this fixture");
    let lines = entry_lines(&report.lexc_source);

    // FIRE COUNT (`UEmitReport::compound_null_shaped_prefix_hops_suppressed`'s own doc). A passing
    // suite cannot by itself distinguish a mechanism that ENGAGED from one that is dead code on this
    // path, so the mechanism reports how many times it acted and this gate refuses a zero. Before the
    // fix this number was structurally 0 (no such reroute existed) and the self-looping lines below
    // were all present; both halves of that A/B are asserted here, in one test, on one input.
    eprintln!(
        "compound_level_null_shaped_prefix_is_not_a_free_epsilon_loop: FIRE COUNT \
         compound_null_shaped_prefix_hops_suppressed={} prefix_entries={} root_entries={}",
        report.compound_null_shaped_prefix_hops_suppressed,
        report.prefix_entries,
        report.root_entries
    );
    assert!(
        report.compound_null_shaped_prefix_hops_suppressed > 0,
        "the at-most-once null-shaped discipline never fired on a fixture built specifically to \
         trigger it -- the mechanism is not on this path, so the structural assertions below would \
         be passing for the wrong reason. Emitted lexc:\n{}",
        report.lexc_source
    );

    let is_null_shaped = |underlying: &str| {
        !underlying.is_empty() && underlying.chars().all(|c| boundary.contains(&c))
    };
    // Non-vacuity 1: the compound loop is actually emitted, with at least one per-level prefix hop.
    let hop_lexicons: HashSet<&str> = lines
        .iter()
        .map(|(lex, _, _)| lex.as_str())
        .filter(|lex| lex.starts_with("UCmp") && lex.contains("Pfx"))
        .collect();
    assert!(
        !hop_lexicons.is_empty(),
        "no `UCmp*Pfx*` lexicon was emitted -- the compound loop did not run, so this test would \
         pass vacuously. Emitted lexc:\n{}",
        report.lexc_source
    );
    // Non-vacuity 2: a null-shaped prefix line really does reach those lexicons (this is the whole
    // exposure -- if `^0+` classified as anything other than `Role::Prefix` it would never be in
    // `prefix_lines` and so never in a compound prefix hop at all).
    let null_lines_in_hops = lines
        .iter()
        .filter(|(lex, u, _)| hop_lexicons.contains(lex.as_str()) && is_null_shaped(u))
        .count();
    assert!(
        null_lines_in_hops > 0,
        "no null-shaped line reached a `UCmp*Pfx*` lexicon -- the fixture no longer reproduces the \
         defect's precondition. Emitted lexc:\n{}",
        report.lexc_source
    );

    // THE ASSERTION.
    let self_looping: Vec<&(String, String, String)> = lines
        .iter()
        .filter(|(lex, u, cont)| {
            hop_lexicons.contains(lex.as_str()) && is_null_shaped(u) && cont == lex
        })
        .collect();
    assert!(
        self_looping.is_empty(),
        "{} null-shaped (all-Boundary) prefix line(s) sit on a SELF-LOOPING continuation inside the \
         bounded compound loop's own prefix hop -- a free epsilon cycle once the boundary cleanup \
         deletes them to nothing: {:?}\nEmitted lexc:\n{}",
        self_looping.len(),
        self_looping,
        report.lexc_source
    );

    // Recall half, structurally: the marker must still be REACHABLE (exactly once) and every
    // ORDINARY prefix must still be offered after it, in any quantity -- otherwise "no epsilon loop"
    // was bought by making the compound juncture's affixation narrower than the ground truth, which
    // is the mistake `reroute_null_shaped_affix_chains`'s own doc records being made and corrected
    // ("A first version of this function routed a null-shaped line straight to `RootBare`/`#`").
    let after_null_targets: HashSet<&str> = lines
        .iter()
        .filter(|(lex, u, _)| hop_lexicons.contains(lex.as_str()) && is_null_shaped(u))
        .map(|(_, _, cont)| cont.as_str())
        .collect();
    for target in &after_null_targets {
        let ordinary_offered = lines
            .iter()
            .filter(|(lex, u, cont)| lex == target && !is_null_shaped(u) && cont == target)
            .count();
        assert!(
            ordinary_offered > 0,
            "the post-marker lexicon `{target}` offers no self-looping ORDINARY prefix line, so a \
             real affix can no longer stack after the null marker at a compound juncture -- recall \
             regression, not a precision win. Emitted lexc:\n{}",
            report.lexc_source
        );
        let nulls_offered = lines
            .iter()
            .filter(|(lex, u, _)| lex == target && is_null_shaped(u))
            .count();
        assert_eq!(
            nulls_offered, 0,
            "the post-marker lexicon `{target}` offers a SECOND null-shaped line, which reopens the \
             cycle by another route. Emitted lexc:\n{}",
            report.lexc_source
        );
    }
}

/// The behavioral companion to the structural pin above: the compound path must still run end to end
/// through the full public runtime and still PROPOSE, with a bounded count and reported
/// states/arcs -- i.e. the structural change did not disconnect the compound levels or make the net
/// unqueryable.
///
/// **What this test deliberately does NOT claim, and why.** It does not require a
/// `FullHcConfirmed` certification. On a fixture where a null-shaped prefix can attach at the head
/// juncture, at the non-head juncture, or both, exact agreement between this proposer and the full-HC
/// oracle is a question about the proposer's general precision/recall on compound+null-morph
/// interaction -- a pre-existing property this change neither creates nor fixes -- and asserting it
/// here would make a fixture I authored the arbiter of it. The compound RECALL claim is carried where
/// it already lives and is already oracle-verified: `cross_compiler_equivalence_gate.rs` (RED-1),
/// `plan_composed_distinguishes_headedness_ambiguity_red2` (RED-2), and
/// `uflexc_compound_loop.rs` -- all of which must keep passing. What this file adds is the epsilon-loop
/// invariant, pinned structurally above.
///
/// Numbers reported are DETERMINISTIC only (proposals, states, arcs). No wall-clock: this machine runs
/// concurrent builds from several worktrees, so elapsed time cannot distinguish a real effect from
/// neighbouring load.
#[test]
fn compound_path_still_proposes_with_a_null_shaped_prefix() {
    let grammar = pg_grammar::load(COMPOUND_FIXTURE_XML)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{COMPOUND_FIXTURE_XML}"));

    // "s" -- bare root; "st" -- head + non-head compound (the juncture the null marker sits at);
    // "spt" -- compound with the ORDINARY prefix on the non-head span, which is what the compound
    // loop's prefix hop exists for in the first place.
    let words: Vec<String> = vec!["s".to_string(), "st".to_string(), "spt".to_string()];
    let evaluations = materialize_and_evaluate(&grammar, &words);

    assert!(
        !evaluations.is_empty(),
        "no candidate was evaluated at all on the compounding fixture"
    );
    let total_proposals: u64 = evaluations.iter().map(|e| e.score.proposals).sum();
    eprintln!(
        "compound_path_still_proposes_with_a_null_shaped_prefix: total_proposals={total_proposals} \
         per_candidate_proposals={:?} per_candidate_states={:?} per_candidate_arcs={:?} \
         certifications={:?}",
        evaluations
            .iter()
            .map(|e| e.score.proposals)
            .collect::<Vec<_>>(),
        evaluations.iter().map(|e| e.score.states).collect::<Vec<_>>(),
        evaluations.iter().map(|e| e.score.arcs).collect::<Vec<_>>(),
        evaluations
            .iter()
            .map(|e| &e.certification)
            .collect::<Vec<_>>()
    );
    assert!(
        total_proposals > 0,
        "the compounding fixture's net proposed NOTHING for any of {words:?} -- the compound levels \
         have been disconnected or the net is unqueryable, which is a recall regression, not a \
         precision win. Certifications: {:?}",
        evaluations
            .iter()
            .map(|e| &e.certification)
            .collect::<Vec<_>>()
    );
    assert!(
        total_proposals <= 500,
        "proposal count {total_proposals} is far more than three short words should cost on a \
         2-root fixture with one compound level"
    );
}

/// The fire count's NEGATIVE control, so `> 0` above is a statement about this mechanism and not
/// about "any grammar with a `^0+` prefix". The non-compounding fixture at the top of this file has
/// the identical null-shaped prefix allomorph but declares no `CompoundingRule`, so no compound level
/// is emitted and the compound-level discipline correctly never fires -- that grammar's null-shaped
/// prefix is handled entirely by `build::reroute_null_shaped_affix_chains` instead. A count that were
/// non-zero here would mean the counter is measuring something other than what its name says.
#[test]
fn the_fire_count_is_zero_when_no_compound_level_is_emitted() {
    let grammar = pg_grammar::load(FIXTURE_XML)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{FIXTURE_XML}"));
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let report = pg_foma::uflexc::emit_underlying(&grammar, &alphabet)
        .expect("uflexc emission must succeed on this fixture");
    assert!(
        !report.lexc_source.contains("LEXICON UCmp"),
        "this fixture declares no CompoundingRule, so no compound level should be emitted"
    );
    assert_eq!(
        report.compound_null_shaped_prefix_hops_suppressed, 0,
        "the compound-level null-shaped discipline reported firing on a grammar with no compound \
         level at all -- the counter is not measuring what it claims to"
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
