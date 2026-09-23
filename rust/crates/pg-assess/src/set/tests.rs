use super::*;

fn id(morphemes: &[&str]) -> AnalysisIdentity {
    AnalysisIdentity {
        morphemes: morphemes.iter().map(|m| Some(m.to_string())).collect(),
        root_index: 0,
        category: None,
    }
}

#[test]
fn discovery_order_does_not_change_the_set() {
    let a = AnalysisSet::from_observed([id(&["x"]), id(&["y"]), id(&["z"])]);
    let b = AnalysisSet::from_observed([id(&["z"]), id(&["x"]), id(&["y"])]);
    assert_eq!(a, b);
    assert_eq!(a.to_outcome_value(), b.to_outcome_value());
}

#[test]
fn duplicates_collapse_but_are_counted() {
    let set = AnalysisSet::from_observed([id(&["x"]), id(&["x"]), id(&["y"])]);
    assert_eq!(set.len(), 2);
    let x = set
        .entries()
        .iter()
        .find(|e| e.identity == id(&["x"]))
        .unwrap();
    assert_eq!(x.duplicate_count, 2);
}

#[test]
fn duplicate_counts_are_invisible_to_the_outcome_projection() {
    // The load-bearing property behind `outcomeDigest`: redundant proposal work is not a behavior change.
    let once = AnalysisSet::from_observed([id(&["x"])]);
    let thrice = AnalysisSet::from_observed([id(&["x"]), id(&["x"]), id(&["x"])]);
    assert_eq!(once.to_outcome_value(), thrice.to_outcome_value());
    assert_ne!(once.to_semantic_value(), thrice.to_semantic_value());
}

#[test]
fn a_complete_empty_set_is_representable() {
    // Distinct from an incomplete outcome, which carries no authoritative set at all.
    let empty = AnalysisSet::from_observed([]);
    assert!(empty.is_empty());
    assert_eq!(empty.to_outcome_value(), json!([]));
}

#[test]
fn a_guessed_flip_is_visible_to_the_outcome_projection() {
    // A root that fell out of the lexicon and was fabricated instead is a behavior change, even though the morpheme sequence and category are untouched.
    let found = AnalysisSet::from_annotated([(id(&["x"]), false)]);
    let fabricated = AnalysisSet::from_annotated([(id(&["x"]), true)]);
    assert_eq!(
        found.entries()[0].identity,
        fabricated.entries()[0].identity,
        "identity is unchanged — that is the point"
    );
    assert_ne!(found.to_outcome_value(), fabricated.to_outcome_value());
}

#[test]
fn contains_confirms_the_structured_value() {
    let set = AnalysisSet::from_observed([id(&["x"])]);
    assert!(set.contains(&id(&["x"])));
    assert!(!set.contains(&id(&["y"])));
}
