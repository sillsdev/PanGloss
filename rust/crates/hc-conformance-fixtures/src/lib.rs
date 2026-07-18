//! Shared dual-root discovery + `words.yaml` parsing + oracle-replay helper for this repo's
//! conformance fixtures, per `docs/conformance-staging-plan.md`. One helper, used by every test
//! that walks `machine/conformance/**` and/or `conformance-staging/**`, so the path logic and the
//! `words.yaml` schema are defined exactly once rather than duplicated per test file (the plan
//! doc's explicit requirement).
//!
//! The two roots:
//! - `machine/conformance/**` — the `sillsdev/machine` submodule (`conformance-framework` branch),
//!   the eventual permanent home for every fixture.
//! - `conformance-staging/**` — this repo, committed, for fixtures that pin a bug/pathology
//!   immediately, ahead of upstream acceptance. See `docs/conformance-staging-plan.md`.
//!
//! Either root may be absent (a fresh clone with the `machine` submodule not initialized still
//! discovers staged fixtures fine); [`discover`] tolerates both independently.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Which of the two fixture roots a fixture was found under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Root {
    /// `machine/conformance/**`.
    Machine,
    /// `conformance-staging/**`.
    Staging,
}

impl Root {
    pub fn label(self) -> &'static str {
        match self {
            Root::Machine => "machine",
            Root::Staging => "staging",
        }
    }
}

/// One discovered fixture directory (holds `grammar.xml` + `words.yaml`, and — staged fixtures
/// only — `STAGING.md`).
#[derive(Debug, Clone)]
pub struct FixtureRef {
    pub root: Root,
    /// `"edge-cases"` or `"languages"`.
    pub category: String,
    pub name: String,
    pub dir: PathBuf,
}

impl FixtureRef {
    /// The `(category, name)` identity the graduation guard keys on — a fixture accepted upstream
    /// under the same category+name as a staged copy is a collision regardless of root.
    pub fn key(&self) -> (String, String) {
        (self.category.clone(), self.name.clone())
    }

    pub fn grammar_path(&self) -> PathBuf {
        self.dir.join("grammar.xml")
    }

    pub fn words_yaml_path(&self) -> PathBuf {
        self.dir.join("words.yaml")
    }

    pub fn staging_md_path(&self) -> PathBuf {
        self.dir.join("STAGING.md")
    }

    /// A stable label for assertion messages: `"<root>:<category>/<name>"`.
    pub fn label(&self) -> String {
        format!("{}:{}/{}", self.root.label(), self.category, self.name)
    }

    pub fn load_grammar_xml(&self) -> String {
        std::fs::read_to_string(self.grammar_path())
            .unwrap_or_else(|e| panic!("{}: read grammar.xml: {e}", self.dir.display()))
    }

    pub fn load_words_yaml(&self) -> WordsYaml {
        let text = std::fs::read_to_string(self.words_yaml_path())
            .unwrap_or_else(|e| panic!("{}: read words.yaml: {e}", self.dir.display()));
        serde_yaml::from_str(&text)
            .unwrap_or_else(|e| panic!("{}: parse words.yaml: {e}", self.dir.display()))
    }
}

/// Repo root, two levels up from this crate's manifest dir (`rust/crates/hc-conformance-fixtures`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub fn machine_conformance_root() -> PathBuf {
    repo_root().join("machine").join("conformance")
}

pub fn staging_root() -> PathBuf {
    repo_root().join("conformance-staging")
}

fn scan_one_root(root_dir: &Path, root: Root, out: &mut Vec<FixtureRef>) {
    if !root_dir.is_dir() {
        return;
    }
    for category in ["edge-cases", "languages"] {
        let cat_dir = root_dir.join(category);
        let Ok(read) = std::fs::read_dir(&cat_dir) else {
            continue;
        };
        let mut entries: Vec<_> = read.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir()
                && path.join("grammar.xml").is_file()
                && path.join("words.yaml").is_file()
            {
                out.push(FixtureRef {
                    root,
                    category: category.to_string(),
                    name: entry.file_name().to_string_lossy().into_owned(),
                    dir: path,
                });
            }
        }
    }
}

