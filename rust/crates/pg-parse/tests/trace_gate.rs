//! P12 chunk 2 acceptance test (design doc §5, chunk 2): the smallest end-to-end tracing slice --
//! `Morpher::parse_word_traced` mints the root `WordAnalysis` node and wires the three morpher-level
//! `Failed(...)` reasons (`PartialParse`/`ObligatorySyntacticFeatures` in `is_word_valid_traced`,
//! `SurfaceFormMismatch` in `is_match_traced`) plus `Successful`. This file proves the handle threads
//! correctly from `parse_word`'s entry through to its exit without touching `pg_rules` internals.
//!
//! `ObligatorySyntacticFeatures` and `SurfaceFormMismatch` are exercised with real, deterministic
//! fixtures below (a rule that declares an obligatory feature it never actually contributes; a real
//! Indonesian word empirically confirmed, via this same tracing machinery, to produce a
//! `SurfaceFormMismatch`-rejected candidate on the normal parse path). `PartialParse` needs a
//! multi-stratum/template scenario this crate's shared test grammar helper does not cheaply support
//! (see `pg-parse/src/morpher.rs`'s own `#[cfg(test)]` module for a direct, hand-built-`Word` unit
//! test of that gate instead, since it is private-method-level and does not need a full grammar).

mod csharp_port_common;
use csharp_port_common::build_grammar;
use pg_parse::{Morpher, ParseOptions};
use pg_rules::trace::{FailureReason, TraceHandle, TraceType, TreeTraceSink};
use std::path::{Path, PathBuf};

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

/// The shared `ed_suffix`-shaped grammar (same shape as `word_timeout_gate.rs`'s `simple_grammar`,
/// duplicated here to keep this file self-contained): `posV` entry "32" = "sag", rule `ed_suffix`
/// appends "+d". A trivially-valid word.
fn valid_grammar() -> pg_grammar::model::Grammar {
    let mrules = r#"
      <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV"><Name>ed_suffix</Name><MorphemeId>PAST</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEd">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    build_grammar("", "", mrules, "mrEd", "")
}

/// A rule that declares `outputObligatoryFeatures="featEvid"` (the shared `featEvid`/`symWit`
/// evidential feature, never otherwise referenced by any fixture in this crate's shared grammar
/// helper — grepped clean) but whose own `<MorphologicalOutput>` never sets it (no
/// `<OutputHeadFeatures>` at all) and which no lexical entry provides either. Confirming this rule
/// during synthesis unconditionally pushes `featEvid`'s `FeatId` onto `Word::obligatory`
/// (`pg_rules::morph::synth_process_allomorph`, unconditional `w.obligatory.extend_from_slice`) while
/// `Word::syn_fs` never actually contains it -- a guaranteed, deterministic
/// `FailureReason::ObligatorySyntacticFeatures` rejection at `Morpher::is_word_valid_traced`'s second
/// clause.
fn obligatory_feature_never_satisfied_grammar() -> pg_grammar::model::Grammar {
    let mrules = r#"
      <MorphologicalRule id="mrObl" requiredPartsOfSpeech="posV" outputObligatoryFeatures="featEvid">
        <Name>obl_suffix</Name><MorphemeId>OBL</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subObl">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+z</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    build_grammar("", "", mrules, "mrObl", "")
}

/// Walk the whole tree collecting `(TraceType, FailureReason)` for every node that carries a
/// `failure_reason`, plus a flag for whether any `Successful` node exists.
fn scan(
    sink: &TreeTraceSink,
    h: pg_rules::trace::TraceHandle,
    reasons: &mut Vec<(TraceType, FailureReason)>,
    has_success: &mut bool,
) {
    let n = sink.node(h);
    if n.type_ == TraceType::Successful {
        *has_success = true;
    }
    if let Some(r) = n.failure_reason {
        reasons.push((n.type_, r));
    }
    for &c in &n.children {
        scan(sink, c, reasons, has_success);
    }
}

