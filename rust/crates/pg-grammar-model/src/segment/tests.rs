use super::*;
use crate::chardef::{CharDefTable, RawCharDef, RawFeatureValue};
use crate::featsys::PhonFeatureSystem;
use pg_shape::NodeKind;

fn seg(xml_id: &str, reps: &[&str]) -> RawCharDef {
    RawCharDef {
        xml_id: xml_id.to_string(),
        kind: CharDefKind::Segment,
        representations: reps.iter().map(|s| s.to_string()).collect(),
        feature_values: Vec::<RawFeatureValue>::new(),
    }
}

fn bnd(xml_id: &str, reps: &[&str]) -> RawCharDef {
    RawCharDef {
        xml_id: xml_id.to_string(),
        kind: CharDefKind::Boundary,
        representations: reps.iter().map(|s| s.to_string()).collect(),
        feature_values: vec![],
    }
}

fn table(defs: Vec<RawCharDef>) -> CharDefTable {
    let feat_sys = PhonFeatureSystem::from_raw(vec![]).unwrap();
    CharDefTable::from_raw("t".to_string(), None, defs, &feat_sys).unwrap()
}

// --- Finding N3: root-allomorph PhoneticShape pattern-language fallback --------------------

fn segments_class(
    xml_id: &str,
    name: &str,
    members: Vec<crate::chardef::CharDefId>,
) -> NaturalClass {
    NaturalClass {
        xml_id: xml_id.to_string(),
        name: Some(name.to_string()),
        kind: NaturalClassKind::Segments(members),
    }
}

#[test]
fn bracket_class_reference_inserts_an_abstract_node_with_the_class_members() {
    let t = table(vec![
        seg("c_b", &["b"]),
        seg("c_t", &["t"]),
        seg("c_a", &["a"]),
        seg("c_e", &["e"]),
    ]);
    let a = t.lookup_nfd("a").unwrap();
    let e = t.lookup_nfd("e").unwrap();
    let nc = segments_class("nc1", "Vowel", vec![a, e]);
    let shape = segment_with_patterns(&t, std::slice::from_ref(&nc), "b[Vowel]t").unwrap();
    let interior: Vec<_> = shape.interior().collect();
    assert_eq!(interior.len(), 3, "b, [Vowel], t");
    assert_eq!(interior[0].2, t.lookup_nfd("b").unwrap().0);
    assert_eq!(interior[2].2, t.lookup_nfd("t").unwrap().0);
    // The middle node is the abstract class reference: NO_CHAR_DEF, not optional/iterative.
    assert_eq!(interior[1].2, pg_shape::NO_CHAR_DEF);
    assert!(!interior[1].3.is_optional());
    assert!(!interior[1].3.is_iterative());
    match shape.node_cd_set(interior[1].0) {
        pg_shape::EffectiveCdSet::Members(b) => {
            assert!(b.contains(a.0) && b.contains(e.0));
            assert_eq!(b.count(), 2);
        }
        other => panic!("expected Members, got {other:?}"),
    }
}

#[test]
fn bracket_class_lookup_is_by_name_not_by_xml_id() {
    // Lookup is keyed by `Name` ("Vowel"), never by the `id` attribute ("vwl") (XmlLanguageLoader.cs:704,719).
    let t = table(vec![seg("c_b", &["b"]), seg("c_a", &["a"])]);
    let a = t.lookup_nfd("a").unwrap();
    let nc = segments_class("vwl", "Vowel", vec![a]);
    assert!(segment_with_patterns(&t, std::slice::from_ref(&nc), "b[vwl]").is_err());
    assert!(segment_with_patterns(&t, std::slice::from_ref(&nc), "b[Vowel]").is_ok());
}

#[test]
fn optional_group_marks_the_class_node_optional_but_not_iterative() {
    let t = table(vec![seg("c_b", &["b"]), seg("c_a", &["a"])]);
    let a = t.lookup_nfd("a").unwrap();
    let nc = segments_class("nc1", "Vowel", vec![a]);
    let shape = segment_with_patterns(&t, std::slice::from_ref(&nc), "b([Vowel])").unwrap();
    let interior: Vec<_> = shape.interior().collect();
    assert_eq!(interior.len(), 2);
    assert!(interior[1].3.is_optional());
    assert!(!interior[1].3.is_iterative());
}

#[test]
fn kleene_star_marks_the_class_node_optional_and_iterative() {
    let t = table(vec![seg("c_b", &["b"]), seg("c_a", &["a"])]);
    let a = t.lookup_nfd("a").unwrap();
    let nc = segments_class("nc1", "Vowel", vec![a]);
    let shape = segment_with_patterns(&t, std::slice::from_ref(&nc), "b[Vowel]*").unwrap();
    let interior: Vec<_> = shape.interior().collect();
    assert_eq!(interior.len(), 2);
    assert!(interior[1].3.is_optional());
    assert!(interior[1].3.is_iterative());
}

