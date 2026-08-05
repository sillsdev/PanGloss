//! Fail-closed access to the PRIVATE real-language corpora under `samples/data/`, declared by
//! `rust/tools/corpus-manifest.json`.
//!
//! # The false-success path this closes
//! The corpora are intentionally untracked, and corpus-dependent tests are `#[ignore]`d. The
//! conventional guard is skip-if-absent:
//!
//! ```ignore
//! let Some(g) = load_grammar() else {
//!     eprintln!("skipping: corpus not present");
//!     return;                 // <-- reports SUCCESS having measured nothing
//! };
//! ```
//!
//! In a fresh worktree `samples/data/` does not exist at all, so every such test passes while
//! testing no corpus data. That is not hypothetical: it happened repeatedly in this repo's own
//! agent worktrees, and it is the second false-success path this module exists to close.
//!
//! # The contract
//! `required` reads `PANGLOSS_CORPUS_REQUIRED`, which `rust/tools/pg.ps1 -Mode corpus-test` sets
//! after validating every required manifest path. When it is on, `path` PANICS on a missing input
//! instead of returning `None`, so a corpus suite cannot exit successfully with missing inputs.
//! When it is off, behaviour is unchanged and ordinary fixture-independent runs are unaffected.
//!
//! Tests should also call `record_cases` with the number of corpus cases they actually executed.
//! The front end rejects a successful cargo exit whose total executed-case count is zero — a suite
//! that compiles, runs, and exercises nothing is a failure, not a pass.

use std::path::{Path, PathBuf};

/// Env var the managed `corpus-test` mode sets. Any value other than `0`/`false`/empty enables
/// required mode.
pub const REQUIRED_ENV: &str = "PANGLOSS_CORPUS_REQUIRED";

/// Prefix of the machine-readable line `record_cases` emits.
pub const CASE_COUNT_PREFIX: &str = "PANGLOSS_CORPUS_CASES";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The manifest's `corpus_root`, overridable by `PANGLOSS_CORPUS_ROOT` so a linked worktree can
/// point at an external corpus instead of copying gigabytes per worktree.
pub fn corpus_root() -> PathBuf {
    match std::env::var("PANGLOSS_CORPUS_ROOT") {
        Ok(root) if !root.trim().is_empty() => PathBuf::from(root),
        _ => repo_root().join("samples").join("data"),
    }
}

/// Whether a missing corpus input must be a hard failure rather than a skip.
pub fn required() -> bool {
    match std::env::var(REQUIRED_ENV) {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            !(v.is_empty() || v == "0" || v == "false")
        }
        Err(_) => false,
    }
}

/// A corpus file's path when present.
///
/// # Panics
/// When `required` is on and the file is absent — the whole point of this module. The message
/// names the file, the resolved root, and how the caller got into required mode, because the usual
/// symptom otherwise is a mysteriously-passing suite.
pub fn path(relative: &str) -> Option<PathBuf> {
    let candidate = corpus_root().join(relative);
    if candidate.is_file() {
        return Some(candidate);
    }
    if required() {
        panic!(
            "required corpus input missing: {relative}\n  \
             resolved to: {}\n  \
             corpus root: {} (override with PANGLOSS_CORPUS_ROOT)\n  \
             {REQUIRED_ENV} is set, so this is a hard failure rather than a skip. Either provide \
             the corpus or do not run this suite in corpus-required mode.",
            candidate.display(),
            corpus_root().display()
        );
    }
    None
}

/// `path`, but always a hard failure when absent regardless of required mode. Use in a test that
/// is meaningless without its corpus and is only ever run deliberately.
pub fn require(relative: &str) -> PathBuf {
    let candidate = corpus_root().join(relative);
    assert!(
        candidate.is_file(),
        "corpus input absent: {relative}\n  resolved to: {}\n  \
         This test was requested explicitly, so it fails rather than reporting a pass it did not \
         earn. Corpus root override: PANGLOSS_CORPUS_ROOT.",
        candidate.display()
    );
    candidate
}

/// Emit the machine-readable executed-case count the managed front end checks.
///
/// `label` should identify the suite (typically the test-function name). Call it once per test with
/// the number of corpus cases actually exercised — words analyzed, grammars compiled, whatever the
/// suite's unit is. Zero is legal to report and is exactly what the front end rejects.
pub fn record_cases(label: &str, cases: usize) {
    println!("{CASE_COUNT_PREFIX} {label} {cases}");
}

// ---------------------------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------------------------

