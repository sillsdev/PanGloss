//! `PhonRuleDef::Metathesis` real FST semantics via `compile_metathesis_rule`'s dedicated swap relation: oracle-exact for `Dir::LeftToRight`, a proven safe superset (`ConfirmOnly`) for `Dir::RightToLeft`. Synthetic fixtures, named by construct, checked against `pg_parse::Morpher` via the established `fst_candidate_set`/`oracle_candidate_set` methodology. Also pins two `build_analysis_pattern` invariants this file calls "gap 1" (physical position, not tag name, decides switch order) and "gap 2" (a context node between switches is kept unless it's a boundary).
//! See `docs/research/pg-foma-phase-c-metathesis-gate-notes.md` for the full scope line, the RTL direction-blindness finding, and both invariants in detail.

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
use pg_foma::replace::{compile_and_compose_rules_with_budget, SegAlphabet};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying_filtered_with_budget;
use pg_grammar::model::{Grammar, LexEntryId, PhonRuleDef};
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

/// Every DECODED `apply_up` candidate for `query` against `net` -- the FST-proposer half of the containment check (`tests/two_table_symbol_divergence.rs`'s helper, reused verbatim).
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

/// The full-HC oracle's own candidate set for `surface`, restricted to `allowed_morphemes` (`tests/two_table_symbol_divergence.rs`'s helper, reused verbatim).
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

/// Compiles `rule` (stratum 0's table) via `compile_and_compose_rules_with_budget`, composes after `lexc_source`, and minimizes -- shared plumbing every containment witness below uses.
fn compile_net(
    g: &Grammar,
    alphabet: &SegAlphabet,
    rule: &PhonRuleDef,
    lexc_source: &str,
) -> foma::types::Fsm {
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
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
    .expect("metathesis rule must now compile to Some(net)");
    assert!(skipped.is_empty(), "rule must not be skipped: {skipped:?}");
    fsm_minimize(&opts, fsm_compose(&opts, lexc_net, rule_net))
}

fn full_containment_check(
    g: &Grammar,
    entry_xml_id: &str,
    swapped_surface: &str,
    raw_surface: &str,
) {
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let entry = entry_id_of(g, entry_xml_id);
    let allowed: HashSet<u32> = [g.entries[entry.0 as usize].morpheme.0]
        .into_iter()
        .collect();

    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let entries: HashSet<LexEntryId> = [entry].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(g, usize::MAX);

    let query = alphabet
        .encode_query(swapped_surface)
        .unwrap_or_else(|| panic!("{swapped_surface:?} must segment against table 0"));
    let fst_out = fst_candidate_set(&net, &query);
    let oracle_out = oracle_candidate_set(&morpher, swapped_surface, &allowed);
    assert_eq!(
        oracle_out.len(),
        1,
        "oracle must recall {entry_xml_id} for its own obligatorily-metathesized surface \
         {swapped_surface:?}: {oracle_out:?}"
    );
    assert_eq!(
        fst_out, oracle_out,
        "CONTAINMENT: FST propose+decode must EQUAL the oracle for surface {swapped_surface:?}"
    );

    let oracle_raw = oracle_candidate_set(&morpher, raw_surface, &allowed);
    assert!(
        oracle_raw.is_empty(),
        "the raw un-swapped spelling {raw_surface:?} must have no oracle analysis: {oracle_raw:?}"
    );
}

// metathesis-adjacent-singleton: two adjacent, distinct, singleton-class switch segments, the WELL-FORMED switch-tag convention every real fixture uses. Proves EXACT oracle containment for the basic, attested shape.

