//! `HealthFinding` is built through `HealthFinding::new` and nowhere else.

use std::fs;
use std::path::{Path, PathBuf};

/// Exempt: the module defining the type, and this gate, whose self-test holds literal examples.
const EXEMPT: &[&str] = &["health.rs", "health_finding_seam.rs"];

fn crates_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("crates root must resolve")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Struct literals only: not a definition, an `impl` header, or a return type.
fn literal_count(source: &str) -> usize {
    let mut count = 0;
    let mut rest = source;
    while let Some(at) = rest.find("HealthFinding") {
        let before = rest[..at].trim_end();
        let after = rest[at + "HealthFinding".len()..].trim_start();
        let opens_literal = after.starts_with('{');
        let is_definition =
            before.ends_with("struct") || before.ends_with("impl") || before.ends_with("->");
        if opens_literal && !is_definition {
            count += 1;
        }
        rest = &rest[at + "HealthFinding".len()..];
    }
    count
}

#[test]
fn health_findings_are_built_through_the_constructor() {
    let mut sources = Vec::new();
    rust_sources(&crates_root(), &mut sources);
    assert!(
        sources.len() > 100,
        "the source walk found only {} files -- it is not reaching the tree",
        sources.len()
    );

    let mut offenders = Vec::new();
    for path in &sources {
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| EXEMPT.contains(&name))
        {
            continue;
        }
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        let count = literal_count(&source);
        if count > 0 {
            offenders.push(format!("{} ({count})", path.display()));
        }
    }

    assert!(
        offenders.is_empty(),
        "HealthFinding must be built through HealthFinding::new, which checks that a severity \
         naming an axis agrees with its code's class and gives a newly added field one place to \
         reach every producer. Struct literal(s) found in:\n  {}",
        offenders.join("\n  ")
    );
}

/// The gate is only worth having if it can actually see a literal.
#[test]
fn the_gate_detects_a_literal_and_ignores_the_shapes_that_are_not_one() {
    assert_eq!(literal_count("let f = HealthFinding { code: c };"), 1);
    assert_eq!(literal_count("pub struct HealthFinding {"), 0);
    assert_eq!(literal_count("impl HealthFinding {"), 0);
    assert_eq!(literal_count("fn make() -> HealthFinding {"), 0);
    assert_eq!(literal_count("HealthFinding::new(code, sev)"), 0);
    assert_eq!(literal_count("Some(HealthFinding {\n    code,\n})"), 1);
}
