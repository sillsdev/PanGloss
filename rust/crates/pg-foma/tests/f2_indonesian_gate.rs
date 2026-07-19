//! Phase P1 stage 2 gate (docs/fst-plan/foma-fst-plan.md §P1, gate F1, Indonesian leg): the
//! junction-aware emitter (`pg_foma::emit` + `pg_foma::junctions::PhonologyProbe`) against the real
//! Indonesian grammar (66 entries, 5 phonological rules: nasal-place assimilation of the `meN-`
//! prefix's placeholder nasal, plus voiceless-obstruent deletion at the resulting prefix/root
//! junction — `meN+tulis -> menulis`), with the FULL ENGINE (`pg_parse::Morpher`, a dev-dependency
//! only) as the recall oracle, exactly like `tests/f1_sena_gate.rs`'s Sena leg.
//!
//! Reduplication (7 corpus words: `membagi-bagi`, `memijit-mijit`, `meminta-minta`,
//! `mengamat-amati`, `mengayuh-ngayuh`, `menulis-nulis`, `menyewa-nyewa`) is explicitly OUT OF
//! SCOPE for this stage (plan D6 — the peel is P2's job); every rule that produces a reduplicated
//! form (`-Cont`, `-Pl`, `REDUP-meN`) already gets routed to `emit`'s `uncovered` list by the same
//! zone-mismatch logic stage 1 uses for every other exotic role, so these words simply have no
//! foma-proposed analysis at all. Test (b) excludes them from the recall denominator explicitly,
//! printing each with its reason, per the task's requirement — not because the engine has no
//! analysis for them (it does), but because this stage doesn't attempt to cover it.
//!
//! ## Test-timing policy (revised 2026-07-17)
//! The default local `cargo test --workspace --release` run must stay under ~60s and must not
//! depend on the gitignored real-language corpus fixtures (`samples/data/*`) at all. Every test in
//! this file loads `samples/data/indonesian-hc.xml`, so all four are unconditionally
//! `#[ignore = "..."]`d, each with a self-skip guard so `--include-ignored` runs stay green where
//! the fixture is absent (CI). Run the full set locally with
//! `cargo test -p pg-foma --release --test f2_indonesian_gate -- --include-ignored`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use pg_foma::analyzer::FomaProposer;
use pg_foma::emit;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

/// The 7 real reduplication corpus words (module doc) — excluded from the recall gate's
/// denominator, each with why.
const REDUP_EXCLUDED: &[(&str, &str)] = &[
    ("membagi-bagi", "reduplication — P2 peel"),
    ("memijit-mijit", "reduplication — P2 peel"),
    ("meminta-minta", "reduplication — P2 peel"),
    ("mengamat-amati", "reduplication — P2 peel"),
    ("mengayuh-ngayuh", "reduplication — P2 peel"),
    ("menulis-nulis", "reduplication — P2 peel"),
    ("menyewa-nyewa", "reduplication — P2 peel"),
];

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

/// Self-skip guard: gitignored real-corpus fixtures aren't present in a fresh clone or CI.
fn have(name: &str) -> bool {
    sample_path(name).exists()
}