const ADJACENT_SINGLETON_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisAdjacentSingleton</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <!-- One distinct symbol value PER SEGMENT - required so `pg_parse::Morpher`'s own
           analysis-side unapplication can disambiguate segments (`tests/phase_c_right_to_left.rs`'s
           own "one distinct symbol value per segment" note). -->
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols><Symbol id="symQ">q</Symbol><Symbol id="symP">p</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations><FeatureValue feature="featId" symbolValues="symQ" /></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featId" symbolValues="symP" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cQ" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cP" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrAdjacent" leftSwitch="swP" rightSwitch="swQ">
        <Name>metathesisAdjacent</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <SimpleContext id="swQ" naturalClass="ncQ" />
              <SimpleContext id="swP" naturalClass="ncP" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrAdjacent">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryQP" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloQP"><PhoneticShape>qp</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>qp</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn metathesis_adjacent_singleton_swap_matches_oracle_exactly() {
    let g = load(ADJACENT_SINGLETON_XML);
    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);
    full_containment_check(&g, "entryQP", "pq", "qp");
}

// metathesis-multi-member-precision: BOTH switch positions are multi-member classes ({q,r}/{s,t}), proving the per-branch cross-product transposes EXACTLY the matched pair, never any other combination a naive rendering would also accept.

const MULTI_MEMBER_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisMultiMemberPrecision</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols>
          <Symbol id="symQ">q</Symbol><Symbol id="symR">r</Symbol>
          <Symbol id="symS">s</Symbol><Symbol id="symT">t</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations><FeatureValue feature="featId" symbolValues="symQ" /></SegmentDefinition>
        <SegmentDefinition id="cR"><Representations><Representation>r</Representation></Representations><FeatureValue feature="featId" symbolValues="symR" /></SegmentDefinition>
        <SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations><FeatureValue feature="featId" symbolValues="symS" /></SegmentDefinition>
        <SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations><FeatureValue feature="featId" symbolValues="symT" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncSwitchA"><Name>SwitchA</Name><Segment segment="cQ" /><Segment segment="cR" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncSwitchB"><Name>SwitchB</Name><Segment segment="cS" /><Segment segment="cT" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrMultiMember" leftSwitch="swB" rightSwitch="swA">
        <Name>metathesisMultiMember</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <SimpleContext id="swA" naturalClass="ncSwitchA" />
              <SimpleContext id="swB" naturalClass="ncSwitchB" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrMultiMember">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryQS" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloQS"><PhoneticShape>qs</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>qs</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn metathesis_multi_member_classes_transpose_precisely_not_naively() {
    let g = load(MULTI_MEMBER_XML);
    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let entry_qs = entry_id_of(&g, "entryQS");
    let allowed: HashSet<u32> = [g.entries[entry_qs.0 as usize].morpheme.0]
        .into_iter()
        .collect();

    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let entries: HashSet<LexEntryId> = [entry_qs].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    // "sq": the CORRECT, obligatory swap of underlying "qs".
    let query_sq = alphabet.encode_query("sq").expect("'sq' must segment");
    let fst_sq = fst_candidate_set(&net, &query_sq);
    let oracle_sq = oracle_candidate_set(&morpher, "sq", &allowed);
    assert_eq!(
        oracle_sq.len(),
        1,
        "oracle must recall entryQS for 'sq': {oracle_sq:?}"
    );
    assert_eq!(
        fst_sq, oracle_sq,
        "CONTAINMENT for 'sq' (the one genuine swap)"
    );

    // "qs": the raw, un-swapped spelling must never surface (obligatory metathesis).
    let oracle_raw = oracle_candidate_set(&morpher, "qs", &allowed);
    assert!(
        oracle_raw.is_empty(),
        "'qs' (obligatorily swapped) must have no oracle analysis"
    );

    // THE PRECISION WITNESS: "sr"/"tq"/"tr" are every OTHER combination of ncSwitchA x ncSwitchB, but entryQS's underlying is "qs" alone, so a FAITHFUL per-branch swap relation must propose NOTHING for any of them.
    for spurious in ["sr", "tq", "tr"] {
        let query = alphabet.encode_query(spurious).expect("must segment");
        let fst_spurious = fst_candidate_set(&net, &query);
        assert!(
            fst_spurious.is_empty(),
            "PRECISION: the FST must propose NOTHING for {spurious:?} -- a naive cross-product \
             swap would incorrectly accept it as an alternate transposition of the SAME rule, \
             cross-contaminating entryQS's own 'qs'/'sq' pair with the other class members: \
             {fst_spurious:?}"
        );
    }
}

