use std::borrow::Cow;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

pub(crate) fn normalize_newlines(input: &str) -> Cow<'_, str> {
    if !input.contains('\r') {
        return Cow::Borrowed(input);
    }

    let mut normalized = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            chars.next_if_eq(&'\n');
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }
    Cow::Owned(normalized)
}

fn first_mismatch(actual: &str, expected: &str) -> Option<usize> {
    let actual_chars: Vec<char> = actual.chars().collect();
    let expected_chars: Vec<char> = expected.chars().collect();
    let shared_len = actual_chars.len().min(expected_chars.len());
    for index in 0..shared_len {
        if actual_chars[index] != expected_chars[index] {
            return Some(index);
        }
    }
    (actual_chars.len() != expected_chars.len()).then_some(shared_len)
}

fn line_column(input: &str, char_index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for ch in input.chars().take(char_index) {
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn escaped_context(input: &str, char_index: usize) -> String {
    let chars: Vec<char> = input.chars().collect();
    if char_index >= chars.len() {
        return "<EOF>".to_string();
    }
    let start = char_index.saturating_sub(20);
    let end = (char_index + 20).min(chars.len());
    chars[start..end]
        .iter()
        .collect::<String>()
        .escape_default()
        .collect()
}

fn trailing_newline_count(input: &str) -> usize {
    input.chars().rev().take_while(|&ch| ch == '\n').count()
}

fn text_mismatch_message(kind: &str, actual: &str, expected: &str) -> String {
    let index = first_mismatch(actual, expected).expect("a mismatch is required");
    let (line, column) = line_column(actual, index);
    let at_eof = index >= actual.chars().count() || index >= expected.chars().count();
    let eof = if at_eof { " (EOF)" } else { "" };
    let trailing = if trailing_newline_count(actual) != trailing_newline_count(expected) {
        "; trailing newline difference"
    } else {
        ""
    };
    format!(
        "{kind} mismatch at line {line}, column {column}{eof}{trailing}; \
         actual context: {:?}; expected context: {:?}",
        escaped_context(actual, index),
        escaped_context(expected, index),
    )
}

#[track_caller]
pub(crate) fn assert_rendered_text_eq(actual: &str, expected: &str) {
    let actual_normalized = normalize_newlines(actual);
    let expected_normalized = normalize_newlines(expected);
    if actual_normalized != expected_normalized {
        panic!(
            "{}",
            text_mismatch_message(
                "rendered text",
                actual_normalized.as_ref(),
                expected_normalized.as_ref()
            )
        );
    }
}

#[track_caller]
pub(crate) fn assert_canonical_lf_text_eq(actual: &str, expected: &str) {
    let expected_normalized = normalize_newlines(expected);
    if actual != expected_normalized {
        panic!(
            "{}",
            text_mismatch_message("canonical LF text", actual, expected_normalized.as_ref())
        );
    }
}

struct JsonValueSeed;

impl<'de> DeserializeSeed<'de> for JsonValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(JsonValueVisitor)
    }
}

struct JsonValueVisitor;

impl<'de> Visitor<'de> for JsonValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_i128<E>(self, value: i128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_i128(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("JSON number is outside serde_json's supported range"))
    }

    fn visit_u128<E>(self, value: u128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_u128(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("JSON number is outside serde_json's supported range"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(JsonValueSeed)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if object.contains_key(&key) {
                return Err(de::Error::custom(format!("duplicate object key {key:?}")));
            }
            let value = map.next_value_seed(JsonValueSeed)?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}

fn parse_json_rejecting_duplicates(input: &str) -> Result<Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let value = JsonValueSeed.deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

#[track_caller]
fn parse_json_or_panic(side: &str, input: &str) -> Value {
    match parse_json_rejecting_duplicates(input) {
        Ok(value) => value,
        Err(error) => {
            let message = error.to_string();
            let label = if message.contains("duplicate object key") {
                format!("{side} JSON duplicate object key")
            } else {
                format!(
                    "{side} JSON parse failure at line {}, column {}",
                    error.line(),
                    error.column()
                )
            };
            panic!("{label}: {message}");
        }
    }
}

#[track_caller]
pub(crate) fn assert_semantic_json_eq(actual: &str, expected: &str) {
    let actual_value = parse_json_or_panic("actual", actual);
    let expected_value = parse_json_or_panic("expected", expected);
    if actual_value != expected_value {
        let actual_pretty =
            serde_json::to_string_pretty(&actual_value).expect("JSON values must serialize");
        let expected_pretty =
            serde_json::to_string_pretty(&expected_value).expect("JSON values must serialize");
        panic!("semantic JSON mismatch:\nactual:\n{actual_pretty}\nexpected:\n{expected_pretty}");
    }
}

pub(crate) struct PanicLocation {
    pub(crate) file: String,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

pub(crate) fn capture_panic_location<F>(assertion: F) -> PanicLocation
where
    F: FnOnce(),
{
    use std::panic::{catch_unwind, set_hook, take_hook, AssertUnwindSafe};
    use std::sync::{Arc, Mutex, OnceLock};

    static PANIC_HOOK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _hook_guard = PANIC_HOOK_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("panic-hook lock must not be poisoned");
    let location = Arc::new(Mutex::new(None));
    let location_for_hook = Arc::clone(&location);
    let previous_hook = take_hook();
    set_hook(Box::new(move |panic_info| {
        *location_for_hook
            .lock()
            .expect("panic location lock must not be poisoned") =
            panic_info.location().map(|location| {
                (
                    location.file().to_string(),
                    location.line(),
                    location.column(),
                )
            });
    }));

    let result = catch_unwind(AssertUnwindSafe(assertion));
    set_hook(previous_hook);
    assert!(result.is_err(), "the assertion must panic");

    let (file, line, column) = location
        .lock()
        .expect("panic location lock must not be poisoned")
        .take()
        .expect("panic hook must report a location");
    PanicLocation { file, line, column }
}

#[cfg(test)]
mod tests {
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
}
