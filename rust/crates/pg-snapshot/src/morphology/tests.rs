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
