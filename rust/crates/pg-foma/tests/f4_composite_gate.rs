//! Phase P2 gate (docs/fst-plan/foma-fst-plan.md §P2, gate F2): the propose→confirm composite
//! (`pg_foma::composite::FomaAnalyzer`) against the real Sena and Indonesian grammars, with the FULL
//! ENGINE (`pg_parse::Morpher::parse_word_opts`, now a normal — not dev-only — dependency as of P2)
//! as the parity oracle (plan D7: "our own full engine, because confirm IS our engine").
//!
//! (a) over-generation pruned: `mbali` (Sena) has far more proposed candidates than the engine has
//!     analyses; `analyze_word` must return exactly the engine's analyses, not the candidate count.
//! (b) MULTIPLICITY (plan D4): `mbali`'s full 8-analysis multiset (with duplicates) must round-trip
//!     exactly, both as `(morpheme_ids, root_morpheme_index)` tuples and as the display-string pairs.
//! (c) REDUP (plan D6): the 7 Indonesian corpus reduplication words round-trip against the engine.
//! (d) no-analysis word returns empty, never panics; consistency with the engine under the SAME
//!     `ParseOptions::default()` (no guessing) is checked directly against the Sena corpus.
//! (e) MINI-PARITY smoke: first 40 Sena corpus words + every non-redup Indonesian corpus word,
//!     100% multiset parity required; per-word timings reported.
//!
//! ## Test-timing policy
//! The default local `cargo test --workspace --release` run must stay under ~60s and must not
//! depend on the gitignored real-language corpus fixtures (`samples/data/*`) at all. Every test in
//! this file loads a real Sena and/or Indonesian grammar, so all five are unconditionally
//! `#[ignore = "..."]`d (replacing the old `cfg_attr(debug_assertions, ...)` debug-only ignore on
//! (d)/(e)), each with a self-skip guard so `--include-ignored` runs stay green where the fixture
//! is absent (CI). Run the full set locally with
//! `cargo test -p pg-foma --release --test f4_composite_gate -- --include-ignored`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

