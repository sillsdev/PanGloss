use super::*;
use crate::xml::{parse_fwdata_reader, RawGraph};

fn graph(records: &[String]) -> RawGraph {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<languageproject version=\"7000072\">\n{}\n</languageproject>\n",
        records.join("\n")
    );
    parse_fwdata_reader(xml.as_bytes()).expect("test graph parses")
}

fn rt(class: &str, guid: &str, body: &str) -> String {
    format!("<rt class=\"{class}\" guid=\"{guid}\">{body}</rt>")
}

fn refs(field: &str, guids: &[&str]) -> String {
    let inner: String = guids
        .iter()
        .map(|g| format!("<objsur guid=\"{g}\" t=\"r\" />"))
        .collect();
    format!("<{field}>{inner}</{field}>")
}

fn owns(field: &str, guids: &[&str]) -> String {
    let inner: String = guids
        .iter()
        .map(|g| format!("<objsur guid=\"{g}\" t=\"o\" />"))
        .collect();
    format!("<{field}>{inner}</{field}>")
}

fn int(field: &str, value: i64) -> String {
    format!("<{field} val=\"{value}\" />")
}

/// `this.EqualsMsa(other)` over `records`.
fn equals(records: &[String], this: &str, other: &str) -> bool {
    let graph = graph(records);
    let mut ctx = Ctx::new(&graph);
    ctx.analysis_ws = vec!["en".to_string()];
    equals_msa(&ctx, graph.get(this).unwrap(), graph.get(other).unwrap())
}

fn stem(guid: &str, body: &str) -> String {
    rt("MoStemMsa", guid, body)
}

fn closed(guid: &str, feature: &str, value: &str) -> String {
    rt(
        "FsClosedValue",
        guid,
        &format!("{}{}", refs("Feature", &[feature]), refs("Value", &[value])),
    )
}

fn fs(guid: &str, body: &str) -> String {
    rt("FsFeatStruc", guid, body)
}

/// Two stem MSAs `a` and `b` whose `MsFeatures` are `fs-a` and `fs-b`, plus `extra` records.
fn stems_with_features(extra: &[String]) -> Vec<String> {
    let mut records = vec![
        stem("a", &owns("MsFeatures", &["fs-a"])),
        stem("b", &owns("MsFeatures", &["fs-b"])),
    ];
    records.extend(extra.iter().cloned());
    records
}

#[test]
fn every_msa_kind_is_reflexive_and_equals_an_identical_twin() {
    let stem_body = format!(
        "{}{}{}{}",
        refs("PartOfSpeech", &["pos"]),
        refs("InflectionClass", &["ic"]),
        refs("FromPartsOfSpeech", &["pos2"]),
        refs("ProdRestrict", &["pr"])
    );
    let infl_body = format!(
        "{}{}{}",
        refs("PartOfSpeech", &["pos"]),
        refs("Slots", &["s1", "s2"]),
        refs("FromProdRestrict", &["pr"])
    );
    let deriv_body = format!(
        "{}{}{}{}{}{}{}{}",
        refs("FromPartOfSpeech", &["pos"]),
        refs("ToPartOfSpeech", &["pos2"]),
        refs("FromInflectionClass", &["ic"]),
        refs("ToInflectionClass", &["ic2"]),
        refs("FromStemName", &["sn"]),
        refs("FromProdRestrict", &["pr"]),
        refs("ToProdRestrict", &["pr2"]),
        refs("Stratum", &["st"])
    );
    for (class, body) in [
        ("MoStemMsa", stem_body),
        ("MoInflAffMsa", infl_body),
        ("MoDerivAffMsa", deriv_body),
        ("MoUnclassifiedAffixMsa", refs("PartOfSpeech", &["pos"])),
    ] {
        let records = vec![rt(class, "a", &body), rt(class, "b", &body)];
        assert!(equals(&records, "a", "a"), "{class} reflexive");
        assert!(equals(&records, "a", "b"), "{class} twin");
        assert!(equals(&records, "b", "a"), "{class} twin, reversed");
    }
}

