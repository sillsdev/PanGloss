//! Which registered predicates have a NEGATIVE witness among the discovered conformance fixtures: for every fixture x strategy, the envelope's typed refusal (`strategy_coverage_join::envelope_refusal_predicates`), tallied per predicate. Envelope only -- characterizes without emitting, so it compiles no artifact and makes no claim about what a build would do.

use std::collections::{BTreeMap, BTreeSet};

use pg_conformance_fixtures::discover;
use pg_foma::strategy_coverage::ALL_STRATEGIES;
use pg_foma::strategy_coverage_join::envelope_refusal_predicates;

fn main() {
    let fixtures = discover();
    println!("fixtures discovered: {}", fixtures.len());
    println!("strategies: {:?}\n", ALL_STRATEGIES);

    // predicate -> strategy -> fixtures citing it, so a predicate proven only on one backend is visible as such.
    let mut fired: BTreeMap<&'static str, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut refused_cells = 0usize;
    let mut admitted_cells = 0usize;

    for fixture in &fixtures {
        let grammar = match pg_grammar::load(&fixture.load_grammar_xml()) {
            Ok(g) => g,
            Err(e) => {
                println!("  LOAD FAILED {}: {e:?}", fixture.label());
                continue;
            }
        };
        for &strategy in ALL_STRATEGIES {
            let predicates = envelope_refusal_predicates(&grammar, strategy);
            if predicates.is_empty() {
                admitted_cells += 1;
                continue;
            }
            refused_cells += 1;
            for p in predicates {
                fired
                    .entry(p)
                    .or_default()
                    .entry(format!("{strategy:?}"))
                    .or_default()
                    .push(fixture.label());
            }
        }
    }

    println!("cells: {admitted_cells} admitted, {refused_cells} refused\n");
    println!("=== predicates WITH a negative witness ({}) ===", fired.len());
    for (predicate, by_strategy) in &fired {
        let total: usize = by_strategy.values().map(|v| v.len()).sum();
        let strategies: Vec<&str> = by_strategy.keys().map(String::as_str).collect();
        println!(
            "  {predicate:<52} {total:>3} cell(s) across {:?}",
            strategies
        );
        for (strategy, labels) in by_strategy {
            let shown: Vec<&str> = labels.iter().take(2).map(String::as_str).collect();
            let more = labels.len().saturating_sub(shown.len());
            let suffix = if more > 0 {
                format!(" (+{more} more)")
            } else {
                String::new()
            };
            println!("      {strategy:<28} {}{suffix}", shown.join(", "));
        }
    }

    // The point of the census: a registered control no fixture provokes has never been demonstrated to act.
    let registry = pg_foma::capability::default_registry();
    let mut all_registered: BTreeSet<&'static str> =
        registry.predicates().iter().map(|p| p.id()).collect();
    all_registered.extend(
        pg_foma::capability::default_grammar_wide_checks()
            .iter()
            .map(|c| c.id()),
    );
    let without: Vec<&&'static str> = all_registered
        .iter()
        .filter(|p| !fired.contains_key(**p))
        .collect();
    println!(
        "\n=== predicates WITHOUT a negative witness ({} of {} registered) ===",
        without.len(),
        all_registered.len()
    );
    for p in without {
        println!("  {p}");
    }

    // Emitted by `capability::strategy_floor`, not by a registered predicate, so `inert_predicates` cannot see it and the two counts above do not sum to the ids that fired.
    let unregistered: Vec<&&'static str> = fired
        .keys()
        .filter(|p| !all_registered.contains(*p))
        .collect();
    if !unregistered.is_empty() {
        println!(
            "\n=== fired but NOT in the predicate registry ({}) ===",
            unregistered.len()
        );
        for p in unregistered {
            println!("  {p}");
        }
    }
}
