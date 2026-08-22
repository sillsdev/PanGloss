//! Large-lexicon proposer-recall gate against the real Sena grammar, with the full engine (`pg_parse::Morpher`) as the recall oracle: every true engine analysis must appear among the proposer's candidates, since under-generation is a silently lost analysis while over-generation is harmless (confirm prunes it). Corpus-blocked (needs gitignored `samples/data/sena-*`), so every test here is `#[ignore]`d unconditionally with a self-skip guard; run with `--include-ignored`.

use std::path::PathBuf;
use std::time::Instant;

use pg_foma::analyzer::FomaProposer;
use pg_foma::emit;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn sample_path(name: &str) -> PathBuf {
    pg_conformance_fixtures::corpus::path(name)
        .unwrap_or_else(|| pg_conformance_fixtures::corpus::corpus_root().join(name))
}

/// Self-skip guard: returns early rather than panicking when the gitignored corpus fixture is absent, so `--include-ignored` runs stay green without it.
fn have(name: &str) -> bool {
    sample_path(name).exists()
}

fn load_grammar() -> Grammar {
    let path = sample_path("sena-hc.xml");
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load sena-hc.xml: {e}"))
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

// (a) emit + compile: must succeed, counts plausible, compile wall time < 30s.

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn a_emits_and_compiles() {
    if !have("sena-hc.xml") {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    }
    let g = load_grammar();

    let t_emit = Instant::now();
    let emitted = emit::emit(&g);
    let emit_elapsed = t_emit.elapsed();

    // Plausibility: every entry and rule must be accounted for, and lexc must carry at least one line per lexical entry.
    assert!(
        emitted.report.counts.entries >= 1369,
        "expected >= 1369 entries, got {}",
        emitted.report.counts.entries
    );
    assert!(
        emitted.report.counts.rules >= 132,
        "expected >= 132 mrules, got {}",
        emitted.report.counts.rules
    );
    assert!(
        emitted.report.counts.lexc_lines >= 1369,
        "expected at least one lexc line per entry, got {}",
        emitted.report.counts.lexc_lines
    );
    assert!(
        !matches!(emitted.report.tier, emit::FomaTier::Unsupported { .. }),
        "Sena must not tier out: {:?}",
        emitted.report.tier
    );

    let t_compile = Instant::now();
    let proposer = FomaProposer::new(&g);
    let compile_elapsed = t_compile.elapsed();

    println!(
        "sena emit: {emit_elapsed:?}; emit+foma-compile: {compile_elapsed:?}; \
         lexc lines: {}; lexc bytes: {}; tier: {:?}; uncovered: {}",
        emitted.report.counts.lexc_lines,
        emitted.lexc_source.len(),
        emitted.report.tier,
        emitted.report.uncovered.len(),
    );
    for u in &emitted.report.uncovered {
        println!("  uncovered: [{}] {} — {}", u.kind, u.id, u.reason);
    }

    proposer.unwrap_or_else(|e| panic!("Sena lexc failed to foma-compile: {e}"));
    assert!(
        compile_elapsed.as_secs() < 30,
        "emit+compile took {compile_elapsed:?} (budget 30s; only meaningful in --release)"
    );
}

// (b) recall gate: for the first 120 corpus words with engine analyses, every engine (morpheme_ids, root_morpheme_index) pair must appear among the proposer's candidates.

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn b_recall_first_120_words() {
    if !have("sena-hc.xml") {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    }
    let g = load_grammar();
    let mut proposer = FomaProposer::new(&g).expect("Sena compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    let words_path = sample_path("sena-words.txt");
    let words_text = std::fs::read_to_string(&words_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", words_path.display()));
    let words: Vec<&str> = words_text
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .take(120)
        .collect();
    assert!(words.len() == 120, "corpus has at least 120 words");

    let mut n_total = 0usize; // engine analyses on words with any analysis
    let mut n_covered = 0usize;
    let mut misses: Vec<String> = Vec::new();
    let mut engine_time = std::time::Duration::ZERO;
    let mut propose_time = std::time::Duration::ZERO;
    let mut max_propose = std::time::Duration::ZERO;
    let mut n_words_analyzed = 0usize;

    for word in &words {
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

        // Distinct engine sequences: the engine returns a multiset, so the recall property here is per distinct sequence, not multiplicity.
        let mut engine_seqs: Vec<(Vec<u32>, i32)> = Vec::new();
        for a in &outcome.structured {
            let key = (a.morpheme_ids.clone(), a.root_morpheme_index);
            if !engine_seqs.contains(&key) {
                engine_seqs.push(key);
            }
        }

        for (seq, root_idx) in &engine_seqs {
            n_total += 1;
            let hit = candidates.iter().any(|c| {
                c.root_index == *root_idx
                    && c.morphemes.len() == seq.len()
                    && c.morphemes.iter().zip(seq.iter()).all(|(m, s)| m.0 == *s)
            });
            if hit {
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
         words (of {} corpus words)",
        words.len()
    );
    println!(
        "engine total: {engine_time:?}; propose total: {propose_time:?}; propose max/word: \
         {max_propose:?}; propose mean/word: {:?}",
        propose_time / (n_words_analyzed.max(1) as u32)
    );
    if !misses.is_empty() {
        println!(
            "--- MISSES ({} of {n_total}) — the fix list ---",
            misses.len()
        );
        for m in &misses {
            println!("MISS {m}");
        }
    }
    pg_conformance_fixtures::corpus::record_cases("sena_recall_first_120_words", words.len());
    assert_eq!(
        n_covered,
        n_total,
        "recall gate: {} engine analyses not proposed (see MISS lines above)",
        n_total - n_covered
    );
}

// (c) mbali: the proposer must offer both of the engine's distinct analysis sequences.

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn c_mbali_covers_both_engine_sequences() {
    if !have("sena-hc.xml") {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    }
    let g = load_grammar();
    let mut proposer = FomaProposer::new(&g).expect("Sena compiles");
    let morpher = Morpher::new(&g, usize::MAX);

    let outcome = morpher.parse_word_opts("mbali", &ParseOptions::default());
    assert!(
        !outcome.structured.is_empty(),
        "engine finds analyses for mbali"
    );
    let mut engine_seqs: Vec<(Vec<u32>, i32)> = Vec::new();
    for a in &outcome.structured {
        let key = (a.morpheme_ids.clone(), a.root_morpheme_index);
        if !engine_seqs.contains(&key) {
            engine_seqs.push(key);
        }
    }
    println!(
        "mbali: engine multiset size {}, distinct sequences {}",
        outcome.structured.len(),
        engine_seqs.len()
    );

    let candidates = proposer.propose("mbali");
    println!("mbali: proposer offers {} candidates", candidates.len());
    for (seq, root_idx) in &engine_seqs {
        let names: Vec<String> = seq.iter().map(|&id| morpheme_name(&g, id)).collect();
        let hit = candidates.iter().any(|c| {
            c.root_index == *root_idx
                && c.morphemes.len() == seq.len()
                && c.morphemes.iter().zip(seq.iter()).all(|(m, s)| m.0 == *s)
        });
        assert!(
            hit,
            "mbali: engine sequence root_index={root_idx} morphemes=[{}] not among proposer \
             candidates",
            names.join(", ")
        );
    }
}

// (d) overgeneration sanity: a nonsense word must not panic and must propose nothing (or very little).

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn d_nonsense_word_proposes_nothing() {
    if !have("sena-hc.xml") {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    }
    let g = load_grammar();
    let mut proposer = FomaProposer::new(&g).expect("Sena compiles");
    let t0 = Instant::now();
    let candidates = proposer.propose("zzzq");
    println!(
        "zzzq: {} candidates in {:?}",
        candidates.len(),
        t0.elapsed()
    );
    assert!(
        candidates.len() <= 3,
        "nonsense word should propose (almost) nothing, got {} candidates",
        candidates.len()
    );
}
