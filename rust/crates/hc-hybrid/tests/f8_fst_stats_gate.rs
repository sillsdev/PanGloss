//! F8 gate (HYBRID_FST_RUST_PLAN.md §8): "`fst-stats` output byte-identical on all three
//! grammars". Two tiers of test here:
//!
//! 1. `*_advisor_report_matches_golden`: JUST the `== GrammarFstAdvisor report ==` section
//!    (`advisor::analyze(&g).format()`), compared against the golden. This is pure static
//!    object-model analysis (no synthesis, no probing) — cheap on all three grammars, including
//!    Amharic, so it runs UNCONDITIONALLY (no `#[ignore]`) per an independent Fable review of this
//!    milestone's draft: §5.3 requires the advisor report on Amharic unconditionally, and nothing
//!    about this computation is expensive the way per-affix/bare-root probing is.
//! 2. `*_full_stats_matches_golden`: the WHOLE `fst-stats` file (all six sections) assembled via
//!    `hc_hybrid::stats::assemble_lines`, compared line-for-line against the golden in full. This
//!    is the stronger, unit-level gate advisor review called for (per-section tests never confirm
//!    the sections concatenate with the right headers/blank-line separators in order). Amharic's
//!    variant is `#[ignore]`d for the SAME reason `f2_surface_phonology_gate.rs`'s own Amharic test
//!    is (per-affix/bare-root DeletionJunctions probing is expensive in debug builds) — the
//!    advisor-only test above already covers Amharic's NEW-this-milestone section unconditionally.

use std::path::{Path, PathBuf};

use hc_hybrid::advisor;
use hc_hybrid::compiler;
use hc_hybrid::stats;
use hc_hybrid::surface::SurfacePhonology;
use hc_hybrid::trie::Trie;
use hc_parse::Morpher;

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn golden_path(grammar: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir
        .join("../../parity-out/golden/fst-advisor")
        .join(grammar)
        .join("stats.txt");
    path.exists().then_some(path)
}

/// Every line of the golden file, CRLF-normalized (same convention every other gate in this crate
/// uses) — the unit for the full-file comparison.
fn golden_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect()
}

/// Extract the `== GrammarFstAdvisor report ==` section's OWN lines: unlike
/// `f2_surface_phonology_gate.rs`'s `section_lines` (which stops at the FIRST blank line, correct
/// for the single-blank-line-terminated per-affix/bare-root sections), the advisor section
/// contains MANY internal blank lines (one before each advisory block) — so this instead reads
/// until the NEXT `== ... ==` section header, then trims exactly the one trailing blank separator
/// line the CLI's own extra `WriteLine()` adds (matching `advisor::Report::format`'s own
/// documented "always ends in exactly one line-terminated line" guarantee, with no internal
/// trailing blank of its own).
fn advisor_section_lines(text: &str) -> Vec<String> {
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    for line in lines.by_ref() {
        if line == "== GrammarFstAdvisor report ==" {
            break;
        }
    }
    let mut out = Vec::new();
    for line in lines {
        if line.starts_with("== ") && line.ends_with(" ==") {
            break;
        }
        out.push(line.to_string());
    }
    if out.last().is_some_and(|s| s.is_empty()) {
        out.pop();
    }
    out
}

fn run_advisor_gate(grammar_file: &str, golden_dir: &str) {
    let Some(grammar_path) = sample_path(grammar_file) else {
        eprintln!("skipping {grammar_file}: not present on disk");
        return;
    };
    let Some(golden) = golden_path(golden_dir) else {
        eprintln!("skipping {golden_dir}: stats.txt golden not present on disk");
        return;
    };

    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let g = hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let report = advisor::analyze(&g);
    let got: Vec<String> = report.format().lines().map(str::to_string).collect();

    let golden_text = std::fs::read_to_string(&golden).expect("read golden");
    let expected = advisor_section_lines(&golden_text);

    assert_eq!(
        got, expected,
        "{grammar_file}: GrammarFstAdvisor report section mismatch"
    );
}

#[test]
fn indonesian_advisor_report_matches_golden() {
    run_advisor_gate("indonesian-hc.xml", "indonesian");
}

#[test]
fn sena_advisor_report_matches_golden() {
    run_advisor_gate("sena-hc.xml", "sena");
}

/// Unconditional (NOT `#[ignore]`d) per this file's module doc: `GrammarFstAdvisor` is pure static
/// analysis, cheap regardless of Amharic's otherwise-expensive junction/DeletionJunctions probing.
#[test]
fn amharic_advisor_report_matches_golden() {
    run_advisor_gate("amharic-hc.xml", "amharic");
}

fn run_full_stats_gate(grammar_file: &str, golden_dir: &str) {
    let Some(grammar_path) = sample_path(grammar_file) else {
        eprintln!("skipping {grammar_file}: not present on disk");
        return;
    };
    let Some(golden) = golden_path(golden_dir) else {
        eprintln!("skipping {golden_dir}: stats.txt golden not present on disk");
        return;
    };

    let xml = std::fs::read_to_string(&grammar_path).expect("read grammar");
    let g = hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}"));
    let build_morpher = Morpher::new(&g, usize::MAX);
    let surface = SurfacePhonology::new(&g);
    let trie = Trie::build(&g, &surface, &build_morpher, 1_000_000, 2, true);
    let compiled = compiler::compile_default(&g);
    let advisor_report = advisor::analyze(&g);

    let got = stats::assemble_lines(
        &trie,
        &compiled,
        &advisor_report,
        &g,
        &surface,
        &build_morpher,
    );

    let golden_text = std::fs::read_to_string(&golden).expect("read golden");
    let expected = golden_lines(&golden_text);

    assert_eq!(
        got.len(),
        expected.len(),
        "{grammar_file}: fst-stats line-count mismatch"
    );
    for (i, (g, e)) in got.iter().zip(expected.iter()).enumerate() {
        assert_eq!(g, e, "{grammar_file}: fst-stats diverges at line {i}");
    }
}

#[test]
fn indonesian_full_stats_matches_golden() {
    run_full_stats_gate("indonesian-hc.xml", "indonesian");
}

#[test]
fn sena_full_stats_matches_golden() {
    run_full_stats_gate("sena-hc.xml", "sena");
}

#[test]
#[ignore = "slow probe: same cost as f2_surface_phonology_gate.rs's Amharic test (per-affix/\
            bare-root DeletionJunctions probing over a 417-segment alphabet) -- the advisor-only \
            gate above already covers Amharic's F8-new section unconditionally. Run explicitly \
            with --ignored --release."]
fn amharic_full_stats_matches_golden() {
    run_full_stats_gate("amharic-hc.xml", "amharic");
}