/// Discover every fixture under both roots (`machine/conformance/**` and
/// `conformance-staging/**`), sorted deterministically within each root/category.
pub fn discover() -> Vec<FixtureRef> {
    let mut out = Vec::new();
    scan_one_root(&machine_conformance_root(), Root::Machine, &mut out);
    scan_one_root(&staging_root(), Root::Staging, &mut out);
    out
}

/// The "graduation guard": fixture `(category, name)` identities that exist under BOTH roots.
/// Non-empty means a staged fixture has been accepted upstream (its name now also exists under
/// `machine/conformance/`) and the staged copy must be deleted in the same change — see
/// `docs/conformance-staging-plan.md`'s "graduation guard" mechanism.
pub fn graduation_guard_violations(fixtures: &[FixtureRef]) -> Vec<(String, String)> {
    use std::collections::HashSet;
    let machine_keys: HashSet<(String, String)> = fixtures
        .iter()
        .filter(|f| f.root == Root::Machine)
        .map(|f| f.key())
        .collect();
    let mut violations: Vec<(String, String)> = fixtures
        .iter()
        .filter(|f| f.root == Root::Staging && machine_keys.contains(&f.key()))
        .map(|f| f.key())
        .collect();
    violations.sort();
    violations.dedup();
    violations
}

// -------------------------------------------------------------------------------------------
// words.yaml schema (`machine/conformance/PROTOCOL.md` + `conformance/README.md`'s "Shape"
// section). Only the fields this repo's tests consume are modeled; no `deny_unknown_fields`, so
// this tolerates upstream schema additions without breaking.
// -------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct WordsYaml {
    pub language: String,
    #[serde(default)]
    pub inspired_by: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    /// Edge-cases-only: this fixture's founding-oracle run crashed — see `PROTOCOL.md`'s
    /// "expect_crash" section. A generic replay skips such fixtures (there is no signature to
    /// diff — the oracle died before producing one).
    #[serde(default)]
    pub expect_crash: bool,
    /// Edge-cases-only: C#'s own `isPathological` gate (`RunnerV2.RunOneSelfCheck`) — excluded
    /// from a default self-check run, exercised only with `--include-pathological`. A generic
    /// replay in this repo's default (<60s) suite skips these the same way.
    #[serde(default)]
    pub budget_ms: Option<u64>,
    pub words: Vec<WordEntry>,
}

impl WordsYaml {
    /// Reason to skip this fixture entirely in a generic, always-on replay test — `None` means
    /// "replay it". See the field docs above for why `expect_crash`/`budget_ms` are excluded.
    pub fn skip_in_generic_replay(&self) -> Option<&'static str> {
        if self.expect_crash {
            return Some("expect_crash fixture (no signature ground truth to diff against)");
        }
        if self.budget_ms.is_some() {
            return Some(
                "budget_ms/pathological fixture (opt-in only, mirrors C#'s isPathological gate)",
            );
        }
        None
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WordEntry {
    pub word: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub provenance: Option<String>,
    /// Well-formed input, zero valid analyses (the oracle's `-` signature).
    #[serde(default)]
    pub expect_fail: bool,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    /// The oracle throws `InvalidShapeException` — batch status `SKIPPED`, not `ok`.
    #[serde(default)]
    pub expect_skip: bool,
    #[serde(default)]
    pub exercises: Vec<String>,
    #[serde(default)]
    pub parses: Vec<ParseEntry>,
}

impl WordEntry {
    /// `PROTOCOL.md` section 3's adapter-mode omission rule: a word carrying any `guess: true`
    /// parse is invisible to adapter-mode (non-self-check) engines — `hc-rs`'s plain
    /// `Morpher::parse_word` (`guess_root: false`) included. A generic replay skips such words
    /// entirely rather than asserting on them.
    pub fn adapter_visible(&self) -> bool {
        !self.parses.iter().any(|p| p.guess)
    }

