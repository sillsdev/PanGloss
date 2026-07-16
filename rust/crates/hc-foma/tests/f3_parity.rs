//! P3 gate 3a (docs/fst-plan/foma-fst-plan.md, "P3 — CLI wiring + full parity, conformance, and
//! timing gates (gate F3)"): corpus parity harness comparing the foma path
//! (`hc_foma::composite::FomaAnalyzer::analyze_word`) against the full engine
//! (`hc_parse::Morpher::parse_word_opts`, `ParseOptions::default()`) as MULTISETS keyed by
//! `(morpheme_ids sequence, root_morpheme_index)` — plan D7: "parity oracle = our own full engine
//! ... the property being tested is exactly 'the foma path loses nothing vs full search'."
//!
//! Denominators, per plan §P3 3a verbatim:
//! - Indonesian: all 121 corpus words — required 100%.
//! - Sena: sample-300 corpus (first 300 lines of `sena-words.txt`) — required 100%.
//! - Amharic: corpus words file (`amharic-words.txt`, all 673 lines) — required 100%, following
//!   `tests/f3_amharic_gate.rs`'s precedent for engine-timeout exclusions (a word where the FULL
//!   ENGINE itself times out with a PARTIAL result cannot be a parity baseline — the foma path's
//!   confirm is uncapped and can legitimately find analyses the timed-out full-search pass never
//!   reached; a word timing out with ZERO analyses is excluded outright, same as f3's own rule).
//!
//! No reduplication exclusion for Indonesian here (unlike the P1-stage `f2_indonesian_gate.rs`
//! recall-only gate): P2's `FomaAnalyzer` composite (propose UNION peel -> confirm) is exactly the
//! mechanism that closes the redup gap end-to-end (`tests/f4_composite_gate.rs` test (c) already
//! demonstrates all 7 redup words round-trip byte-for-byte) — this file's Indonesian test covers
//! the full, unfiltered 121-word corpus in one place as the P3 gate record.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hc_foma::composite::FomaAnalyzer;
use hc_grammar::model::Grammar;
use hc_parse::{Morpher, ParseOptions, WordAnalysis};

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_grammar(xml_name: &str) -> Grammar {
    let path = sample_path(xml_name);
    let xml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {xml_name}: {e}"))
}

fn read_words(name: &str) -> Vec<String> {
    let path = sample_path(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines().map(str::trim).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

fn morpheme_name(g: &Grammar, id: u32) -> String {
    match g.morphemes.get(id as usize) {
        Some(m) => format!("{}({}/{})", id, m.xml_key, m.gloss.as_deref().unwrap_or("-")),
        None => format!("{id}(?)"),
    }
}

fn seq_names(g: &Grammar, seq: &[u32]) -> String {
    seq.iter().map(|&id| morpheme_name(g, id)).collect::<Vec<_>>().join(", ")
}

/// The full multiset (duplicates preserved, sorted) — plan §2's parity unit: "(morpheme_ids
/// sequence, root_morpheme_index)".
fn multiset(structured: &[WordAnalysis]) -> Vec<(Vec<u32>, i32)> {
    let mut m: Vec<(Vec<u32>, i32)> =
        structured.iter().map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index)).collect();
    m.sort();
    m
}

/// One word where the foma path's multiset differed from the full engine's. Keyed by word +
/// both cardinalities so the KNOWN-FAILURES LEDGER below (`assert_against_ledger`) can pin the
/// exact shape of each open gap: a *new* mismatch, a mismatch whose counts *changed*, or a ledger
/// entry that *stopped* mismatching (i.e. got fixed) all fail the gate — the ledger tracks reality
/// exactly, it does not merely cap the failure count.
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

/// Assert `stats`'s mismatch set is EXACTLY `ledger`. `ledger` is the KNOWN-FAILURES record for a
/// grammar's open gate-F3 recall gaps: each `(word, engine_len, foma_len)` entry is a documented
/// bug (see the entry's own comment). Three ways to fail, all fatal:
///   * a mismatch NOT in the ledger — a NEW or REGRESSED parity gap;
///   * a ledger entry whose live cardinalities changed — the gap moved, re-triage it;
///   * a ledger entry that no longer mismatches — the bug is FIXED, so delete the entry (a fix
///     MUST shrink the ledger; that is how the ledger stays honest instead of drifting).
/// The plan doc keeps gate F3 recorded as NOT MET while this ledger is non-empty — the ledger is a
/// green-CI record of *known* gaps under active fix, never an acceptance of them.
fn assert_against_ledger(stats: &ParityStats, ledger: &[(&str, usize, usize)], label: &str) {
    let actual: BTreeSet<(String, usize, usize)> = stats
        .mismatches
        .iter()
        .map(|m| (m.word.clone(), m.engine_len, m.foma_len))
        .collect();
    let expected: BTreeSet<(String, usize, usize)> =
        ledger.iter().map(|&(w, e, f)| (w.to_string(), e, f)).collect();

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

/// Whether a word contributed a comparison, was excluded (engine timeout), or compared as a
/// legitimate zero-analysis non-parse (both sides empty) -- the Amharic test's diagnostic wants
/// this last case broken out without a second engine call.
enum CompareResult {
    Compared { zero_analyses: bool },
    ExcludedTimeout,
}

/// Compare one word's foma-path multiset against the full engine's. `morpher` may carry a
/// `--word-timeout-ms`-style deadline (Amharic); Indonesian/Sena pass an uncapped `Morpher` that
/// never times out in practice.
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
        // A partial full-search result cannot be a parity baseline (plan §P3 3a / f3 gate
        // precedent): foma's confirm is uncapped, so it can legitimately find MORE than a
        // timed-out full search did. Exclude rather than false-fail.
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
    CompareResult::Compared { zero_analyses: engine_ms.is_empty() }
}

