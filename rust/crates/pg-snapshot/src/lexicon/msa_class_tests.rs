use super::*;

#[test]
fn fieldworks_class_matches_msa_variant() {
    for (json, expected) in [
        (r#"{"kind":"stem","guid":"a"}"#, FwClass::MoStemMsa),
        (
            r#"{"kind":"inflectional","guid":"b"}"#,
            FwClass::MoInflAffMsa,
        ),
        (
            r#"{"kind":"derivational","guid":"c"}"#,
            FwClass::MoDerivAffMsa,
        ),
        (
            r#"{"kind":"unclassified","guid":"d"}"#,
            FwClass::MoUnclassifiedAffixMsa,
        ),
    ] {
        let msa: Msa = serde_json::from_str(json).expect("MSA variant deserializes");
        assert_eq!(msa.fw_class(), expected);
    }
}
