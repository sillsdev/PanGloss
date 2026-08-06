//! Pinned-worst-word generator: runs the full corpus through `analyze_words` three times, takes each word's median per-word time to suppress contention noise, and prints the top 20 outliers per grammar (median/min/max ms, candidate count, hex byte dump) for manual pinning into a gitignored fixture.

use std::path::{Path, PathBuf};

use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;

const GRAMMARS: &[(&str, &str, &str, usize)] = &[
    ("sena", "sena-hc.xml", "sena-words.txt", 20),
    ("amharic", "amharic-hc.xml", "amharic-words.txt", 20),
];

fn sample_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name)
}

fn hex(s: &str) -> String {
    s.bytes()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn run(name: &str, xml: &str, words_file: &str, top_n: usize) {
    let Ok(xml_text) = std::fs::read_to_string(sample_path(xml)) else {
        println!("{name}: SKIPPED (no grammar fixture)");
        return;
    };
    let g: Grammar = pg_grammar::load(&xml_text).unwrap_or_else(|e| panic!("load {name}: {e}"));
    let words: Vec<String> = std::fs::read_to_string(sample_path(words_file))
        .expect("words fixture")
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();

    let mut analyzer = FomaAnalyzer::new(&g).expect("analyzer build failed");
    // Per-word ms across 3 runs, plus the candidate count (stable, take from run 1).
    let n = words.len();
    let mut times: Vec<Vec<f64>> = vec![Vec::with_capacity(3); n];
    let mut cands: Vec<usize> = vec![0; n];
    for run_i in 0..3 {
        let outcomes = analyzer.analyze_words(&words);
        for (i, (o, d)) in outcomes.iter().enumerate() {
            times[i].push(d.as_secs_f64() * 1000.0);
            if run_i == 0 {
                cands[i] = o.candidates_generated;
            }
        }
    }

    let mut rows: Vec<(usize, f64, f64, f64)> = (0..n)
        .map(|i| {
            let mut t = times[i].clone();
            t.sort_by(|a, b| a.partial_cmp(b).unwrap());
            (i, t[1], t[0], t[2]) // median (of 3), min, max
        })
        .collect();
    rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("=== {name}: top {top_n} words by median-of-3 per-word ms (corpus n={n}) ===");
    for (rank, (i, med, lo, hi)) in rows.iter().take(top_n).enumerate() {
        println!(
            "{:>3}. median={:>9.2}ms min={:>9.2} max={:>9.2} cands={:<5} word={}  hex=[{}]",
            rank + 1,
            med,
            lo,
            hi,
            cands[*i],
            words[*i],
            hex(&words[*i]),
        );
    }
    println!();
}

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(|| {
            for (name, xml, words, top_n) in GRAMMARS {
                run(name, xml, words, *top_n);
            }
        })
        .expect("spawn");
    handle.join().expect("worst_words thread panicked");
}
