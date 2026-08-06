//! The smallest end-to-end tracing slice: `Morpher::parse_word_traced` mints the root `WordAnalysis` node and wires the three morpher-level `Failed(...)` reasons plus `Successful`, proving the handle threads correctly from `parse_word`'s entry to its exit without touching `pg_rules` internals. `PartialParse` needs a scenario this crate's grammar helper can't cheaply support; see `pg-parse/src/morpher.rs`'s `#[cfg(test)]` module for a direct unit test of that gate instead.

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

/// The shared `ed_suffix`-shaped grammar, duplicated here to keep this file self-contained: `posV` entry "32" = "sag", rule `ed_suffix` appends "+d" -- a trivially-valid word.
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

/// A rule that declares `outputObligatoryFeatures="featEvid"` but whose output never sets it and which no lexical entry provides either -- a guaranteed, deterministic `FailureReason::ObligatorySyntacticFeatures` rejection.
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

/// Walk the whole tree collecting `(TraceType, FailureReason)` for every node that carries a `failure_reason`, plus a flag for whether any `Successful` node exists.
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

// G4: the five previously-unwired analysis-side trace events.
// See `docs/research/pg-parse-trace-gate-notes.md` for the full event list and wiring sites.

/// One stratum, no phonological rules, ONE affix template with a single MANDATORY slot referencing a template-only rule: "sagd" unapplies its "+d" off entry "32" ("sag"), and the fully-unapplied "sag" survives as a second stratum output alongside "sagd" itself.
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

/// Pins all five newly-wired G4 events: that each fires, in the right order, and nested under the right parent.
/// See `docs/research/pg-parse-trace-gate-notes.md` for the cursor-reassignment discipline this mirrors.
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

    // `BeginUnapplyStratum`/`EndUnapplyStratum`(for `input` itself)/`BeginUnapplyTemplate` never reassign the trace cursor, so all three fire as DIRECT children of root.
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

    // Order: Begin before End; End(input) before BeginUnapplyTemplate, since this port evaluates eagerly and places the `input`-itself `EndUnapplyStratum` before that computation starts.
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

    // The mandatory slot's OWN level always exits `unapplied=false` for the ORIGINAL "sagd" word -- `end_unapply_template` only sets `.output` when `unapplied`, so "no `output`" identifies this exit.
    let mut template_outputs = Vec::new();
    find_all_by_type(
        &sink,
        root,
        TraceType::TemplateAnalysisOutput,
        &mut template_outputs,
    );
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

    // The RECURSED (fully-consumed) level exits `unapplied=true` for the UNAPPLIED "sag" word, and that word's cursor was already reassigned by the rule-level event, so this exit nests UNDER that rule event, not as a second sibling of the bookends above.
    let mut mrule_events = Vec::new();
    find_all_by_type(
        &sink,
        root,
        TraceType::MorphologicalRuleAnalysis,
        &mut mrule_events,
    );
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

    // The per-word EndUnapplyStratum for the SURVIVING unapplied word is subject to the same cursor-reassignment rule: find a SECOND StratumAnalysisOutput nested under a MorphologicalRuleAnalysis event, not a second direct child of root.
    let mut stratum_outputs = Vec::new();
    find_all_by_type(
        &sink,
        root,
        TraceType::StratumAnalysisOutput,
        &mut stratum_outputs,
    );
    assert!(
        stratum_outputs.len() >= 2,
        "expected at least 2 EndUnapplyStratum events (one for `input` itself, one for the \
         surviving unapplied \"sag\" word); got {}",
        stratum_outputs.len()
    );
    let nested_stratum_output = stratum_outputs
        .iter()
        .find(|&&h| !root_children.contains(&h));
    assert!(
        nested_stratum_output
            .is_some_and(|&h| mrule_events.iter().any(|&mr| is_descendant(&sink, mr, h))),
        "expected the surviving word's EndUnapplyStratum to nest under the rule event that \
         produced it, not directly under root"
    );

    // `LexicalLookup` fires during synthesis-confirmation for the surviving analysis candidate(s).
    let mut lex = Vec::new();
    find_all_by_type(&sink, root, TraceType::LexicalLookup, &mut lex);
    assert!(
        !lex.is_empty(),
        "expected at least one LexicalLookup event (Morpher.cs:349-371)"
    );
}

/// `pg_rules::trace::NoopSink`'s five G4 methods are all `unreachable!()`: a successful, non-panicking untraced parse over the SAME template grammar is direct proof none were invoked on that path.
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

/// `Morpher.cs` fires `LexicalLookup` from TWO sites: the real-lexicon path (exercised above) and the guesser path. This grammar has no entry any real word can match, only a lexical-PATTERN entry, so a guess-root parse reaches `lexical_guess` exclusively, confirming the second site independently.
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
    pg_grammar::load(xml)
        .unwrap_or_else(|e| panic!("guess-only fixture grammar failed to load: {e}"))
}

#[test]
fn g4_lexical_lookup_fires_on_the_guess_path_too() {
    let g = guess_only_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let sink = TreeTraceSink::new();
    // `guess_only` skips `lexical_lookup_filtered`'s real-lexicon loop entirely, isolating this test to the guess path (`guess::lexical_guess`) exclusively.
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

/// Real-corpus fixture, self-skips if the untracked sample corpus isn't present on disk: a real grammar naturally exercising the third wired reason (`SurfaceFormMismatch`), not just a hand-built one.
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
