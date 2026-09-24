use super::*;

fn table_with(defs: Vec<RawCharDef>) -> Result<CharDefTable, ModelError> {
    let feat_sys = PhonFeatureSystem::from_raw(vec![]).unwrap();
    CharDefTable::from_raw("table1".to_string(), None, defs, &feat_sys)
}

fn seg(xml_id: &str, reps: &[&str]) -> RawCharDef {
    RawCharDef {
        xml_id: xml_id.to_string(),
        source_guid: None,
        kind: CharDefKind::Segment,
        representations: reps.iter().map(|s| s.to_string()).collect(),
        feature_values: vec![],
    }
}

fn bnd(xml_id: &str, reps: &[&str]) -> RawCharDef {
    RawCharDef {
        xml_id: xml_id.to_string(),
        source_guid: None,
        kind: CharDefKind::Boundary,
        representations: reps.iter().map(|s| s.to_string()).collect(),
        feature_values: vec![],
    }
}

#[test]
fn multi_representation_char_def_shares_one_id() {
    let table = table_with(vec![seg("char4", &["m", "n"])]).unwrap();
    assert_eq!(table.len(), 1);
    let id_m = table.lookup_nfd("m").unwrap();
    let id_n = table.lookup_nfd("n").unwrap();
    assert_eq!(id_m, id_n);
    assert_eq!(table.get(id_m).representations(), &["m", "n"]);
}

#[test]
fn duplicate_representation_across_defs_is_an_error() {
    let err = table_with(vec![seg("char1", &["s"]), seg("char2", &["s"])]).unwrap_err();
    assert!(matches!(err, ModelError::DuplicateRepresentation(_)));
}

#[test]
fn boundary_kind_is_preserved() {
    // A boundary's `feature_lanes()` is not empty: it is `feat_sys.len()`-wide with `Type` pinned to `Boundary`-only bits.
    let table = table_with(vec![bnd("char41", &["+"])]).unwrap();
    let id = table.lookup_nfd("+").unwrap();
    let cd = table.get(id);
    assert_eq!(cd.kind(), CharDefKind::Boundary);
    assert_eq!(cd.feature_lanes(), &[crate::featsys::TYPE_BOUNDARY_BITS]);
}

#[test]
fn representations_are_normalized_to_nfd_for_lookup() {
    // U+00E9 (precomposed é) must be found via its NFD key (e + combining acute).
    let table = table_with(vec![seg("char1", &["\u{00e9}"])]).unwrap();
    assert!(table.lookup_nfd("e\u{0301}").is_some());
    assert!(table.lookup_nfd("\u{00e9}").is_none()); // precomposed form is not the lookup key
}

#[test]
fn feature_lanes_default_to_full_mask_and_override_on_explicit_value() {
    // Build a tiny 2-feature system by hand via the raw path used elsewhere in the crate.
    use crate::featsys::RawFeature;
    let feat_sys = PhonFeatureSystem::from_raw(vec![
        RawFeature {
            xml_id: "feat1".into(),
            name: "voice".into(),
            symbols: vec![("symP".into(), "+".into()), ("symM".into(), "-".into())],
            default_symbol: None,
        },
        RawFeature {
            xml_id: "feat2".into(),
            name: "place".into(),
            symbols: vec![("symA".into(), "lab".into()), ("symB".into(), "vel".into())],
            default_symbol: None,
        },
    ])
    .unwrap();
    let raw = vec![RawCharDef {
        xml_id: "char1".to_string(),
        source_guid: None,
        kind: CharDefKind::Segment,
        representations: vec!["p".to_string()],
        feature_values: vec![RawFeatureValue {
            feature_xml_id: "feat1".to_string(),
            symbol_xml_ids: vec!["symM".to_string()],
        }],
    }];
    let table = CharDefTable::from_raw("t".to_string(), None, raw, &feat_sys).unwrap();
    let id = table.lookup_nfd("p").unwrap();
    let lanes = table.get(id).feature_lanes();
    // 2 authored features + the always-appended synthetic `Type` feature.
    assert_eq!(lanes.len(), 3);
    assert_eq!(lanes[0], 0b10); // feat1 explicitly set to symM (index 1)
    assert_eq!(lanes[1], feat_sys.mask(FlatIndex(1))); // feat2 unmentioned -> full mask
                                                       // Type is pinned to Segment-only bits, never left at the default full mask.
    assert_eq!(
        lanes[feat_sys.type_flat().0 as usize],
        crate::featsys::TYPE_SEGMENT_BITS
    );
}

#[test]
fn greedy_longest_match_prefers_two_char_rep() {
    // The table itself doesn't do the matching (that's segment.rs), but confirm "s", "y", and "sy" are all independently resolvable so its greedy scan has real work to do.
    let table = table_with(vec![
        seg("c1", &["s"]),
        seg("c2", &["y"]),
        seg("c3", &["sy"]),
    ])
    .unwrap();
    assert!(table.lookup_nfd("s").is_some());
    assert!(table.lookup_nfd("y").is_some());
    assert!(table.lookup_nfd("sy").is_some());
    assert_ne!(table.lookup_nfd("s"), table.lookup_nfd("sy"));
}
