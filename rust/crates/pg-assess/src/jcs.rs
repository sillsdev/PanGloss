//! RFC 8785 JSON Canonicalization Scheme.
//!
//! Every digest in this crate hashes JCS bytes, so two artifacts that differ only in key order or
//! whitespace hash alike.
//!
//! ## No floating point
//!
//! RFC 8785 §3.2.2.3 defines number serialization by ECMAScript `Number::toString`, whose exponent
//! and rounding rules are genuinely fiddly to reproduce. Rather than ship a subtly wrong
//! implementation under a digest that is supposed to be authoritative, canonical assessment
//! artifacts contain **no floating-point values at all** — durations are integer microseconds, and
//! every other quantity is a count. `canonicalize` returns `JcsError::FloatNotPermitted` rather
//! than guessing. If a future artifact genuinely needs a real number, that decision comes with the
//! obligation to implement ECMAScript number formatting and prove it against the RFC's test
//! vectors.

use std::fmt;

use serde_json::{Map, Value};

#[derive(Debug, PartialEq, Eq)]
pub enum JcsError {
    /// A float or non-integer number reached canonicalization. See the module doc.
    FloatNotPermitted(String),
}

impl fmt::Display for JcsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JcsError::FloatNotPermitted(at) => write!(
                f,
                "canonical artifacts contain no floating-point values (found {at}); \
                 durations are integer microseconds"
            ),
        }
    }
}

/// Canonicalize `value` to RFC 8785 bytes.
pub fn canonicalize(value: &Value) -> Result<String, JcsError> {
    let mut out = String::new();
    write_value(value, &mut out)?;
    Ok(out)
}

fn write_value(value: &Value, out: &mut String) -> Result<(), JcsError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else {
                return Err(JcsError::FloatNotPermitted(n.to_string()));
            }
        }
        Value::String(s) => write_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => write_object(map, out)?,
    }
    Ok(())
}

fn write_object(map: &Map<String, Value>, out: &mut String) -> Result<(), JcsError> {
    // RFC 8785 §3.2.3 sorts keys by UTF-16 code units, not Rust's byte-wise UTF-8 order.
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort_by(|a, b| utf16_cmp(a, b));

    out.push('{');
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_string(key, out);
        out.push(':');
        write_value(&map[*key], out)?;
    }
    out.push('}');
    Ok(())
}

fn utf16_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_keys_sort_and_whitespace_is_stripped() {
        let a = serde_json::from_str::<Value>(r#"{ "b": 1, "a": 2 }"#).unwrap();
        let b = serde_json::from_str::<Value>(r#"{"a":2,"b":1}"#).unwrap();
        assert_eq!(canonicalize(&a).unwrap(), canonicalize(&b).unwrap());
        assert_eq!(canonicalize(&a).unwrap(), r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn array_order_is_preserved() {
        // Arrays are ordered sequences; canonicalization must not reorder them.
        let v = json!([3, 1, 2]);
        assert_eq!(canonicalize(&v).unwrap(), "[3,1,2]");
    }

    #[test]
    fn astral_keys_sort_by_utf16_not_utf8() {
        // U+10000 sorts before U+E000 in UTF-16 but after it in UTF-8 byte order.
        let mut map = Map::new();
        map.insert("\u{10000}".to_string(), json!(1));
        map.insert("\u{e000}".to_string(), json!(2));
        let out = canonicalize(&Value::Object(map)).unwrap();
        let astral_at = out.find('\u{10000}').unwrap();
        let bmp_at = out.find('\u{e000}').unwrap();
        assert!(
            astral_at < bmp_at,
            "astral key must sort first under UTF-16 ordering: {out}"
        );
    }

    #[test]
    fn short_escapes_are_used_where_they_exist() {
        // Quote, backslash, newline and tab all have single-letter escapes.
        let v = json!({ "k": "a\"b\\c\nd\te" });
        assert_eq!(canonicalize(&v).unwrap(), r#"{"k":"a\"b\\c\nd\te"}"#);
    }

    #[test]
    fn backspace_and_formfeed_use_their_short_escapes() {
        let v = json!({ "k": "\u{8}\u{c}" });
        assert_eq!(canonicalize(&v).unwrap(), r#"{"k":"\b\f"}"#);
    }

    #[test]
    fn c0_controls_without_a_short_escape_use_the_six_character_form() {
        let v = json!({ "k": "\u{1}\u{1f}" });
        // Built from code points so the hex stays lowercase, four digits, per RFC 8785 §3.2.2.2.
        let expected = format!("{{\"k\":\"\\u{:04x}\\u{:04x}\"}}", 0x1, 0x1f);
        assert_eq!(canonicalize(&v).unwrap(), expected);
    }

    #[test]
    fn del_and_high_controls_are_emitted_literally() {
        // JCS escapes only C0 controls below 0x20; U+007F and U+0085 pass through unescaped.
        let v = json!({ "k": "\u{7f}\u{85}" });
        assert_eq!(canonicalize(&v).unwrap(), "{\"k\":\"\u{7f}\u{85}\"}");
    }

    #[test]
    fn non_ascii_is_not_escaped() {
        // JCS emits UTF-8 literals rather than \u escapes for printable non-ASCII.
        let v = json!({ "k": "ñ\u{10000}" });
        assert_eq!(canonicalize(&v).unwrap(), "{\"k\":\"ñ\u{10000}\"}");
    }

    #[test]
    fn integers_round_trip_at_the_extremes() {
        let v = json!({ "min": i64::MIN, "max": u64::MAX, "zero": 0 });
        assert_eq!(
            canonicalize(&v).unwrap(),
            format!("{{\"max\":{},\"min\":{},\"zero\":0}}", u64::MAX, i64::MIN)
        );
    }

    #[test]
    fn floats_are_refused_rather_than_guessed() {
        let v = json!({ "elapsed": 1.5 });
        assert_eq!(
            canonicalize(&v),
            Err(JcsError::FloatNotPermitted("1.5".to_string()))
        );
    }

    #[test]
    fn nested_objects_canonicalize_recursively() {
        let a = serde_json::from_str::<Value>(r#"{"o":{"z":1,"a":[{"y":1,"x":2}]}}"#).unwrap();
        assert_eq!(
            canonicalize(&a).unwrap(),
            r#"{"o":{"a":[{"x":2,"y":1}],"z":1}}"#
        );
    }
}
