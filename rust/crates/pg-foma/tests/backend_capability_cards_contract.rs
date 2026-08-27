use pg_foma::advice_catalog::{builtin_catalog, GRAMMAR_SAFETY_WARNING};
use pg_foma::backend_cards::{
    catalog, checked_in_relative_path, render_markdown, EnvelopeControl, CARD_SCHEMA_VERSION,
};
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

#[test]
fn catalog_covers_exactly_the_executable_backends_with_complete_static_envelopes() {
    let cards = catalog();
    let expected = ALL_STRATEGIES
        .iter()
        .map(|strategy| strategy.label())
        .collect::<BTreeSet<_>>();
    let actual = cards
        .iter()
        .map(|card| card.backend_id)
        .collect::<BTreeSet<_>>();

    assert_eq!(CARD_SCHEMA_VERSION, 1);
    assert_eq!(cards.len(), 3);
    assert_eq!(actual, expected);

    let advice = builtin_catalog().expect("embedded advice catalog must remain valid");
    let remedy_ids = advice
        .entries
        .iter()
        .flat_map(|entry| entry.remedies.iter())
        .map(|remedy| remedy.remedy_key.as_str())
        .collect::<BTreeSet<_>>();

    for card in cards {
        assert!(!card.display_name.is_empty());
        assert!(!card.summary.is_empty());
        assert!(!card.envelopes.is_empty());
        for envelope in card.envelopes {
            assert!(!envelope.id.is_empty());
            assert!(!envelope.name.is_empty());
            assert!(!envelope.big_o.time.is_empty());
            assert!(!envelope.big_o.space.is_empty());
            assert!(!envelope.big_o.variables.is_empty());
            assert!(
                envelope.big_o.time.contains('F'),
                "{} must include the feature/unification cost axis F",
                envelope.id
            );
            assert!(
                envelope
                    .big_o
                    .variables
                    .iter()
                    .any(|variable| variable.starts_with("F:")),
                "{} must define F",
                envelope.id
            );
            assert!(!envelope.contributors.is_empty());
            assert!(!envelope.source_refs.is_empty());
            if let EnvelopeControl::SwitchControlled { switch_id, .. } = envelope.control {
                assert!(!switch_id.is_empty());
            }
            for &remedy_id in envelope.remedy_ids {
                assert!(
                    remedy_ids.contains(remedy_id),
                    "{} references unknown remedy {remedy_id}",
                    envelope.id
                );
            }
        }
    }
}

#[test]
fn rendered_cards_are_deterministic_and_static() {
    for card in catalog() {
        let first = render_markdown(card);
        let second = render_markdown(card);
        assert_eq!(first, second);
        assert!(first.contains("Static backend contract"));
        assert!(first.contains(GRAMMAR_SAFETY_WARNING));
        assert!(first.lines().count() < 100, "{} card is too long", card.backend_id);
        for language_name in ["Mbugwe", "Aweti", "Sena", "Indonesian", "Warlpiri"] {
            assert!(!first.contains(language_name));
        }

        let relative = checked_in_relative_path(card.backend_id);
        assert!(relative.starts_with("docs/fst-plan/backend-cards/"));
        assert!(relative.ends_with(".md"));
    }
}

#[test]
fn tuned_surface_card_omits_removed_resource_controls_but_keeps_static_cost_evidence() {
    let card = catalog()
        .iter()
        .find(|card| card.backend_id == "tuned-surface-probed")
        .expect("tuned surface card");
    let rendered = render_markdown(card);

    for removed in [
        "CompileRequest.resource_envelope",
        "managed-v1",
        "tuned-surface-work-10k-v1",
        "retry-larger-closure-envelope",
    ] {
        assert!(
            !rendered.contains(removed),
            "tuned surface card must omit removed control {removed}: {rendered}"
        );
    }
    assert!(rendered.contains("Static backend contract"));
    assert!(rendered.contains("O(E x J x P x F + N)"));
    assert!(rendered.contains("F = feature/unification cost and fan-out"));
}

#[test]
fn managed_build_wrapper_regenerates_the_checked_in_cards_after_success() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("crate remains below rust/crates")
        .join("tools/pg.ps1");
    let source = fs::read_to_string(wrapper).expect("read managed build wrapper");

    assert!(source.contains("regenerate_backend_cards"));
    assert!(source.contains("backend capability cards regenerated"));
}
