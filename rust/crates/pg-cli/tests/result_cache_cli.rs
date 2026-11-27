//! `batch --result-cache` through the real CLI: its option forms, its refusals and what its eviction may delete.

#![cfg(any(unix, windows))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Scratch {
    _dir: tempfile::TempDir,
    grammar: PathBuf,
    words: PathBuf,
    out: PathBuf,
    analyses: PathBuf,
}

fn scratch() -> Scratch {
    let dir = tempfile::tempdir().unwrap();
    let grammar = dir.path().join("grammar.xml");
    let source =
        pg_conformance_fixtures::require_fixture("edge-cases", "standalone-combining-mark")
            .grammar_path();
    std::fs::copy(source, &grammar).unwrap();
    let words = dir.path().join("words.txt");
    std::fs::write(&words, "a\n").unwrap();
    Scratch {
        out: dir.path().join("out.tsv"),
        analyses: dir.path().join("a.jsonl"),
        grammar,
        words,
        _dir: dir,
    }
}

fn batch(s: &Scratch, extra: &[String], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pangloss"));
    command
        .arg("batch")
        .arg(&s.grammar)
        .arg(&s.words)
        .arg(&s.out)
        .args(extra);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("pangloss must start")
}

fn eq_forms(s: &Scratch, root: &Path, max_mib: &str) -> Vec<String> {
    vec![
        format!("--analyses={}", s.analyses.display()),
        format!("--result-cache={}", root.display()),
        format!("--result-cache-max-mib={max_mib}"),
    ]
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn result_cache_accepts_the_equals_forms_and_marks_its_root() {
    let s = scratch();
    let root = s.out.with_file_name("cache");
    for _ in 0..2 {
        let run = batch(&s, &eq_forms(&s, &root, "1"), &[]);
        assert!(run.status.success(), "{}", stderr(&run));
    }
    assert!(root.join(".pangloss-result-cache").is_file());
    assert!(stderr(&batch(&s, &eq_forms(&s, &root, "1"), &[])).contains("parsed_rows=0"));
}

#[test]
fn result_cache_refuses_a_folder_it_did_not_create_and_leaves_it_alone() {
    let s = scratch();
    let root = s.out.with_file_name("project");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src").join("main.txt"), "mine").unwrap();
    let run = batch(&s, &eq_forms(&s, &root, "1"), &[]);
    assert!(!run.status.success());
    assert!(stderr(&run).contains("not a result cache"), "{}", stderr(&run));
    assert_eq!(std::fs::read_to_string(root.join("src").join("main.txt")).unwrap(), "mine");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    assert!(!s.out.exists());
}

#[test]
fn result_cache_eviction_never_deletes_a_folder_it_did_not_create() {
    let s = scratch();
    let root = s.out.with_file_name("cache");
    let first = batch(&s, &eq_forms(&s, &root, "1"), &[]);
    assert!(first.status.success(), "{}", stderr(&first));
    let foreign = root.join("project");
    let lookalike = root.join("b".repeat(64));
    for dir in [&foreign, &lookalike] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("keep.txt"), vec![b'x'; 2 << 20]).unwrap();
    }
    std::fs::write(s.words.as_path(), "a\nb\n").unwrap();
    let run = batch(&s, &eq_forms(&s, &root, "1"), &[]);
    assert!(run.status.success(), "{}", stderr(&run));
    assert!(foreign.join("keep.txt").is_file());
    assert!(lookalike.join("keep.txt").is_file());
}

#[test]
fn result_cache_is_refused_when_a_parse_affecting_variable_is_set() {
    let s = scratch();
    let root = s.out.with_file_name("cache");
    for name in ["HC_RSL", "HC_STEP_STATS", "HC_ALLOC_STATS", "HC_WORD_STATS"] {
        let run = batch(&s, &eq_forms(&s, &root, "1"), &[(name, "1")]);
        assert!(!run.status.success(), "{name}");
        assert!(stderr(&run).contains(name), "{name}: {}", stderr(&run));
    }
    assert!(!root.exists() && !s.out.exists());
}
