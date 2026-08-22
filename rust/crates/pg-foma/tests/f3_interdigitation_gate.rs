//! Recall gate for interdigitation (infix) and Ge'ez boundary-fusion composites (`crate::preexpand`) against the real Amharic grammar, with `pg_parse::Morpher` as the recall oracle; corpus-blocked, `#[ignore]`d unconditionally, and no synthetic replacement exists for this grammar's pathology.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use pg_foma::analyzer::FomaProposer;
use pg_foma::composite::FomaAnalyzer;
use pg_foma::emit;
use pg_foma::peel::ReduplicationPeeler;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

/// Vendored foma's recursive lexc parser overflows libtest's default Windows thread stack on this gate's large real grammar.
const FOMA_LEXC_STACK_BYTES: usize = 512 * 1024 * 1024;

/// First 100 corpus words (`amharic-words.txt` has 673 lines).
const WORD_CAP: usize = 100;

/// Per-word engine-oracle timeout: zero-analysis timeouts are skipped from recall, partial-analysis timeouts count for recall but are excluded from the end-to-end multiset parity test.
const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

fn sample_path(name: &str) -> PathBuf {
    pg_conformance_fixtures::corpus::path(name)
        .unwrap_or_else(|| pg_conformance_fixtures::corpus::corpus_root().join(name))
}

/// Self-skip guard: gitignored real-corpus fixtures aren't present in a fresh clone or CI.
fn have(name: &str) -> bool {
    sample_path(name).exists()
}
fn run_on_foma_lexc_stack(test: fn()) {
    std::thread::Builder::new()
        .stack_size(FOMA_LEXC_STACK_BYTES)
        .spawn(test)
        .expect("spawn large-stack foma lexc worker thread")
        .join()
        .expect("foma lexc worker thread panicked");
}

fn load_grammar() -> Grammar {
    let path = sample_path("amharic-hc.xml");
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic-hc.xml: {e}"))
}

