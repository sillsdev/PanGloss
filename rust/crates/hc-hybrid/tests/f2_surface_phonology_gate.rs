//! F2 gate (HYBRID_FST_RUST_PLAN.md §8): "`Variants`/`DeletionJunctions`/bare-root dumps
//! byte-identical to the `fst-stats` goldens on all three grammars".
//!
//! `fst-stats` itself (StateCount, tier report, advisor report) is later milestones' work (F3/F7/
//! F8); this test extracts and reproduces only the two sections F2's `SurfacePhonology` owns:
//! "== Per-affix Variants / DeletionJunctions ==" and "== Bare-root surfaces ==", mirroring
//! `FstStatsCommand.cs`'s exact enumeration (`AffixUnderlyingForms`, the per-stratum sorted-entry
//! bare-root loop) and line format (`FstStatsCommand.cs:72-100`) line-for-line.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use hc_grammar::model::{LexEntryId, MorphRuleDef, OutputAction};
use hc_hybrid::surface::SurfacePhonology;
use hc_parse::Morpher;

fn sample_path(name: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn golden_path(grammar: &str) -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../parity-out/golden/fst-advisor").join(grammar).join("stats.txt");
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

fn join(items: &[String]) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items.join(";")
    }
}

/// C# `FstStatsCommand.AffixUnderlyingForms` (`FstStatsCommand.cs:119-142`): every distinct affix
/// underlying-form string across the grammar's affix-process/realizational-affix rules.
fn affix_underlying_forms(g: &hc_grammar::model::Grammar) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    // C# iterates `stratum.MorphologicalRules` PER STRATUM (`FstStatsCommand.cs:122-124`), not a
    // language-wide rule table -- a declared rule unreferenced by any stratum's own
    // `morphologicalRules` list (e.g. Sena has several such rules) must NOT contribute here.
    for sd in &g.strata {
        for &mid in &sd.mrules {
            let mrule = &g.mrules[mid.0 as usize];
            let allomorphs = match mrule {
                MorphRuleDef::AffixProcess(def) => &def.allomorphs,
                MorphRuleDef::Realizational(def) => &def.allomorphs,
                MorphRuleDef::Compounding(_) => continue,
            };
            for allo in allomorphs {
                for action in &allo.rhs {
                    if let OutputAction::InsertSegments { shape, .. } = action {
                        seen.insert(shape.text.clone());
                    }
                }
            }
        }
    }
    seen
}

/// C# `FstStatsCommand.XmlIdOf` (`XmlMorphemeIds.TryGet(entry, id) ?? entry.Id ?? "?"`) — this
/// port's `MorphemeInfo::xml_key` is exactly that stable id (see `f1_selector_gate.rs`'s identical
/// convention).
fn xml_key_of(g: &hc_grammar::model::Grammar, entry: LexEntryId) -> &str {
    let morpheme = g.entries[entry.0 as usize].morpheme;
    &g.morphemes[morpheme.0 as usize].xml_key
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

    // --- "== Per-affix Variants / DeletionJunctions ==" ---
    let mut variant_lines = Vec::new();
    for underlying in affix_underlying_forms(&g) {
        let variants = surface.variants(&underlying);
        variant_lines.push(format!("{underlying}\tVariants\t{}", join(&variants)));
        let junctions: Vec<String> = {
            let mut js: Vec<String> = surface
                .deletion_junctions(&underlying)
                .into_iter()
                .map(|j| format!("{}/{}", j.affix_surface, j.deleted_neighbor))
                .collect();
            js.sort();
            js
        };
        variant_lines.push(format!("{underlying}\tDeletionJunctions\t{}", join(&junctions)));
    }

    // --- "== Bare-root surfaces ==" ---
    let mut bare_root_lines = Vec::new();
    for sd in &g.strata {
        let mut entries: Vec<LexEntryId> = sd.entries.clone();
        entries.sort_by(|&a, &b| xml_key_of(&g, a).cmp(xml_key_of(&g, b)));
        for entry in entries {
            let n_allomorphs = g.entries[entry.0 as usize].allomorphs.len();
            let surfaces = surface.bare_root_surfaces(&morpher, entry);
            let line = format!("{}\t{}", xml_key_of(&g, entry), join(&surfaces));
            // C# prints one line PER ROOT ALLOMORPH, each independently recomputing the SAME
            // whole-entry `GenerateWords` result (`FstStatsCommand.cs:93-98`) -- so an entry with N
            // allomorphs produces N identical lines, not one.
            for _ in 0..n_allomorphs {
                bare_root_lines.push(line.clone());
            }
        }
    }

    let golden_text = std::fs::read_to_string(&golden).expect("read golden");
    let golden_variants = section_lines(&golden_text, "== Per-affix Variants / DeletionJunctions ==");
    let golden_bare_roots = section_lines(&golden_text, "== Bare-root surfaces ==");

    assert_eq!(variant_lines, golden_variants, "{grammar_file}: Variants/DeletionJunctions dump mismatch");
    assert_eq!(bare_root_lines, golden_bare_roots, "{grammar_file}: bare-root surfaces dump mismatch");
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
