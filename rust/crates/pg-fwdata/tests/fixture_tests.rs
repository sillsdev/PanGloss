//! Unit tests over the hand-written synthetic fixture `tests/data/fixture.fwdata`, covering the full extraction surface plus dangling-reference and unknown-morph-type warnings.

use std::path::{Path, PathBuf};

use pg_snapshot::{Msa, NaturalClass, PhonologicalRule};

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fixture.fwdata")
}

fn fixture_variant(dir: &Path, old: &str, new: &str) -> PathBuf {
    let source = std::fs::read_to_string(fixture_path()).unwrap();
    assert!(source.contains(old), "fixture variant replacement must match");
    let variant = source.replacen(old, new, 1);
    let path = dir.join("variant.fwdata");
    std::fs::write(&path, variant).unwrap();
    path
}

fn parser_parameters_variant(dir: &Path, replacement: &str) -> PathBuf {
    let source = std::fs::read_to_string(fixture_path()).unwrap();
    let opening = "<ParserParameters>";
    let closing = "</ParserParameters>";
    let start = source
        .find(opening)
        .expect("fixture ParserParameters opening needle must be present");
    let end = start
        + source[start..]
            .find(closing)
            .expect("fixture ParserParameters closing needle must be present")
        + closing.len();
    let mut variant = source;
    variant.replace_range(start..end, replacement);
    let path = dir.join("variant.fwdata");
    std::fs::write(&path, variant).unwrap();
    path
}

fn omitted_parser_parameters_variant(dir: &Path) -> PathBuf {
    let source = std::fs::read_to_string(fixture_path()).unwrap();
    let opening = "<ParserParameters>";
    let closing = "</ParserParameters>";
    let start = source
        .find(opening)
        .expect("fixture ParserParameters opening needle must be present");
    let end = start
        + source[start..]
            .find(closing)
            .expect("fixture ParserParameters closing needle must be present")
            + closing.len();
    let mut variant = source;
    variant.replace_range(start..end, "");
    let path = dir.join("variant.fwdata");
    std::fs::write(&path, variant).unwrap();
    path
}

fn assert_invalid_active_parser_source(path: &Path) {
    match pg_fwdata::import_file(path).unwrap_err() {
        pg_fwdata::ImportError::InvalidSource { code, .. } => {
            assert_eq!(code, "invalid-source.active-parser")
        }
        other => panic!("expected invalid parser source error, got {other:?}"),
    }
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
    // Only the valid lexeme-form allomorph should have survived; the extra alternate form with an unrecognized morph-type guid must have been dropped.
    assert_eq!(run_entry.allomorphs.len(), 1);
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("00000000-0000-0000-0000-00000000abcd")));
}

/// The unrecognized-morph-type-guid warning carries a specific, stable code, so a future reword of the message is never itself a code change.
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
    // The dangling guid is still carried through (pg-fwdata doesn't dereference environment guids on allomorphs); `Snapshot::validate()` is where it's flagged.
    assert_eq!(suffix_entry.allomorphs.len(), 1);
    assert_eq!(suffix_entry.allomorphs[0].environments.len(), 2);
    let warnings = snap.validate();
    assert!(warnings
        .iter()
        .any(|w| w.contains("00000000-0000-0000-0000-0000000000ff")));
}

/// Two structurally different situations -- import-time "unrecognized morph-type guid" and validate-time "dangling environment reference" -- must get different codes.
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

/// This warning's exact prose is pinned here (not just a substring, as the tests above check).
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
fn fixture_reports_active_parser_and_xample_caps() {
    let (snapshot, _report) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let pp = &snapshot.morphology.parser_parameters;
    assert_eq!(pp.active_parser, pg_snapshot::ActiveParser::Hc);
    assert_eq!(pp.xample.max_prefixes, Some(2));
    assert_eq!(pp.xample.max_analyses_to_return, Some(10));
}

#[test]
fn malformed_xample_cap_is_a_nonfatal_import_warning() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_variant(
        dir.path(),
        "&lt;MaxPrefixes&gt;2&lt;/MaxPrefixes&gt;",
        "&lt;MaxPrefixes&gt;many&lt;/MaxPrefixes&gt;",
    );
    let (snapshot, report) = pg_fwdata::import_file(&path).unwrap();
    assert_eq!(snapshot.morphology.parser_parameters.xample.max_prefixes, None);
    let warning = report
        .warnings
        .iter()
        .find(|warning| warning.code == "fwdata.invalid-parser-parameter")
        .expect("invalid cap must be reported");
    assert!(warning.message.contains("MaxPrefixes"));
    assert_eq!(
        report
            .warnings
            .iter()
            .filter(|warning| warning.code == "fwdata.invalid-parser-parameter")
            .count(),
        1
    );
}

