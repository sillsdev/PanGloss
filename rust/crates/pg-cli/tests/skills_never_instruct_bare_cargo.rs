//! No skill may instruct an agent to run a Cargo subcommand the bare-cargo hook refuses.
//! Why it reads the hook rather than restating it: docs/design/agent-doc-gates.md

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The subcommands the hook refuses, read from the hook so the two cannot drift apart.
fn refused_subcommands() -> Vec<String> {
    let hook = fs::read_to_string(repo_root().join(".claude/hooks/block-bare-cargo.py"))
        .expect("block-bare-cargo.py is the source of truth for this gate");
    let mut out: Vec<String> = ["build", "test", "check", "run"]
        .iter()
        .filter(|s| hook.contains(*s))
        .map(|s| (*s).to_string())
        .collect();
    assert!(
        !out.is_empty(),
        "read no refused subcommands out of block-bare-cargo.py — the hook's shape changed and this \
         gate would now pass vacuously"
    );
    out.sort();
    out
}

fn skill_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let skills = repo_root().join(".claude/skills");
    for entry in fs::read_dir(&skills).expect("skills directory").flatten() {
        let candidate = entry.path().join("SKILL.md");
        if candidate.is_file() {
            out.push(candidate);
        }
    }
    assert!(!out.is_empty(), "no SKILL.md files found under {skills:?}");
    out
}

/// A fenced line is an instruction to run something; prose that merely names `cargo test` is not.
fn offending_lines(text: &str, refused: &[String]) -> Vec<String> {
    let mut fenced = false;
    let mut hits = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            continue;
        }
        let t = line.trim();
        for sub in refused {
            if t.starts_with(&format!("cargo {sub}")) || t.contains(&format!("&& cargo {sub}")) {
                hits.push(t.to_string());
            }
        }
        // The other half of the same hazard: running a built binary outside the managed run slot.
        if t.contains("target/release/pangloss") || t.contains("target\\release\\pangloss") {
            hits.push(t.to_string());
        }
    }
    hits
}

#[test]
fn no_skill_instructs_a_command_the_hook_refuses() {
    let refused = refused_subcommands();
    let mut offenders = Vec::new();
    for path in skill_files() {
        let text = fs::read_to_string(&path).expect("read skill");
        for line in offending_lines(&text, &refused) {
            offenders.push(format!("{}: {line}", path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "skill(s) instruct a command `.claude/hooks/block-bare-cargo.py` refuses, or run a built \
         binary outside `pg.ps1 -Mode run`'s run slot. Use the managed entry point instead:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn the_gate_can_actually_see_an_offending_line() {
    let refused = vec!["test".to_string()];
    assert_eq!(
        offending_lines("```\ncargo test -p pg-parse\n```", &refused).len(),
        1
    );
    assert_eq!(
        offending_lines("```\ntarget/release/pangloss batch a b c\n```", &refused).len(),
        1
    );
    assert!(offending_lines("prose mentioning cargo test inline", &refused).is_empty());
    assert!(offending_lines("```\npg.ps1 -Mode test\n```", &refused).is_empty());
}
