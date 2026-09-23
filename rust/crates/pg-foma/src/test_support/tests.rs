use std::any::Any;

use super::*;

fn panic_message(result: std::thread::Result<()>) -> String {
    let payload: Box<dyn Any + Send> = result.expect_err("the assertion must panic");
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "non-string panic payload".to_string(),
        },
    }
}

#[test]
fn normalize_newlines_maps_crlf_and_lone_cr_to_lf() {
    assert_eq!(normalize_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
    assert_eq!(normalize_newlines("a\r\n\r\n"), "a\n\n");
}

#[test]
fn normalize_newlines_preserves_all_non_cr_lf_content() {
    let input = "\u{feff}πe\u{301}\0 spaces\ttabs\u{0085}\u{2028}\u{2029}\r\n";
    let expected = "\u{feff}πe\u{301}\0 spaces\ttabs\u{0085}\u{2028}\u{2029}\n";
    assert_eq!(normalize_newlines(input), expected);
}

#[test]
fn rendered_text_accepts_lf_expected_and_crlf_or_lone_cr_actual() {
    assert_rendered_text_eq("first\r\nsecond\rthird", "first\nsecond\nthird");
}

#[test]
fn rendered_text_rejects_non_newline_and_trailing_newline_drift() {
    let content = panic_message(std::panic::catch_unwind(|| {
        assert_rendered_text_eq("same\ncontent", "same\nchanged");
    }));
    assert!(content.contains("line 2, column 2"), "{content}");
    assert!(content.contains("actual context"), "{content}");
    assert!(content.contains("\\n"), "{content}");

    let trailing = panic_message(std::panic::catch_unwind(|| {
        assert_rendered_text_eq("same\n", "same");
    }));
    assert!(trailing.contains("EOF"), "{trailing}");
    assert!(trailing.contains("trailing newline"), "{trailing}");
}

#[test]
fn rendered_text_diagnostic_reports_one_based_position_and_escaped_context() {
    let content = panic_message(std::panic::catch_unwind(|| {
        assert_rendered_text_eq("header\nvalue\0X", "header\nvalue\0Y");
    }));
    assert!(content.contains("line 2, column 7"), "{content}");
    assert!(content.contains("\\u{0}"), "{content}");
    assert!(content.contains("actual context"), "{content}");
    assert!(content.contains("expected context"), "{content}");
}

#[test]
fn canonical_lf_text_normalizes_only_the_expected_fixture() {
    assert_canonical_lf_text_eq("first\nsecond\n", "first\r\nsecond\r\n");
}

#[test]
fn canonical_lf_text_rejects_crlf_actual_and_other_drift() {
    let actual_crlf = panic_message(std::panic::catch_unwind(|| {
        assert_canonical_lf_text_eq("first\r\nsecond\n", "first\nsecond\n");
    }));
    assert!(actual_crlf.contains("\\r"), "{actual_crlf}");

    let content = panic_message(std::panic::catch_unwind(|| {
        assert_canonical_lf_text_eq("{\n  \"a\": 1\n}\n", "{\n\"a\": 1\n}\n");
    }));
    assert!(content.contains("line 2"), "{content}");

    let ordering = std::panic::catch_unwind(|| {
        assert_canonical_lf_text_eq(
            "{\n  \"a\": 1,\n  \"b\": 2\n}\n",
            "{\n  \"b\": 2,\n  \"a\": 1\n}\n",
        );
    });
    assert!(ordering.is_err());

    let trailing_newline = std::panic::catch_unwind(|| {
        assert_canonical_lf_text_eq("{\n  \"a\": 1\n}", "{\n  \"a\": 1\n}\n");
    });
    assert!(trailing_newline.is_err());
}

#[test]
fn semantic_json_accepts_insignificant_formatting_key_order_and_source_line_endings() {
    assert_semantic_json_eq(
        "{\r\n  \"b\": 2,\r\n  \"a\": [true, false]\r\n}",
        "{\"a\":[true,false],\"b\":2}",
    );
}

#[test]
fn semantic_json_preserves_array_order_and_duplicate_multiplicity() {
    let reordered = panic_message(std::panic::catch_unwind(|| {
        assert_semantic_json_eq("[1, 2]", "[2, 1]");
    }));
    assert!(reordered.contains("semantic JSON mismatch"), "{reordered}");

    let multiplicity = panic_message(std::panic::catch_unwind(|| {
        assert_semantic_json_eq("[1, 1]", "[1]");
    }));
    assert!(
        multiplicity.contains("semantic JSON mismatch"),
        "{multiplicity}"
    );
}

#[test]
fn semantic_json_keeps_numeric_representation_and_decoded_newlines_significant() {
    let number = panic_message(std::panic::catch_unwind(|| {
        assert_semantic_json_eq("1", "1.0");
    }));
    assert!(number.contains("semantic JSON mismatch"), "{number}");

    let decoded_crlf = panic_message(std::panic::catch_unwind(|| {
        assert_semantic_json_eq(r#"{"text":"\r\n"}"#, r#"{"text":"\n"}"#);
    }));
    assert!(
        decoded_crlf.contains("semantic JSON mismatch"),
        "{decoded_crlf}"
    );
}

#[test]
fn semantic_json_rejects_recursive_duplicate_keys_on_expected_and_actual() {
    let expected = panic_message(std::panic::catch_unwind(|| {
        assert_semantic_json_eq("{}", r#"{"outer":{"key":1,"key":2}}"#);
    }));
    assert!(
        expected.contains("expected JSON duplicate object key"),
        "{expected}"
    );

    let actual = panic_message(std::panic::catch_unwind(|| {
        assert_semantic_json_eq(r#"{"outer":{"key":1,"key":2}}"#, "{}");
    }));
    assert!(
        actual.contains("actual JSON duplicate object key"),
        "{actual}"
    );
}

#[test]
fn semantic_json_parse_diagnostics_identify_side_and_position() {
    let expected = panic_message(std::panic::catch_unwind(|| {
        assert_semantic_json_eq("{}", "{\n  \"key\": 1,\n}");
    }));
    assert!(
        expected.contains("expected JSON parse failure"),
        "{expected}"
    );
    assert!(expected.contains("line 3"), "{expected}");
    assert!(expected.contains("column"), "{expected}");

    let actual = panic_message(std::panic::catch_unwind(|| {
        assert_semantic_json_eq("{\n  \"key\": 1,\n}", "{}");
    }));
    assert!(actual.contains("actual JSON parse failure"), "{actual}");
    assert!(actual.contains("line 3"), "{actual}");
    assert!(actual.contains("column"), "{actual}");
}

#[test]
fn semantic_json_value_diagnostic_is_stably_pretty_printed() {
    let content = panic_message(std::panic::catch_unwind(|| {
        assert_semantic_json_eq("{\"b\":2,\"a\":1}", "{\"a\":1,\"b\":3}");
    }));
    assert!(content.contains("actual:\n{\n  \"a\": 1,"), "{content}");
    assert!(content.contains("expected:\n{\n  \"a\": 1,"), "{content}");
}
