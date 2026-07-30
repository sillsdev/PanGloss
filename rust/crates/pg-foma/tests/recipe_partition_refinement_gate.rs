//! Pins that the recipe registry offers more than one real plan shape — and that the extra shapes
//! are semantics-preserving.
//!
//! # What was wrong
//! `recipe_registry`'s `SEEDS` declared seven families but only three `SafeTransform` values, and
//! `impl Materializer for SeededFamily` dispatches on the TRANSFORM alone (family identity and
//! parameters are ignored; each family's `topology` parameter has a one-value domain). Two families
//! were `Identity`, i.e. byte-identical to the baseline, and four shared one `UnionPermutation`.
//! `materialize_distinct` then content-address-dedups the collisions, so the reachable space was at
//! most {baseline, gate-permutation, union-permutation}. Measured consequence: the reference
//! template-less grammar yielded 3 candidates with IDENTICAL 209 states / 484 arcs, and the
//! large-lexicon grammar yielded exactly 1 (`static_count: 1`, 4 deduped away). The optimizer could
//! demonstrate correctness but never a comparison, because there was nothing distinct to compare.
//!
//! # What changed, and why it is safe
//! Two families now use `oracle::refine_gate_partition`, which already existed, was already argued
//! sound, and was simply never wired in — its own doc even anticipates
//! "`recipe_registry`'s own `HasSplittableGateGroup`", an applicability that did not exist until now.
//! Refinement changes a `Gate` node's partition CARDINALITY rather than its order, which is a
//! genuinely different axis from the two permutations. It is safe because composition distributes
//! over union — `(A ∪ B) .o. R == (A .o. R) ∪ (B .o. R)` — so splitting a group's entries while
//! keeping that group's OWN unchanged `Replace` node, then re-unioning, reproduces the original net.
//!
//! # Why this gate is not vacuous
//! Three assertions, and no one of them would be enough:
//!  * DISTINCTNESS — the refined plans must have different root content addresses from the baseline,
//!    or `materialize_distinct` dedups them and nothing was added. A count-only assertion would pass
//!    on a registry that merely relabelled duplicates, which is exactly the defect being fixed.
//!  * BOTH GRANULARITIES — the two refinement families must EACH own a surviving distinct plan.
//!    Counting alone is too weak here: `Bisect` and `FanOut` coincide on any partition whose groups
//!    hold at most two entries (`chunk_sizes(2)` is `[1, 1]` for both), so on such a fixture one of
//!    them silently dedups away and a bare `len() > 3` could still be satisfied by an unrelated
//!    union permutation. That is not hypothetical: it is what the first fixture tried here did.
//!  * EQUIVALENCE — each non-baseline plan must agree with the baseline under
//!    `oracle::differential_oracle`, which runs real query words through BOTH compiled nets'
//!    `apply_up` and compares result sets. That is this repo's established predicate for plan
//!    equality (`build.rs`'s own `equivalence_tests` uses it) precisely because two nets can differ
//!    in shape and still be the same relation, so structural comparison would prove the wrong thing.
//!    A distinctness-only assertion would happily accept a transform that changed the language.
//!
//! `recipe_registry_census.rs` covers the complementary, fixture-independent question — that SOME
//! staged fixture separates more than three transforms at all — and prints the per-fixture table
//! this fixture choice was made from.

use pg_foma::build::build_controllable;
use pg_foma::compose_budget::ComposeBudget;
use pg_foma::enumerate::enumerate_default;
use pg_foma::junctions::PhonologyProbe;
use pg_foma::oracle::{differential_oracle, OracleResult};
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::Grammar;

use foma::apply::apply_init;
use foma::options::FomaOptions;

/// Multi-stratum grammar with nine lexical entries. Chosen from `recipe_registry_census.rs`'s table
/// because it is the fixture tracked in THIS repository (not the `machine/` submodule) that holds a
/// `Gate` group of three or more entries, so `Bisect` and `FanOut` land on genuinely different
/// partitions rather than both producing `[1, 1]`.
///
/// SCOPE, measured rather than assumed: this fixture's plan root is a `Union` of the `Gate` node
/// PLUS both `CompositeEmissionMarker` and `StructuralCompositeMarker` leaves, so every net compared
/// below is `build_controllable`'s controllable-only net and omits what those subtrees contribute.
/// The equivalence this gate establishes is therefore "these plans denote the same relation over the
/// controllable subtree", which is the right and sufficient claim for a transform that only ever
/// restructures `Gate`/`Union` nodes — but it is NOT "these plans compile the same whole grammar".
/// `unbuildable_markers`' own doc requires any caller reading the controllable net as the whole
/// grammar to consult it first; `the_scope_of_this_gate_is_stated_not_assumed` below does, so this
/// limitation cannot silently stop being true. An earlier version of this comment claimed the
/// fixture was marker-free because it declares no `<AffixTemplate>`; markers also come from
/// composite entries and circumfix/dropped-material rules, which this fixture does have.
const FIXTURE: &str = "recipe-ordered-generic";

/// The two families wired onto `refine_gate_partition`. Each must own a surviving distinct plan.
const REFINEMENT_FAMILIES: [&str; 2] = ["specialized-branch", "layered-morphology"];

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

/// Pins WHICH net the equivalence check above is talking about, so the scope note on `FIXTURE`
/// cannot quietly become false. If this fixture ever loses its marker leaves the assertion fires and
/// the note gets strengthened; if a future fixture swap silently introduces them where the note says
/// there are none, likewise. Either way the claim and the code stay in step.
#[test]
fn the_scope_of_this_gate_is_stated_not_assumed() {
    let (grammar, _) = load();
    let alphabet = SegAlphabet::new(&grammar.char_tables[0]);
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phonology = PhonologyProbe::new(&grammar);
    let baseline = enumerate_default(&grammar, &alphabet, &prules, phonology.as_ref());
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
    let baseline = enumerate_default(&grammar, &alphabet, &prules, phonology.as_ref());
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

    // Before the refinement transforms were wired in, this fixture produced exactly 3 distinct
    // plans. Anything <= 3 means the registry has fallen back to relabelled duplicates.
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

    // Every candidate that is NOT the baseline must be both structurally distinct from it and
    // relationally identical to it.
    let opts = FomaOptions::default();
    // Unbounded: this fixture is tiny, and a budget trip here would surface as an Err rather than a
    // disagreement, which would silently weaken the equivalence claim below.
    let budget = ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    );
    let word_refs: Vec<&str> = words.iter().map(String::as_str).collect();

    // NON-VACUITY. `differential_oracle` returns `Agree` whenever the two result sets are equal for
    // every word -- including when both are EMPTY, which is what `apply_up_results` yields for a
    // `None` net or a word that fails to encode. So "Agree" alone proves nothing: a gate whose
    // fixture analyzes zero words would confirm every transform, correct or not. (This project has
    // already shipped exactly that failure once, in `certify_corpus`.) Establish first, against the
    // baseline plan only, that these words really do produce analyses.
    let baseline_built = build_controllable(
        &baseline,
        &opts,
        &grammar,
        &alphabet,
        &prules,
        &ComposeBudget::with_caps(
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
            usize::MAX,
            None,
        ),
    )
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
            &budget,
            &word_refs,
        )
        .expect("differential oracle must complete on this fixture");
        assert!(
            matches!(verdict, OracleResult::Agree),
            "{}'s plan is not relation-equivalent to the baseline -- a recipe transform must never \
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
