use super::*;
use pg_conformance_fixtures::{discover, FixtureRef};

fn find_fixture<'a>(fixtures: &'a [FixtureRef], category: &str, name: &str) -> &'a FixtureRef {
    fixtures
        .iter()
        .find(|f| f.category == category && f.name == name)
        .unwrap_or_else(|| panic!("fixture {category}/{name} must be discoverable"))
}

/// Two structurally different fixtures already used by `pg-parse`'s own conformance gate.
fn two_sample_grammars() -> [crate::model::Grammar; 2] {
    let fixtures = discover();
    assert!(!fixtures.is_empty(), "no conformance fixtures discovered");
    let a = find_fixture(&fixtures, "languages", "metathesis-phase-isolation");
    let b = find_fixture(&fixtures, "edge-cases", "truncate-morphotactic");
    [
        crate::load(&a.load_grammar_xml()).expect("fixture a must load"),
        crate::load(&b.load_grammar_xml()).expect("fixture b must load"),
    ]
}

/// A key collision silently merges distinct stats rows, so injectivity is checked over every discoverable fixture rather than the two sampled above.
#[test]
fn stats_keys_are_injective_across_every_fixture() {
    let fixtures = discover();
    assert!(!fixtures.is_empty(), "no conformance fixtures discovered");
    for f in &fixtures {
        let label = format!("{}/{}", f.category, f.name);
        let grammar = crate::load(&f.load_grammar_xml())
            .unwrap_or_else(|error| panic!("{label}: fixture must load: {error}"));
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for i in 0..grammar.mrules.len() {
            let key = morph_rule_identity(&grammar, MRuleId(i as u32)).key;
            if let Some(prev) = seen.insert(key.clone(), i) {
                panic!("{label}: mrules {prev} and {i} share the identity key {key:?}");
            }
        }
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for i in 0..grammar.prules.len() {
            let key = phon_rule_identity(&grammar, PRuleId(i as u32)).key;
            if let Some(prev) = seen.insert(key.clone(), i) {
                panic!("{label}: prules {prev} and {i} share the identity key {key:?}");
            }
        }
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (i, owner) in grammar.allomorph_owners.iter().enumerate() {
            let key = allomorph_identity(&grammar, AllomorphId(i as u32)).key;
            let owner_kind = match owner {
                AllomorphOwner::Root(..) => "lex_entry",
                AllomorphOwner::Affix(..) => "morph_rule",
            };
            assert!(
                key.starts_with(&format!("{owner_kind}:")),
                "{label}: allomorph {i} key {key:?} must include its owner kind"
            );
            if let Some(prev) = seen.insert(key.clone(), i) {
                panic!("{label}: allomorphs {prev} and {i} share the identity key {key:?}");
            }
        }
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for i in 0..grammar.entries.len() {
            let key = lex_entry_identity(&grammar, LexEntryId(i as u32)).key;
            if let Some(prev) = seen.insert(key.clone(), i) {
                panic!("{label}: entries {prev} and {i} share the identity key {key:?}");
            }
        }
    }
}

/// Resolving the guessed-root sentinel must not index the allomorph registry out of bounds.
#[test]
fn the_guessed_allomorph_sentinel_resolves_instead_of_panicking() {
    let [grammar, _] = two_sample_grammars();
    let identity = allomorph_identity(&grammar, crate::model::AllomorphId::GUESSED);
    assert!(!identity.key.is_empty());
    assert_eq!(identity.quality, IdentityQuality::Synthetic);
}

