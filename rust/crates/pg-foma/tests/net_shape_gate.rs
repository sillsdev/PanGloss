//! The SHAPE screen's gate: `pg_foma::net_shape` must separate a known-good finished net from a known-pathological one, on real compiled `foma` networks, without applying a single word.

use std::collections::HashSet;

use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::types::Fsm;
use pg_conformance_fixtures::{discover, FixtureRef};
use pg_foma::compose_budget::ComposeBudget;
use pg_foma::enumerate::{enumerate_default, prules_in_order};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::net_shape::{ApplyDirection, NetShape, ShapeVerdict};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::Grammar;

// Both XML fixtures below are pinned copies duplicated from `boundary_marker_epsilon_collapse_gate.rs`, since this file's claim is about the exact network shape those two grammars compile to.

/// One ordinary prefix (inserts `p`), one all-`Boundary` prefix (inserts `^0+`), one bare root, no `CompoundingRule`: the first regression's fixture, whose pathology lives on the self-looping top-level `PrefixChain`.
const PREFIX_CHAIN_FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>NetShapePrefixChainFixture</Name>
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

/// The same shape plus one unrestricted `CompoundingRule`, so `uflexc`'s bounded compound loop and its per-level `UCmp*Pfx0` prefix hops are genuinely emitted: the second regression's fixture, fixed at emission time in `uflexc::prefix_hop`.
const COMPOUND_FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>NetShapeCompoundNullShapedPrefixFixture</Name>
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

/// A never-tripping budget built through the public constructor, since `ComposeBudget::unbounded()` is `#[cfg(test)]`-only inside the crate; never `from_env()`, so this gate's numbers can't depend on the launching shell's environment.
fn never_trips() -> ComposeBudget {
    ComposeBudget::with_caps(
        usize::MAX, usize::MAX)
}

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
}

/// The isolated `uflexc` pipeline: lexc source in, one queryable net out, through the mandatory boundary-token cleanup + re-minimize (`build::finish_controllable_net`) -- the null-morph pathology only appears after cleanup, so the screen must run on this finished net.
fn finished_net_from_lexc(grammar: &Grammar, alphabet: &SegAlphabet, lexc_source: &str) -> Fsm {
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, lexc_source)
        .expect("emitted lexc must compile to a network");
    pg_foma::build::finish_controllable_net(
        &opts,
        net,
        &grammar.char_tables[0],
        alphabet,
    )
}

/// The production plan-composed pipeline `backend_runtime::realize_plan_composed` hands to a proposer: `build::build_controllable` (applying `reroute_null_shaped_affix_chains`), then `finish_controllable_net`.
fn finished_production_net(grammar: &Grammar) -> Fsm {
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules = prules_in_order(grammar);
    let phonology = PhonologyProbe::new(grammar);
    let plan = enumerate_default(grammar, &prules, phonology.as_ref());
    let opts = FomaOptions::default();
    let budget = never_trips();
    let mut built =
        pg_foma::build::build_controllable(&plan, &opts, grammar, &alphabet, &prules, &budget)
            .expect("the default plan must build on a synthetic fixture");
    let net = built
        .net
        .take()
        .expect("the default plan must produce a network");
    pg_foma::build::finish_controllable_net(&opts, net, &grammar.char_tables[0], &alphabet)
}

