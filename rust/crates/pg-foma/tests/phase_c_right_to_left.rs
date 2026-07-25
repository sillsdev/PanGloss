//! `openspec/changes/compile-right-to-left-rewrites`: `Dir::RightToLeft` REAL directional
//! semantics, via [`pg_foma::replace::compile_rtl_branch_net`]'s reversal-plus-safety-net-union
//! construction (that function's own doc: reverse the mirror rule, `fsm_union` with the plain
//! `LeftToRight`-style compile — a documented, conservative response to a real, empirically-
//! verified gap in this repo's own oracle, see the "Known, out-of-scope oracle gap" section
//! below). BEFORE this change, a `multipleApplicationOrder="rightToLeftIterative"` rule was
//! silently compiled as if `LeftToRight` (design doc §5's "SILENT MIS-MAP" row), then later
//! honestly skipped (`is_fully_supported_shape` returning `false`, this file's OLD sole test).
//! Now it compiles with real directional semantics.
//!
//! Synthetic, delanguaged fixtures (`openspec/changes/STAGING.md`'s "Hard rule: synthetic data
//! only"), named by construct: `rtl-plain`, `rtl-feature-environment`, `rtl-deletion`,
//! `rtl-epenthesis`, `rtl-distinct-leftmost-rightmost`. Each (except the last) is a
//! proposer-to-confirm CONTAINMENT check against `pg_parse::Morpher` (this codebase's own full-HC
//! oracle), following `tests/two_table_symbol_divergence.rs`'s established methodology exactly
//! (`fst_candidate_set`/`oracle_candidate_set`, decode via `pg_foma::tags`).
//!
//! ## Known, out-of-scope oracle gap (discovered during this change, NOT fixed here)
//! `pg_rules::rewrite`'s own `Iterative` synthesis/analysis loops (`syn_feature`/`syn_narrow`/
//! `ana_feature`/…) pick which of several candidate spans to act on first via `all_spans`'/
//! `candidates.sort_unstable()`'s own ASCENDING sort, with NO dependence on `rule.dir` anywhere in
//! either loop or in the compiled target/environment FST's own matched-span SET (`pg_rules::
//! rewrite::compile_lane_fst`'s own doc: "direction changes scan order/preference, not the matched
//! string" — and the "preference" half is exactly what the unconditional ascending sort erases).
//! Verified directly (a throwaway probe, not checked in): a hand-built `aa -> b` rule applied via
//! `pg_rules::rewrite::synthesize` to `"aaa"` returns `"ba"` whether the rule is declared
//! `LeftToRight` or `rightToLeftIterative` — IDENTICAL output. This repo's own full-HC oracle is
//! therefore, today, direction-BLIND for "which overlapping match wins" — a pre-existing gap in
//! `pg_rules::rewrite`, entirely outside `replace.rs`'s single-owner boundary this change holds to.
//! ADR 0001 (`docs/adr/0001-honest-capability-boundary.md`): *"Where the oracle itself is
//! unverified for a configuration... the configuration is unsupported by definition — there is no
//! correct behavior to check against."* `rtl_distinct_leftmost_rightmost_differs_from_ltr_and_is_
//! recall_safe_against_the_current_oracle` (below) documents this precisely: it proves the
//! REVERSED branch alone genuinely differs from `LeftToRight` (a pure, oracle-independent FST
//! fact), and proves the UNION'd construction this file's other four tests already pin never
//! UNDER-recalls relative to today's oracle (safe superset) — it does NOT claim exact oracle
//! equality for the specific "which overlapping match wins" dimension, because no correct oracle
//! answer for that dimension exists in this repo today.

mod common;

use std::collections::HashSet;

use foma::apply::{apply_down, apply_init};
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::reverse::fsm_reverse;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::replace::{
    compile_and_compose_rules_with_budget, is_fully_supported_shape, SegAlphabet,
};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered_with_budget;
use pg_grammar::model::{Dir, Grammar, LexEntryId, PatternNode, PhonRuleDef};
use pg_grammar_gen::{ConstructKnobs, Recipe, ScaleKnobs};
use pg_parse::{Morpher, ParseOptions};

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}\n{xml}"))
}

fn entry_id_of(g: &Grammar, xml_id: &str) -> LexEntryId {
    LexEntryId(
        g.entries
            .iter()
            .position(|e| g.morphemes[e.morpheme.0 as usize].xml_key == xml_id)
            .unwrap_or_else(|| panic!("no entry with xml id {xml_id:?}")) as u32,
    )
}

