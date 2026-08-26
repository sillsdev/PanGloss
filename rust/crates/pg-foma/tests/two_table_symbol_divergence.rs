//! Proposer-to-confirm containment for the multi-table construct where the same symbol diverges between tables --
//! see docs/research/pg-foma-replace-design-notes.md for the methodology, scope, and a known out-of-scope anomaly.

use std::collections::HashSet;

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::replace::{compile_and_compose_rules_with_budget, SegAlphabet};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered_with_budget;
use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions};

/// Two tables, deliberately misaligned (table 0: index 0 = voice+, index 1 = voice-; table 1: index 0 = voice-, index 1 = voice+); one obligatory devoice rewrite on stratum 1 only.
const TWO_TABLE_SYMBOL_DIVERGENCE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>TwoTableSymbolDivergence</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
    </PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featVoice">
        <Name>voice</Name>
        <Symbols><Symbol id="symVoicePlus">+</Symbol><Symbol id="symVoiceMinus">-</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t0">
      <Name>Table0</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c0a"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVoicePlus" /></SegmentDefinition>
        <SegmentDefinition id="c0b"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVoiceMinus" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <CharacterDefinitionTable id="t1">
      <Name>Table1</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="c1a"><Representations><Representation>k</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVoiceMinus" /></SegmentDefinition>
        <SegmentDefinition id="c1b"><Representations><Representation>g</Representation></Representations><FeatureValue feature="featVoice" symbolValues="symVoicePlus" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncVoicedAny"><Name>VoicedAny</Name>
        <FeatureValue feature="featVoice" symbolValues="symVoicePlus" />
      </FeatureNaturalClass>
      <FeatureNaturalClass id="ncVoicelessAny"><Name>VoicelessAny</Name>
        <FeatureValue feature="featVoice" symbolValues="symVoiceMinus" />
      </FeatureNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prDevoice1">
        <Name>devoiceDemo</Name>
        <PhoneticInput><PhoneticSequence><SimpleContext naturalClass="ncVoicedAny" /></PhoneticSequence></PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule id="subDevoice1">
            <PhoneticOutput><PhoneticSequence><SimpleContext naturalClass="ncVoicelessAny" /></PhoneticSequence></PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t0" morphologicalRuleOrder="unordered">
        <Name>S0</Name>
        <LexicalEntries>
          <LexicalEntry id="entryP" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloP"><PhoneticShape>p</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>p</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryB" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloB"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>b</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prDevoice1">
        <Name>S1</Name>
        <LexicalEntries>
          <LexicalEntry id="entryK" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>k</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryG" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloG"><PhoneticShape>g</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>g</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

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

/// The FST-proposer half of the containment check: every decoded `apply_up` candidate for `query` against `net`, as `(root_index, morpheme ids)` pairs.
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

/// The full-HC oracle's candidate set for `surface`, in the same `(root_index, morpheme ids)` shape, restricted to `allowed_morphemes` to sidestep the documented cross-table root-lookup anomaly.
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

#[test]
fn stratum_1_devoice_rewrite_proposer_confirm_matches_oracle() {
    let g = load(TWO_TABLE_SYMBOL_DIVERGENCE_XML);
    assert_eq!(
        g.char_tables.len(),
        2,
        "fixture must declare exactly 2 tables"
    );
    assert_eq!(g.char_tables[0].len(), 2);
    assert_eq!(g.char_tables[1].len(), 2);
    assert_eq!(g.strata.len(), 2, "fixture must declare exactly 2 strata");
    assert!(
        g.strata[0].prules.is_empty(),
        "stratum 0 must carry no phonological rule"
    );
    assert_eq!(
        g.strata[1].prules.len(),
        1,
        "stratum 1 must own exactly the devoice rule"
    );

    let entry_k = entry_id_of(&g, "entryK"); // voice-, own spelling unchanged
    let entry_g = entry_id_of(&g, "entryG"); // voice+, must devoice to 'k's spelling
    let morpheme_k = g.entries[entry_k.0 as usize].morpheme.0;
    let morpheme_g = g.entries[entry_g.0 as usize].morpheme.0;
    let allowed_morphemes: HashSet<u32> = [morpheme_k, morpheme_g].into_iter().collect();

    let table1 = &g.char_tables[1];
    let alphabet1 = SegAlphabet::new(table1);
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(
        usize::MAX, usize::MAX);

    let mut entries = HashSet::new();
    entries.insert(entry_k);
    entries.insert(entry_g);
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet1, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("stratum-1 lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());
    let lexc_net = fsm_lexc_parse_string(&opts, None, &uemit.lexc_source)
        .unwrap_or_else(|| panic!("stratum-1 lexc must compile:\n{}", uemit.lexc_source));

    let devoice_rule = g
        .prules
        .iter()
        .find(|p| matches!(p, PhonRuleDef::Rewrite(r) if r.xml_id == "prDevoice1"))
        .expect("devoice rule must be present in g.prules");

    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rule_net = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet1,
        &[devoice_rule],
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("devoice rule compile must not hit any budget: {e}"))
    .expect("devoice rule must compile to Some(net)");
    assert!(skipped.is_empty());

    let net = fsm_minimize(&opts, fsm_compose(&opts, lexc_net, rule_net));
    let morpher = Morpher::new(&g, usize::MAX);

    // --- Surface "k": both the voice- root (unchanged) and the voice+ root (devoiced) share this one surface form. ---
    let query_k = alphabet1
        .encode_query("k")
        .expect("'k' must segment against table 1");
    let fst_k = fst_candidate_set(&net, &query_k);
    let oracle_k = oracle_candidate_set(&morpher, "k", &allowed_morphemes);
    assert_eq!(
        oracle_k.len(),
        2,
        "oracle must find both the unchanged voice- root and the devoiced voice+ root sharing \
         surface \"k\" (restricted to stratum 1's own morphemes): {oracle_k:?}"
    );
    assert_eq!(
        fst_k, oracle_k,
        "CONTAINMENT: FST propose+decode set must EQUAL the full-HC oracle set for surface \"k\" \
         -- this is the exact multi-table proposer-to-confirm equality the design doc's scenario \
         requires (prDevoice1 now resolves ncVoicedAny/ncVoicelessAny against stratum 1's OWN \
         table, never table 0's)"
    );

    // --- Surface "g": the voice+ root's raw spelling is never a valid surface form; the devoice rule is obligatory. ---
    let oracle_g = oracle_candidate_set(&morpher, "g", &allowed_morphemes);
    assert!(
        oracle_g.is_empty(),
        "the voice+ root's raw (undevoiced) spelling must have NO oracle analysis: {oracle_g:?}"
    );
    if let Some(query_g) = alphabet1.encode_query("g") {
        let fst_g = fst_candidate_set(&net, &query_g);
        assert_eq!(
            fst_g, oracle_g,
            "CONTAINMENT: FST and oracle must agree that surface \"g\" has NO valid analysis"
        );
    }
}
