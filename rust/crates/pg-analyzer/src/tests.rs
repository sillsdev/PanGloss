use crate::{analyze, ProfileInput, StyledRun};

const LDML: &str = "<ldml><characters><exemplarCharacters>[a-z \\u0027 \\-]</exemplarCharacters></characters></ldml>";

#[test]
fn ldml_and_json_profiles_produce_same_source_spans() {
    let text = "𐐀don't 16th mother-in-law\u{200b}e\u{301} Ⅳ";
    let result = analyze(
        text,
        ProfileInput::Ldml {
            identity: "test.ldml",
            xml: LDML,
        },
        &[],
    )
    .unwrap();
    let json = serde_json::to_string(&result.profile).unwrap();
    let again = analyze(text, ProfileInput::Json(&json), &[]).unwrap();
    assert_eq!(result.occurrences, again.occurrences);
    assert_eq!(
        result
            .occurrences
            .iter()
            .map(|x| x.text.as_str())
            .collect::<Vec<_>>(),
        ["𐐀don't", "16th", "mother-in-law", "e\u{301}", "Ⅳ"]
    );
    assert_eq!(result.occurrences[0].utf8, [0, 9]);
    assert_eq!(result.occurrences[0].utf16, [0, 7]);
    assert_eq!(result.occurrences[0].code_points, [0, 6]);
}

#[test]
fn defaults_and_styles_follow_wordmaker() {
    let profile = analyze(
        "",
        ProfileInput::Ldml {
            identity: "empty",
            xml: "<ldml/>",
        },
        &[],
    )
    .unwrap()
    .profile;
    let result = analyze(
        "don't 1ab 2 ᵃ \u{301} x\u{200b}y",
        ProfileInput::Profile(&profile),
        &[StyledRun {
            start: 6,
            end: 9,
            style: "Verse Number".into(),
            profile: None,
        }],
    )
    .unwrap();
    assert_eq!(
        result
            .occurrences
            .iter()
            .map(|x| x.text.as_str())
            .collect::<Vec<_>>(),
        ["don", "t", "2", "ᵃ", "\u{301}", "x", "y"]
    );
    assert!(analyze(
        "x",
        ProfileInput::Profile(&profile),
        &[StyledRun {
            start: 0,
            end: 2,
            style: "x".into(),
            profile: None
        }]
    )
    .is_err());
}

#[test]
fn malformed_profiles_and_unsupported_sets_are_not_silently_accepted() {
    assert!(analyze("", ProfileInput::Json("{}"), &[]).is_err());
    assert!(analyze(
        "",
        ProfileInput::Ldml {
            identity: "a",
            xml: "<ldml><exemplarCharacters>[[:L:]]</exemplarCharacters></ldml>"
        },
        &[]
    )
    .is_err());
}

#[test]
fn writing_system_runs_use_their_own_profile() {
    let base = analyze(
        "",
        ProfileInput::Ldml {
            identity: "base",
            xml: "<ldml/>",
        },
        &[],
    )
    .unwrap()
    .profile;
    let alternate = analyze(
        "",
        ProfileInput::Ldml {
            identity: "alt",
            xml: "<ldml><exemplarCharacters>[\\u0027]</exemplarCharacters></ldml>",
        },
        &[],
    )
    .unwrap()
    .profile;
    let spans = [StyledRun {
        start: 4,
        end: 7,
        style: String::new(),
        profile: Some(alternate),
    }];
    let result = analyze("a'b c'd", ProfileInput::Profile(&base), &spans).unwrap();
    assert_eq!(
        result
            .occurrences
            .iter()
            .map(|x| x.text.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c'd"]
    );
}

#[test]
fn parity_corpus_runs_through_the_public_entry() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../dotnet-harness/WordMakerParity/cases.json"
    ))
    .unwrap();
    let expectations: &[&[&str]] = &[
        &["don", "t", "mother", "in", "law", "Mr", "16th", "Ⅳ", "漢字"],
        &["don't", "l’homme", "mother-in-law"],
        &["𐐀a", "é", "˂", "१२", "foo", "bar", "z"],
        &["abc", "def", "end"],
        &["a", "b", "c'd"],
        &["a-b"],
        &["a", "b"],
        &["x-y"],
        &["a b"],
        &["a", "b"],
        &["a-b", "c", "d"],
        &["a-b"],
        &["a", "b"],
        &["a'b", "c-d"],
        &["a:b"],
        &["x-y", "c"],
        &["x-y"],
        &["a:b"],
        &["abc"],
    ];
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), expectations.len());
    for (case, expected) in cases.iter().zip(expectations) {
        let text = case["text"].as_str().unwrap();
        let styles: Vec<StyledRun> = serde_json::from_value(case["styles"].clone()).unwrap();
        let result = analyze(
            text,
            ProfileInput::Ldml {
                identity: case["name"].as_str().unwrap(),
                xml: case["ldml"].as_str().unwrap(),
            },
            &styles,
        )
        .unwrap();
        assert_eq!(
            result
                .occurrences
                .iter()
                .map(|x| x.text.as_str())
                .collect::<Vec<_>>(),
            *expected
        );
    }
}

