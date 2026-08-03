//! **The sizing measurement for net-level candidate dedup — committed BEFORE the optimization it
//! justifies.**
//!
//! # The question
//!
//! `evaluate_plans_with_cache_mode` builds, finishes and runs the WHOLE corpus for every plan.
//! Nothing notices that two plans produced the same network. The optimization that follows this file
//! collapses those duplicates, and its entire value is the ratio measured here:
//!
//! > across a fixture's materialized candidate set, how many DISTINCT finished networks are there,
//! > against how many plans?
//!
//! If that ratio is 1:1 everywhere, the optimization is worth nothing on our corpora and the honest
//! finding is worth more than the code. So the number is taken first, with the instrument
//! (`finished_net_digests`) and this census committed as their own change, and it stays as a
//! permanent gate: a future change that made every plan produce a distinct network would silently
//! remove the whole basis for the dedup, and this test is what says so out loud.
//!
//! # Why the digest is taken after `finish_controllable_net`
//!
//! That is the last point at which a plan-composed candidate is still an `Fsm`, and it is the net that
//! is actually queried. Digesting the pre-finish net would key on a network that returns nothing for
//! every surface query (`crate::build::finish_controllable_net`'s own doc), and digesting the plan
//! instead would measure the wrong thing entirely — plan-shape differences are ERASED by
//! minimization, which is exactly why duplicates exist.
//!
//! # What this census deliberately does not do
//!
//! No oracle, no propose, no confirm. It is the build half only, which is what makes a sweep over the
//! whole discoverable fixture corpus affordable. A distinct-net count needs nothing else: the
//! measurement is a property of the compilation.

use pg_conformance_fixtures::{discover, FixtureRef};
use pg_foma::enumerate::{enumerate_default, LoweredCandidate};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::recipe_runtime::{finished_net_digests, RuntimeBudget};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};
use std::collections::BTreeSet;

/// `pg_foma::emit::surface_table`, which is `pub(crate)`: the surface char-def table is the LAST
/// stratum's, not `char_tables[0]`, and on a multi-stratum grammar those differ. Replicated (as
/// `parity_divergence_census` already does) so the census builds over the same alphabet the evaluator
/// does.
fn surface_table(grammar: &Grammar) -> &pg_grammar::chardef::CharDefTable {
    let surface_stratum = grammar
        .strata
        .last()
        .expect("a loaded grammar always has at least one stratum");
    &grammar.char_tables[surface_stratum.table.0 as usize]
}

fn registry_plans(grammar: &Grammar) -> Vec<LoweredCandidate> {
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = pg_foma::enumerate::prules_in_order(grammar);
    let phonology = PhonologyProbe::new(grammar);
    let baseline = enumerate_default(grammar, &alphabet, &prules, phonology.as_ref());
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

// The exclusion list that stood here is GONE (2026-08-03). It named
// `deep-optional-affix-nesting`/`recipe-template-generic` and left the load-bearing question open in
// its own words: "Whether they abort in the BUILD half specifically is unknown — this census runs no
// propose/confirm, so it might well survive them". It does. Measured: `finished_net_digests` for that
// grammar's registry plans completes in 0.027s; the death was entirely in `apply_up` traversal
// (`12^k` paths for a k-`x` word), never in construction. See `tests/apply_path_refusal_gate.rs` and
// `pg_foma::compose_budget::DEFAULT_EVALUATION_APPLY_PATH_BUDGET`. So this census now sweeps every
// discoverable fixture with no exclusion at all.

fn measure_one_fixture(fixture: &FixtureRef) -> Result<Sizing, String> {
    // Named BEFORE any work, so a process-killing fixture is identifiable from the last line of
    // captured output. `parity_divergence_census` learned this the hard way: without it, a 250s abort
    // names nothing at all.
    eprintln!("net-dedup sizing: entering {}", fixture.label());
    let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
        return Err(format!("{}: grammar failed to load", fixture.label()));
    };
    let plans = registry_plans(&grammar);
    if plans.is_empty() {
        return Err(format!("{}: no candidate materialized", fixture.label()));
    }
    let outcomes = finished_net_digests(&grammar, &plans, RuntimeBudget::default());
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

/// **The measurement.** Per-fixture rows plus the corpus-wide totals, printed unconditionally so the
/// number is in the run log whether the assertions pass or not.
#[test]
fn distinct_finished_nets_versus_plan_count_per_fixture() {
    let mut measured: Vec<Sizing> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for fixture in discover() {
        // A panic in one fixture's compilation must not cost the corpus-wide number. Known live
        // example: `machine:edge-cases/loader-pattern-shapes` panics at `replace.rs:498` ("char table
        // too large for the PUA token scheme"). Caught, named, and counted -- never swallowed.
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
    // The claim the optimization rests on, asserted rather than assumed. If this ever fails, the
    // finding is that net-level dedup buys nothing on this corpus -- which is a reason to delete the
    // optimization, not to weaken this line.
    assert!(
        duplicates > 0,
        "every one of {digested} digested plans produced a DISTINCT finished network across {} \
         fixtures. Net-level dedup can then never fire on this corpus and its entire justification is \
         gone -- report that, do not relax this assertion",
        measured.len()
    );
}
