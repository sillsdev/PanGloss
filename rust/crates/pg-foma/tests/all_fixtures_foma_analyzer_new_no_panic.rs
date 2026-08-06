//! Every fixture `pg_conformance_fixtures::discover` finds must go through `FomaAnalyzer::new` without panicking; an `Err` (capability refusal, budget trip, compile failure) is always acceptable, since a fixture's own narrower conformance gate may never drive this entry point at all. Deliberately does not grade which fixtures compile vs decline — that is `tests/conformance_coverage_gate.rs`'s job; this gate's only claim is "never crashes".

use std::panic::{self, AssertUnwindSafe};

use pg_conformance_fixtures::discover;
use pg_foma::composite::FomaAnalyzer;

#[test]
fn every_fixture_reaches_foma_analyzer_new_without_panicking() {
    // Suppresses the default panic-message spam; we catch and report ourselves with the fixture label attached, and restore the hook unconditionally before returning.
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
            // A fixture this preview can't even load is a different gate's job to diagnose, not counted as a panic here.
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
