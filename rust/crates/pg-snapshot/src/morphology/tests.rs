use super::*;

#[test]
fn parser_parameters_default_is_xample_with_fieldworks_default_caps() {
    let p = ParserParameters::default();
    assert_eq!(p.active_parser, ActiveParser::XAmple);
    assert_eq!(p.xample, XAmpleParameters::default());
    assert_eq!(p.xample.max_nulls, None);
    assert_eq!(p.xample.max_analyses_to_return, None);
}

#[test]
fn parser_parameters_round_trips_through_json_and_old_json_still_loads() {
    let mut p = ParserParameters {
        active_parser: ActiveParser::Hc,
        ..Default::default()
    };
    p.xample.max_prefixes = Some(3);
    let json = serde_json::to_string(&p).unwrap();
    let back: ParserParameters = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);
    // A snapshot written before these fields existed must still deserialize.
    let old =
        r#"{"notOnClitics":true,"acceptUnspecifiedGraphemes":false,"noDefaultCompounding":false}"#;
    let back: ParserParameters = serde_json::from_str(old).unwrap();
    assert_eq!(back.active_parser, ActiveParser::XAmple);
    assert_eq!(back.xample, XAmpleParameters::default());
}

#[test]
fn morpher_settings_are_omitted_at_their_defaults_and_round_trip_when_set() {
    let default_json = serde_json::to_value(ParserParameters::default()).unwrap();
    for omitted in [
        "maxRoots",
        "makePartialsFinal",
        "mergeAnalyses",
        "guessRoots",
    ] {
        assert!(default_json.get(omitted).is_none(), "{default_json}");
    }

    let p = ParserParameters {
        max_roots: Some(3),
        make_partials_final: true,
        merge_analyses: false,
        guess_roots: false,
        ..Default::default()
    };
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["maxRoots"], 3);
    assert_eq!(json["makePartialsFinal"], true);
    assert_eq!(json["mergeAnalyses"], false);
    assert_eq!(json["guessRoots"], false);
    let back: ParserParameters = serde_json::from_value(json).unwrap();
    assert_eq!(back, p);
}

#[test]
fn a_snapshot_without_merge_analyses_reads_the_fieldworks_default_true() {
    let back: ParserParameters = serde_json::from_str(
        r#"{"notOnClitics":true,"acceptUnspecifiedGraphemes":false,"noDefaultCompounding":false}"#,
    )
    .unwrap();
    assert!(back.merge_analyses);
    assert!(back.guess_roots, "FieldWorks guesses roots by default");
    assert_eq!(back.max_roots, None);
    assert!(!back.make_partials_final);
}

#[test]
fn merge_msas_defaults_true_is_omitted_at_true_and_round_trips_false() {
    let default_json = serde_json::to_value(ParserParameters::default()).unwrap();
    assert!(default_json.get("mergeMsas").is_none(), "{default_json}");
    let old: ParserParameters = serde_json::from_str(
        r#"{"notOnClitics":true,"acceptUnspecifiedGraphemes":false,"noDefaultCompounding":false}"#,
    )
    .unwrap();
    assert!(old.merge_msas, "FieldWorks merges equal MSAs by default");

    let p = ParserParameters {
        merge_msas: false,
        ..Default::default()
    };
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["mergeMsas"], false);
    let back: ParserParameters = serde_json::from_value(json).unwrap();
    assert_eq!(back, p);
}
