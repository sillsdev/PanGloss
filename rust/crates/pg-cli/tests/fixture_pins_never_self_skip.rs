//! No test may reach a conformance fixture by hand-rolled path, or skip because one is absent.
//! The 32 guard sites this replaces, and the scope decision: docs/design/fixture-pins.md

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Excluded from its own scan: this file quotes both forbidden patterns in its diagnostics. The
/// predicates below are pure and unit-tested instead: docs/design/fixture-pins.md
const SELF: &str = "fixture_pins_never_self_skip.rs";

fn test_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let crates = repo_root().join("rust/crates");
    for krate in fs::read_dir(&crates).expect("rust/crates").flatten() {
        let dir = krate.path().join("tests");
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&dir).expect("tests dir").flatten() {
            let path = entry.path();
            let is_rs = path.extension().is_some_and(|e| e == "rs");
            let is_self = path.file_name().is_some_and(|n| n == SELF);
            if is_rs && !is_self {
                out.push(path);
            }
        }
    }
    out
}

/// The fixture layout the v1 -> v2 migration retired; it has never existed in this tree.
fn names_the_retired_v1_root(text: &str) -> bool {
    text.contains("rust/conformance/") || text.contains("../../conformance/")
}

/// Only a skip naming a FIXTURE file counts; a missing private corpus is a separate fail-closed
/// mechanism whose skips are legitimate: docs/design/fixture-pins.md
fn skips_on_a_missing_fixture(text: &str) -> bool {
    if text.contains("fn have_fixture") {
        return true;
    }
    text.lines().any(|line| {
        line.contains("not present on disk")
            && (line.contains("grammar.xml") || line.contains("words.yaml"))
    })
}

/// Baking a fixture path in breaks the BUILD when it graduates, not one test.
/// Why that is strictly worse than the runtime form: docs/design/fixture-pins.md
fn bakes_in_a_fixture_path(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    compact
        .match_indices("include_str!")
        .chain(compact.match_indices("include_bytes!"))
        .any(|(at, _)| {
            let window = &compact[at..compact.len().min(at + 300)];
            window.contains("conformance-staging/") || window.contains("machine/conformance/")
        })
}

fn label(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().to_string()
}

fn scan(predicate: fn(&str) -> bool) -> Vec<String> {
    let mut offenders: Vec<String> = test_files()
        .into_iter()
        .filter(|p| predicate(&fs::read_to_string(p).expect("read test file")))
        .map(|p| label(&p))
        .collect();
    offenders.sort();
    offenders
}

#[test]
fn no_test_names_the_retired_v1_fixture_root() {
    let offenders = scan(names_the_retired_v1_root);
    assert!(
        offenders.is_empty(),
        "test file(s) name a fixture root that does not exist. Reach fixtures through \
         `pg_conformance_fixtures::require_fixture(category, name)` (a named pin) or `discover()` \
         (a sweep); both see the real roots. Offenders:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn no_test_skips_itself_because_a_fixture_is_missing() {
    let offenders = scan(skips_on_a_missing_fixture);
    assert!(
        offenders.is_empty(),
        "test file(s) return early when a fixture is absent. A missing fixture is a failure: the \
         pin cannot run, and saying nothing is indistinguishable from passing. Use \
         `pg_conformance_fixtures::require_fixture`, which panics and lists what was discovered. \
         Offenders:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn no_test_bakes_a_fixture_into_the_binary_at_compile_time() {
    let offenders = scan(bakes_in_a_fixture_path);
    assert!(
        offenders.is_empty(),
        "test file(s) reach a fixture with `include_str!`/`include_bytes!`. That path is resolved \
         when the crate COMPILES, so a fixture graduating upstream stops the workspace building \
         instead of failing one test. Read it at run time through \
         `pg_conformance_fixtures::require_fixture`. Offenders:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn the_gate_can_actually_see_test_files() {
    let found = test_files();
    assert!(
        found.len() >= 100,
        "scanned only {} test files across rust/crates/*/tests — the layout changed and both \
         checks above now assert nothing",
        found.len()
    );
}

/// Both predicates against real shapes, so excluding `SELF` from the scan stays falsifiable.
#[test]
fn the_predicates_catch_the_real_shapes_and_spare_the_legitimate_ones() {
    assert!(names_the_retired_v1_root(
        r#".join("../../conformance/allomorphy/discontinuous-env")"#
    ));
    assert!(names_the_retired_v1_root(
        "skipping: rust/conformance/rewrite/merge absent"
    ));
    assert!(!names_the_retired_v1_root(
        r#".join("../../../machine/conformance/edge-cases/loader-isactive")"#
    ));

    assert!(skips_on_a_missing_fixture("fn have_fixture() -> bool {"));
    assert!(skips_on_a_missing_fixture(
        r#"eprintln!("skipping: deep-optional-affix-nesting/grammar.xml not present on disk");"#
    ));
    // The corpus mechanism is fail-closed elsewhere; these skips are legitimate and must stay.
    assert!(!skips_on_a_missing_fixture(
        r#"eprintln!("skipping: samples/data/sena-hc.xml not present on disk");"#
    ));
    assert!(!skips_on_a_missing_fixture(
        r#"eprintln!("skipping: Sena 3 FieldWorks project not present on disk");"#
    ));

    // The form that broke the build, including the `concat!` spelling split across lines.
    assert!(bakes_in_a_fixture_path(
        "const F: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"),\n    \
         \"/../../../conformance-staging/edge-cases/template-category-sharing/grammar.xml\"\n));"
    ));
    assert!(bakes_in_a_fixture_path(
        "load(include_str!(\n  \"../../../../machine/conformance/edge-cases/strrep-identity/grammar.xml\"\n))"
    ));
    assert!(!bakes_in_a_fixture_path(
        r#"const HELP: &str = include_str!("../docs/help.txt");"#
    ));
}
