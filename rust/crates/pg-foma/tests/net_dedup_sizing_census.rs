//! The sizing measurement for net-level candidate dedup, committed BEFORE the optimization it justifies: how many DISTINCT finished networks a fixture's materialized candidate set produces, against how many plans -- build half only, no oracle/propose/confirm.
//! See `docs/research/pg-foma-net-dedup-sizing-census.md` for why the digest is taken after `finish_controllable_net` and why the census excludes no fixture.

use pg_conformance_fixtures::{discover, FixtureRef};
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::finished_net_digests;
use pg_foma::enumerate::{enumerate_default, LoweredCandidate};
use pg_foma::junctions::PhonologyProbe;
use pg_grammar::model::{Grammar, PhonRuleDef};
use std::collections::BTreeSet;

fn registry_plans(grammar: &Grammar) -> Vec<LoweredCandidate> {
    let prules: Vec<&PhonRuleDef> = pg_foma::enumerate::prules_in_order(grammar);
    let phonology = PhonologyProbe::new(grammar);
    let baseline = enumerate_default(grammar, &prules, phonology.as_ref());
    Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar,
            baseline: &baseline,
        })
        .map(|candidates| candidates.into_iter().map(|(_, plan)| plan).collect())
        .unwrap_or_default()
}

/// One fixture's sizing row.
struct Sizing {
    label: String,
    /// Plans materialized for this fixture, whatever their strategy.
    plans: usize,
    /// Plans that produced a finished network to digest.
    digested: usize,
    /// Distinct digests among those.
    distinct: usize,
    /// Plans with no digest, and why (whole-grammar strategy, build failure, empty net).
    unrealized: Vec<String>,
}

impl Sizing {
    /// Plans whose measurement a net-level cache could have served from another plan's.
    fn duplicates(&self) -> usize {
        self.digested - self.distinct
    }
}

// No fixture is excluded: this census runs no propose/confirm, so the `apply_up` traversal blowup seen elsewhere (see `tests/apply_path_refusal_gate.rs`) never applies to construction alone.

fn measure_one_fixture(fixture: &FixtureRef) -> Result<Sizing, String> {
    // Named BEFORE any work, so a process-killing fixture is identifiable from the last line of captured output.
    eprintln!("net-dedup sizing: entering {}", fixture.label());
    let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
        return Err(format!("{}: grammar failed to load", fixture.label()));
    };
    let plans = registry_plans(&grammar);
    if plans.is_empty() {
        return Err(format!("{}: no candidate materialized", fixture.label()));
    }
    let outcomes = finished_net_digests(&grammar, &plans);
    let mut digests = BTreeSet::new();
    let mut digested = 0usize;
    let mut unrealized = Vec::new();
    for (plan, outcome) in plans.iter().zip(outcomes) {
        match outcome {
            Ok(digest) => {
                digested += 1;
                digests.insert(digest);
            }
            Err(reason) => unrealized.push(format!("{}: {reason}", plan.label)),
        }
    }
    Ok(Sizing {
        label: fixture.label(),
        plans: plans.len(),
        digested,
        distinct: digests.len(),
        unrealized,
    })
}

/// The measurement: per-fixture rows plus corpus-wide totals, printed unconditionally so the number is in the run log whether the assertions pass or not.
#[test]
fn distinct_finished_nets_versus_plan_count_per_fixture() {
    let mut measured: Vec<Sizing> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for fixture in discover() {
        // A panic in one fixture's compilation must not cost the corpus-wide number: caught, named, and counted, never swallowed.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            measure_one_fixture(&fixture)
        }));
        match outcome {
            Ok(Ok(sizing)) => measured.push(sizing),
            Ok(Err(reason)) => skipped.push(reason),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "non-string panic payload".to_string());
                skipped.push(format!("{}: PANICKED -- {message}", fixture.label()));
            }
        }
    }

    eprintln!("--- net-dedup sizing: distinct finished nets vs plan count ---");
    for sizing in &measured {
        eprintln!(
            "{}: plans={} digested={} DISTINCT={} duplicates={}{}",
            sizing.label,
            sizing.plans,
            sizing.digested,
            sizing.distinct,
            sizing.duplicates(),
            if sizing.unrealized.is_empty() {
                String::new()
            } else {
                format!(" unrealized=[{}]", sizing.unrealized.join("; "))
            }
        );
    }
    for skip in &skipped {
        eprintln!("skipped {skip}");
    }
    let plans: usize = measured.iter().map(|sizing| sizing.plans).sum();
    let digested: usize = measured.iter().map(|sizing| sizing.digested).sum();
    let distinct: usize = measured.iter().map(|sizing| sizing.distinct).sum();
    let duplicates: usize = measured.iter().map(Sizing::duplicates).sum();
    let fixtures_with_duplicates = measured
        .iter()
        .filter(|sizing| sizing.duplicates() > 0)
        .count();
    eprintln!(
        "TOTAL: {} fixtures measured, {} skipped, {plans} plans, {digested} digested, \
         {distinct} distinct nets, {duplicates} duplicate nets, \
         {fixtures_with_duplicates} fixtures with at least one duplicate",
        measured.len(),
        skipped.len()
    );

    assert!(
        measured.len() >= 10,
        "the sizing census must actually span the fixture corpus; measured only {} fixtures ({} \
         skipped) -- a census that measures almost nothing sizes nothing",
        measured.len(),
        skipped.len()
    );
    assert!(
        digested > 0,
        "no fixture produced a finished network at all, so a zero duplicate count is the absence of \
         evidence rather than evidence of absence"
    );
    // The claim the optimization rests on, asserted rather than assumed: a failure here means dedup buys nothing on this corpus, a reason to delete the optimization, not to weaken this line.
    assert!(
        duplicates > 0,
        "every one of {digested} digested plans produced a DISTINCT finished network across {} \
         fixtures. Net-level dedup can then never fire on this corpus and its entire justification is \
         gone -- report that, do not relax this assertion",
        measured.len()
    );
}