/// Every DECODED `apply_up` candidate for `query` against `net` -- the FST-proposer half of the
/// containment check (`tests/two_table_symbol_divergence.rs`'s own helper, reused verbatim).
fn fst_candidate_set(net: &foma::types::Fsm, query: &str) -> HashSet<(i32, Vec<u32>)> {
    let mut out = HashSet::new();
    let mut handle = apply_init(net);
    for s in handle.up(query) {
        let Some(path) = tags::decode_path(&s) else {
            continue;
        };
        for c in tags::to_candidates(&path) {
            out.insert((c.root_index, c.morphemes.iter().map(|m| m.0).collect()));
        }
    }
    out
}

/// The full-HC oracle's own candidate set for `surface`, restricted to `allowed_morphemes`
/// (`tests/two_table_symbol_divergence.rs`'s own helper, reused verbatim).
fn oracle_candidate_set(
    morpher: &Morpher,
    surface: &str,
    allowed_morphemes: &HashSet<u32>,
) -> HashSet<(i32, Vec<u32>)> {
    let outcome = morpher.parse_word_opts(surface, &ParseOptions::default());
    outcome
        .structured
        .iter()
        .filter(|a| a.morpheme_ids.iter().all(|m| allowed_morphemes.contains(m)))
        .map(|a| (a.root_morpheme_index, a.morpheme_ids.clone()))
        .collect()
}

/// Compiles `rule` (stratum 0's own table) via [`compile_and_compose_rules_with_budget`], composes
/// it after `lexc_source`, and minimizes -- the shared plumbing every witness below uses.
fn compile_net(g: &Grammar, alphabet: &SegAlphabet, rule: &PhonRuleDef, lexc_source: &str) -> foma::types::Fsm {
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
    let lexc_net = fsm_lexc_parse_string(&opts, None, lexc_source)
        .unwrap_or_else(|| panic!("lexc must compile:\n{lexc_source}"));
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules_with_budget(
        &opts,
        g,
        alphabet,
        &[rule],
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("rule compile must not hit any budget: {e}"))
    .expect("RightToLeft rule must now compile to Some(net)");
    assert!(skipped.is_empty(), "rule must not be skipped: {skipped:?}");
    fsm_minimize(&opts, fsm_compose(&opts, lexc_net, rule_net))
}

// =================================================================================================
// rtl-plain: reuses `pg_grammar_gen`'s existing `rtl_rule_count` recipe (single fixed-segment LHS,
// no environment -- direction cannot matter here, since a single segment's own occurrences never
// overlap with each other). Proves basic compile success + containment now that the honest-skip
// gate this file used to pin is gone.
// =================================================================================================

fn plain_recipe() -> Recipe {
    Recipe {
        name: "rtl-plain",
        seed: 20260724,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            rtl_rule_count: 1,
            ..Default::default()
        },
    }
}

#[test]
fn rtl_plain_rule_now_compiles_and_matches_oracle() {
    let recipe = plain_recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = load(&rendered.xml);

    let rtl = rendered
        .right_to_left
        .as_ref()
        .expect("recipe declared rtl_rule_count > 0");
    assert_eq!(rtl.rule_xml_ids.len(), 1);
    assert_eq!(g.prules.len(), 1);
    assert_eq!(g.entries.len(), 1);

    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule at prules[0]");
    };
    assert_eq!(rule.dir, Dir::RightToLeft);
    assert!(
        is_fully_supported_shape(rule),
        "an Iterative RightToLeft rule must now be reported fully-supported"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);

    // The RHS's own single fixed segment names the expected (obligatory) output spelling.
    let PatternNode::CharDef(rhs_id) = rule.subrules[0].rhs.nodes[0] else {
        panic!("generator's rtl demo rule RHS must be a single fixed segment");
    };
    let out_text = table.get(rhs_id).representations()[0].clone();

    let root_entry = &g.entries[0];
    let root_morpheme = root_entry.morpheme.0;
    let allowed: HashSet<u32> = [root_morpheme].into_iter().collect();
    let in_text = root_entry.allomorphs[0].shape.text.clone();

    let alphabet_ref = &alphabet;
    let entries: HashSet<LexEntryId> = [LexEntryId(0)].into_iter().collect();
    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
    let uemit = emit_underlying_filtered_with_budget(&g, alphabet_ref, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    let query = alphabet
        .encode_query(&out_text)
        .unwrap_or_else(|| panic!("{out_text:?} must segment against table 0"));
    let fst_out = fst_candidate_set(&net, &query);
    let oracle_out = oracle_candidate_set(&morpher, &out_text, &allowed);
    assert_eq!(
        oracle_out.len(),
        1,
        "oracle must recall the root for its own obligatorily-rewritten surface {out_text:?}: {oracle_out:?}"
    );
    assert_eq!(
        fst_out, oracle_out,
        "CONTAINMENT: FST propose+decode must EQUAL the oracle for surface {out_text:?}"
    );

    // The root's own RAW (un-rewritten) spelling must never itself be a valid surface (obligatory
    // rule; no ambiguity possible with a single-segment LHS and no environment).
    let oracle_raw = oracle_candidate_set(&morpher, &in_text, &allowed);
    assert!(
        oracle_raw.is_empty(),
        "the raw un-rewritten spelling {in_text:?} must have no oracle analysis: {oracle_raw:?}"
    );
}

