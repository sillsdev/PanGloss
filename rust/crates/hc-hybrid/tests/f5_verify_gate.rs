//! F5 gate (HYBRID_FST_RUST_PLAN.md §8): "verified bare-FST parity on Indonesian + the Sena
//! slice-60 file vs the F0 `--bare` batch goldens; thread-invariance asserted; soundness assert
//! (every emitted analysis re-confirms) run once on Indonesian."
//!
//! Reproduces `fst-batch --bare`'s exact per-word dump (`FstBatchCommand.cs`, `fst-oracle` branch):
//! `{idx}\t{word}\tSTARTED` followed by `{idx}\t{word}\tok\t{sig}` (status is unconditionally `"ok"`
//! on the `--bare` path -- see `hc_hybrid::replay::batch_lines`'s doc for why `SKIPPED` never fires
//! here, empirically confirmed by the Indonesian golden's own idx-118 `write-CONTpijit` line:
//! `ok\t-`, not `SKIPPED`).
//!
//! Unlike F4's `--bare` CANDIDATES gate (pre-verify, no `FstReplay` involved at all), this gate
//! exercises the full propose (F4's bare walker) -> verify (F5's `confirm`) loop against a
//! `fst-batch --bare` golden -- the first milestone whose Rust output is compared to a VERIFIED
//! golden rather than a raw-candidate one (see `f4_bare_walker_gate.rs`'s own module doc, which
//! flags `sena/slice-60-batch-bare.tsv` as exactly this milestone's golden, not F4's).

use std::path::{Path, PathBuf};

use hc_grammar::model::{Grammar, MorphemeId};
use hc_hybrid::replay::{self, VerifiedFstAnalyzer};
use hc_hybrid::surface::SurfacePhonology;
use hc_hybrid::trie::Trie;
use hc_hybrid::walk::{self, WordAnalysis as Candidate};
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

