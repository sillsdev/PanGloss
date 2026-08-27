//! `RewriteMode::Simultaneous` compiles via `replace.rs`'s ordinary sequential-compose machinery, unchanged, for any rule the overlap predicate proves pairwise non-overlapping; one it cannot clear stays honestly gated.
//! See `docs/research/pg-foma-simultaneous-rewrite-notes.md` for the fixture design and why no oracle mode-blindness workaround is needed here.

mod common;

use std::collections::HashSet;

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;

use pg_foma::replace::{
    compile_and_compose_rules, is_fully_supported_shape, SegAlphabet,
};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered;
use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef, RewriteMode};
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

/// Every decoded `apply_up` candidate for `query` against `net`.
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

/// The full-HC oracle's own candidate set for `surface`, restricted to `allowed_morphemes`.
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

/// Compiles `rule` via `compile_and_compose_rules`, composes it after `lexc_source`, and minimizes.
fn compile_net(
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &PhonRuleDef,
    lexc_source: &str,
) -> foma::types::Fsm {
    let opts = FomaOptions::default();
    let lexc_net = fsm_lexc_parse_string(&opts, None, lexc_source)
        .unwrap_or_else(|| panic!("lexc must compile:\n{lexc_source}"));
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules(
        &opts,
        g,
        alphabet,
        &[rule],
        &mut skipped,
        &mut tuple_reports,
    )
    .expect("admitted Simultaneous rule must now compile to Some(net)");
    assert!(skipped.is_empty(), "rule must not be skipped: {skipped:?}");
    fsm_minimize(&opts, fsm_compose(&opts, lexc_net, rule_net))
}

// sim-trivial: single, ungated, environment-free subrule -- vacuously admitted, now proven to actually compile end-to-end.

fn trivial_recipe() -> Recipe {
    Recipe {
        name: "phase-c-simultaneous",
        seed: 20260720,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            simultaneous_rule_count: 1,
            ..Default::default()
        },
    }
}

fn rules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    g.strata[0]
        .prules
        .iter()
        .map(|&id| &g.prules[id.0 as usize])
        .collect()
}

#[test]
fn sim_trivial_lone_subrule_now_compiles() {
    let recipe = trivial_recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = pg_grammar::load(&rendered.xml).unwrap_or_else(|e| {
        panic!(
            "generated simultaneous XML failed to load: {e}\n{}",
            rendered.xml
        )
    });

    let simultaneous = rendered
        .simultaneous
        .as_ref()
        .expect("recipe declared simultaneous_rule_count > 0");
    assert_eq!(simultaneous.rule_xml_ids.len(), 1);
    assert_eq!(g.prules.len(), 1);
    assert_eq!(g.entries.len(), 1);

    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule at prules[0]");
    };
    assert_eq!(
        rule.mode,
        RewriteMode::Simultaneous,
        "recipe's own multipleApplicationOrder=\"simultaneous\" must round-trip to RewriteMode::Simultaneous"
    );
    assert_eq!(
        rule.subrules.len(),
        1,
        "the bail-gate recipe's own rule must have exactly one, ungated, environment-free subrule"
    );
    assert!(
        !rule.subrules[0].self_opaquing,
        "an ungated, environment-free subrule must not be self_opaquing"
    );
    assert!(
        is_fully_supported_shape(&g, rule),
        "a Simultaneous rule with a single non-self-opaquing subrule has no PEER for D3's own \
         pairwise overlap check to ever examine, so it must now be reported fully-supported"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);

    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let composed = compile_and_compose_rules(
        &opts,
        &g,
        &alphabet,
        &ro,
        &mut skipped,
        &mut tuple_reports,
    )

    assert!(
        skipped.is_empty(),
        "the admitted Simultaneous rule must NOT be reported skipped any more: {skipped:?}"
    );
    assert!(
        composed.is_some(),
        "the admitted Simultaneous rule must now compile into a real composed network"
    );
    assert_eq!(
        tuple_reports.len(),
        1,
        "exactly one compiled rule contributes exactly one (empty, alpha-free) tuple report"
    );
}

// sim-nonoverlap-env: two subrules with mutually exclusive right environments (Front/Back) -- proven non-overlapping via lowered-span intersection; full grammar + lexicon + oracle containment check.

