use super::*;

const VERSION: &str = "test-compiler-1";

const XML: &str =
    "<HermitCrabInput>\n  <Language>\n    <Name>X</Name>\n  </Language>\n</HermitCrabInput>\n";

#[test]
fn line_endings_move_the_source_hash_but_not_the_fingerprint() {
    // The exact case D3a exists for: one committed grammar, two checkouts.
    let lf = XML.to_string();
    let crlf = XML.replace('\n', "\r\n");

    assert_ne!(
        source_sha256(lf.as_bytes()),
        source_sha256(crlf.as_bytes()),
        "sourceSha256 identifies the file, so it must notice the bytes differ"
    );
    assert_eq!(
        model_fingerprint(SourceKind::HcXml, &lf, VERSION).unwrap(),
        model_fingerprint(SourceKind::HcXml, &crlf, VERSION).unwrap(),
        "modelFingerprint identifies what was analyzed, which did not change"
    );
}

#[test]
fn any_content_change_moves_the_fingerprint() {
    let changed = XML.replace("<Name>X</Name>", "<Name>Y</Name>");
    assert_ne!(
        model_fingerprint(SourceKind::HcXml, XML, VERSION).unwrap(),
        model_fingerprint(SourceKind::HcXml, &changed, VERSION).unwrap()
    );
}

#[test]
fn a_compiler_change_moves_the_fingerprint() {
    // Two compilers may turn identical source into different models, so the fingerprint has to name the compiler.
    assert_ne!(
        model_fingerprint(SourceKind::HcXml, XML, "compiler-1").unwrap(),
        model_fingerprint(SourceKind::HcXml, XML, "compiler-2").unwrap()
    );
}

#[test]
fn snapshot_key_order_and_whitespace_do_not_move_the_fingerprint() {
    let a = r#"{ "b": 1, "a": [2, 3] }"#;
    let b = r#"{"a":[2,3],"b":1}"#;
    assert_ne!(source_sha256(a.as_bytes()), source_sha256(b.as_bytes()));
    assert_eq!(
        model_fingerprint(SourceKind::Snapshot, a, VERSION).unwrap(),
        model_fingerprint(SourceKind::Snapshot, b, VERSION).unwrap()
    );
}

#[test]
fn snapshot_array_order_does_move_the_fingerprint() {
    // Arrays are sequences: reordering them is a content change, not formatting.
    let a = r#"{"a":[2,3]}"#;
    let b = r#"{"a":[3,2]}"#;
    assert_ne!(
        model_fingerprint(SourceKind::Snapshot, a, VERSION).unwrap(),
        model_fingerprint(SourceKind::Snapshot, b, VERSION).unwrap()
    );
}

#[test]
fn the_two_source_kinds_do_not_collide() {
    // Identical text under two source kinds compiles to different models.
    let text = "{}";
    assert_ne!(
        model_fingerprint(SourceKind::Snapshot, text, VERSION).unwrap(),
        model_fingerprint(SourceKind::HcXml, text, VERSION).unwrap()
    );
}

#[test]
fn unparsable_snapshot_still_yields_a_fingerprint() {
    // A failed assessment artifact still records what it tried to compile.
    let broken = "{ not json";
    assert!(model_fingerprint(SourceKind::Snapshot, broken, VERSION).is_ok());
}

#[test]
fn xml_attribute_order_moves_the_fingerprint_a_known_conservative_limit() {
    // Without real XML canonicalization this reports a difference that is only formatting; conservative, not silent, so the limit is visible rather than discovered.
    let a = r#"<E a="1" b="2"/>"#;
    let b = r#"<E b="2" a="1"/>"#;
    assert_ne!(
        model_fingerprint(SourceKind::HcXml, a, VERSION).unwrap(),
        model_fingerprint(SourceKind::HcXml, b, VERSION).unwrap()
    );
}
