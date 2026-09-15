//! Every path an agent-facing document names must exist; CLAUDE.md accumulated four dead ones.
//! Rationale and the false-positive classes: docs/design/agent-doc-gates.md

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn agent_docs() -> Vec<PathBuf> {
    let mut out = vec![repo_root().join("CLAUDE.md")];
    let skills = repo_root().join(".claude/skills");
    for entry in fs::read_dir(&skills).expect("skills directory").flatten() {
        let candidate = entry.path().join("SKILL.md");
        if candidate.is_file() {
            out.push(candidate);
        }
    }
    out
}

/// Backticked tokens that look like filesystem paths; bare symbol names are deliberately excluded.
/// Why those exclusions and not others: docs/design/agent-doc-gates.md
fn path_claims(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else { break };
        let token = &after[..end];
        rest = &after[end + 1..];
        let looks_like_path = (token.contains('/') || token.contains('\\'))
            && !token.contains(' ')
            && !token.starts_with("http")
            && !token.starts_with('-')
            && token.len() > 3;
        // `a/b::c` is a Rust item path, not a filesystem one.
        if looks_like_path && !token.contains("::") {
            out.push(token.trim_end_matches(&['/', '\\'][..]).to_string());
        }
    }
    out
}

/// Only a token rooted here is unambiguously a repo path; without the allowlist, 20 false
/// positives against 4 real findings. The four shapes: docs/design/agent-doc-gates.md
const REPO_ROOTS: &[&str] = &[
    "rust/",
    "docs/",
    ".claude/",
    "conformance-staging/",
    "machine/",
    "samples/",
    "openspec/",
];

fn checkable(token: &str) -> bool {
    REPO_ROOTS.iter().any(|r| token.starts_with(r))
        && !token.contains(':')
        && !token.contains('*')
        && !token.contains('<')
}

#[test]
fn every_path_named_in_an_agent_doc_exists() {
    let root = repo_root();
    let mut dead = Vec::new();
    for doc in agent_docs() {
        let text = fs::read_to_string(&doc).expect("read agent doc");
        for token in path_claims(&text) {
            if !checkable(&token) {
                continue;
            }
            if !root.join(&token).exists() {
                dead.push(format!("{}: `{token}`", doc.display()));
            }
        }
    }
    dead.sort();
    dead.dedup();
    assert!(
        dead.is_empty(),
        "agent-facing doc(s) name {} path(s) that do not exist. A reader told to open one of these \
         finds nothing, and a recipe built from one fails. Fix the path or delete the claim:\n  {}",
        dead.len(),
        dead.join("\n  ")
    );
}

#[test]
fn the_gate_can_actually_see_a_dead_path() {
    assert_eq!(
        path_claims("see `rust/tools/pg.ps1` and `docs/design/x.md`").len(),
        2
    );
    assert!(path_claims("the `Morpher` type").is_empty());
    assert!(path_claims("`pg_rules::morph::analyze`").is_empty());
    assert!(!checkable("C:\\Users\\x\\y.exe"));
    assert!(!checkable("conformance-staging/**"));
    assert!(checkable("rust/tools/pg.ps1"));
    // The four shapes that made the first version of this gate useless.
    assert!(
        !checkable("sillsdev/machine"),
        "a GitHub slug is not a path"
    );
    assert!(
        !checkable("perf/pr494-priority-union"),
        "a git branch is not a path"
    );
    assert!(!checkable("languages/bantu-verbal"), "submodule-relative");
    assert!(!checkable("recipes/schema.json"), "skill-relative");
}
