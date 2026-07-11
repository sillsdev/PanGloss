//! F6 gate (HYBRID_FST_RUST_PLAN.md §8, **THE HEADLINE**): "full composite candidate AND verified
//! parity, chain-off, Indonesian 121/121 byte-identical + Sena slice-60 + negatives goldens all `-`".
//!
//! Scope per this milestone's brief: Indonesian full corpus (121 words) and Sena's guarded slice-60
//! (NOT the full 7,121-word corpus -- F4's documented bare-walker performance gap on Sena's
//! pathological tail still applies to this composite, which includes that same walker). Amharic is
//! out of scope for this milestone (gated later, per plan §5.3, on the engine-parity subset).
//!
//! Candidate golden format: `{idx}\t{word}\t{proposer}\t{signature}`, one line per surviving
//! composite candidate, in COMPOSITE EMISSION ORDER (not sorted -- see `composite.rs`'s own doc).
//! Verified/batch golden format: the same `{STARTED}/{ok\t{sig}}` pair `f5_verify_gate.rs` already
//! exercises, sourced from the COMPOSITE instead of the bare walker.

use std::path::{Path, PathBuf};

use hc_grammar::model::Grammar;
use hc_hybrid::composite::{self, CompositeAnalyzer};
use hc_hybrid::replay;
use hc_hybrid::surface::SurfacePhonology;
use hc_hybrid::trie::Trie;
use hc_hybrid::walk;
use hc_parse::Morpher;

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn golden_path(grammar: &str, file_name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir
        .join("../../parity-out/golden/fst-advisor")
        .join(grammar)
        .join(file_name);
    path.exists().then_some(path)
}

fn words_path(grammar_dir: &str, name: &str) -> Option<PathBuf> {
    sample_path(name).or_else(|| golden_path(grammar_dir, name))
}

fn read_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .expect("read golden")
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn read_words(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .expect("read word list")
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

fn load_grammar(grammar_file: &str) -> Option<Grammar> {
    let gpath = sample_path(grammar_file)?;
    let xml = std::fs::read_to_string(&gpath).expect("read grammar");
    Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
}

fn build_trie(g: &Grammar, surface: &SurfacePhonology) -> Trie {
    let build_morpher = Morpher::new(g, usize::MAX);
    Trie::build(g, surface, &build_morpher, 1_000_000, 2, true)
}

/// Candidate-parity gate: byte-identical `{idx}\t{word}\t{proposer}\t{signature}` lines, in
/// composite emission order, against `<grammar_dir>/candidates-composite.tsv`.
fn run_candidate_gate(grammar_file: &str, words_file: &str, grammar_dir: &str, golden_file: &str) {
    let Some(wpath) = words_path(grammar_dir, words_file) else {
        eprintln!("skipping {words_file}: not present on disk");
        return;
    };
    let Some(gold_path) = golden_path(grammar_dir, golden_file) else {
        eprintln!("skipping {grammar_dir}/{golden_file}: golden not present on disk");
        return;
    };
    let Some(g) = load_grammar(grammar_file) else {
        eprintln!("skipping {grammar_file}: not present on disk");
        return;
    };

    let surface = SurfacePhonology::new(&g);
    let trie = build_trie(&g, &surface);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);

    let words = read_words(&wpath);
    let golden_lines = read_lines(&gold_path);

    let mut rust_lines = Vec::new();
    for (i, word) in words.iter().enumerate() {
        rust_lines.extend(composite::candidate_lines(&g, &composite, i, word));
    }

    assert_eq!(
        rust_lines.len(),
        golden_lines.len(),
        "{grammar_dir}/{golden_file}: line-count mismatch (golden {}, got {})",
        golden_lines.len(),
        rust_lines.len()
    );
    for (i, (rust_line, golden_line)) in rust_lines.iter().zip(golden_lines.iter()).enumerate() {
        assert_eq!(
            rust_line, golden_line,
            "{grammar_dir}/{golden_file}: diverges at line {i}"
        );
    }
}

