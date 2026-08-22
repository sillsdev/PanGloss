//! MPR/POS subrule-gating acceptance gate: Indonesian MPR exclusion and Amharic-shaped POS gating.
//! See `docs/research/pg-foma-p6-mpr-pos-gate-parity-notes.md`.

use std::collections::HashSet;
use std::path::PathBuf;

use foma::apply::apply_init;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;

use pg_foma::gate::{compile_gated_grammar, find_gated_subrules, partition_entries};
use pg_foma::replace::{compile_and_compose_rules, SegAlphabet};
use pg_foma::tags;
use pg_foma::uflexc::emit_underlying;
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions};

const REDUP_EXCLUDED: &[&str] = &[
    "membagi-bagi",
    "memijit-mijit",
    "meminta-minta",
    "mengamat-amati",
    "mengayuh-ngayuh",
    "menulis-nulis",
    "menyewa-nyewa",
];

/// Routed through `pg_conformance_fixtures::corpus` so `PANGLOSS_CORPUS_REQUIRED` turns an absent corpus into a hard failure, not a vacuous pass.
fn sample_path(name: &str) -> PathBuf {
    pg_conformance_fixtures::corpus::path(name)
        .unwrap_or_else(|| pg_conformance_fixtures::corpus::corpus_root().join(name))
}

/// Real `indonesian-hc.xml` plus two synthetic lexical entries exercising `prule5`'s MPR exclusion at a juncture the real corpus never reaches.
/// See `docs/research/pg-foma-p6-mpr-pos-gate-parity-notes.md`.
fn load_indonesian_augmented() -> Option<Grammar> {
    let path = sample_path("indonesian-hc.xml");
    if !path.exists() {
        return None;
    }
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let inject = r#"
          <LexicalEntry id="entry67" partOfSpeech="pos2014">
            <Allomorphs>
              <Allomorph id="allo67">
                <PhoneticShape>tanam</PhoneticShape>
              </Allomorph>
            </Allomorphs>
            <Gloss>synthetic-test-tanam</Gloss>
          </LexicalEntry>
          <LexicalEntry id="entry68" partOfSpeech="pos2014" ruleFeatures="mpr1">
            <Allomorphs>
              <Allomorph id="allo68">
                <PhoneticShape>tabur</PhoneticShape>
              </Allomorph>
            </Allomorphs>
            <Gloss>synthetic-test-tabur-mpr1</Gloss>
          </LexicalEntry>
        </LexicalEntries>"#;
    let count = xml.matches("</LexicalEntries>").count();
    assert_eq!(
        count, 1,
        "expected exactly one </LexicalEntries> to splice before, found {count}"
    );
    let xml = xml.replacen("</LexicalEntries>", inject, 1);
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load augmented grammar: {e}")))
}

/// Minimal, template-less, hand-authored grammar reproducing Amharic `prule1`'s exact shape.
/// See `docs/research/pg-foma-p6-mpr-pos-gate-parity-notes.md`.
const POS_FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
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

fn rules_in_order(g: &Grammar) -> Vec<&PhonRuleDef> {
    let mut out = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            out.push(&g.prules[prid.0 as usize]);
        }
    }
    out
}

