//! The SHAPE screen's gate: [`pg_foma::net_shape`] must separate a known-good finished net from a
//! known-pathological one, on real compiled `foma` networks, without applying a single word.
//!
//! # Why this file exists at all, and what it can do that the existing gate cannot
//! `boundary_marker_epsilon_collapse_gate.rs` records a MEASURED limitation of its own synthetic
//! fixture, in its module doc: with `build::reroute_null_shaped_affix_chains` bypassed and `pg-foma`
//! genuinely rebuilt, the fixture "still reports `total_proposals <= 20` and PASSES". A proposal
//! ceiling on a two-word fixture cannot distinguish the fixed build from the broken one, so the
//! precision half of that pin had to be exiled to an `#[ignore]`d test on a PRIVATE corpus
//! (`samples/data/sena-hc.xml`) that no ordinary `-Mode test` run ever executes.
//!
//! A STRUCTURAL check has no threshold to slip under. The defect is "some cycle in this net can be
//! traversed without consuming input", which is either true or false about a compiled automaton
//! regardless of how few words or how short a fixture is. So the tests below reproduce, on
//! *synthetic* grammars in the ORDINARY test suite, the discrimination that previously required
//! private real-language data:
//!
//! - [`prefix_chain_epsilon_cycle_is_visible_where_the_proposal_ceiling_is_not`] flags the FIRST
//!   regression of this class (the top-level `PrefixChain` one, on the very fixture whose proposal
//!   ceiling is documented as unable to see it).
//! - [`reconstructed_pre_fix_compound_emission_reopens_a_zero_width_cycle`] flags the SECOND (the
//!   per-compound-level one that `7644b52` fixed), by reconstructing that commit's parent's emission
//!   from the current emitter's own output and screening both.
//!
//! # SCOPE, restated because it is a hard constraint and not a preference
//! Nothing here — and nothing in [`pg_foma::net_shape`] — feeds a `Score` field, a ranking key, an
//! eligibility predicate, or a certification verdict, and no code path exists from the screen to a
//! decision to stop proposing. A pathological verdict is INFORMATION. Recall is not negotiable.
//!
//! Every number printed is a deterministic count. **No wall clock appears anywhere in this file**:
//! this machine runs several worktrees' builds concurrently, so elapsed time cannot separate a real
//! effect from a neighbour's load.

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

// -------------------------------------------------------------------------------------------------
// Fixtures
// -------------------------------------------------------------------------------------------------
//
// Both XML fixtures below are duplicated from `boundary_marker_epsilon_collapse_gate.rs` rather than
// shared, matching that file's own stated convention for its synthetic fixtures ("duplicated rather
// than shared across a test-module boundary"). They are pinned copies on purpose: this file's whole
// claim is about the SHAPE of the network those two exact grammars compile to, so a fixture that
// drifted underneath it would silently change what is being screened.
//
// Delanguaged per this repo's synthetic-conformance rule (`s`/`p`/`t` segments, no real language's
// morphology): these are constructions pinning an FST-construction defect, not language samples.

/// One ordinary prefix (inserts `p`), one all-`Boundary` prefix (inserts `^0+`), one bare root, NO
/// `CompoundingRule`. The FIRST regression's fixture: the pathology lives on the top-level
/// self-looping `PrefixChain`, which `build::reroute_null_shaped_affix_chains` de-loops.
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

/// The same shape plus ONE unrestricted `CompoundingRule`, so `uflexc`'s bounded compound loop is
/// genuinely emitted and its per-level `UCmp*Pfx0` prefix hops exist. The SECOND regression's
/// fixture — the one `7644b52` fixed at emission time in `uflexc::prefix_hop`.
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

// -------------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------------

/// A never-tripping budget built through the public constructor. `ComposeBudget::unbounded()` is
/// `#[cfg(test)]`-only inside the crate, so an integration test spells the equivalent out — the same
/// shape `grammar_semantics_owner_gate.rs` uses for the same reason. Deliberately never
/// `from_env()`: a budget read from process-global env state would make this gate's numbers depend on
/// the shell that launched it.
fn never_trips() -> ComposeBudget {
    ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    )
}

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
}

/// The ISOLATED `uflexc` pipeline: one lexc source in, one queryable net out.
///
/// Compile the lexc, then run the MANDATORY boundary-token cleanup + re-minimize
/// (`build::finish_controllable_net`). The cleanup is not an optimization and skipping it would make
/// this whole file vacuous in the most misleading possible way: the null-morph pathology does not
/// exist BEFORE cleanup (the arcs still require literal boundary characters, so they consume input
/// and no cycle is zero-width) and only appears AFTER it, which is exactly why a screen has to run
/// on the FINISHED net that `apply_up` will actually traverse.
///
/// No replace cascade and no gate partition: this deliberately isolates the ONE variable the tests
/// below vary, which is `uflexc`'s emitted continuation structure.
fn finished_net_from_lexc(grammar: &Grammar, alphabet: &SegAlphabet, lexc_source: &str) -> Fsm {
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, lexc_source)
        .expect("emitted lexc must compile to a network");
    pg_foma::build::finish_controllable_net(
        &opts,
        net,
        &grammar.char_tables[0],
        alphabet,
        &never_trips(),
    )
    .expect("the boundary-cleanup finish must not trip an unbounded budget")
}