fn corpus_words() -> Vec<String> {
    let words_path = sample_path("amharic-words.txt");
    let words_text = std::fs::read_to_string(&words_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", words_path.display()));
    let words: Vec<String> = words_text
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .take(WORD_CAP)
        .map(str::to_string)
        .collect();
    assert_eq!(
        words.len(),
        WORD_CAP,
        "corpus has at least {WORD_CAP} words"
    );
    words
}

fn morpheme_name(g: &Grammar, id: u32) -> String {
    match g.morphemes.get(id as usize) {
        Some(m) => {
            let gloss = m.gloss.as_deref().unwrap_or("-");
            format!("{}({}/{})", id, m.xml_key, gloss)
        }
        None => format!("{id}(?)"),
    }
}

fn seq_names(g: &Grammar, seq: &[u32]) -> String {
    seq.iter()
        .map(|&id| morpheme_name(g, id))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Distinct `(morpheme_ids, root_morpheme_index)` sequences in an engine outcome (recall's unit).
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

/// The FULL multiset (duplicates preserved, sorted for comparison) — the end-to-end parity unit.
fn multiset(structured: &[pg_parse::WordAnalysis]) -> Vec<(Vec<u32>, i32)> {
    let mut m: Vec<(Vec<u32>, i32)> = structured
        .iter()
        .map(|a| (a.morpheme_ids.clone(), a.root_morpheme_index))
        .collect();
    m.sort();
    m
}

fn candidates_cover(candidates: &[pg_foma::tags::Candidate], seq: &[u32], root_idx: i32) -> bool {
    candidates.iter().any(|c| {
        c.root_index == root_idx
            && c.morphemes.len() == seq.len()
            && c.morphemes.iter().zip(seq.iter()).all(|(m, s)| m.0 == *s)
    })
}

// (a) emit + compile: counts, uncovered (the infix items must be GONE), tier, wall time.

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn a_emits_and_compiles() {
    if !have("amharic-hc.xml") {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    }
    run_on_foma_lexc_stack(a_emits_and_compiles_impl);
}

fn a_emits_and_compiles_impl() {
    let g = load_grammar();

    let t_emit = Instant::now();
    let emitted = emit::emit(&g);
    let emit_elapsed = t_emit.elapsed();

    assert!(
        emitted.report.counts.entries >= 76,
        "expected >= 76 entries, got {}",
        emitted.report.counts.entries
    );
    assert!(
        emitted.report.counts.rules >= 87,
        "expected >= 87 mrules, got {}",
        emitted.report.counts.rules
    );
    assert!(emitted.report.counts.lexc_lines > 0);
    assert!(
        !matches!(emitted.report.tier, emit::FomaTier::Unsupported { .. }),
        "Amharic failed to emit at all — got Unsupported: {:?}",
        emitted.report.tier
    );

    // Both composite mechanisms fired, and the Role::Infix rules are gone from `uncovered`.
    assert!(
        emitted.report.counts.composite_interdigitation_entries > 0,
        "expected interdigitation composites (Amharic has 3 Role::Infix rules matching 36 roots)"
    );
    assert!(
        emitted.report.counts.composite_fusion_entries > 0,
        "expected boundary-fusion composites (Ge'ez coalescence)"
    );
    assert!(
        emitted.report.counts.composite_pairs_probed > 0,
        "composite pair probing must have run"
    );
    let infix_uncovered: Vec<_> = emitted
        .report
        .uncovered
        .iter()
        .filter(|u| u.kind == "infix")
        .collect();
    assert!(
        infix_uncovered.is_empty(),
        "P1d requires the infix items GONE from uncovered; still present: {infix_uncovered:?}"
    );

    println!(
        "amharic emit: {emit_elapsed:?}; lexc lines: {}; lexc bytes: {}; tier: {:?}; uncovered: {}",
        emitted.report.counts.lexc_lines,
        emitted.lexc_source.len(),
        emitted.report.tier,
        emitted.report.uncovered.len(),
    );
    println!(
        "counts: entries={} rules={} slots={} groups={} allomorphs_emitted={} allomorphs_skipped={}",
        emitted.report.counts.entries,
        emitted.report.counts.rules,
        emitted.report.counts.slots,
        emitted.report.counts.groups,
        emitted.report.counts.allomorphs_emitted,
        emitted.report.counts.allomorphs_skipped,
    );
    println!(
        "composites: pairs_probed={} interdigitation_entries={} fusion_entries={}",
        emitted.report.counts.composite_pairs_probed,
        emitted.report.counts.composite_interdigitation_entries,
        emitted.report.counts.composite_fusion_entries,
    );
    for u in &emitted.report.uncovered {
        println!("  uncovered: [{}] {} — {}", u.kind, u.id, u.reason);
    }

    let t_compile = Instant::now();
    let proposer = FomaProposer::new(&g);
    let compile_elapsed = t_compile.elapsed();
    println!(
        "amharic emit+foma-compile (fresh emit inside FomaProposer::new): {compile_elapsed:?} \
         (soft expectation < 60s; if this regresses far past that, the preexpand chain probing is \
         the first place to profile)"
    );
    proposer.unwrap_or_else(|e| panic!("Amharic lexc failed to foma-compile: {e}"));
}

// (b) recall — asserted 100%: a miss is a compiler bug, not a fallback-tier trigger.

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn b_recall_first_100_words_is_100_percent() {
    if !have("amharic-hc.xml") {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    }
    run_on_foma_lexc_stack(b_recall_first_100_words_is_100_percent_impl);
}

fn b_recall_first_100_words_is_100_percent_impl() {
    let g = load_grammar();
    assert!(
        !ReduplicationPeeler::new(&g).has_redup_rules(),
        "Amharic was verified to have zero Role::Reduplication rules; if this grammar changed, \
         re-derive the denominator exclusion rule (plan P1d gate item b)"
    );
    let mut proposer = FomaProposer::new(&g).expect("Amharic compiles");
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(ENGINE_TIMEOUT));
    let opts = ParseOptions::default();

    let words = corpus_words();

    let mut n_total = 0usize;
    let mut n_covered = 0usize;
    let mut misses: Vec<String> = Vec::new();
    let mut n_words_analyzed = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    let mut n_partial_timeout = 0usize;
    let mut engine_time = Duration::ZERO;
    let mut propose_time = Duration::ZERO;
    let mut max_propose = Duration::ZERO;

    for word in &words {
        let t0 = Instant::now();
        let outcome = morpher.parse_word_opts(word, &opts);
        engine_time += t0.elapsed();

        if outcome.structured.is_empty() {
            if outcome.timed_out {
                skipped.push(format!(
                    "{word:?}: engine timed out ({ENGINE_TIMEOUT:?}) with zero analyses"
                ));
            }
            continue;
        }
        if outcome.timed_out {
            n_partial_timeout += 1;
        }
        n_words_analyzed += 1;

        let t1 = Instant::now();
        let candidates = proposer.propose(word);
        let dt = t1.elapsed();
        propose_time += dt;
        max_propose = max_propose.max(dt);

        for (seq, root_idx) in engine_sequences(&outcome) {
            n_total += 1;
            if candidates_cover(&candidates, &seq, root_idx) {
                n_covered += 1;
            } else {
                misses.push(format!(
                    "word {word:?}: engine analysis root_index={root_idx} morphemes=[{}]",
                    seq_names(&g, &seq)
                ));
            }
        }
    }

    for s in &skipped {
        println!("SKIPPED {s}");
    }
    println!(
        "recall: {n_covered}/{n_total} engine analyses covered across {n_words_analyzed} analyzed \
         words (of {WORD_CAP} scanned; {} skipped on zero-analysis timeout; {n_partial_timeout} \
         analyzed words had partial/timed-out engine results)",
        skipped.len()
    );
    println!(
        "engine total: {engine_time:?}; propose total: {propose_time:?}; propose max/word: \
         {max_propose:?}; propose mean/word: {:?}",
        propose_time / (n_words_analyzed.max(1) as u32)
    );
    if !misses.is_empty() {
        println!(
            "--- MISSES ({} of {n_total}) — every one is a P1d bug ---",
            misses.len()
        );
        for m in &misses {
            println!("MISS {m}");
        }
    }
    pg_conformance_fixtures::corpus::record_cases(
        "amharic_recall_first_100_words",
        n_words_analyzed,
    );
    assert!(
        n_total > 0,
        "recall gate must exercise at least one engine analysis"
    );
    assert_eq!(
        n_covered,
        n_total,
        "P1d requires 100% Amharic proposer recall (plan §0: no fallback tier); {} engine \
         analyses not proposed — see MISS lines above",
        n_total - n_covered
    );
}

// (c) end-to-end: FomaAnalyzer::analyze_word multiset == engine parse_word_opts multiset, stronger than proposer recall since it exercises confirm's positional matching and multiplicity recovery.

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn c_end_to_end_multiset_parity() {
    if !have("amharic-hc.xml") {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    }
    run_on_foma_lexc_stack(c_end_to_end_multiset_parity_impl);
}

fn c_end_to_end_multiset_parity_impl() {
    let g = load_grammar();
    let mut analyzer = FomaAnalyzer::new(&g).expect("Amharic compiles");
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(ENGINE_TIMEOUT));
    let opts = ParseOptions::default();

    let words = corpus_words();

    let mut n_words_compared = 0usize;
    let mut n_excluded_timeout = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    let mut analyze_time = Duration::ZERO;
    let mut max_analyze = Duration::ZERO;

    for word in &words {
        let outcome = morpher.parse_word_opts(word, &opts);
        if outcome.timed_out {
            // Partial full-search results cannot be a parity baseline (foma confirm is uncapped).
            n_excluded_timeout += 1;
            continue;
        }
        if outcome.structured.is_empty() {
            continue; // same denominator rule as (b): engine-analyzed words only.
        }
        n_words_compared += 1;

        let t0 = Instant::now();
        let foma_outcome = analyzer.analyze_word(word);
        let dt = t0.elapsed();
        analyze_time += dt;
        max_analyze = max_analyze.max(dt);

        let engine_ms = multiset(&outcome.structured);
        let foma_ms = multiset(&foma_outcome.structured);
        if engine_ms != foma_ms {
            let fmt = |ms: &[(Vec<u32>, i32)]| {
                ms.iter()
                    .map(|(seq, ri)| format!("[{}]@{ri}", seq_names(&g, seq)))
                    .collect::<Vec<_>>()
                    .join(" | ")
            };
            mismatches.push(format!(
                "word {word:?}: engine {} analyses vs foma {} —\n  engine: {}\n  foma:   {}",
                engine_ms.len(),
                foma_ms.len(),
                fmt(&engine_ms),
                fmt(&foma_ms),
            ));
        }
    }

    println!(
        "end-to-end parity: {n_words_compared} words compared ({n_excluded_timeout} excluded on \
         engine timeout); analyze_word total: {analyze_time:?}; max/word: {max_analyze:?}; \
         mean/word: {:?}",
        analyze_time / (n_words_compared.max(1) as u32)
    );
    if !mismatches.is_empty() {
        println!("--- MULTISET MISMATCHES ({}) ---", mismatches.len());
        for m in &mismatches {
            println!("MISMATCH {m}");
        }
    }
    assert!(
        n_words_compared > 0,
        "parity gate must compare at least one word"
    );
    assert!(
        mismatches.is_empty(),
        "{} word(s) with foma-path multiset != full-engine multiset (see MISMATCH lines above)",
        mismatches.len()
    );
}

// (d) overgeneration sanity + no panic on nonsense words, through both the raw proposer and the full analyzer.

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn d_nonsense_word_proposes_boundedly_and_never_panics() {
    if !have("amharic-hc.xml") {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    }
    run_on_foma_lexc_stack(d_nonsense_word_proposes_boundedly_and_never_panics_impl);
}

fn d_nonsense_word_proposes_boundedly_and_never_panics_impl() {
    let g = load_grammar();
    let mut proposer = FomaProposer::new(&g).expect("Amharic compiles");
    let t0 = Instant::now();
    let candidates = proposer.propose("ዝጎጠቃኝዬ");
    println!(
        "ዝጎጠቃኝዬ: {} candidates in {:?}",
        candidates.len(),
        t0.elapsed()
    );
    assert!(
        candidates.len() <= 20,
        "nonsense word should propose boundedly few candidates, got {}",
        candidates.len()
    );

    let candidates2 = proposer.propose("zzzq");
    assert!(
        candidates2.is_empty(),
        "unsegmentable word should propose nothing"
    );

    // Through the full composite (propose -> confirm): nonsense must confirm to zero analyses.
    let mut analyzer = FomaAnalyzer::new(&g).expect("Amharic compiles");
    let outcome = analyzer.analyze_word("ዝጎጠቃኝዬ");
    assert!(
        outcome.structured.is_empty(),
        "nonsense word must not confirm ({} confirmed)",
        outcome.confirmed
    );
    let outcome2 = analyzer.analyze_word("zzzq");
    assert!(outcome2.structured.is_empty());
}