// -------------------------------------------------------------------------------------------
// Indonesian: all 121 corpus words, no exclusions. 100% multiset parity required.
// -------------------------------------------------------------------------------------------

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "engine oracle is slow unoptimized; run in --release (where this test always runs) or with --ignored"
)]
fn indonesian_121_corpus_words_multiset_parity() {
    let g = load_grammar("indonesian-hc.xml");
    let mut analyzer = FomaAnalyzer::new(&g).expect("indonesian compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    let words = read_words("indonesian-words.txt");
    assert_eq!(words.len(), 121, "plan §P3 3a requires exactly the 121-word Indonesian corpus");

    let mut stats = ParityStats::new();
    for word in &words {
        let _ = compare_word(&g, &mut analyzer, &morpher, &opts, word, &mut stats);
    }
    stats.report("indonesian (121/121)");

    assert_eq!(stats.n_excluded, 0, "Indonesian corpus is not expected to time out the full engine");
    assert_eq!(stats.n_compared, 121, "every one of the 121 words must be compared");
    // Indonesian is at full multiset parity — the ledger is empty, and any mismatch is a hard fail.
    assert_against_ledger(&stats, &[], "indonesian (121/121)");
}

// -------------------------------------------------------------------------------------------
// Sena: sample-300 (first 300 corpus words). 100% multiset parity required.
// -------------------------------------------------------------------------------------------

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "engine oracle is slow unoptimized; run in --release (where this test always runs) or with --ignored"
)]
fn sena_sample_300_multiset_parity() {
    let g = load_grammar("sena-hc.xml");
    let mut analyzer = FomaAnalyzer::new(&g).expect("sena compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    let words: Vec<String> = read_words("sena-words.txt").into_iter().take(300).collect();
    assert_eq!(words.len(), 300, "plan §P3 3a requires a 300-word Sena sample");

    let mut stats = ParityStats::new();
    for word in &words {
        let _ = compare_word(&g, &mut analyzer, &morpher, &opts, word, &mut stats);
    }
    stats.report("sena (sample-300)");

    assert_eq!(stats.n_excluded, 0, "Sena sample-300 is not expected to time out the full engine");
    assert_eq!(stats.n_compared, 300, "every one of the 300 sample words must be compared");
    // KNOWN-FAILURES LEDGER (gate F3 NOT MET — foma-fst-plan.md "Gate F3 verdict"):
    //   `musandilesera` — engine finds 10 analyses, foma 2. Same morpheme_ids chain surfaces at
    //   multiple root_index values (2/3/4); the composite propose→confirm path (`composite.rs` /
    //   `confirm.rs`) collapses the positional/multiplicity variants the full search keeps. Owned
    //   by the recall-bug fix task; when fixed, delete this entry (the ledger must shrink).
    assert_against_ledger(&stats, &[("musandilesera", 10, 2)], "sena (sample-300)");
}

// -------------------------------------------------------------------------------------------
// Amharic: the full corpus words file (673 words). 100% multiset parity required on every word
// the full engine actually reaches a (possibly partial-free) result for, following
// `tests/f3_amharic_gate.rs`'s precedent for engine-timeout exclusions.
// -------------------------------------------------------------------------------------------

/// Per-word engine-oracle timeout, matching `tests/f3_amharic_gate.rs::ENGINE_TIMEOUT`.
const AMHARIC_ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "engine oracle is slow unoptimized; run in --release (where this test always runs) or with --ignored"
)]
fn amharic_corpus_words_multiset_parity() {
    let g = load_grammar("amharic-hc.xml");
    let mut analyzer = FomaAnalyzer::new(&g).expect("amharic compiles");
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(AMHARIC_ENGINE_TIMEOUT));
    let opts = ParseOptions::default();

    let words = read_words("amharic-words.txt");
    assert!(words.len() >= 673, "amharic-words.txt should have at least 673 lines, got {}", words.len());

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
    println!(
        "amharic: {n_zero_analysis_words} compared words had zero engine analyses (both sides \
         empty -- a legitimate non-parse, not a mismatch)"
    );

    assert!(stats.n_compared > 0, "parity gate must compare at least one word");
    // KNOWN-FAILURES LEDGER (gate F3): empty — the former `ገለፀ` interdigitation recall miss is
    //   FIXED (`preexpand.rs`'s `render_all_variants`: the composite emitter now renders every
    //   letter-series-merged spelling a probed Ge'ez glyph can honestly carry, not just the
    //   table-order-first one `hc_rules::surface_probe::render_nodes` returned, which had silently
    //   picked the wrong ጸ/ፀ series for root entry30 + -pfv- + pfv.3m). Amharic is now at full
    //   multiset parity — any mismatch is a hard fail.
    assert_against_ledger(&stats, &[], "amharic (full corpus)");
    if stats.n_excluded > 0 {
        println!(
            "NOTE: {} Amharic corpus word(s) excluded from parity because the full engine itself \
             timed out ({AMHARIC_ENGINE_TIMEOUT:?}/word) -- a timed-out full search cannot be a \
             parity baseline (plan D7); see tests/f3_amharic_gate.rs for the same policy applied \
             to the recall gate.",
            stats.n_excluded
        );
    }
}
