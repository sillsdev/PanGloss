mod provenance_contract_tests;

use super::*;

#[test]
fn a_load_decision_written_by_0_11_to_0_14_still_deserializes() {
    // 0.11.0 to 0.14.0 wrote this reason code for DoNotUseForParsing entries (divergence 121).
    let decision = LoadDecision {
        subject: InventoryKey::object(InventoryKind::Entry, "00000000-0000-0000-0000-000000000030"),
        pipeline_stage: LoadPipelineStage::Import,
        decision_ordinal: 0,
        context_key: String::new(),
        disposition: LoadDisposition::NotConsidered,
        loaded: None,
        reason_code: LoadReasonCode::Represented,
        effective_value_json: None,
        issue_code: None,
    };
    let current = serde_json::to_string(&decision).unwrap();
    let old = current.replace(
        r#""reasonCode":{"kind":"represented"}"#,
        r#""reasonCode":{"kind":"doNotUseForParsing"}"#,
    );
    assert_ne!(
        old, current,
        "the reason code must appear in the JSON shape assumed here"
    );
    let read: LoadDecision = serde_json::from_str(&old).unwrap();
    assert_eq!(read.reason_code, LoadReasonCode::DoNotUseForParsing);
    assert_eq!(read.reason_code.as_str(), "doNotUseForParsing");
}