// =================================================================================================
// rtl-feature-environment: a feature-changing (LHS/RHS both width 1) rule gated by a real, ASYMMETRIC
// two-sided environment -- proves `left_env`/`right_env` genuinely swap-and-reverse under the
// mirror construction (a wrong swap would silently gate on the WRONG side and this containment
// check would fail), with only one valid match per word (no leftmost/rightmost ambiguity) so LTR-
// style and reversed constructions necessarily agree in OUTCOME here -- this is the "ordinary case
// stays correct" witness, not the discriminating one.
// =================================================================================================

const RTL_FEATURE_ENV_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>RtlFeatureEnvironment</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <!-- One distinct symbol value PER SEGMENT (not per natural-class membership) -- required so
           `pg_parse::Morpher`'s own analysis-side unapplication can disambiguate segments; two
           segments sharing one feature-value combination defeats it (a real, pre-existing
           `pg-rules` analysis-engine characteristic, unrelated to this change -- see
           `tests/two_table_symbol_divergence.rs`'s own "Known, out-of-scope anomaly" note for the
           same finding). Natural-class MEMBERSHIP is still by explicit `<Segment>` list below, so
           this feature has no bearing on which segments the rule's own classes contain. -->
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols>
          <Symbol id="symX">x</Symbol><Symbol id="symA">a</Symbol>
          <Symbol id="symB">b</Symbol><Symbol id="symY">y</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations><FeatureValue feature="featId" symbolValues="symX" /></SegmentDefinition>
        <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featId" symbolValues="symA" /></SegmentDefinition>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featId" symbolValues="symB" /></SegmentDefinition>
        <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations><FeatureValue feature="featId" symbolValues="symY" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncVoiced"><Name>Voiced</Name><Segment segment="ca" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncVoiceless"><Name>Voiceless</Name><Segment segment="cb" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncX"><Name>X</Name><Segment segment="cx" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncY"><Name>Y</Name><Segment segment="cy" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prDevoiceEnv" multipleApplicationOrder="rightToLeftIterative">
        <Name>devoiceEnvDemo</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncVoiced" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoiceless" /></PhoneticSequence></PhoneticOutput>
            <Environment>
              <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncX" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
              <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncY" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
            </Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prDevoiceEnv">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryContextful" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloContextful"><PhoneticShape>xay</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>contextful</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryNoRightContext" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloNoRightContext"><PhoneticShape>xa</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>noRightContext</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn rtl_feature_environment_swap_matches_oracle() {
    let g = load(RTL_FEATURE_ENV_XML);
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(rule.dir, Dir::RightToLeft);
    assert!(is_fully_supported_shape(rule));

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let entry_contextful = entry_id_of(&g, "entryContextful");
    let entry_no_right = entry_id_of(&g, "entryNoRightContext");
    let allowed: HashSet<u32> = [
        g.entries[entry_contextful.0 as usize].morpheme.0,
        g.entries[entry_no_right.0 as usize].morpheme.0,
    ]
    .into_iter()
    .collect();

    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
    let entries: HashSet<LexEntryId> = [entry_contextful, entry_no_right].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    // "xby": the devoiced surface of "xay" (right context 'y' present -- rule fires).
    let query = alphabet.encode_query("xby").expect("'xby' must segment");
    let fst_out = fst_candidate_set(&net, &query);
    let oracle_out = oracle_candidate_set(&morpher, "xby", &allowed);
    assert_eq!(oracle_out.len(), 1, "oracle must recall entryContextful for 'xby': {oracle_out:?}");
    assert_eq!(fst_out, oracle_out, "CONTAINMENT for 'xby' (right-context-gated devoice)");

    // "xay": the raw, un-rewritten spelling must never surface (rule is obligatory whenever both
    // sides of the environment hold).
    let oracle_raw = oracle_candidate_set(&morpher, "xay", &allowed);
    assert!(oracle_raw.is_empty(), "'xay' (obligatorily rewritten) must have no oracle analysis");

    // "xa": entryNoRightContext's own (unchanged) spelling -- right context 'y' is ABSENT, so the
    // devoice rule must NOT fire. If left_env/right_env were swapped-but-not-also-reversed (or not
    // swapped at all) this environment check would silently gate on the wrong side and the FST
    // would either wrongly devoice this too (making `fst_unchanged` empty/coincide with `fst_out`)
    // or reject it outright (empty) -- either failure mode is caught below.
    //
    // FST-ONLY (no oracle comparison here): `pg_parse::Morpher` has an independent, PRE-EXISTING,
    // out-of-scope limitation for this specific shape -- discovered while writing this test,
    // unrelated to RTL direction (reproduces identically for `LeftToRight`): `pg_rules::rewrite`'s
    // analysis side (`ana_feature`) rejects ANY surface containing an occurrence of the rule's OWN
    // LHS class (here, `ncVoiced`/'a') outright, REGARDLESS of whether the environment actually
    // holds there -- i.e. it does not correctly recognize "the class is present but the rule's own
    // environment doesn't license it, so leave it be" as a valid non-application. This is a
    // characteristic of `pg_rules::rewrite`'s environment-gated-obligatory-rule analysis, entirely
    // outside `replace.rs`'s single-owner boundary this change holds to -- flagged for a future
    // investigation, not silently avoided (see this file's own top doc for the OTHER, direction-
    // specific oracle gap this change already documents).
    let query_unchanged = alphabet.encode_query("xa").expect("'xa' must segment");
    let fst_unchanged = fst_candidate_set(&net, &query_unchanged);
    assert!(
        !fst_unchanged.is_empty(),
        "the FST must still propose entryNoRightContext's own spelling 'xa' unchanged -- the \
         devoice rule's environment (right context 'y') is absent, so it must not fire"
    );
    assert_ne!(
        fst_unchanged, fst_out,
        "'xa' must decode to a DIFFERENT candidate than 'xby' (distinct roots)"
    );
}