#[test]
fn traced_and_untraced_parse_agree_on_the_signature() {
    let g = valid_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let plain = m.parse_word("sagd");
    let sink = TreeTraceSink::new();
    let traced = m.parse_word_traced("sagd", &ParseOptions::default(), &sink);
    assert_eq!(
        plain.signature(),
        traced.signature(),
        "tracing must not change parse behavior"
    );
}

#[test]
fn trivially_valid_word_produces_a_successful_node_under_the_root() {
    let g = valid_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let sink = TreeTraceSink::new();
    let outcome = m.parse_word_traced("sagd", &ParseOptions::default(), &sink);
    assert!(
        !outcome.analyses.is_empty(),
        "sanity: the grammar still parses \"sagd\""
    );

    let root = sink.root().expect("analyze_word must mint a root");
    assert_eq!(sink.node(root).type_, TraceType::WordAnalysis);

    let mut reasons = Vec::new();
    let mut has_success = false;
    scan(&sink, root, &mut reasons, &mut has_success);
    assert!(
        has_success,
        "a valid parse must produce a Successful node somewhere in the tree"
    );
}

#[test]
fn obligatory_syntactic_feature_never_satisfied_is_reported() {
    let g = obligatory_feature_never_satisfied_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let sink = TreeTraceSink::new();
    let outcome = m.parse_word_traced("sagz", &ParseOptions::default(), &sink);
    // The rule's own obligatory feature is never satisfiable, so this word must never validate.
    assert!(
        outcome.analyses.is_empty(),
        "sanity: \"sagz\" must NOT validate (its obligatory feature is unsatisfiable)"
    );

    let root = sink.root().expect("analyze_word must mint a root");
    let mut reasons = Vec::new();
    let mut has_success = false;
    scan(&sink, root, &mut reasons, &mut has_success);
    assert!(!has_success);
    assert!(
        reasons.iter().any(
            |&(t, r)| t == TraceType::Failed && r == FailureReason::ObligatorySyntacticFeatures
        ),
        "expected a Failed(ObligatorySyntacticFeatures) node; got {reasons:?}"
    );
}

// =================================================================================================
// G4: the five previously-unwired analysis-side trace events
// (`begin_unapply_stratum`/`end_unapply_stratum`/`begin_unapply_template`/`end_unapply_template`/
// `lexical_lookup`) -- a synthetic, single-stratum, single-template grammar small enough to hand-
// derive the exact expected tree shape (see the wiring sites' own doc comments in
// `pg-rules/src/stratum.rs`'s `analyze`/`analyze_template`/`template_unapply_slots` and
// `pg-parse/src/morpher.rs`'s `lexical_lookup_filtered`).
// =================================================================================================

/// One stratum, no phonological rules, ONE affix template (`requiredPartsOfSpeech="posV"`) with a
/// single MANDATORY slot referencing a template-only rule (`mrEdT`, never listed in the stratum's
/// own `morphologicalRules` -- mirrors `csharp_port_affix_template.rs::non_final_template`'s
/// ordinary-vs-template-only convention). "sagd" unapplies as: `AnalysisStratumRule.Apply` ->
/// `ApplyTemplates` -> `AnalysisAffixTemplateRule.Apply` -> the one mandatory slot unapplies "+d"
/// off entry "32" ("sag", posV) -> falls through (no more slots) -> the fully-unapplied "sag"
/// survives as a second stratum output alongside "sagd" itself.
fn template_grammar() -> pg_grammar::model::Grammar {
    let mrules = r#"
      <MorphologicalRule id="mrEdT" requiredPartsOfSpeech="posV"><Name>ed_suffix_t</Name><MorphemeId>PASTT</MorphemeId>
        <MorphologicalSubrules>
          <MorphologicalSubrule id="subEdT">
            <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
            <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments></MorphologicalOutput>
          </MorphologicalSubrule>
        </MorphologicalSubrules>
      </MorphologicalRule>
    "#;
    let templates = r#"
      <AffixTemplate requiredPartsOfSpeech="posV"><Name>verbT</Name><Slot morphologicalRules="mrEdT"><Name>Sl1</Name></Slot></AffixTemplate>
    "#;
    build_grammar("", "", mrules, "", templates)
}

