//! Unit tests over the hand-written synthetic fixture `tests/data/fixture.fwdata`, covering the full extraction surface plus dangling-reference and unknown-morph-type warnings.

use std::path::{Path, PathBuf};

use pg_snapshot::{Msa, NaturalClass, PhonologicalRule};

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fixture.fwdata")
}

#[test]
fn imports_without_error() {
    let (_, report) = pg_fwdata::import_file(&fixture_path()).expect("fixture must import");
    // At least the unknown-morph-type allomorph and the dangling environment guid should be reported.
    assert!(
        !report.warnings.is_empty(),
        "expected at least the deliberately-planted warnings"
    );
}

#[test]
fn extracts_project_writing_systems() {
    let (snap, _) = pg_fwdata::import_file(&fixture_path()).unwrap();
    assert_eq!(snap.project.vernacular_writing_systems, vec!["fx"]);
    assert_eq!(snap.project.analysis_writing_systems, vec!["en"]);
    assert_eq!(snap.project.name, "fixture");
}

#[test]
fn extracts_feature_system() {
    let (snap, _) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let fs = &snap.feature_systems.morphosyntactic;
    assert_eq!(fs.closed_features.len(), 1);
    let number = &fs.closed_features[0];
    assert_eq!(number.name, "Number");
    assert_eq!(number.values.len(), 2);
    assert_eq!(number.values[0].abbreviation, "sg");
    assert_eq!(number.values[1].abbreviation, "pl");
}

#[test]
fn extracts_phonemes_and_boundary_markers() {
    let (snap, _) = pg_fwdata::import_file(&fixture_path()).unwrap();
    assert_eq!(snap.phonology.phonemes.len(), 3);
    let names: Vec<_> = snap
        .phonology
        .phonemes
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert!(names.contains(&"a"));
    assert!(names.contains(&"t"));
    assert!(names.contains(&"d"));
    assert_eq!(snap.phonology.boundary_markers.len(), 1);
}

#[test]
fn extracts_one_natural_class() {
    let (snap, _) = pg_fwdata::import_file(&fixture_path()).unwrap();
    assert_eq!(snap.phonology.natural_classes.len(), 1);
    match &snap.phonology.natural_classes[0] {
        NaturalClass::Segments { name, phonemes, .. } => {
            assert_eq!(name, "V");
            assert_eq!(phonemes.len(), 1);
        }
        other => panic!("expected a segments-based natural class, got {other:?}"),
    }
}

#[test]
fn extracts_one_environment() {
    let (snap, _) = pg_fwdata::import_file(&fixture_path()).unwrap();
    assert_eq!(snap.phonology.environments.len(), 1);
    assert_eq!(snap.phonology.environments[0].representation, "/[V]_");
}

#[test]
fn extracts_one_rewrite_rule() {
    let (snap, _) = pg_fwdata::import_file(&fixture_path()).unwrap();
    assert_eq!(snap.phonology.rules.len(), 1);
    match &snap.phonology.rules[0] {
        PhonologicalRule::Rewrite(r) => {
            assert_eq!(r.name, "Voicing");
            assert_eq!(r.structural_description.len(), 1);
            assert_eq!(r.right_hand_sides.len(), 1);
            assert!(r.right_hand_sides[0].left_context.is_some());
        }
        other => panic!("expected a rewrite rule, got {other:?}"),
    }
}

#[test]
fn extracts_affix_template_with_slot() {
    let (snap, _) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let verb = snap
        .morphology
        .parts_of_speech
        .iter()
        .find(|p| p.name == "Verb")
        .expect("Verb POS must be present");
    assert_eq!(verb.affix_slots.len(), 1);
    assert_eq!(verb.affix_templates.len(), 1);
    let template = &verb.affix_templates[0];
    assert_eq!(
        template.suffix_slots,
        vec![verb.affix_slots[0].guid.clone()]
    );
    assert!(template.is_final);
}

#[test]
fn extracts_inflectional_affix_msa_filling_the_slot() {
    let (snap, _) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let suffix_entry = snap
        .lexicon
        .entries
        .iter()
        .find(|e| e.citation_form.iter().any(|f| f.form == "-s"))
        .expect("the -s entry must be present");
    assert_eq!(suffix_entry.msas.len(), 1);
    match &suffix_entry.msas[0] {
        Msa::Inflectional {
            slots, features, ..
        } => {
            assert_eq!(slots.len(), 1);
            assert!(features.is_some());
        }
        other => panic!("expected an inflectional MSA, got {other:?}"),
    }
}

