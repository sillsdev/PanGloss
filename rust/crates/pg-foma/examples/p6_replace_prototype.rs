//! Feasibility prototype driver: builds the composed `underlying-lexc .o. rules .o. boundary-cleanup` network for Indonesian, runs a smoke test, then a full corpus parity gate against `pg_parse::Morpher`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;

use pg_foma::replace::{compile_and_compose_rules, is_fully_supported_shape, SegAlphabet};
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

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_indonesian() -> Grammar {
    let path = sample_path("indonesian-hc.xml");
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load indonesian-hc.xml: {e}"))
}

fn engine_sequences(outcome: &pg_parse::ParseOutcome) -> Vec<(Vec<u32>, i32)> {
    let mut seqs: Vec<(Vec<u32>, i32)> = Vec::new();
    for a in &outcome.structured {
        let key = (a.morpheme_ids.clone(), a.root_morpheme_index);
        if !seqs.contains(&key) {
            seqs.push(key);
        }
    }
    seqs
}

fn candidates_cover(candidates: &[tags::Candidate], seq: &[u32], root_idx: i32) -> bool {
    candidates.iter().any(|c| {
        c.root_index == root_idx
            && c.morphemes.len() == seq.len()
            && c.morphemes.iter().zip(seq.iter()).all(|(m, s)| m.0 == *s)
    })
}

/// The vendored foma-rs's `fsm_union`/`fsm_compose` recurse deeply enough to overflow the default thread stack, so this runs on a dedicated large-stack thread.
const STACK_BYTES: usize = 256 * 1024 * 1024;

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run)
        .expect("spawn large-stack worker thread");
    handle.join().expect("worker thread panicked");
}