fn children_of(sink: &TreeTraceSink, h: TraceHandle) -> Vec<TraceHandle> {
    sink.node(h).children
}

fn find_all_by_type(
    sink: &TreeTraceSink,
    h: TraceHandle,
    ty: TraceType,
    out: &mut Vec<TraceHandle>,
) {
    let n = sink.node(h);
    if n.type_ == ty {
        out.push(h);
    }
    for &c in &n.children {
        find_all_by_type(sink, c, ty, out);
    }
}

fn is_descendant(sink: &TreeTraceSink, ancestor: TraceHandle, target: TraceHandle) -> bool {
    let mut stack = sink.node(ancestor).children;
    while let Some(h) = stack.pop() {
        if h == target {
            return true;
        }
        stack.extend(sink.node(h).children);
    }
    false
}

/// Pins all five newly-wired G4 events: that each fires, in the right order, and nested under the
/// right parent (mirroring the already-wired synthesis-side bookends' "no cursor reassignment on
/// Begin/marker events, but the ALREADY-WIRED rule-level event's cursor reassignment carries
/// forward" discipline).
#[test]
fn g4_unapply_stratum_and_template_bookends_nest_correctly() {
    let g = template_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let sink = TreeTraceSink::new();
    let outcome = m.parse_word_traced("sagd", &ParseOptions::default(), &sink);
    assert!(
        !outcome.analyses.is_empty(),
        "sanity: \"sagd\" must parse (root \"sag\" + template suffix \"+d\")"
    );

    let root = sink.root().expect("analyze_word must mint a root");
    let root_children = children_of(&sink, root);
    let root_child_types: Vec<TraceType> =
        root_children.iter().map(|&h| sink.node(h).type_).collect();

    // `BeginUnapplyStratum`/`EndUnapplyStratum`(for `input` itself)/`BeginUnapplyTemplate` never
    // reassign the trace cursor (mirroring the already-wired synthesis-side `begin_apply_stratum`/
    // `end_apply_stratum`/`begin_apply_template`), so all three fire as DIRECT children of root.
    assert!(
        root_child_types.contains(&TraceType::StratumAnalysisInput),
        "expected a StratumAnalysisInput (BeginUnapplyStratum) child of root; got {root_child_types:?}"
    );
    assert!(
        root_child_types.contains(&TraceType::StratumAnalysisOutput),
        "expected a StratumAnalysisOutput (EndUnapplyStratum, the `input`-itself exit, \
         AnalysisStratumRule.cs:124-125) child of root; got {root_child_types:?}"
    );
    assert!(
        root_child_types.contains(&TraceType::TemplateAnalysisInput),
        "expected a TemplateAnalysisInput (BeginUnapplyTemplate) child of root; got {root_child_types:?}"
    );

    // Order (children are appended in call order): Begin before End; End(input) before
    // BeginUnapplyTemplate (this port evaluates `apply_templates`/`apply_mrules` eagerly, so the
    // `input`-itself `EndUnapplyStratum` is placed textually before that computation starts --
    // see `analyze`'s own doc comment for why that reproduces C#'s lazy-`IEnumerable` event order).
    let pos = |ty: TraceType| {
        root_child_types
            .iter()
            .position(|&t| t == ty)
            .unwrap_or_else(|| panic!("missing {ty:?} among root's children: {root_child_types:?}"))
    };
    let begin_stratum = pos(TraceType::StratumAnalysisInput);
    let end_stratum = pos(TraceType::StratumAnalysisOutput);
    let begin_template = pos(TraceType::TemplateAnalysisInput);
    assert!(
        begin_stratum < end_stratum,
        "BeginUnapplyStratum must fire before EndUnapplyStratum; got positions {begin_stratum} \
         then {end_stratum} in {root_child_types:?}"
    );
    assert!(
        end_stratum < begin_template,
        "EndUnapplyStratum(input) must fire before BeginUnapplyTemplate; got positions \
         {end_stratum} then {begin_template} in {root_child_types:?}"
    );

    // The mandatory slot's OWN level always exits `unapplied=false` for the ORIGINAL "sagd" word
    // (`AnalysisAffixTemplateRule.cs:71-72`) -- also a direct child of root (no cursor reassignment
    // there either); `TreeTraceSink::end_unapply_template` only sets `.output` when `unapplied`, so
    // "no `output`" identifies this exit.
    let mut template_outputs = Vec::new();
    find_all_by_type(&sink, root, TraceType::TemplateAnalysisOutput, &mut template_outputs);
    assert!(
        template_outputs
            .iter()
            .any(|&h| root_children.contains(&h) && sink.node(h).output.is_none()),
        "expected an `unapplied=false` TemplateAnalysisOutput (no `output` recorded) as a direct \
         child of root; got {:?}",
        template_outputs
            .iter()
            .map(|&h| (root_children.contains(&h), sink.node(h).output.is_some()))
            .collect::<Vec<_>>()
    );

    // The RECURSED (fully-consumed) level exits `unapplied=true` for the UNAPPLIED "sag" word
    // (`AnalysisAffixTemplateRule.cs:77-78`) -- and THAT word's own trace cursor was already
    // reassigned by the already-wired rule-level `MorphologicalRuleUnapplied` event
    // (`morph.rs::ana_affix_cached_traced`), so this exit nests UNDER that rule event, not as a
    // second sibling of the bookends above.
    let mut mrule_events = Vec::new();
    find_all_by_type(&sink, root, TraceType::MorphologicalRuleAnalysis, &mut mrule_events);
    assert!(
        !mrule_events.is_empty(),
        "expected the already-wired MorphologicalRuleAnalysis (unapplication) event for mrEdT"
    );
    let true_exit = *template_outputs
        .iter()
        .find(|&&h| sink.node(h).output.is_some())
        .expect("expected an `unapplied=true` TemplateAnalysisOutput somewhere in the tree");
    assert!(
        mrule_events
            .iter()
            .any(|&mr| is_descendant(&sink, mr, true_exit)),
        "the `unapplied=true` TemplateAnalysisOutput must nest under the rule event that produced \
         its word (the reassigned trace cursor), not float as a root sibling"
    );

    // The per-word EndUnapplyStratum for the SURVIVING unapplied word ("sag") is subject to the
    // exact same cursor-reassignment rule (`AnalysisStratumRule.cs:141-143`, C#'s `output.Add(...)`
    // unconditional-then-trace idiom -- see `analyze`'s own doc comment): find a SECOND
    // StratumAnalysisOutput and confirm it nests under a MorphologicalRuleAnalysis event too,
    // rather than becoming a second direct child of root.
    let mut stratum_outputs = Vec::new();
    find_all_by_type(&sink, root, TraceType::StratumAnalysisOutput, &mut stratum_outputs);
    assert!(
        stratum_outputs.len() >= 2,
        "expected at least 2 EndUnapplyStratum events (one for `input` itself, one for the \
         surviving unapplied \"sag\" word); got {}",
        stratum_outputs.len()
    );
    let nested_stratum_output = stratum_outputs.iter().find(|&&h| !root_children.contains(&h));
    assert!(
        nested_stratum_output
            .is_some_and(|&h| mrule_events.iter().any(|&mr| is_descendant(&sink, mr, h))),
        "expected the surviving word's EndUnapplyStratum to nest under the rule event that \
         produced it, not directly under root"
    );

    // `LexicalLookup` (G4's fifth site, `Morpher::lexical_lookup_filtered`) fires during
    // synthesis-confirmation for the surviving analysis candidate(s).
    let mut lex = Vec::new();
    find_all_by_type(&sink, root, TraceType::LexicalLookup, &mut lex);
    assert!(
        !lex.is_empty(),
        "expected at least one LexicalLookup event (Morpher.cs:349-371)"
    );
}