#[test]
fn different_msa_classes_are_never_equal() {
    let pos = refs("PartOfSpeech", &["pos"]);
    let records = vec![
        rt("MoInflAffMsa", "a", &pos),
        rt("MoUnclassifiedAffixMsa", "b", &pos),
    ];
    assert!(!equals(&records, "a", "b"));
}

#[test]
fn stem_msa_compares_category_class_features_attachment_and_restrictions() {
    let base = [
        ("PartOfSpeech", "pos"),
        ("InflectionClass", "ic"),
        ("FromPartsOfSpeech", "pos2"),
        ("ProdRestrict", "pr"),
    ];
    let body = |skip: Option<&str>| -> String {
        base.iter()
            .filter(|(field, _)| Some(*field) != skip)
            .map(|(field, guid)| refs(field, &[guid]))
            .collect()
    };
    for (field, _) in base {
        let records = vec![stem("a", &body(None)), stem("b", &body(Some(field)))];
        assert!(!equals(&records, "a", "b"), "{field} must be compared");
        assert!(!equals(&records, "b", "a"), "{field} must be compared");
    }
}

#[test]
fn stem_msa_ignores_slots_stratum_and_the_pos_default_inflection_class() {
    let records = vec![
        stem(
            "a",
            &format!(
                "{}{}{}",
                refs("PartOfSpeech", &["pos"]),
                refs("Slots", &["s1"]),
                refs("Stratum", &["st1"])
            ),
        ),
        stem(
            "b",
            &format!(
                "{}{}",
                refs("PartOfSpeech", &["pos"]),
                refs("Slots", &["s2"])
            ),
        ),
    ];
    assert!(equals(&records, "a", "b"));
}

#[test]
fn reference_collections_compare_as_sets() {
    let records = vec![
        stem("a", &refs("ProdRestrict", &["p1", "p2"])),
        stem("b", &refs("ProdRestrict", &["p2", "p1", "P1"])),
        stem("c", &refs("ProdRestrict", &["p1"])),
    ];
    assert!(
        equals(&records, "a", "b"),
        "order and duplicates are ignored"
    );
    assert!(!equals(&records, "a", "c"));
    assert!(!equals(&records, "c", "a"));
}

#[test]
fn inflectional_msa_compares_features_restrictions_category_and_slots() {
    let full = format!(
        "{}{}{}{}",
        refs("PartOfSpeech", &["pos"]),
        refs("Slots", &["s1", "s2"]),
        refs("FromProdRestrict", &["pr"]),
        owns("InflFeats", &["fs-a"])
    );
    let fs_a = fs("fs-a", &owns("FeatureSpecs", &["cv-a"]));
    let cv_a = closed("cv-a", "f", "v");
    let records = |b: &str| {
        vec![
            rt("MoInflAffMsa", "a", &full),
            rt("MoInflAffMsa", "b", b),
            fs_a.clone(),
            cv_a.clone(),
            fs("fs-b", &owns("FeatureSpecs", &["cv-b"])),
            closed("cv-b", "f", "v"),
        ]
    };
    let same = full.replace("fs-a", "fs-b");
    assert!(equals(&records(&same), "a", "b"));
    for (label, changed) in [
        ("category", same.replace("pos", "other")),
        ("slots", same.replace("\"s2\"", "\"s3\"")),
        (
            "slot count",
            same.replace("<objsur guid=\"s2\" t=\"r\" />", ""),
        ),
        ("restrictions", same.replace("\"pr\"", "\"pr2\"")),
        ("features", same.replace(&owns("InflFeats", &["fs-b"]), "")),
    ] {
        assert!(!equals(&records(&changed), "a", "b"), "{label}");
    }
    let affix_category = format!("{same}{}", refs("AffixCategory", &["cat"]));
    assert!(
        equals(&records(&affix_category), "a", "b"),
        "AffixCategory is not compared"
    );
}

