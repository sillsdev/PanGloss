//! Bidirectional conversion-loss gate over every checked-in FieldWorks project fixture (`pg_snapshot::conversion`'s vocabulary).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pg_grammar::compile::test_support::assert_grammars_equal;
use pg_grammar::compile_project_measured;
use pg_snapshot::{ConversionInventory, InventoryDelta, RawSourceCensus, Snapshot};

fn fixture_fwdata_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../pg-fwdata/tests/data/fixture.fwdata")
}

/// One checked-in project fixture this gate measures.
struct Fixture {
    name: &'static str,
    path: PathBuf,
}

/// Every checked-in project fixture this gate covers; a missing one is a hard failure, never a self-skip.
fn fixtures() -> Vec<Fixture> {
    let path = fixture_fwdata_path();
    assert!(
        path.exists(),
        "checked-in fixture missing: {} -- a missing fixture is a gate failure, not a skip",
        path.display()
    );
    vec![Fixture {
        name: "fixture.fwdata",
        path,
    }]
}

/// Today's measured ceiling for one (fixture, stage) pair -- a ratchet, not a target; raise it only alongside a measurement showing why.
struct Ratchet {
    silently_omitted: usize,
    unclassified: usize,
    synthesized_only: usize,
    rejected: usize,
}

const ZERO_RATCHET: Ratchet = Ratchet {
    silently_omitted: 0,
    unclassified: 0,
    synthesized_only: 0,
    rejected: 0,
};

fn import_ratchet(fixture: &str) -> Ratchet {
    match fixture {
        // Measured: 2 rejected, the fixture's own deliberate dangling references (fixture_tests.rs).
        "fixture.fwdata" => Ratchet {
            rejected: 2,
            ..ZERO_RATCHET
        },
        other => panic!("no import ratchet recorded for fixture {other:?}"),
    }
}

fn compile_ratchet(fixture: &str) -> Ratchet {
    match fixture {
        // Measured today's session; see this test's own --no-capture output for the breakdown.
        "fixture.fwdata" => Ratchet {
            silently_omitted: 1,
            synthesized_only: 9,
            rejected: 9,
            ..ZERO_RATCHET
        },
        other => panic!("no compile ratchet recorded for fixture {other:?}"),
    }
}

fn print_census(census: &RawSourceCensus) {
    println!(
        "  raw census: total_occurrences={} classes={} unhandled_classes={}",
        census.total_occurrences,
        census.class_occurrences.len(),
        census.unhandled_class_occurrences.len(),
    );
}

fn print_inventory(stage: &str, inventory: &ConversionInventory) {
    println!(
        "  {stage} inventory: authored={} considered={} selected={} represented={} rejected={} synthesized={}",
        inventory.authored.len(),
        inventory.considered.len(),
        inventory.selected.len(),
        inventory.represented.len(),
        inventory.rejected.len(),
        inventory.synthesized.len(),
    );
}

/// How many keys of a large category to print by name before eliding the rest.
const KEY_LISTING_CAP: usize = 20;

/// Prints up to [`KEY_LISTING_CAP`] keys from `keys` under `label`, eliding and counting the rest.
fn print_keys(label: &str, keys: &BTreeSet<pg_snapshot::InventoryKey>) {
    if keys.is_empty() {
        return;
    }
    println!("    {label} ({}):", keys.len());
    for key in keys.iter().take(KEY_LISTING_CAP) {
        println!("      {key:?}");
    }
    if keys.len() > KEY_LISTING_CAP {
        println!("      ... and {} more elided", keys.len() - KEY_LISTING_CAP);
    }
}

fn print_delta(stage: &str, delta: &InventoryDelta) {
    println!(
        "  {stage} delta: silently_omitted={} unclassified={} synthesized_only={} rejected={}",
        delta.silently_omitted.len(),
        delta.unclassified.len(),
        delta.synthesized_only.len(),
        delta.inventory.rejected.len(),
    );
    print_keys("silently_omitted", &delta.silently_omitted);
    print_keys("unclassified", &delta.unclassified);
    print_keys("synthesized_only", &delta.synthesized_only);
}

