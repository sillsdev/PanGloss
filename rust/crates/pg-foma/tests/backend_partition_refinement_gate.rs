//! Pins that the backend registry offers more than one real plan shape, and that the extra shapes are semantics-preserving.
//! See `docs/research/pg-foma-recipe-registry-partition-refinement-notes.md` for why refinement was added and why this gate needs three separate assertions.

use pg_foma::backend_registry::{
    MaterializerContext, Registry, FAMILY_LAYERED_MORPHOLOGY, FAMILY_SPECIALIZED_BRANCH,
};
use pg_foma::build::build_controllable;
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::oracle::{differential_oracle, OracleResult};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::Grammar;

use foma::apply::apply_init;
use foma::options::FomaOptions;

/// Chosen because its `Gate` group has >= 3 entries (so `Bisect`/`FanOut` differ); its plan root also carries marker leaves, so this gate's equivalence claim covers only the controllable subtree, not the whole grammar.
const FIXTURE: &str = "backend-ordered-generic";

/// The two families wired onto `refine_gate_partition`. Each must own a surviving distinct plan.
const REFINEMENT_FAMILIES: [&str; 2] = [FAMILY_SPECIALIZED_BRANCH, FAMILY_LAYERED_MORPHOLOGY];

fn load() -> (Grammar, Vec<String>) {
    let fixtures = pg_conformance_fixtures::discover();
    let fixture = fixtures
        .iter()
        .find(|f| f.root == pg_conformance_fixtures::Root::Staging && f.name == FIXTURE)
        .unwrap_or_else(|| panic!("missing staged fixture {FIXTURE}"));
    let grammar = pg_grammar::load(&fixture.load_grammar_xml()).expect("fixture must load");
    let words = fixture
        .load_words_yaml()
        .words
        .iter()
        .map(|w| w.word.clone())
        .collect();
    (grammar, words)
}

/// Pins that this fixture carries both marker kinds, so the scope note on `FIXTURE` (equivalence covers only the controllable subtree) cannot silently go stale.
#[test]
fn the_scope_of_this_gate_is_stated_not_assumed() {
    let (grammar, _) = load();
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(&grammar);
    let baseline = enumerate_default(&grammar, &prules, phonology.as_ref());
    let markers = pg_foma::build::unbuildable_markers(&baseline);
    assert_eq!(
        markers.len(),
        2,
        "{FIXTURE} is documented as carrying BOTH marker kinds, which is why this gate's equivalence \
         claim is scoped to the controllable subtree; got {markers:?}"
    );
}

#[test]
fn the_registry_offers_more_than_three_distinct_plans_and_every_extra_one_is_equivalent() {
    let (grammar, words) = load();
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(&grammar);
    let baseline = enumerate_default(&grammar, &prules, phonology.as_ref());
    let baseline_root = baseline.root().expect("baseline plan has a root");

    let candidates = Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar: &grammar,
            baseline: &baseline,
        })
        .expect("materialization must succeed");

    let owners = candidates
        .iter()
        .map(|(instance, _)| instance.family_id.as_str())
        .collect::<Vec<_>>();

    // <= 3 distinct plans means the registry fell back to relabelled duplicates.
    assert!(
        candidates.len() > 3,
        "registry produced only {} distinct plan(s) for {FIXTURE}; the seven declared families have \
         collapsed onto too few transforms again (owners: {owners:?})",
        candidates.len(),
    );
    for family in REFINEMENT_FAMILIES {
        assert!(
            owners.contains(&family),
            "{family} owns no surviving distinct plan for {FIXTURE} -- its refinement either \
             degenerated to a no-op or coincided with the other granularity, so only one of the two \
             is really reachable (owners: {owners:?})"
        );
    }

    // Every non-baseline candidate must be both structurally distinct from, and relationally identical to, the baseline.
    let opts = FomaOptions::default();
    let word_refs: Vec<&str> = words.iter().map(String::as_str).collect();

    // NON-VACUITY: `differential_oracle`'s `Agree` also holds vacuously when both result sets are empty, so first confirm the baseline actually analyzes some of these words before trusting any `Agree` verdict below.
    let baseline_built = build_controllable(&baseline, &opts, &grammar, &alphabet, &prules)
        .expect("baseline plan must compile");
    let baseline_net = baseline_built
        .net
        .as_ref()
        .expect("baseline plan must yield a net, not None -- an absent net analyzes nothing");
    let analyzable = word_refs
        .iter()
        .filter(|word| {
            alphabet.encode_query(word).is_some_and(|query| {
                let mut handle = apply_init(baseline_net);
                handle.up(&query).next().is_some()
            })
        })
        .count();
    assert!(
        analyzable >= 2,
        "only {analyzable} of {FIXTURE}'s {} words analyze against the BASELINE net; with too few \
         analyzable words the equivalence checks below would agree on empty sets and prove nothing",
        word_refs.len()
    );

    let mut checked = 0usize;
    for (instance, candidate) in &candidates {
        let root = candidate.plan.root().expect("candidate plan has a root");
        if root == baseline_root {
            continue;
        }
        checked += 1;
        let verdict = differential_oracle(
            &baseline,
            &candidate.plan,
            ("baseline", &instance.family_id),
            &opts,
            &grammar,
            &alphabet,
            &prules,
            &word_refs,
        )
        .expect("differential oracle must complete on this fixture");
        assert!(
            matches!(verdict, OracleResult::Agree),
            "{}'s plan is not relation-equivalent to the baseline -- a backend transform must never \
             change what the network accepts: {verdict:?}",
            instance.family_id
        );
    }
    assert!(
        checked >= 2,
        "expected at least two non-baseline candidates to compare, checked {checked}"
    );
    eprintln!(
        "distinct plans: {} ({} non-baseline, all relation-equivalent)",
        candidates.len(),
        checked
    );
}