#[test]
fn derivational_msa_compares_every_from_and_to_property() {
    let fields = [
        ("FromPartOfSpeech", "pos"),
        ("ToPartOfSpeech", "pos2"),
        ("FromInflectionClass", "ic"),
        ("ToInflectionClass", "ic2"),
        ("FromStemName", "sn"),
        ("FromProdRestrict", "pr"),
        ("ToProdRestrict", "pr2"),
    ];
    let body = |skip: Option<&str>| -> String {
        let mut body: String = fields
            .iter()
            .filter(|(field, _)| Some(*field) != skip)
            .map(|(field, guid)| refs(field, &[guid]))
            .collect();
        for owned_field in ["FromMsFeatures", "ToMsFeatures"] {
            if Some(owned_field) != skip {
                body.push_str(&owns(owned_field, &[&format!("{owned_field}-fs")]));
            }
        }
        body
    };
    let feature_records = |prefix: &str| {
        vec![
            fs(
                &format!("{prefix}FromMsFeatures-fs"),
                &owns("FeatureSpecs", &[&format!("{prefix}from-cv")]),
            ),
            closed(&format!("{prefix}from-cv"), "f", "v1"),
            fs(
                &format!("{prefix}ToMsFeatures-fs"),
                &owns("FeatureSpecs", &[&format!("{prefix}to-cv")]),
            ),
            closed(&format!("{prefix}to-cv"), "f", "v2"),
        ]
    };
    let records = |b: String| {
        let mut records = vec![
            rt("MoDerivAffMsa", "a", &body(None)),
            rt(
                "MoDerivAffMsa",
                "b",
                &b.replace("Features-fs", "Features-fs-b"),
            ),
        ];
        records.extend(feature_records(""));
        let mut b_features = feature_records("");
        for record in &mut b_features {
            *record = record
                .replace("Features-fs\"", "Features-fs-b\"")
                .replace("-cv\"", "-cv-b\"");
        }
        records.extend(b_features);
        records
    };
    assert!(equals(&records(body(None)), "a", "b"));
    for skip in fields
        .iter()
        .map(|(field, _)| *field)
        .chain(["FromMsFeatures", "ToMsFeatures"])
    {
        assert!(!equals(&records(body(Some(skip))), "a", "b"), "{skip}");
    }
    let stratum = format!("{}{}", body(None), refs("Stratum", &["st"]));
    assert!(
        equals(&records(stratum), "a", "b"),
        "Stratum is not compared"
    );
}

#[test]
fn unclassified_msa_compares_only_its_category() {
    let records = vec![
        rt(
            "MoUnclassifiedAffixMsa",
            "a",
            &refs("PartOfSpeech", &["pos"]),
        ),
        rt(
            "MoUnclassifiedAffixMsa",
            "b",
            &refs("PartOfSpeech", &["pos"]),
        ),
        rt("MoUnclassifiedAffixMsa", "c", &refs("PartOfSpeech", &["x"])),
        rt("MoUnclassifiedAffixMsa", "d", ""),
    ];
    assert!(equals(&records, "a", "b"));
    assert!(!equals(&records, "a", "c"));
    assert!(!equals(&records, "a", "d"));
    assert!(!equals(&records, "d", "a"));
}

#[test]
fn an_absent_and_an_empty_feature_structure_are_equal_but_a_typed_one_is_not() {
    let records = vec![
        stem("a", ""),
        stem("b", &owns("MsFeatures", &["fs-b"])),
        fs("fs-b", ""),
        stem("c", &owns("MsFeatures", &["fs-c"])),
        fs("fs-c", &refs("Type", &["t"])),
    ];
    assert!(equals(&records, "a", "b"));
    assert!(equals(&records, "b", "a"));
    assert!(!equals(&records, "a", "c"));
    assert!(!equals(&records, "c", "a"));
}