/// The PRODUCTION plan-composed pipeline, i.e. exactly the network
/// `recipe_runtime::realize_plan_composed` measures and hands to a proposer: the default reified
/// plan, `build::build_controllable` (which applies `reroute_null_shaped_affix_chains` to each
/// group's raw lexc), then `finish_controllable_net`.
fn finished_production_net(grammar: &Grammar) -> Fsm {
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules = prules_in_order(grammar);
    let phonology = PhonologyProbe::new(grammar);
    let plan = enumerate_default(grammar, &alphabet, &prules, phonology.as_ref());
    let opts = FomaOptions::default();
    let budget = never_trips();
    let mut built =
        pg_foma::build::build_controllable(&plan, &opts, grammar, &alphabet, &prules, &budget)
            .expect("the default plan must build on a synthetic fixture");
    let net = built
        .net
        .take()
        .expect("the default plan must produce a network");
    pg_foma::build::finish_controllable_net(
        &opts,
        net,
        &grammar.char_tables[0],
        &alphabet,
        &budget,
    )
    .expect("the boundary-cleanup finish must not trip an unbounded budget")
}

/// The `SegAlphabet` token characters standing for this grammar's `Boundary`-kind char-defs.
/// Recomputed here because `build::boundary_tokens` is `pub(crate)` — the same duplication
/// `boundary_marker_epsilon_collapse_gate.rs` already does, for the same reason.
fn boundary_token_set(grammar: &Grammar, alphabet: &SegAlphabet) -> HashSet<char> {
    grammar.char_tables[0]
        .iter()
        .filter(|(_, cd)| cd.kind() == pg_grammar::chardef::CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect()
}

/// Splits one `uflexc`-shaped entry line at the first `:` not preceded by `%` (the tag's own
/// embedded colon is always escaped `%:`), returning `(tag, underlying, continuation)`.
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

/// Deletes every null-shaped line from the TOP-LEVEL `PrefixChain`/`SuffixChain` lexicons, returning
/// the rewritten source and how many lines it removed.
///
/// # Why the compound A/B has to do this to BOTH of its inputs
/// The top-level chains carry the SAME null-shaped allomorph the compound levels do, and their
/// exposure is closed by a completely different mechanism: `build::reroute_null_shaped_affix_chains`,
/// a private build-time rewrite of the raw lexc that the isolated pipeline in
/// [`finished_net_from_lexc`] does not run. Left in place, both sides of that A/B would carry a
/// top-level zero-width cycle and the compound-level difference under test would be masked by it.
///
/// Applied IDENTICALLY to both sides, so the single remaining variable is the compound prefix hop —
/// which is the only thing `7644b52` changed. Deleting rather than rerouting is deliberate: a
/// reroute reimplemented here could diverge from the real one and quietly become the thing under
/// test, whereas a deletion is trivially the same operation on both inputs. It is not
/// recall-preserving and is not meant to be; the top-level regression is screened on its own,
/// against the real production network, in
/// [`prefix_chain_epsilon_cycle_is_visible_where_the_proposal_ceiling_is_not`].
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
        if matches!(current.as_deref(), Some("PrefixChain") | Some("SuffixChain")) {
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

/// RECONSTRUCTS the emission `7644b52`'s PARENT produced, from the current emitter's own output.
///
/// That commit changed exactly one thing in `uflexc::prefix_hop`: a null-shaped prefix line inside a
/// compound level's `UCmp{k}Pfx0` lexicon used to be written with `UCmp{k}Pfx0` itself as its
/// continuation (closing a self-loop), and now goes to a new `UCmp{k}Pfx0AfterNull` lexicon that
/// re-offers every ORDINARY prefix but no null-shaped one. So the inverse is exactly two edits:
///
/// 1. every null-shaped line in a `UCmp*Pfx0` lexicon gets its continuation put back to its own
///    lexicon, and
/// 2. the `UCmp*Pfx0AfterNull` lexicon blocks — which did not exist before that commit — are dropped.
///
/// Returns `(pre_fix_lexc, lines_relooped, after_null_lexicons_dropped)`. Both counts are asserted
/// non-zero by the caller: a reconstruction that silently rewrote nothing would make the A/B below
/// compare a net against itself and pass for entirely the wrong reason.
///
/// This is a reconstruction and is labelled as one. It is preferred over checking out the parent
/// commit's `uflexc.rs` because that file no longer compiles against the current
/// `UEmitReport` (the fire-count field landed in the same commit), so the alternative is not "the
/// real parent" but "the parent plus unrelated edits".
fn reconstruct_pre_fix_compound_emission(
    lexc_source: &str,
    boundary: &HashSet<char>,
) -> (String, usize, usize) {
    let is_null_shaped =
        |u: &str| !u.is_empty() && u.chars().all(|c| boundary.contains(&c));
    // A compound-level prefix hop, as `uflexc::prefix_hop` names them: `UCmpPfx0` for level 1 and
    // `UCmp{k}Pfx0` after that. `*AfterNull` is the POST-fix sibling, never a hop itself.
    let is_hop = |lex: &str| {
        lex.starts_with("UCmp") && lex.contains("Pfx") && !lex.ends_with("AfterNull")
    };
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

// -------------------------------------------------------------------------------------------------
// The A/B that answers "does the screen separate 7644b52 from its parent"
// -------------------------------------------------------------------------------------------------

/// **THE FALSIFICATION TARGET.** One grammar, one emitter, two continuation structures differing by
/// exactly the line `7644b52` changed. The screen must call one bounded and the other pathological.
///
/// If this reported the same verdict for both, the screen would not be measuring the thing that
/// matters and this file would say so rather than be tuned until it agreed.
#[test]
fn reconstructed_pre_fix_compound_emission_reopens_a_zero_width_cycle() {
    let grammar = load(COMPOUND_FIXTURE_XML);
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let boundary = boundary_token_set(&grammar, &alphabet);
    assert!(
        !boundary.is_empty(),
        "fixture must declare Boundary char-defs, or the null-shaped classification is vacuous"
    );

    let report = pg_foma::uflexc::emit_underlying(&grammar, &alphabet)
        .expect("uflexc emission must succeed on this fixture");
    // Non-vacuity: the compound loop must genuinely be emitted, and the at-most-once discipline must
    // genuinely have fired, or "post-fix is clean" is a statement about a grammar with no exposure.
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

    // The SAME operation on both inputs (see `strip_top_level_null_shaped_affix_lines`'s own doc):
    // removes the top-level chains' exposure, which a different mechanism owns, so the one remaining
    // difference between these two sources is the compound prefix hop's continuation.
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

    // Printed unconditionally, passing or failing: these are the numbers that discriminate, and a
    // green run that says nothing cannot be used to obtain the next A/B.
    eprintln!(
        "reconstructed_pre_fix_compound_emission_reopens_a_zero_width_cycle: \
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
    // The FULL continuation graph is cyclic in BOTH -- `uflexc`'s affix chains are deliberately
    // self-looping. Asserted so the discrimination above is provably about ZERO-WIDTH cycles and not
    // about "one of these has loops and the other does not".
    assert!(
        post_fix.cycles.any() && pre_fix.cycles.any(),
        "both nets must contain ordinary (input-consuming) cycles, or the zero-width distinction is \
         not what separated them: post={} pre={}",
        post_fix.evidence_line(),
        pre_fix.evidence_line()
    );
}

/// The FIRST regression of this class, on the very fixture whose proposal ceiling is DOCUMENTED as
/// unable to see it.
///
/// `boundary_marker_epsilon_collapse_gate.rs`'s module doc records, measured: with
/// `reroute_null_shaped_affix_chains` bypassed and the crate genuinely rebuilt, its synthetic
/// fixture "still reports `total_proposals <= 20` and PASSES", which is why the precision half of
/// that pin lives in an `#[ignore]`d test against a private corpus. The structural screen has no
/// such blind spot: the raw `uflexc` emission for the SAME grammar carries a zero-width cycle and the
/// production (rerouted) network does not.
///
/// The two nets here come from two different pipelines (raw isolated `uflexc` emission vs. the
/// production `build_controllable` compose), so this is not a single-variable A/B like the compound
/// test above — it is "the screen flags the unguarded emission and clears the guarded one". Stated
/// rather than glossed, because `reroute_null_shaped_affix_chains` is private and there is no public
/// way to bypass it in place.
#[test]
fn prefix_chain_epsilon_cycle_is_visible_where_the_proposal_ceiling_is_not() {
    let grammar = load(PREFIX_CHAIN_FIXTURE_XML);
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);

    let raw = pg_foma::uflexc::emit_underlying(&grammar, &alphabet)
        .expect("uflexc emission must succeed on this fixture");
    // Non-vacuity: the exposure must exist in the raw text -- a null-shaped line sitting on the
    // self-looping `PrefixChain`.
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
        "prefix_chain_epsilon_cycle_is_visible_where_the_proposal_ceiling_is_not: \
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

/// The production plan-composed network for the compound fixture — the one a proposer actually
/// queries — screened end to end, with the branching distribution reported.
///
/// Asserts non-vacuity as hard as it asserts the verdict: a screen reporting zeros for a net it
/// failed to decode would satisfy `no zero-width cycle` trivially.
#[test]
fn production_compound_net_is_bounded_and_its_branching_is_reported() {
    let grammar = load(COMPOUND_FIXTURE_XML);
    let up = NetShape::inspect(&finished_production_net(&grammar), ApplyDirection::Up);
    eprintln!(
        "production_compound_net_is_bounded_and_its_branching_is_reported:\n  {}",
        up.evidence_line()
    );

    assert!(up.states > 0, "the screen decoded no states: {}", up.evidence_line());
    assert!(up.arcs > 0, "the screen decoded no arcs: {}", up.evidence_line());
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

// -------------------------------------------------------------------------------------------------
// Corpus-wide census: per-fixture structural numbers, computed without applying a word
// -------------------------------------------------------------------------------------------------

/// The fixtures this census cannot visit, verbatim from `parity_divergence_census.rs`'s own
/// `ABORTING_FIXTURES` and for the same measured reason: they kill the test PROCESS
/// (`STATUS_STACK_BUFFER_OVERRUN`, i.e. Rust's stack-overflow handler, not an allocation failure)
/// somewhere inside `evaluate_plans`, and an aborting fixture does not FAIL a census — it destroys
/// the measurement.
///
/// **This exclusion is an admission of a real limitation of this whole approach, not a tidy-up.**
/// A static screen reads a FINISHED net. If the process dies while that net is still being built,
/// there is nothing for the screen to read and it cannot help. Whether these two die in construction
/// or in traversal decides whether the screen would have caught them, and this census cannot answer
/// that question because it cannot reach them — see
/// [`aborting_fixture_uflexc_emission_probe`], which narrows it as far as is safe to narrow it here.
const ABORTING_FIXTURES: &[&str] = &["deep-optional-affix-nesting", "recipe-template-generic"];

/// Every discoverable conformance fixture whose grammar this crate can emit, screened on the
/// ISOLATED `uflexc` network. **Report-only on the verdict.**
///
/// Deliberately does NOT assert that every checked-in fixture is clean. There is no basis for that
/// claim yet — this screen is new, nobody has looked before, and a gate asserting a property nobody
/// has measured would either be tuned until it agreed or would block unrelated work. What IS
/// asserted is that the screen ran, on a non-trivial number of real nets, and produced non-trivial
/// structure. Any zero-width cycle found is printed with `FLAGGED` so it is loud and countable,
/// which is what makes this a tripwire: the number in a future run can be compared against the
/// number this run prints.
#[test]
fn conformance_fixture_shape_census() {
    for name in ABORTING_FIXTURES {
        eprintln!(
            "EXCLUDED from this census: {name} -- aborts the test process; see ABORTING_FIXTURES"
        );
    }
    let fixtures: Vec<FixtureRef> = discover()
        .into_iter()
        .filter(|f| !ABORTING_FIXTURES.contains(&f.name.as_str()))
        .collect();
    assert!(
        !fixtures.is_empty(),
        "no conformance fixture was discovered -- is the `machine` submodule initialized?"
    );

    let mut inspected = 0usize;
    let mut flagged: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut any_cyclic = false;
    for fixture in &fixtures {
        // Named BEFORE any work on it, so a process abort inside an unexpected fixture is
        // attributable from the last line of captured output rather than anonymous -- the same
        // lesson `parity_divergence_census.rs` records paying 252s to learn.
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

    // Non-vacuity, three ways. Without these a census that inspected nothing, or one whose walk
    // returned zeros for every net, would read as "everything is fine".
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

/// Narrows the open question [`ABORTING_FIXTURES`] leaves: for the fixture whose name literally
/// describes the pathological shape (`deep-optional-affix-nesting` — deep, optional, nested
/// affixation), does `uflexc` EMISSION and lexc compilation survive, so that a static screen would
/// have had a finished net to read?
///
/// Report-only, and asserts nothing about the shape. Two outcomes are both informative and neither
/// is a failure:
/// - emission and compile succeed → the screen WOULD have had something to read before any corpus
///   word was proposed, and the printed numbers say what it would have said;
/// - emission or compile refuses (a typed `ComposeError`, a budget trip, a caught panic) → the
///   process death happens at or before net construction, so no screen on a finished net could have
///   helped, and that limitation is recorded rather than glossed.
///
/// It does NOT call `evaluate_plans`, propose any word, or touch the full-HC oracle — the three
/// things this fixture is known to die inside. If it dies here anyway, `cargo-nextest` runs each
/// test in its own process, so the loss is this one test rather than the file.
#[test]
fn aborting_fixture_uflexc_emission_probe() {
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
