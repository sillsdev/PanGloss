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
        eprintln!("  inferred segment: {:?} ({:?})", seg.representation, seg.evidence);
    }
    for boundary in &out.substrate.inferred_boundaries {
        eprintln!("  inferred boundary: {:?} ({:?})", boundary.representation, boundary.evidence);
    }

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

#[test]
fn sena3_compiles_through_compile_project_with() {
    // Ratchet baseline: stem-only usage collection, 40 ambiguous (undeclared `-`/`'`/`:` in reduplicated citation forms, Sena 3's plain-fwdata import never populates exemplar_characters), 0 unresolved.
    compile_and_report("Sena 3", 40, 0);
}

#[test]
fn amharic_compiles_through_compile_project_with() {
    compile_and_report("Amharic", 0, 0);
}