/// Every individual resolver, called directly over `grammar`'s own runtime ids.
fn assert_all_non_empty_and_expected_quality(grammar: &crate::model::Grammar, label: &str) {
    for i in 0..grammar.mrules.len() {
        let oi = morph_rule_identity(grammar, MRuleId(i as u32));
        assert!(!oi.key.is_empty(), "{label}: empty key for {oi:?}");
        assert!(!oi.label.is_empty(), "{label}: empty label for {oi:?}");
        assert!(
            matches!(
                oi.quality,
                IdentityQuality::Authored | IdentityQuality::Structural
            ),
            "{label}: unexpected quality for morph rule {oi:?}"
        );
    }
    for i in 0..grammar.prules.len() {
        let oi = phon_rule_identity(grammar, PRuleId(i as u32));
        assert!(!oi.key.is_empty(), "{label}: empty key for {oi:?}");
        assert!(!oi.label.is_empty(), "{label}: empty label for {oi:?}");
        assert!(
            matches!(
                oi.quality,
                IdentityQuality::Authored | IdentityQuality::Structural
            ),
            "{label}: unexpected quality for phon rule {oi:?}"
        );
    }
    for i in 0..grammar.entries.len() {
        let oi = lex_entry_identity(grammar, LexEntryId(i as u32));
        assert!(!oi.key.is_empty(), "{label}: empty key for {oi:?}");
        assert!(!oi.label.is_empty(), "{label}: empty label for {oi:?}");
        assert_eq!(oi.quality, IdentityQuality::Authored);
    }
    for i in 0..grammar.strata.len() {
        let si = stratum_identity(grammar, StratumId(i as u8));
        assert!(!si.key.is_empty(), "{label}: empty stratum key");
        assert!(!si.label.is_empty(), "{label}: empty stratum label");
        assert_eq!(si.quality, IdentityQuality::Structural);

        let oi = root_index_identity(grammar, StratumId(i as u8));
        assert!(!oi.key.is_empty());
        assert_eq!(oi.quality, IdentityQuality::Synthetic);
    }
    for i in 0..grammar.allomorph_owners.len() {
        let ai = allomorph_identity(grammar, AllomorphId(i as u32));
        assert!(!ai.key.is_empty(), "{label}: empty allomorph key");
        assert!(!ai.label.is_empty(), "{label}: empty allomorph label");
        assert_eq!(ai.quality, IdentityQuality::Structural);
    }
    for i in 0..grammar.morphemes.len() {
        let mi = morpheme_identity(grammar, MorphemeId(i as u32));
        assert!(!mi.key.is_empty(), "{label}: empty morpheme key");
        assert!(!mi.label.is_empty(), "{label}: empty morpheme label");
    }

    assert_eq!(
        guesser_identity(grammar).quality,
        IdentityQuality::Synthetic
    );
    for phase in OverlayPhase::ALL {
        let identity = overlay_identity(grammar, phase);
        assert_eq!(identity.quality, IdentityQuality::Synthetic);
        assert_eq!(OverlayPhase::from_index(phase.index()), phase);
        assert_eq!(identity.key, format!("overlay:{}", phase.name()));
    }

    // Guards against a fixture change silently emptying what this test actually exercises.
    assert!(!grammar.entries.is_empty(), "{label}: no lex entries");
    assert!(!grammar.mrules.is_empty(), "{label}: no morph rules");
    assert!(
        !grammar.allomorph_owners.is_empty(),
        "{label}: no allomorphs"
    );
}

#[test]
fn every_object_in_two_fixtures_resolves_non_empty_with_expected_quality() {
    let [a, b] = two_sample_grammars();
    assert_all_non_empty_and_expected_quality(&a, "metathesis-phase-isolation");
    assert_all_non_empty_and_expected_quality(&b, "truncate-morphotactic");
}

#[test]
fn identities_are_stable_across_two_loads_of_the_same_grammar() {
    let fixtures = discover();
    let f = find_fixture(&fixtures, "languages", "metathesis-phase-isolation");
    let xml = f.load_grammar_xml();
    let g1 = crate::load(&xml).unwrap();
    let g2 = crate::load(&xml).unwrap();

    assert_eq!(
        morph_rule_identity(&g1, MRuleId(0)),
        morph_rule_identity(&g2, MRuleId(0))
    );
    assert_eq!(
        lex_entry_identity(&g1, LexEntryId(0)),
        lex_entry_identity(&g2, LexEntryId(0))
    );
    assert_eq!(
        stratum_identity(&g1, StratumId(0)),
        stratum_identity(&g2, StratumId(0))
    );
    assert_eq!(
        allomorph_identity(&g1, AllomorphId(0)),
        allomorph_identity(&g2, AllomorphId(0))
    );
    assert_eq!(
        morpheme_identity(&g1, MorphemeId(0)),
        morpheme_identity(&g2, MorphemeId(0))
    );
}

#[test]
fn root_index_identity_preserves_the_ordinal_persisted_key() {
    let [grammar, _] = two_sample_grammars();
    assert_eq!(
        root_index_identity(&grammar, StratumId(0)).key,
        "root_index#0"
    );
}

#[test]
fn morpheme_identity_resolves_the_guessed_sentinel_without_panicking() {
    let [grammar, _] = two_sample_grammars();
    let identity = morpheme_identity(&grammar, MorphemeId::GUESSED);
    assert!(!identity.key.is_empty());
    assert_eq!(identity.quality, IdentityQuality::Synthetic);
}