#[test]
fn ldml_icu_escapes_ranges_and_unicode_version_boundary() {
    let xml = r"<ldml><exemplarCharacters>[\x{002d} a - c]</exemplarCharacters></ldml>";
    let result = analyze("a-b", ProfileInput::Ldml { identity: "x", xml }, &[]).unwrap();
    assert_eq!(result.occurrences[0].text, "a-b");
    assert_eq!(result.profile.word_forming, vec!['-', 'a', 'b', 'c']);
    let unsupported = r"<ldml><exemplarCharacters>[\p{Pd}]</exemplarCharacters></ldml>";
    assert!(analyze(
        "",
        ProfileInput::Ldml {
            identity: "x",
            xml: unsupported
        },
        &[]
    )
    .is_err());
    let result = analyze(
        "a\u{11f02}b",
        ProfileInput::Ldml {
            identity: "x",
            xml: "<ldml/>",
        },
        &[],
    )
    .unwrap();
    assert_eq!(
        result
            .occurrences
            .iter()
            .map(|o| o.text.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
}

#[test]
fn unicode_14_includes_its_new_letters_but_not_unicode_17_letters() {
    let result = analyze(
        "a\u{0870}\u{11db0}b",
        ProfileInput::Ldml {
            identity: "version",
            xml: "<ldml/>",
        },
        &[],
    )
    .unwrap();
    assert_eq!(
        result
            .occurrences
            .iter()
            .map(|o| o.text.as_str())
            .collect::<Vec<_>>(),
        ["a\u{0870}", "b"]
    );
}

#[test]
fn unicode_set_pattern_whitespace_and_unsupported_strings() {
    let xml = r"<ldml><exemplarCharacters>[\  { \u200e}]</exemplarCharacters></ldml>";
    let result = analyze("", ProfileInput::Ldml { identity: "x", xml }, &[]).unwrap();
    assert_eq!(result.profile.word_forming, vec![' ', '\u{200e}']);
    for pattern in [
        "[:L:]", "[[:L:]]", "[{}]", "[{{a}}]", "[{a{b}}]", "[{ab}-c]", "[a-{bc}]",
    ] {
        let xml = format!("<ldml><exemplarCharacters>{pattern}</exemplarCharacters></ldml>");
        assert!(analyze(
            "",
            ProfileInput::Ldml {
                identity: "x",
                xml: &xml
            },
            &[]
        )
        .is_err());
    }
    for pattern in [r"[:]", r"[:a\:]", "[{a}-   ]"] {
        let xml = format!("<ldml><exemplarCharacters>{pattern}</exemplarCharacters></ldml>");
        assert!(analyze(
            "",
            ProfileInput::Ldml {
                identity: "x",
                xml: &xml
            },
            &[]
        )
        .is_ok());
    }
}
