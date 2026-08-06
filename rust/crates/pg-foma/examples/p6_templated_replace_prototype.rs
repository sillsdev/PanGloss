//! Prototype driver: composes an underlying-templated lexc network for a large grammar via `emit_underlying_templated` (`emit::emit` OOMs on it), and skips a full-corpus recall gate since `apply_up` against the composed network can hang unboundedly on some words with no reliable in-process bound.

use std::path::{Path, PathBuf};
use std::time::Instant;

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::lexcread::fsm_lexc_parse_string;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;

use pg_foma::emit::{emit_underlying_templated, FomaTier};
use pg_foma::replace::{compile_and_compose_rules, SegAlphabet};
use pg_foma::tags;
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{Grammar, PhonRuleDef};
use pg_parse::{Morpher, ParseOptions};

/// Large stack: `fsm_compose`/`fsm_minimize` plus this grammar's deep template/slot recursion can overflow the default thread stack.
const STACK_BYTES: usize = 512 * 1024 * 1024;

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_aweti() -> Grammar {
    let path = sample_path("aweti.json");
    let json =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let snapshot = pg_snapshot::Snapshot::from_json(&json)
        .unwrap_or_else(|e| panic!("parse snapshot {}: {e}", path.display()));
    let (grammar, warnings) = pg_grammar::compile_project(&snapshot)
        .unwrap_or_else(|e| panic!("compile_project {}: {e}", path.display()));
    if !warnings.is_empty() {
        println!("  ({} compile_project warnings)", warnings.len());
    }
    grammar
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

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run)
        .expect("spawn large-stack worker thread");
    handle.join().expect("worker thread panicked");
}

fn run() {
    println!("=== P6 templated-morphotactics prototype: Aweti ===\n");

    let t_load = Instant::now();
    let g = load_aweti();
    println!("load: {:?}", t_load.elapsed());
    println!(
        "entries={} mrules={} prules={} templates={} strata={} char_tables={}\n",
        g.entries.len(),
        g.mrules.len(),
        g.prules.len(),
        g.templates.len(),
        g.strata.len(),
        g.char_tables.len()
    );

    let table = &g.char_tables[0];
    let alphabet = SegAlphabet::new(table);
    let opts = FomaOptions::default();

    // 1. Templated underlying-form lexc emitter.
    let t_emit = Instant::now();
    let result = emit_underlying_templated(&g, &alphabet, None);
    let emit_elapsed = t_emit.elapsed();
    println!("underlying (templated) lexc emit: {emit_elapsed:?}");
    println!("tier: {:?}", result.report.tier);
    println!(
        "enum_budget_exceeded: {:?}",
        result.report.enum_budget_exceeded
    );
    println!("counts: {:?}", result.report.counts);
    println!("uncovered ({}):", result.report.uncovered.len());
    for u in &result.report.uncovered {
        println!("  {} {} -- {}", u.kind, u.id, u.reason);
    }
    assert!(
        !matches!(result.report.tier, FomaTier::Unsupported { .. }),
        "emit_underlying_templated must not be Unsupported for Aweti: {:?}",
        result.report.tier
    );
    assert!(
        result.report.enum_budget_exceeded.is_none(),
        "the enumeration budget must not trip for the templated path"
    );
    assert!(
        result.report.counts.entries >= 855,
        "counts.entries={} looks too small for the real Aweti grammar (expected >= 855)",
        result.report.counts.entries
    );
    assert!(
        result.report.counts.rules >= 135,
        "counts.rules={} looks too small for the real Aweti grammar (expected >= 135)",
        result.report.counts.rules
    );

    let t_lexc = Instant::now();
    let lexc_net = fsm_lexc_parse_string(&opts, None, &result.lexc_source)
        .unwrap_or_else(|| panic!("templated underlying-form lexc failed to compile"));
    let lexc_elapsed = t_lexc.elapsed();
    println!(
        "lexc compile: {lexc_elapsed:?}; net: {} states, {} arcs",
        lexc_net.statecount, lexc_net.arccount
    );

    // 2. Compile and compose the phonological rules, in stratum order.
    let mut rules_in_order: Vec<&PhonRuleDef> = Vec::new();
    for st in &g.strata {
        for &prid in &st.prules {
            rules_in_order.push(&g.prules[prid.0 as usize]);
        }
    }
    println!(
        "\nphonological rules in stratum order: {}",
        rules_in_order.len()
    );

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
    )
    .expect("compose budget ok");
    let rules_elapsed = t_rules.elapsed();
    println!("rule compile+compose: {rules_elapsed:?}");
    println!("skipped rules: {skipped_rules:?}");
    for (rid, reports) in &tuple_reports {
        for r in reports {
            println!(
                "  alpha-tuple expansion [{rid}]: raw_product={} surviving={}",
                r.raw_product, r.surviving
            );
        }
    }
    let rule_net = rule_net.expect("Aweti's 18 rules must compile (see skipped_rules if not)");
    println!(
        "composed rule net (before lexc/cleanup): {} states, {} arcs",
        rule_net.statecount, rule_net.arccount
    );

    // 3. Boundary cleanup: every Boundary-kind char-def token maps to 0 (deleted), applied last.
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

    // 4. Compose lexc, rules, and cleanup, then minimize.
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

    // 5/6. Spot-check recall only: a full-corpus parity gate is deliberately not attempted here.
    // See docs/research/templated-underlying-apply-up-hang.md for why apply_up can hang unboundedly on this composed network.
    println!("\n--- spot-check recall (NOT a full-corpus gate -- see comment above) ---");
    let morpher = Morpher::new(&g, usize::MAX);
    let popts = ParseOptions::default();
    const SPOT_CHECK_RAW_CAP: usize = 50_000;
    for word in ["parua", "an"] {
        let outcome = morpher.parse_word_opts(word, &popts);
        let engine_seqs = engine_sequences(&outcome);
        println!(
            "{word:?}: oracle has {} analysis(es): {:?}",
            engine_seqs.len(),
            engine_seqs
        );
        let Some(query) = alphabet.encode_query(word) else {
            println!("  FAILED to segment query into token space");
            continue;
        };

        let mut covered = vec![false; engine_seqs.len()];
        let mut all_candidates: Vec<tags::Candidate> = Vec::new();
        let t0 = Instant::now();
        let mut raw_n = 0usize;
        let mut hit_cap = false;
        for s in handle.up(&query) {
            raw_n += 1;
            if raw_n > SPOT_CHECK_RAW_CAP {
                hit_cap = true;
                break;
            }
            if let Some(path) = tags::decode_path(&s) {
                for c in tags::to_candidates(&path) {
                    for (i, (seq, root_idx)) in engine_seqs.iter().enumerate() {
                        if !covered[i] && candidates_cover(std::slice::from_ref(&c), seq, *root_idx)
                        {
                            covered[i] = true;
                        }
                    }
                    all_candidates.push(c);
                }
            }
            if covered.iter().all(|&b| b) {
                break; // every oracle analysis found -- no need to keep enumerating
            }
        }
        let n_covered = covered.iter().filter(|&&b| b).count();
        println!(
            "  recall {n_covered}/{} (raw_n={raw_n}, distinct_candidates={}, elapsed={:?}{})",
            engine_seqs.len(),
            all_candidates.len(),
            t0.elapsed(),
            if hit_cap {
                ", HIT_CAP -- some analyses may be uncounted misses, not confirmed gaps"
            } else {
                ""
            }
        );
    }

    println!(
        "\n(deliberately NOT a full-corpus gate -- see the comment above and this crate's own \
         P6-Aweti task report for the full investigation trail and next steps)"
    );
    println!("\n=== done ===");
}
