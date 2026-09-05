//! Ratchet on how many `discover_scoped(All)` fixtures leave `fieldworks_producible` unmarked.

use pg_conformance_fixtures::{
    discover_scoped, producibility_census, ConformanceScope, FieldworksProducibility,
};

/// Ratchet, not a target: falls only when a fixture is actually marked, never raised to admit a new silent one.
const UNMARKED_ALLOWED: usize = 62;

#[test]
fn unmarked_fixtures_do_not_grow() {
    let fixtures = discover_scoped(ConformanceScope::All);
    assert!(
        !fixtures.is_empty(),
        "no fixtures discovered at all under ConformanceScope::All -- check the `machine` \
         submodule is initialized and conformance-staging/ exists"
    );
    let census = producibility_census(&fixtures);

    println!(
        "producibility census (scope=all, {} fixtures discovered)",
        fixtures.len()
    );
    println!(
        "  producible:  {} {:?}",
        census.producible.len(),
        census.producible
    );
    println!(
        "  engine-only: {} {:?}",
        census.engine_only.len(),
        census.engine_only
    );
    println!(
        "  unmarked:    {} {:?}",
        census.unmarked.len(),
        census.unmarked
    );

    // EngineOnly fixtures print their reason too -- a reader must see WHY without opening the file.
    for fixture in &fixtures {
        if let FieldworksProducibility::EngineOnly { notes } =
            fixture.load_words_yaml().fieldworks_producible
        {
            let first_line = notes.lines().next().unwrap_or("").trim();
            println!("  engine-only reason -- {}: {first_line}", fixture.label());
        }
    }

    assert_eq!(
        census.producible.len() + census.engine_only.len() + census.unmarked.len(),
        fixtures.len(),
        "the three buckets must partition every discovered fixture exactly once"
    );

    assert!(
        census.unmarked.len() <= UNMARKED_ALLOWED,
        "unmarked (fieldworks_producible absent) fixture count grew from the ratchet of \
         {UNMARKED_ALLOWED} to {} -- a newly staged or upstream-advanced fixture must state its \
         FieldWorks producibility explicitly, never leave it silent: {:?}",
        census.unmarked.len(),
        census.unmarked
    );
}