const REDUP_WORDS: &[&str] = &[
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

/// Self-skip guard: gitignored real-corpus fixtures aren't present in a fresh clone or CI.
fn have(name: &str) -> bool {
    sample_path(name).exists()
}

fn load_sena() -> Grammar {
    let path = sample_path("sena-hc.xml");
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load sena-hc.xml: {e}"))
}

fn load_indonesian() -> Grammar {
    let path = sample_path("indonesian-hc.xml");
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load indonesian-hc.xml: {e}"))
}

fn read_words(name: &str) -> Vec<String> {
    let path = sample_path(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Multiset key for a `WordAnalysis`: `(morpheme_ids, root_morpheme_index)`, sorted — comparing two
/// sorted `Vec`s IS multiset comparison (duplicates preserved, order-independent).
fn structured_multiset(v: &[WordAnalysis]) -> Vec<(Vec<u32>, i32)> {
    let mut m: Vec<(Vec<u32>, i32)> = v
        .iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect();
    m.sort();
    m
}

fn analyses_multiset(v: &[(String, String)]) -> Vec<(String, String)> {
    let mut m = v.to_vec();
    m.sort();
    m
}

// -------------------------------------------------------------------------------------------
// (a) over-generation pruned: mbali's candidate count vastly exceeds the engine's analysis
//     count; analyze_word must return exactly the engine's analyses, not the candidate count.
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn a_overgeneration_pruned_mbali() {
    if !have("sena-hc.xml") {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    }
    let g = load_sena();
    let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    let engine = morpher.parse_word_opts("mbali", &opts);
    assert!(
        !engine.structured.is_empty(),
        "engine finds analyses for mbali"
    );

    let outcome = analyzer.analyze_word("mbali");
    println!(
        "mbali: candidates_generated={} confirmed={} engine_analyses={}",
        outcome.candidates_generated,
        outcome.confirmed,
        engine.structured.len()
    );
    assert!(
        outcome.candidates_generated > engine.structured.len(),
        "expected over-generation (candidates_generated={} should exceed engine analyses={}) to \
         demonstrate confirm actually prunes something, not just pass through 1:1",
        outcome.candidates_generated,
        engine.structured.len()
    );
    assert_eq!(
        outcome.confirmed,
        engine.structured.len(),
        "confirmed count must equal the engine's exact analysis count, not the candidate count"
    );
    assert_eq!(
        structured_multiset(&outcome.structured),
        structured_multiset(&engine.structured)
    );
}

// -------------------------------------------------------------------------------------------
// (b) MULTIPLICITY (plan D4): mbali's full multiset (duplicates included) round-trips exactly,
//     both as (morpheme_ids, root_index) tuples and as the analyses-string pairs.
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn b_mbali_multiplicity_matches_full_engine() {
    if !have("sena-hc.xml") {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    }
    let g = load_sena();
    let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    let engine = morpher.parse_word_opts("mbali", &opts);
    let outcome = analyzer.analyze_word("mbali");

    println!(
        "mbali: engine multiset size {}, foma multiset size {}",
        engine.structured.len(),
        outcome.structured.len()
    );
    assert_eq!(
        outcome.structured.len(),
        engine.structured.len(),
        "mbali: multiset SIZE (with multiplicity) must match exactly"
    );
    assert_eq!(
        structured_multiset(&outcome.structured),
        structured_multiset(&engine.structured),
        "mbali: (morpheme_ids, root_morpheme_index) multisets must match exactly"
    );
    assert_eq!(
        analyses_multiset(&outcome.analyses),
        analyses_multiset(&engine.analyses),
        "mbali: (morpheme-join, surface) string-pair multisets must match exactly"
    );
}

// -------------------------------------------------------------------------------------------
// (c) REDUP (plan D6): the 7 Indonesian corpus reduplication words round-trip against the engine.
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn c_indonesian_redup_words_round_trip() {
    if !have("indonesian-hc.xml") {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    }
    let g = load_indonesian();
    let mut analyzer = FomaAnalyzer::new(&g).expect("indonesian compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    for word in REDUP_WORDS {
        let engine = morpher.parse_word_opts(word, &opts);
        assert!(
            !engine.structured.is_empty(),
            "{word:?}: engine finds no analysis at all -- test word is wrong"
        );
        let outcome = analyzer.analyze_word(word);
        println!(
            "{word:?}: peel_used={} candidates_generated={} confirmed={} engine={}",
            outcome.peel_used,
            outcome.candidates_generated,
            outcome.confirmed,
            engine.structured.len()
        );
        assert!(
            outcome.peel_used,
            "{word:?}: expected the redup peel to fire for this word"
        );
        assert_eq!(
            structured_multiset(&outcome.structured),
            structured_multiset(&engine.structured),
            "{word:?}: structured multiset mismatch"
        );
        assert_eq!(
            analyses_multiset(&outcome.analyses),
            analyses_multiset(&engine.analyses),
            "{word:?}: analyses-string multiset mismatch"
        );
    }
}

// -------------------------------------------------------------------------------------------
// (d) no-analysis word returns empty, never panics; consistency with the engine under the SAME
//     ParseOptions::default() (no guessing) — scanned against a bounded prefix of the Sena corpus.
// -------------------------------------------------------------------------------------------

/// How many Sena corpus words test (d) scans through the engine oracle. The full corpus is 7,121
/// words at roughly 0.3 s/word of engine search in release — over half an hour for one gate test —
/// so, like the f1 gate's bounded recall scan, we take a fixed prefix. 400 words yields a healthy
/// crop of genuine zero-analysis engine misses to cross-check.
const D_SCAN_WORDS: usize = 400;

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn d_no_analysis_word_returns_empty_consistent_with_engine() {
    if !have("sena-hc.xml") {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    }
    let g = load_sena();
    let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    // A plain nonsense word: never panics, empty outcome.
    let outcome = analyzer.analyze_word("zzzqxxxnonsense");
    assert!(outcome.structured.is_empty());
    assert!(outcome.analyses.is_empty());
    assert_eq!(outcome.confirmed, 0);
    assert_eq!(outcome.candidates_generated, 0);

    // A guess_root-style miss: `ParseOptions::default()` never guesses (guess_root = false), so any
    // corpus word the engine fails to analyze under these SAME options is exactly that case --
    // `analyze_word` must be empty for it too, never inventing a guess-only result of its own (it
    // never opts into guessing anywhere in its own pipeline).
    let words = read_words("sena-words.txt");
    let mut n_checked_misses = 0usize;
    for word in words.iter().take(D_SCAN_WORDS) {
        let engine = morpher.parse_word_opts(word, &opts);
        if engine.structured.is_empty() {
            n_checked_misses += 1;
            let outcome = analyzer.analyze_word(word);
            assert!(
                outcome.structured.is_empty(),
                "{word:?}: engine has zero analyses under ParseOptions::default(), but \
                 analyze_word returned {} -- the foma path must not invent a guess-only result",
                outcome.structured.len()
            );
        }
    }
    println!(
        "checked {n_checked_misses} of the first {D_SCAN_WORDS} corpus words with zero \
         default-opts engine analyses; all confirmed empty via the foma path too"
    );
}

// -------------------------------------------------------------------------------------------
// (e) MINI-PARITY smoke: first 40 Sena corpus words + every non-redup Indonesian corpus word.
//     100% multiset parity required. Per-word timings reported (mean/max/total).
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/{sena,indonesian}-hc.xml); run with --include-ignored"]
fn e_mini_parity_sena_40_and_indonesian_non_redup() {
    if !have("sena-hc.xml") || !have("indonesian-hc.xml") {
        eprintln!("skipping: sena-hc.xml/indonesian-hc.xml not present on disk");
        return;
    }
    let opts = ParseOptions::default();
    let mut mismatches: Vec<String> = Vec::new();

    // --- Sena: first 40 corpus words -----------------------------------------------------------
    let g_sena = load_sena();
    let mut analyzer_sena = FomaAnalyzer::new(&g_sena).expect("sena compiles");
    let morpher_sena = Morpher::new(&g_sena, usize::MAX);
    let sena_words: Vec<String> = read_words("sena-words.txt").into_iter().take(40).collect();
    assert_eq!(
        sena_words.len(),
        40,
        "expected at least 40 Sena corpus words"
    );

    let mut sena_timings: Vec<Duration> = Vec::new();
    let mut sena_total = Duration::ZERO;
    let mut sena_engine_timings: Vec<Duration> = Vec::new();
    let mut sena_engine_total = Duration::ZERO;
    let mut sena_ok = 0usize;
    for word in &sena_words {
        let te0 = Instant::now();
        let engine = morpher_sena.parse_word_opts(word, &opts);
        let dte = te0.elapsed();
        sena_engine_timings.push(dte);
        sena_engine_total += dte;

        let t0 = Instant::now();
        let outcome = analyzer_sena.analyze_word(word);
        let dt = t0.elapsed();
        sena_timings.push(dt);
        sena_total += dt;
        let got = structured_multiset(&outcome.structured);
        let want = structured_multiset(&engine.structured);
        if got == want {
            sena_ok += 1;
        } else {
            mismatches.push(format!(
                "sena {word:?}: foma {} analyses, engine {} analyses",
                got.len(),
                want.len()
            ));
        }
    }
    let sena_mean = sena_total / (sena_words.len() as u32);
    let sena_max = sena_timings.iter().max().copied().unwrap_or_default();
    let sena_engine_mean = sena_engine_total / (sena_words.len() as u32);
    let sena_engine_max = sena_engine_timings
        .iter()
        .max()
        .copied()
        .unwrap_or_default();
    println!(
        "sena mini-parity: {sena_ok}/{} words match; per-word FOMA (propose+peel+confirm) \
         mean={sena_mean:?} max={sena_max:?} total={sena_total:?}; per-word ENGINE \
         (parse_word_opts, full search) mean={sena_engine_mean:?} max={sena_engine_max:?} \
         total={sena_engine_total:?}",
        sena_words.len()
    );

    // --- Indonesian: every non-redup corpus word ------------------------------------------------
    let g_indo = load_indonesian();
    let mut analyzer_indo = FomaAnalyzer::new(&g_indo).expect("indonesian compiles");
    let morpher_indo = Morpher::new(&g_indo, usize::MAX);
    let indo_words: Vec<String> = read_words("indonesian-words.txt")
        .into_iter()
        .filter(|w| !REDUP_WORDS.contains(&w.as_str()))
        .collect();
    assert!(
        indo_words.len() >= 100,
        "expected most of the 121-word Indonesian corpus after excluding 7 redup words, got {}",
        indo_words.len()
    );

    let mut indo_timings: Vec<Duration> = Vec::new();
    let mut indo_total = Duration::ZERO;
    let mut indo_engine_timings: Vec<Duration> = Vec::new();
    let mut indo_engine_total = Duration::ZERO;
    let mut indo_ok = 0usize;
    for word in &indo_words {
        let te0 = Instant::now();
        let engine = morpher_indo.parse_word_opts(word, &opts);
        let dte = te0.elapsed();
        indo_engine_timings.push(dte);
        indo_engine_total += dte;

        let t0 = Instant::now();
        let outcome = analyzer_indo.analyze_word(word);
        let dt = t0.elapsed();
        indo_timings.push(dt);
        indo_total += dt;
        let got = structured_multiset(&outcome.structured);
        let want = structured_multiset(&engine.structured);
        if got == want {
            indo_ok += 1;
        } else {
            mismatches.push(format!(
                "indonesian {word:?}: foma {} analyses, engine {} analyses",
                got.len(),
                want.len()
            ));
        }
    }
    let indo_mean = indo_total / (indo_words.len().max(1) as u32);
    let indo_max = indo_timings.iter().max().copied().unwrap_or_default();
    let indo_engine_mean = indo_engine_total / (indo_words.len().max(1) as u32);
    let indo_engine_max = indo_engine_timings
        .iter()
        .max()
        .copied()
        .unwrap_or_default();
    println!(
        "indonesian mini-parity: {indo_ok}/{} words match; per-word FOMA (propose+peel+confirm) \
         mean={indo_mean:?} max={indo_max:?} total={indo_total:?}; per-word ENGINE \
         (parse_word_opts, full search) mean={indo_engine_mean:?} max={indo_engine_max:?} \
         total={indo_engine_total:?}",
        indo_words.len()
    );

    if !mismatches.is_empty() {
        println!("--- MISMATCHES ({}) ---", mismatches.len());
        for m in &mismatches {
            println!("MISMATCH {m}");
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mini-parity mismatches (100% required, see MISMATCH lines above)",
        mismatches.len()
    );
}
