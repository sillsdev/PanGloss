//! Exploratory driver for the POS-gating half (mirrors Amharic `prule1`'s exact shape: LHS = 3
//! fixed segments, RHS = 1 fixed segment, no environment, `requiredPartsOfSpeech`). Hand-authored
//! minimal grammar (Amharic's own templated morphotactics is a separate, already-costed gap this
//! prototype doesn't attempt -- see `pg-foma/src/gate.rs`'s module doc) with two lexical entries
//! sharing the identical underlying shape, differing ONLY in part of speech, so the gate is the
//! ONLY thing that can distinguish their surface realization.

use foma::apply::apply_init;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;

use pg_foma::gate::compile_gated_grammar;
use pg_foma::replace::SegAlphabet;
use pg_foma::tags;
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::PhonRuleDef;
use pg_parse::{Morpher, ParseOptions};

const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PosGateFixture</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech>
      <PartOfSpeech id="posN"><Name>N</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cY"><Representations><Representation>y</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cW"><Representations><Representation>w</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <PhonologicalRuleDefinitions>
      <PhonologicalRule id="prule1">
        <Name>merge-if-verb</Name>
        <PhoneticInput>
          <PhoneticSequence>
            <Segment segment="cX" />
            <Segment segment="cY" />
            <Segment segment="cX" />
          </PhoneticSequence>
        </PhoneticInput>
        <PhonologicalSubrules>
          <PhonologicalSubrule requiredPartsOfSpeech="posV">
            <PhoneticOutput>
              <PhoneticSequence>
                <Segment segment="cW" />
              </PhoneticSequence>
            </PhoneticOutput>
          </PhonologicalSubrule>
        </PhonologicalSubrules>
      </PhonologicalRule>
    </PhonologicalRuleDefinitions>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" phonologicalRules="prule1">
        <Name>S</Name>
        <LexicalEntries>
          <LexicalEntry id="entryV" partOfSpeech="posV">
            <Allomorphs><Allomorph id="alloV"><PhoneticShape>xyx</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>verb-root</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entryN" partOfSpeech="posN">
            <Allomorphs><Allomorph id="alloN"><PhoneticShape>xyx</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>noun-root</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

fn main() {
    let g = pg_grammar::load(XML).unwrap_or_else(|e| panic!("failed to load POS fixture: {e}\n{XML}"));

    println!("=== POS-gating fixture: oracle exploration ===");
    let morpher = Morpher::new(&g, usize::MAX);
    let popts = ParseOptions::default();
    for word in ["xyx", "w"] {
        let outcome = morpher.parse_word_opts(word, &popts);
        println!("parse_word({word:?}):");
        if outcome.structured.is_empty() {
            println!("    (no analyses)");
        }
        for a in &outcome.structured {
            let names: Vec<String> = a
                .morpheme_ids
                .iter()
                .map(|&id| {
                    g.morphemes
                        .get(id as usize)
                        .map(|mi| format!("{}({}/{})", id, mi.xml_key, mi.gloss.as_deref().unwrap_or("-")))
                        .unwrap_or_else(|| format!("{id}(?)"))
                })
                .collect();
            println!("    root_index={} morphemes=[{}]", a.root_morpheme_index, names.join(", "));
        }
    }

    println!("\n=== gated P6 compile ===");
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let mut rules_in_order: Vec<&PhonRuleDef> = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            rules_in_order.push(&g.prules[prid.0 as usize]);
        }
    }
    let result = compile_gated_grammar(&opts, &g, &alphabet, &rules_in_order);
    println!("groups: {}", result.groups);
    for (key, roots, prefixes, suffixes) in &result.group_reports {
        println!("  group key={key:?} root_entries={roots} prefix_entries={prefixes} suffix_entries={suffixes}");
    }
    println!("skipped rules: {:?}", result.skipped_rules);

    let net = result.net.expect("gated network must be non-empty");
    let boundary_tokens: Vec<char> = table
        .iter()
        .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect();
    let net = if boundary_tokens.is_empty() {
        net
    } else {
        let cleanup_regex = boundary_tokens.iter().map(|c| format!("{c} -> 0")).collect::<Vec<_>>().join(", ");
        let cleanup_net = fsm_parse_regex(&opts, &cleanup_regex, None, None).expect("cleanup regex");
        foma::constructions::fsm_compose(&opts, net, cleanup_net)
    };
    let net = foma::minimize::fsm_minimize(&opts, net);

    println!("\n--- gated network query words ---");
    for word in ["xyx", "w"] {
        let Some(query) = alphabet.encode_query(word) else {
            println!("{word:?}: FAILED to segment");
            continue;
        };
        let mut h = apply_init(&net);
        let mut cands = Vec::new();
        for s in h.up(&query) {
            if let Some(path) = tags::decode_path(&s) {
                cands.extend(tags::to_candidates(&path));
            }
        }
        println!("{word:?}: {} candidate(s)", cands.len());
        for c in &cands {
            let names: Vec<String> = c
                .morphemes
                .iter()
                .map(|m| {
                    g.morphemes
                        .get(m.0 as usize)
                        .map(|mi| format!("{}({}/{})", m.0, mi.xml_key, mi.gloss.as_deref().unwrap_or("-")))
                        .unwrap_or_else(|| format!("{}(?)", m.0))
                })
                .collect();
            println!("    root_index={} morphemes=[{}]", c.root_index, names.join(", "));
        }
    }
}