fn load_indonesian() -> Grammar {
    let path = sample_path("indonesian-hc.xml");
    let xml = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load indonesian-hc.xml: {e}"))
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

fn candidates_cover(candidates: &[pg_foma::tags::Candidate], seq: &[u32], root_idx: i32) -> bool {
    candidates.iter().any(|c| {
        c.root_index == root_idx
            && c.morphemes.len() == seq.len()
            && c.morphemes.iter().zip(seq.iter()).all(|(m, s)| m.0 == *s)
    })
}

// -------------------------------------------------------------------------------------------
// (a) emit + compile: must succeed, counts plausible, compile wall time < 30s.
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn a_indonesian_emits_and_compiles() {
    if !have("indonesian-hc.xml") {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    }
    let g = load_indonesian();

    let t_emit = Instant::now();
    let emitted = emit::emit(&g);
    let emit_elapsed = t_emit.elapsed();

    assert!(
        emitted.report.counts.entries >= 66,
        "expected >= 66 entries, got {}",
        emitted.report.counts.entries
    );
    assert!(
        emitted.report.counts.rules >= 15,
        "expected >= 15 mrules, got {}",
        emitted.report.counts.rules
    );
    assert!(
        emitted.report.counts.lexc_lines > 0,
        "expected at least one lexc line"
    );
    assert!(
        !matches!(emitted.report.tier, emit::FomaTier::Unsupported { .. }),
        "Indonesian must not tier out: {:?}",
        emitted.report.tier
    );

    let t_compile = Instant::now();
    let proposer = FomaProposer::new(&g);
    let compile_elapsed = t_compile.elapsed();

    println!(
        "indonesian emit: {emit_elapsed:?}; emit+foma-compile: {compile_elapsed:?}; \
         lexc lines: {}; lexc bytes: {}; tier: {:?}; uncovered: {}",
        emitted.report.counts.lexc_lines,
        emitted.lexc_source.len(),
        emitted.report.tier,
        emitted.report.uncovered.len(),
    );
    for u in &emitted.report.uncovered {
        println!("  uncovered: [{}] {} — {}", u.kind, u.id, u.reason);
    }

    proposer.unwrap_or_else(|e| panic!("Indonesian lexc failed to foma-compile: {e}"));
    assert!(
        compile_elapsed.as_secs() < 30,
        "emit+compile took {compile_elapsed:?} (budget 30s; only meaningful in --release)"
    );
}

// -------------------------------------------------------------------------------------------
// (b) RECALL GATE: for every word in the Indonesian corpus (minus the 7 reduplication words,
//     explicitly excluded and printed), every engine (morpheme_ids, root_morpheme_index) pair must
//     appear among the proposer's candidates. 100% required on the non-redup denominator.
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn b_indonesian_recall_full_corpus_minus_redup() {
    if !have("indonesian-hc.xml") {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    }
    let g = load_indonesian();
    let mut proposer = FomaProposer::new(&g).expect("Indonesian compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    let words_path = sample_path("indonesian-words.txt");
    let words_text = std::fs::read_to_string(&words_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", words_path.display()));
    let words: Vec<&str> = words_text
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect();
    assert!(words.len() >= 121, "corpus has at least 121 words, got {}", words.len());

    println!("--- excluded (reduplication, P2 peel) ---");
    for (w, reason) in REDUP_EXCLUDED {
        println!("EXCLUDED {w:?}: {reason}");
    }
    let excluded: Vec<&str> = REDUP_EXCLUDED.iter().map(|&(w, _)| w).collect();

    let mut n_total = 0usize;
    let mut n_covered = 0usize;
    let mut misses: Vec<String> = Vec::new();
    let mut engine_time = std::time::Duration::ZERO;
    let mut propose_time = std::time::Duration::ZERO;
    let mut max_propose = std::time::Duration::ZERO;
    let mut n_words_analyzed = 0usize;

    for word in &words {
        if excluded.contains(word) {
            continue;
        }
        let t0 = Instant::now();
        let outcome = morpher.parse_word_opts(word, &opts);
        engine_time += t0.elapsed();
        if outcome.structured.is_empty() {
            continue;
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
                let names: Vec<String> = seq.iter().map(|&id| morpheme_name(&g, id)).collect();
                misses.push(format!(
                    "word {word:?}: engine analysis root_index={root_idx} morphemes=[{}]",
                    names.join(", ")
                ));
            }
        }
    }

    println!(
        "recall: {n_covered}/{n_total} engine analyses covered across {n_words_analyzed} analyzed \
         words (of {} corpus words, {} excluded)",
        words.len(),
        excluded.len(),
    );
    println!(
        "engine total: {engine_time:?}; propose total: {propose_time:?}; propose max/word: \
         {max_propose:?}; propose mean/word: {:?}",
        propose_time / (n_words_analyzed.max(1) as u32)
    );
    if !misses.is_empty() {
        println!("--- MISSES ({} of {n_total}) — the fix list ---", misses.len());
        for m in &misses {
            println!("MISS {m}");
        }
    }
    assert_eq!(
        n_covered, n_total,
        "recall gate: {} engine analyses not proposed (see MISS lines above)",
        n_total - n_covered
    );
}

// -------------------------------------------------------------------------------------------
// (c) junction spot-checks: N-assimilation without deletion, assimilation WITH root-initial
//     deletion, a suffixed form, and a plain unprefixed root.
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn c_junction_spot_checks() {
    if !have("indonesian-hc.xml") {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    }
    let g = load_indonesian();
    let mut proposer = FomaProposer::new(&g).expect("Indonesian compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    // (word, description)
    let cases = [
        ("membaca", "N-assimilation without deletion (meN+baca -> membaca, b retained)"),
        ("menulis", "N-assimilation WITH root-initial deletion (meN+tulis -> menulis, t deleted)"),
        ("memukul", "N-assimilation WITH root-initial deletion (meN+pukul -> memukul, p deleted)"),
        ("mengkhawatirkan", "meN- + root + -kan suffix"),
        ("tulis", "plain unprefixed root"),
    ];

    for (word, desc) in cases {
        let outcome = morpher.parse_word_opts(word, &opts);
        assert!(
            !outcome.structured.is_empty(),
            "{word:?} ({desc}): engine finds no analysis at all -- test case is wrong"
        );
        let seqs = engine_sequences(&outcome);
        let candidates = proposer.propose(word);
        println!(
            "{word:?} ({desc}): engine {} distinct sequence(s), proposer {} candidate(s)",
            seqs.len(),
            candidates.len()
        );
        for (seq, root_idx) in &seqs {
            let names: Vec<String> = seq.iter().map(|&id| morpheme_name(&g, id)).collect();
            let hit = candidates_cover(&candidates, seq, *root_idx);
            println!(
                "  engine seq root_index={root_idx} morphemes=[{}] -> {}",
                names.join(", "),
                if hit { "COVERED" } else { "MISSING" }
            );
            assert!(
                hit,
                "{word:?} ({desc}): engine sequence root_index={root_idx} morphemes=[{}] not \
                 among proposer candidates",
                names.join(", ")
            );
        }
    }
}

// -------------------------------------------------------------------------------------------
// (d) overgeneration sanity: a nonsense word must not panic and must propose nothing (or very
//     little).
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn d_nonsense_word_proposes_nothing() {
    if !have("indonesian-hc.xml") {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    }
    let g = load_indonesian();
    let mut proposer = FomaProposer::new(&g).expect("Indonesian compiles");
    let t0 = Instant::now();
    let candidates = proposer.propose("zzzq");
    println!(
        "zzzq: {} candidates in {:?}",
        candidates.len(),
        t0.elapsed()
    );
    assert!(
        candidates.len() <= 5,
        "nonsense word should propose (almost) nothing, got {} candidates",
        candidates.len()
    );
}