#[test]
fn feature_structures_compare_type_spec_counts_ref_numbers_and_value_states() {
    let pair = |b_fs_body: &str, b_spec: String| {
        stems_with_features(&[
            fs(
                "fs-a",
                &format!(
                    "{}{}",
                    refs("Type", &["t"]),
                    owns("FeatureSpecs", &["cv-a"])
                ),
            ),
            closed("cv-a", "f", "v"),
            fs("fs-b", b_fs_body),
            b_spec,
        ])
    };
    let typed = format!(
        "{}{}",
        refs("Type", &["t"]),
        owns("FeatureSpecs", &["cv-b"])
    );
    assert!(equals(&pair(&typed, closed("cv-b", "f", "v")), "a", "b"));
    assert!(!equals(
        &pair(&owns("FeatureSpecs", &["cv-b"]), closed("cv-b", "f", "v")),
        "a",
        "b"
    ));
    assert!(!equals(&pair(&typed, closed("cv-b", "f", "w")), "a", "b"));
    assert!(!equals(&pair(&typed, closed("cv-b", "g", "v")), "a", "b"));
    for field in ["RefNumber", "ValueState"] {
        let spec = rt(
            "FsClosedValue",
            "cv-b",
            &format!(
                "{}{}{}",
                refs("Feature", &["f"]),
                refs("Value", &["v"]),
                int(field, 1)
            ),
        );
        assert!(!equals(&pair(&typed, spec), "a", "b"), "{field}");
        let zero = rt(
            "FsClosedValue",
            "cv-b",
            &format!(
                "{}{}{}",
                refs("Feature", &["f"]),
                refs("Value", &["v"]),
                int(field, 0)
            ),
        );
        assert!(
            equals(&pair(&typed, zero), "a", "b"),
            "{field} 0 is the default"
        );
    }
    let two_specs = format!(
        "{}{}",
        refs("Type", &["t"]),
        owns("FeatureSpecs", &["cv-b", "cv-b2"])
    );
    let mut records = pair(&two_specs, closed("cv-b", "f", "v"));
    records.push(closed("cv-b2", "f", "v"));
    assert!(!equals(&records, "a", "b"), "specification counts differ");
}

#[test]
fn feature_structure_equivalence_is_existential_so_it_is_not_symmetric() {
    // a's features {f:v, g:w}; b's features {f:v, f:v} -- every b spec matches some a spec, not conversely.
    let records = stems_with_features(&[
        fs("fs-a", &owns("FeatureSpecs", &["a1", "a2"])),
        closed("a1", "f", "v"),
        closed("a2", "g", "w"),
        fs("fs-b", &owns("FeatureSpecs", &["b1", "b2"])),
        closed("b1", "f", "v"),
        closed("b2", "f", "v"),
    ]);
    assert!(equals(&records, "a", "b"));
    assert!(!equals(&records, "b", "a"));
}

#[test]
fn complex_values_recurse_into_their_nested_structures() {
    let records = |nested_b: &str| {
        stems_with_features(&[
            fs("fs-a", &owns("FeatureSpecs", &["cx-a"])),
            rt(
                "FsComplexValue",
                "cx-a",
                &format!("{}{}", refs("Feature", &["agr"]), owns("Value", &["n-a"])),
            ),
            fs("n-a", &owns("FeatureSpecs", &["n-a-cv"])),
            closed("n-a-cv", "f", "v"),
            fs("fs-b", &owns("FeatureSpecs", &["cx-b"])),
            rt(
                "FsComplexValue",
                "cx-b",
                &format!("{}{}", refs("Feature", &["agr"]), owns("Value", &["n-b"])),
            ),
            fs("n-b", &owns("FeatureSpecs", &["n-b-cv"])),
            closed("n-b-cv", "f", nested_b),
        ])
    };
    assert!(equals(&records("v"), "a", "b"));
    assert!(!equals(&records("w"), "a", "b"));
}

