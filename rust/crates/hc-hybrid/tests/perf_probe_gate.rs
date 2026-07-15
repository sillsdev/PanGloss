//! TEMPORARY INVESTIGATION INSTRUMENTATION (not part of the permanent test suite; added to
//! diagnose the ~100ms/word "worst case" hybrid-parse latency question, per
//! `reports/03-parse-latency-profile.md`). Not a correctness gate -- no golden comparison, no
//! assertions beyond sanity. Safe to delete once the investigation report is written.
//!
//! Times, per word: propose (`CompositeAnalyzer::analyze_word`) vs verify (sum of
//! `replay::confirm_checked` over every surviving candidate), plus candidate count and whether any
//! candidate's restricted verify timed out. Writes a per-word CSV to stdout (redirect to a file)
//! and a summary (percentiles + top-N slowest) to stderr.
//!
//! Run (release only -- never trust debug timings):
//! `cargo test -p hc-hybrid --release --test perf_probe_gate -- --ignored --nocapture sena_probe
//!   > /tmp/sena_probe.csv`
//!
//! Word-count subset is controlled by the `HC_PERF_PROBE_LIMIT` env var (default: all words in the
//! corpus) so a quick partial run is possible without editing this file.

use std::path::{Path, PathBuf};
use std::time::Instant;

