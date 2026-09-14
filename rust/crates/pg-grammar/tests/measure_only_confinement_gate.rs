//! `SemanticLossPolicy::MeasureOnly` must never reach a production entry point -- enforced here by scanning source, not stated and left unchecked.

use std::path::{Path, PathBuf};

/// Path prefixes (relative to `rust/crates/`) allowed to mention `MeasureOnly` at all.
const ALLOWED_PREFIXES: &[&str] =
    &["pg-grammar/src/compile/", "pg-grammar/tests/", "pg-fwdata/tests/"];

fn crates_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Recursively collects every `.rs` file under `dir`.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn measure_only_appears_only_in_allowed_crates_and_paths() {
    let root = crates_root();
    assert!(root.join("pg-grammar").is_dir(), "crates root resolved wrong: {}", root.display());

    let mut files = Vec::new();
    collect_rs_files(&root, &mut files);
    assert!(!files.is_empty(), "scan found zero .rs files -- the gate is not looking at anything");

    let mut offenders = Vec::new();
    for path in &files {
        let rel = path.strip_prefix(&root).unwrap_or(path).to_string_lossy().replace('\\', "/");
        if ALLOWED_PREFIXES.iter().any(|p| rel.starts_with(p)) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        for (idx, line) in text.lines().enumerate() {
            if line.contains("MeasureOnly") {
                offenders.push(format!("{rel}:{}", idx + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "MeasureOnly reached outside its allowed paths ({ALLOWED_PREFIXES:?}); a production entry \
         point must never be able to construct it. Offending file:line(s): {offenders:?}"
    );
}
