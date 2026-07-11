//! `stats.rs` (F8, HYBRID_FST_RUST_PLAN.md §8) — assembles the `fst-stats` dump (C#
//! `FstStatsCommand.cs`) from pieces earlier milestones already built: [`crate::trie::Trie::
//! state_count`] (F3), the fixed knob-defaults literal (F0's frozen format), [`crate::compiler::
//! format_tier_report`] (F7), [`crate::advisor::Report::format`] (F8), and the per-affix /
//! bare-root dumps (F2 -- moved here from `tests/f2_surface_phonology_gate.rs`'s own private
//! helpers so BOTH that test and this milestone's full-file gate call the SAME production code,
//! per the plan's "reuse, don't duplicate" instruction).
//!
//! ## Byte-parity target: what the frozen golden actually contains
//! `FstStatsCommand.cs` (as read on the `fst-oracle` branch today) ALSO appends a trailing
//! `== StructuralDump ==\nsee structural-dump.txt (N lines)` section after bare-root surfaces. The
//! `stats.txt` goldens this crate gates against do NOT have that trailer (confirmed empirically:
//! `rust/parity-out/golden/fst-advisor/indonesian/stats.txt` is exactly 173 lines, ending right
//! after the last bare-root line, CRLF, no trailing blank line, no `== StructuralDump ==` --
//! verified via `od -c` on the tail bytes). The golden was captured from an EARLIER
//! `FstStatsCommand.cs` revision, before that extension landed (F3 added the separate
//! `structural-dump.txt` SIBLING file directly, without re-freezing `stats.txt` itself or updating
//! `MANIFEST.txt` to record it — a documentation gap on the F3 milestone's part, not a data
//! problem: `f3_trie_gate.rs`'s own gate already reads `structural-dump.txt` as an independent
//! artifact and never expects a trailer inside `stats.txt`). [`assemble_lines`] therefore produces
//! the SIX frozen sections only (StateCount / Knob defaults / tier report / advisor report /
//! per-affix / bare-root), matching the golden's actual shape — NOT current `FstStatsCommand.cs`
//! verbatim. A future re-freeze that folds the structural-dump pointer back into `stats.txt` is a
//! manifest/golden update, not a code fix here.

use std::collections::BTreeSet;

use hc_grammar::model::{Grammar, LexEntryId, MorphRuleDef, OutputAction};
use hc_parse::Morpher;

use crate::advisor;
use crate::compiler::{self, CompiledRuleInverse};
use crate::surface::SurfacePhonology;
use crate::trie::Trie;

fn join(items: &[String]) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items.join(";")
    }
}

/// C# `FstStatsCommand.XmlIdOf` (`TryGet(entry, id) ?? entry.Id ?? "?"`) — this port's
/// `MorphemeInfo::xml_key` is exactly that stable id (see `f1_selector_gate.rs`'s identical
/// convention, and `f2_surface_phonology_gate.rs`'s own former copy of this helper).
pub fn xml_key_of(g: &Grammar, entry: LexEntryId) -> &str {
    let morpheme = g.entries[entry.0 as usize].morpheme;
    &g.morphemes[morpheme.0 as usize].xml_key
}

/// C# `FstStatsCommand.AffixUnderlyingForms` (`:119-142`): every distinct affix underlying-form
/// string across the grammar's affix-process/realizational-affix rules, iterated PER STRATUM (a
/// rule unreferenced by any stratum's own `morphologicalRules` list must not contribute — see
/// `f2_surface_phonology_gate.rs`'s original doc for this same finding).
pub fn affix_underlying_forms(g: &Grammar) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
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

/// `== Per-affix Variants / DeletionJunctions ==` section body lines (no header, no trailing
/// blank).
pub fn per_affix_lines(g: &Grammar, surface: &SurfacePhonology) -> Vec<String> {
    let mut lines = Vec::new();
    for underlying in affix_underlying_forms(g) {
        let variants = surface.variants(&underlying);
        lines.push(format!("{underlying}\tVariants\t{}", join(&variants)));

        let mut junctions: Vec<String> = surface
            .deletion_junctions(&underlying)
            .into_iter()
            .map(|j| format!("{}/{}", j.affix_surface, j.deleted_neighbor))
            .collect();
        junctions.sort();
        lines.push(format!(
            "{underlying}\tDeletionJunctions\t{}",
            join(&junctions)
        ));
    }
    lines
}

/// `== Bare-root surfaces ==` section body lines (no header, no trailing blank). C# prints one
/// line PER ROOT ALLOMORPH, each independently recomputing the same whole-entry `GenerateWords`
/// result (`FstStatsCommand.cs:93-98`) — an entry with N allomorphs produces N identical lines.
pub fn bare_root_lines(g: &Grammar, surface: &SurfacePhonology, morpher: &Morpher) -> Vec<String> {
    let mut lines = Vec::new();
    for sd in &g.strata {
        let mut entries: Vec<LexEntryId> = sd.entries.clone();
        entries.sort_by(|&a, &b| xml_key_of(g, a).cmp(xml_key_of(g, b)));
        for entry in entries {
            let n_allomorphs = g.entries[entry.0 as usize].allomorphs.len();
            let surfaces = surface.bare_root_surfaces(morpher, entry);
            let line = format!("{}\t{}", xml_key_of(g, entry), join(&surfaces));
            for _ in 0..n_allomorphs {
                lines.push(line.clone());
            }
        }
    }
    lines
}

/// The full frozen `fst-stats` text, as a `Vec` of lines (no trailing newline concerns — see this
/// module's doc for why comparing lines rather than raw bytes sidesteps `format_tier_report`'s own
/// inconsistent trailing-newline convention across the zero-rules/some-rules cases). Six sections,
/// each preceded by its `== Header ==` line and followed by exactly one blank separator line,
/// EXCEPT the last (bare-root surfaces), which simply ends the file (matching the golden's own
/// EOF, confirmed via hexdump — see this module's doc).
#[allow(clippy::vec_init_then_push)] // interleaved with computed values (state count, tier
                                     // report/advisor lines) -- not expressible as one `vec![]`.
pub fn assemble_lines(
    trie: &Trie,
    compiled: &[CompiledRuleInverse],
    advisor_report: &advisor::Report,
    g: &Grammar,
    surface: &SurfacePhonology,
    morpher: &Morpher,
) -> Vec<String> {
    let mut out = Vec::new();

    out.push("== StateCount ==".to_string());
    out.push(trie.state_count().to_string());
    out.push(String::new());

    out.push("== Knob defaults ==".to_string());
    out.push("maxStates=1000000".to_string());
    out.push("derivDepth=2".to_string());
    out.push("maxBeamWork=1000000".to_string());
    out.push("maxAffixes=2".to_string());
    out.push("enableJunctionProbing=true".to_string());
    out.push("forwardSynthesis=false".to_string());
    out.push("useChainPhonology=false".to_string());
    out.push(String::new());

    out.push("== RuleInverseCompiler tier report ==".to_string());
    for line in compiler::format_tier_report(compiled).lines() {
        out.push(line.to_string());
    }
    out.push(String::new());

    out.push("== GrammarFstAdvisor report ==".to_string());
    for line in advisor_report.format().lines() {
        out.push(line.to_string());
    }
    out.push(String::new());

    out.push("== Per-affix Variants / DeletionJunctions ==".to_string());
    out.extend(per_affix_lines(g, surface));
    out.push(String::new());

    out.push("== Bare-root surfaces ==".to_string());
    out.extend(bare_root_lines(g, surface, morpher));

    out
}
