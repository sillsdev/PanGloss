//! Checks whether the real production path (`emit::emit` with `preexpand` on) covers a specific infix+ablaut+suffix word that a junction/preexpand-off probe missed.

use std::path::{Path, PathBuf};

use pg_foma::analyzer::FomaProposer;
use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

const STACK_BYTES: usize = 256 * 1024 * 1024;

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_amharic() -> Grammar {
    let path = sample_path("amharic-hc.xml");
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}"))
}

fn seq_names(g: &Grammar, seq: &[u32]) -> String {
    seq.iter()
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
        .collect::<Vec<_>>()
        .join(", ")
}

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(run)
        .expect("spawn large-stack worker thread");
    handle.join().expect("worker thread panicked");
}

fn run() {
    let word = "ሰብሬ";
    println!("=== mainline production-path check on {word:?} ===\n");
    let g = load_amharic();

    // 1. Re-confirm the engine's own analysis (ground truth).
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();
    let outcome = morpher.parse_word_opts(word, &opts);
    println!("engine analyses ({}):", outcome.structured.len());
    let mut seqs: Vec<(Vec<u32>, i32)> = Vec::new();
    for a in &outcome.structured {
        let key = (a.morpheme_ids.clone(), a.root_morpheme_index);
        if !seqs.contains(&key) {
            seqs.push(key.clone());
        }
    }
    for (seq, root_idx) in &seqs {
        println!("  root_index={root_idx} morphemes=[{}]", seq_names(&g, seq));
    }

    // 2. Mainline emit + propose (crate::emit::emit, preexpand ON -- the real production path).
    println!("\n--- FomaProposer (mainline emit, preexpand ON) ---");
    let t0 = std::time::Instant::now();
    let mut proposer = FomaProposer::new(&g).expect("Amharic compiles (mainline)");
    println!("mainline emit+compile: {:?}", t0.elapsed());
    let candidates = proposer.propose(word);
    println!("propose: {} candidate(s)", candidates.len());
    for c in &candidates {
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
            "  root_index={} morphemes=[{}]",
            c.root_index,
            names.join(", ")
        );
    }

    let mut covered_by_propose = false;
    for (seq, root_idx) in &seqs {
        let hit = candidates.iter().any(|c| {
            c.root_index == *root_idx
                && c.morphemes.len() == seq.len()
                && c.morphemes.iter().zip(seq.iter()).all(|(m, s)| m.0 == *s)
        });
        println!(
            "  engine analysis root_index={root_idx} morphemes=[{}] -> propose covers: {hit}",
            seq_names(&g, seq)
        );
        covered_by_propose |= hit;
    }

    // 3. Full production path: FomaAnalyzer (propose -> confirm), the actual product API.
    println!("\n--- FomaAnalyzer (propose -> confirm, the real product API) ---");
    let mut analyzer = FomaAnalyzer::new(&g).expect("Amharic compiles (mainline)");
    let foma_outcome = analyzer.analyze_word(word);
    println!(
        "analyze_word: {} structured analyses",
        foma_outcome.structured.len()
    );
    for a in &foma_outcome.structured {
        println!(
            "  root_index={} morphemes=[{}]",
            a.root_morpheme_index,
            seq_names(&g, &a.morpheme_ids)
        );
    }
    let engine_ms: Vec<(Vec<u32>, i32)> = {
        let mut m: Vec<(Vec<u32>, i32)> = outcome
            .structured
            .iter()
            .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
            .collect();
        m.sort();
        m
    };
    let foma_ms: Vec<(Vec<u32>, i32)> = {
        let mut m: Vec<(Vec<u32>, i32)> = foma_outcome
            .structured
            .iter()
            .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
            .collect();
        m.sort();
        m
    };
    println!(
        "\nengine multiset == foma multiset: {}",
        engine_ms == foma_ms
    );

    println!(
        "\n=== VERDICT: mainline (preexpand ON) {} on {word:?} ===",
        if covered_by_propose {
            "COVERS (propose finds it)"
        } else {
            "MISSES (propose does NOT find it)"
        }
    );
}
