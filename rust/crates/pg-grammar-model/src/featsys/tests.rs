use super::*;

fn raw(xml_id: &str, name: &str, symbols: &[(&str, &str)]) -> RawFeature {
    raw_with_default(xml_id, name, symbols, None)
}

fn raw_with_default(
    xml_id: &str,
    name: &str,
    symbols: &[(&str, &str)],
    default_symbol: Option<&str>,
) -> RawFeature {
    RawFeature {
        xml_id: xml_id.to_string(),
        name: name.to_string(),
        symbols: symbols
            .iter()
            .map(|(id, n)| (id.to_string(), n.to_string()))
            .collect(),
        default_symbol: default_symbol.map(str::to_string),
    }
}

#[test]
fn zero_authored_features_still_carries_type() {
    // A grammar with zero authored `<PhonologicalFeatureSystem>` features (Sena) still gets `len() == 1` for the synthetic `Type` feature, not 0.
    let fs = PhonFeatureSystem::from_raw(vec![]).unwrap();
    assert!(fs.is_empty(), "zero *authored* features");
    assert_eq!(
        fs.len(),
        1,
        "Type is always appended, even with zero authored features"
    );
    assert_eq!(fs.flat_index("nope"), None);
    assert_eq!(fs.type_flat(), FlatIndex(0));
    assert_eq!(fs.feature_name(fs.type_flat()), "Type");
    assert_eq!(
        fs.symbol_index(fs.type_flat(), "Segment"),
        Some(TYPE_SEGMENT_SYMBOL)
    );
    assert_eq!(
        fs.symbol_index(fs.type_flat(), "Boundary"),
        Some(TYPE_BOUNDARY_SYMBOL)
    );
    assert_eq!(fs.mask(fs.type_flat()), 0b11);
}

// --- Finding N2: SymbolicFeature@defaultSymbol -----------------------------------------

#[test]
fn no_default_symbol_yields_none() {
    let fs =
        PhonFeatureSystem::from_raw(vec![raw("f", "voice", &[("p", "+"), ("m", "-")])]).unwrap();
    assert_eq!(fs.default_bits(FlatIndex(0)), None);
    // The always-appended synthetic Type feature never has a default either.
    assert_eq!(fs.default_bits(fs.type_flat()), None);
}

#[test]
fn default_symbol_resolves_to_its_own_dense_bit() {
    let fs = PhonFeatureSystem::from_raw(vec![raw_with_default(
        "f",
        "voice",
        &[("p", "+"), ("m", "-")],
        Some("m"),
    )])
    .unwrap();
    // "m" is dense symbol index 1 -> bit 0b10.
    assert_eq!(fs.default_bits(FlatIndex(0)), Some(0b10));
}

#[test]
fn default_symbol_referencing_an_unknown_id_is_a_semantic_error() {
    let err = PhonFeatureSystem::from_raw(vec![raw_with_default(
        "f",
        "voice",
        &[("p", "+"), ("m", "-")],
        Some("nope"),
    )])
    .unwrap_err();
    assert!(matches!(err, ModelError::Semantic(_)));
}

#[test]
fn dense_indices_in_document_order() {
    let fs = PhonFeatureSystem::from_raw(vec![
        raw("feat200", "dr", &[("sym1", "+"), ("sym2", "-")]),
        raw(
            "feat271",
            "OrthPlace",
            &[("symA", "velar"), ("symB", "labial")],
        ),
    ])
    .unwrap();
    // 2 authored features + the always-appended synthetic `Type` feature.
    assert!(!fs.is_empty());
    assert_eq!(fs.len(), 3);
    assert_eq!(fs.flat_index("feat200"), Some(FlatIndex(0)));
    assert_eq!(fs.flat_index("feat271"), Some(FlatIndex(1)));
    assert_eq!(fs.feature_name(FlatIndex(0)), "dr");
    assert_eq!(fs.symbol_index(FlatIndex(0), "sym1"), Some(0));
    assert_eq!(fs.symbol_index(FlatIndex(0), "sym2"), Some(1));
    assert_eq!(fs.symbol_index(FlatIndex(1), "symB"), Some(1));
    assert_eq!(fs.mask(FlatIndex(0)), 0b11);
    // `Type` is appended last, never shifting the authored features' indices.
    assert_eq!(fs.type_flat(), FlatIndex(2));
}

#[test]
fn sixty_four_symbols_is_unsupported() {
    let symbols: Vec<(String, String)> =
        (0..64).map(|i| (format!("s{i}"), format!("{i}"))).collect();
    let symbols_ref: Vec<(&str, &str)> = symbols
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let err = PhonFeatureSystem::from_raw(vec![raw("featBig", "big", &symbols_ref)]).unwrap_err();
    assert!(matches!(err, ModelError::Unsupported(_)));
}

#[test]
fn sixty_three_symbols_is_supported() {
    let symbols: Vec<(String, String)> =
        (0..63).map(|i| (format!("s{i}"), format!("{i}"))).collect();
    let symbols_ref: Vec<(&str, &str)> = symbols
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let fs = PhonFeatureSystem::from_raw(vec![raw("featBig", "big", &symbols_ref)]).unwrap();
    assert_eq!(fs.symbol_count(FlatIndex(0)), 63);
}