#[test]
fn kleene_star_does_not_apply_after_an_optional_groups_close_paren() {
    // C#'s Kleene-star check is a literal "previous char is ']'" test, so `([Vowel])*` fails.
    let t = table(vec![seg("c_b", &["b"]), seg("c_a", &["a"])]);
    let a = t.lookup_nfd("a").unwrap();
    let nc = segments_class("nc1", "Vowel", vec![a]);
    assert!(segment_with_patterns(&t, std::slice::from_ref(&nc), "b([Vowel])*").is_err());
}

#[test]
fn malformed_optional_group_with_two_nodes_fails() {
    let t = table(vec![seg("c_a", &["a"]), seg("c_e", &["e"])]);
    let a = t.lookup_nfd("a").unwrap();
    let e = t.lookup_nfd("e").unwrap();
    let vowel = segments_class("nc1", "Vowel", vec![a]);
    let front = segments_class("nc2", "Front", vec![e]);
    // Two nodes before ')' fails C#'s `nodesList.Count == optionalCount + 1` guard.
    assert!(segment_with_patterns(&t, &[vowel, front], "([Vowel][Front])").is_err());
}

#[test]
fn unclosed_optional_group_fails_at_the_open_paren() {
    let t = table(vec![seg("c_b", &["b"]), seg("c_a", &["a"])]);
    let a = t.lookup_nfd("a").unwrap();
    let nc = segments_class("nc1", "Vowel", vec![a]);
    let err = segment_with_patterns(&t, std::slice::from_ref(&nc), "b([Vowel]").unwrap_err();
    assert_eq!(err.position, 1); // the '(' position
}

#[test]
fn segment_with_patterns_behaves_like_segment_when_no_pattern_syntax_is_present() {
    let t = table(vec![seg("c1", &["c"]), seg("c2", &["v"])]);
    let plain = segment(&t, "cvc").unwrap();
    let patterned = segment_with_patterns(&t, &[], "cvc").unwrap();
    assert_eq!(plain, patterned);
}

#[test]
fn unknown_class_name_fails_like_an_unmatched_literal() {
    let t = table(vec![seg("c_b", &["b"])]);
    assert!(segment_with_patterns(&t, &[], "b[NoSuchClass]").is_err());
}

#[test]
fn simple_cvc_segmentation() {
    let t = table(vec![seg("c1", &["c"]), seg("c2", &["v"])]);
    let shape = segment(&t, "cvc").unwrap();
    let interior: Vec<_> = shape.interior().map(|(_, k, _, _)| k).collect();
    assert_eq!(
        interior,
        vec![NodeKind::Segment, NodeKind::Segment, NodeKind::Segment]
    );
}

#[test]
fn greedy_longest_match_prefers_two_char_rep_over_two_singles() {
    // Greedy longest-match: "sy" must win over "s" + "y" (GetShapeNodes' descending `j` loop).
    let t = table(vec![
        seg("c_s", &["s"]),
        seg("c_y", &["y"]),
        seg("c_sy", &["sy"]),
    ]);
    let sy_id = t.lookup_nfd("sy").unwrap();
    let shape = segment(&t, "sy").unwrap();
    let interior: Vec<_> = shape.interior().collect();
    assert_eq!(
        interior.len(),
        1,
        "should be one node (the 2-char match), not two"
    );
    assert_eq!(interior[0].2, sy_id.0);
}

#[test]
fn boundary_becomes_optional_node() {
    let t = table(vec![seg("c1", &["a"]), bnd("b1", &["+"])]);
    let shape = segment(&t, "a+a").unwrap();
    let flags: Vec<_> = shape
        .interior()
        .map(|(_, k, _, f)| (k, f.is_optional()))
        .collect();
    assert_eq!(
        flags,
        vec![
            (NodeKind::Segment, false),
            (NodeKind::Boundary, true),
            (NodeKind::Segment, false),
        ]
    );
}

#[test]
fn segmentation_is_deterministic() {
    let t = table(vec![seg("c1", &["a"]), seg("c2", &["b"])]);
    let s1 = segment(&t, "ababab").unwrap();
    let s2 = segment(&t, "ababab").unwrap();
    assert_eq!(s1, s2);
}

#[test]
fn unmatched_character_fails_at_correct_position() {
    let t = table(vec![seg("c1", &["a"]), seg("c2", &["b"])]);
    let err = segment(&t, "abz").unwrap_err();
    assert_eq!(err.position, 2);
    assert_eq!(err.word, "abz");
}

#[test]
fn error_position_remaps_from_nfd_space_to_original_space() {
    // Chosen so the NFD failure index differs from the remapped original-string position.
    let t = table(vec![seg("c_e", &["e"]), seg("c_acc", &["\u{0301}"])]);
    let word = "\u{00e9}n"; // precomposed é (not NFD) followed by an undefined "n"
    assert!(!is_nfd(word));
    let err = segment(&t, word).unwrap_err();
    // The remap recomposes the consumed NFD prefix back to the original string's coordinates.
    assert_eq!(err.position, 1);
}

#[test]
fn representation_matches_after_nfd_normalization() {
    // The char def uses a combining sequence; a precomposed input word must still match it.
    let t = table(vec![seg("c1", &["e\u{0301}"])]); // e + combining acute
    let shape = segment(&t, "\u{00e9}").unwrap(); // precomposed é
    assert_eq!(shape.interior().count(), 1);
}