// metathesis-middle-context: the two switches are NOT adjacent -- a fixed, singleton "middle" context node sits between them, exercising gap 2 (see this file's top doc) as a FULL containment witness.

const MIDDLE_CONTEXT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisMiddleContext</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols>
          <Symbol id="symQ">q</Symbol><Symbol id="symM">m</Symbol><Symbol id="symP">p</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations><FeatureValue feature="featId" symbolValues="symQ" /></SegmentDefinition>
        <SegmentDefinition id="cM"><Representations><Representation>m</Representation></Representations><FeatureValue feature="featId" symbolValues="symM" /></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featId" symbolValues="symP" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cQ" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncM"><Name>M</Name><Segment segment="cM" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cP" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrMiddle" leftSwitch="swP" rightSwitch="swQ">
        <Name>metathesisMiddle</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <SimpleContext id="swQ" naturalClass="ncQ" />
              <SimpleContext naturalClass="ncM" />
              <SimpleContext id="swP" naturalClass="ncP" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrMiddle">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryQMP" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloQMP"><PhoneticShape>qmp</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>qmp</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn metathesis_middle_context_node_now_matches_the_oracle() {
    let g = load(MIDDLE_CONTEXT_XML);
    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);
    // FIXED (gap 2, see top doc): `build_analysis_pattern` no longer drops the middle context node, so the oracle now recalls "pmq" exactly like the FST does.
    full_containment_check(&g, "entryQMP", "pmq", "qmp");
}

// metathesis-reversed-tag-order: `leftSwitch` tags the node PHYSICALLY FIRST, `rightSwitch` the one physically last -- the OPPOSITE of the well-formed convention, exercising gap 1 (see top doc) as a full oracle-recall witness.

fn reversed_tag_recipe() -> Recipe {
    Recipe {
        name: "metathesis-reversed-tag-order",
        seed: 20260725,
        scale: ScaleKnobs::default(),
        construct: ConstructKnobs {
            table_count: 1,
            metathesis_rule_count: 1,
            ..Default::default()
        },
    }
}

#[test]
fn metathesis_grammar_gen_recipe_confirms_the_reversed_tag_round_trip() {
    let recipe = reversed_tag_recipe();
    let rendered = pg_grammar_gen::render_indexed(&recipe);
    let g = load(&rendered.xml);

    let metathesis = rendered
        .metathesis
        .as_ref()
        .expect("recipe declared metathesis_rule_count > 0");
    assert_eq!(metathesis.rule_xml_ids.len(), 1);
    assert_eq!(g.entries.len(), 1);
    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);

    // This generator's builder tags `leftSwitch` on the PHYSICALLY FIRST node -- the reversed convention this file's top doc names.
    let PhonRuleDef::Metathesis(rule) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert!(
        rule.left_switch < rule.right_switch,
        "this recipe's own reversed-tag convention"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let root_entry = &g.entries[0];
    let allowed: HashSet<u32> = [root_entry.morpheme.0].into_iter().collect();
    let underlying_text = root_entry.allomorphs[0].shape.text.clone();
    assert_eq!(underlying_text.chars().count(), 2);
    let swapped_text: String = underlying_text.chars().rev().collect();

    let shape = pg_grammar::segment::segment_phonemes_only(table, &underlying_text)
        .unwrap_or_else(|_| panic!("{underlying_text:?} must segment"));
    let synthesized = pg_rules::metathesis::synthesize(&g, rule, &shape);
    assert_eq!(synthesized.len(), 1, "the rule must apply obligatorily");
    let synthesized_text: String = synthesized[0]
        .interior()
        .map(|(_, _, cd, _)| {
            table
                .get(pg_grammar::chardef::CharDefId(cd))
                .representations()[0]
                .clone()
        })
        .collect();
    assert_eq!(
        synthesized_text, swapped_text,
        "pg_rules::metathesis::synthesize genuinely swaps (physical-position-driven, tag-name-\
         agnostic) -- NOT the vacuous no-op MetathesisRuleDef::left_switch's own doc comment would \
         predict for this reversed tag order (this file's top doc, gap 1)"
    );

    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let entries: HashSet<LexEntryId> = [LexEntryId(0)].into_iter().collect();
    let uemit = emit_underlying_filtered_with_budget(&g, &alphabet, Some(&entries), &budget)
        .unwrap_or_else(|e| panic!("lexc emission must not hit any budget: {e}"));
    assert!(uemit.skipped.is_empty());

    let net = compile_net(&g, &alphabet, &g.prules[0], &uemit.lexc_source);
    let morpher = Morpher::new(&g, usize::MAX);

    // RECALL (FST-only): the compiled relation must still PROPOSE the genuinely-synthesized swapped surface, regardless of what the oracle's analysis side can confirm.
    let query = alphabet
        .encode_query(&swapped_text)
        .unwrap_or_else(|| panic!("{swapped_text:?} must segment"));
    let fst_out = fst_candidate_set(&net, &query);
    assert!(
        !fst_out.is_empty(),
        "FST must propose the root for its own genuinely-synthesized surface {swapped_text:?}"
    );

    // FIXED (gap 1, see top doc): `build_analysis_pattern` now orders by physical position, so the oracle recalls the swapped spelling exactly; the raw un-swapped spelling still has no oracle analysis.
    let oracle_raw = oracle_candidate_set(&morpher, &underlying_text, &allowed);
    let oracle_swapped = oracle_candidate_set(&morpher, &swapped_text, &allowed);
    assert!(
        oracle_raw.is_empty(),
        "the raw un-swapped spelling {underlying_text:?} must have no oracle analysis \
         (obligatory metathesis): {oracle_raw:?}"
    );
    assert_eq!(
        oracle_swapped.len(),
        1,
        "oracle must recall the root for its own genuinely-synthesized surface {swapped_text:?}: \
         {oracle_swapped:?}"
    );
    assert_eq!(
        fst_out, oracle_swapped,
        "CONTAINMENT: FST propose+decode must EQUAL the oracle for surface {swapped_text:?}"
    );
}

