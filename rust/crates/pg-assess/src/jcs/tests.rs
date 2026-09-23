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
