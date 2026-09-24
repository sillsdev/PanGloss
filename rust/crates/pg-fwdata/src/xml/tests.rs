use super::*;

#[test]
fn parse_fwdata_reader_accepts_in_memory_bytes() {
    let xml = br#"<?xml version="1.0"?><languageproject><rt class="LangProject" guid="00000000-0000-0000-0000-000000000001"/></languageproject>"#;
    let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    assert!(graph.get("00000000-0000-0000-0000-000000000001").is_some());
}

#[test]
fn census_counts_every_header_including_unknown_classes() {
    let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="LangProject" guid="00000000-0000-0000-0000-000000000001"/>
<rt class="ZzUnknown" guid="00000000-0000-0000-0000-000000000002"/>
<rt class="ZzUnknown" guid="00000000-0000-0000-0000-000000000003"/>
</languageproject>"#;
    let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    let census = graph.census();
    assert_eq!(census.total_occurrences, 3);
    assert_eq!(census.class_occurrences.get("LangProject"), Some(&1));
    assert_eq!(census.class_occurrences.get("ZzUnknown"), Some(&2));
    assert_eq!(
        census.unhandled_class_occurrences.get("ZzUnknown"),
        Some(&2)
    );
    assert!(!census
        .unhandled_class_occurrences
        .contains_key("LangProject"));
    assert_eq!(census.ordered_header_sha256.len(), 64);
    assert!(census
        .ordered_header_sha256
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[test]
fn census_is_stable_across_two_parses_of_the_same_bytes() {
    let xml = br#"<?xml version="1.0"?><languageproject><rt class="LangProject" guid="00000000-0000-0000-0000-000000000001"/></languageproject>"#;
    let a = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    let b = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    assert_eq!(
        a.census().ordered_header_sha256,
        b.census().ordered_header_sha256
    );
}

#[test]
fn duplicate_guid_on_an_allowed_class_keeps_the_first_and_reports_one_issue() {
    let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="LexDb" guid="00000000-0000-0000-0000-000000000002"/>
<rt class="LexDb" guid="00000000-0000-0000-0000-000000000002"/>
</languageproject>"#;
    let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    let duplicate_issues: Vec<_> = graph
        .issues
        .iter()
        .filter(|i| i.code == pg_snapshot::ImportWarningCode::InvalidSourceDuplicateGuid)
        .collect();
    assert_eq!(duplicate_issues.len(), 1);
    assert!(duplicate_issues[0].fatal);
    assert!(graph.get("00000000-0000-0000-0000-000000000002").is_some());
}

#[test]
fn first_recognized_occurrence_wins_over_an_earlier_unknown_class_duplicate() {
    let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="ZzUnknown" guid="00000000-0000-0000-0000-000000000002"/>
<rt class="LexDb" guid="00000000-0000-0000-0000-000000000002"/>
</languageproject>"#;
    let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    let duplicate_issues: Vec<_> = graph
        .issues
        .iter()
        .filter(|i| i.code == pg_snapshot::ImportWarningCode::InvalidSourceDuplicateGuid)
        .collect();
    assert_eq!(duplicate_issues.len(), 1);
    let source = duplicate_issues[0].source.as_ref().unwrap();
    assert_eq!(source.kind, pg_snapshot::FwClass::Unknown);
    assert_eq!(source.id, "00000000-0000-0000-0000-000000000002");
    let record = graph
        .get("00000000-0000-0000-0000-000000000002")
        .expect("the recognized LexDb occurrence must be kept");
    assert_eq!(record.class, "LexDb");
}

#[test]
fn duplicate_guid_across_two_different_allowed_classes_keeps_the_first() {
    let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="LexEntry" guid="00000000-0000-0000-0000-000000000002"/>
<rt class="MoStemMsa" guid="00000000-0000-0000-0000-000000000002"/>
</languageproject>"#;
    let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    let duplicate_issues: Vec<_> = graph
        .issues
        .iter()
        .filter(|i| i.code == pg_snapshot::ImportWarningCode::InvalidSourceDuplicateGuid)
        .collect();
    assert_eq!(duplicate_issues.len(), 1);
    let source = duplicate_issues[0].source.as_ref().unwrap();
    assert_eq!(source.kind, pg_snapshot::FwClass::LexEntry);
    assert_eq!(source.id, "00000000-0000-0000-0000-000000000002");
    let record = graph
        .get("00000000-0000-0000-0000-000000000002")
        .expect("the first occurrence must be kept");
    assert_eq!(record.class, "LexEntry");
}

#[test]
fn two_allowed_class_records_missing_guid_each_get_their_own_issue() {
    let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="LexDb"/>
<rt class="MoStemMsa"/>
</languageproject>"#;
    let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    let missing_issues: Vec<_> = graph
        .issues
        .iter()
        .filter(|i| i.code == pg_snapshot::ImportWarningCode::InvalidSourceMissingGuid)
        .collect();
    assert_eq!(missing_issues.len(), 2);
    let duplicate_issues = graph
        .issues
        .iter()
        .filter(|i| i.code == pg_snapshot::ImportWarningCode::InvalidSourceDuplicateGuid)
        .count();
    assert_eq!(duplicate_issues, 0);
    assert!(!graph.records.contains_key(""));
}

#[test]
fn missing_guid_on_an_allowed_class_is_a_fatal_issue_and_is_not_inserted() {
    let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="LexDb"/>
</languageproject>"#;
    let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    let missing_issues: Vec<_> = graph
        .issues
        .iter()
        .filter(|i| i.code == pg_snapshot::ImportWarningCode::InvalidSourceMissingGuid)
        .collect();
    assert_eq!(missing_issues.len(), 1);
    assert!(missing_issues[0].fatal);
    assert_eq!(graph.records.len(), 0);
}

#[test]
fn missing_guid_on_an_unknown_class_is_census_only_with_no_issue() {
    let xml = br#"<?xml version="1.0"?><languageproject>
<rt class="ZzUnknown"/>
</languageproject>"#;
    let graph = parse_fwdata_reader(std::io::Cursor::new(&xml[..])).unwrap();
    assert!(graph.issues.is_empty());
    assert_eq!(
        graph.census().unhandled_class_occurrences.get("ZzUnknown"),
        Some(&1)
    );
}
