//! F2 gate (HYBRID_FST_RUST_PLAN.md §8): "`Variants`/`DeletionJunctions`/bare-root dumps
//! byte-identical to the `fst-stats` goldens on all three grammars".
//!
//! `fst-stats` itself (StateCount, tier report, advisor report) is later milestones' work (F3/F7/
//! F8); this test extracts and reproduces only the two sections F2's `SurfacePhonology` owns:
//! "== Per-affix Variants / DeletionJunctions ==" and "== Bare-root surfaces ==", mirroring
//! `FstStatsCommand.cs`'s exact enumeration (`AffixUnderlyingForms`, the per-stratum sorted-entry
//! bare-root loop) and line format (`FstStatsCommand.cs:72-100`) line-for-line.
//!
//! F8 moved this test's own private helpers (`affix_underlying_forms`/`xml_key_of`/the two dump
//! loops) into production code (`hc_hybrid::stats`), since F8's own full-file `fst-stats` gate
//! (`f8_fst_stats_gate.rs`) needs the identical computation — this test now calls that production
//! code directly instead of carrying its own copy (plan §4.1's "reuse, don't duplicate").

use std::path::{Path, PathBuf};

use hc_hybrid::stats::{bare_root_lines, per_affix_lines};
use hc_hybrid::surface::SurfacePhonology;
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

/// Extract the lines of one `== Section ==` block (exclusive of the header, up to the next blank
/// line or EOF), normalizing CRLF -> nothing-extra (`\r` stripped) so this compares CONTENT, not
/// the golden's Windows line-ending convention.
fn section_lines(text: &str, header: &str) -> Vec<String> {
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    for line in lines.by_ref() {
        if line == header {
            break;
        }
    }
    let mut out = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        out.push(line.to_string());
    }
    out
}

/// Runs the F2 gate for one grammar: builds both dump sections from the live grammar + a fresh
/// `SurfacePhonology`/`Morpher`, and asserts they match the golden `stats.txt`'s corresponding
/// sections line-for-line (order included -- both sides sort the same way).
fn run_gate(grammar_file: &str, golden_dir: &str) {
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
    let surface = SurfacePhonology::new(&g);
    let morpher = Morpher::new(&g, usize::MAX);

    let variant_lines = per_affix_lines(&g, &surface);
    let bare_lines = bare_root_lines(&g, &surface, &morpher);

    let golden_text = std::fs::read_to_string(&golden).expect("read golden");
    let golden_variants =
        section_lines(&golden_text, "== Per-affix Variants / DeletionJunctions ==");
    let golden_bare_roots = section_lines(&golden_text, "== Bare-root surfaces ==");

    assert_eq!(
        variant_lines, golden_variants,
        "{grammar_file}: Variants/DeletionJunctions dump mismatch"
    );
    assert_eq!(
        bare_lines, golden_bare_roots,
        "{grammar_file}: bare-root surfaces dump mismatch"
    );
}

#[test]
fn indonesian_variants_and_bare_roots_match_golden() {
    run_gate("indonesian-hc.xml", "indonesian");
}

#[test]
fn sena_variants_and_bare_roots_match_golden() {
    run_gate("sena-hc.xml", "sena");
}

#[test]
#[ignore = "slow probe: Amharic's 417-segment alphabet makes DeletionJunctions probing \
            expensive in debug builds (release-mode run: 104.14s, in line with the C# oracle's \
            ~112s figure). Verified byte-identical against the golden in release mode as part \
            of the F2 milestone; run explicitly with --ignored --release when re-verifying."]
fn amharic_variants_and_bare_roots_match_golden() {
    run_gate("amharic-hc.xml", "amharic");
}
