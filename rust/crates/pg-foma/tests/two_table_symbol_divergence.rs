//! `openspec/changes/fix-multitable-fst-compilation` task 3: PROPOSER-TO-CONFIRM CONTAINMENT for
//! the multi-table construct -- named by construct ("two-table-symbol-divergence"), synthetic and
//! delanguaged, no language nouns anywhere (per `openspec/changes/STAGING.md`'s "Hard rule:
//! synthetic data only"). Hand-authored XML (mirroring `pg_foma::replace`'s/`pg_foma::capability`'s
//! own test-module convention) rather than `pg_grammar_gen`'s recipe generator: the generator's
//! own `build::tables` module always adds a per-segment-unique `featId` feature (needed for ITS
//! OWN unrelated purpose, avoiding `generate_words` surface collisions -- that module's own doc),
//! which turns out to defeat `pg_parse::Morpher`'s un-apply of an environment-free feature-changing
//! rewrite (a real, PRE-EXISTING, separate characteristic of `pg-rules`' analysis engine unrelated
//! to this change -- see this file's own "Known, out-of-scope anomaly" note below). This fixture
//! sidesteps that by declaring only ONE phonological feature (`featVoice`) per table.
//!
//! ## What this proves, beyond `tests/phase_c_multi_table.rs`
//! `phase_c_multi_table.rs` (GATE 1, now inverted) proves recall-via-compose for ONE stratum's own
//! rule. This file proves the STRONGER claim design.md's own scenario asks for: "two strata, same
//! symbol differs between tables... each compiled rule uses its own table AND proposer-to-confirm
//! results match the oracle" -- using this codebase's own established containment methodology
//! (`tests/f2_junction_gate.rs`'s `engine_sequences`/`candidates_cover`, `tests/f3_parity.rs`'s
//! "multiset parity" framing, `tests/p6_templated_morphotactics_gate.rs` test (c)'s `apply_up` ->
//! `tags::decode_path` -> `tags::to_candidates` decode): decode every raw `apply_up` result off the
//! P6-compiled net into [`pg_foma::tags::Candidate`]s, and assert that set is EXACTLY EQUAL (not
//! just a superset or subset) to `pg_parse::Morpher`'s own oracle analysis set for the same surface
//! word -- `pg_rules::rewrite` (Morpher's own rewrite engine) already resolves every rule against
//! its real owning stratum's table via an explicit `TableId` parameter at every call site (verified
//! by inspection), so it is a trustworthy oracle for exactly the bug this change fixes: the
//! proposer (`pg_foma::replace`) used to be the ONLY table-zero-biased link in this chain.
//!
//! Since this fixture is deliberately tiny (2 entries per stratum, 1 rule, no MPR/POS gating, no
//! compounding), the decoded FST candidate set is already sound with no possible false positive a
//! separate `pg_foma::confirm` pass would need to prune -- direct set equality against the oracle
//! is the faithful, minimal realization of "propose, then confirm, must equal the oracle" for a
//! grammar this small.
//!
//! ## Scope: stratum 1 only (matches GATE 1's own established scope)
//! Only stratum 1 (the LAST stratum, table 1) is checked against the oracle. A bare, unaffixed root
//! declared on a NON-final stratum (stratum 0 here) with no morphological rule bridging it forward
//! is never a complete surface word by itself in this architecture (`pg_grammar_gen::build::strata`
//! 's own module doc: extra strata need an OBLIGATORY rule specifically to let a root reach the
//! surface) -- `tests/phase_c_multi_table.rs`/GATE 1 never queries stratum 0 via the oracle either,
//! for the same reason. Table 0 exists here purely so the fixture genuinely has TWO strata each
//! owning their OWN table (design.md's scenario), not one orphaned second table.
//!
//! ## Known, out-of-scope anomaly (documented, not hidden)
//! `pg_parse::Morpher`'s root lookup, when run over the UNFILTERED whole grammar, returns a THIRD,
//! spurious analysis for surface "k" naming stratum 0's own root ("p") -- table 0's and table 1's
//! segments happen to share the same RAW per-table index (0), and `pg-parse`'s own root-allomorph
//! trie appears not to disambiguate cross-stratum/cross-table `CharDefId` identity the way
//! `pg_foma::replace::owning_table` now does for rewrite-rule compilation. This is a DIFFERENT
//! component (`pg-parse`'s root trie, not `pg_foma::replace`'s rewrite compiler or `pg-rules`'
//! rewrite engine) and a DIFFERENT bug class, entirely out of scope for `replace.rs`'s single-owner
//! boundary this change holds to -- flagged here for a future investigation, not silently avoided.
//! This file's own oracle comparison is restricted to stratum 1's own two morphemes (the exact
//! candidate universe the compiled net below actually contains) so this unrelated anomaly cannot
//! contaminate the containment assertion this change is responsible for.

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

/// Two tables, deliberately MISALIGNED (table 0: index 0 = voice+, index 1 = voice-; table 1:
/// index 0 = voice-, index 1 = voice+ -- the SAME mechanism `tests/phase_c_multi_table.rs`/
/// `pg_grammar_gen::build::tables` use), two strata each owning one table, ONE obligatory
/// environment-free devoice rewrite on stratum 1 only (`ncVoicedAny -> ncVoicelessAny`).
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

/// Every DECODED `apply_up` candidate for `query` against `net`, as `(root_index, morpheme ids)`
/// pairs -- the FST-proposer half of the containment check (module doc). `net` is small by
/// construction, so an unbounded raw `apply_up` enumeration is safe.
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

/// The full-HC oracle's own candidate set for `surface`, in the SAME `(root_index, morpheme ids)`
/// shape, restricted to `allowed_morphemes` (module doc: the exact candidate universe the compiled
/// net actually contains -- sidesteps the documented, out-of-scope cross-table root-lookup anomaly
/// this file's own top doc names).
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
    assert_eq!(g.char_tables.len(), 2, "fixture must declare exactly 2 tables");
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
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );

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

    // --- Surface "k": both the voice- root (its own unchanged spelling) AND the voice+ root
    // (devoiced) share this ONE surface form. Proposer-decode set must EQUAL the oracle set. ---
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

    // --- Surface "g": the voice+ root's raw (undevoiced) spelling must never be a valid surface
    // form at all -- the devoice rule is obligatory. Neither the oracle nor the FST proposes
    // anything for it. ---
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
