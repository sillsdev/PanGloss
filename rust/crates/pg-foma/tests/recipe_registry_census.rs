//! Census: for every staged conformance fixture, how many DISTINCT plans does the seeded recipe
//! registry actually reach, and which families own them?
//!
//! This exists because "seven declared families" says nothing about the size of the reachable plan
//! space. Applicability predicates cut the seven down per grammar, and `materialize_distinct` then
//! content-address-dedups whatever survives — two families holding the same transform, or two
//! refinement granularities that coincide on a particular partition shape (a 2-entry group bisects
//! into the same [1,1] that fanning out produces), collapse to one candidate. The only way to know
//! which fixtures can tell the transforms apart is to run the registry against all of them.
//!
//! The printed table is the diagnostic; the assertion is the gate. See
//! `recipe_partition_refinement_gate.rs` for the equivalence half of the story.

use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::replace::SegAlphabet;

#[test]
fn some_staged_fixture_separates_more_than_three_registry_transforms() {
    let fixtures = pg_conformance_fixtures::discover();
    assert!(!fixtures.is_empty(), "no staged fixtures discovered");

    let mut best = (0usize, String::new());
    // Tracked separately, and it is the number that actually matters. A distinct plan on a
    // marker-carrying grammar cannot be COMPARED against the baseline:
    // [`pg_foma::recipe_runtime`] rescues the baseline through the tuned `emit` path and refuses
    // every permutation as `unsupported`, because the tuned path derives topology from a plan it
    // builds itself. So plan-space width only becomes an optimization opportunity on the
    // marker-free rows. Measured: the widest fixtures (5 and 4 plans) all carry markers, and no
    // marker-free fixture exceeds 3 -- which is why the optimizer has never reported a
    // non-baseline winner on a real grammar.
    let mut best_marker_free = (0usize, String::new());
    // `markers` matters as much as `distinct` when choosing a fixture. On a plan carrying either
    // marker leaf, `build_controllable` compiles the controllable subtree ONLY, so every candidate
    // net -- and every comparison drawn between them -- excludes what those subtrees contribute, and
    // the CLI refuses non-baseline candidates outright ([`pg_foma::recipe_runtime`]'s marker
    // branch). A fixture with markers can still prove plan-level facts; it cannot demonstrate a
    // whole-grammar comparison. Counting `<AffixTemplate>` declarations does NOT answer this:
    // markers also come from composite entries and circumfix/dropped-material rules.
    eprintln!(
        "{:<58} {:>7} {:>8} {:>7}  families owning a distinct plan",
        "fixture", "entries", "distinct", "markers"
    );
    for fixture in &fixtures {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            eprintln!("{:<58} {:>7} {:>8}  --", fixture.name, "-", "no-load");
            continue;
        };
        if grammar.char_tables.is_empty() {
            continue;
        }
        let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
        let prules = grammar
            .strata
            .iter()
            .flat_map(|s| &s.prules)
            .map(|id| &grammar.prules[id.0 as usize])
            .collect::<Vec<_>>();
        let phonology = PhonologyProbe::new(&grammar);
        let baseline = enumerate_default(&grammar, &alphabet, &prules, phonology.as_ref());
        let candidates = Registry::seeded()
            .materialize_distinct(&MaterializerContext {
                grammar: &grammar,
                baseline: &baseline,
            })
            .expect("materialization must succeed for every staged fixture");
        let families = candidates
            .iter()
            .map(|(instance, _)| instance.family_id.as_str())
            .collect::<Vec<_>>();
        let markers = pg_foma::build::unbuildable_markers(&baseline).len();
        eprintln!(
            "{:<58} {:>7} {:>8} {:>7}  {}",
            fixture.name,
            grammar.entries.len(),
            candidates.len(),
            markers,
            families.join(" ")
        );
        if candidates.len() > best.0 {
            best = (candidates.len(), fixture.name.clone());
        }
        if markers == 0 && candidates.len() > best_marker_free.0 {
            best_marker_free = (candidates.len(), fixture.name.clone());
        }
    }

    // The registry declares five distinct transforms. If NO staged fixture can separate more than
    // three of them, the plan space the optimizer searches is effectively {baseline, one
    // permutation, one refinement} no matter what the seed table claims, and the four-grammar
    // evidence run has nothing to compare. That is the defect this census exists to catch.
    assert!(
        best.0 > 3,
        "no staged fixture reaches more than 3 distinct plans (best: {} at {}); the registry's \
         seven families have collapsed onto too few transforms again",
        best.0,
        best.1
    );
    eprintln!("best: {} distinct plans at {}", best.0, best.1);
    eprintln!(
        "best MARKER-FREE (the only rows where a candidate can be compared to the baseline at all): \
         {} distinct plans at {}",
        best_marker_free.0, best_marker_free.1
    );
}