fn boundary_cleanup(
    opts: &FomaOptions,
    table: &pg_grammar::chardef::CharDefTable,
    alphabet: &SegAlphabet,
) -> Option<foma::types::Fsm> {
    let boundary_tokens: Vec<char> = table
        .iter()
        .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect();
    if boundary_tokens.is_empty() {
        return None;
    }
    let cleanup_regex = boundary_tokens
        .iter()
        .map(|c| format!("{c} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    Some(fsm_parse_regex(opts, &cleanup_regex, None, None).expect("boundary cleanup regex"))
}

/// Query `net` and decode every candidate as `(morpheme_ids, root_index)` sets.
fn query_candidates(
    net: &foma::types::Fsm,
    alphabet: &SegAlphabet,
    word: &str,
) -> HashSet<(Vec<u32>, i32)> {
    let mut out = HashSet::new();
    let Some(query) = alphabet.encode_query(word) else {
        return out;
    };
    let mut h = apply_init(net);
    for s in h.up(&query) {
        if let Some(path) = tags::decode_path(&s) {
            for c in tags::to_candidates(&path) {
                out.insert((c.morphemes.iter().map(|m| m.0).collect(), c.root_index));
            }
        }
    }
    out
}

fn oracle_analyses(morpher: &Morpher, word: &str) -> HashSet<(Vec<u32>, i32)> {
    let popts = ParseOptions::default();
    morpher
        .parse_word_opts(word, &popts)
        .structured
        .iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect()
}

/// Case 1: the gated compile must match the real oracle exactly on all four query words.
/// See `docs/research/pg-foma-p6-mpr-pos-gate-parity-notes.md`.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn indonesian_mpr_exclusion_matches_oracle() {
    let Some(g) = load_indonesian_augmented() else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let morpher = Morpher::new(&g, usize::MAX);

    // Oracle ground truth, gathered empirically, not predicted.
    // See `docs/research/pg-foma-p6-mpr-pos-gate-parity-notes.md`.
    assert!(
        !oracle_analyses(&morpher, "menanam").is_empty(),
        "oracle sanity: menanam must analyze"
    );
    assert!(
        oracle_analyses(&morpher, "mentanam").is_empty(),
        "oracle sanity: mentanam must NOT analyze"
    );
    assert!(
        oracle_analyses(&morpher, "menabur").is_empty(),
        "oracle sanity: menabur must NOT analyze"
    );
    assert!(
        !oracle_analyses(&morpher, "mentabur").is_empty(),
        "oracle sanity: mentabur must analyze"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);

    let result = compile_gated_grammar(&opts, &g, &alphabet, &ro).expect("compose budget ok");
    assert_eq!(
        result.groups, 2,
        "exactly 2 gating groups expected: {:?}",
        result.group_reports
    );
    let mut net = result.net.expect("gated network must be non-empty");
    if let Some(cleanup) = boundary_cleanup(&opts, table, &alphabet) {
        net = foma::constructions::fsm_compose(&opts, net, cleanup);
    }
    net = foma::minimize::fsm_minimize(&opts, net);

    for word in ["menanam", "mentanam", "menabur", "mentabur"] {
        let want = oracle_analyses(&morpher, word);
        let got = query_candidates(&net, &alphabet, word);
        assert_eq!(
            got, want,
            "gated network must match the oracle exactly for {word:?}"
        );
    }
    pg_conformance_fixtures::corpus::record_cases("indonesian_mpr_exclusion_matches_oracle", 4);
}

/// Demonstrates the recall gap: the ungated cascade must miss `mentabur`'s real analysis, proving the gate above is not vacuous.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn ungated_cascade_would_have_missed_the_excluded_root() {
    let Some(g) = load_indonesian_augmented() else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let morpher = Morpher::new(&g, usize::MAX);
    assert!(
        !oracle_analyses(&morpher, "mentabur").is_empty(),
        "oracle sanity"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);

    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rules_net =
        compile_and_compose_rules(&opts, &g, &alphabet, &ro, &mut skipped, &mut tuple_reports)
            .expect("compose budget ok")
            .expect(
                "ungated cascade must still compile (prule5's shape is supported, just ungated)",
            );
    let ureport = emit_underlying(&g, &alphabet).expect("compose budget ok");
    let lexc_net = foma::lexcread::fsm_lexc_parse_string(&opts, None, &ureport.lexc_source)
        .expect("underlying lexc must compile");
    let mut net = foma::constructions::fsm_compose(&opts, lexc_net, rules_net);
    if let Some(cleanup) = boundary_cleanup(&opts, table, &alphabet) {
        net = foma::constructions::fsm_compose(&opts, net, cleanup);
    }
    net = foma::minimize::fsm_minimize(&opts, net);

    let got = query_candidates(&net, &alphabet, "mentabur");
    assert!(
        got.is_empty(),
        "the UNGATED cascade should incorrectly reject 'mentabur' (obligatory deletion with no \
         exception) -- got {got:?}; if this now passes, prule5's compiled regex changed shape and \
         this regression guard needs re-deriving, not deleting"
    );
}

