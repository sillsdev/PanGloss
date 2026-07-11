//! F7 gate (HYBRID_FST_RUST_PLAN.md §8): "chain-on verified parity (Indonesian 121/121
//! byte-identical vs the chain-on golden); the I4 marquee cross-check reproduced (`--no-junctions
//! --chain` over `men-words.txt` byte-matches its golden -- 46/46)."
//!
//! `--chain` alone means the FULL composite (`FstTemplateAnalyzer` + `ReduplicationProposer` +
//! `InfixProposer` + `ComposedPhonologyProposer` (stub) + `ChainPhonologyProposer` instead of
//! `LockstepPhonologyProposer`) -- NOT the bare walker alone (that's `--bare`'s separate meaning,
//! per `HYBRID_FST_RUST_PLAN.md` §6.1). `--no-junctions` additionally disables junction probing on
//! the MAIN (shared bare-FST/Redup/Infix) trie ONLY -- `ChainPhonologyProposer`'s own
//! underlying-only trie never has junction probing (or affix surface variants) regardless, per
//! `proposers.rs`'s doc.

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
    let path = manifest_dir.join("../../parity-out/golden/fst-advisor").join(grammar).join(file_name);
    path.exists().then_some(path)
}

fn words_path(grammar_dir: &str, name: &str) -> Option<PathBuf> {
    sample_path(name).or_else(|| golden_path(grammar_dir, name))
}

fn read_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path).expect("read golden").lines().map(|l| l.trim_end_matches('\r').to_string()).filter(|l| !l.is_empty()).collect()
}

fn read_words(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path).expect("read word list").lines().map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect()
}

fn load_grammar(grammar_file: &str) -> Option<Grammar> {
    let gpath = sample_path(grammar_file)?;
    let xml = std::fs::read_to_string(&gpath).expect("read grammar");
    Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
}

/// Verified-composite-batch gate, chain-on.
fn run_chain_batch_gate(grammar_file: &str, words_file: &str, grammar_dir: &str, golden_file: &str, enable_junction_probing: bool) {
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
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, enable_junction_probing);
    let chain_morpher = Morpher::new(&g, usize::MAX);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false)
        .with_chain_phonology(&g, &surface, &chain_morpher, 1_000_000, 2);

    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);

    let words = read_words(&wpath);
    let golden_lines = read_lines(&gold_path);

    let mut rust_lines = Vec::with_capacity(words.len() * 2);
    for (i, word) in words.iter().enumerate() {
        let [started, result] = composite::batch_lines(&g, &composite, &verify_morpher, &owners, i, word);
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
        assert_eq!(rust_line, golden_line, "{grammar_dir}/{golden_file}: diverges at line {i}");
    }
}

/// The F7 primary gate: Indonesian full 121-word corpus, chain-on, byte-identical to
/// `batch-chainon.tsv` (default junction probing -- ON -- on the main trie; the golden was
/// generated with the composite's own default knobs plus `--chain`).
#[test]
fn indonesian_chain_on_verified_matches_chainon_golden() {
    run_chain_batch_gate("indonesian-hc.xml", "indonesian-words.txt", "indonesian", "batch-chainon.tsv", true);
}

/// The I4 marquee cross-check: `--no-junctions --chain` over the 46 non-reduplicated meN- words
/// byte-matches `men-words-batch.tsv` -- proves the general chain SUBSUMES the grammar-specific
/// junction-probing special case (junction probing disabled on the main trie; the chain phonology
/// proposer, which never used junction probing anyway, recovers the same words alone).
#[test]
fn indonesian_no_junctions_chain_matches_men_words_golden() {
    run_chain_batch_gate("indonesian-hc.xml", "men-words.txt", "indonesian", "men-words-batch.tsv", false);
}

/// F6's empirical finding ("neither ComposedPhonologyProposer nor LockstepPhonologyProposer ever
/// contributes a distinguishable candidate on Indonesian") re-confirmed now that BOTH are built for
/// REAL (not just assumed-safe stubs): enabling both against the real Indonesian grammar must still
/// reproduce the chain-OFF golden byte-for-byte (`batch-chainoff.tsv`), same as the plain stub
/// composite F6 already gates. This is the corpus-level double-check the plan asks for; the NEW
/// required gate (a toy grammar where they DO contribute) is what actually forces them to prove
/// themselves -- see the toy-gate test file.
#[test]
fn indonesian_real_lockstep_and_composed_are_still_corpus_inert() {
    let Some(g) = load_grammar("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present");
        return;
    };
    let Some(wpath) = words_path("indonesian", "indonesian-words.txt") else {
        eprintln!("skipping: word list not present");
        return;
    };
    let Some(gold_path) = golden_path("indonesian", "batch-chainoff.tsv") else {
        eprintln!("skipping: golden not present");
        return;
    };

    let surface = SurfacePhonology::new(&g);
    let build_morpher = Morpher::new(&g, usize::MAX);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let lockstep_morpher = Morpher::new(&g, usize::MAX);
    let composite = CompositeAnalyzer::new(&g, &trie, &surface, walk::DEFAULT_MAX_BEAM_WORK, false)
        .with_composed_phonology(&g)
        .with_lockstep_phonology(&g, &surface, &lockstep_morpher, 1_000_000, 2);

    let verify_morpher = Morpher::new(&g, usize::MAX);
    let owners = replay::build_morpheme_owners(&g);
    let words = read_words(&wpath);
    let golden_lines = read_lines(&gold_path);

    let mut rust_lines = Vec::with_capacity(words.len() * 2);
    for (i, word) in words.iter().enumerate() {
        let [started, result] = composite::batch_lines(&g, &composite, &verify_morpher, &owners, i, word);
        rust_lines.push(started);
        rust_lines.push(result);
    }
    assert_eq!(rust_lines.len(), golden_lines.len());
    for (i, (rust_line, golden_line)) in rust_lines.iter().zip(golden_lines.iter()).enumerate() {
        assert_eq!(rust_line, golden_line, "diverges at line {i} -- Lockstep/Composed contributed something new");
    }
}
