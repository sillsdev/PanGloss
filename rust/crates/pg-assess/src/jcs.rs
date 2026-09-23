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
mod tests;
