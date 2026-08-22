//! Corpus multiset-parity harness comparing the foma path against the full engine; see
//! `docs/research/pg-foma-f3-parity.md` for the denominators, ledger discipline, and timing policy.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions, WordAnalysis};

fn sample_path(name: &str) -> PathBuf {
    pg_conformance_fixtures::corpus::path(name)
        .unwrap_or_else(|| pg_conformance_fixtures::corpus::corpus_root().join(name))
}

/// Self-skip guard: gitignored real-corpus fixtures aren't present in a fresh clone or CI.
fn have(name: &str) -> bool {
    sample_path(name).exists()
}

fn load_grammar(xml_name: &str) -> Grammar {
    let path = sample_path(xml_name);
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {xml_name}: {e}"))
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

fn morpheme_name(g: &Grammar, id: u32) -> String {
    match g.morphemes.get(id as usize) {
        Some(m) => format!(
            "{}({}/{})",
            id,
            m.xml_key,
            m.gloss.as_deref().unwrap_or("-")
        ),
        None => format!("{id}(?)"),
    }
}

fn seq_names(g: &Grammar, seq: &[u32]) -> String {
    seq.iter()
        .map(|&id| morpheme_name(g, id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The full multiset (duplicates preserved, sorted), keyed by `(morpheme_ids sequence, root_morpheme_index)`.
fn multiset(structured: &[WordAnalysis]) -> Vec<(Vec<u32>, i32)> {
    let mut m: Vec<(Vec<u32>, i32)> = structured
        .iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect();
    m.sort();
    m
}

/// One word where the foma path's multiset differed from the full engine's; keyed by word + both cardinalities so the ledger tracks reality exactly, not merely a failure count.
struct Mismatch {
    word: String,
    engine_len: usize,
    foma_len: usize,
    detail: String,
}

struct ParityStats {
    n_compared: usize,
    n_excluded: usize,
    mismatches: Vec<Mismatch>,
    foma_time: Duration,
    engine_time: Duration,
    foma_max: Duration,
    engine_max: Duration,
}

impl ParityStats {
    fn new() -> Self {
        ParityStats {
            n_compared: 0,
            n_excluded: 0,
            mismatches: Vec::new(),
            foma_time: Duration::ZERO,
            engine_time: Duration::ZERO,
            foma_max: Duration::ZERO,
            engine_max: Duration::ZERO,
        }
    }

    fn report(&self, label: &str) {
        println!(
            "{label}: {} words compared ({} excluded); foma total={:?} mean={:?} max={:?}; \
             engine total={:?} mean={:?} max={:?}",
            self.n_compared,
            self.n_excluded,
            self.foma_time,
            self.foma_time / (self.n_compared.max(1) as u32),
            self.foma_max,
            self.engine_time,
            self.engine_time / (self.n_compared.max(1) as u32),
            self.engine_max,
        );
        if !self.mismatches.is_empty() {
            println!("--- MISMATCHES ({}) ---", self.mismatches.len());
            for m in &self.mismatches {
                println!("MISMATCH {}", m.detail);
            }
        }
    }
}

/// Asserts `stats`'s mismatch set is exactly `ledger`: a new/regressed mismatch, a changed cardinality, or a no-longer-mismatching (fixed) entry are all fatal, so the ledger tracks reality exactly.
fn assert_against_ledger(stats: &ParityStats, ledger: &[(&str, usize, usize)], label: &str) {
    let actual: BTreeSet<(String, usize, usize)> = stats
        .mismatches
        .iter()
        .map(|m| (m.word.clone(), m.engine_len, m.foma_len))
        .collect();
    let expected: BTreeSet<(String, usize, usize)> = ledger
        .iter()
        .map(|&(w, e, f)| (w.to_string(), e, f))
        .collect();

    let unexpected: Vec<_> = actual.difference(&expected).collect();
    let fixed: Vec<_> = expected.difference(&actual).collect();

    assert!(
        unexpected.is_empty(),
        "{label}: {} UNEXPECTED parity mismatch(es) beyond the known-failures ledger — a new or \
         regressed gate-F3 recall gap: {unexpected:?}\n(see MISMATCH lines above for the full \
         engine-vs-foma breakdown)",
        unexpected.len()
    );
    assert!(
        fixed.is_empty(),
        "{label}: {} known-failures-ledger entr(y/ies) no longer mismatch — the gap is FIXED, so \
         DELETE the entry from this test's ledger (a fix must shrink the ledger): {fixed:?}",
        fixed.len()
    );
}

/// Whether a word contributed a comparison, was excluded (engine timeout), or was a legitimate zero-analysis non-parse.
enum CompareResult {
    Compared { zero_analyses: bool },
    ExcludedTimeout,
}

/// Compares one word's foma-path multiset against the full engine's; `morpher` may carry a per-word timeout (Amharic) or be uncapped (Indonesian/Sena).
fn compare_word(
    g: &Grammar,
    analyzer: &mut FomaAnalyzer,
    morpher: &Morpher,
    opts: &ParseOptions,
    word: &str,
    stats: &mut ParityStats,
) -> CompareResult {
    let t_engine = Instant::now();
    let engine_outcome = morpher.parse_word_opts(word, opts);
    let dt_engine = t_engine.elapsed();

    if engine_outcome.timed_out {
        // A partial full-search result cannot be a parity baseline: foma's confirm is uncapped and can legitimately find more than a timed-out full search did.
        stats.n_excluded += 1;
        return CompareResult::ExcludedTimeout;
    }

    stats.engine_time += dt_engine;
    stats.engine_max = stats.engine_max.max(dt_engine);

    let t_foma = Instant::now();
    let foma_outcome = analyzer.analyze_word(word);
    let dt_foma = t_foma.elapsed();
    stats.foma_time += dt_foma;
    stats.foma_max = stats.foma_max.max(dt_foma);
    stats.n_compared += 1;

    let engine_ms = multiset(&engine_outcome.structured);
    let foma_ms = multiset(&foma_outcome.structured);
    if engine_ms != foma_ms {
        let fmt = |ms: &[(Vec<u32>, i32)]| {
            ms.iter()
                .map(|(seq, ri)| format!("[{}]@{ri}", seq_names(g, seq)))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        stats.mismatches.push(Mismatch {
            word: word.to_string(),
            engine_len: engine_ms.len(),
            foma_len: foma_ms.len(),
            detail: format!(
                "word {word:?}: engine {} analyses vs foma {} —\n  engine: {}\n  foma:   {}",
                engine_ms.len(),
                foma_ms.len(),
                fmt(&engine_ms),
                fmt(&foma_ms),
            ),
        });
    }
    CompareResult::Compared {
        zero_analyses: engine_ms.is_empty(),
    }
}

// Indonesian: all 121 corpus words, no exclusions, 100% multiset parity required.

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn indonesian_121_corpus_words_multiset_parity() {
    if !have("indonesian-hc.xml") {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    }
    let g = load_grammar("indonesian-hc.xml");
    let mut analyzer = FomaAnalyzer::new(&g).expect("indonesian compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    let words = read_words("indonesian-words.txt");
    assert_eq!(
        words.len(),
        121,
        "the reference corpus must contain exactly 121 Indonesian words"
    );

    let mut stats = ParityStats::new();
    for word in &words {
        let _ = compare_word(&g, &mut analyzer, &morpher, &opts, word, &mut stats);
    }
    stats.report("indonesian (121/121)");
    pg_conformance_fixtures::corpus::record_cases(
        "indonesian_121_corpus_words_multiset_parity",
        words.len(),
    );

    assert_eq!(
        stats.n_excluded, 0,
        "Indonesian corpus is not expected to time out the full engine"
    );
    assert_eq!(
        stats.n_compared, 121,
        "every one of the 121 words must be compared"
    );
    // Indonesian is at full multiset parity — the ledger is empty, and any mismatch is a hard fail.
    assert_against_ledger(&stats, &[], "indonesian (121/121)");
}

// Sena: sample-300 (first 300 corpus words), 100% multiset parity required.

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_sample_300_multiset_parity() {
    if !have("sena-hc.xml") {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    }
    let g = load_grammar("sena-hc.xml");
    let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    let words: Vec<String> = read_words("sena-words.txt").into_iter().take(300).collect();
    assert_eq!(
        words.len(),
        300,
        "the reference parity sample must contain 300 Sena words"
    );

    let mut stats = ParityStats::new();
    for word in &words {
        let _ = compare_word(&g, &mut analyzer, &morpher, &opts, word, &mut stats);
    }
    stats.report("sena (sample-300)");
    pg_conformance_fixtures::corpus::record_cases("sena_sample_300_multiset_parity", words.len());

    assert_eq!(
        stats.n_excluded, 0,
        "Sena sample-300 is not expected to time out the full engine"
    );
    assert_eq!(
        stats.n_compared, 300,
        "every one of the 300 sample words must be compared"
    );
    // KNOWN-FAILURES LEDGER (gate F3): empty (see docs/research/pg-foma-f3-parity.md); any mismatch is a hard fail.
    assert_against_ledger(&stats, &[], "sena (sample-300)");
}

// Amharic: full 673-word corpus, 100% multiset parity required on every word the engine reaches a result for.

/// Per-word engine-oracle timeout.
const AMHARIC_ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs the full 673-word corpus on the test harness's own thread; see `docs/research/pg-foma-f3-parity.md` for the stack-size hardening rationale.
fn amharic_corpus_words_multiset_parity_impl() {
    let g = load_grammar("amharic-hc.xml");
    let mut analyzer = FomaAnalyzer::new(&g).expect("amharic compiles");
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(AMHARIC_ENGINE_TIMEOUT));
    let opts = ParseOptions::default();

    let words = read_words("amharic-words.txt");
    assert!(
        words.len() >= 673,
        "amharic-words.txt should have at least 673 lines, got {}",
        words.len()
    );

    let mut stats = ParityStats::new();
    let mut n_zero_analysis_words = 0usize;
    for word in &words {
        if let CompareResult::Compared { zero_analyses } =
            compare_word(&g, &mut analyzer, &morpher, &opts, word, &mut stats)
        {
            if zero_analyses {
                n_zero_analysis_words += 1;
            }
        }
    }
    stats.report("amharic (full corpus)");
    pg_conformance_fixtures::corpus::record_cases(
        "amharic_corpus_words_multiset_parity",
        words.len(),
    );
    println!(
        "amharic: {n_zero_analysis_words} compared words had zero engine analyses (both sides \
         empty -- a legitimate non-parse, not a mismatch)"
    );

    assert!(
        stats.n_compared > 0,
        "parity gate must compare at least one word"
    );
    // KNOWN-FAILURES LEDGER (gate F3): empty (see docs/research/pg-foma-f3-parity.md); any mismatch is a hard fail.
    assert_against_ledger(&stats, &[], "amharic (full corpus)");
    if stats.n_excluded > 0 {
        println!(
            "NOTE: {} Amharic corpus word(s) excluded from parity because the full engine itself \
             timed out ({AMHARIC_ENGINE_TIMEOUT:?}/word) -- a timed-out full search cannot be a \
             parity baseline; see tests/f3_interdigitation_gate.rs for the same policy applied \
             to the recall gate.",
            stats.n_excluded
        );
    }
}

/// Stack size matching `pg-cli`'s main-thread worker and `hc_parse_batch`'s rayon workers (same `Morpher::parse_word_selected` recursion class), not `pg-foma`'s own smaller `PROBE_STACK_BYTES` (sized for a shallower foma-side probe).
const AMHARIC_PARITY_STACK_BYTES: usize = 1 << 30; // 1 GiB

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_corpus_words_multiset_parity() {
    if !have("amharic-hc.xml") {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    }
    std::thread::Builder::new()
        .stack_size(AMHARIC_PARITY_STACK_BYTES)
        .spawn(amharic_corpus_words_multiset_parity_impl)
        .expect("spawn amharic_corpus_words_multiset_parity worker thread")
        .join()
        .expect("amharic_corpus_words_multiset_parity worker thread panicked");
}