/// C# `File.ReadAllLines(...).Select(w => w.Trim()).Where(w => w.Length > 0)`.
fn read_words(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .expect("read word list")
        .lines()
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Build the F3 trie (bare proposer) exactly as F4's own gate does.
fn build_trie(g: &Grammar) -> Trie {
    let build_morpher = Morpher::new(g, usize::MAX);
    let surface = SurfacePhonology::new(g);
    Trie::build(g, &surface, &build_morpher, 1_000_000, 2, true)
}

/// Load a grammar by file name, returning `None` (with a diagnostic) rather than panicking when
/// the sample data isn't present on disk -- same convention as every sibling gate file in this
/// crate (F2/F3/F4).
fn load_grammar(grammar_file: &str) -> Option<Grammar> {
    let gpath = sample_path(grammar_file)?;
    let xml = std::fs::read_to_string(&gpath).expect("read grammar");
    Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
}

/// Verified-bare-batch gate: propose (F4's bare walker) -> verify (F5's `confirm`) over every word
/// in `words_file`, and byte-compare the `{STARTED, ok\t{sig}}` line pairs against
/// `golden_dir/golden_file_name`. `confirm`'s own verify `Morpher` is built UNCAPPED
/// (`Morpher::new(g, usize::MAX)`) -- see `replay::confirm`'s doc for why a cap would risk
/// silently dropping a golden-covered result.
fn run_gate(grammar_file: &str, words_file: &str, golden_dir: &str, golden_file_name: &str) {
    let Some(wpath) = words_path(golden_dir, words_file) else {
        eprintln!("skipping {words_file}: not present on disk");
        return;
    };
    let Some(gold_path) = golden_path(golden_dir, golden_file_name) else {
        eprintln!("skipping {golden_dir}/{golden_file_name}: golden not present on disk");
        return;
    };
    let Some(g) = load_grammar(grammar_file) else {
        eprintln!("skipping {grammar_file}: not present on disk");
        return;
    };

    let trie = build_trie(&g);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let analyzer =
        VerifiedFstAnalyzer::new(&g, &trie, &verify_morpher, walk::DEFAULT_MAX_BEAM_WORK);

    let words = read_words(&wpath);
    let golden_lines = read_lines(&gold_path);

    let mut rust_lines = Vec::with_capacity(words.len() * 2);
    for (i, word) in words.iter().enumerate() {
        let [started, result] = replay::batch_lines(&g, &analyzer, i, word);
        rust_lines.push(started);
        rust_lines.push(result);
    }

    assert_eq!(
        rust_lines.len(),
        golden_lines.len(),
        "{golden_dir}/{golden_file_name}: line-count mismatch (golden {}, got {})",
        golden_lines.len(),
        rust_lines.len()
    );
    for (i, (rust_line, golden_line)) in rust_lines.iter().zip(golden_lines.iter()).enumerate() {
        assert_eq!(
            rust_line, golden_line,
            "{golden_dir}/{golden_file_name}: diverges at line {i}"
        );
    }
}

#[test]
fn indonesian_verified_bare_matches_batch_bare_golden_full_corpus() {
    run_gate(
        "indonesian-hc.xml",
        "indonesian-words.txt",
        "indonesian",
        "batch-bare.tsv",
    );
}

#[test]
fn sena_verified_bare_matches_slice_60_golden() {
    run_gate(
        "sena-hc.xml",
        "slice-60.txt",
        "sena",
        "slice-60-batch-bare.tsv",
    );
}

/// Thread-invariance (plan §4.2: "assert `--threads=1` equals `--threads=N` once, then run
/// parallel"). Runs a word list through [`replay::verify_words_parallel`] at `threads=1`
/// (sequential, no rayon pool spun up at all -- see that function's doc) and a higher thread
/// count, asserting byte-identical output in original word order (independent of scheduling/
/// completion order across the pool).
fn assert_thread_invariant(
    grammar_file: &str,
    words_file: &str,
    golden_dir: &str,
    high_threads: usize,
) {
    let Some(wpath) = words_path(golden_dir, words_file) else {
        eprintln!("skipping {words_file}: not present on disk");
        return;
    };
    let Some(g) = load_grammar(grammar_file) else {
        eprintln!("skipping {grammar_file}: not present on disk");
        return;
    };
    let trie = build_trie(&g);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let analyzer =
        VerifiedFstAnalyzer::new(&g, &trie, &verify_morpher, walk::DEFAULT_MAX_BEAM_WORK);
    let words = read_words(&wpath);

    let seq = replay::verify_words_parallel(&g, &analyzer, &words, 1);
    let par = replay::verify_words_parallel(&g, &analyzer, &words, high_threads);

    assert_eq!(seq.len(), words.len());
    assert_eq!(par.len(), words.len());
    for (i, (s, p)) in seq.iter().zip(par.iter()).enumerate() {
        assert_eq!(
            s, p,
            "word {i} (\"{}\"): threads=1 vs threads={high_threads} diverge",
            words[i]
        );
    }
}

#[test]
fn verify_pool_is_thread_invariant_on_indonesian() {
    assert_thread_invariant("indonesian-hc.xml", "indonesian-words.txt", "indonesian", 8);
}

/// Sena slice-60 gets the same treatment (a second, structurally different grammar -- zero
/// phonological rules, the largest trie -- so this isn't just re-exercising Indonesian's own code
/// path).
#[test]
fn verify_pool_is_thread_invariant_on_sena_slice_60() {
    assert_thread_invariant("sena-hc.xml", "slice-60.txt", "sena", 4);
}

/// Soundness assert (F5 gate, plan §3.4: "a Rust-side re-verification that every emitted analysis
/// is confirmed -- by-construction, but assert it in the harness once"). For every analysis the
/// Indonesian corpus's verified run actually emits, convert it back into a [`Candidate`]
/// (`morpheme_ids` -> `Vec<MorphemeId>`, `root_morpheme_index` -> `root_index`, the exact inverse
/// of what `confirm` compares against) and feed it through the SAME `confirm` a second time: it
/// must re-confirm to an analysis with an identical signature. This is not a tautology-by-
/// construction in the test itself -- a bug in `confirm`'s own match predicate (e.g. an overly
/// loose comparison) could make the first pass wrongly accept something the SECOND real-engine run
/// then fails to reproduce -- so this drives the real `confirm` path twice, independently, per the
/// F5 gate's own instruction to assert this in the harness rather than take it on faith.
#[test]
fn every_emitted_indonesian_analysis_reconfirms() {
    let Some(wpath) = sample_path("indonesian-words.txt") else {
        eprintln!("skipping: indonesian-words.txt not present on disk");
        return;
    };
    let Some(g) = load_grammar("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let trie = build_trie(&g);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let analyzer =
        VerifiedFstAnalyzer::new(&g, &trie, &verify_morpher, walk::DEFAULT_MAX_BEAM_WORK);
    let owners = replay::build_morpheme_owners(&g);
    let words = read_words(&wpath);

    let mut total_emitted = 0usize;
    let mut total_reconfirmed = 0usize;
    for word in &words {
        for wa in analyzer.analyze_word(word) {
            total_emitted += 1;
            let candidate = Candidate {
                morphemes: wa.morpheme_ids.iter().map(|&id| MorphemeId(id)).collect(),
                root_index: wa.root_morpheme_index,
            };
            let reconfirmed = replay::confirm(&g, &owners, &verify_morpher, &candidate, word);
            assert!(
                reconfirmed.is_some(),
                "word {word:?}: emitted analysis {:?} (root {}) did not re-confirm",
                wa.morpheme_ids,
                wa.root_morpheme_index
            );
            assert_eq!(
                replay::signature(&g, &reconfirmed.unwrap()),
                replay::signature(&g, &wa),
                "word {word:?}: re-confirmed analysis has a different signature than the original"
            );
            total_reconfirmed += 1;
        }
    }
    assert!(
        total_emitted > 0,
        "expected at least one verified analysis across the Indonesian corpus"
    );
    assert_eq!(
        total_emitted, total_reconfirmed,
        "every emitted analysis must re-confirm, none should be lost"
    );
}

/// Quirk 8's compounding-CONDITIONALLY-open clause, specifically exercised (advisor review of this
/// milestone): the Indonesian and Sena-slice-60 gates above are both byte-identical, but NEITHER
/// corpus contains a verified multi-root candidate (Indonesian's compounding rules "must stay
/// silent" per the feasibility report; Sena's only known compound, `ndikhali`, is corpus index
/// 4386 -- outside the first 60 words). That means `confirm`'s `extra_roots`-population loop and
/// its `(!extra_roots.is_empty() && Compounding)` admission clause -- the single most load-bearing
/// line of this whole milestone -- ran ZERO times in the gates above, even though both passed. This
/// test drives it for real: on the FULL Sena grammar (not the slice), the bare proposer's own
/// candidate for "ndikhali" includes `entry413+entry1072` (confirmed directly against
/// `candidates-bare-full.tsv` line 287711, `mrule47+entry413+entry1072+mrule9:1` -- a genuine
/// two-root compound candidate the bare walker itself proposes), and the C# COMPOSITE verified
/// golden (`batch-chainoff-full.tsv`, idx 4386) confirms that exact signature as a real verified
/// analysis. So `VerifiedFstAnalyzer::analyze_word("ndikhali")` must yield at least one analysis
/// carrying two `entry` (root) tokens, and every one of those must re-confirm (idempotence) --
/// exactly the same double-run discipline as the Indonesian soundness assert above, targeted at
/// the one path that assert structurally could not reach.
#[test]
fn ndikhali_compound_verifies_with_two_roots_and_reconfirms() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let trie = build_trie(&g);
    let verify_morpher = Morpher::new(&g, usize::MAX);
    let analyzer =
        VerifiedFstAnalyzer::new(&g, &trie, &verify_morpher, walk::DEFAULT_MAX_BEAM_WORK);
    let owners = replay::build_morpheme_owners(&g);

    let verified = analyzer.analyze_word("ndikhali");
    assert!(
        !verified.is_empty(),
        "\"ndikhali\" (a known 2-root Sena compound) must verify to >=1 analysis"
    );

    let is_root_entry = |id: u32| {
        owners
            .get(id as usize)
            .copied()
            .flatten()
            .is_some_and(|o| matches!(o, replay::MorphemeOwner::LexEntry(_)))
    };
    let has_two_roots = verified.iter().any(|wa| {
        wa.morpheme_ids
            .iter()
            .filter(|&&id| is_root_entry(id))
            .count()
            >= 2
    });
    assert!(
        has_two_roots,
        "expected at least one verified \"ndikhali\" analysis with two root (LexEntry) morphemes, \
         got signatures: {:?}",
        verified
            .iter()
            .map(|wa| replay::signature(&g, wa))
            .collect::<Vec<_>>()
    );

    // Per an independent Fable review of F5: the exact-signature cross-check against the golden
    // was previously only a doc comment, not an assertion. Strengthened per that recommendation --
    // the bare walker's own candidate golden (candidates-bare-full.tsv:287711) proposes
    // "mrule47+entry413+entry1072+mrule9:1" for "ndikhali", and the C# composite-verified golden
    // confirms it as a real analysis; assert it's actually among what THIS engine verifies, not
    // just that some two-root analysis exists.
    let signatures: Vec<String> = verified.iter().map(|wa| replay::signature(&g, wa)).collect();
    assert!(
        signatures.iter().any(|s| s == "mrule47+entry413+entry1072+mrule9:1"),
        "expected the C#-oracle-confirmed signature \"mrule47+entry413+entry1072+mrule9:1\" among \
         \"ndikhali\"'s verified analyses, got: {signatures:?}"
    );

    for wa in &verified {
        let candidate = Candidate {
            morphemes: wa.morpheme_ids.iter().map(|&id| MorphemeId(id)).collect(),
            root_index: wa.root_morpheme_index,
        };
        let reconfirmed = replay::confirm(&g, &owners, &verify_morpher, &candidate, "ndikhali");
        assert!(
            reconfirmed.is_some(),
            "\"ndikhali\" analysis {:?} did not re-confirm",
            replay::signature(&g, wa)
        );
        assert_eq!(
            replay::signature(&g, &reconfirmed.unwrap()),
            replay::signature(&g, wa)
        );
    }
}
