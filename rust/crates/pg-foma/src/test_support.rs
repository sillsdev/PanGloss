//! Shared assertions for crate test targets and test-support-enabled consumers.

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

/// Assert equality after normalizing source line endings in rendered output.
#[track_caller]
pub fn assert_rendered_text_eq(actual: &str, expected: &str) {
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

/// Assert that `actual` is canonical LF text matching line-ending-normalized `expected`.
#[track_caller]
pub fn assert_canonical_lf_text_eq(actual: &str, expected: &str) {
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

/// Assert JSON equality while preserving numeric, array-order, and duplicate-key significance.
#[track_caller]
pub fn assert_semantic_json_eq(actual: &str, expected: &str) {
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

#[cfg(test)]
mod tests;
