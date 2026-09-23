use super::*;
use pg_snapshot::ActiveParser;

#[test]
fn absent_active_parser_means_xample_like_liblcm() {
    let p = parse_with_issues(Some(
        "<ParserParameters><HC><NotOnClitics>true</NotOnClitics></HC></ParserParameters>",
    ))
    .unwrap()
    .0;
    assert_eq!(p.active_parser, ActiveParser::XAmple);
    assert_eq!(p.xample, XAmpleParameters::default());
}

#[test]
fn hc_active_parser_is_read() {
    let p = parse_with_issues(Some(
        "<ParserParameters><HC/><ActiveParser>HC</ActiveParser></ParserParameters>",
    ))
    .unwrap()
    .0;
    assert_eq!(p.active_parser, ActiveParser::Hc);
}

#[test]
fn unknown_active_parser_is_fatal() {
    let err = parse_with_issues(Some(
        "<ParserParameters><ActiveParser>Toneparser</ActiveParser></ParserParameters>",
    ))
    .unwrap_err();
    assert!(matches!(
        err,
        ImportError::InvalidSource { code, .. } if code == codes::INVALID_ACTIVE_PARSER
    ));
}

#[test]
fn active_parser_with_nested_element_is_fatal() {
    let err = parse_with_issues(Some(
        "<ParserParameters><ActiveParser>HC<Unexpected/></ActiveParser></ParserParameters>",
    ))
    .unwrap_err();
    assert!(matches!(
        err,
        ImportError::InvalidSource { code, .. } if code == codes::INVALID_ACTIVE_PARSER
    ));
}

#[test]
fn xample_block_is_read_field_by_field() {
    let p = parse_with_issues(Some(
        "<ParserParameters><XAmple><MaxNulls>0</MaxNulls><MaxPrefixes>1</MaxPrefixes>\
             <MaxInfixes>0</MaxInfixes><MaxRoots>1</MaxRoots><MaxSuffixes>0</MaxSuffixes>\
             <MaxInterfixes>0</MaxInterfixes><MaxAnalysesToReturn>20</MaxAnalysesToReturn></XAmple>\
             <ActiveParser>XAmple</ActiveParser></ParserParameters>",
    ))
    .unwrap()
    .0;
    assert_eq!(p.xample.max_nulls, Some(0));
    assert_eq!(p.xample.max_prefixes, Some(1));
    assert_eq!(p.xample.max_infixes, Some(0));
    assert_eq!(p.xample.max_roots, Some(1));
    assert_eq!(p.xample.max_suffixes, Some(0));
    assert_eq!(p.xample.max_interfixes, Some(0));
    assert_eq!(p.xample.max_analyses_to_return, Some(20));
}

#[test]
fn partial_xample_block_leaves_missing_values_none() {
    let p = parse_with_issues(Some(
        "<ParserParameters><XAmple><MaxNulls>1</MaxNulls></XAmple></ParserParameters>",
    ))
    .unwrap()
    .0;
    assert_eq!(p.xample.max_nulls, Some(1));
    assert_eq!(p.xample.max_roots, None);
    assert_eq!(p.xample.max_analyses_to_return, None);
}

#[test]
fn negative_max_analyses_is_kept_raw() {
    let p = parse_with_issues(Some(
        "<ParserParameters><XAmple><MaxAnalysesToReturn>-1</MaxAnalysesToReturn></XAmple></ParserParameters>",
    ))
    .unwrap()
    .0;
    assert_eq!(p.xample.max_analyses_to_return, Some(-1));
}

#[test]
fn malformed_xample_cap_is_none_and_reported() {
    let (params, issues, _) = parse_with_issues(Some(
        "<ParserParameters><XAmple><MaxPrefixes>many</MaxPrefixes></XAmple></ParserParameters>",
    ))
    .unwrap();
    assert_eq!(params.xample.max_prefixes, None);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].code, "fwdata.invalid-parser-parameter");
    assert!(issues[0].message.contains("MaxPrefixes"));
}

#[test]
fn presence_reports_parsed_ok_per_xample_field() {
    let (_, _, presence) = parse_with_issues(Some(
        "<ParserParameters><XAmple><MaxPrefixes>many</MaxPrefixes>\
             <MaxRoots>2</MaxRoots></XAmple></ParserParameters>",
    ))
    .unwrap();
    assert_eq!(
        presence.xample_fields,
        vec![("MaxPrefixes", false), ("MaxRoots", true)]
    );
}

#[test]
fn every_xample_field_tag_appears_in_the_presence_list_exactly_once() {
    let uni = "<ParserParameters><XAmple><MaxNulls>0</MaxNulls><MaxPrefixes>1</MaxPrefixes>\
             <MaxInfixes>0</MaxInfixes><MaxSuffixes>0</MaxSuffixes><MaxInterfixes>0</MaxInterfixes>\
             <MaxRoots>1</MaxRoots><MaxAnalysesToReturn>20</MaxAnalysesToReturn></XAmple>\
             </ParserParameters>";
    let (_, _, presence) = parse_with_issues(Some(uni)).unwrap();
    for tag in XAMPLE_FIELD_TAGS {
        assert_eq!(
            presence
                .xample_fields
                .iter()
                .filter(|(name, _)| name == tag)
                .count(),
            1,
            "{tag} must appear exactly once"
        );
    }
}

#[test]
fn malformed_parser_parameters_are_fatal_for_active_parser() {
    let err = parse_with_issues(Some("<ParserParameters><ActiveParser>XAmple")).unwrap_err();
    assert!(matches!(
        err,
        ImportError::InvalidSource { code, .. } if code == codes::INVALID_ACTIVE_PARSER
    ));
}

#[test]
fn explicit_xample_and_untrimmed_values_follow_exact_selector_rules() {
    let p = parse_with_issues(Some(
        "<ParserParameters><ActiveParser>XAmple</ActiveParser></ParserParameters>",
    ))
    .unwrap()
    .0;
    assert_eq!(p.active_parser, ActiveParser::XAmple);

    for raw in [
        "<ParserParameters><ActiveParser/></ParserParameters>",
        "<ParserParameters><ActiveParser> XAmple</ActiveParser></ParserParameters>",
        "<ParserParameters><ActiveParser>XAmple </ActiveParser></ParserParameters>",
        "<ParserParameters><ActiveParser> \n\t</ActiveParser></ParserParameters>",
    ] {
        assert!(
            parse_with_issues(Some(raw)).is_err(),
            "{raw:?} must be fatal"
        );
    }
}

#[test]
fn only_absent_raw_uses_parser_defaults() {
    assert_eq!(
        parse_with_issues(None).unwrap().0,
        ParserParameters::default()
    );
    assert!(parse_with_issues(Some("")).is_err());
    assert!(parse_with_issues(Some(" \n\t")).is_err());
}