// =================================================================================================
// rtl-deletion: RHS empty ("0"), gated by an environment -- proves the `"0"` deletion literal and
// the environment swap both survive the reversal construction together.
// =================================================================================================

const RTL_DELETION_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>RtlDeletion</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <!-- One distinct symbol value per segment -- see `RTL_FEATURE_ENV_XML`'s own comment. -->
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols><Symbol id="symX">x</Symbol><Symbol id="symD">d</Symbol><Symbol id="symY">y</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations><FeatureValue feature="featId" symbolValues="symX" /></SegmentDefinition>
        <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations><FeatureValue feature="featId" symbolValues="symD" /></SegmentDefinition>
        <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations><FeatureValue feature="featId" symbolValues="symY" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncD"><Name>Deletable</Name><Segment segment="cd" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncX"><Name>X</Name><Segment segment="cx" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncY"><Name>Y</Name><Segment segment="cy" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prDelete" multipleApplicationOrder="rightToLeftIterative">
        <Name>deleteDemo</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncD" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence /></PhoneticOutput>
            <Environment>
              <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncX" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
              <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncY" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
            </Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prDelete">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryWithDeletable" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloWithDeletable"><PhoneticShape>xdy</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>withDeletable</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryNoRightContext" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloNoRightContext"><PhoneticShape>xd</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>noRightContext</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn rtl_deletion_matches_oracle() {
    let g = load(RTL_DELETION_XML);
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(rule.dir, Dir::RightToLeft);
    assert!(is_fully_supported_shape(rule));
    assert!(rule.subrules[0].rhs.nodes.is_empty(), "deletion subrule must have an empty RHS pattern");

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let entry_with = entry_id_of(&g, "entryWithDeletable");
    let entry_no_right = entry_id_of(&g, "entryNoRightContext");
    let allowed: HashSet<u32> = [
        g.entries[entry_with.0 as usize].morpheme.0,
        g.entries[entry_no_right.0 as usize].morpheme.0,
    ]
    .into_iter()
    .collect();

    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
    let entries: HashSet<LexEntryId> = [entry_with, entry_no_right].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    // "xy": entryWithDeletable's own surface after obligatory deletion of 'd'.
    let query = alphabet.encode_query("xy").expect("'xy' must segment");
    let fst_out = fst_candidate_set(&net, &query);
    let oracle_out = oracle_candidate_set(&morpher, "xy", &allowed);
    assert_eq!(oracle_out.len(), 1, "oracle must recall entryWithDeletable for 'xy': {oracle_out:?}");
    assert_eq!(fst_out, oracle_out, "CONTAINMENT for 'xy' (right-context-gated deletion)");

    // "xdy": the raw spelling must never surface (obligatory deletion).
    let oracle_raw = oracle_candidate_set(&morpher, "xdy", &allowed);
    assert!(oracle_raw.is_empty(), "'xdy' (obligatorily deleted) must have no oracle analysis");

    // "xd": entryNoRightContext's own (unchanged) spelling -- right context 'y' absent, deletion
    // must not fire.
    let query_unchanged = alphabet.encode_query("xd").expect("'xd' must segment");
    let fst_unchanged = fst_candidate_set(&net, &query_unchanged);
    let oracle_unchanged = oracle_candidate_set(&morpher, "xd", &allowed);
    assert_eq!(oracle_unchanged.len(), 1, "oracle must recall entryNoRightContext unchanged: {oracle_unchanged:?}");
    assert_eq!(fst_unchanged, oracle_unchanged, "CONTAINMENT for 'xd' (environment correctly fails to gate)");
}