// metathesis-right-to-left: `Dir::RightToLeft` compiles via the SAME mirror-and-reverse construction RTL rewrite rules use; see this file's top doc for the direction-blindness finding.

const RIGHT_TO_LEFT_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisRightToLeft</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols><Symbol id="symQ">q</Symbol><Symbol id="symP">p</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations><FeatureValue feature="featId" symbolValues="symQ" /></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featId" symbolValues="symP" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cQ" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cP" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrRtl" leftSwitch="swP" rightSwitch="swQ" multipleApplicationOrder="rightToLeftIterative">
        <Name>metathesisRtl</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <SimpleContext id="swQ" naturalClass="ncQ" />
              <SimpleContext id="swP" naturalClass="ncP" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrRtl">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryQP" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloQP"><PhoneticShape>qp</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>qp</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

/// The load-bearing containment witness: every analysis `pg_parse::Morpher` finds for `entryQP`'s words is a member of the FST proposer candidate set. `RIGHT_TO_LEFT_XML` has exactly ONE valid switch window, so this word alone cannot distinguish `Dir::RightToLeft` from `Dir::LeftToRight` -- see this file's top doc, "Empirical finding", for why that's deliberate.
#[test]
fn metathesis_right_to_left_reversal_matches_oracle_exactly() {
    let g = load(RIGHT_TO_LEFT_XML);
    let PhonRuleDef::Metathesis(rule) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert_eq!(rule.dir, pg_grammar::model::Dir::RightToLeft);

    let metathesis_rules = g
        .prules
        .iter()
        .filter(|p| matches!(p, PhonRuleDef::Metathesis(_)))
        .count();
    assert_eq!(metathesis_rules, 1);

    // Same structural shape as `ADJACENT_SINGLETON_XML` above: underlying "qp" obligatorily metathesizes to "pq".
    full_containment_check(&g, "entryQP", "pq", "qp");
}

