use super::*;

#[test]
fn inferred_segment_has_no_authored_feature_values() {
    let q = inferred_raw_def("q", CharDefKind::Segment);
    assert!(q.feature_values.is_empty());
}

#[test]
fn inferred_id_is_deterministic_from_nfd_scalars() {
    assert_eq!(inferred_id("q"), "inferred:71");
    assert_eq!(inferred_id(&nfd("q")), inferred_id("q"));
}

#[test]
fn ascii_space_is_in_the_safe_boundary_table_but_common_punctuation_is_not() {
    let empty = HashSet::new();
    assert!(matches!(
        classify(' ', &empty, &empty),
        Classification::Boundary(InferenceEvidence::SafeBoundaryTable { version: 1 })
    ));
    for ch in ['\'', '\u{02BC}', '-', '\u{2011}', '§'] {
        assert!(
            matches!(classify(ch, &empty, &empty), Classification::Ambiguous),
            "{ch:?} must not be classifiable without authored/LDML evidence"
        );
    }
}

#[test]
fn an_exemplar_character_classifies_as_a_segment_even_if_it_would_otherwise_be_ambiguous() {
    let exemplar: HashSet<String> = ["q".to_string()].into_iter().collect();
    let empty = HashSet::new();
    assert!(matches!(
        classify('q', &exemplar, &empty),
        Classification::Segment(InferenceEvidence::LdmlExemplar)
    ));
}

#[test]
fn an_authored_boundary_representation_classifies_as_a_boundary() {
    let empty = HashSet::new();
    let authored: HashSet<String> = ["\u{2011}".to_string()].into_iter().collect();
    assert!(matches!(
        classify('\u{2011}', &empty, &authored),
        Classification::Boundary(InferenceEvidence::AuthoredBoundary)
    ));
}
