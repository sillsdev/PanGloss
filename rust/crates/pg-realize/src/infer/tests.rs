use super::*;

/// Table-driven: one row per gloss -> expected assignment (or `None` for no entry at all).
#[test]
fn alias_table_matches_the_design_docs_examples() {
    let cases: &[(&str, Option<FeatureAssignment>)] = &[
        ("pl", Some(FeatureAssignment::Num(Num::Pl))),
        ("PL", Some(FeatureAssignment::Num(Num::Pl))),
        ("plural", Some(FeatureAssignment::Num(Num::Pl))),
        ("sg", Some(FeatureAssignment::Num(Num::Sg))),
        ("1sg.poss", Some(FeatureAssignment::Poss(Poss::P1Sg))),
        ("POSS.3PL", Some(FeatureAssignment::Poss(Poss::P3Pl))),
        ("poss-2pl", Some(FeatureAssignment::Poss(Poss::P2Pl))),
        ("loc", Some(FeatureAssignment::Case(CaseRole::Loc))),
        ("ablative", Some(FeatureAssignment::Case(CaseRole::Abl))),
        // Negatives: never guess -- no entry at all.
        ("appl", None),
        ("caus", None),
        ("1sg", None), // no poss/gen token alongside it
        ("3", None),
    ];

    for (gloss, expected) in cases {
        assert_eq!(infer_one(gloss), *expected, "gloss {gloss:?}");
    }
}

#[test]
fn infer_english_only_produces_entries_for_glosses_present_in_the_input() {
    let glosses = ["pl", "appl", "loc", "1sg", "caus"];
    let map = infer_english(glosses.into_iter());

    assert_eq!(map.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
    assert_eq!(
        map.lookup("loc"),
        Some(FeatureAssignment::Case(CaseRole::Loc))
    );
    assert_eq!(map.lookup("appl"), None);
    assert_eq!(map.lookup("1sg"), None);
    assert_eq!(map.lookup("caus"), None);
    // Never asked about at all -- also absent, same as any other unmapped gloss.
    assert_eq!(map.lookup("nonexistent"), None);
}

#[test]
fn gender_limitation_defaults_2sg_and_3sg_possessives_to_masculine() {
    let map = infer_english(["2sg.poss", "3sg.poss"].into_iter());
    assert_eq!(
        map.lookup("2sg.poss"),
        Some(FeatureAssignment::Poss(Poss::P2SgM))
    );
    assert_eq!(
        map.lookup("3sg.poss"),
        Some(FeatureAssignment::Poss(Poss::P3SgM))
    );
}

#[test]
fn merge_precedence_sidecar_overrides_inferred_base_per_gloss_key() {
    // Base: purely inferred from grammar glosses, no sidecar.
    let mut base = infer_english(["pl", "loc", "appl"].into_iter());
    assert_eq!(base.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
    assert_eq!(base.lookup("appl"), None, "inference never guesses appl");

    // Sidecar: overrides "loc" and adds "appl" — a curated mapping, not a guess, so it may fill in what inference alone leaves absent.
    let sidecar = RealizeMap::parse("[features]\n\"loc\" = \"Case:Abl\"\n\"appl\" = \"Ignore\"\n")
        .expect("valid sidecar");

    base.extend_overriding(sidecar);

    // Untouched key survives from the inferred base.
    assert_eq!(base.lookup("pl"), Some(FeatureAssignment::Num(Num::Pl)));
    // Sidecar wins on the shared key.
    assert_eq!(
        base.lookup("loc"),
        Some(FeatureAssignment::Case(CaseRole::Abl))
    );
    // Sidecar adds a mapping inference alone left absent.
    assert_eq!(base.lookup("appl"), Some(FeatureAssignment::Ignore));
}
