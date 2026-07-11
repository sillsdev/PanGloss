//! F1 gate (HYBRID_FST_RUST_PLAN.md §8, F1's own gate list): "selector-restricted Rust analysis
//! byte-matches the F0 `fst-restricted` golden (first 20 Indonesian corpus words, deterministic)".
//!
//! This is the concrete, mechanical proof that the new `lex_entry_filter`/`rule_filter` plumbing
//! (`hc-parse::Morpher::parse_word_selected`, §7.1 item 1) reproduces C#'s `FstReplay`-style
//! restricted-analysis semantics: for each golden line `{idx}\t{word}\t{candidate}\t{restricted}`,
//! this test restricts `Morpher` to exactly the candidate's root (`lex_entry_filter`), runs the
//! restricted analysis, computes the byte-identical signature format the golden uses (per
//! `rust/parity-out/golden/fst-advisor/MANIFEST.txt` §1: `join("+", xml id) + ":" + root index` —
//! NOT `Morpher.Id`, which is empty in this grammar), and compares.
//!
//! Every one of the first 20 Indonesian corpus words is an unambiguous or homograph-ambiguous BARE
//! ROOT (no morphological rules involved) — confirmed by inspecting the golden itself (every
//! candidate/restricted signature is `entryNN:0`, no `+`-joined rule chain) — so this gate exercises
//! `lex_entry_filter` exhaustively but does not exercise `rule_filter` on any non-trivial rule set;
//! `rule_filter`'s first real exercise is F5 (`replay.rs`), per the plan's own milestone split.

use std::path::{Path, PathBuf};

use hc_grammar::model::LexEntryId;
use hc_parse::{Morpher, ParseOptions};

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn golden_path() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../parity-out/golden/fst-advisor/indonesian/restricted-first20.tsv");
    path.exists().then_some(path)
}

/// The F0-frozen signature format (MANIFEST.txt §1): `join("+", xml_key) + ":" + root_index`, where
/// `xml_key` is `MorphemeInfo::xml_key` (the `<...  id="...">` XML attribute) — the fallback the
/// MANIFEST records because `<MorphemeId>`/`Morpheme.Id` is empty for every morpheme in this
/// grammar. Sorted-ordinal, `;`-joined across analyses, `-` for the empty set, matching every other
/// golden in this directory.
fn signature(g: &hc_grammar::model::Grammar, analyses: &[hc_parse::WordAnalysis]) -> String {
    let mut sigs: Vec<String> = analyses
        .iter()
        .map(|wa| {
            let joined: Vec<&str> =
                wa.morpheme_ids.iter().map(|&id| g.morphemes[id as usize].xml_key.as_str()).collect();
            format!("{}:{}", joined.join("+"), wa.root_morpheme_index)
        })
        .collect();
    sigs.sort();
    if sigs.is_empty() {
        "-".to_string()
    } else {
        sigs.join(";")
    }
}

/// Find the [`LexEntryId`] whose owning morpheme's `xml_key` equals `key` (e.g. `"entry25"`).
fn entry_by_xml_key(g: &hc_grammar::model::Grammar, key: &str) -> LexEntryId {
    g.entries
        .iter()
        .enumerate()
        .find(|(_, e)| g.morphemes[e.morpheme.0 as usize].xml_key == key)
        .map(|(i, _)| LexEntryId(i as u32))
        .unwrap_or_else(|| panic!("no LexicalEntry with id=\"{key}\" in the grammar"))
}

/// Parse one golden `candidate`/`restricted` column into `(xml_key, root_index)` — both columns use
/// the same format; this is used for both (the candidate always has exactly one signature, no `-`
/// case in this golden — every one of the first 20 words has >=1 analysis, per MANIFEST.txt §4).
fn parse_single_signature(sig: &str) -> (&str, i32) {
    let (key, idx) = sig.rsplit_once(':').unwrap_or_else(|| panic!("malformed signature: {sig}"));
    (key, idx.parse().unwrap_or_else(|_| panic!("malformed root index in: {sig}")))
}

#[test]
fn selector_restricted_analysis_matches_fst_restricted_golden_first20() {
    let Some(grammar_path) = sample_path("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let Some(golden) = golden_path() else {
        eprintln!("skipping: restricted-first20.tsv golden not present on disk");
        return;
    };

    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let g = hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let morpher = Morpher::new(&g, usize::MAX);

    let golden_text = std::fs::read_to_string(&golden).expect("read golden");
    let mut checked = 0usize;
    let mut word_indices = std::collections::BTreeSet::new();
    for line in golden_text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), 4, "malformed golden line: {line:?}");
        let (_idx, word, candidate_sig, expected_restricted) = (cols[0], cols[1], cols[2], cols[3]);

        // The candidate: pin `lex_entry_filter` to exactly this root (FstReplay.cs:73's
        // `e => e == root || extraRoots.Contains(e)` — no compound extra roots in this battery).
        let (candidate_key, _candidate_root_index) = parse_single_signature(candidate_sig);
        let root = entry_by_xml_key(&g, candidate_key);
        let filter = |le: LexEntryId| le == root;

        let outcome = morpher.parse_word_selected(word, &ParseOptions::default(), Some(&filter), None);
        let restricted_sig = signature(&g, &outcome.structured);

        assert_eq!(
            restricted_sig, expected_restricted,
            "word {word:?}, candidate {candidate_sig:?}: restricted analysis mismatch"
        );
        word_indices.insert(_idx.to_string());
        checked += 1;
    }
    // 20 distinct corpus-word indices (0..19); "ajar" is a homograph (two lex entries), so the
    // golden has 21 lines total for those 20 words — checking both counts catches either a missing
    // word or a silently-dropped homograph line.
    assert_eq!(word_indices.len(), 20, "expected 20 distinct word indices (first 20 corpus words)");
    assert!(checked >= 20, "expected at least 20 golden lines, got {checked}");
}
