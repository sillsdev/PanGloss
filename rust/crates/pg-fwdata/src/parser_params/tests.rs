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
fn guess_roots_is_read_from_the_hc_element_and_defaults_to_true() {
    let read = |xml: &str| parse_with_issues(Some(xml)).unwrap();
    for (raw, expected) in [("false", false), ("0", false), ("True", true), ("1", true)] {
        let (params, issues, presence) = read(&format!(
            "<ParserParameters><HC><GuessRoots>{raw}</GuessRoots></HC></ParserParameters>"
        ));
        assert_eq!(params.guess_roots, expected, "{raw:?}");
        assert!(issues.is_empty());
        assert_eq!(presence.hc_fields, vec![("GuessRoots", true)]);
    }
    let (absent, _, _) = read("<ParserParameters><HC/></ParserParameters>");
    assert!(absent.guess_roots, "FieldWorks guesses roots by default");
    let (no_hc, _, _) = read("<ParserParameters/>");
    assert!(no_hc.guess_roots);
}

#[test]
fn malformed_guess_roots_is_fatal_and_names_the_field_and_value() {
    let err = parse_with_issues(Some(
        "<ParserParameters><HC><GuessRoots>maybe</GuessRoots></HC></ParserParameters>",
    ))
    .unwrap_err();
    let ImportError::InvalidSource { code, message } = err else {
        panic!("expected InvalidSource, got {err:?}");
    };
    assert_eq!(code, codes::INVALID_PARSER_PARAMETER);
    assert!(message.contains("HC.GuessRoots"), "{message}");
    assert!(message.contains("\"maybe\""), "{message}");
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
fn merge_msas_is_read_from_the_hc_element_and_defaults_to_true() {
    let read = |xml: &str| parse_with_issues(Some(xml)).unwrap();
    for (raw, expected) in [("false", false), ("0", false), ("True", true), ("1", true)] {
        let (params, issues, presence) = read(&format!(
            "<ParserParameters><HC><MergeMSAs>{raw}</MergeMSAs></HC></ParserParameters>"
        ));
        assert_eq!(params.merge_msas, expected, "{raw}");
        assert!(issues.is_empty());
        assert_eq!(presence.hc_fields, vec![("MergeMSAs", true)]);
    }
    let (absent, _, _) = read("<ParserParameters><HC/></ParserParameters>");
    assert!(absent.merge_msas, "FieldWorks merges equal MSAs by default");
    let (no_hc, _, _) = read("<ParserParameters/>");
    assert!(no_hc.merge_msas);
}

#[test]
fn malformed_merge_msas_is_fatal_and_names_the_field_and_value() {
    let err = parse_with_issues(Some(
        "<ParserParameters><HC><MergeMSAs>sometimes</MergeMSAs></HC></ParserParameters>",
    ))
    .unwrap_err();
    let ImportError::InvalidSource { code, message } = err else {
        panic!("expected InvalidSource, got {err:?}");
    };
    assert_eq!(code, codes::INVALID_PARSER_PARAMETER);
    assert!(message.contains("HC.MergeMSAs"), "{message}");
    assert!(message.contains("\"sometimes\""), "{message}");
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

const G1: &str = "11111111-1111-1111-1111-111111111111";
const G2: &str = "22222222-2222-2222-2222-222222222222";

fn affix_counts(rules: &str) -> (Vec<(String, u32)>, usize) {
    let xml =
        format!("<ParserParameters><HC><AffixRules>{rules}</AffixRules></HC></ParserParameters>");
    let (p, issues, _) = parse_with_issues(Some(&xml)).unwrap();
    (
        p.affix_rule_max_applications
            .into_iter()
            .map(|e| (e.msa, e.max_applications))
            .collect(),
        issues.len(),
    )
}

fn one_count(value: &str) -> (Option<u32>, usize) {
    let (counts, warnings) =
        affix_counts(&format!("<AffixRule guid=\"{G1}\" maxApps=\"{value}\"/>"));
    (counts.first().map(|c| c.1), warnings)
}

#[test]
fn affix_rule_max_apps_loads_counts_above_one_and_ignores_one() {
    let (counts, warnings) = affix_counts(&format!(
        "<AffixRule guid=\"{G1}\" maxApps=\"2\"/><AffixRule guid=\"{G2}\" maxApps=\"1\"/>"
    ));
    assert_eq!(counts, vec![(G1.to_string(), 2)]);
    assert_eq!(warnings, 0);
}

/// HCLoader clamps with `Math.Max(1, Math.Min(10, maxApps))` after `int.TryParse`.
#[test]
fn affix_rule_max_apps_is_clamped_to_one_through_ten_like_hcloader() {
    assert_eq!(one_count("1"), (None, 0));
    assert_eq!(one_count("2"), (Some(2), 0));
    assert_eq!(one_count("10"), (Some(10), 0));
    assert_eq!(one_count("11"), (Some(10), 1));
    assert_eq!(one_count("1000"), (Some(10), 1));
    assert_eq!(one_count("70000"), (Some(10), 1));
    assert_eq!(one_count("2147483647"), (Some(10), 1));
    assert_eq!(one_count("0"), (None, 0));
    assert_eq!(one_count("-3"), (None, 0));
    assert_eq!(one_count("-2147483648"), (None, 0));
}

/// A value `int.TryParse` rejects leaves the affix at one application.
#[test]
fn affix_rule_max_apps_unparsable_values_are_skipped_with_a_warning() {
    for bad in ["abc", "", "1.5", "2147483648", "1e1", "0x2", "--2", "2 3"] {
        assert_eq!(one_count(bad), (None, 1), "{bad:?}");
    }
}

#[test]
fn affix_rule_max_apps_accepts_whitespace_and_a_plus_sign_like_int_try_parse() {
    assert_eq!(one_count(" 2 "), (Some(2), 0));
    assert_eq!(one_count("+3"), (Some(3), 0));
    assert_eq!(one_count("\t4\n"), (Some(4), 0));
}

#[test]
fn affix_rule_max_apps_guid_is_parsed_like_guid_try_parse() {
    let count = |guid: &str| affix_counts(&format!("<AffixRule guid=\"{guid}\" maxApps=\"3\"/>"));
    assert_eq!(count(G1), (vec![(G1.to_string(), 3)], 0));
    assert_eq!(count(&G1.to_uppercase()), (vec![(G1.to_string(), 3)], 0));
    assert_eq!(
        count(&format!(" {{{G1}}} ")),
        (vec![(G1.to_string(), 3)], 0)
    );
    assert_eq!(count(&G1.replace('-', "")), (vec![(G1.to_string(), 3)], 0));
    for bad in ["", "not-a-guid", "a1", &G1[1..]] {
        assert_eq!(count(bad), (vec![], 1), "{bad:?}");
    }
}

#[test]
fn affix_rule_max_apps_missing_attribute_is_skipped_with_a_warning() {
    let (counts, warnings) = affix_counts(&format!(
        "<AffixRule maxApps=\"2\"/><AffixRule guid=\"{G1}\"/>"
    ));
    assert!(counts.is_empty());
    assert_eq!(warnings, 2);
}

/// HCLoader assigns into a dictionary, so the last valid entry for a guid wins, and an invalid later entry does not cancel an earlier one.
#[test]
fn affix_rule_max_apps_repeated_guid_takes_the_last_valid_entry() {
    let rule = |v: &str| format!("<AffixRule guid=\"{G1}\" maxApps=\"{v}\"/>");
    assert_eq!(
        affix_counts(&format!("{}{}", rule("2"), rule("5"))).0,
        vec![(G1.to_string(), 5)]
    );
    assert_eq!(
        affix_counts(&format!("{}{}", rule("5"), rule("1"))).0,
        vec![]
    );
    assert_eq!(
        affix_counts(&format!("{}{}", rule("5"), rule("x"))).0,
        vec![(G1.to_string(), 5)]
    );
}

#[test]
fn absent_affix_rules_means_no_counts() {
    let (p, issues, _) =
        parse_with_issues(Some("<ParserParameters><HC/></ParserParameters>")).unwrap();
    assert!(p.affix_rule_max_applications.is_empty() && issues.is_empty());
}

/// `<CompoundRules>` stays on its own path: an `AffixRules` block does not touch it.
#[test]
fn affix_rules_do_not_touch_compound_rule_counts() {
    let (p, _, _) = parse_with_issues(Some(&format!(
        "<ParserParameters><HC><AffixRules><AffixRule guid=\"{G1}\" maxApps=\"3\"/></AffixRules></HC>\
         <CompoundRules><CompoundRule guid=\"{G2}\" maxApps=\"70000\"/></CompoundRules></ParserParameters>"
    )))
    .unwrap();
    assert_eq!(p.affix_rule_max_applications.len(), 1);
    assert_eq!(p.compound_rule_max_applications[0].max_applications, 70000);
}
