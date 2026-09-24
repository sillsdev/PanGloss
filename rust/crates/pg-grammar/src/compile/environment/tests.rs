use super::literal_text_elements;

#[test]
fn literal_text_elements_excludes_natural_classes_stem_placeholder_and_anchors() {
    assert_eq!(literal_text_elements("/[V]q_#"), vec!["q".to_string()]);
}
