//! Census of how many DISTINCT plans the seeded recipe registry actually reaches per staged conformance fixture: applicability predicates and content-address dedup can collapse the declared families onto far fewer transforms than claimed, and only running the registry against every fixture reveals which ones can tell them apart.

use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::replace::SegAlphabet;

#[test]
fn some_staged_fixture_separates_more_than_three_registry_transforms() {
    let fixtures = pg_conformance_fixtures::discover();
    assert!(!fixtures.is_empty(), "no staged fixtures discovered");

    let mut best = (0usize, String::new());
    // Tracked separately: a marker-carrying grammar's `PlanComposed` candidate can fall back to the tuned `emit` path, so only a marker-free row guarantees the network measured is the one the candidate names.
    let mut best_marker_free = (0usize, String::new());
    // `markers` matters as much as `distinct`: on a marker-carrying plan, `build_controllable` compiles only the controllable subtree, so every candidate net excludes what those subtrees contribute, and a whole-grammar comparison there needs `realized_strategy` too.
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

    // If no staged fixture separates more than three of the registry's five declared transforms, the searched plan space is effectively {baseline, one permutation, one refinement} regardless of what the seed table claims.
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
