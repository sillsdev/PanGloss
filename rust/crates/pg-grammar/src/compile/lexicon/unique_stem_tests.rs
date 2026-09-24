use super::*;

fn stem(guid: &str) -> Msa {
    Msa::Stem {
        guid: guid.into(),
        part_of_speech: Some("pos".into()),
        inflection_class: None,
        features: None,
        exception_features: vec![],
        from_parts_of_speech: vec![],
        slots: vec![],
    }
}

fn closed(feature: &str, value: &str) -> FeatureValue {
    FeatureValue {
        feature: feature.into(),
        value: FeatureValueKind::Closed {
            value: value.into(),
        },
    }
}

#[test]
fn unique_stem_recursive_unordered_features_and_collections() {
    let nested = |reverse: bool| {
        let mut inner = vec![closed("person", "third"), closed("gender", "masc")];
        if reverse {
            inner.reverse();
        }
        let mut values = vec![
            closed("number", "plural"),
            FeatureValue {
                feature: "agreement".into(),
                value: FeatureValueKind::Complex {
                    value: FeatureStructure { values: inner },
                },
            },
        ];
        if reverse {
            values.reverse();
        }
        FeatureStructure { values }
    };
    let mut left = stem("first");
    let mut right = stem("second");
    if let Msa::Stem {
        features,
        exception_features,
        from_parts_of_speech,
        slots,
        ..
    } = &mut left
    {
        *features = Some(nested(false));
        *exception_features = vec!["a".into(), "b".into()];
        *from_parts_of_speech = vec!["n".into(), "v".into()];
        *slots = vec!["slot-left".into()];
    }
    if let Msa::Stem {
        features,
        exception_features,
        from_parts_of_speech,
        slots,
        ..
    } = &mut right
    {
        *features = Some(nested(true));
        *exception_features = vec!["b".into(), "a".into()];
        *from_parts_of_speech = vec!["v".into(), "n".into()];
        *slots = vec!["slot-right".into()];
    }
    assert!(stem_msas_equivalent(&left, &right));
    if let Msa::Stem {
        features: Some(fs), ..
    } = &mut right
    {
        if let FeatureValueKind::Complex { value } = &mut fs.values[0].value {
            value.values[0] = closed("gender", "fem");
        }
    }
    assert!(!stem_msas_equivalent(&left, &right));
}

#[test]
fn unique_stem_none_equals_empty_features() {
    let left = stem("first");
    let mut right = stem("second");
    if let Msa::Stem { features, .. } = &mut right {
        *features = Some(FeatureStructure::default());
    }
    assert!(stem_msas_equivalent(&left, &right));
    assert!(stem_msas_equivalent(&right, &left));
}

#[test]
fn unique_stem_grammatical_field_differences_survive() {
    let base = stem("first");
    for field in 0..5 {
        let mut other = stem("second");
        if let Msa::Stem {
            part_of_speech,
            inflection_class,
            features,
            exception_features,
            from_parts_of_speech,
            ..
        } = &mut other
        {
            match field {
                0 => *part_of_speech = None,
                1 => *inflection_class = Some("class".into()),
                2 => {
                    *features = Some(FeatureStructure {
                        values: vec![closed("number", "plural")],
                    })
                }
                3 => exception_features.push("exception".into()),
                _ => from_parts_of_speech.push("other-pos".into()),
            }
        }
        assert!(!stem_msas_equivalent(&base, &other), "field {field}");
    }
}

#[test]
fn unique_stem_stable_first_and_non_stem_exclusion() {
    let msas = vec![
        Msa::Unclassified {
            guid: "affix".into(),
            part_of_speech: None,
        },
        stem("first"),
        stem("duplicate"),
    ];
    let retained = unique_stem_msas(&msas);
    assert_eq!(retained.len(), 1);
    assert!(std::ptr::eq(retained[0], &msas[1]));
}
