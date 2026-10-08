use super::*;

#[test]
fn provisional_definition_has_no_authored_feature_values() {
    assert!(inferred_raw_def("q", CharDefKind::Segment)
        .feature_values
        .is_empty());
}

#[test]
fn provisional_id_is_deterministic_from_nfd_scalars() {
    assert_eq!(inferred_id(&nfd("ã")), "inferred:61-303");
    assert_eq!(inferred_id("👩‍💻"), "inferred:1f469-200d-1f4bb");
}

#[test]
fn controls_refuse_even_when_listed_as_exemplars() {
    assert!(classify("\u{1}", InferenceEvidence::LdmlExemplar, &HashSet::new()).is_none());
}

#[test]
fn ascii_space_is_a_boundary_and_common_punctuation_is_a_provisional_letter() {
    let empty = HashSet::new();
    assert_eq!(
        classify(" ", InferenceEvidence::GraphemeCluster, &empty),
        Some((
            CharDefKind::Boundary,
            InferenceEvidence::SafeBoundaryTable { version: 1 }
        ))
    );
    for letter in ["'", "\u{02BC}", "-", "\u{2011}", "§"] {
        assert_eq!(
            classify(letter, InferenceEvidence::GraphemeCluster, &empty),
            Some((CharDefKind::Segment, InferenceEvidence::GraphemeCluster))
        );
    }
}

#[test]
fn an_exemplar_letter_retains_its_evidence() {
    assert_eq!(exemplar_letter("quma", &["q".into()]), Some("q"));
    assert_eq!(
        classify("q", InferenceEvidence::LdmlExemplar, &HashSet::new()),
        Some((CharDefKind::Segment, InferenceEvidence::LdmlExemplar))
    );
}

#[test]
fn an_authored_boundary_retains_its_classification() {
    let authored = HashSet::from(["\u{2011}".to_string()]);
    assert_eq!(
        classify("\u{2011}", InferenceEvidence::GraphemeCluster, &authored),
        Some((CharDefKind::Boundary, InferenceEvidence::AuthoredBoundary))
    );
}