fn assert_ratchet(fixture: &str, stage: &str, delta: &InventoryDelta, ratchet: &Ratchet) {
    assert!(
        delta.silently_omitted.len() <= ratchet.silently_omitted,
        "{fixture} {stage}: silently_omitted grew to {} (ratchet {})",
        delta.silently_omitted.len(),
        ratchet.silently_omitted
    );
    assert!(
        delta.unclassified.len() <= ratchet.unclassified,
        "{fixture} {stage}: unclassified grew to {} (ratchet {})",
        delta.unclassified.len(),
        ratchet.unclassified
    );
    assert!(
        delta.synthesized_only.len() <= ratchet.synthesized_only,
        "{fixture} {stage}: synthesized_only grew to {} (ratchet {})",
        delta.synthesized_only.len(),
        ratchet.synthesized_only
    );
    assert!(
        delta.inventory.rejected.len() <= ratchet.rejected,
        "{fixture} {stage}: rejected grew to {} (ratchet {})",
        delta.inventory.rejected.len(),
        ratchet.rejected
    );
}

/// Measures import-through-compile conversion loss plus a serialize/reload round trip, printing and ratcheting every category.
#[test]
fn conversion_inventory_gate() {
    let fixtures = fixtures();
    assert!(!fixtures.is_empty(), "no checked-in project fixture discovered");

    let mut all_keys: BTreeSet<pg_snapshot::InventoryKey> = BTreeSet::new();

    for fixture in &fixtures {
        println!("fixture: {}", fixture.name);

        let (snapshot, _import_report, import_delta) =
            pg_fwdata::import_file_measured(&fixture.path)
                .unwrap_or_else(|e| panic!("{}: must import: {e}", fixture.name));

        snapshot
            .conversion_provenance
            .validate()
            .unwrap_or_else(|e| panic!("{}: provenance must validate: {e}", fixture.name));

        assert!(
            !import_delta.inventory.authored.is_empty(),
            "{}: authored must be non-empty -- an empty census means this gate is not looking at anything",
            fixture.name
        );

        print_census(&snapshot.conversion_provenance.source_census);
        print_inventory("import", &import_delta.inventory);
        print_delta("import", &import_delta);
        assert_ratchet(fixture.name, "import", &import_delta, &import_ratchet(fixture.name));

        let (grammar, compile_warnings, compile_delta) = compile_project_measured(&snapshot)
            .unwrap_or_else(|e| panic!("{}: must compile: {e}", fixture.name));

        print_inventory("compile", &compile_delta.inventory);
        print_delta("compile", &compile_delta);
        assert_ratchet(fixture.name, "compile", &compile_delta, &compile_ratchet(fixture.name));

        // --- serialize/reload round trip: must recompile to an equal Grammar, byte-identical warnings
        let json = snapshot.to_json();
        let reloaded = Snapshot::from_json(&json)
            .unwrap_or_else(|e| panic!("{}: reloaded snapshot must parse: {e}", fixture.name));
        let (grammar_reloaded, warnings_reloaded, _reloaded_delta) =
            compile_project_measured(&reloaded)
                .unwrap_or_else(|e| panic!("{}: reloaded snapshot must compile: {e}", fixture.name));
        assert_grammars_equal(&grammar, &grammar_reloaded);
        assert_eq!(
            compile_warnings.len(),
            warnings_reloaded.len(),
            "{}: reloaded warning count must match",
            fixture.name
        );
        for (a, b) in compile_warnings.iter().zip(warnings_reloaded.iter()) {
            assert_eq!(a, b, "{}: reloaded warnings must be byte-identical", fixture.name);
        }

        for key in import_delta
            .inventory
            .authored
            .iter()
            .chain(import_delta.inventory.considered.iter())
            .chain(import_delta.inventory.selected.iter())
            .chain(import_delta.inventory.represented.iter())
            .chain(import_delta.inventory.rejected.iter())
            .chain(import_delta.inventory.synthesized.iter())
            .chain(compile_delta.inventory.authored.iter())
            .chain(compile_delta.inventory.considered.iter())
            .chain(compile_delta.inventory.selected.iter())
            .chain(compile_delta.inventory.represented.iter())
            .chain(compile_delta.inventory.rejected.iter())
            .chain(compile_delta.inventory.synthesized.iter())
        {
            all_keys.insert(key.clone());
        }
    }

    assert!(
        !all_keys.is_empty(),
        "no InventoryKeys observed across any fixture -- the gate compared nothing"
    );
    println!("total distinct InventoryKeys compared across all fixtures: {}", all_keys.len());
}
