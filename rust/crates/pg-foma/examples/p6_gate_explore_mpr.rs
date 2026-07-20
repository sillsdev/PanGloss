//! Exploratory driver (not the final deliverable test): augments Indonesian's real grammar with 2
//! synthetic lexical entries to exercise `prule5`'s `excludedMPRFeatures="mpr1"` exclusion at the
//! exact structural juncture the real corpus never hits (see `pg-foma/src/gate.rs`'s module doc),
//! then prints what the REAL oracle (`pg_parse::Morpher`) says for 4 query words -- gathered
//! empirically, not predicted, per the investigation's own methodology. Also explores the
//! synthetic POS-gating fixture. Once verified, the derived expected values get hard-coded into
//! `tests/p6_gate_parity.rs`.

use std::path::{Path, PathBuf};

use foma::apply::apply_init;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;

use pg_foma::gate::compile_gated_grammar;
use pg_foma::replace::SegAlphabet;
use pg_foma::tags;
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::PhonRuleDef;
use pg_parse::{Morpher, ParseOptions};

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_indonesian_augmented() -> pg_grammar::model::Grammar {
    let path = sample_path("indonesian-hc.xml");
    let xml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
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
    assert_eq!(count, 1, "expected exactly one </LexicalEntries> to splice before, found {count}");
    let xml = xml.replacen("</LexicalEntries>", inject, 1);
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load augmented grammar: {e}"))
}

fn main() {
    println!("=== Indonesian + synthetic mpr1 entries: oracle exploration ===");
    let g = load_indonesian_augmented();
    let morpher = Morpher::new(&g, usize::MAX);
    let popts = ParseOptions::default();

    for word in ["menanam", "mentanam", "menabur", "mentabur", "tanam", "tabur"] {
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

    // ---------------------------------------------------------------------------------------
    // Now build the GATED P6 network and check it reproduces the oracle above.
    // ---------------------------------------------------------------------------------------
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

    let result = compile_gated_grammar(&opts, &g, &alphabet, &rules_in_order)
        .expect("compose budget ok");
    println!("groups: {}", result.groups);
    for (key, roots, prefixes, suffixes) in &result.group_reports {
        println!("  group key={key:?} root_entries={roots} prefix_entries={prefixes} suffix_entries={suffixes}");
    }
    println!("skipped rules: {:?}", result.skipped_rules);
    println!("skipped allomorphs: {} lines", result.skipped_allomorphs.len());

    let net = result.net.expect("gated network must be non-empty");

    let boundary_tokens: Vec<char> = table
        .iter()
        .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect();
    let cleanup_regex = boundary_tokens.iter().map(|c| format!("{c} -> 0")).collect::<Vec<_>>().join(", ");
    let cleanup_net = fsm_parse_regex(&opts, &cleanup_regex, None, None).expect("cleanup regex");
    let net = foma::constructions::fsm_compose(&opts, net, cleanup_net);
    let net = foma::minimize::fsm_minimize(&opts, net);

    println!("\n--- gated network query words ---");
    for word in ["menanam", "mentanam", "menabur", "mentabur"] {
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

    // ---------------------------------------------------------------------------------------
    // Regression check: full corpus parity (same predicate p6_replace_prototype.rs uses) must
    // stay 97/97 through the AUGMENTED grammar + GATED compile path.
    // ---------------------------------------------------------------------------------------
    println!("\n=== full corpus parity (regression check) ===");
    const REDUP_EXCLUDED: &[&str] = &[
        "membagi-bagi", "memijit-mijit", "meminta-minta", "mengamat-amati",
        "mengayuh-ngayuh", "menulis-nulis", "menyewa-nyewa",
    ];
    let words_text = std::fs::read_to_string(sample_path("indonesian-words.txt")).expect("read words");
    let words: Vec<&str> = words_text.lines().map(str::trim).filter(|w| !w.is_empty()).collect();
    let mut n_total = 0usize;
    let mut n_covered = 0usize;
    let mut n_words_analyzed = 0usize;
    let mut misses = Vec::new();
    for word in &words {
        if REDUP_EXCLUDED.contains(word) {
            continue;
        }
        let outcome = morpher.parse_word_opts(word, &popts);
        if outcome.structured.is_empty() {
            continue;
        }
        n_words_analyzed += 1;
        let candidates: Vec<tags::Candidate> = match alphabet.encode_query(word) {
            Some(query) => {
                let mut out = Vec::new();
                let mut h = apply_init(&net);
                for s in h.up(&query) {
                    if let Some(path) = tags::decode_path(&s) {
                        out.extend(tags::to_candidates(&path));
                    }
                }
                out
            }
            None => Vec::new(),
        };
        let mut seqs: Vec<(Vec<u32>, i32)> = Vec::new();
        for a in &outcome.structured {
            let key = (a.morpheme_ids.clone(), a.root_morpheme_index);
            if !seqs.contains(&key) {
                seqs.push(key);
            }
        }
        for (seq, root_idx) in seqs {
            n_total += 1;
            let covered = candidates.iter().any(|c| {
                c.root_index == root_idx
                    && c.morphemes.len() == seq.len()
                    && c.morphemes.iter().zip(seq.iter()).all(|(m, s)| m.0 == *s)
            });
            if covered {
                n_covered += 1;
            } else {
                misses.push(format!("word {word:?}: root_index={root_idx} morphemes={seq:?}"));
            }
        }
    }
    println!(
        "recall: {n_covered}/{n_total} across {n_words_analyzed} analyzed words (of {} corpus words)",
        words.len()
    );
    for m in &misses {
        println!("MISS {m}");
    }
    if misses.is_empty() {
        println!("ZERO misses: 100% recall preserved through the gated compile path.");
    }
}
