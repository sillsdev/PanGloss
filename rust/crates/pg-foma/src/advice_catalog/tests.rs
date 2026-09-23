use super::*;

fn sample() -> AdviceCatalog {
    AdviceCatalog {
        schema_version: 1,
        entries: vec![AdviceEntry {
            shape_key: "a-shape".to_string(),
            backend_id: "foma".to_string(),
            route: "route".to_string(),
            failed_predicate: "predicate".to_string(),
            evidence_refs: vec![EvidenceReference {
                kind: EvidenceKind::Witness,
                value: "witness-1".to_string(),
            }],
            equivalence_caveat: None,
            remedies: vec![Remedy {
                rank: 1,
                remedy_key: "shared".to_string(),
                description: "test remedy".to_string(),
                effort: RemedyEffort::Easy,
                prerequisites: vec!["proof".to_string()],
                contraindications: vec!["not-proven".to_string()],
                equivalence_caveat: Some("review required".to_string()),
            }],
        }],
    }
}

#[test]
fn validates_duplicate_shape_and_remedy_pairs() {
    let mut catalog = sample();
    catalog.entries.push(catalog.entries[0].clone());
    assert!(matches!(
        validate_catalog(&catalog),
        Err(CatalogError::DuplicateShapeKey(_))
    ));
    catalog.entries.pop();
    let duplicate = catalog.entries[0].remedies[0].clone();
    catalog.entries[0].remedies.push(duplicate);
    assert!(matches!(
        validate_catalog(&catalog),
        Err(CatalogError::DuplicateRemedyShapePair { .. })
    ));
}

#[test]
fn rejects_unsupported_versions_and_unordered_entries() {
    let mut catalog = sample();
    catalog.schema_version = 2;
    assert!(matches!(
        validate_catalog(&catalog),
        Err(CatalogError::UnsupportedSchemaVersion(2))
    ));
    catalog.schema_version = 1;
    let template = catalog.entries[0].clone();
    catalog.entries.push(AdviceEntry {
        shape_key: "0-shape".to_string(),
        ..template
    });
    assert!(matches!(
        validate_catalog(&catalog),
        Err(CatalogError::NondeterministicOrder { .. })
    ));
}

#[test]
fn parser_requires_typed_evidence_and_renders_warning() {
    let source = r#"
schema_version = 1
[[entry]]
shape_key = "a-shape"
backend_id = "foma"
route = "route"
failed_predicate = "predicate"
evidence_refs = ["witness:w1"]
[[entry.remedy]]
rank = 1
remedy_key = "shared"
description = "test remedy"
effort = "easy"
prerequisites = ["proof"]
contraindications = ["not-proven"]
equivalence_caveat = "review"
"#;
    let catalog = parse_catalog(source).expect("sample catalog parses");
    let rendered = render_remedy_group(&catalog.entries[0]);
    assert!(rendered.contains("would make this backend work for your language"));
    assert!(rendered.contains(GRAMMAR_SAFETY_WARNING));
}