/// The complementary, oracle-free witness that the CONSTRUCTION itself is genuinely direction-aware: needed because the containment witness above deliberately uses a NO-OVERLAP grammar and so cannot, by itself, rule out silently degenerating to "compiled as if `Dir::LeftToRight`".
/// See `docs/research/pg-foma-phase-c-metathesis-gate-notes.md` for the bare-automaton proof and why it doesn't extend to a full grammar-level compile.
#[test]
fn metathesis_right_to_left_differs_from_compiling_as_left_to_right() {
    let opts = FomaOptions::default();

    let plain = fsm_parse_regex(&opts, "a b a b -> b b a a", None, None).expect("plain compiles");
    let mut h = apply_init(&plain);
    assert_eq!(
        apply_down(&mut h, Some("ababab")),
        Some("bbaaab".to_string()),
        "plain LeftToRight-style compile must prefer the LEFTMOST non-overlapping match"
    );

    let mirror = fsm_parse_regex(&opts, "b a b a -> a a b b", None, None).expect("mirror compiles");
    let reversed = fsm_reverse(mirror);
    let mut h2 = apply_init(&reversed);
    assert_eq!(
        apply_down(&mut h2, Some("ababab")),
        Some("abbbaa".to_string()),
        "the reversal construction alone must prefer the RIGHTMOST non-overlapping match -- \
         PROVABLY DIFFERENT from the plain/LeftToRight-style branch above, entirely independent of \
         any oracle"
    );

    // Deliberately NOT extended to a full grammar-level compile comparison: `apply_down`'s exploration order over the larger unioned automaton doesn't reliably favor this branch, which is an `fsm_union` state-numbering artifact, not evidence against direction-awareness.
    // See `docs/research/pg-foma-phase-c-metathesis-gate-notes.md` for the full account of why this was tried and removed.
}

/// Behavioral confirmation, end-to-end, of the switch-index remap `metathesis_mirror_switch_index_remap_tests` already pins arithmetically in-crate, over an ASYMMETRIC 5-node pattern whose entire compiled relation is exactly one literal mapping, so any remap error fails loudly either way.
#[test]
fn metathesis_right_to_left_switch_index_remap_matches_the_derived_formula() {
    // Pattern: [a(swA=leftSwitch), b(swB=rightSwitch), c, d, e] -- synthesis swaps positions 0,1: "a b c d e" -> "b a c d e", and with only ONE possible assignment, the RTL mirror-then-reverse branch must derive the SAME relation.
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisRtlRemapPin</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols>
          <Symbol id="symA">a</Symbol><Symbol id="symB">b</Symbol><Symbol id="symC">c</Symbol>
          <Symbol id="symD">d</Symbol><Symbol id="symE">e</Symbol>
        </Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations><FeatureValue feature="featId" symbolValues="symA" /></SegmentDefinition>
        <SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations><FeatureValue feature="featId" symbolValues="symB" /></SegmentDefinition>
        <SegmentDefinition id="cC"><Representations><Representation>c</Representation></Representations><FeatureValue feature="featId" symbolValues="symC" /></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations><FeatureValue feature="featId" symbolValues="symD" /></SegmentDefinition>
        <SegmentDefinition id="cE"><Representations><Representation>e</Representation></Representations><FeatureValue feature="featId" symbolValues="symE" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrRemap" leftSwitch="swA" rightSwitch="swB" multipleApplicationOrder="rightToLeftIterative">
        <Name>metaRemap</Name>
        <StructuralDescription>
          <PhoneticTemplate>
            <PhoneticSequence>
              <Segment id="swA" segment="cA" />
              <Segment id="swB" segment="cB" />
              <Segment segment="cC" />
              <Segment segment="cD" />
              <Segment segment="cE" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrRemap">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryDummy" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloDummy"><PhoneticShape>abcde</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>dummy</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = load(XML);
    let PhonRuleDef::Metathesis(rule) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert_eq!(rule.dir, pg_grammar::model::Dir::RightToLeft);
    assert_eq!((rule.left_switch, rule.right_switch), (0, 1));

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let net = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet,
        &[&g.prules[0]],
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("compile must not hit any budget: {e}"))
    .expect("RTL metathesis rule must compile to Some(net)");
    assert!(skipped.is_empty());

    // Single-shot AND full enumeration must both agree on exactly "bacde": with only one possible assignment, a wrong remap would swap some OTHER pair of the 5 positions.
    let query = alphabet
        .encode_query("abcde")
        .expect("'abcde' must segment");
    let mut h = apply_init(&net);
    let single = apply_down(&mut h, Some(&query));
    let expected = alphabet
        .encode_query("bacde")
        .expect("'bacde' must segment");
    assert_eq!(
        single,
        Some(expected.clone()),
        "the Dir::RightToLeft compile must swap positions 0,1 exactly (leaving 2,3,4 untouched) -- \
         an off-by-one remap would transpose a DIFFERENT pair and so miss this exact value"
    );
    let mut h2 = apply_init(&net);
    let all: Vec<String> = h2.down(&query).collect();
    assert!(
        all.iter().all(|s| *s == expected),
        "every path in the full relation must agree (no alternate-pair transposition sneaking in \
         from a wrong remap): {all:?}"
    );
}

