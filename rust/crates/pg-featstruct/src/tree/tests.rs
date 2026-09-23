use super::*;

const OUT_OF_ORDER: &str = r#"{"entries":[[3,{"Symbolic":2}],[1,{"Symbolic":1}]]}"#;
const DUPLICATE: &str = r#"{"entries":[[1,{"Symbolic":1}],[1,{"Symbolic":2}]]}"#;
const NESTED_DUPLICATE: &str =
    r#"{"entries":[[1,{"Complex":{"entries":[[2,{"Symbolic":1}],[2,{"Symbolic":2}]]}}]]}"#;

#[test]
fn builder_sorts_and_dedups() {
    let mut b = FeatureStructBuilder::new();
    b.add(FeatId(3), FeatureValue::Symbolic(SymbolBits(0b10)));
    b.add(FeatId(1), FeatureValue::Symbolic(SymbolBits(0b01)));
    b.add(FeatId(3), FeatureValue::Symbolic(SymbolBits(0b11))); // overwrite
    let fs = b.build();
    assert_eq!(fs.len(), 2);
    assert_eq!(fs.entries()[0].0, FeatId(1));
    assert_eq!(
        fs.get(FeatId(3)),
        Some(&FeatureValue::Symbolic(SymbolBits(0b11)))
    );
    assert_eq!(fs.get(FeatId(2)), None);
}

#[test]
fn structural_equality_is_order_independent() {
    let mut a = FeatureStructBuilder::new();
    a.add(FeatId(1), FeatureValue::Symbolic(SymbolBits(1)));
    a.add(FeatId(2), FeatureValue::Complex(FeatureStruct::EMPTY));
    let mut b = FeatureStructBuilder::new();
    b.add(FeatId(2), FeatureValue::Complex(FeatureStruct::EMPTY));
    b.add(FeatId(1), FeatureValue::Symbolic(SymbolBits(1)));
    assert_eq!(a.build(), b.build());
}

#[test]
fn deserialize_canonicalizes_out_of_order_entries() {
    let fs: FeatureStruct = serde_json::from_str(OUT_OF_ORDER).unwrap();
    assert_eq!(
        fs.entries()
            .iter()
            .map(|(feat, _)| feat.0)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
}

#[test]
fn deserialize_rejects_duplicate_features() {
    let error = serde_json::from_str::<FeatureStruct>(DUPLICATE).unwrap_err();
    assert!(error.to_string().contains("duplicate feature 1"));
}

#[test]
fn deserialize_rejects_duplicates_in_nested_complex_values() {
    let error = serde_json::from_str::<FeatureStruct>(NESTED_DUPLICATE).unwrap_err();
    assert!(error.to_string().contains("duplicate feature 2"));
}

#[test]
fn serde_round_trip_preserves_the_public_wire_shape() {
    let fs: FeatureStruct = serde_json::from_str(OUT_OF_ORDER).unwrap();
    let json = serde_json::to_string(&fs).unwrap();
    assert_eq!(
        json,
        r#"{"entries":[[1,{"Symbolic":1}],[3,{"Symbolic":2}]]}"#
    );
    assert_eq!(serde_json::from_str::<FeatureStruct>(&json).unwrap(), fs);
}
