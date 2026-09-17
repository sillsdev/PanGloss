//! Compiles the real Sena 3 / Amharic FieldWorks projects through `compile_project_with`, unlike `real_projects.rs` which only imports.

use std::path::PathBuf;

use pg_grammar::compile::{CompileOptions, SemanticLossPolicy};

fn project_fwdata(project_dir_name: &str) -> Option<PathBuf> {
    let base = std::env::var("PANGLOSS_FW_PROJECTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(r"C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects")
        });
    let path = base
        .join(project_dir_name)
        .join(format!("{project_dir_name}.fwdata"));
    path.exists().then_some(path)
}

/// `MeasureOnly` keeps every issue non-fatal, so the counts below are the real measurement.
fn compile_and_report(project_dir_name: &str, max_ambiguous: usize, max_unresolved: usize) {
    let Some(path) = project_fwdata(project_dir_name) else {
        eprintln!("skipping {project_dir_name}: FieldWorks checkout not present");
        return;
    };
    let (snap, _report) = pg_fwdata::import_file(&path).expect("must import");
    let out = pg_grammar::compile_project_with(
        &snap,
        CompileOptions {
            semantic_loss: SemanticLossPolicy::MeasureOnly,
            ..CompileOptions::default()
        },
    )
    .expect("MeasureOnly never refuses");

    eprintln!(
        "{project_dir_name}: inferred_segments={} inferred_boundaries={} unresolved_uses={} \
         ambiguous_uses={} total_issues={}",
        out.substrate.inferred_segments.len(),
        out.substrate.inferred_boundaries.len(),
        out.substrate.unresolved_uses.len(),
        out.substrate.ambiguous_uses.len(),
        out.issues.len(),
    );
    for seg in &out.substrate.inferred_segments {
        eprintln!(
            "  inferred segment: {:?} ({:?})",
            seg.representation, seg.evidence
        );
    }
    for boundary in &out.substrate.inferred_boundaries {
        eprintln!(
            "  inferred boundary: {:?} ({:?})",
            boundary.representation, boundary.evidence
        );
    }
    for issue in out
        .issues
        .iter()
        .filter(|i| i.code == "substrate.classification-ambiguous")
    {
        eprintln!("  ambiguous: {}", issue.message);
    }

    let unexpected: Vec<&str> = out
        .issues
        .iter()
        .filter(|i| i.code == "substrate.classification-ambiguous")
        .map(|i| i.message.as_str())
        .filter(|m| unexpected_ambiguous_char(m))
        .collect();
    assert!(
        unexpected.is_empty(),
        "{project_dir_name}: ambiguous issue over a character outside the known \
         `-`/`'`/`:`/`_`/`^` set: {unexpected:#?}"
    );

    // A ratchet, not a target: today's count stays legible while a new regression fails.
    assert!(
        out.substrate.ambiguous_uses.len() <= max_ambiguous,
        "{project_dir_name}: ambiguous_uses grew to {} (ratchet: {max_ambiguous})",
        out.substrate.ambiguous_uses.len()
    );
    assert!(
        out.substrate.unresolved_uses.len() <= max_unresolved,
        "{project_dir_name}: unresolved_uses grew to {} (ratchet: {max_unresolved})",
        out.substrate.unresolved_uses.len()
    );
}

/// Matches `ambiguous_issue`'s literal `{ch:?} at position` fragment -- a plain substring search over the whole message is vacuous, since the fixed template text always itself contains a `:` and `{ch:?}`'s own Debug-escaping quotes.
fn unexpected_ambiguous_char(message: &str) -> bool {
    const KNOWN_CHARS: [char; 5] = ['-', '\'', ':', '_', '^'];
    !KNOWN_CHARS
        .iter()
        .any(|c| message.contains(&format!("{c:?} at position")))
}

#[cfg(test)]
mod unexpected_ambiguous_char_tests {
    use super::unexpected_ambiguous_char;

    /// The bug this guards: a bare `:`/`'` substring search matches every message regardless of the actual failing character, since the fixed template text contains both.
    #[test]
    fn a_known_character_is_not_flagged() {
        let msg = "cannot segment \"alt:.nkhundu\": ':' at position 3 is neither a vernacular \
                   exemplar, an authored boundary, nor in the safe boundary table; refusing \
                   rather than guessing whether it is a segment or a boundary";
        assert!(!unexpected_ambiguous_char(msg));
    }

    #[test]
    fn a_genuinely_new_character_is_flagged() {
        let msg = "cannot segment \"pakati_na_kati\": '_' at position 6 is neither a vernacular \
                   exemplar, an authored boundary, nor in the safe boundary table; refusing \
                   rather than guessing whether it is a segment or a boundary";
        // '_' is already in KNOWN_CHARS, so substitute a truly unknown character instead.
        let msg_with_unknown_char = msg.replace('_', "~");
        assert!(unexpected_ambiguous_char(&msg_with_unknown_char));
    }
}

#[test]
fn sena3_compiles_through_compile_project_with() {
    // Ratchet baseline (post substrate-probe/builder segmenter-agreement fix): 9 of the previous 18 were false positives from `substrate::complete`'s probe skipping Boundary-kind matches a real builder accepts (see `substrate`'s module doc); the remaining 9 ambiguous are `:` (LDML's separate "punctuation" exemplar type, never the main set) and `_` (not in any LDML exemplar type), 0 unresolved.
    compile_and_report("Sena 3", 9, 0);
}

#[test]
fn amharic_compiles_through_compile_project_with() {
    compile_and_report("Amharic", 0, 0);
}