// metathesis-anchor: a `finalBoundaryCondition="true"` pattern compiles as a ConfirmOnly swap superset -- the edge anchor is stripped rather than enforced in the net.

const ANCHOR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MetathesisAnchor</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <PhonologicalFeatureSystem>
      <SymbolicFeature id="featId">
        <Name>id</Name>
        <Symbols><Symbol id="symQ">q</Symbol><Symbol id="symP">p</Symbol></Symbols>
      </SymbolicFeature>
    </PhonologicalFeatureSystem>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cQ"><Representations><Representation>q</Representation></Representations><FeatureValue feature="featId" symbolValues="symQ" /></SegmentDefinition>
        <SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="featId" symbolValues="symP" /></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <SegmentNaturalClass id="ncQ"><Name>Q</Name><Segment segment="cQ" /></SegmentNaturalClass>
      <SegmentNaturalClass id="ncP"><Name>P</Name><Segment segment="cP" /></SegmentNaturalClass>
    </NaturalClasses>
    <PhonologicalRuleDefinitions>
      <MetathesisRule id="mrAnchor" leftSwitch="swP" rightSwitch="swQ">
        <Name>metathesisAnchor</Name>
        <StructuralDescription>
          <PhoneticTemplate finalBoundaryCondition="true">
            <PhoneticSequence>
              <SimpleContext id="swQ" naturalClass="ncQ" />
              <SimpleContext id="swP" naturalClass="ncP" />
            </PhoneticSequence>
          </PhoneticTemplate>
        </StructuralDescription>
      </MetathesisRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="mrAnchor">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryQP" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloQP"><PhoneticShape>qp</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>qp</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn metathesis_anchor_pattern_compiles_as_confirm_only_swap_superset() {
    let g = load(ANCHOR_XML);
    let PhonRuleDef::Metathesis(rule) = &g.prules[0] else {
        panic!("expected a Metathesis-kind rule");
    };
    assert!(
        matches!(
            rule.pattern.nodes.last(),
            Some(pg_grammar::model::PatternNode::Anchor(_))
        ),
        "finalBoundaryCondition must lower to a trailing PatternNode::Anchor"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let composed = compile_and_compose_rules_with_budget(
        &opts,
        &g,
        &alphabet,
        &[&g.prules[0]],
        &mut skipped,
        &mut tuple_reports,
        &budget,
    )
    .unwrap_or_else(|e| panic!("compile must not hit any budget: {e}"));

    let net = composed.expect("a final-anchor metathesis pattern must compile");
    assert!(
        skipped.is_empty(),
        "supported anchor must not be skipped: {skipped:?}"
    );
    assert!(tuple_reports.is_empty());

    let query = alphabet
        .encode_query("qp")
        .expect("underlying must segment");
    let expected = alphabet.encode_query("pq").expect("surface must segment");
    let mut h = apply_init(&net);
    let outputs = h.down(&query).collect::<Vec<_>>();
    assert!(
        outputs.iter().any(|s| s == &expected),
        "the swap must apply at the final word boundary: {outputs:?}"
    );
    // Edge anchors are erased only in the proposer; complete confirmation applies the exact boundary condition, so this is a safe ConfirmOnly over-approximation.
}
