use pg_conformance_fixtures::case_set::{parse_case_set, source_sha256, CaseSetError};

fn document(source: &str, cases: &str, declared_count: usize) -> String {
    format!(
        r#"{{
          "schema": "pangloss.conformance-case-set",
          "schemaVersion": 1,
          "caseSetId": "synthetic-v1",
          "source": "words.txt",
          "sourceSha256": "{}",
          "declaredCount": {},
          "cases": {}
        }}"#,
        source_sha256(source.as_bytes()),
        declared_count,
        cases
    )
}

#[test]
fn parses_and_verifies_exact_source_lines_and_hash() {
    let source = "alpha\r\nbeta\r\n";
    let json = document(
        source,
        r#"[
          {"caseId":"case-1","sourceLine":1,"input":"alpha"},
          {"caseId":"case-2","sourceLine":2,"input":"beta"}
        ]"#,
        2,
    );
    let set = parse_case_set(&json).expect("valid case set");
    set.verify_source(source.as_bytes())
        .expect("source matches");
}

#[test]
fn rejects_duplicate_ids_ordinals_unstable_order_and_wrong_count() {
    let source = "a\nb\n";
    let duplicate_id = document(
        source,
        r#"[{"caseId":"same","sourceLine":1,"input":"a"},{"caseId":"same","sourceLine":2,"input":"b"}]"#,
        2,
    );
    assert!(matches!(
        parse_case_set(&duplicate_id),
        Err(CaseSetError::DuplicateCaseId { .. })
    ));

    let duplicate_line = document(
        source,
        r#"[{"caseId":"a","sourceLine":1,"input":"a"},{"caseId":"b","sourceLine":1,"input":"a"}]"#,
        2,
    );
    assert!(matches!(
        parse_case_set(&duplicate_line),
        Err(CaseSetError::DuplicateSourceLine { .. })
    ));

    let wrong_count = document(source, r#"[{"caseId":"a","sourceLine":1,"input":"a"}]"#, 2);
    assert!(matches!(
        parse_case_set(&wrong_count),
        Err(CaseSetError::DeclaredCountMismatch { .. })
    ));
}

#[test]
fn rejects_changed_source_bytes_and_changed_source_text() {
    let source = "alpha\nbeta\n";
    let json = document(
        source,
        r#"[{"caseId":"case-1","sourceLine":1,"input":"alpha"},{"caseId":"case-2","sourceLine":2,"input":"beta"}]"#,
        2,
    );
    let set = parse_case_set(&json).expect("valid case set");
    assert!(matches!(
        set.verify_source(b"changed\nbeta\n"),
        Err(CaseSetError::SourceHashMismatch { .. })
    ));

    let changed_text = document(
        source,
        r#"[{"caseId":"case-1","sourceLine":1,"input":"changed"},{"caseId":"case-2","sourceLine":2,"input":"beta"}]"#,
        2,
    );
    let changed = parse_case_set(&changed_text).expect("valid case set shape");
    assert!(matches!(
        changed.verify_source(source.as_bytes()),
        Err(CaseSetError::SourceLineTextMismatch { .. })
    ));
}