const SIM_NONOVERLAP_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SimNonoverlapEnv</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <!-- Two ORTHOGONAL features, deliberately: `featVoice` (LHS/RHS, a distinct symbol per
         segment so analysis' own "did this feature genuinely change" check is nonvacuous) and
         `featPlace` (the environment classes only) - EVERY segment gets an EXPLICIT value for
         BOTH features (never left unconstrained/full-mask), because this codebase's natural-class
         membership test is bitwise-overlap, not strict equality (this crate's own `AlphaVar` doc):
         an UNCONSTRAINED (full-mask) lane on 'p'/'b'/'d' would overlap-match BOTH `ncFront` AND
         `ncBack`'s single-bit criteria, making the two environments spuriously "genuinely overlap"
         (found empirically while building this fixture - an earlier draft that left `featPlace`
         unset on the consonants failed `is_fully_supported_shape` for exactly this reason, a real,
         non-cosmetic modeling requirement, not a compiler bug). `symNeutral` gives the consonants a
         concrete, non-Front/non-Back `featPlace` value so they never satisfy either environment
         class. Neither RHS pin (`ncB`/`ncD`, `featVoice` only) ever CONSTRAINS `featPlace`, so it
         still unifies trivially with either environment class (no shared feature dimension to
         contradict on) - keeping `self_opaquing` FALSE (checked below) while giving analysis a
         real, non-vacuous feature change to unapply. A single dummy feature shared identically by
         every segment (tried first) keeps `self_opaquing` safe but makes the change feature-
         vacuous (LHS/RHS pins identical), which analysis correctly refuses to unapply as "no
         actual change" - this two-feature, fully-specified design is the fix, not a cosmetic
         choice. -->
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featVoice"><Name>voice</Name><Symbols>
        <Symbol id="symVless">vless</Symbol><Symbol id="symVd1">vd1</Symbol><Symbol id="symVd2">vd2</Symbol>
      </Symbols></SymbolicFeature>
      <SymbolicFeature id="featPlace"><Name>place</Name><Symbols>
        <Symbol id="symFront">front</Symbol><Symbol id="symBack">back</Symbol><Symbol id="symNeutral">neutral</Symbol>
      </Symbols></SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVless" /><FeatureValue feature="featPlace" symbolValues="symNeutral" /></SegmentDefinition>
        <SegmentDefinition id="ci"><Representations><Representation>i</Representation></Representations><FeatureValue feature="featPlace" symbolValues="symFront" /><FeatureValue feature="featVoice" symbolValues="symVless" /></SegmentDefinition>
        <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations><FeatureValue feature="featPlace" symbolValues="symBack" /><FeatureValue feature="featVoice" symbolValues="symVless" /></SegmentDefinition>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVd1" /><FeatureValue feature="featPlace" symbolValues="symNeutral" /></SegmentDefinition>
        <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVd2" /><FeatureValue feature="featPlace" symbolValues="symNeutral" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncStop"><Name>Stop</Name><FeatureValue feature="featVoice" symbolValues="symVless" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncFront"><Name>Front</Name><FeatureValue feature="featPlace" symbolValues="symFront" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncBack"><Name>Back</Name><FeatureValue feature="featPlace" symbolValues="symBack" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncB"><Name>B</Name><FeatureValue feature="featVoice" symbolValues="symVd1" /></FeatureNaturalClass>
      <FeatureNaturalClass id="ncD"><Name>D</Name><FeatureValue feature="featVoice" symbolValues="symVd2" /></FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prSimNonoverlap" multipleApplicationOrder="simultaneous">
        <Name>simNonoverlapDemo</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncB" /></PhoneticSequence></PhoneticOutput>
            <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncFront" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
          </PhonologicalSubrule>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncD" /></PhoneticSequence></PhoneticOutput>
            <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prSimNonoverlap">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryPI" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloPI"><PhoneticShape>pi</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pi</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryPU" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloPU"><PhoneticShape>pu</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pu</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn sim_nonoverlap_env_now_compiles_and_matches_oracle_exactly() {
    let g = load(SIM_NONOVERLAP_XML);
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(rule.mode, RewriteMode::Simultaneous);
    assert_eq!(rule.subrules.len(), 2);
    assert!(
        !rule.subrules[0].self_opaquing && !rule.subrules[1].self_opaquing,
        "no PhonologicalFeatureSystem is declared, so self_opaquing must be vacuously false for \
         both subrules"
    );
    assert!(
        is_fully_supported_shape(&g, rule),
        "Front/Back-flanked, non-overlapping subrules must now be reported fully-supported \
         (the real lowered-span intersection proves them disjoint)"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let entry_pi = entry_id_of(&g, "entryPI");
    let entry_pu = entry_id_of(&g, "entryPU");
    let allowed: HashSet<u32> = [
        g.entries[entry_pi.0 as usize].morpheme.0,
        g.entries[entry_pu.0 as usize].morpheme.0,
    ]
    .into_iter()
    .collect();

    let entries: HashSet<LexEntryId> = [entry_pi, entry_pu].into_iter().collect();
    let uemit = emit_underlying_filtered(&g, &alphabet, Some(&entries))
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    // "bi": entryPI's own surface after obligatory subrule-1 rewrite ('p' before Front 'i' -> 'b').
    let query_bi = alphabet.encode_query("bi").expect("'bi' must segment");
    let fst_bi = fst_candidate_set(&net, &query_bi);
    let oracle_bi = oracle_candidate_set(&morpher, "bi", &allowed);
    assert_eq!(
        oracle_bi.len(),
        1,
        "oracle must recall entryPI for 'bi': {oracle_bi:?}"
    );
    assert_eq!(
        fst_bi, oracle_bi,
        "CONTAINMENT for 'bi' (subrule 1, Front environment)"
    );

    // "du": entryPU's own surface after obligatory subrule-2 rewrite ('p' before Back 'u' -> 'd').
    let query_du = alphabet.encode_query("du").expect("'du' must segment");
    let fst_du = fst_candidate_set(&net, &query_du);
    let oracle_du = oracle_candidate_set(&morpher, "du", &allowed);
    assert_eq!(
        oracle_du.len(),
        1,
        "oracle must recall entryPU for 'du': {oracle_du:?}"
    );
    assert_eq!(
        fst_du, oracle_du,
        "CONTAINMENT for 'du' (subrule 2, Back environment)"
    );
    assert_ne!(
        fst_bi, fst_du,
        "'bi' and 'du' must decode to DISTINCT roots"
    );

    // Raw, un-rewritten spellings must never surface: both subrules are obligatory wherever their environment holds.
    let oracle_pi_raw = oracle_candidate_set(&morpher, "pi", &allowed);
    assert!(
        oracle_pi_raw.is_empty(),
        "'pi' (obligatorily rewritten) must have no oracle analysis"
    );
    let oracle_pu_raw = oracle_candidate_set(&morpher, "pu", &allowed);
    assert!(
        oracle_pu_raw.is_empty(),
        "'pu' (obligatorily rewritten) must have no oracle analysis"
    );
}

// sim-overlap-env: two subrules whose right environments genuinely overlap (shared member) -- must stay honest-unsupported (`None`, `skipped`), never a wrong compile.

const SIM_OVERLAP_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>SimOverlapEnv</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="ci"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cu"><Representations><Representation>u</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cd"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncStop"><Name>Stop</Name><Segment segment="cp" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncFrontOrBack"><Name>FrontOrBack</Name><Segment segment="ci" /><Segment segment="cu" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncBack"><Name>Back</Name><Segment segment="cu" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prSimOverlap" multipleApplicationOrder="simultaneous">
        <Name>simOverlapDemo</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncStop" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="cb" /></PhoneticSequence></PhoneticOutput>
            <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncFrontOrBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
          </PhonologicalSubrule>
          <PhonologicalSubrule>
            <PhoneticOutput><PhoneticSequence><Segment segment="cd" /></PhoneticSequence></PhoneticOutput>
            <Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncBack" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prSimOverlap">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryPU" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloPU"><PhoneticShape>pu</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pu</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn sim_overlap_env_stays_honest_unsupported() {
    let g = load(SIM_OVERLAP_XML);
    let PhonRuleDef::Rewrite(rule) = &g.prules[0] else {
        panic!("expected a Rewrite-kind rule");
    };
    assert_eq!(rule.mode, RewriteMode::Simultaneous);
    assert_eq!(rule.subrules.len(), 2);
    assert!(!rule.subrules[0].self_opaquing && !rule.subrules[1].self_opaquing);
    assert!(
        !is_fully_supported_shape(&g, rule),
        "subrules whose right environments genuinely overlap (shared 'u' member) must NOT be \
         reported fully-supported -- D3 cannot rule out contention at that shared focus position"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro: Vec<&PhonRuleDef> = g.strata[0]
        .prules
        .iter()
        .map(|&id| &g.prules[id.0 as usize])
        .collect();

    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let composed = compile_and_compose_rules(
        &opts,
        &g,
        &alphabet,
        &ro,
        &mut skipped,
        &mut tuple_reports,
    )

    assert_eq!(
        skipped,
        vec!["prSimOverlap".to_string()],
        "the overlapping-subrule rule must be the ONLY skipped rule, and must be reported (never \
         silently mis-compiled)"
    );
    assert!(
        composed.is_none(),
        "zero compilable rules -- the cascade must be a no-op, never a wrong network"
    );
    assert!(
        tuple_reports.is_empty(),
        "a skipped rule contributes no alpha-tuple report"
    );
}
