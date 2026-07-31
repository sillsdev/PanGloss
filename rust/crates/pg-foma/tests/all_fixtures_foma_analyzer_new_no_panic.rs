//! Task #45's own broader finding: `bistratal-overlapping-segment-representation`'s panic in
//! `FomaAnalyzer::new` (`pg_foma::composite::FomaAnalyzer::new` -> `FomaProposer::new` ->
//! `emit::emit_with_budget_profiled` -> `collect_roots` -> `emit::pattern_variants` ->
//! `pg_grammar::chardef::CharDefTable::get`, an out-of-bounds `Vec` index) hid for as long as it did
//! because that fixture's OWN conformance gate
//! (`tests/cover_bistratal_overlapping_segment_representation.rs`) never drives `FomaAnalyzer::new`
//! at all -- it only exercises `evaluate_capability` and `pg_parse::Morpher` directly. Discovering
//! the crash needed a harness that pushes EVERY discovered fixture through the real production
//! entry point, regardless of what that fixture's own gate happens to check.
//!
//! This is that harness, kept permanently (not a one-off repro): every fixture
//! `pg_conformance_fixtures::discover` finds (both `machine/conformance/**` and
//! `conformance-staging/**`, task brief: "do not write a second path walker") must go through
//! `FomaAnalyzer::new` without ever PANICKING. An `Err` (an honest decline -- a capability refusal,
//! an enumeration-budget trip, a lexc compile failure) is always an acceptable outcome; a panic
//! never is -- "a crash, not a refusal" is exactly the class of bug this gate exists to catch before
//! it hides behind an unrelated test's own narrower path again.
//!
//! Deliberately does NOT assert anything about which fixtures compile vs decline (that is
//! `tests/conformance_coverage_gate.rs`'s job, and this crate's `capability.rs`/coverage-ledger
//! machinery already grades each construct's disposition) -- this gate's only claim is "never
//! crashes", checked across the whole corpus on every run.

use std::panic::{self, AssertUnwindSafe};

use pg_conformance_fixtures::discover;
use pg_foma::composite::FomaAnalyzer;

#[test]
fn every_fixture_reaches_foma_analyzer_new_without_panicking() {
    // Suppress the default panic-message spam for this loop (we catch and report ourselves, with
    // the fixture label attached) -- restored unconditionally before this test returns, panic or not.
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let mut panicked: Vec<String> = Vec::new();
    let mut compiled = 0usize;
    let mut declined = 0usize;
    let mut load_failed = 0usize;

    for f in discover() {
        let label = f.label();
        let xml = f.load_grammar_xml();
        let Ok(g) = pg_grammar::load(&xml) else {
            // A fixture this preview can't even load is a different gate's job to diagnose
            // (`pg-parse`'s own `conformance_fixtures_gate.rs`) -- not counted as a panic here.
            load_failed += 1;
            continue;
        };

        let result = panic::catch_unwind(AssertUnwindSafe(|| FomaAnalyzer::new(&g).is_ok()));
        match result {
            Ok(true) => compiled += 1,
            Ok(false) => declined += 1,
            Err(payload) => {
                let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "<non-string panic payload>".to_string()
                };
                panicked.push(format!("{label}: {msg}"));
            }
        }
    }

    panic::set_hook(default_hook);

    println!(
        "all_fixtures_foma_analyzer_new_no_panic: compiled={compiled} declined={declined} \
         load_failed={load_failed} panicked={}",
        panicked.len()
    );

    assert!(
        panicked.is_empty(),
        "FomaAnalyzer::new PANICKED on {} fixture(s) -- a crash is never an acceptable outcome (an \
         honest Err decline always is): {panicked:#?}",
        panicked.len()
    );
    assert!(
        compiled + declined > 0,
        "sanity: this sweep must actually reach at least one fixture (discover() returned an \
         unexpectedly empty/all-unloadable set -- check the fixture roots)"
    );
}
