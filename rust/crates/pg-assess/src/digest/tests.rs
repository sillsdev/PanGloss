use super::*;

fn id(morphemes: &[&str], root_index: i32, category: Option<&str>) -> AnalysisIdentity {
    AnalysisIdentity {
        morphemes: morphemes.iter().map(|m| Some(m.to_string())).collect(),
        root_index,
        category: category.map(str::to_string),
    }
}

#[test]
fn identity_digest_is_stable_and_prefixed() {
    let d = identity_digest(&id(&["a", "b"], 0, Some("noun")));
    assert!(d.starts_with("sha256:"), "{d}");
    assert_eq!(d.len(), "sha256:".len() + 64);
    assert_eq!(d, identity_digest(&id(&["a", "b"], 0, Some("noun"))));
}

#[test]
fn every_identity_field_changes_the_digest() {
    let base = identity_digest(&id(&["a", "b"], 0, Some("noun")));
    assert_ne!(
        base,
        identity_digest(&id(&["a", "c"], 0, Some("noun"))),
        "morpheme"
    );
    assert_ne!(
        base,
        identity_digest(&id(&["b", "a"], 0, Some("noun"))),
        "order"
    );
    assert_ne!(
        base,
        identity_digest(&id(&["a", "b"], 1, Some("noun"))),
        "root index"
    );
    assert_ne!(
        base,
        identity_digest(&id(&["a", "b"], 0, Some("verb"))),
        "category"
    );
    assert_ne!(
        base,
        identity_digest(&id(&["a", "b"], 0, None)),
        "no category"
    );
}

#[test]
fn a_guessed_slot_differs_from_a_morpheme_literally_named_null() {
    let guessed = AnalysisIdentity {
        morphemes: vec![None],
        root_index: 0,
        category: None,
    };
    assert_ne!(
        identity_digest(&guessed),
        identity_digest(&id(&["null"], 0, None))
    );
}

#[test]
fn projections_are_namespaced_so_the_same_value_digests_differently() {
    // The whole point of binding the projection name into the preimage: two different projections over identical bytes must never collide.
    let v = json!({ "cases": [] });
    assert_ne!(
        digest_projection(SEMANTIC_PROJECTION, &v).unwrap(),
        digest_projection(OUTCOME_PROJECTION, &v).unwrap()
    );
}

#[test]
fn projection_digests_ignore_key_order() {
    let a = serde_json::from_str::<Value>(r#"{"b":1,"a":2}"#).unwrap();
    let b = serde_json::from_str::<Value>(r#"{"a":2,"b":1}"#).unwrap();
    assert_eq!(
        digest_projection(SEMANTIC_PROJECTION, &a).unwrap(),
        digest_projection(SEMANTIC_PROJECTION, &b).unwrap()
    );
}

#[test]
fn sha256_bytes_is_exact_and_does_not_normalize_line_endings() {
    // The whole reason `sourceSha256` cannot reuse `pg_lexicon::grammar_source_fingerprint`.
    assert_ne!(sha256_bytes(b"a\r\nb"), sha256_bytes(b"a\nb"));
}