#[test]
fn omitted_parser_parameters_field_defaults_to_xample() {
    let dir = tempfile::tempdir().unwrap();
    let path = omitted_parser_parameters_variant(dir.path());
    let (snapshot, _) = pg_fwdata::import_file(&path).unwrap();
    assert_eq!(
        snapshot.morphology.parser_parameters.active_parser,
        pg_snapshot::ActiveParser::XAmple
    );
}

#[test]
fn present_parser_parameters_without_uni_is_a_fatal_import_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = parser_parameters_variant(dir.path(), "<ParserParameters/>");
    assert_invalid_active_parser_source(&path);
}

#[test]
fn present_empty_uni_is_a_fatal_import_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = parser_parameters_variant(dir.path(), "<ParserParameters><Uni/></ParserParameters>");
    assert_invalid_active_parser_source(&path);
}

#[test]
fn present_whitespace_uni_is_a_fatal_import_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = parser_parameters_variant(
        dir.path(),
        "<ParserParameters><Uni> \n\t</Uni></ParserParameters>",
    );
    assert_invalid_active_parser_source(&path);
}

#[test]
fn invalid_active_parser_is_a_fatal_import_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_variant(
        dir.path(),
        "&lt;ActiveParser&gt;HC&lt;/ActiveParser&gt;",
        "&lt;ActiveParser&gt;Toneparser&lt;/ActiveParser&gt;",
    );
    let error = pg_fwdata::import_file(&path).unwrap_err();
    match error {
        pg_fwdata::ImportError::InvalidSource { code, .. } => {
            assert_eq!(code, "invalid-source.active-parser")
        }
        other => panic!("expected invalid active parser error, got {other:?}"),
    }
}

#[test]
fn malformed_parser_parameters_are_a_fatal_import_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_variant(
        dir.path(),
        "&lt;ActiveParser&gt;HC&lt;/ActiveParser&gt;",
        "&lt;ActiveParser&gt;XAmple",
    );
    let error = pg_fwdata::import_file(&path).unwrap_err();
    match error {
        pg_fwdata::ImportError::InvalidSource { code, .. } => {
            assert_eq!(code, "invalid-source.active-parser")
        }
        other => panic!("expected malformed parser parameters error, got {other:?}"),
    }
}

#[test]
fn import_is_deterministic() {
    let (snap1, report1) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let (snap2, report2) = pg_fwdata::import_file(&fixture_path()).unwrap();
    assert_eq!(snap1.to_json(), snap2.to_json());
    assert_eq!(report1.warnings, report2.warnings);
}