/// Verified-composite-batch gate: propose (this composite) -> verify (`replay::confirm`) over every
/// word, byte-compared against `<grammar_dir>/<golden_file>` (the C# hybrid's default, chain-off
/// composite batch golden).
fn run_verified_gate(grammar_file: &str, words_file: &str, grammar_dir: &str, golden_file: &str) {
    let Some(wpath) = words_path(grammar_dir, words_file) else {
        eprintln!("skipping {words_file}: not present on disk");
        return;
    };
    let Some(gold_path) = golden_path(grammar_dir, golden_file) else {
        eprintln!("skipping {grammar_dir}/{golden_file}: golden not present on disk");
        return;
    };
    let Some(g) = load_grammar(grammar_file) else {
        eprintln!("skipping {grammar_file}: not present on disk");
        return;
    };

    let surface = SurfacePhonology::new(&g);
    let trie = build_trie(&g, &surface);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);

    let words = read_words(&wpath);
    let golden_lines = read_lines(&gold_path);

    let mut rust_lines = Vec::with_capacity(words.len() * 2);
    for (i, word) in words.iter().enumerate() {
        let [started, result] =
            composite::batch_lines(&g, &composite, &verify_morpher, &owners, i, word);
        rust_lines.push(started);
        rust_lines.push(result);
    }

    assert_eq!(
        rust_lines.len(),
        golden_lines.len(),
        "{grammar_dir}/{golden_file}: line-count mismatch (golden {}, got {})",
        golden_lines.len(),
        rust_lines.len()
    );
    for (i, (rust_line, golden_line)) in rust_lines.iter().zip(golden_lines.iter()).enumerate() {
        assert_eq!(
            rust_line, golden_line,
            "{grammar_dir}/{golden_file}: diverges at line {i}"
        );
    }
}

/// Soundness gate: every word in `negatives.txt` must verify to the empty set (`ok\t-`) through the
/// FULL composite (plan §3.4 -- "the 50-word near-miss battery must never produce a false
/// positive").
fn run_negatives_gate(grammar_file: &str, grammar_dir: &str) {
    let Some(npath) = golden_path(grammar_dir, "negatives.txt") else {
        eprintln!("skipping {grammar_dir}/negatives.txt: not present on disk");
        return;
    };
    let Some(g) = load_grammar(grammar_file) else {
        eprintln!("skipping {grammar_file}: not present on disk");
        return;
    };
    let surface = SurfacePhonology::new(&g);
    let trie = build_trie(&g, &surface);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);

    let words = read_words(&npath);
    assert!(
        !words.is_empty(),
        "{grammar_dir}/negatives.txt must be non-empty"
    );
    for word in &words {
        let verified = composite.analyze_word_verified(&verify_morpher, &owners, word);
        assert!(
            verified.is_empty(),
            "{grammar_dir}: negative example {word:?} must verify to zero analyses, got {} \
             (soundness violation -- a false positive)",
            verified.len()
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Indonesian (full 121-word corpus)
// ---------------------------------------------------------------------------------------------

#[test]
fn indonesian_composite_candidates_match_golden() {
    run_candidate_gate(
        "indonesian-hc.xml",
        "indonesian-words.txt",
        "indonesian",
        "candidates-composite.tsv",
    );
}

#[test]
fn indonesian_composite_verified_matches_chainoff_golden() {
    run_verified_gate(
        "indonesian-hc.xml",
        "indonesian-words.txt",
        "indonesian",
        "batch-chainoff.tsv",
    );
}

#[test]
fn indonesian_negatives_all_verify_empty() {
    run_negatives_gate("indonesian-hc.xml", "indonesian");
}

// ---------------------------------------------------------------------------------------------
// Sena (guarded slice-60 -- NOT the full corpus, see module doc)
// ---------------------------------------------------------------------------------------------

#[test]
fn sena_slice60_composite_candidates_match_golden() {
    run_candidate_gate(
        "sena-hc.xml",
        "slice-60.txt",
        "sena",
        "candidates-composite.tsv",
    );
}

#[test]
fn sena_slice60_composite_verified_matches_golden() {
    run_verified_gate("sena-hc.xml", "slice-60.txt", "sena", "slice-60-batch.tsv");
}

#[test]
fn sena_negatives_all_verify_empty() {
    run_negatives_gate("sena-hc.xml", "sena");
}