/// Case 2 (module doc): the synthetic POS-gated fixture, gated network vs oracle.
#[test]
fn synthetic_pos_gate_matches_oracle() {
    let g = pg_grammar::load(POS_FIXTURE_XML)
        .unwrap_or_else(|e| panic!("failed to load POS fixture: {e}\n{POS_FIXTURE_XML}"));
    let morpher = Morpher::new(&g, usize::MAX);

    // Oracle ground truth: "xyx" (undeleted) can only be the noun entry; "w" (merged) only the verb.
    assert!(
        !oracle_analyses(&morpher, "xyx").is_empty(),
        "oracle sanity: xyx must analyze (noun)"
    );
    assert!(
        !oracle_analyses(&morpher, "w").is_empty(),
        "oracle sanity: w must analyze (verb)"
    );
    assert_ne!(
        oracle_analyses(&morpher, "xyx"),
        oracle_analyses(&morpher, "w"),
        "must be distinct roots"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);

    let result = compile_gated_grammar(&opts, &g, &alphabet, &ro).expect("compose budget ok");
    assert_eq!(
        result.groups, 2,
        "exactly 2 gating groups (verb/noun) expected: {:?}",
        result.group_reports
    );
    let mut net = result.net.expect("gated network must be non-empty");
    if let Some(cleanup) = boundary_cleanup(&opts, table, &alphabet) {
        net = foma::constructions::fsm_compose(&opts, net, cleanup);
    }
    net = foma::minimize::fsm_minimize(&opts, net);

    for word in ["xyx", "w"] {
        let want = oracle_analyses(&morpher, word);
        let got = query_candidates(&net, &alphabet, word);
        assert_eq!(
            got, want,
            "gated network must match the oracle exactly for {word:?}"
        );
    }
}

/// Mirrors `ungated_cascade_would_have_missed_the_excluded_root` for case 2: the POS gate closes a real recall gap.
/// See `docs/research/pg-foma-p6-mpr-pos-gate-parity-notes.md`.
#[test]
fn ungated_cascade_would_have_missed_the_noun_entry() {
    let g = pg_grammar::load(POS_FIXTURE_XML)
        .unwrap_or_else(|e| panic!("failed to load POS fixture: {e}\n{POS_FIXTURE_XML}"));
    let morpher = Morpher::new(&g, usize::MAX);
    assert!(
        !oracle_analyses(&morpher, "xyx").is_empty(),
        "oracle sanity: xyx must analyze (noun)"
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);

    let mut skipped = Vec::new();
    let mut tuple_reports = Vec::new();
    let rules_net =
        compile_and_compose_rules(&opts, &g, &alphabet, &ro, &mut skipped, &mut tuple_reports)
            .expect("compose budget ok")
            .expect(
                "ungated cascade must still compile (prule1's shape is supported, just ungated)",
            );
    let ureport = emit_underlying(&g, &alphabet).expect("compose budget ok");
    let lexc_net = foma::lexcread::fsm_lexc_parse_string(&opts, None, &ureport.lexc_source)
        .expect("underlying lexc must compile");
    let mut net = foma::constructions::fsm_compose(&opts, lexc_net, rules_net);
    if let Some(cleanup) = boundary_cleanup(&opts, table, &alphabet) {
        net = foma::constructions::fsm_compose(&opts, net, cleanup);
    }
    net = foma::minimize::fsm_minimize(&opts, net);

    let got = query_candidates(&net, &alphabet, "xyx");
    assert!(
        got.is_empty(),
        "the UNGATED cascade should incorrectly reject raw 'xyx' (obligatory merge with no POS \
         exception applied to both entries) -- got {got:?}; if this now passes, prule1's compiled \
         regex changed shape and this regression guard needs re-deriving, not deleting"
    );
}