fn run() {
    println!("=== P6 replace-rule compilation prototype: Indonesian ===\n");

    let g = load_indonesian();
    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    // Compile + compose the phonological rule cascade, in stratum/document order.
    let mut rules_in_order: Vec<&PhonRuleDef> = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            rules_in_order.push(&g.prules[prid.0 as usize]);
        }
    }
    println!(
        "phonological rules in stratum order: {}",
        rules_in_order.len()
    );
    for pr in &rules_in_order {
        match pr {
            PhonRuleDef::Rewrite(r) => println!(
                "  {} {:?} mode={:?} dir={:?} fully-supported-shape={}",
                r.xml_id,
                r.name,
                r.mode,
                r.dir,
                is_fully_supported_shape(&g, r)
            ),
            PhonRuleDef::Metathesis(m) => println!("  {} (metathesis)", m.xml_id),
        }
    }

    let t_rules = Instant::now();
    let mut skipped_rules: Vec<String> = Vec::new();
    let mut tuple_reports: Vec<(String, Vec<pg_foma::replace::TupleReport>)> = Vec::new();
    let rule_net = compile_and_compose_rules(
        &opts,
        &g,
        &alphabet,
        &rules_in_order,
        &mut skipped_rules,
        &mut tuple_reports,
    );
    let rules_elapsed = t_rules.elapsed();
    println!("\nrule compile+compose: {rules_elapsed:?}");
    println!("skipped rules: {skipped_rules:?}");
    for (rid, reports) in &tuple_reports {
        for r in reports {
            println!(
                "  alpha-tuple expansion [{rid}]: raw_product={} surviving={}",
                r.raw_product, r.surviving
            );
        }
    }
    let rule_net = rule_net.expect("Indonesian's 5 rules must compile (see skipped_rules if not)");
    // Diagnostic: run the rule cascade alone (no lexc, no cleanup) to isolate whether the cascade itself is correct.
    {
        let m = table.lookup_nfd("m").expect("m in table");
        let e = table.lookup_nfd("e").expect("e in table");
        let placeholder = table
            .lookup_nfd("\u{207f}")
            .expect("placeholder nasal in table");
        let bound = table.lookup_nfd("+").expect("+ boundary in table");
        let mut h = apply_init(&rule_net);
        for root in ["baca", "tulis", "pukul", "ambil"] {
            let mut u = String::new();
            u.push(alphabet.token(m));
            u.push(alphabet.token(e));
            u.push(alphabet.token(placeholder));
            u.push(alphabet.token(bound));
            u.push_str(&alphabet.encode_query(root).expect("root segments"));
            let results: Vec<String> = h.down(&u).collect();
            println!("diag apply_down(meN+{root}): {} result(s)", results.len());
            for r in &results {
                let hex: Vec<String> = r.chars().map(|c| format!("{:04x}", c as u32)).collect();
                println!("    [{}]", hex.join(" "));
            }
        }
    }
    println!(
        "composed rule net (before lexc/cleanup): {} states, {} arcs",
        rule_net.statecount, rule_net.arccount
    );

    // Boundary cleanup: every Boundary-kind char-def's token -> 0 (deleted), applied last.
    let boundary_tokens: Vec<char> = table
        .iter()
        .filter(|(_, cd)| cd.kind() == CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect();
    println!("\nboundary tokens to strip: {}", boundary_tokens.len());
    let cleanup_regex = boundary_tokens
        .iter()
        .map(|c| format!("{c} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    let cleanup_net = fsm_parse_regex(&opts, &cleanup_regex, None, None)
        .unwrap_or_else(|| panic!("boundary cleanup regex failed to compile: {cleanup_regex:?}"));

    // Underlying-form lexc emitter + compile.
    let t_emit = Instant::now();
    let ureport = emit_underlying(&g, &alphabet).expect("compose budget ok");
    let emit_elapsed = t_emit.elapsed();
    println!(
        "\nunderlying lexc emit: {emit_elapsed:?}; root_entries={} prefix_entries={} suffix_entries={}",
        ureport.root_entries, ureport.prefix_entries, ureport.suffix_entries
    );
    println!("skipped allomorphs ({}):", ureport.skipped.len());
    for s in &ureport.skipped {
        println!("  skipped: {s}");
    }

    let t_lexc = Instant::now();
    let lexc_net = fsm_lexc_parse_string(&opts, None, &ureport.lexc_source)
        .unwrap_or_else(|| panic!("underlying-form lexc failed to compile"));
    let lexc_elapsed = t_lexc.elapsed();
    println!(
        "lexc compile: {lexc_elapsed:?}; net: {} states, {} arcs",
        lexc_net.statecount, lexc_net.arccount
    );

    // Compose: lexc .o. rules .o. cleanup, then minimize.
    let t_compose = Instant::now();
    let composed = fsm_compose(&opts, lexc_net, rule_net);
    let composed = fsm_compose(&opts, composed, cleanup_net);
    let composed = fsm_minimize(&opts, composed);
    let compose_elapsed = t_compose.elapsed();
    println!(
        "\nfull composition + minimize: {compose_elapsed:?}; final net: {} states, {} arcs",
        composed.statecount, composed.arccount
    );

    let mut handle = apply_init(&composed);

    // Smoke test: a few words before the full gate.
    println!("\n--- smoke test ---");
    for word in ["menulis", "membaca", "mengambil", "tulis"] {
        let Some(query) = alphabet.encode_query(word) else {
            println!("{word:?}: FAILED to segment query into token space");
            continue;
        };
        let mut cands = Vec::new();
        for s in handle.up(&query) {
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
                        .map(|mi| {
                            format!(
                                "{}({}/{})",
                                m.0,
                                mi.xml_key,
                                mi.gloss.as_deref().unwrap_or("-")
                            )
                        })
                        .unwrap_or_else(|| format!("{}(?)", m.0))
                })
                .collect();
            println!(
                "    root_index={} morphemes=[{}]",
                c.root_index,
                names.join(", ")
            );
        }
    }

    // Full corpus parity gate.
    println!("\n--- full corpus parity gate ---");
    let morpher = Morpher::new(&g, usize::MAX);
    let popts = ParseOptions::default();
    let words_text = std::fs::read_to_string(sample_path("indonesian-words.txt"))
        .expect("read indonesian-words.txt");
    let words: Vec<&str> = words_text
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();

    let mut n_total = 0usize;
    let mut n_covered = 0usize;
    let mut n_words_analyzed = 0usize;
    let mut misses: Vec<String> = Vec::new();
    let mut propose_time = std::time::Duration::ZERO;
    let mut max_propose = std::time::Duration::ZERO;
    let mut total_overgenerated = 0usize;

    for word in &words {
        if REDUP_EXCLUDED.contains(word) {
            continue;
        }
        let outcome = morpher.parse_word_opts(word, &popts);
        if outcome.structured.is_empty() {
            continue;
        }
        n_words_analyzed += 1;

        let t0 = Instant::now();
        let candidates: Vec<tags::Candidate> = match alphabet.encode_query(word) {
            Some(query) => {
                let mut out = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for s in handle.up(&query) {
                    if let Some(path) = tags::decode_path(&s) {
                        for c in tags::to_candidates(&path) {
                            let key: (Vec<u32>, i32) =
                                (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
                            if seen.insert(key) {
                                out.push(c);
                            }
                        }
                    }
                }
                out
            }
            None => Vec::new(),
        };
        let dt = t0.elapsed();
        propose_time += dt;
        max_propose = max_propose.max(dt);
        total_overgenerated += candidates.len();

        for (seq, root_idx) in engine_sequences(&outcome) {
            n_total += 1;
            if candidates_cover(&candidates, &seq, root_idx) {
                n_covered += 1;
            } else {
                let names: Vec<String> = seq
                    .iter()
                    .map(|&id| {
                        g.morphemes
                            .get(id as usize)
                            .map(|mi| {
                                format!(
                                    "{}({}/{})",
                                    id,
                                    mi.xml_key,
                                    mi.gloss.as_deref().unwrap_or("-")
                                )
                            })
                            .unwrap_or_else(|| format!("{id}(?)"))
                    })
                    .collect();
                misses.push(format!(
                    "word {word:?}: engine analysis root_index={root_idx} morphemes=[{}]",
                    names.join(", ")
                ));
            }
        }
    }

    println!(
        "recall: {n_covered}/{n_total} engine analyses covered across {n_words_analyzed} analyzed words \
         (of {} corpus words, {} excluded)",
        words.len(),
        REDUP_EXCLUDED.len()
    );
    println!(
        "propose total: {propose_time:?}; propose max/word: {max_propose:?}; propose mean/word: {:?}",
        propose_time / (n_words_analyzed.max(1) as u32)
    );
    println!(
        "total candidates proposed across corpus (overgeneration count): {total_overgenerated}"
    );
    if !misses.is_empty() {
        println!("--- MISSES ({} of {n_total}) ---", misses.len());
        for m in &misses {
            println!("MISS {m}");
        }
    } else {
        println!("ZERO misses: 100% recall on this corpus/predicate.");
    }

    println!("\n=== done ===");
}

// silence "unused" for Fsm import used only via method calls on returned values
#[allow(dead_code)]
fn _touch(_: &Fsm) {}
