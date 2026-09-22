use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use pg_comment_hygiene::{
    classify_batch, code_portion, comment_line_mask, scan_repo, ClassifyRequest, ScanOptions,
};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("pg-comment-hygiene-{}-{stamp}", std::process::id()));
        fs::create_dir_all(path.join("rust/crates")).unwrap();
        fs::create_dir_all(path.join("rust/tools")).unwrap();
        Self(path)
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn scan(&self) -> pg_comment_hygiene::Report {
        scan_repo(
            &self.0,
            ScanOptions {
                list: true,
                list_limit: 400,
            },
        )
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn count(fixture: &Fixture, category: &str) -> usize {
    fixture.scan().counts[category]
}

#[test]
fn all_ten_categories_and_informational_counters_match_legacy_contract() {
    let fixture = Fixture::new();
    fixture.write(
        "rust/crates/sample/src/lib.rs",
        include_str!(
            "fixtures/all_ten_categories_and_informational_counters_match_legacy_contract-1.txt"
        ),
    );
    let report = fixture.scan();
    assert_eq!(report.counts["plan-reference"], 1);
    assert_eq!(report.counts["step-marker"], 1);
    assert_eq!(report.counts["wiring-status"], 1);
    assert_eq!(report.counts["date-in-comment"], 1);
    assert_eq!(report.counts["history-prose"], 1);
    assert_eq!(report.counts["impl-comment-too-long"], 1);
    assert_eq!(report.counts["unanchored-exception"], 1);
    assert_eq!(report.counts["cross-reference-claim"], 1);
    assert_eq!(report.counts["docs-link-broken"], 1);
    assert_eq!(report.counts["dead-citation"], 1);
    assert_eq!(report.api_docs_long, 1);
    assert_eq!(report.reference_backed, 1);
    assert_eq!(report.claimed_exceptions["SAFETY:"], 1);
    assert_eq!(report.total, 10);
}

#[test]
fn boundary_negative_cases_and_marker_splitting_stay_clean() {
    let fixture = Fixture::new();
    fixture.write(
        "rust/crates/sample/src/lib.rs",
        include_str!("fixtures/boundary_negative_cases_and_marker_splitting_stay_clean-2.txt"),
    );
    let report = fixture.scan();
    assert_eq!(report.total, 0);
    assert_eq!(report.reference_backed, 0);
}

#[test]
fn citations_accept_live_path_and_wrapped_prefix_but_reject_dead_names() {
    let fixture = Fixture::new();
    fixture.write(
        "rust/crates/sample/src/lib.rs",
        include_str!(
            "fixtures/citations_accept_live_path_and_wrapped_prefix_but_reject_dead_names-3.txt"
        ),
    );
    fixture.write("rust/crates/sample/src/other.rs", "fn path_test() {}\n");
    let report = fixture.scan();
    assert_eq!(report.counts["dead-citation"], 0);
    assert_eq!(report.reference_backed, 1);
}

#[test]
fn nested_public_and_private_enum_trait_docs_use_reachability() {
    let fixture = Fixture::new();
    fixture.write(
        "rust/crates/sample/src/lib.rs",
        "pub mod public; mod private;\n",
    );
    fixture.write(
        "rust/crates/sample/src/public.rs",
        include_str!("fixtures/nested_public_and_private_enum_trait_docs_use_reachability-4.txt"),
    );
    fixture.write(
        "rust/crates/sample/src/private.rs",
        include_str!("fixtures/nested_public_and_private_enum_trait_docs_use_reachability-5.txt"),
    );
    let report = fixture.scan();
    assert_eq!(report.api_docs_long, 2);
    assert_eq!(report.counts["impl-comment-too-long"], 1);
}

#[test]
fn safety_exception_requires_anchor_after_three_lines() {
    let fixture = Fixture::new();
    fixture.write(
        "rust/crates/sample/src/lib.rs",
        include_str!("fixtures/safety_exception_requires_anchor_after_three_lines-6.txt"),
    );
    assert_eq!(count(&fixture, "unanchored-exception"), 1);
    let anchored = Fixture::new();
    anchored.write(
        "rust/crates/sample/src/lib.rs",
        include_str!("fixtures/f-7.txt"),
    );
    assert_eq!(count(&anchored, "unanchored-exception"), 0);
}

#[test]
fn powershell_help_and_python_docstrings_are_interface_blocks() {
    let fixture = Fixture::new();
    fixture.write(
        "rust/tools/help.ps1",
        include_str!("fixtures/powershell_help_and_python_docstrings_are_interface_blocks-8.txt"),
    );
    fixture.write(
        ".claude/hooks/hook.py",
        include_str!("fixtures/powershell_help_and_python_docstrings_are_interface_blocks-9.txt"),
    );
    let report = fixture.scan();
    assert_eq!(report.counts["impl-comment-too-long"], 0);
    assert_eq!(report.api_docs_long, 3);
}

#[test]
fn target_tree_is_excluded() {
    let fixture = Fixture::new();
    fixture.write("rust/crates/target/bad.rs", "// 2026-09-21\n");
    assert_eq!(count(&fixture, "date-in-comment"), 0);
}

#[test]
fn utf8_bom_does_not_hide_a_comment() {
    let fixture = Fixture::new();
    fixture.write("rust/crates/sample/src/lib.rs", "\u{feff}// 2026-09-21\n");
    assert_eq!(count(&fixture, "date-in-comment"), 1);
}

#[test]
fn shared_classifier_handles_masks_code_and_unknown_extensions() {
    let lines = vec![
        "let url = \"http://x\"; // note".to_owned(),
        "// 2026-09-21".to_owned(),
    ];
    assert_eq!(comment_line_mask(&lines, ".rs"), vec![false, true]);
    assert_eq!(code_portion(&lines[0], "//"), "let url = \"http://x\";");
    let batch = classify_batch(ClassifyRequest {
        extension: ".wat".into(),
        lines: lines.clone(),
        token: None,
    });
    assert!(!batch.supported);
    assert_eq!(batch.mask, vec![false, false]);
    assert_eq!(
        batch.code,
        vec![
            "let url = \"http://x\"; // note".to_owned(),
            "// 2026-09-21".to_owned()
        ]
    );
}

#[test]
fn cli_reports_usage_and_json_errors_with_exit_two() {
    let binary = env!("CARGO_BIN_EXE_pg-comment-hygiene");
    let missing = Command::new(binary).output().unwrap();
    assert_eq!(missing.status.code(), Some(2));
    let malformed = Command::new(binary)
        .arg("--classify-json")
        .stdin(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(malformed.status.code(), Some(2));
}

#[test]
fn missing_required_scan_directory_is_a_tool_error() {
    let fixture = Fixture::new();
    fs::remove_dir(fixture.0.join("rust/tools")).unwrap();
    assert!(scan_repo(&fixture.0, ScanOptions::default()).is_err());
}

#[test]
fn powershell_function_help_and_extension_matching_are_case_insensitive() {
    let fixture = Fixture::new();
    fixture.write(
        "rust/tools/function.ps1",
        "FUNCTION Invoke-Thing {\n<#\n.DESCRIPTION\nPublic detail.\nMore detail.\n#>\n}\n",
    );
    let report = fixture.scan();
    assert_eq!(report.api_docs_long, 1);
    assert_eq!(report.total, 0);
    let response = classify_batch(ClassifyRequest {
        extension: ".RS".into(),
        lines: vec!["// note".into()],
        token: None,
    });
    assert!(response.supported);
    assert_eq!(response.mask, vec![true]);
}

#[test]
fn cli_classify_json_returns_supported_false_for_unknown_extension() {
    let binary = env!("CARGO_BIN_EXE_pg-comment-hygiene");
    let mut child = Command::new(binary)
        .arg("--classify-json")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"extension":".wat","lines":["value"]}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"supported\":false"));
}