    /// The `PROTOCOL.md` section 2/3 expected `signature` column for this word: `-` if the word
    /// has no (adapter-visible) parses, else the ordinally-sorted, `;`-joined set of declared
    /// `parses[].signature` values — the same algorithm `hc_parse::ParseOutcome::signature`
    /// implements for the engine's actual output, so the two are directly comparable.
    pub fn expected_signature(&self) -> String {
        if self.parses.is_empty() {
            return "-".to_string();
        }
        let mut sigs: Vec<&str> = self.parses.iter().map(|p| p.signature.as_str()).collect();
        sigs.sort_unstable();
        sigs.join(";")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParseEntry {
    pub signature: String,
    #[serde(default)]
    pub gloss: Option<String>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub exercises: Vec<String>,
    /// A `Guesser`/`LexicalGuess` analysis (`PROTOCOL.md` section 3) — self-check-only, never
    /// producible through the adapter contract.
    #[serde(default)]
    pub guess: bool,
}

// -------------------------------------------------------------------------------------------
// Oracle replay: drives `hc_parse::Morpher` (this repo's own full engine) as the adapter, per
// `docs/conformance-staging-plan.md`'s oracle-discipline note (hc-rs IS the oracle for anything
// authored/verified against it rather than the C# founding oracle — callers/STAGING.md say which).
// -------------------------------------------------------------------------------------------

/// Replay every adapter-visible word in `words_yaml` against `morpher`, asserting the
/// `PROTOCOL.md` status+signature match (`expect_skip` -> `invalid_shape`, else the sorted-joined
/// signature). Panics (via `assert_eq!`/`assert!`) on the first mismatch, naming `fixture_label`.
/// Returns the number of words actually checked, so callers can additionally assert that count
/// against an expected total (catches a fixture silently shrinking to zero checked words).
pub fn assert_matches_oracle(
    fixture_label: &str,
    words_yaml: &WordsYaml,
    morpher: &hc_parse::Morpher,
) -> usize {
    let mut checked = 0;
    for w in &words_yaml.words {
        if !w.adapter_visible() {
            continue; // self-check-only (guess:true parse), PROTOCOL.md section 3
        }
        let outcome = morpher.parse_word(&w.word);
        if w.expect_skip {
            assert!(
                outcome.invalid_shape,
                "{fixture_label}: word {:?} expected SKIPPED (invalid shape) but engine produced a result",
                w.word
            );
            checked += 1;
            continue;
        }
        assert!(
            !outcome.invalid_shape,
            "{fixture_label}: word {:?} unexpectedly SKIPPED (invalid shape)",
            w.word
        );
        let got = outcome.signature();
        let expected = w.expected_signature();
        assert_eq!(
            got, expected,
            "{fixture_label}: word {:?} signature mismatch vs oracle",
            w.word
        );
        checked += 1;
    }
    checked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_tolerates_absent_roots() {
        // Must not panic even if neither root exists on disk in some hypothetical checkout —
        // exercised for real by pointing scan_one_root at a nonexistent directory.
        let mut out = Vec::new();
        scan_one_root(
            Path::new("/definitely/does/not/exist"),
            Root::Machine,
            &mut out,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn graduation_guard_flags_same_key_both_roots() {
        let fixtures = vec![
            FixtureRef {
                root: Root::Machine,
                category: "edge-cases".into(),
                name: "foo".into(),
                dir: PathBuf::from("machine/conformance/edge-cases/foo"),
            },
            FixtureRef {
                root: Root::Staging,
                category: "edge-cases".into(),
                name: "foo".into(),
                dir: PathBuf::from("conformance-staging/edge-cases/foo"),
            },
            FixtureRef {
                root: Root::Staging,
                category: "edge-cases".into(),
                name: "bar".into(),
                dir: PathBuf::from("conformance-staging/edge-cases/bar"),
            },
        ];
        let violations = graduation_guard_violations(&fixtures);
        assert_eq!(
            violations,
            vec![("edge-cases".to_string(), "foo".to_string())]
        );
    }

    #[test]
    fn words_yaml_parses_minimal_doc() {
        let doc = r#"
language: Test
requires: [phonology]
words:
  - word: foo
    parses:
      - signature: "M1|foo"
        rules: []
  - word: bar
    expect_fail: true
"#;
        let parsed: WordsYaml = serde_yaml::from_str(doc).unwrap();
        assert_eq!(parsed.language, "Test");
        assert_eq!(parsed.requires, vec!["phonology".to_string()]);
        assert_eq!(parsed.words.len(), 2);
        assert_eq!(parsed.words[0].expected_signature(), "M1|foo");
        assert_eq!(parsed.words[1].expected_signature(), "-");
    }
}