/// Word-list hazard metadata for a `role: "corpus"` file, so a reader (or a future automated
/// slicer) does not have to rediscover hazards already found by hand -- e.g. the amharic-words.txt
/// bug where `head -3` silently returns three unanalyzable English glosses instead of a
/// build/recall failure.
///
/// This is descriptive metadata, not an enforced contract: nothing in this crate currently slices
/// a word list using it. It exists so the facts (line endings, a non-word header, an expected
/// analyzable count) live next to the file they describe instead of only in a point-in-time audit
/// doc that can drift.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WordList {
    /// Exact line-ending discipline the file is expected to have: `"LF"`, `"CRLF"`, or `"mixed"`.
    pub line_ending: String,
    /// Number of leading lines that are NOT surface words (e.g. an English-gloss header) and must
    /// be skipped before treating "the first N lines" as a word sample. Zero when the file has no
    /// such header.
    #[serde(default)]
    pub skip_leading_lines: u32,
    /// Free-text account of known hazards beyond the header: alphabet-invalid characters (a
    /// symbol the grammar's char table does not define), embedded gloss/metalinguistic
    /// contamination, or anything else that makes a line an implausible surface word. Required
    /// (non-empty) whenever `skip_leading_lines` is nonzero, so the skip is never undocumented.
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CorpusFile {
    pub path: String,
    pub role: String,
    pub required: bool,
    /// Present only for `role: "corpus"` files; see `WordList`.
    #[serde(default)]
    pub word_list: Option<WordList>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CorpusEntry {
    pub logical_name: String,
    pub purpose: String,
    pub files: Vec<CorpusFile>,
    #[serde(default)]
    pub requiring_tests: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Manifest {
    pub schema_version: u16,
    pub corpus_root: String,
    pub corpora: Vec<CorpusEntry>,
}

pub fn manifest_path() -> PathBuf {
    repo_root()
        .join("rust")
        .join("tools")
        .join("corpus-manifest.json")
}

/// Parse the committed manifest. Errors are returned rather than panicked so a validation test can
/// assert on them.
pub fn load_manifest() -> Result<Manifest, String> {
    let path = manifest_path();
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Structural checks that hold with or without any corpus present, so they run in CI.
pub fn validate_manifest(m: &Manifest) -> Vec<String> {
    let mut problems = Vec::new();
    if m.schema_version != 1 {
        problems.push(format!("unsupported schema_version {}", m.schema_version));
    }
    if m.corpus_root.trim().is_empty() {
        problems.push("corpus_root is empty".to_owned());
    }
    if m.corpora.is_empty() {
        problems.push("manifest declares no corpora".to_owned());
    }
    let mut seen_names = std::collections::BTreeSet::new();
    let mut seen_paths = std::collections::BTreeSet::new();
    for c in &m.corpora {
        if !seen_names.insert(c.logical_name.clone()) {
            problems.push(format!("duplicate logical_name {}", c.logical_name));
        }
        if c.purpose.trim().len() < 20 {
            problems.push(format!(
                "{}: purpose must actually explain what the corpus is for",
                c.logical_name
            ));
        }
        if c.files.is_empty() {
            problems.push(format!("{}: declares no files", c.logical_name));
        }
        if !c.files.iter().any(|f| f.required) {
            problems.push(format!(
                "{}: no required file -- an all-optional corpus cannot fail closed",
                c.logical_name
            ));
        }
        if c.requiring_tests.is_empty() {
            problems.push(format!(
                "{}: no requiring_tests -- a reader cannot tell what breaks when it is missing",
                c.logical_name
            ));
        }
        for f in &c.files {
            if !seen_paths.insert(f.path.clone()) {
                problems.push(format!("duplicate file path {}", f.path));
            }
            let p = Path::new(&f.path);
            if p.is_absolute() || f.path.contains("..") {
                problems.push(format!(
                    "{}: file path must be relative to corpus_root and must not escape it: {}",
                    c.logical_name, f.path
                ));
            }
            if f.role.trim().is_empty() {
                problems.push(format!("{}: file {} has no role", c.logical_name, f.path));
            }
            if let Some(wl) = &f.word_list {
                if f.role != "corpus" {
                    problems.push(format!(
                        "{}: file {} has a word_list but role is {:?}, not \"corpus\"",
                        c.logical_name, f.path, f.role
                    ));
                }
                if !matches!(wl.line_ending.as_str(), "LF" | "CRLF" | "mixed") {
                    problems.push(format!(
                        "{}: file {} word_list.line_ending must be LF, CRLF, or mixed, got {:?}",
                        c.logical_name, f.path, wl.line_ending
                    ));
                }
                if wl.skip_leading_lines > 0 && wl.notes.trim().is_empty() {
                    problems.push(format!(
                        "{}: file {} has skip_leading_lines > 0 but no notes explaining why",
                        c.logical_name, f.path
                    ));
                }
            }
        }
    }
    problems
}

/// Every required manifest path missing under `root`; empty means all are present. Pinned by
/// `a_missing_required_file_is_reported_against_a_synthetic_manifest`.
pub fn missing_required_under(m: &Manifest, root: &Path) -> Vec<String> {
    let mut missing = Vec::new();
    for c in &m.corpora {
        for f in c.files.iter().filter(|f| f.required) {
            if !root.join(&f.path).is_file() {
                missing.push(format!("{}:{}", c.logical_name, f.path));
            }
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic() -> Manifest {
        Manifest {
            schema_version: 1,
            corpus_root: "samples/data".into(),
            corpora: vec![CorpusEntry {
                logical_name: "synthetic".into(),
                purpose: "a synthetic corpus used only to prove fail-closed behaviour".into(),
                files: vec![
                    CorpusFile {
                        path: "present.txt".into(),
                        role: "corpus".into(),
                        required: true,
                        word_list: None,
                    },
                    CorpusFile {
                        path: "absent.txt".into(),
                        role: "grammar".into(),
                        required: true,
                        word_list: None,
                    },
                ],
                requiring_tests: vec!["synthetic-suite".into()],
            }],
        }
    }

    #[test]
    fn the_committed_manifest_parses_and_validates() {
        let m = load_manifest().expect("committed manifest must parse");
        let problems = validate_manifest(&m);
        assert!(problems.is_empty(), "manifest problems: {problems:?}");
        // The four real grammars this repo's perf and recall tracks depend on.
        for expected in ["indonesian", "sena", "amharic", "aweti"] {
            assert!(
                m.corpora.iter().any(|c| c.logical_name == expected),
                "manifest must declare the {expected} corpus"
            );
        }
    }

    /// CI proves fail-closed with a synthetic manifest and intentionally missing files, needing no
    /// private corpus (design doc: "CI does not need or receive private corpus data").
    #[test]
    fn a_missing_required_file_is_reported_against_a_synthetic_manifest() {
        let dir = std::env::temp_dir().join(format!("pg-corpus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("present.txt"), b"x").unwrap();
        let _ = std::fs::remove_file(dir.join("absent.txt"));

        let missing = missing_required_under(&synthetic(), &dir);
        assert_eq!(missing, vec!["synthetic:absent.txt".to_owned()]);

        std::fs::write(dir.join("absent.txt"), b"y").unwrap();
        assert!(missing_required_under(&synthetic(), &dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validation_rejects_the_shapes_that_would_defeat_the_gate() {
        let mut m = synthetic();
        m.corpora[0]
            .files
            .iter_mut()
            .for_each(|f| f.required = false);
        assert!(validate_manifest(&m)
            .iter()
            .any(|p| p.contains("cannot fail closed")));

        let mut m = synthetic();
        m.corpora[0].requiring_tests.clear();
        assert!(validate_manifest(&m)
            .iter()
            .any(|p| p.contains("no requiring_tests")));

        let mut m = synthetic();
        m.corpora[0].files[0].path = "../escape.txt".into();
        assert!(validate_manifest(&m)
            .iter()
            .any(|p| p.contains("escape it")));

        let mut m = synthetic();
        m.corpora.push(m.corpora[0].clone());
        assert!(validate_manifest(&m)
            .iter()
            .any(|p| p.contains("duplicate logical_name")));
    }

    #[test]
    fn word_list_metadata_is_validated() {
        // Bad line_ending value.
        let mut m = synthetic();
        m.corpora[0].files[0].word_list = Some(WordList {
            line_ending: "CR".into(),
            skip_leading_lines: 0,
            notes: String::new(),
        });
        assert!(validate_manifest(&m)
            .iter()
            .any(|p| p.contains("line_ending must be LF, CRLF, or mixed")));

        // skip_leading_lines > 0 with no explanation.
        let mut m = synthetic();
        m.corpora[0].files[0].word_list = Some(WordList {
            line_ending: "CRLF".into(),
            skip_leading_lines: 4,
            notes: String::new(),
        });
        assert!(validate_manifest(&m)
            .iter()
            .any(|p| p.contains("no notes explaining why")));

        // Valid word_list on a role: "corpus" file passes cleanly.
        let mut m = synthetic();
        m.corpora[0].files[0].word_list = Some(WordList {
            line_ending: "CRLF".into(),
            skip_leading_lines: 4,
            notes: "first 4 lines are an English-gloss header, not surface words".into(),
        });
        assert!(validate_manifest(&m).is_empty());

        // word_list on a non-"corpus"-role file is rejected.
        let mut m = synthetic();
        m.corpora[0].files[1].word_list = Some(WordList {
            line_ending: "LF".into(),
            skip_leading_lines: 0,
            notes: String::new(),
        });
        assert!(validate_manifest(&m)
            .iter()
            .any(|p| p.contains("has a word_list but role is")));
    }

    #[test]
    fn required_mode_reads_the_env_var_in_both_directions() {
        // Serialized within one test rather than split across two: cargo runs tests in one process
        // with shared env, so two tests toggling the same var would race.
        let restore = std::env::var(REQUIRED_ENV).ok();
        for (value, expected) in [
            ("1", true),
            ("true", true),
            ("yes", true),
            ("0", false),
            ("false", false),
            ("", false),
        ] {
            std::env::set_var(REQUIRED_ENV, value);
            assert_eq!(required(), expected, "value {value:?}");
        }
        std::env::remove_var(REQUIRED_ENV);
        assert!(!required());
        if let Some(v) = restore {
            std::env::set_var(REQUIRED_ENV, v);
        }
    }
}