#[test]
fn fixture_conversion_provenance_is_a_clean_complete_import() {
    let (snapshot, _report) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let provenance = &snapshot.conversion_provenance;
    assert_eq!(
        provenance.source_inventory_status,
        pg_snapshot::SourceInventoryStatus::ImportedComplete
    );
    assert_ne!(
        provenance.source_inventory_status,
        pg_snapshot::SourceInventoryStatus::Synthetic
    );
    assert!(provenance.import_issues.is_empty());

    let census = &provenance.source_census;
    assert!(census.total_occurrences > 0);
    let class_sum: u64 = census.class_occurrences.values().sum();
    assert_eq!(class_sum, census.total_occurrences);
    assert_eq!(census.unhandled_class_occurrences.len(), 0);
    assert_eq!(census.ordered_header_sha256.len(), 64);
    assert!(census
        .ordered_header_sha256
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

    let (snapshot2, _report2) = pg_fwdata::import_file(&fixture_path()).unwrap();
    assert_eq!(
        census.ordered_header_sha256,
        snapshot2.conversion_provenance.source_census.ordered_header_sha256
    );
}

#[test]
fn duplicating_an_allowed_class_guid_yields_one_fatal_issue_and_the_first_content_wins() {
    let dir = tempfile::tempdir().unwrap();
    let source = std::fs::read_to_string(fixture_path()).unwrap();
    let needle = r#"<rt class="LexEntry" guid="00000000-0000-0000-0000-000000000050">"#;
    assert!(source.contains(needle), "fixture must contain the -s LexEntry header");
    // Duplicate the "-s" LexEntry header with a visibly different citation form in the copy, appended at the end.
    let duplicate_block = r#"<rt class="LexEntry" guid="00000000-0000-0000-0000-000000000050">
<CitationForm>
<AUni ws="fx">zzz-duplicate</AUni>
</CitationForm>
</rt>
"#;
    let variant = source.replacen(
        "</languageproject>",
        &format!("{duplicate_block}</languageproject>"),
        1,
    );
    let path = dir.path().join("variant.fwdata");
    std::fs::write(&path, variant).unwrap();

    let (snapshot, _report) = pg_fwdata::import_file(&path).unwrap();
    let provenance = &snapshot.conversion_provenance;
    assert_eq!(
        provenance.source_inventory_status,
        pg_snapshot::SourceInventoryStatus::ImportedWithFatalIssues
    );
    let duplicate_issues: Vec<_> = provenance
        .import_issues
        .iter()
        .filter(|issue| issue.code == "invalid-source.duplicate-guid")
        .collect();
    assert_eq!(duplicate_issues.len(), 1);
    assert!(duplicate_issues[0].fatal);

    let suffix_entries: Vec<_> = snapshot
        .lexicon
        .entries
        .iter()
        .filter(|e| e.guid == "00000000-0000-0000-0000-000000000050")
        .collect();
    assert_eq!(suffix_entries.len(), 1, "the duplicated entry must appear once");
    assert!(
        suffix_entries[0]
            .citation_form
            .iter()
            .any(|f| f.form == "-s"),
        "the FIRST occurrence's content must win"
    );
}

#[test]
fn unknown_class_record_is_census_only_and_raises_no_issue() {
    let dir = tempfile::tempdir().unwrap();
    let source = std::fs::read_to_string(fixture_path()).unwrap();
    let injected = r#"<rt class="ZzUnknown" guid="00000000-0000-0000-0000-0000000000zz">
</rt>
"#;
    let variant = source.replacen(
        "</languageproject>",
        &format!("{injected}</languageproject>"),
        1,
    );
    let path = dir.path().join("variant.fwdata");
    std::fs::write(&path, variant).unwrap();

    let (snapshot, _report) = pg_fwdata::import_file(&path).unwrap();
    let provenance = &snapshot.conversion_provenance;
    assert!(!provenance
        .import_issues
        .iter()
        .any(|issue| issue.source.as_ref().is_some_and(|s| s.kind == "ZzUnknown")));
    assert_eq!(
        provenance.source_census.unhandled_class_occurrences.get("ZzUnknown"),
        Some(&1)
    );
}

#[test]
fn missing_guid_on_an_allowed_class_is_a_fatal_issue_and_drops_the_record() {
    let dir = tempfile::tempdir().unwrap();
    let source = std::fs::read_to_string(fixture_path()).unwrap();
    let needle = r#"<rt class="LexEntry" guid="00000000-0000-0000-0000-000000000050">"#;
    assert!(source.contains(needle), "fixture must contain the -s LexEntry header");
    let variant = source.replacen(needle, r#"<rt class="LexEntry">"#, 1);
    let path = dir.path().join("variant.fwdata");
    std::fs::write(&path, variant).unwrap();

    let (snapshot, _report) = pg_fwdata::import_file(&path).unwrap();
    let provenance = &snapshot.conversion_provenance;
    assert_eq!(
        provenance.source_inventory_status,
        pg_snapshot::SourceInventoryStatus::ImportedWithFatalIssues
    );
    let missing_issues: Vec<_> = provenance
        .import_issues
        .iter()
        .filter(|issue| issue.code == "invalid-source.missing-guid")
        .collect();
    assert_eq!(missing_issues.len(), 1);
    assert!(missing_issues[0].fatal);
    assert!(!snapshot
        .lexicon
        .entries
        .iter()
        .any(|e| e.guid == "00000000-0000-0000-0000-000000000050"));
}

#[test]
fn unknown_class_duplicate_before_an_allowed_class_keeps_the_recognized_record() {
    let dir = tempfile::tempdir().unwrap();
    let source = std::fs::read_to_string(fixture_path()).unwrap();
    let needle = r#"<rt class="LexEntry" guid="00000000-0000-0000-0000-000000000050">"#;
    assert!(source.contains(needle), "fixture must contain the -s LexEntry header");
    // Inject an unknown-class record sharing the "-s" LexEntry's guid, placed BEFORE it in document order.
    let injected = r#"<rt class="ZzUnknown" guid="00000000-0000-0000-0000-000000000050">
</rt>
"#;
    let variant = source.replacen(needle, &format!("{injected}{needle}"), 1);
    let path = dir.path().join("variant.fwdata");
    std::fs::write(&path, variant).unwrap();

    let (snapshot, _report) = pg_fwdata::import_file(&path).unwrap();
    let provenance = &snapshot.conversion_provenance;
    let duplicate_issues: Vec<_> = provenance
        .import_issues
        .iter()
        .filter(|issue| issue.code == "invalid-source.duplicate-guid")
        .collect();
    assert_eq!(duplicate_issues.len(), 1);
    let source = duplicate_issues[0].source.as_ref().unwrap();
    assert_eq!(source.kind, "ZzUnknown");
    assert_eq!(source.id, "00000000-0000-0000-0000-000000000050");
    let suffix_entries: Vec<_> = snapshot
        .lexicon
        .entries
        .iter()
        .filter(|e| e.guid == "00000000-0000-0000-0000-000000000050")
        .collect();
    assert_eq!(suffix_entries.len(), 1, "the recognized LexEntry must be kept");
}

#[test]
fn conversion_provenance_round_trips_through_json() {
    let (snapshot, _report) = pg_fwdata::import_file(&fixture_path()).unwrap();
    let round_tripped = pg_snapshot::Snapshot::from_json(&snapshot.to_json()).unwrap();
    assert_eq!(
        round_tripped.conversion_provenance,
        snapshot.conversion_provenance
    );
}