use hc_hybrid::composite::CompositeAnalyzer;
use hc_hybrid::replay;
use hc_hybrid::surface::SurfacePhonology;
use hc_hybrid::trie::Trie;
use hc_parse::Morpher;

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn read_words(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .expect("read word list")
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

struct WordStat {
    idx: usize,
    word: String,
    propose_ns: u128,
    verify_ns: u128,
    total_ns: u128,
    n_candidates: usize,
    n_verified: usize,
    any_timed_out: bool,
}

fn percentile(sorted: &[u128], p: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn run_probe(label: &str, grammar_xml_name: &str, words_file: &str) {
    let Some(gpath) = sample_path(grammar_xml_name) else {
        eprintln!("skipping {label}: {grammar_xml_name} not present on disk");
        return;
    };
    let Some(wpath) = sample_path(words_file) else {
        eprintln!("skipping {label}: {words_file} not present on disk");
        return;
    };

    let xml = std::fs::read_to_string(&gpath).expect("read grammar xml");

    let load_start = Instant::now();
    let g = hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {label} grammar: {e}"));
    let grammar_load_ms = load_start.elapsed().as_secs_f64() * 1000.0;

    let build_start = Instant::now();
    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, hc_hybrid::walk::DEFAULT_MAX_BEAM_WORK, false);
    let trie_build_ms = build_start.elapsed().as_secs_f64() * 1000.0;

    let verify_morpher = Morpher::new(&g, usize::MAX)
        .with_word_timeout(Some(std::time::Duration::from_secs(30)));
    let owners = replay::build_morpheme_owners(&g);

    let mut words = read_words(&wpath);
    if let Ok(limit) = std::env::var("HC_PERF_PROBE_LIMIT") {
        if let Ok(n) = limit.parse::<usize>() {
            words.truncate(n);
        }
    }

    eprintln!(
        "{label}: grammar load {grammar_load_ms:.1}ms, trie+composite build {trie_build_ms:.1}ms, \
         probing {} words",
        words.len()
    );

    let mut stats: Vec<WordStat> = Vec::with_capacity(words.len());
    let run_start = Instant::now();

    for (i, word) in words.iter().enumerate() {
        let word_start = Instant::now();

        let propose_start = Instant::now();
        let candidates = composite.analyze_word(word);
        let propose_ns = propose_start.elapsed().as_nanos();

        let mut verify_ns: u128 = 0;
        let mut n_verified = 0usize;
        let mut any_timed_out = false;
        for c in &candidates {
            let vstart = Instant::now();
            let (found, timed_out) = replay::confirm_checked(&g, &owners, &verify_morpher, c, word);
            verify_ns += vstart.elapsed().as_nanos();
            any_timed_out |= timed_out;
            if found.is_some() {
                n_verified += 1;
            }
        }

        let total_ns = word_start.elapsed().as_nanos();
        stats.push(WordStat {
            idx: i,
            word: word.clone(),
            propose_ns,
            verify_ns,
            total_ns,
            n_candidates: candidates.len(),
            n_verified,
            any_timed_out,
        });

        if i % 200 == 0 {
            eprintln!(
                "{label}: progress {i}/{} words, {:.1}s elapsed",
                words.len(),
                run_start.elapsed().as_secs_f64()
            );
        }

        // Per-word CSV to stdout.
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            i, word, propose_ns, verify_ns, total_ns, candidates.len(), n_verified, any_timed_out
        );
    }

    let total_elapsed = run_start.elapsed();

    // Summary to stderr.
    let mut totals: Vec<u128> = stats.iter().map(|s| s.total_ns).collect();
    let mut proposes: Vec<u128> = stats.iter().map(|s| s.propose_ns).collect();
    let mut verifies: Vec<u128> = stats.iter().map(|s| s.verify_ns).collect();
    let mut cand_counts: Vec<u128> = stats.iter().map(|s| s.n_candidates as u128).collect();
    totals.sort();
    proposes.sort();
    verifies.sort();
    cand_counts.sort();

    let ms = |ns: u128| ns as f64 / 1_000_000.0;

    eprintln!("\n=== {label} summary ({} words, {:.1}s total wall) ===", stats.len(), total_elapsed.as_secs_f64());
    eprintln!(
        "total/word (ms):    p50={:.3} p90={:.3} p99={:.3} max={:.3} mean={:.3}",
        ms(percentile(&totals, 0.50)),
        ms(percentile(&totals, 0.90)),
        ms(percentile(&totals, 0.99)),
        ms(*totals.last().unwrap_or(&0)),
        ms(totals.iter().sum::<u128>()) / stats.len().max(1) as f64
    );
    eprintln!(
        "propose/word (ms):  p50={:.3} p90={:.3} p99={:.3} max={:.3} mean={:.3}",
        ms(percentile(&proposes, 0.50)),
        ms(percentile(&proposes, 0.90)),
        ms(percentile(&proposes, 0.99)),
        ms(*proposes.last().unwrap_or(&0)),
        ms(proposes.iter().sum::<u128>()) / stats.len().max(1) as f64
    );
    eprintln!(
        "verify/word (ms):   p50={:.3} p90={:.3} p99={:.3} max={:.3} mean={:.3}",
        ms(percentile(&verifies, 0.50)),
        ms(percentile(&verifies, 0.90)),
        ms(percentile(&verifies, 0.99)),
        ms(*verifies.last().unwrap_or(&0)),
        ms(verifies.iter().sum::<u128>()) / stats.len().max(1) as f64
    );
    eprintln!(
        "candidates/word:    p50={} p90={} p99={} max={} mean={:.2}",
        percentile(&cand_counts, 0.50),
        percentile(&cand_counts, 0.90),
        percentile(&cand_counts, 0.99),
        cand_counts.last().unwrap_or(&0),
        cand_counts.iter().sum::<u128>() as f64 / stats.len().max(1) as f64
    );
    let sum_propose_ms: f64 = proposes.iter().map(|&n| ms(n)).sum();
    let sum_verify_ms: f64 = verifies.iter().map(|&n| ms(n)).sum();
    let sum_total_ms: f64 = totals.iter().map(|&n| ms(n)).sum();
    eprintln!(
        "aggregate share: propose={:.1}% verify={:.1}% (of summed per-word total; wall-clock incl. overhead={:.1}ms)",
        100.0 * sum_propose_ms / sum_total_ms.max(1e-9),
        100.0 * sum_verify_ms / sum_total_ms.max(1e-9),
        total_elapsed.as_secs_f64() * 1000.0
    );

    let n_timed_out = stats.iter().filter(|s| s.any_timed_out).count();
    eprintln!("words with a timed-out candidate: {n_timed_out}");

    let mut by_total: Vec<&WordStat> = stats.iter().collect();
    by_total.sort_by(|a, b| b.total_ns.cmp(&a.total_ns));
    eprintln!("\nTop 20 slowest words (idx, word, total_ms, propose_ms, verify_ms, n_candidates, n_verified, timed_out):");
    for s in by_total.iter().take(20) {
        eprintln!(
            "  {}\t{:?}\t{:.3}\t{:.3}\t{:.3}\t{}\t{}\t{}",
            s.idx, s.word, ms(s.total_ns), ms(s.propose_ns), ms(s.verify_ns), s.n_candidates, s.n_verified, s.any_timed_out
        );
    }
}

#[test]
#[ignore] // investigation-only, run explicitly with --ignored --nocapture
fn sena_probe() {
    run_probe("sena", "sena-hc.xml", "sena-words.txt");
}

#[test]
#[ignore]
fn amharic_probe() {
    run_probe("amharic", "amharic-hc.xml", "amharic-words.txt");
}

#[test]
#[ignore]
fn indonesian_probe() {
    run_probe("indonesian", "indonesian-hc.xml", "indonesian-words.txt");
}
