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
fn rendered_text_helper_accepts_crlf_and_lone_cr() {
    assert_rendered_text_eq("first\r\nsecond\rthird", "first\nsecond\nthird");
}

#[test]
fn rendered_text_helper_preserves_content_and_reports_diagnostics() {
    assert_rendered_text_eq(
        "\u{feff} node-π\tvalue\u{0085}\u{2028}\u{2029}\n",
        "\u{feff} node-π\tvalue\u{0085}\u{2028}\u{2029}\r\n",
    );

    let content = panic_message(std::panic::catch_unwind(|| {
        assert_rendered_text_eq("header\nvalue\0X", "header\nvalue\0Y");
    }));
    assert!(content.contains("line 2, column 7"), "{content}");
    assert!(content.contains("\\u{0}"), "{content}");
    assert!(content.contains("actual context"), "{content}");
    assert!(content.contains("expected context"), "{content}");

    let trailing = panic_message(std::panic::catch_unwind(|| {
        assert_rendered_text_eq("same\n", "same");
    }));
    assert!(trailing.contains("EOF"), "{trailing}");
    assert!(trailing.contains("trailing newline"), "{trailing}");

    let identifier = panic_message(std::panic::catch_unwind(|| {
        assert_rendered_text_eq("node-A", "node-B");
    }));
    assert!(identifier.contains("line 1, column 6"), "{identifier}");
}

#[test]
fn rendered_text_helper_rejects_whitespace_and_unicode_drift() {
    let whitespace = panic_message(std::panic::catch_unwind(|| {
        assert_rendered_text_eq("value\tA", "value A");
    }));
    assert!(
        whitespace.contains("rendered text mismatch"),
        "{whitespace}"
    );

    let unicode = panic_message(std::panic::catch_unwind(|| {
        assert_rendered_text_eq("naïve", "naive");
    }));
    assert!(unicode.contains("rendered text mismatch"), "{unicode}");
}
