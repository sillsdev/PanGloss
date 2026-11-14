//! Compare the complete grammar and diagnostics for FieldWorks conformance projects.
use pg_grammar::compile::test_support::assert_grammars_equal;
use pg_grammar::{
    compile_project_grammar_only_with_import_warnings as lean,
    compile_project_with_options_and_import_warnings as full, GrammarError,
};
use std::path::{Path, PathBuf};

fn projects(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if path.is_dir() {
            projects(&path, out);
        } else if path.extension().is_some_and(|e| e == "fwdata") {
            out.push(path);
        }
    }
}

fn compare(snapshot: &pg_snapshot::Snapshot, warnings: &[pg_snapshot::Warning]) -> bool {
    let a = full(snapshot, Default::default(), warnings.iter().cloned());
    let b = lean(snapshot, Default::default(), warnings.iter().cloned());
    match (a, b) {
        (Ok(a), Ok(b)) => {
            assert_grammars_equal(&a.grammar, &b.grammar);
            assert_eq!(a.warnings, b.warnings);
            assert_eq!(a.issues, b.issues);
            assert_eq!(a.substrate, b.substrate);
            false
        }
        (Err(GrammarError::Conversion(a)), Err(GrammarError::Conversion(b))) => {
            assert_eq!(a.warnings, b.warnings);
            assert_eq!(a.issues, b.issues);
            assert_eq!(a.substrate, b.substrate);
            true
        }
        (Err(a), Err(b)) => {
            assert_eq!(a.to_string(), b.to_string());
            false
        }
        (a, b) => panic!("compile results differ: {a:?} vs {b:?}"),
    }
}

#[test]
fn lean_compile_matches_full_on_fieldworks_conformance_projects_and_refusal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut paths = Vec::new();
    projects(&root.join("conformance-staging"), &mut paths);
    let suite = pg_conformance_fixtures::suite_root();
    if std::env::var("PANGLOSS_CONFORMANCE_SCOPE").as_deref() != Ok("local") {
        assert!(
            suite.is_dir(),
            "conformance suite missing: {}",
            suite.display()
        );
        projects(&suite, &mut paths);
    }
    paths.sort();
    assert!(!paths.is_empty());
    for path in paths {
        let (snapshot, report) =
            pg_fwdata::import_file(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        compare(&snapshot, &report.warnings);
    }
    let fixture = root.join("rust/crates/pg-fwdata/tests/data/fixture.fwdata");
    let (mut snapshot, report) = pg_fwdata::import_file(&fixture).unwrap();
    snapshot.conversion_provenance = Default::default();
    assert!(
        compare(&snapshot, &report.warnings),
        "unknown provenance must refuse"
    );
}
