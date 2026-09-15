//! Ratchet on how many discovered fixtures leave `fieldworks_producible` unmarked; covers every staged category by adding `discover_filter_passes` to `discover`'s own result.

use pg_conformance_fixtures::{
    claimed_scope, discover, discover_filter_passes, producibility_census, ConformanceScope,
    FieldworksProducibility,
};

/// Ratchet, not a target: falls only when a fixture is actually marked, never raised to admit a new silent one.
fn unmarked_allowed(scope: ConformanceScope) -> usize {
    match scope {
        ConformanceScope::Local => 0,
        ConformanceScope::All => 0,
    }
}

#[test]
fn unmarked_fixtures_do_not_grow() {
    let scope = claimed_scope();
    let mut fixtures = discover();
    fixtures.extend(discover_filter_passes());
    assert!(
        !fixtures.is_empty(),
        "no fixtures discovered at all under scope {} -- check the `machine` submodule is \
         initialized and conformance-staging/ exists",
        scope.label()
    );
    let census = producibility_census(&fixtures);

    println!(
        "producibility census (scope={}, {} fixtures discovered)",
        scope.label(),
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

    let allowed = unmarked_allowed(scope);
    assert!(
        census.unmarked.len() <= allowed,
        "unmarked (fieldworks_producible absent) fixture count grew from the scope={} ratchet of \
         {allowed} to {} -- a newly staged or upstream-advanced fixture must state its FieldWorks \
         producibility explicitly, never leave it silent: {:?}",
        scope.label(),
        census.unmarked.len(),
        census.unmarked
    );
}
