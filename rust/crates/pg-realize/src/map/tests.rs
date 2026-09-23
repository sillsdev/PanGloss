use super::*;
use crate::ir::{CaseRole, Num, Poss};

#[test]
fn parses_amharic_sidecar_syntax_shape() {
    let text = r#"
# a comment
[features]
"pl" = "Num:Pl"
"poss.1s" = "Poss:P1Sg"
"at" = "Case:Loc"
bare_key = "Ignore"
"#;
    let map = RealizeMap::parse(text).expect("should parse");
    assert_eq!(map.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
    assert_eq!(
        map.lookup("poss.1s"),
        Some(FeatureAssignment::Poss(Poss::P1Sg))
    );
    assert_eq!(
        map.lookup("at"),
        Some(FeatureAssignment::Case(CaseRole::Loc))
    );
    assert_eq!(map.lookup("bare_key"), Some(FeatureAssignment::Ignore));
    assert_eq!(map.lookup("nope"), None);
}

#[test]
fn rejects_unknown_feature_name_with_line_number() {
    let text = "[features]\n\"x\" = \"Bogus:Sg\"\n";
    let err = RealizeMap::parse(text).unwrap_err();
    assert_eq!(err.line, 2);
    assert!(err.message.contains("Bogus"), "{}", err.message);
}

#[test]
fn rejects_unknown_feature_value_with_line_number() {
    let text = "[features]\n\"x\" = \"Num:Triple\"\n";
    let err = RealizeMap::parse(text).unwrap_err();
    assert_eq!(err.line, 2);
    assert!(err.message.contains("Triple"), "{}", err.message);
}

#[test]
fn rejects_junk_line_with_line_number() {
    let text = "[features]\nthis is not valid\n";
    let err = RealizeMap::parse(text).unwrap_err();
    assert_eq!(err.line, 2);
}

#[test]
fn rejects_unquoted_value() {
    let text = "[features]\n\"x\" = Num:Pl\n";
    let err = RealizeMap::parse(text).unwrap_err();
    assert_eq!(err.line, 2);
}

#[test]
fn rejects_bare_key_needing_quotes() {
    let text = "[features]\npl.foo = \"Num:Pl\"\n";
    let err = RealizeMap::parse(text).unwrap_err();
    assert_eq!(err.line, 2);
}

#[test]
fn rejects_second_section_header() {
    let text = "[features]\n[other]\n";
    let err = RealizeMap::parse(text).unwrap_err();
    assert_eq!(err.line, 2);
}

#[test]
fn rejects_duplicate_features_section() {
    let text = "[features]\n[features]\n";
    let err = RealizeMap::parse(text).unwrap_err();
    assert_eq!(err.line, 2);
}

#[test]
fn rejects_entry_before_section_header() {
    let text = "\"pl\" = \"Num:Pl\"\n[features]\n";
    let err = RealizeMap::parse(text).unwrap_err();
    assert_eq!(err.line, 1);
}

#[test]
fn empty_map_has_no_entries() {
    let map = RealizeMap::empty();
    assert_eq!(map.lookup("anything"), None);
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let text = "\n# comment\n\n[features]\n\n# another\n\"pl\" = \"Num:Pl\"\n\n";
    let map = RealizeMap::parse(text).expect("should parse");
    assert_eq!(map.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
}

#[test]
fn extend_overriding_replaces_shared_keys_adds_new_keys_keeps_untouched_keys() {
    let mut base = RealizeMap::parse("[features]\n\"pl\" = \"Num:Pl\"\n\"loc\" = \"Case:Loc\"\n")
        .expect("valid base");
    let overrides =
        RealizeMap::parse("[features]\n\"loc\" = \"Case:Abl\"\n\"abl\" = \"Case:Abl\"\n")
            .expect("valid overrides");

    base.extend_overriding(overrides);

    // Untouched key survives.
    assert_eq!(base.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
    // Shared key: override wins.
    assert_eq!(
        base.lookup("loc"),
        Some(FeatureAssignment::Case(CaseRole::Abl))
    );
    // New key from overrides is added.
    assert_eq!(
        base.lookup("abl"),
        Some(FeatureAssignment::Case(CaseRole::Abl))
    );
}
