//! Every registered capability predicate owes a NEGATIVE witness -- a discovered fixture whose envelope refusal cites it -- because a predicate nothing provokes is a control never demonstrated to act. This is the attributable half of coverage: a refusal names its own predicate, where a fixture's aggregate outcome names only the fixture.

use std::collections::BTreeSet;

use pg_conformance_fixtures::discover;
use pg_foma::capability::{default_grammar_wide_checks, default_registry};
use pg_foma::strategy_coverage_join::negative_witness_index;

/// A ratchet, not a target: an entry may only ever be REMOVED, and a stale entry fails the test.
const WITHOUT_NEGATIVE_WITNESS: &[&str] = &[
    "circumfix-output-action.faithful-structural-composite",
    "compounding.non-recursive",
    "epenthesis.structural-composite-route",
    // Fixed: this predicate's only witness now compiles under TunedSurfaceProbed; see `AllomorphZoneOutcome::OwnZoneElsewhere`.
    "surface-probe.circumfix-zone-exclusive-allomorph",
    // Fixed: `realizational_rule_is_semantically_unbounded` is now always `false`; see `TunedSurfaceClosureCheck`'s own doc.
    "surface-probe.finite-closure-bound",
    "metathesis.faithful-swap-construction",
    "mpr-group.append-output",
    "mpr-group.overwrite-output",
    "multi-table.faithful-table-threading",
    "quantifier.bounded-expansion",
    "reduplication.peel-eligible-rule-kind",
    "right-to-left-rewrite.faithful-reversal-construction",
    "unordered-application.chain-depth-bounded",
];

fn registered_ids() -> BTreeSet<&'static str> {
    let registry = default_registry();
    let mut ids: BTreeSet<&'static str> = registry.predicates().iter().map(|p| p.id()).collect();
    ids.extend(default_grammar_wide_checks().iter().map(|c| c.id()));
    ids
}

#[test]
fn every_registered_predicate_has_a_negative_witness_or_is_named_in_the_backlog() {
    let fixtures = discover();
    // Non-vacuity: an empty walk would make every assertion below pass while proving nothing.
    assert!(
        fixtures.len() > 40,
        "only {} fixtures discovered; a short walk makes this gate vacuous",
        fixtures.len()
    );

    let grammars: Vec<(String, pg_grammar::model::Grammar)> = fixtures
        .iter()
        .filter_map(|f| {
            pg_grammar::load(&f.load_grammar_xml())
                .ok()
                .map(|g| (f.label(), g))
        })
        .collect();
    assert!(
        grammars.len() > 40,
        "only {} fixture grammars loaded of {}",
        grammars.len(),
        fixtures.len()
    );

    let index = negative_witness_index(grammars.iter().map(|(l, g)| (l.clone(), g)));
    assert!(
        !index.is_empty(),
        "no predicate was provoked by any fixture -- the envelope is not being consulted"
    );

    let registered = registered_ids();
    let witnessed: BTreeSet<&'static str> = index
        .keys()
        .copied()
        .filter(|p| registered.contains(p))
        .collect();
    let missing: BTreeSet<&'static str> =
        registered.difference(&witnessed).copied().collect();
    let allowed: BTreeSet<&'static str> = WITHOUT_NEGATIVE_WITNESS.iter().copied().collect();

    let regressed: Vec<&&str> = missing.difference(&allowed).collect();
    assert!(
        regressed.is_empty(),
        "these predicates lost their negative witness: {regressed:?}\n\
         a registered predicate that no fixture provokes cannot be shown to act"
    );

    // The ratchet only tightens: a predicate that gained a witness must leave the list, or the list stops describing reality.
    let stale: Vec<&&str> = allowed.difference(&missing).collect();
    assert!(
        stale.is_empty(),
        "these predicates now HAVE a negative witness and must be removed from \
         WITHOUT_NEGATIVE_WITNESS: {stale:?}"
    );
}

/// Guards the guard: a renamed or deleted predicate must not leave a silently-passing backlog entry.
#[test]
fn the_backlog_names_only_registered_predicates() {
    let registered = registered_ids();
    let unknown: Vec<&&str> = WITHOUT_NEGATIVE_WITNESS
        .iter()
        .filter(|p| !registered.contains(**p))
        .collect();
    assert!(
        unknown.is_empty(),
        "WITHOUT_NEGATIVE_WITNESS names ids that are not registered predicates: {unknown:?}"
    );
}