/// `NoopSink`'s five newly-wired-in-G4 methods are all `unreachable!()` (see `pg_rules::trace`'s
/// module doc): a successful, non-panicking untraced parse over the SAME template grammar is
/// direct proof none of them were invoked on that path.
#[test]
fn g4_events_do_not_fire_when_tracing_is_off() {
    let g = template_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let untraced = m.parse_word("sagd"); // would panic if any NoopSink stub were reached.
    assert!(
        !untraced.analyses.is_empty(),
        "sanity: the grammar must still parse untraced"
    );

    let sink = TreeTraceSink::new();
    let traced = m.parse_word_traced("sagd", &ParseOptions::default(), &sink);
    assert_eq!(
        untraced.signature(),
        traced.signature(),
        "tracing must not change parse behavior for the template grammar either"
    );
}

/// `Morpher.cs` fires `LexicalLookup` from TWO sites -- the real-lexicon path
/// (`LexicalLookup`/`Morpher.cs:349-371`, exercised above) and the guesser path
/// (`LexicalGuess`/`Morpher.cs:373+`, `guess::lexical_guess` here). This grammar (duplicated from
/// `guesser_gate.rs`'s own fixture, self-contained per this file's existing convention) has NO
/// entry any real word can match at all -- only a lexical-PATTERN entry, which never enters the
/// trie (P11) -- so a guess-root parse reaches `lexical_guess` exclusively, confirming the second
/// site independently of the first.
fn guess_only_grammar() -> pg_grammar::model::Grammar {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>TraceGateGuess</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>Morphophonemic</Name>
        <LexicalEntries>
          <LexicalEntry id="ePattern">
            <Allomorphs><Allomorph id="aPattern"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pattern</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("guess-only fixture grammar failed to load: {e}"))
}