#[test]
fn negated_disjunctive_shared_and_open_values_are_compared_by_their_own_rules() {
    let pair = |class: &str, a_value: &str, b_value: &str| {
        stems_with_features(&[
            fs("fs-a", &owns("FeatureSpecs", &["sa"])),
            rt(
                class,
                "sa",
                &format!("{}{a_value}", refs("Feature", &["f"])),
            ),
            fs("fs-b", &owns("FeatureSpecs", &["sb"])),
            rt(
                class,
                "sb",
                &format!("{}{b_value}", refs("Feature", &["f"])),
            ),
        ])
    };
    for class in ["FsNegatedValue", "FsSharedValue"] {
        let v = refs("Value", &["v"]);
        assert!(equals(&pair(class, &v, &v), "a", "b"), "{class}");
        assert!(
            !equals(&pair(class, &v, &refs("Value", &["w"])), "a", "b"),
            "{class}"
        );
    }
    assert!(equals(
        &pair(
            "FsDisjunctiveValue",
            &refs("Value", &["v", "w"]),
            &refs("Value", &["w", "v"])
        ),
        "a",
        "b"
    ));
    assert!(!equals(
        &pair(
            "FsDisjunctiveValue",
            &refs("Value", &["v", "w"]),
            &refs("Value", &["v"])
        ),
        "a",
        "b"
    ));
    let open = |en: &str, fr: &str| {
        format!("<Value><AUni ws=\"en\">{en}</AUni><AUni ws=\"fr\">{fr}</AUni></Value>")
    };
    assert!(
        equals(
            &pair("FsOpenValue", &open("x", "1"), &open("x", "2")),
            "a",
            "b"
        ),
        "only the default analysis writing system is compared"
    );
    assert!(!equals(
        &pair("FsOpenValue", &open("x", "1"), &open("y", "1")),
        "a",
        "b"
    ));
    let negated_vs_closed = stems_with_features(&[
        fs("fs-a", &owns("FeatureSpecs", &["sa"])),
        rt(
            "FsNegatedValue",
            "sa",
            &format!("{}{}", refs("Feature", &["f"]), refs("Value", &["v"])),
        ),
        fs("fs-b", &owns("FeatureSpecs", &["sb"])),
        closed("sb", "f", "v"),
    ]);
    assert!(
        !equals(&negated_vs_closed, "a", "b"),
        "specification classes must match"
    );
}

#[test]
fn feature_disjunctions_are_compared() {
    let records = |b_inner: &str| {
        stems_with_features(&[
            fs("fs-a", &owns("FeatureDisjunctions", &["d-a"])),
            rt("FsFeatStrucDisj", "d-a", &owns("Contents", &["c-a"])),
            fs("c-a", &owns("FeatureSpecs", &["c-a-cv"])),
            closed("c-a-cv", "f", "v"),
            fs("fs-b", &owns("FeatureDisjunctions", &["d-b"])),
            rt("FsFeatStrucDisj", "d-b", &owns("Contents", &["c-b"])),
            fs("c-b", &owns("FeatureSpecs", &["c-b-cv"])),
            closed("c-b-cv", "f", b_inner),
        ])
    };
    assert!(equals(&records("v"), "a", "b"));
    assert!(!equals(&records("w"), "a", "b"));
    let none = stems_with_features(&[fs("fs-a", ""), fs("fs-b", "")]);
    let mut one_side = none.clone();
    one_side[2] = fs("fs-a", &owns("FeatureDisjunctions", &["d-a"]));
    one_side.push(rt("FsFeatStrucDisj", "d-a", ""));
    assert!(equals(&none, "a", "b"));
    assert!(
        !equals(&one_side, "a", "b"),
        "a disjunction makes a structure non-empty"
    );
}

#[test]
fn equal_msas_lists_each_msas_equal_candidates_in_entry_order() {
    let pos = refs("PartOfSpeech", &["pos"]);
    let graph = graph(&[
        stem("s1", &pos),
        rt("MoUnclassifiedAffixMsa", "u1", &pos),
        stem("s2", &pos),
        stem("s3", &refs("PartOfSpeech", &["other"])),
        stem("s4", &pos),
    ]);
    let ctx = Ctx::new(&graph);
    let equal = equal_msas(&ctx, &["s1", "u1", "s2", "s3", "s4", "dangling"]);
    let as_pairs: Vec<(&str, Vec<&str>)> = equal
        .iter()
        .map(|e| {
            (
                e.msa.as_str(),
                e.equal_to.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        as_pairs,
        vec![
            ("s1", vec!["s2", "s4"]),
            ("s2", vec!["s1", "s4"]),
            ("s4", vec!["s1", "s2"]),
        ]
    );
}
