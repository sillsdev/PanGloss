//! Self-skipping conformance tests against real FieldWorks sample projects (Sena 3, Amharic).
//! These are untracked corpus files present on this development machine but not guaranteed
//! present elsewhere (CI, a fresh clone) — the same self-skip process rule `hc-grammar`'s
//! `sample_path()` tests follow. Located via `PANGLOSS_FW_PROJECTS_DIR`, falling back to the
//! known sibling checkout path.

use std::path::PathBuf;

use pg_snapshot::PartOfSpeech;

/// Locates a FieldWorks project's `.fwdata` file, or `None` if the FieldWorks checkout isn't
/// present on this machine (so tests can self-skip rather than fail).
fn project_fwdata(project_dir_name: &str) -> Option<PathBuf> {
    let base = std::env::var("PANGLOSS_FW_PROJECTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(r"C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects")
        });
    let path = base
        .join(project_dir_name)
        .join(format!("{project_dir_name}.fwdata"));
    path.exists().then_some(path)
}

fn count_pos_tree(pos: &[PartOfSpeech]) -> usize {
    pos.iter()
        .map(|p| 1 + count_pos_tree(&p.children))
        .sum()
}

fn count_templates(pos: &[PartOfSpeech]) -> usize {
    pos.iter()
        .map(|p| p.affix_templates.len() + count_templates(&p.children))
        .sum()
}

#[test]
fn sena3_imports_with_expected_counts() {
    let Some(path) = project_fwdata("Sena 3") else {
        eprintln!("skipping sena3_imports_with_expected_counts: FieldWorks checkout not present");
        return;
    };
    let (snap, report) = pg_fwdata::import_file(&path).expect("Sena 3 must import");

    // Exact counts independently confirmed via `grep -c '<rt class="X"' "Sena 3.fwdata"` (note:
    // anchored to `<rt class=`, not just `class="X"` — Sena 3's `<AdditionalFields>` block also
    // contains two `<CustomField class="LexEntry" .../>` elements that a naive `class="X"` count
    // would double-count as if they were `<rt>` records).
    assert_eq!(snap.lexicon.entries.len(), 1462, "LexEntry count");
    assert_eq!(snap.phonology.phonemes.len(), 44, "PhPhoneme count");
    assert_eq!(count_pos_tree(&snap.morphology.parts_of_speech), 37, "PartOfSpeech count");
    assert_eq!(count_templates(&snap.morphology.parts_of_speech), 25, "MoInflAffixTemplate count");
    assert_eq!(snap.phonology.environments.len(), 44, "PhEnvironment count");
    assert_eq!(snap.phonology.natural_classes.len(), 18, "PhNCSegments count (no PhNCFeatures in Sena 3)");
    assert_eq!(snap.morphology.compound_rules.len(), 4, "compound rule count (all MoExoCompound)");
    assert_eq!(snap.morphology.lex_entry_infl_types.len(), 3, "LexEntryInflType count");

    eprintln!("Sena 3 import warnings ({}):", report.warnings.len());
    for w in &report.warnings {
        eprintln!("  {w}");
    }
    assert!(
        report.warnings.is_empty(),
        "Sena 3 is a clean project; unexpected import warnings: {:#?}",
        report.warnings
    );

    // A genuine stale reference in Sena 3 itself: one LexSense's MorphoSyntaxAnalysis points at
    // an MSA that isn't one of its own entry's — `Snapshot::validate()`'s job to catch, not
    // pg-fwdata's to resolve/repair.
    let validate_warnings = snap.validate();
    eprintln!("Sena 3 validate() warnings ({}):", validate_warnings.len());
    for w in &validate_warnings {
        eprintln!("  {w}");
    }
    assert_eq!(
        validate_warnings.len(),
        1,
        "expected exactly the one known stale sense/MSA reference; got: {:#?}",
        validate_warnings
    );
    assert!(validate_warnings[0].contains("does not resolve within this entry"));
}

#[test]
fn amharic_imports_with_expected_counts_and_adhoc_warning() {
    let Some(path) = project_fwdata("Amharic") else {
        eprintln!("skipping amharic_imports_with_expected_counts_and_adhoc_warning: FieldWorks checkout not present");
        return;
    };
    let (snap, report) = pg_fwdata::import_file(&path).expect("Amharic must import successfully, not crash");

    assert_eq!(snap.lexicon.entries.len(), 130, "LexEntry count");
    assert_eq!(snap.phonology.phonemes.len(), 417, "PhPhoneme count");
    assert_eq!(count_pos_tree(&snap.morphology.parts_of_speech), 16, "PartOfSpeech count");
    assert_eq!(count_templates(&snap.morphology.parts_of_speech), 18, "MoInflAffixTemplate count");
    assert_eq!(snap.phonology.environments.len(), 8, "PhEnvironment count");
    assert_eq!(snap.phonology.natural_classes.len(), 18, "natural class count (3 PhNCSegments + 15 PhNCFeatures)");
    assert_eq!(snap.morphology.compound_rules.len(), 1, "compound rule count (1 MoEndoCompound)");
    assert_eq!(snap.morphology.adhoc_prohibitions.len(), 1, "adhoc prohibition count (the stale one)");
    assert_eq!(snap.morphology.lex_entry_infl_types.len(), 5, "LexEntryInflType count");

    // §1's motivating example: a stale MoMorphAdhocProhib crashes FieldWorks' own C# exporter.
    // pg-fwdata must import successfully (already asserted above via `expect`) *and* surface a
    // warning about it.
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("adhocProhibitions") || w.contains("ad-hoc")),
        "expected a warning about the stale ad-hoc rule; warnings were: {:#?}",
        report.warnings
    );

    eprintln!("Amharic import warnings ({}):", report.warnings.len());
    for w in &report.warnings {
        eprintln!("  {w}");
    }
    assert_eq!(
        report.warnings.len(),
        1,
        "expected exactly the one stale-ad-hoc-rule warning; got: {:#?}",
        report.warnings
    );

    // Unlike Sena 3's dangling sense/MSA reference, the stale ad-hoc rule here is *structural*
    // (its MSA's slot belongs to a disabled affix template — the FieldWorks-exporter-crashing
    // condition from §1) rather than a dangling guid, so `validate()` (which only checks raw
    // reference resolution) has nothing to add.
    let validate_warnings = snap.validate();
    eprintln!("Amharic validate() warnings ({}):", validate_warnings.len());
    for w in &validate_warnings {
        eprintln!("  {w}");
    }
    assert!(
        validate_warnings.is_empty(),
        "unexpected validate() warnings: {:#?}",
        validate_warnings
    );
}

#[test]
fn imports_are_deterministic_across_runs() {
    for project in ["Sena 3", "Amharic"] {
        let Some(path) = project_fwdata(project) else {
            eprintln!("skipping imports_are_deterministic_across_runs({project}): FieldWorks checkout not present");
            continue;
        };
        let (snap1, report1) = pg_fwdata::import_file(&path).unwrap();
        let (snap2, report2) = pg_fwdata::import_file(&path).unwrap();
        assert_eq!(snap1.to_json(), snap2.to_json(), "{project}: repeated import must be byte-identical");
        assert_eq!(report1.warnings, report2.warnings, "{project}: warnings must be identical across imports");
    }
}