// =================================================================================================
// rtl-epenthesis: empty LHS (insertion), gated by an environment -- proves the `"[..]"` epenthesis
// literal survives the reversal construction (the mirror rule's own LHS/RHS are both still empty/
// single-segment; only the environment swap is exercised structurally here, same as deletion).
// =================================================================================================

const RTL_EPENTHESIS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>RtlEpenthesis</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <!-- One distinct symbol value per segment -- see `RTL_FEATURE_ENV_XML`'s own comment. -->
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols><Symbol id="symX">x</Symbol><Symbol id="symE">e</Symbol><Symbol id="symY">y</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cx"><Representations><Representation>x</Representation></Representations><FeatureValue feature="featId" symbolValues="symX" /></SegmentDefinition>
        <SegmentDefinition id="ce"><Representations><Representation>e</Representation></Representations><FeatureValue feature="featId" symbolValues="symE" /></SegmentDefinition>
        <SegmentDefinition id="cy"><Representations><Representation>y</Representation></Representations><FeatureValue feature="featId" symbolValues="symY" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncE"><Name>Epenthetic</Name><Segment segment="ce" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncX"><Name>X</Name><Segment segment="cx" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncY"><Name>Y</Name><Segment segment="cy" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prEpenthesis" multipleApplicationOrder="rightToLeftIterative">
        <Name>epenthesisDemo</Name>
        <PhoneticInput><PhoneticSequence /></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncE" /></PhoneticSequence></PhoneticOutput>
            <Environment>
              <LeftEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncX" /></PhoneticSequence></PhoneticTemplate></LeftEnvironment>
              <RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncY" /></PhoneticSequence></PhoneticTemplate></RightEnvironment>
            </Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prEpenthesis">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryXY" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloXY"><PhoneticShape>xy</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>xy</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryXOnly" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloXOnly"><PhoneticShape>x</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>xOnly</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// **FST-only** (not a `pg_parse::Morpher` containment check -- see below): `Kind::Epenthesis`
