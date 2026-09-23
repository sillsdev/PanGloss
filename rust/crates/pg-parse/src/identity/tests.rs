use super::*;
use serde_json::json;

fn identity(
    morphemes: Vec<MorphemeKey>,
    root_index: i32,
    category: Option<&str>,
) -> AnalysisIdentity {
    AnalysisIdentity {
        morphemes,
        root_index,
        category: category.map(str::to_string),
    }
}

#[test]
fn identity_ordering_is_total_and_stable() {
    // Analysis sets are sorted before digesting, so `Ord` must be a total order over every field, including the guessed-slot `None`.
    let mut ids = [
        identity(vec![Some("b".into())], 0, None),
        identity(vec![Some("a".into())], 1, Some("noun")),
        identity(vec![Some("a".into())], 0, Some("verb")),
        identity(vec![None], 0, None),
    ];
    ids.sort();
    assert_eq!(ids[0], identity(vec![None], 0, None));
    assert_eq!(ids[1], identity(vec![Some("a".into())], 0, Some("verb")));
    assert_eq!(ids[2], identity(vec![Some("a".into())], 1, Some("noun")));
    assert_eq!(ids[3], identity(vec![Some("b".into())], 0, None));
}

#[test]
fn guessed_slot_and_absent_category_are_distinguishable() {
    // `None` in `morphemes` means "fabricated root"; `None` in `category` means "no POS" — they must not collapse into one another.
    let guessed = identity(vec![None], 0, Some("noun"));
    let uncategorized = identity(vec![Some("m".into())], 0, None);
    assert_ne!(
        guessed.to_canonical_value(),
        uncategorized.to_canonical_value()
    );
}

#[test]
fn canonical_value_names_every_field() {
    let v = identity(vec![Some("guid-a".into()), None], 1, Some("guid-noun")).to_canonical_value();
    assert_eq!(v["morphemes"][0], json!("guid-a"));
    assert_eq!(v["morphemes"][1], Value::Null);
    assert_eq!(v["rootIndex"], json!(1));
    assert_eq!(v["category"], json!("guid-noun"));
}

#[test]
fn category_absent_serializes_as_null_not_missing() {
    // A missing key and an explicit null canonicalize differently, so the projection must be consistent about which it emits.
    let v = identity(vec![Some("m".into())], 0, None).to_canonical_value();
    assert_eq!(v.get("category"), Some(&Value::Null));
}
