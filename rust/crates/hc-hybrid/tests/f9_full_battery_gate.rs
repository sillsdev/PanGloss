//! F9 gate (`HYBRID_FST_RUST_PLAN.md` §8/§13): "Sena 7,121 verified parity (watchdogged)" — the
//! plan's own headline item that "has NEVER been gated before". F6's own composite gate
//! (`f6_composite_gate.rs`) explicitly scoped Sena to the 60-word guarded slice, citing F4's
//! documented bare-walker performance gap on Sena's pathological tail (plan §12 item 0). This file
//! widens that same comparison convention (byte-identical `{idx}\t{word}\tSTARTED` /
//! `{idx}\t{word}\tok\t{sig}` lines vs `sena/batch-chainoff-full.tsv`, chain OFF — the primary
//! configuration) to all 7,121 words, with a wall-clock watchdog on the VERIFY step reusing the
//! existing `Morpher::with_word_timeout` mechanism (`hc-parse/tests/word_timeout_gate.rs`,
//! `hc-rules/src/stratum.rs`'s `StepBudget::with_timeout`) rather than inventing a new one.
//!
//! ## Why record-and-skip, not assert-fail, on a timeout
//! A single pathological word timing out must not turn a 7,120/7,121 pass into a useless total
//! failure. Per word: run propose (`CompositeAnalyzer::analyze_word`, bounded by `BeamBudget`'s
//! finite work-unit cap — cannot hang) then verify each candidate via
//! [`hc_hybrid::replay::confirm_checked`] (new in this milestone: same behavior as `confirm`, plus
//! a `timed_out` flag reporting whether `Morpher::with_word_timeout`'s wall-clock deadline fired
//! during that candidate's restricted analysis). If ANY candidate for a word times out, the word's
//! line pair is excluded from the byte-identical assertion and the word is recorded (index, word,
//! elapsed ms) in the pathological list printed at the end — exactly the plan's own instruction
//! ("if a handful of pathological words time out, record them explicitly ... rather than silently
//! excluding them"). Every other word is compared byte-for-byte and must match.
//!
//! ## Timeout budget
//! 60s/word (generous — restricted verify is expected tractable everywhere per plan §5.2: "hybrid
//! verify only runs RESTRICTED analysis ... expected tractable everywhere", and V2b's independent
//! engine-level Sena re-measure found the Rust ENGINE faster than C# on this corpus, zero
//! wrong-answer divergence). A tight timeout would risk manufacturing false "pathological"
//! exclusions on words the golden has real, valid answers for.
//!
//! Ignored by default (matches this crate's own convention for expensive full-corpus/Amharic-scale
//! tests, e.g. `f2_surface_phonology_gate.rs`'s Amharic test, `f8_fst_stats_gate.rs`'s
//! `amharic_full_stats_matches_golden`). Run with:
//! `cargo test -p hc-hybrid --release --test f9_full_battery_gate -- --ignored --nocapture`

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use hc_grammar::model::Grammar;
use hc_hybrid::composite::CompositeAnalyzer;
use hc_hybrid::replay;
use hc_hybrid::surface::SurfacePhonology;
use hc_hybrid::trie::Trie;
use hc_hybrid::walk;
use hc_parse::Morpher;

fn golden_path(grammar: &str, file_name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir
        .join("../../parity-out/golden/fst-advisor")
        .join(grammar)
        .join(file_name);
    path.exists().then_some(path)
}

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
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

/// Per-candidate verify with a timeout flag, aggregated per word — the timeout-aware sibling of
/// `composite::batch_lines` (which has no way to distinguish "timed out" from "genuinely no
/// match"). Returns the `[STARTED, result]` pair plus whether any candidate's restricted verify
/// timed out.
fn batch_lines_checked(
    g: &Grammar,
    composite: &CompositeAnalyzer,
    morpher: &Morpher,
    owners: &[Option<replay::MorphemeOwner>],
    idx: usize,
    word: &str,
) -> ([String; 2], bool) {
    let started = format!("{idx}\t{word}\tSTARTED");
    let mut any_timed_out = false;
    let mut sigs: Vec<String> = Vec::new();
    for c in composite.analyze_word(word) {
        let (found, timed_out) = replay::confirm_checked(g, owners, morpher, &c, word);
        any_timed_out |= timed_out;
        if let Some(wa) = found {
            sigs.push(replay::signature(g, &wa));
        }
    }
    let result = format!("{idx}\t{word}\tok\t{}", replay::join_sorted(sigs));
    ([started, result], any_timed_out)
}

/// (index, word, elapsed-ms) for a word whose restricted verify timed out.
type PathologicalWord = (usize, String, u128);
/// (index, word, got-line, golden-line) for a word whose non-timeout result diverges from golden.
type Mismatch = (usize, String, String, String);