/// (`rewrite.rs`'s own `classify`, `rule.lhs.nodes.is_empty()`) has, per `pg_rules::rewrite`'s own
/// doc (`ana_epenthesis_target_lanes`'s neighboring comment), **zero occurrences in any reference
/// grammar** -- its analysis-side unapplication (`ana_epenthesis`) is, discovered while writing
/// this test, effectively unexercised: `pg_parse::Morpher` finds NO analysis at all for this
/// fixture's surface forms, INCLUDING the trivial "xOnly" root's own unaffected spelling `"x"` --
/// and, verified directly, this reproduces byte-identically with the SAME fixture's rule declared
/// plain `LeftToRight` (no `multipleApplicationOrder` attribute at all). This is therefore a
/// separate, pre-existing, direction-INDEPENDENT gap in `pg_rules::rewrite`'s epenthesis analysis
/// path, entirely outside `replace.rs`'s single-owner boundary -- flagged for a future
/// investigation, not silently avoided (distinct from the "Known, out-of-scope oracle gap" this
/// file's top doc documents for the DIRECTION-specific pick-order dimension). This test therefore
/// verifies the reversal construction's OWN correctness directly against the compiled `Fsm`
/// (`apply_up`/`apply_down`, no oracle needed), the same oracle-independent style
/// `rtl_distinct_leftmost_rightmost_...`'s first half already uses.
#[test]
fn rtl_epenthesis_construction_is_correct_at_the_fst_level() {
    let g = load(RTL_EPENTHESIS_XML);
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(rule.dir, Dir::RightToLeft);
    assert!(rule.lhs.nodes.is_empty(), "epenthesis rule must have an empty LHS pattern");
    assert!(is_fully_supported_shape(rule));

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let entry_xy = entry_id_of(&g, "entryXY");
    let entry_x_only = entry_id_of(&g, "entryXOnly");

    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
    let entries: HashSet<LexEntryId> = [entry_xy, entry_x_only].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);

    // "xey": entryXY's own surface after obligatory insertion of 'e' between 'x' and 'y'. The FST
    // must PROPOSE it (decode to a non-empty candidate set) -- this is the recall half of the
    // propose/confirm contract, verified independent of the broken oracle above.
    let query_xey = alphabet.encode_query("xey").expect("'xey' must segment");
    let fst_xey = fst_candidate_set(&net, &query_xey);
    assert!(!fst_xey.is_empty(), "FST must propose entryXY for 'xey' (obligatory epenthesis fired)");

    // "xy": the raw, un-inserted-into spelling must never be proposed (obligatory insertion).
    let query_xy_raw = alphabet.encode_query("xy").expect("'xy' must segment");
    let fst_xy_raw = fst_candidate_set(&net, &query_xy_raw);
    assert!(fst_xy_raw.is_empty(), "FST must not propose anything for 'xy' (obligatorily inserted-into)");

    // "x": entryXOnly's own (unchanged) spelling -- right context 'y' absent, insertion must not
    // fire. Must still be proposed, and must decode to a DIFFERENT candidate than 'xey'.
    let query_x = alphabet.encode_query("x").expect("'x' must segment");
    let fst_x = fst_candidate_set(&net, &query_x);
    assert!(!fst_x.is_empty(), "FST must propose entryXOnly for 'x' unchanged (environment absent)");
    assert_ne!(fst_x, fst_xey, "'x' must decode to a DIFFERENT candidate than 'xey' (distinct roots)");
}

// =================================================================================================
// rtl-distinct-leftmost-rightmost: the spec's own discriminating scenario ("Direction changes the
// result"). See this file's own top doc, "Known, out-of-scope oracle gap", for why this witness
// proves genuine FST-level directional divergence + safe superset-containment against today's
// oracle, rather than exact oracle equality (no correct oracle answer for this dimension exists in
// this repo today).
// =================================================================================================

