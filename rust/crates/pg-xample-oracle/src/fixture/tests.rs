use super::*;

const REAL_MANIFEST: &str = r#"version: 1
base_sha256: 1c7cd5c9113b2dd501a72c6f80085975a7fff15938e228c48fe0421da98ea58f
cases:
  - id: empty-phoneme-inventory
    operations:
      - remove_all_phonemes:
          require_unreferenced: true
    expect:
      xample_projection: same_as_base
      hc_analyses: same_as_base
      inferred_segments: [x, k]
  - id: remove-k-only
    operations:
      - remove_phoneme:
          guid: 46fbfc78-6e09-4421-8781-ae99a0013c97
          assert_representations: [k]
          require_unreferenced: true
    expect:
      xample_projection: same_as_base
      hc_analyses: same_as_base
      inferred_segments: [k]
"#;

#[test]
fn real_manifest_parses_both_cases() {
    let manifest = parse_manifest(REAL_MANIFEST).expect("the real checked-in shape must parse");
    assert_eq!(
        manifest.base_sha256,
        "1c7cd5c9113b2dd501a72c6f80085975a7fff15938e228c48fe0421da98ea58f"
    );
    assert_eq!(manifest.cases.len(), 2);

    let empty = manifest
        .case("empty-phoneme-inventory")
        .expect("case present");
    assert_eq!(
        empty.operations,
        vec![MutationOperation::RemoveAllPhonemes {
            require_unreferenced: true
        }]
    );
    assert_eq!(empty.expect.xample_projection, ExpectRelation::SameAsBase);
    assert_eq!(empty.expect.hc_analyses, ExpectRelation::SameAsBase);
    assert_eq!(
        empty.expect.inferred_segments,
        vec!["x".to_string(), "k".to_string()]
    );

    let remove_k = manifest.case("remove-k-only").expect("case present");
    assert_eq!(
        remove_k.operations,
        vec![MutationOperation::RemovePhoneme {
            guid: "46fbfc78-6e09-4421-8781-ae99a0013c97".to_string(),
            assert_representations: vec!["k".to_string()],
            require_unreferenced: true,
        }]
    );
}

#[test]
fn wrong_version_is_refused() {
    let text = REAL_MANIFEST.replacen("version: 1", "version: 2", 1);
    let err = parse_manifest(&text).expect_err("version 2 must be refused");
    assert!(matches!(err, FixtureError::UnsupportedVersion { found: 2 }));
    assert!(err.to_string().contains('2') && err.to_string().contains('1'));
}

#[test]
fn absent_base_sha256_is_refused() {
    let value: serde_yaml::Value = serde_yaml::from_str(REAL_MANIFEST).unwrap();
    let mut mapping = value.as_mapping().unwrap().clone();
    mapping.remove("base_sha256");
    let text = serde_yaml::to_string(&mapping).unwrap();
    let err = parse_manifest(&text).expect_err("a manifest missing base_sha256 must be refused");
    assert!(matches!(err, FixtureError::Malformed(_)));
}

#[test]
fn unknown_operation_is_refused() {
    let text = REAL_MANIFEST.replacen("remove_all_phonemes:", "remove_every_phoneme_ever:", 1);
    let err = parse_manifest(&text).expect_err("an unrecognized operation key must be refused");
    match err {
        FixtureError::UnknownOperation { case_id, found } => {
            assert_eq!(case_id, "empty-phoneme-inventory");
            assert_eq!(found, "remove_every_phoneme_ever");
        }
        other => panic!("expected UnknownOperation, got {other:?}"),
    }
}

#[test]
fn unknown_expect_relation_is_refused() {
    let text = REAL_MANIFEST.replacen(
        "xample_projection: same_as_base",
        "xample_projection: definitely_different",
        1,
    );
    let err = parse_manifest(&text)
        .expect_err("an expect value outside the v1 vocabulary must be refused");
    match err {
        FixtureError::UnsupportedExpectRelation { field, found, .. } => {
            assert_eq!(field, "xample_projection");
            assert_eq!(found, "definitely_different");
        }
        other => panic!("expected UnsupportedExpectRelation, got {other:?}"),
    }
}

#[test]
fn digest_mismatch_is_refused() {
    let manifest = parse_manifest(REAL_MANIFEST).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "pg-xample-oracle-fixture-digest-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("not-the-real-project.fwdata");
    std::fs::write(&path, b"definitely not the checked-in project bytes").unwrap();
    let err =
        verify_base_sha256(&manifest, &path).expect_err("a mismatched digest must be refused");
    assert!(matches!(err, FixtureError::Sha256Mismatch { .. }));
    assert!(err.to_string().contains(&manifest.base_sha256));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn digest_match_succeeds() {
    let manifest = parse_manifest(REAL_MANIFEST).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "pg-xample-oracle-fixture-digest-match-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("project.fwdata");
    // Any bytes whose sha256 we then declare as the manifest's own -- proving the positive path independent of a real .fwdata file.
    let bytes = b"hello xample";
    std::fs::write(&path, bytes).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = format!("{:x}", hasher.finalize());
    let manifest = PhonologyMutations {
        base_sha256: digest,
        ..manifest
    };
    verify_base_sha256(&manifest, &path).expect("matching digest must succeed");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn to_request_json_matches_the_helper_wire_schema() {
    let manifest = parse_manifest(REAL_MANIFEST).unwrap();
    let case = manifest.case("remove-k-only").unwrap();
    let request = case.to_request_json(&manifest.base_sha256);
    assert_eq!(request["schemaVersion"], 1);
    assert_eq!(request["caseId"], "remove-k-only");
    assert_eq!(request["baseSha256"], manifest.base_sha256);
    let ops = request["operations"].as_array().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["op"], "remove_phoneme");
    assert_eq!(ops[0]["guid"], "46fbfc78-6e09-4421-8781-ae99a0013c97");
    assert_eq!(ops[0]["assertRepresentations"], serde_json::json!(["k"]));
    assert_eq!(ops[0]["requireUnreferenced"], true);
}