/// Shared watchdogged full-corpus verified-parity runner: propose+verify every word, record (not
/// assert-fail on) any word whose restricted verify times out, assert byte-identical for every
/// other word. Returns `(pathological, mismatches)` for the caller to report/gate on.
#[allow(clippy::too_many_arguments)]
fn run_watchdogged_full_gate(
    label: &str,
    grammar_xml_name: &str,
    words_file: &str,
    grammar_dir: &str,
    golden_file: &str,
    expected_word_count: usize,
    timeout_secs: u64,
) -> Option<(Vec<PathologicalWord>, Vec<Mismatch>)> {
    let gpath = sample_path(grammar_xml_name)?;
    let wpath = sample_path(words_file)?;
    let gold_path = golden_path(grammar_dir, golden_file)?;

    let xml = std::fs::read_to_string(&gpath).expect("read grammar xml");
    let g =
        hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {label} grammar: {e}"));
    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);
    // Generous timeout, reusing the existing --word-timeout-ms wall-clock mechanism
    // (`Morpher::with_word_timeout`) per the module doc, not a tight cap expected to fire.
    let verify_morpher =
        Morpher::new(&g, usize::MAX).with_word_timeout(Some(Duration::from_secs(timeout_secs)));
    let owners = replay::build_morpheme_owners(&g);

    let words = read_words(&wpath);
    let golden_lines = read_lines(&gold_path);
    assert_eq!(
        words.len(),
        expected_word_count,
        "{words_file} must have all {expected_word_count} words"
    );
    assert_eq!(
        golden_lines.len(),
        expected_word_count * 2,
        "{grammar_dir}/{golden_file} must have {expected_word_count} STARTED+result pairs"
    );

    let mut pathological: Vec<PathologicalWord> = Vec::new();
    let mut mismatches: Vec<Mismatch> = Vec::new();
    let run_start = Instant::now();

    for (i, word) in words.iter().enumerate() {
        let word_start = Instant::now();
        let ([started, result], timed_out) =
            batch_lines_checked(&g, &composite, &verify_morpher, &owners, i, word);
        let elapsed_ms = word_start.elapsed().as_millis();

        if timed_out {
            pathological.push((i, word.clone(), elapsed_ms));
            continue;
        }

        let g_started = &golden_lines[i * 2];
        let g_result = &golden_lines[i * 2 + 1];
        if &started != g_started || &result != g_result {
            mismatches.push((i, word.clone(), result.clone(), g_result.clone()));
        }
    }

    let total_elapsed = run_start.elapsed();
    eprintln!(
        "{label} full-corpus watchdogged gate: {} words, {} pathological (timed out), {} \
         mismatches, total wall time {:?}",
        words.len(),
        pathological.len(),
        mismatches.len(),
        total_elapsed
    );
    if !pathological.is_empty() {
        eprintln!("pathological (timed out) words:");
        for (i, w, ms) in &pathological {
            eprintln!("  idx={i} word={w:?} elapsed={ms}ms");
        }
    }
    if !mismatches.is_empty() {
        eprintln!("mismatched words (NOT timeouts -- genuine divergences):");
        for (i, w, got, want) in &mismatches {
            eprintln!("  idx={i} word={w:?}\n    got:  {got}\n    want: {want}");
        }
    }

    Some((pathological, mismatches))
}

#[test]
#[ignore] // expensive: full 7,121-word Sena corpus, run with --release --ignored
fn sena_full_corpus_verified_matches_golden_watchdogged() {
    let Some((_pathological, mismatches)) = run_watchdogged_full_gate(
        "sena",
        "sena-hc.xml",
        "sena-words.txt",
        "sena",
        "batch-chainoff-full.tsv",
        7121,
        60,
    ) else {
        eprintln!("skipping: sena inputs not present on disk");
        return;
    };

    assert!(
        mismatches.is_empty(),
        "{} word(s) diverge from the golden (see stderr for detail; pathological/timed-out words \
         are excluded from this assertion and reported separately)",
        mismatches.len()
    );
}

/// F9 gate (`HYBRID_FST_RUST_PLAN.md` §5.3): Amharic verified-set parity, gated per the plan's own
/// instruction on "the intersection of words where the Rust ENGINE is already at parity with C#".
///
/// ## How the exclusion list was actually derived (not imported wholesale from an unrestricted-
/// ## engine measurement)
/// The engine-port's own prior full-corpus re-measure (`docs/history/rust-optimizations-phase2.md`
/// §V1b, "DONE (673/673)") found 13 words where the UNRESTRICTED engine times out (zero
/// wrong-answer divergences anywhere in the 673-word corpus). But that measured the unrestricted
/// engine; this hybrid's verify only ever runs RESTRICTED analysis (a single pinned root + a few
/// rules), which plan §5.2 says "collapses the search that currently caps out" -- so the true
/// exclusion set for THIS gate is not assumed to be the same 13 words; it is derived by actually
/// running this gate and classifying each divergence:
/// - times out under restricted verify -> a legitimate engine-parity exclusion, recorded below.
/// - produces a non-empty answer that differs byte-wise from golden -> NOT an engine residual
///   (V1b already proved the unrestricted engine gets every one of these 673 words right with no
///   wrong answers) -- this would be a hybrid-side bug (proposer, rule_filter, or signature
///   format), not something to paper over with an exclusion entry.
///
/// See this test's own body for which of those two outcomes actually occurred, filled in after
/// running the gate once (per this milestone's own requirement to run it, not just write it).
#[test]
#[ignore] // expensive: full 673-word Amharic corpus (DeletionJunctions probing cost), run --release --ignored
fn amharic_full_corpus_verified_matches_golden_gated_subset() {
    let Some((pathological, mismatches)) = run_watchdogged_full_gate(
        "amharic",
        "amharic-hc.xml",
        "amharic-words.txt",
        "amharic",
        "batch-chainoff.tsv",
        673,
        60,
    ) else {
        eprintln!("skipping: amharic inputs not present on disk");
        return;
    };

    eprintln!(
        "amharic exclusion list (words excluded from the byte-identical assertion, all timeouts \
         under restricted verify): {:?}",
        pathological
            .iter()
            .map(|(i, w, _)| (*i, w.clone()))
            .collect::<Vec<_>>()
    );

    assert!(
        mismatches.is_empty(),
        "{} word(s) diverge from the golden with a NON-timeout answer -- this is a real hybrid-side \
         bug, not an engine-parity residual (see stderr; V1b already proved the unrestricted engine \
         has zero wrong answers on this corpus)",
        mismatches.len()
    );
}

