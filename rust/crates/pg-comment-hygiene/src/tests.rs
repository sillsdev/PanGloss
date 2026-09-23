use super::*;

#[test]
fn masks_delimited_power_shell_and_directives() {
    let lines = ["#Requires -Version 7", "<#", "2026-09-21", "#>", "code"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(
        comment_line_mask(&lines, ".ps1"),
        vec![false, true, true, true, false]
    );
}

#[test]
fn masks_python_docstring_and_shebang() {
    let lines = [
        "#!/usr/bin/env python",
        "def f():",
        "    \"\"\"",
        "    note",
        "    \"\"\"",
        "    return 1",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    assert_eq!(
        comment_line_mask(&lines, ".py"),
        vec![false, false, true, true, true, false]
    );
}

#[test]
fn code_portion_ignores_comment_token_in_string() {
    assert_eq!(
        code_portion(r#"let url = "http://x"; // note"#, "//"),
        r#"let url = "http://x";"#
    );
    assert_eq!(code_portion("  value # note  ", "#"), "value");
}

#[test]
fn batch_response_uses_same_mask_and_code_classifier() {
    let response = classify_batch(ClassifyRequest {
        extension: ".rs".to_owned(),
        lines: vec![
            "let x = 1; // note".to_owned(),
            "// date 2026-09-21".to_owned(),
        ],
        token: None,
    });
    assert!(response.supported);
    assert_eq!(response.mask, vec![false, true]);
    assert_eq!(response.code, vec!["let x = 1;".to_owned(), "".to_owned()]);
}