#[test]
fn three_entries_imported() {
    let (snap, _) = pg_fwdata::import_file(&fixture_path()).unwrap();
    assert_eq!(snap.lexicon.entries.len(), 3);
}

#[test]
fn unknown_morph_type_allomorph_is_skipped_with_a_warning() {
    let (snap, report) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let run_entry = snap
        .lexicon
        .entries
        .iter()
        .find(|e| e.citation_form.iter().any(|f| f.form == "ranna"))
        .expect("the run entry must be present");
    // Only the valid lexeme-form allomorph should have survived; the extra alternate form with
    // an unrecognized morph-type guid must have been dropped.
    assert_eq!(run_entry.allomorphs.len(), 1);
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("00000000-0000-0000-0000-00000000abcd")));
}

/// The unrecognized-morph-type-guid warning above carries a specific, stable code -- pinned here
/// alongside the existing prose assertion, exactly, so a future reword of the message is never
/// itself a code change.
#[test]
fn unknown_morph_type_warning_carries_its_stable_code() {
    let (_, report) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let hit = report
        .warnings
        .iter()
        .find(|w| w.contains("00000000-0000-0000-0000-00000000abcd"))
        .expect("the unrecognized-morph-type warning must be present");
    assert_eq!(hit.code, "fwdata.unknown-morph-type-guid");
}

#[test]
fn dangling_environment_reference_does_not_crash_import() {
    let (snap, _) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let suffix_entry = snap
        .lexicon
        .entries
        .iter()
        .find(|e| e.citation_form.iter().any(|f| f.form == "-s"))
        .unwrap();
    // The dangling guid is still carried through (pg-fwdata doesn't dereference environment
    // guids on allomorphs) — `Snapshot::validate()` is where it's flagged.
    assert_eq!(suffix_entry.allomorphs.len(), 1);
    assert_eq!(suffix_entry.allomorphs[0].environments.len(), 2);
    let warnings = snap.validate();
    assert!(warnings
        .iter()
        .any(|w| w.contains("00000000-0000-0000-0000-0000000000ff")));
}

/// Two structurally different situations -- pg-fwdata's "unrecognized morph-type guid"
/// (import-time) and pg-snapshot's "dangling environment reference" (validate-time) -- must get
/// different codes.
#[test]
fn structurally_different_warnings_get_different_codes() {
    let (snap, report) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let morph_type_warning = report
        .warnings
        .iter()
        .find(|w| w.contains("00000000-0000-0000-0000-00000000abcd"))
        .expect("the unrecognized-morph-type warning must be present");
    let validate_warnings = snap.validate();
    let dangling_env_warning = validate_warnings
        .iter()
        .find(|w| w.contains("00000000-0000-0000-0000-0000000000ff"))
        .expect("the dangling-environment warning must be present");
    assert_ne!(morph_type_warning.code, dangling_env_warning.code);
    assert_eq!(morph_type_warning.code, "fwdata.unknown-morph-type-guid");
    assert_eq!(dangling_env_warning.code, "snapshot.dangling-reference");
}

/// The exact prose
/// this warning has always carried is pinned exactly here (not just a substring, as the tests
/// above check) at this representative site. Guids per `tests/data/fixture.fwdata`: the
/// `MoStemAllomorph` with the planted unrecognized morph type is
/// `00000000-0000-0000-0000-000000000044`, referencing `MorphType`
/// `00000000-0000-0000-0000-00000000abcd`.
#[test]
fn import_warning_prose_is_unchanged() {
    let (_, report) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let hit = report
        .warnings
        .iter()
        .find(|w| w.contains("00000000-0000-0000-0000-00000000abcd"))
        .expect("the unrecognized-morph-type warning must be present");
    assert_eq!(
        hit.message,
        "lexicon.entries.allomorphs: 00000000-0000-0000-0000-000000000044 has unrecognized \
         morph-type guid 00000000-0000-0000-0000-00000000abcd; skipping"
    );
}

#[test]
fn import_is_deterministic() {
    let (snap1, report1) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let (snap2, report2) = pg_fwdata::import_file(&fixture_path()).unwrap();
    assert_eq!(snap1.to_json(), snap2.to_json());
    assert_eq!(report1.warnings, report2.warnings);
}
