use super::*;
use crate::AuthoredRef;

fn sid(n: char) -> SignatureId {
    SignatureId::parse(&format!("sig_{}", n.to_string().repeat(64))).unwrap()
}
fn signature(id: SignatureId) -> crate::ClassSignature {
    crate::ClassSignature {
        id,
        pos: Some(AuthoredRef {
            id: "p".into(),
            label: "P".into(),
        }),
        features: vec![],
        mpr: vec![],
        canonical_encoding: "{}".into(),
        entry_count: 1,
    }
}
fn form(id: &str, surface: &str, predictions: Vec<SignatureId>) -> DiagnosticForm {
    DiagnosticForm {
        id: id.into(),
        surface: surface.into(),
        predictions: predictions
            .into_iter()
            .map(|signature_id| FormPrediction {
                signature_id,
                derivations: vec![],
            })
            .collect(),
    }
}

#[test]
fn guide_applies_strict_logic_and_supports_replace_and_undo() {
    let (a, b, c) = (sid('a'), sid('b'), sid('c'));
    let matrix = ClassificationMatrix {
        stem: "x".into(),
        candidates: vec![
            signature(a.clone()),
            signature(b.clone()),
            signature(c.clone()),
        ],
        forms: vec![
            form("one", "xa", vec![a.clone(), b.clone()]),
            form("two", "xb", vec![b.clone()]),
        ],
        exhaustive: true,
        truncation_reason: None,
    };
    let mut guide = ClassificationGuide::new(matrix);
    guide.answer("one", Judgment::No).unwrap();
    assert_eq!(guide.remaining_signatures(), vec![c.clone()]);
    guide.answer("one", Judgment::Yes).unwrap();
    assert_eq!(guide.remaining_signatures(), vec![a.clone(), b.clone()]);
    assert!(guide.undo());
    assert_eq!(guide.remaining_signatures(), vec![c]);
    guide.answer("one", Judgment::Unknown).unwrap();
    assert_eq!(guide.remaining_signatures().len(), 3);
    assert!(guide.undo());
}

#[test]
fn adaptive_form_splits_the_remaining_set_and_unknown_forms_are_rejected() {
    let (a, b, c) = (sid('a'), sid('b'), sid('c'));
    let matrix = ClassificationMatrix {
        stem: "x".into(),
        candidates: vec![
            signature(a.clone()),
            signature(b.clone()),
            signature(c.clone()),
        ],
        forms: vec![
            form("weak", "z", vec![a.clone(), b.clone()]),
            form("strong", "a", vec![b]),
        ],
        exhaustive: false,
        truncation_reason: Some(TruncationReason::DerivationLimit),
    };
    let mut guide = ClassificationGuide::new(matrix);
    assert_eq!(guide.next_form().unwrap().surface, "a");
    let err = guide.answer("missing", Judgment::Yes).unwrap_err();
    assert_eq!(err.code, "unknown_form");
    assert!(!guide.final_selection().exhaustive);
}
