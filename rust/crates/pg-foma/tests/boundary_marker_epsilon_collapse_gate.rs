//! Regression pin: an all-`Boundary` affix allomorph collapses to a free epsilon self-loop.
//! See `docs/research/boundary-marker-epsilon-collapse-regression.md`.

use std::collections::HashSet;

use pg_conformance_fixtures::corpus;
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::Grammar;

/// One ordinary prefix (inserts `p`), one all-`Boundary` prefix (inserts `^0+`), one bare root.
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

/// Mirrors `backend_runtime_net_is_queryable_gate.rs`'s own helper; drives only public API.
fn materialize_and_evaluate(
    grammar: &Grammar,
    words: &[String],
) -> Vec<pg_foma::backend_runtime::RuntimeEvaluation> {
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(grammar);
    let baseline = enumerate_default(grammar, &prules, phonology.as_ref());
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

/// Recall must be preserved and the summed proposal count must stay small (pinned `<= 20`).
/// See `docs/research/boundary-marker-epsilon-collapse-regression.md`.
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

/// The precision pin, on the real grammar where the defect manifests.
/// See `docs/research/boundary-marker-epsilon-collapse-regression.md`.
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
    // Printed unconditionally: the only numbers in this file that ever discriminated a fixed build from a broken one.
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

    // Recall half: a "precision win" that stopped proposing the right analysis would be worthless.
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

// The second regression of this class: a compounding-licensed null-shaped prefix, pinned structurally.
// See `docs/research/boundary-marker-epsilon-collapse-regression.md`.

/// The minimal compounding-licensed shape of the same pathology.
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

/// One entry line of `uflexc`-shaped lexc text, split at the first `:` not preceded by `%`.
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

/// The `SegAlphabet` token characters standing for this grammar's `Boundary`-kind char-defs.
fn boundary_token_set(grammar: &Grammar, alphabet: &SegAlphabet) -> HashSet<char> {
    grammar.char_tables[0]
        .iter()
        .filter(|(_, cd)| cd.kind() == pg_grammar::chardef::CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect()
}

/// A null-shaped prefix line must not continue back to the lexicon it sits in.
/// See `docs/research/boundary-marker-epsilon-collapse-regression.md`.
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

    // Fire count: a passing suite cannot otherwise distinguish a mechanism that engaged from dead code.
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
    // Non-vacuity 2: a null-shaped prefix line really does reach those lexicons.
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

    // Recall half, structurally: the marker must stay reachable and ordinary prefixes must still be offered after it.
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

/// The behavioral companion to the structural pin above: the compound path must still propose with a bounded count.
#[test]
fn compound_path_still_proposes_with_a_null_shaped_prefix() {
    let grammar = pg_grammar::load(COMPOUND_FIXTURE_XML)
        .unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{COMPOUND_FIXTURE_XML}"));

    // "s" bare root; "st" head+non-head compound; "spt" compound with the ordinary prefix added.
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

/// The fire count's negative control: a non-compounding grammar must never trigger the compound-level discipline.
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
    // A cheap, independent check that this fixture carries the boundary-representation split the other tests assume.
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
