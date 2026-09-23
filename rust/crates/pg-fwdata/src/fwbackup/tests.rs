use super::*;

const LDML: &str = r#"<?xml version="1.0"?><ldml><identity><language type="mgz"/></identity>
<characters><exemplarCharacters>[ABD-PR-WYabd-pr-wy\u0190\u0254{CH}{a\u0303}{ch}]</exemplarCharacters></characters></ldml>"#;

#[test]
fn exemplars_expand_ranges_escapes_and_braced_clusters() {
    let ex = exemplar_characters_from_ldml(LDML);
    assert!(ex.contains(&"a".to_string()));
    assert!(ex.contains(&"d".to_string()));
    assert!(ex.contains(&"e".to_string()), "range d-p includes e");
    assert!(ex.contains(&"p".to_string()));
    assert!(!ex.contains(&"q".to_string()), "q is outside every range");
    assert!(ex.contains(&"\u{0190}".to_string()));
    assert!(ex.contains(&"CH".to_string()));
    assert!(ex.contains(&"a\u{0303}".to_string()));
    assert!(!ex
        .iter()
        .any(|s| s.contains('{') || s.contains('}') || s.contains('[')));
}

#[test]
fn missing_characters_element_yields_empty() {
    assert!(exemplar_characters_from_ldml("<ldml/>").is_empty());
}

#[test]
fn nfd_is_applied() {
    let ex = exemplar_characters_from_ldml(
        "<ldml><characters><exemplarCharacters>[\u{00E9}]</exemplarCharacters></characters></ldml>",
    );
    assert_eq!(ex, vec!["e\u{0301}".to_string()]);
}