/// The `SegAlphabet` token characters for this grammar's `Boundary`-kind char-defs; recomputed here since `build::boundary_tokens` is `pub(crate)`.
fn boundary_token_set(grammar: &Grammar, alphabet: &SegAlphabet) -> HashSet<char> {
    grammar.char_tables[0]
        .iter()
        .filter(|(_, cd)| cd.kind() == pg_grammar::chardef::CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect()
}

/// Splits one `uflexc`-shaped entry line at the first unescaped `:`, returning `(tag, underlying, continuation)`.
fn parse_entry_line(line: &str) -> Option<(&str, &str, &str)> {
    let mut sep = None;
    let mut prev = '\0';
    for (i, c) in line.char_indices() {
        if c == ':' && prev != '%' {
            sep = Some(i);
            break;
        }
        prev = c;
    }
    let sep = sep?;
    let tag = &line[..sep];
    let mut fields = line[sep + 1..].split_whitespace();
    let underlying = fields.next()?;
    let continuation = fields.next()?;
    Some((tag, underlying, continuation))
}

/// Deletes every null-shaped line from the top-level chains (closed by a different mechanism the isolated pipeline doesn't run), applied identically to both A/B sides; pinned by `net_shape_sees_the_prefix_chain_epsilon_cycle_a_proposal_ceiling_cannot`.
fn strip_top_level_null_shaped_affix_lines(
    lexc_source: &str,
    boundary: &HashSet<char>,
) -> (String, usize) {
    let is_null_shaped = |u: &str| !u.is_empty() && u.chars().all(|c| boundary.contains(&c));
    let mut out = String::with_capacity(lexc_source.len());
    let mut current: Option<String> = None;
    let mut removed = 0usize;
    for line in lexc_source.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("LEXICON ") {
            current = Some(name.trim().to_string());
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if matches!(
            current.as_deref(),
            Some("PrefixChain") | Some("SuffixChain")
        ) {
            if let Some((_, underlying, _)) = parse_entry_line(trimmed) {
                if is_null_shaped(underlying) {
                    removed += 1;
                    continue;
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    (out, removed)
}

/// Reconstructs the pre-fix emission by inverting `uflexc::prefix_hop`'s one change, rerouting a null-shaped continuation back to its own lexicon; the caller asserts both returned counts non-zero so this can't silently compare a net against itself.
fn reconstruct_pre_fix_compound_emission(
    lexc_source: &str,
    boundary: &HashSet<char>,
) -> (String, usize, usize) {
    let is_null_shaped = |u: &str| !u.is_empty() && u.chars().all(|c| boundary.contains(&c));
    // A compound-level prefix hop (`UCmpPfx0`, `UCmp{k}Pfx0`...); `*AfterNull` is the post-fix sibling, never a hop itself.
    let is_hop =
        |lex: &str| lex.starts_with("UCmp") && lex.contains("Pfx") && !lex.ends_with("AfterNull");
    let is_after_null =
        |lex: &str| lex.starts_with("UCmp") && lex.contains("Pfx") && lex.ends_with("AfterNull");

    let mut out = String::with_capacity(lexc_source.len());
    let mut current: Option<String> = None;
    let mut relooped = 0usize;
    let mut dropped = 0usize;
    for line in lexc_source.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("LEXICON ") {
            let name = name.trim().to_string();
            if is_after_null(&name) {
                dropped += 1;
                current = Some(name);
                continue;
            }
            current = Some(name);
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let Some(lex) = current.as_deref() else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        if is_after_null(lex) {
            // Every body line of a dropped lexicon goes with it.
            continue;
        }
        if is_hop(lex) {
            if let Some((tag, underlying, continuation)) = parse_entry_line(trimmed) {
                if is_null_shaped(underlying) && continuation == format!("{lex}AfterNull") {
                    relooped += 1;
                    out.push_str(&format!("{tag}:{underlying} {lex} ;\n"));
                    continue;
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    (out, relooped, dropped)
}

/// One grammar, one emitter, two continuation structures differing by exactly one fixed line: the screen must call one bounded and the other pathological, or it isn't measuring the thing that matters.
#[test]
fn net_shape_separates_the_pre_fix_compound_emission_from_the_fixed_one() {
    let grammar = load(COMPOUND_FIXTURE_XML);
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let boundary = boundary_token_set(&grammar, &alphabet);
    assert!(
        !boundary.is_empty(),
        "fixture must declare Boundary char-defs, or the null-shaped classification is vacuous"
    );

    let report = pg_foma::uflexc::emit_underlying(&grammar, &alphabet)
        .expect("uflexc emission must succeed on this fixture");
    // Non-vacuity: the compound loop and the at-most-once discipline must have genuinely fired.
    assert!(
        report.lexc_source.contains("LEXICON UCmp"),
        "the bounded compound loop was not emitted, so this fixture has no exposure at all"
    );
    assert!(
        report.compound_null_shaped_prefix_hops_suppressed > 0,
        "FIRE COUNT is zero: the at-most-once null-shaped discipline never engaged on a fixture \
         built to trigger it, so the post-fix net below is clean for the wrong reason"
    );

    let (pre_fix_lexc, relooped, dropped) =
        reconstruct_pre_fix_compound_emission(&report.lexc_source, &boundary);
    assert!(
        relooped > 0,
        "the pre-fix reconstruction re-looped ZERO null-shaped lines -- it compared the fixed net \
         against itself. Emitted lexc:\n{}",
        report.lexc_source
    );
    assert!(
        dropped > 0,
        "the pre-fix reconstruction dropped ZERO `*AfterNull` lexicons -- the post-fix emitter did \
         not produce the shape this reconstruction inverts. Emitted lexc:\n{}",
        report.lexc_source
    );

    // The same operation on both inputs removes the top-level chains' exposure, leaving the compound prefix hop's continuation as the one remaining difference.
    let (post_fix_lexc, post_stripped) =
        strip_top_level_null_shaped_affix_lines(&report.lexc_source, &boundary);
    let (pre_fix_lexc, pre_stripped) =
        strip_top_level_null_shaped_affix_lines(&pre_fix_lexc, &boundary);
    assert_eq!(
        post_stripped, pre_stripped,
        "the top-level strip must remove the same lines from both inputs, or the A/B has two \
         variables instead of one"
    );
    assert!(
        post_stripped > 0,
        "the top-level strip removed nothing, so the shared top-level exposure is still present in \
         both nets and would mask the compound-level difference. Emitted lexc:\n{}",
        report.lexc_source
    );

    let post_fix = NetShape::inspect(
        &finished_net_from_lexc(&grammar, &alphabet, &post_fix_lexc),
        ApplyDirection::Up,
    );
    let pre_fix = NetShape::inspect(
        &finished_net_from_lexc(&grammar, &alphabet, &pre_fix_lexc),
        ApplyDirection::Up,
    );

    // Printed unconditionally, passing or failing: these are the numbers that discriminate.
    eprintln!(
        "net_shape_separates_the_pre_fix_compound_emission_from_the_fixed_one: \
         FIRE_COUNT_hops_suppressed={} lines_relooped={relooped} after_null_lexicons_dropped={dropped} \
         top_level_null_lines_stripped_from_both={post_stripped}",
        report.compound_null_shaped_prefix_hops_suppressed
    );
    eprintln!("  POST-FIX (7644b52)        {}", post_fix.evidence_line());
    eprintln!("  PRE-FIX  (its parent)     {}", pre_fix.evidence_line());

    assert_eq!(
        post_fix.verdict(),
        ShapeVerdict::ZeroWidthBounded,
        "the CURRENT emitter's compound net must have no zero-width cycle: {}",
        post_fix.evidence_line()
    );
    assert!(
        pre_fix.verdict().is_pathological(),
        "the reconstructed PRE-FIX compound net must be flagged -- if the screen cannot separate \
         7644b52 from its parent it is not measuring automaton shape at all: {}",
        pre_fix.evidence_line()
    );
    assert!(
        pre_fix.zero_width_cycles.cycles >= 1,
        "{}",
        pre_fix.evidence_line()
    );
    // The full continuation graph is cyclic in BOTH, so the discrimination above is provably about zero-width cycles, not ordinary looping.
    assert!(
        post_fix.cycles.any() && pre_fix.cycles.any(),
        "both nets must contain ordinary (input-consuming) cycles, or the zero-width distinction is \
         not what separated them: post={} pre={}",
        post_fix.evidence_line(),
        pre_fix.evidence_line()
    );
}

/// The first regression of this class: the raw, un-rerouted `uflexc` emission carries a zero-width cycle a proposal-ceiling test cannot see, while the production (rerouted) network does not.
#[test]
fn net_shape_sees_the_prefix_chain_epsilon_cycle_a_proposal_ceiling_cannot() {
    let grammar = load(PREFIX_CHAIN_FIXTURE_XML);
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);

    let raw = pg_foma::uflexc::emit_underlying(&grammar, &alphabet)
        .expect("uflexc emission must succeed on this fixture");
    // Non-vacuity: the exposure must exist in the raw text as a null-shaped line on the self-looping `PrefixChain`.
    let boundary = boundary_token_set(&grammar, &alphabet);
    let mut current: Option<&str> = None;
    let mut null_lines_on_prefix_chain = 0usize;
    for line in raw.lexc_source.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("LEXICON ") {
            current = Some(name.trim());
            continue;
        }
        if current == Some("PrefixChain") {
            if let Some((_, underlying, _)) = parse_entry_line(trimmed) {
                if !underlying.is_empty() && underlying.chars().all(|c| boundary.contains(&c)) {
                    null_lines_on_prefix_chain += 1;
                }
            }
        }
    }
    assert!(
        null_lines_on_prefix_chain > 0,
        "no null-shaped line reached `PrefixChain`, so this fixture no longer reproduces the first \
         regression's precondition. Emitted lexc:\n{}",
        raw.lexc_source
    );

    let unguarded = NetShape::inspect(
        &finished_net_from_lexc(&grammar, &alphabet, &raw.lexc_source),
        ApplyDirection::Up,
    );
    let production = NetShape::inspect(&finished_production_net(&grammar), ApplyDirection::Up);

    eprintln!(
        "net_shape_sees_the_prefix_chain_epsilon_cycle_a_proposal_ceiling_cannot: \
         null_lines_on_PrefixChain={null_lines_on_prefix_chain}"
    );
    eprintln!("  RAW uflexc (unguarded)    {}", unguarded.evidence_line());
    eprintln!("  PRODUCTION (rerouted)     {}", production.evidence_line());

    assert!(
        unguarded.verdict().is_pathological(),
        "the raw, un-rerouted `uflexc` emission must be flagged -- this is the shape whose 425x \
         proposal explosion `reroute_null_shaped_affix_chains` exists to prevent, and which that \
         fixture's proposal ceiling provably cannot see: {}",
        unguarded.evidence_line()
    );
    assert_eq!(
        production.verdict(),
        ShapeVerdict::ZeroWidthBounded,
        "the PRODUCTION network must be clean: {}",
        production.evidence_line()
    );
}

/// The production plan-composed network for the compound fixture, screened end to end with branching reported; asserts non-vacuity as hard as the verdict, since a screen decoding nothing would trivially satisfy "no zero-width cycle".
#[test]
fn net_shape_of_the_production_compound_net_is_bounded_and_branching_is_reported() {
    let grammar = load(COMPOUND_FIXTURE_XML);
    let up = NetShape::inspect(&finished_production_net(&grammar), ApplyDirection::Up);
    eprintln!(
        "net_shape_of_the_production_compound_net_is_bounded_and_branching_is_reported:\n  {}",
        up.evidence_line()
    );

    assert!(
        up.states > 0,
        "the screen decoded no states: {}",
        up.evidence_line()
    );
    assert!(
        up.arcs > 0,
        "the screen decoded no arcs: {}",
        up.evidence_line()
    );
    assert!(
        up.branching.sampled_states > 0,
        "the branching distribution sampled no states: {}",
        up.evidence_line()
    );
    assert!(
        up.branching.max > 0,
        "every compiled net has at least one state with an outgoing arc: {}",
        up.evidence_line()
    );
    assert!(
        up.branching.p50 <= up.branching.p90
            && up.branching.p90 <= up.branching.p99
            && up.branching.p99 <= up.branching.max,
        "quantiles must be ordered: {}",
        up.evidence_line()
    );
    assert!(
        up.distinct_label_branching.max <= up.branching.max,
        "collapsing duplicate labels cannot widen the fan-out: {}",
        up.evidence_line()
    );
    assert_eq!(
        up.verdict(),
        ShapeVerdict::ZeroWidthBounded,
        "{}",
        up.evidence_line()
    );
}

/// The two fixtures that used to kill the test process during traversal, not net construction, so a static screen over the finished net has something to read.
const ABORTING_FIXTURES: &[&str] = &["deep-optional-affix-nesting", "backend-template-generic"];

/// Every discoverable conformance fixture screened on the isolated `uflexc` network, report-only on the verdict: asserts the screen ran and produced structure, never that every fixture is clean.
#[test]
fn net_shape_census_over_every_discoverable_conformance_fixture() {
    // No exclusions: this census reads finished nets and never proposes a word, so `ABORTING_FIXTURES` need no special-casing here.
    let fixtures: Vec<FixtureRef> = discover().into_iter().collect();
    assert!(
        !fixtures.is_empty(),
        "no conformance fixture was discovered -- is the `machine` submodule initialized?"
    );

    let mut inspected = 0usize;
    let mut flagged: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut any_cyclic = false;
    for fixture in &fixtures {
        // Named before any work on it, so a process abort is attributable from the last captured output line rather than anonymous.
        eprintln!("shape census: entering {}", fixture.label());
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let grammar = pg_grammar::load(&fixture.load_grammar_xml())
                .map_err(|e| format!("grammar failed to load: {e}"))?;
            let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
            let report = pg_foma::uflexc::emit_underlying(&grammar, &alphabet)
                .map_err(|e| format!("uflexc emission refused: {e}"))?;
            let net = finished_net_from_lexc(&grammar, &alphabet, &report.lexc_source);
            Ok::<NetShape, String>(NetShape::inspect(&net, ApplyDirection::Up))
        }));
        match outcome {
            Ok(Ok(shape)) => {
                inspected += 1;
                any_cyclic |= shape.cycles.any();
                let flag = if shape.verdict().is_pathological() {
                    "FLAGGED "
                } else {
                    "        "
                };
                eprintln!("  {flag}{}: {}", fixture.label(), shape.evidence_line());
                if shape.verdict().is_pathological() {
                    flagged.push(format!("{}: {}", fixture.label(), shape.evidence_line()));
                }
            }
            Ok(Err(reason)) => skipped.push(format!("{}: {reason}", fixture.label())),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "non-string panic payload".to_string());
                skipped.push(format!("{}: PANICKED -- {message}", fixture.label()));
            }
        }
    }
    for skip in &skipped {
        eprintln!("shape census skipped {skip}");
    }
    eprintln!(
        "shape census TOTAL: {inspected} nets inspected, {} skipped, {} FLAGGED with a zero-width \
         cycle",
        skipped.len(),
        flagged.len()
    );
    for f in &flagged {
        eprintln!("shape census FLAGGED {f}");
    }

    // Non-vacuity, three ways: without these a census that inspected nothing would read as "everything is fine".
    assert!(
        inspected >= 10,
        "only {inspected} of {} fixtures produced an inspectable net -- too few for the numbers \
         above to mean anything about this corpus",
        fixtures.len()
    );
    assert!(
        any_cyclic,
        "not one inspected net contained a cycle. `uflexc`'s affix chains are deliberately \
         self-looping, so this means the cycle walk found nothing anywhere and every clean verdict \
         above is a false negative"
    );
}

/// For each of `ABORTING_FIXTURES`: does emission and lexc compilation survive, so a static screen would have had a net to read? Report-only; never proposes a word or touches the full-HC oracle.
#[test]
fn net_shape_probe_of_the_two_process_aborting_fixtures() {
    for name in ABORTING_FIXTURES {
        let Some(fixture) = discover().into_iter().find(|f| f.name == *name) else {
            eprintln!("probe: {name} is not discoverable in this checkout -- nothing to report");
            continue;
        };
        eprintln!("probe: entering {}", fixture.label());
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let grammar = pg_grammar::load(&fixture.load_grammar_xml())
                .map_err(|e| format!("grammar failed to load: {e}"))?;
            let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
            let report = pg_foma::uflexc::emit_underlying(&grammar, &alphabet)
                .map_err(|e| format!("uflexc emission refused: {e}"))?;
            let lexc_lines = report.lexc_source.lines().count();
            let net = finished_net_from_lexc(&grammar, &alphabet, &report.lexc_source);
            Ok::<(usize, NetShape), String>((
                lexc_lines,
                NetShape::inspect(&net, ApplyDirection::Up),
            ))
        }));
        match outcome {
            Ok(Ok((lexc_lines, shape))) => eprintln!(
                "probe RESULT {}: net CONSTRUCTED (lexc_lines={lexc_lines}) -- a static screen \
                 would have read this BEFORE any corpus word: {}",
                fixture.label(),
                shape.evidence_line()
            ),
            Ok(Err(reason)) => eprintln!(
                "probe RESULT {}: net NOT constructed -- {reason}. The failure is at or before net \
                 construction, so a screen over a FINISHED net could not have caught it.",
                fixture.label()
            ),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "non-string panic payload".to_string());
                eprintln!(
                    "probe RESULT {}: net construction PANICKED -- {message}. Same conclusion: a \
                     screen over a finished net could not have caught it.",
                    fixture.label()
                );
            }
        }
    }
}
