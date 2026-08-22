use std::collections::{BTreeMap, BTreeSet};

use pg_foma::advice_catalog::{
    builtin_catalog, render_remedy_group, RemedyEffort, ADVICE_CATALOG_SCHEMA_VERSION,
    GRAMMAR_SAFETY_WARNING, PLAN_COMPOSED_MISSING_SUBTREES_SHAPE_KEY,
    TUNED_SURFACE_RESOURCE_SHAPE_KEY,
};

const REQUIRED_SHAPES: &[&str] = &[
    "backend-build-unavailable",
    "late-structural-reachability",
    "nonregular-process-morphology",
    "null-cycle",
    "optional-slot-branching",
    "plan-composed-missing-subtrees",
    "repeated-application",
    "structural-deletion-or-truncation",
    "tuned-surface-resource-envelope",
    "unordered-interactions",
    "wide-phonology",
];

#[test]
fn builtin_catalog_is_versioned_complete_and_deterministic() {
    assert_eq!(ADVICE_CATALOG_SCHEMA_VERSION, 1);
    let first = builtin_catalog().expect("built-in advice catalog must parse and validate");
    let second = builtin_catalog().expect("built-in advice catalog must be deterministic");
    assert_eq!(first, second);
    assert_eq!(first.schema_version, ADVICE_CATALOG_SCHEMA_VERSION);

    let shape_keys: Vec<_> = first
        .entries
        .iter()
        .map(|entry| entry.shape_key.as_str())
        .collect();
    assert_eq!(
        shape_keys, REQUIRED_SHAPES,
        "entries use stable sorted keys"
    );

    for entry in &first.entries {
        assert!(!entry.backend_id.trim().is_empty());
        assert!(!entry.failed_predicate.trim().is_empty());
        assert!(!entry.evidence_refs.is_empty());
        assert!(!entry.remedies.is_empty());
        for remedy in &entry.remedies {
            assert!(!remedy.remedy_key.trim().is_empty());
            assert!(matches!(
                remedy.effort,
                RemedyEffort::Easy | RemedyEffort::Medium | RemedyEffort::Hard
            ));
        }
    }
}

#[test]
fn plan_composed_missing_subtrees_has_backend_specific_advice() {
    let catalog = builtin_catalog().expect("built-in advice catalog must validate");
    let entry = catalog
        .entry_for(PLAN_COMPOSED_MISSING_SUBTREES_SHAPE_KEY)
        .expect("incomplete PlanComposed coverage must have structured advice");

    assert_eq!(entry.backend_id, "plan-composed");
    assert_eq!(entry.route, "plan-composed-materialization");
    assert!(entry
        .evidence_refs
        .iter()
        .any(|reference| reference.value == "required-subtree-marker"));
    assert!(entry
        .remedies
        .iter()
        .any(|remedy| remedy.remedy_key == "use-whole-grammar-backend"));
    assert_eq!(
        entry.equivalence_caveat.as_deref(),
        Some(GRAMMAR_SAFETY_WARNING)
    );
}

#[test]
fn tuned_surface_resource_entry_has_typed_budget_evidence_and_safety_caveat() {
    let catalog = builtin_catalog().expect("built-in advice catalog must validate");
    let entry = catalog
        .entry_for(TUNED_SURFACE_RESOURCE_SHAPE_KEY)
        .expect("TunedSurface resource findings must have structured advice");

    assert_eq!(entry.backend_id, "foma");
    assert!(entry
        .evidence_refs
        .iter()
        .any(|reference| reference.value == "composite-rule-pair-count"));
    assert!(entry
        .remedies
        .iter()
        .any(|remedy| remedy.remedy_key == "retry-larger-closure-envelope"));
    assert_eq!(
        entry.equivalence_caveat.as_deref(),
        Some(GRAMMAR_SAFETY_WARNING)
    );
}

#[test]
fn rendered_groups_use_approved_english_and_safety_warning() {
    let catalog = builtin_catalog().expect("built-in advice catalog must validate");
    for entry in &catalog.entries {
        let rendered = render_remedy_group(entry);
        assert!(
            rendered.contains("would make this backend work for your language"),
            "missing conditional backend wording for {}",
            entry.shape_key
        );
        assert!(
            rendered.contains(GRAMMAR_SAFETY_WARNING),
            "missing grammar-safety warning for {}",
            entry.shape_key
        );
    }
    assert_eq!(
        GRAMMAR_SAFETY_WARNING,
        "Don't make any change that would make your language invalid!"
    );
}

#[test]
fn effort_belongs_to_each_remedy_shape_pair_and_shared_remedies_deduplicate() {
    let catalog = builtin_catalog().expect("built-in advice catalog must validate");
    let mut uses: BTreeMap<&str, Vec<(&str, RemedyEffort)>> = BTreeMap::new();
    for entry in &catalog.entries {
        for remedy in &entry.remedies {
            uses.entry(remedy.remedy_key.as_str())
                .or_default()
                .push((entry.shape_key.as_str(), remedy.effort));
        }
    }

    let shared: Vec<_> = uses.values().filter(|pairs| pairs.len() > 1).collect();
    assert!(
        !shared.is_empty(),
        "at least one remedy must be shared across independently diagnosed shapes"
    );

    let unique_pairs: BTreeSet<_> = uses
        .iter()
        .flat_map(|(remedy, pairs)| pairs.iter().map(move |(shape, _)| (*shape, *remedy)))
        .collect();
    let pair_count: usize = uses.values().map(Vec::len).sum();
    assert_eq!(unique_pairs.len(), pair_count);
}
