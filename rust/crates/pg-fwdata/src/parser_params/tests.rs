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
fn malformed_hc_booleans_warn_and_keep_defaults_without_claiming_representation() {
    let (params, issues, presence) = parse_with_issues(Some(
        "<ParserParameters><HC><NotOnClitics>sometimes</NotOnClitics>\
         <AcceptUnspecifiedGraphemes>yes</AcceptUnspecifiedGraphemes>\
         <NoDefaultCompounding>nope</NoDefaultCompounding></HC></ParserParameters>",
    ))
    .unwrap();
    assert!(params.not_on_clitics);
    assert!(!params.accept_unspecified_graphemes);
    assert!(!params.no_default_compounding);
    assert_eq!(issues.len(), 3);
    assert!(issues
        .iter()
        .all(|issue| issue.code == codes::INVALID_PARSER_PARAMETER.wire()));
    assert_eq!(
        presence.hc_fields,
        vec![
            ("NotOnClitics", false),
            ("AcceptUnspecifiedGraphemes", false),
            ("NoDefaultCompounding", false),
        ]
    );
}

#[test]
fn valid_hc_booleans_are_recorded_as_represented_candidates() {
    let (params, issues, presence) = parse_with_issues(Some(
        "<ParserParameters><HC><NotOnClitics>false</NotOnClitics>\
         <AcceptUnspecifiedGraphemes>true</AcceptUnspecifiedGraphemes>\
         <NoDefaultCompounding>1</NoDefaultCompounding></HC></ParserParameters>",
    ))
    .unwrap();
    assert!(!params.not_on_clitics);
    assert!(params.accept_unspecified_graphemes);
    assert!(params.no_default_compounding);
    assert!(issues.is_empty());
    assert_eq!(
        presence.hc_fields,
        vec![
            ("NotOnClitics", true),
            ("AcceptUnspecifiedGraphemes", true),
            ("NoDefaultCompounding", true),
        ]
    );
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

#[test]
fn del_reapps_is_read_from_the_hc_element_and_defaults_to_zero() {
    let read = |xml: &str| parse_with_issues(Some(xml)).unwrap();
    let (params, issues, presence) =
        read("<ParserParameters><HC><DelReapps>1</DelReapps></HC></ParserParameters>");
    assert_eq!(params.deletion_reapplications, 1);
    assert!(issues.is_empty());
    assert_eq!(presence.hc_fields, vec![("DelReapps", true)]);

    let (absent, _, _) = read("<ParserParameters><HC/></ParserParameters>");
    assert_eq!(absent.deletion_reapplications, 0);
    let (no_hc, _, _) = read("<ParserParameters><DelReapps>3</DelReapps></ParserParameters>");
    assert_eq!(
        no_hc.deletion_reapplications, 0,
        "only <HC>'s DelReapps counts"
    );
    let (negative, negative_issues, _) =
        read("<ParserParameters><HC><DelReapps>-2</DelReapps></HC></ParserParameters>");
    assert_eq!(negative.deletion_reapplications, 0);
    assert!(negative_issues.is_empty());
}

#[test]
fn malformed_del_reapps_is_fatal_and_names_the_field_and_value() {
    let err = parse_with_issues(Some(
        "<ParserParameters><HC><DelReapps>many</DelReapps></HC></ParserParameters>",
    ))
    .unwrap_err();
    let ImportError::InvalidSource { code, message } = err else {
        panic!("expected InvalidSource, got {err:?}");
    };
    assert_eq!(code, codes::INVALID_PARSER_PARAMETER);
    assert!(message.contains("HC.DelReapps"), "{message}");
    assert!(message.contains("\"many\""), "{message}");
}

#[test]
fn hc_max_roots_is_read_from_the_hc_element_apart_from_xample_max_roots() {
    let read = |xml: &str| parse_with_issues(Some(xml)).unwrap();
    let (params, issues, presence) = read(
        "<ParserParameters><XAmple><MaxRoots>1</MaxRoots></XAmple><HC><MaxRoots>3</MaxRoots></HC></ParserParameters>",
    );
    assert_eq!(params.max_roots, Some(3));
    assert_eq!(params.xample.max_roots, Some(1));
    assert!(issues.is_empty());
    assert_eq!(presence.hc_fields, vec![("MaxRoots", true)]);

    let (absent, _, _) = read("<ParserParameters><HC/></ParserParameters>");
    assert_eq!(absent.max_roots, None, "absent means C#'s default 2");
    let (xample_only, _, _) =
        read("<ParserParameters><XAmple><MaxRoots>4</MaxRoots></XAmple></ParserParameters>");
    assert_eq!(xample_only.max_roots, None, "XAmple's MaxRoots is not HC's");
    for below_one in ["0", "-3"] {
        let (clamped, _, _) = read(&format!(
            "<ParserParameters><HC><MaxRoots>{below_one}</MaxRoots></HC></ParserParameters>"
        ));
        assert_eq!(clamped.max_roots, Some(1), "{below_one} gates like 1");
    }
}

#[test]
fn make_partials_final_is_read_from_the_hc_element_and_defaults_to_false() {
    let read = |xml: &str| parse_with_issues(Some(xml)).unwrap();
    for (raw, expected) in [
        ("true", true),
        ("True", true),
        ("1", true),
        (" false ", false),
        ("0", false),
    ] {
        let (params, issues, presence) = read(&format!(
            "<ParserParameters><HC><MakePartialsFinal>{raw}</MakePartialsFinal></HC></ParserParameters>"
        ));
        assert_eq!(params.make_partials_final, expected, "{raw:?}");
        assert!(issues.is_empty());
        assert_eq!(presence.hc_fields, vec![("MakePartialsFinal", true)]);
    }
    let (absent, _, _) = read("<ParserParameters><HC/></ParserParameters>");
    assert!(!absent.make_partials_final);
}

#[test]
fn merge_analyses_is_read_from_the_hc_element_and_defaults_to_true() {
    let read = |xml: &str| parse_with_issues(Some(xml)).unwrap();
    let (params, issues, presence) =
        read("<ParserParameters><HC><MergeAnalyses>false</MergeAnalyses></HC></ParserParameters>");
    assert!(!params.merge_analyses);
    assert!(issues.is_empty());
    assert_eq!(presence.hc_fields, vec![("MergeAnalyses", true)]);
    let (absent, _, _) = read("<ParserParameters><HC/></ParserParameters>");
    assert!(absent.merge_analyses, "FieldWorks merges by default");
    let (no_hc, _, _) = read("<ParserParameters/>");
    assert!(no_hc.merge_analyses);
}

#[test]
fn malformed_merge_analyses_is_fatal_and_names_the_field_and_value() {
    let err = parse_with_issues(Some(
        "<ParserParameters><HC><MergeAnalyses>off</MergeAnalyses></HC></ParserParameters>",
    ))
    .unwrap_err();
    let ImportError::InvalidSource { code, message } = err else {
        panic!("expected InvalidSource, got {err:?}");
    };
    assert_eq!(code, codes::INVALID_PARSER_PARAMETER);
    assert!(message.contains("HC.MergeAnalyses"), "{message}");
    assert!(message.contains("\"off\""), "{message}");
}

#[test]
fn malformed_make_partials_final_is_fatal_and_names_the_field_and_value() {
    let err = parse_with_issues(Some(
        "<ParserParameters><HC><MakePartialsFinal>yes</MakePartialsFinal></HC></ParserParameters>",
    ))
    .unwrap_err();
    let ImportError::InvalidSource { code, message } = err else {
        panic!("expected InvalidSource, got {err:?}");
    };
    assert_eq!(code, codes::INVALID_PARSER_PARAMETER);
    assert!(message.contains("HC.MakePartialsFinal"), "{message}");
    assert!(message.contains("\"yes\""), "{message}");
}

#[test]
fn malformed_hc_max_roots_is_fatal_and_names_the_field_and_value() {
    for bad in ["two", "2.5", "99999999999"] {
        let err = parse_with_issues(Some(&format!(
            "<ParserParameters><HC><MaxRoots>{bad}</MaxRoots></HC></ParserParameters>"
        )))
        .unwrap_err();
        let ImportError::InvalidSource { code, message } = err else {
            panic!("expected InvalidSource, got {err:?}");
        };
        assert_eq!(code, codes::INVALID_PARSER_PARAMETER);
        assert!(message.contains("HC.MaxRoots"), "{message}");
        assert!(message.contains(&format!("{bad:?}")), "{message}");
    }
}