#[test]
fn rtl_distinct_leftmost_rightmost_differs_from_ltr_and_is_recall_safe_against_the_current_oracle() {
    let opts = FomaOptions::default();

    // The bare automaton-level proof, independent of any grammar/oracle: "aa -> b" applied to
    // "aaa" prefers the LEFTMOST match under plain foma `->` ("ba"), and the RIGHTMOST match under
    // the reversal construction alone ("ab") -- see `compile_rtl_branch_net`'s own worked-example
    // doc, which this pins byte-for-byte.
    let plain = fsm_parse_regex(&opts, "a a -> b", None, None).expect("plain compiles");
    let mut h = apply_init(&plain);
    assert_eq!(
        apply_down(&mut h, Some("aaa")),
        Some("ba".to_string()),
        "plain LeftToRight-style compile must prefer the LEFTMOST non-overlapping match"
    );

    let mirror = fsm_parse_regex(&opts, "a a -> b", None, None).expect("mirror compiles");
    let reversed = fsm_reverse(mirror);
    let mut h = apply_init(&reversed);
    assert_eq!(
        apply_down(&mut h, Some("aaa")),
        Some("ab".to_string()),
        "the reversal construction alone must prefer the RIGHTMOST non-overlapping match -- \
         PROVABLY DIFFERENT from the plain/LeftToRight-style branch above, entirely independent \
         of any oracle"
    );

    // Now the full grammar-level containment check, using `compile_rewrite_rule_subset`'s real
    // `Dir::RightToLeft` path (plain-union-reversed, module doc's safety-net rationale).
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>RtlDistinctLeftmostRightmost</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <!-- One distinct symbol value per segment -- see `RTL_FEATURE_ENV_XML`'s own comment. -->
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols><Symbol id="symA">a</Symbol><Symbol id="symB">b</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featId" symbolValues="symA" /></SegmentDefinition>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featId" symbolValues="symB" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncA"><Name>A</Name><Segment segment="ca" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prAaB" multipleApplicationOrder="rightToLeftIterative">
        <Name>aaToB</Name>
        <PhoneticInput><PhoneticSequence>
          <SimpleContext naturalClass="ncA" /><SimpleContext naturalClass="ncA" />
        </PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prAaB">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryAaa" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloAaa"><PhoneticShape>aaa</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>aaa</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = load(XML);
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(rule.dir, Dir::RightToLeft);
    assert!(is_fully_supported_shape(rule));

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let entry_aaa = entry_id_of(&g, "entryAaa");
    let allowed: HashSet<u32> = [g.entries[entry_aaa.0 as usize].morpheme.0].into_iter().collect();

    let budget = ComposeBudget::with_caps(usize::MAX, usize::MAX, usize::MAX, usize::MAX, usize::MAX, None);
    let entries: HashSet<LexEntryId> = [entry_aaa].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    let query_ba = alphabet.encode_query("ba").expect("'ba' must segment");
    let query_ab = alphabet.encode_query("ab").expect("'ab' must segment");
    let fst_ba = fst_candidate_set(&net, &query_ba);
    let fst_ab = fst_candidate_set(&net, &query_ab);
    let oracle_ba = oracle_candidate_set(&morpher, "ba", &allowed);
    let oracle_ab = oracle_candidate_set(&morpher, "ab", &allowed);

    // The oracle gap, pinned as data: `pg_rules::rewrite`'s own resynthesis of "aaa" is always
    // "ba" (leftmost-preferring), REGARDLESS of this rule's declared `RightToLeft` direction --
    // this file's top doc explains why. If a future fix makes the oracle direction-aware, EITHER
    // of these two assertions failing is the expected, welcome signal to revisit this test (not a
    // regression in `replace.rs`).
    assert_eq!(oracle_ba.len(), 1, "oracle recalls 'aaa' for 'ba' (its own leftmost-preferring resynthesis): {oracle_ba:?}");
    assert!(oracle_ab.is_empty(), "KNOWN ORACLE GAP: oracle does NOT recall 'aaa' for 'ab' today (pg_rules::rewrite is direction-blind for pick-order) -- see this file's top doc");

    // RECALL SAFETY (never under-propose relative to today's real oracle): the FST's compiled
    // relation, which the safety-net union guarantees always contains the plain/LeftToRight-style
    // branch's own pairs, must still recall "aaa" for "ba".
    assert_eq!(fst_ba, oracle_ba, "CONTAINMENT for 'ba': FST must recall exactly what today's oracle confirms");

    // GENUINE DIRECTIONALITY (not "compiled as LeftToRight"): the FST's compiled relation must
    // ALSO propose "aaa" for "ab" -- the reversed branch's own genuinely-different candidate,
    // safely over-proposed (ConfirmOnly's own "propose the superset" contract, ADR 0001) even
    // though today's oracle cannot yet confirm it (the documented gap above).
    assert!(
        !fst_ab.is_empty(),
        "the RTL-compiled relation must still PROPOSE 'aaa' for 'ab' (the genuinely-reversed \
         branch's own candidate) even though today's oracle cannot confirm it -- this is what \
         proves the construction is not simply 'compiled as LeftToRight': a plain LeftToRight-only \
         compile of this same rule would NEVER propose 'aaa' for surface 'ab'"
    );
}