#[test]
fn g4_lexical_lookup_fires_on_the_guess_path_too() {
    let g = guess_only_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let sink = TreeTraceSink::new();
    // `guess_only` skips `lexical_lookup_filtered`'s own real-lexicon loop entirely (the OTHER
    // `LexicalLookup` call site, already confirmed by the test above) -- isolating this test to
    // the guess path (`guess::lexical_guess`) exclusively.
    let opts = ParseOptions::default()
        .with_guess_root(true)
        .with_guess_only(true);
    let outcome = m.parse_word_traced("gag", &opts, &sink);
    assert!(
        outcome.guessed && !outcome.analyses.is_empty(),
        "sanity: \"gag\" must only be reachable via the guesser (no real lexicon entry matches it)"
    );

    let root = sink.root().expect("analyze_word must mint a root");
    let mut lex = Vec::new();
    find_all_by_type(&sink, root, TraceType::LexicalLookup, &mut lex);
    assert!(
        !lex.is_empty(),
        "expected LexicalLookup to fire from the guess path (Morpher.cs's `LexicalGuess`, \
         `guess::lexical_guess` here) when the real-lexicon path finds nothing"
    );
}

/// Real-corpus fixture (self-skips if the untracked sample corpus isn't present on disk, matching
/// `reduplication_gate.rs`'s existing convention): empirically confirmed (via this same tracing
/// machinery, during this chunk's development) that Indonesian's "memaca" produces at least one
/// synthesis candidate that passes `is_word_valid_traced` but is rejected by `is_match_traced` with
/// `SurfaceFormMismatch` -- the real grammar naturally exercises the third wired reason, not just a
/// hand-built one.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn real_indonesian_word_exercises_surface_form_mismatch() {
    let Some(grammar_path) = sample_path("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let grammar = pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let m = Morpher::new(&grammar, usize::MAX);

    let sink = TreeTraceSink::new();
    let _outcome = m.parse_word_traced("memaca", &ParseOptions::default(), &sink);
    let root = sink.root().expect("analyze_word must mint a root");
    let mut reasons = Vec::new();
    let mut has_success = false;
    scan(&sink, root, &mut reasons, &mut has_success);
    assert!(
        reasons
            .iter()
            .any(|&(t, r)| t == TraceType::Failed && r == FailureReason::SurfaceFormMismatch),
        "expected at least one Failed(SurfaceFormMismatch) node for \"memaca\"; got {reasons:?}"
    );
}
