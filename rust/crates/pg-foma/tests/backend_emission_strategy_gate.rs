//! Pins the backend registry's first candidate that varies the COMPILER rather than the plan shape: every other seeded family only rewrites the assembly tree, which minimization canonicalizes away, whereas `EmissionStrategy::TemplatedUnderlyingTokens` compiles to a genuinely different network; it claims only REACHABLE/DISTINCT/HONESTLY-EVALUATED, never "better," and needs its own dedup test since a whole-grammar strategy carries the baseline plan verbatim and a plan-root-only dedup key would silently drop it as a duplicate.

use pg_foma::backend_registry::{MaterializerContext, Registry, FAMILY_TOKEN_CASCADE_MORPHOLOGY};
use pg_foma::enumerate::{enumerate_default, EmissionStrategy};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::Grammar;

/// Carries phonological rules (so the strategy applies) and declares no boundary characters (so it also covers the boundary-free compile path below).
const FIXTURE: &str = "backend-gated-generic";

const FAMILY: &str = FAMILY_TOKEN_CASCADE_MORPHOLOGY;

fn load(name: &str) -> Grammar {
    let fixtures = pg_conformance_fixtures::discover();
    let fixture = fixtures
        .iter()
        .find(|f| f.root == pg_conformance_fixtures::Root::Staging && f.name == name)
        .unwrap_or_else(|| panic!("missing staged fixture {name}"));
    pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load")
}

fn baseline_plan(grammar: &Grammar) -> pg_foma::plan::Plan {
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(grammar);
    enumerate_default(grammar, &alphabet, &prules, phonology.as_ref())
}

#[test]
fn the_registry_does_not_dedup_this_candidate_away() {
    let grammar = load(FIXTURE);
    let baseline = baseline_plan(&grammar);
    let baseline_root = baseline.root().expect("baseline plan has a root");

    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");

    let (_, candidate) = candidates
        .iter()
        .find(|(instance, _)| instance.family_id == FAMILY)
        .unwrap_or_else(|| {
            panic!(
                "{FAMILY} owns no surviving candidate for {FIXTURE}; if the dedup key stopped \
                 including the emission strategy, this candidate is silently dropped as a duplicate \
                 of the baseline (owners: {:?})",
                candidates
                    .iter()
                    .map(|(i, _)| i.family_id.as_str())
                    .collect::<Vec<_>>()
            )
        });

    assert_eq!(
        candidate.strategy(),
        EmissionStrategy::TemplatedUnderlyingTokens,
        "{FAMILY} must request the token-cascade compiler; with any other strategy it is just \
         another relabelled copy of the baseline"
    );
    assert!(
        candidate.strategy().is_whole_grammar(),
        "this strategy compiles the whole grammar; if it reported otherwise its network would not be \
         comparable with the baseline's"
    );
    // The precondition that makes the dedup key load-bearing: this candidate's plan IS the baseline plan, asserted rather than relied on.
    assert_eq!(
        candidate.plan.root(),
        Some(baseline_root),
        "a whole-grammar strategy is expected to carry the baseline plan verbatim (its compiler \
         derives its own topology), which is exactly why the dedup key cannot be the plan root alone"
    );
}

/// Regression pin: a boundary-free grammar must compile through the token-cascade path; it did not, because the boundary-deletion regex joined one clause per boundary char-def and produced an EMPTY regex foma's `fsm_parse_regex` rejects, so skipping the pass entirely (the identity on a tape with no boundary tokens) is the fix.
#[test]
fn a_boundary_free_grammar_compiles_through_the_token_cascade_path() {
    let grammar = load(FIXTURE);
    let table = grammar
        .char_tables
        .first()
        .expect("fixture has a character table");
    let boundaries = table
        .iter()
        .filter(|(_, definition)| definition.kind() == pg_grammar::chardef::CharDefKind::Boundary)
        .count();
    assert_eq!(
        boundaries, 0,
        "{FIXTURE} is used here BECAUSE it declares no boundary characters; if it gains one this \
         test silently stops covering the empty-regex path it exists to pin"
    );

    let output = pg_foma::templated_compile::compile_templated_morphotactics(&grammar)
        .expect("a boundary-free grammar must compile, not fail on an empty cleanup regex");
    let (states, arcs) = output.proposer.network_counts();
    assert!(
        states > 0 && arcs > 0,
        "compiling must yield a real network, not an empty one that would analyze nothing: \
         {states} states / {arcs} arcs"
    );
}