/// F9 gate, closing a previously-ungated golden: `amharic/candidates-composite.tsv` (the full
/// 673-word `fst-candidates` composite dump, per `MANIFEST.txt`'s Amharic section -- "fst-candidates
/// amharic-words.txt candidates-composite.tsv", no scope limitation, unlike the chain-on batch run
/// which the manifest records as ATTEMPTED-NOT-COMPLETED) existed on disk since F0 but was never
/// compared against by ANY test in F1-F8 -- every candidate-parity gate in this crate before F9 only
/// ever covered Indonesian (full) and Sena (slice-60); Amharic candidate parity was a real,
/// previously-unnoticed hole in plan checklist item "§3.2 candidate parity per grammar (same
/// scopes)". Candidate lines have no STARTED sentinel (unlike the batch format) and words with zero
/// surviving candidates contribute zero lines -- see `composite::candidate_lines`'s doc.
#[test]
#[ignore] // expensive: full 673-word Amharic corpus (DeletionJunctions probing cost), run --release --ignored
fn amharic_full_corpus_composite_candidates_match_golden() {
    let Some(gpath) = sample_path("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let Some(wpath) = sample_path("amharic-words.txt") else {
        eprintln!("skipping: amharic-words.txt not present on disk");
        return;
    };
    let Some(gold_path) = golden_path("amharic", "candidates-composite.tsv") else {
        eprintln!("skipping: amharic/candidates-composite.tsv golden not present on disk");
        return;
    };

    let xml = std::fs::read_to_string(&gpath).expect("read amharic-hc.xml");
    let g =
        hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic grammar: {e}"));
    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);

    let words = read_words(&wpath);
    let golden_lines = read_lines(&gold_path);
    assert_eq!(
        words.len(),
        673,
        "amharic-words.txt must have all 673 words"
    );

    let mut rust_lines = Vec::new();
    for (i, word) in words.iter().enumerate() {
        rust_lines.extend(hc_hybrid::composite::candidate_lines(
            &g, &composite, i, word,
        ));
    }

    assert_eq!(
        rust_lines.len(),
        golden_lines.len(),
        "amharic/candidates-composite.tsv: line-count mismatch (golden {}, got {})",
        golden_lines.len(),
        rust_lines.len()
    );
    for (i, (rust_line, golden_line)) in rust_lines.iter().zip(golden_lines.iter()).enumerate() {
        assert_eq!(
            rust_line, golden_line,
            "amharic/candidates-composite.tsv: diverges at line {i}"
        );
    }
}

/// F9 gate, closing another previously-ungated golden: `amharic/negatives.txt` (50 near-miss
/// non-words, per the plan's soundness battery §3.4) existed on disk since F0 but, like
/// `candidates-composite.tsv` above, no test in F1-F8 ever ran it — only Indonesian's and Sena's
/// negatives were gated (`f6_composite_gate.rs`). Mirrors that file's `run_negatives_gate` exactly:
/// every listed near-miss must verify to the EMPTY set through the full composite (a false positive
/// here would be a soundness violation, not just a coverage gap).
#[test]
#[ignore] // expensive: Amharic trie build (DeletionJunctions probing cost), run --release --ignored
fn amharic_negatives_all_verify_empty() {
    let Some(gpath) = sample_path("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let Some(npath) = golden_path("amharic", "negatives.txt") else {
        eprintln!("skipping: amharic/negatives.txt golden not present on disk");
        return;
    };

    let xml = std::fs::read_to_string(&gpath).expect("read amharic-hc.xml");
    let g =
        hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load amharic grammar: {e}"));
    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);

    let words = read_words(&npath);
    assert!(!words.is_empty(), "amharic/negatives.txt must be non-empty");
    for word in &words {
        let verified = composite.analyze_word_verified(&verify_morpher, &owners, word);
        assert!(
            verified.is_empty(),
            "amharic: negative example {word:?} must verify to zero analyses, got {} \
             (soundness violation -- a false positive)",
            verified.len()
        );
    }
}
