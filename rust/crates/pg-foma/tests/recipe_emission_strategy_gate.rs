//! Pins the recipe registry's first candidate that varies the COMPILER rather than the plan shape.
//!
//! # Why this axis exists
//! Every other seeded family rewrites the assembly tree (`Gate`/`Union` order or partition
//! cardinality). Measured over eight marker-free synthetic fixtures with ten repetitions each, that
//! cannot change the compiled network: `states`, `arcs`, `proposals`, and `confirmation` come out
//! bit-identical across those candidates, because assembly ends in a minimization step that
//! canonicalizes the difference away. The only metric that moved was `build`, and only upward
//! (partition refinement: 2.1x-5.2x the baseline, non-overlapping ranges). So plan-shape variation
//! cannot express a better compilation, however many families are declared.
//!
//! `EmissionStrategy::TemplatedUnderlyingTokens` varies what the grammar is compiled TO: plain
//! char-def tokens plus a real compiled rewrite cascade, instead of phonology baked into the lexc by
//! `emit`'s surface probe with its expressive gaps patched by synthesized composite entries. That
//! changes what gets composed, not the order of composing it, so minimization cannot erase it.
//! Measured on `recipe-ordered-generic`: the baseline compiles to 79 states / 154 arcs, the
//! composed candidates to 30/49, and this strategy to 32/55 — three genuinely different networks.
//!
//! # What this gate does and does not claim
//! It claims the candidate is REACHABLE, DISTINCT, and HONESTLY EVALUATED. It does NOT claim the
//! strategy is better: on every synthetic fixture measured so far it fails to reproduce full-HC's
//! analysis multiset and reports `multiplicity-mismatch`. That is a real result rather than a
//! disappointing one — the point of the optimizer is to be able to ask, and before this strategy
//! existed the question could not be put, because every candidate on offer compiled to the same
//! network as the baseline.
//!
//! # The subtle part
//! A whole-grammar strategy carries the BASELINE plan, because its compiler derives its own topology
//! and never interprets one. So a dedup keyed on the plan root ALONE — which is what
//! `materialize_distinct` did before this axis existed — classifies it as a duplicate of the
//! baseline and silently drops it. `the_registry_does_not_dedup_this_candidate_away` below is the
//! test that catches that, and it is the assertion most likely to matter later: nothing about the
//! candidate looks wrong when it disappears, there is simply one fewer row in the report.

use pg_foma::enumerate::{enumerate_default, EmissionStrategy};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::Grammar;

/// Staged, tracked in this repository, carries phonological rules (so the strategy applies) and
/// declares no boundary characters (so it also covers the boundary-free compile path below).
const FIXTURE: &str = "recipe-gated-generic";

const FAMILY: &str = "token-cascade-morphology";

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
        candidate.strategy,
        EmissionStrategy::TemplatedUnderlyingTokens,
        "{FAMILY} must request the token-cascade compiler; with any other strategy it is just \
         another relabelled copy of the baseline"
    );
    assert!(
        candidate.strategy.is_whole_grammar(),
        "this strategy compiles the whole grammar; if it reported otherwise its network would not be \
         comparable with the baseline's"
    );
    // The precondition that makes the dedup key load-bearing: this candidate's plan IS the baseline
    // plan. If that ever stops being true the test above would pass for the wrong reason, so assert
    // it rather than rely on it.
    assert_eq!(
        candidate.plan.root(),
        Some(baseline_root),
        "a whole-grammar strategy is expected to carry the baseline plan verbatim (its compiler \
         derives its own topology), which is exactly why the dedup key cannot be the plan root alone"
    );
}

/// Regression pin: a grammar that declares NO boundary characters must compile through the
/// token-cascade path.
///
/// It did not. `compile_templated_morphotactics` built its boundary-deletion regex by joining one
/// `token -> 0` clause per boundary char-def, so a boundary-free grammar produced the EMPTY regex,
/// which `fsm_parse_regex` rejects — and the whole compile failed with `CleanupCompileFailed("")`.
/// Measured: two synthetic conformance fixtures were unbuildable for this reason alone. The path had
/// never been run against them; its only callers were the P6 gate and its own tests, all on grammars
/// that do declare boundaries. Deleting nothing from a tape that has no boundary tokens on it is the
/// identity, so skipping the pass is the correct semantics rather than a workaround.
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