/// Regression: the full Indonesian 97/97 corpus parity gate must stay 100% through the augmented, gated compile path.
/// See `docs/research/pg-foma-p6-mpr-pos-gate-parity-notes.md`.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn indonesian_full_corpus_parity_unregressed() {
    let Some(g) = load_indonesian_augmented() else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let morpher = Morpher::new(&g, usize::MAX);
    let popts = ParseOptions::default();

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();
    let ro = rules_in_order(&g);

    let result = compile_gated_grammar(&opts, &g, &alphabet, &ro).expect("compose budget ok");
    let mut net = result.net.expect("gated network must be non-empty");
    if let Some(cleanup) = boundary_cleanup(&opts, table, &alphabet) {
        net = foma::constructions::fsm_compose(&opts, net, cleanup);
    }
    net = foma::minimize::fsm_minimize(&opts, net);

    let words_text =
        std::fs::read_to_string(sample_path("indonesian-words.txt")).expect("read words");
    let words: Vec<&str> = words_text
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    assert!(
        !words.iter().any(|&w| w == "tanam" || w == "tabur"),
        "synthetic roots must not collide with the real corpus"
    );

    let mut n_total = 0usize;
    let mut n_covered = 0usize;
    let mut n_words_analyzed = 0usize;
    let mut misses: Vec<String> = Vec::new();

    for &word in &words {
        if REDUP_EXCLUDED.contains(&word) {
            continue;
        }
        let outcome = morpher.parse_word_opts(word, &popts);
        if outcome.structured.is_empty() {
            continue;
        }
        n_words_analyzed += 1;
        let candidates = query_candidates(&net, &alphabet, word);

        let mut seqs: Vec<(Vec<u32>, i32)> = Vec::new();
        for a in &outcome.structured {
            let key = (a.morpheme_ids.clone(), a.root_morpheme_index);
            if !seqs.contains(&key) {
                seqs.push(key);
            }
        }
        for seq in seqs {
            n_total += 1;
            if candidates.contains(&seq) {
                n_covered += 1;
            } else {
                misses.push(format!("word {word:?}: {seq:?}"));
            }
        }
    }

    let compared_words = words
        .iter()
        .filter(|word| !REDUP_EXCLUDED.contains(word))
        .count();
    pg_conformance_fixtures::corpus::record_cases(
        "indonesian_full_corpus_parity_unregressed",
        compared_words,
    );

    assert_eq!(
        n_covered, n_total,
        "gated compile path must preserve 100% recall on the Indonesian corpus (misses: {misses:?})"
    );
    assert_eq!(
        n_words_analyzed, 96,
        "sanity: same analyzed-word denominator p6_replace_prototype.rs reports"
    );
    assert_eq!(
        n_total, 97,
        "sanity: same engine-analysis count p6_replace_prototype.rs reports"
    );
}

/// Regression: Amharic's 3 real POS-gated subrules are found without crashing, and the untouched entry point reproduces its exact tuple-expansion numbers.
/// See `docs/research/pg-foma-p6-mpr-pos-gate-parity-notes.md`.
#[test]
#[ignore]
fn amharic_gated_subrules_and_tuple_counts_unregressed() {
    const STACK_BYTES: usize = 512 * 1024 * 1024;
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(|| {
            let path = sample_path("amharic-hc.xml");
            let xml = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let g = pg_grammar::load(&xml)
                .unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}"));

            let table = &g.char_tables[0];
            let alphabet = SegAlphabet::new(table);
            let opts = FomaOptions::default();
            let ro = rules_in_order(&g);

            let gated = find_gated_subrules(&g, &ro);
            assert_eq!(
                gated.len(),
                3,
                "expected prule1/prule2/prule3's own subrule, got {gated:?}"
            );
            let names: HashSet<&str> = gated
                .iter()
                .map(|gs| {
                    let PhonRuleDef::Rewrite(r) = ro[gs.rule_pos] else {
                        unreachable!()
                    };
                    r.xml_id.as_str()
                })
                .collect();
            assert_eq!(
                names,
                ["prule1", "prule2", "prule3"].into_iter().collect(),
                "gated subrules must be exactly prule1/prule2/prule3"
            );

            let groups = partition_entries(&g, &gated, &ro);
            assert!(
                !groups.is_empty(),
                "partitioning must not crash and must produce >=1 group"
            );
            let total: usize = groups.iter().map(|grp| grp.entries.len()).sum();
            assert_eq!(
                total,
                g.entries.len(),
                "every entry must land in exactly one group"
            );

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
            .expect("compose budget ok")
            .expect("Amharic's 7 rules must still compile via the untouched entry point");
            assert!(
                skipped.is_empty(),
                "no Amharic rule should be newly skipped: {skipped:?}"
            );
            assert_eq!(
                composed.statecount, 82,
                "Amharic composed net state count must be unchanged"
            );
            assert_eq!(
                composed.arccount, 1_110_358,
                "Amharic composed net arc count must be unchanged"
            );
            pg_conformance_fixtures::corpus::record_cases(
                "amharic_gated_subrules_and_tuple_counts_unregressed",
                gated.len(),
            );
        })
        .expect("spawn large-stack worker thread");
    handle.join().expect("amharic worker thread panicked");
}
