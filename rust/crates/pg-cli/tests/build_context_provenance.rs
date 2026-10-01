use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

fn run_git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .expect("launch git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn prepare_inputs(directory: &Path, grammar_xml: &str, word: &str) {
    fs::create_dir_all(directory).expect("create invocation directory");
    fs::write(directory.join("grammar.xml"), grammar_xml).expect("write grammar");
    fs::write(directory.join("words.txt"), format!("{word}\n")).expect("write words");
}

fn run_stats(directory: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .current_dir(directory)
        .args([
            "batch",
            "grammar.xml",
            "words.txt",
            "out.tsv",
            "--threads",
            "1",
            "--stats",
            "--cache",
            "cache.sqlite3",
        ])
        .output()
        .expect("launch built pangloss executable");
    assert!(
        output.status.success(),
        "pangloss batch failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let cache = rusqlite::Connection::open(directory.join("cache.sqlite3"))
        .expect("open stats cache written by child process");
    cache
        .query_row(
            "SELECT build_info FROM run ORDER BY run_id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("stats run stores executable build info")
}

#[test]
fn stats_persists_the_executable_build_revision_from_outside_its_checkout() {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!(
        "pangloss-build-context-test-{}-{id}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create test directory");

    let fixture = pg_conformance_fixtures::discover()
        .into_iter()
        .find(|fixture| {
            fixture.category == "languages" && fixture.name == "metathesis-phase-isolation"
        })
        .expect("HC grammar fixture is discoverable");
    let word = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .find(|word| !word.expect_skip)
        .expect("fixture has a required word")
        .word;
    let grammar_xml = fixture.load_grammar_xml();

    let other_checkout = base.join("other-checkout");
    prepare_inputs(&other_checkout, &grammar_xml, &word);
    run_git(&other_checkout, &["init", "--quiet"]);
    run_git(&other_checkout, &["config", "user.name", "PanGloss Test"]);
    run_git(
        &other_checkout,
        &["config", "user.email", "pangloss-test@example.invalid"],
    );
    run_git(&other_checkout, &["config", "commit.gpgsign", "false"]);
    let hooks_dir = base.join("empty-hooks");
    fs::create_dir_all(&hooks_dir).expect("create empty hook directory");
    let hooks_path = hooks_dir.to_string_lossy().into_owned();
    run_git(&other_checkout, &["config", "core.hooksPath", &hooks_path]);
    run_git(&other_checkout, &["add", "grammar.xml", "words.txt"]);
    run_git(
        &other_checkout,
        &["commit", "--quiet", "-m", "external invocation context"],
    );
    let context_revision = run_git(&other_checkout, &["rev-parse", "HEAD"]);
    let from_other_checkout = run_stats(&other_checkout);
    assert_eq!(from_other_checkout, env!("PANGLOSS_BUILD_INFO"));
    assert!(
        from_other_checkout.starts_with(&format!("pangloss/{}+", env!("CARGO_PKG_VERSION"))),
        "unexpected build identity: {from_other_checkout}"
    );
    assert!(
        !from_other_checkout.contains(&context_revision),
        "persisted build provenance must not be replaced by invocation checkout {context_revision}"
    );

    let outside_checkout = base.join("outside-checkout");
    prepare_inputs(&outside_checkout, &grammar_xml, &word);
    let outside_git = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .arg("-C")
        .arg(&outside_checkout)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .expect("probe non-checkout directory");
    assert!(
        !outside_git.status.success(),
        "the second child must run outside every Git checkout"
    );
    assert_eq!(run_stats(&outside_checkout), env!("PANGLOSS_BUILD_INFO"));

    fs::remove_dir_all(base).expect("remove temporary checkout directories");
}
