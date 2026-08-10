//! Measures the one soundness hazard the confirmation-free accuracy path in `pg_foma::backend_accuracy`
//! rests on; see `docs/research/pg-foma-parity-divergence-census-design-notes.md` for the argument.

use pg_conformance_fixtures::{discover, FixtureRef, Root};
use pg_foma::backend_registry::{MaterializerContext, Registry};
use pg_foma::backend_runtime::{evaluate_plans_with_cache, RunEvaluationCache, RuntimeBudget};
use pg_foma::enumerate::{enumerate_default, CandidateRole, LoweredCandidate};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::lowering_adapter::LoweringAdapter;
use pg_foma::parity::IdentityDivergence;
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};

/// A word subset is fine here since each corpus row is an independent observation; a proposal-set subset would fabricate a recall failure instead.
const OCCURRENCES_PER_FIXTURE: usize = 8;

/// Chosen for the hazard, not coverage: grammars where one admission key can plausibly carry more than one category, or where several genuinely different restrictions exist.
const REGISTRY_CENSUS_FIXTURES: [&str; 3] = [
    "template-category-sharing",
    "head-ambiguous-compounding",
    "backend-gated-generic",
];

/// Replicates the pub(crate) `pg_foma::emit::surface_table`: the surface table is the last stratum's, not `char_tables[0]`.
fn surface_table(grammar: &Grammar) -> &pg_grammar::chardef::CharDefTable {
    let surface_stratum = grammar
        .strata
        .last()
        .expect("a loaded grammar always has at least one stratum");
    &grammar.char_tables[surface_stratum.table.0 as usize]
}

fn baseline_plan(grammar: &Grammar) -> pg_foma::plan::Plan {
    let alphabet = SegAlphabet::new(surface_table(grammar));
    let prules: Vec<&PhonRuleDef> = pg_foma::enumerate::prules_in_order(grammar);
    let phonology = PhonologyProbe::new(grammar);
    enumerate_default(grammar, &alphabet, &prules, phonology.as_ref())
}

fn baseline_only(grammar: &Grammar) -> Vec<LoweredCandidate> {
    vec![LoweredCandidate {
        label: "divergence-census-baseline",
        plan: baseline_plan(grammar),
        adapter: LoweringAdapter::ControllablePlanCompose,
        role: CandidateRole::Baseline,
    }]
}

fn registry_plans(grammar: &Grammar) -> Vec<LoweredCandidate> {
    let baseline = baseline_plan(grammar);
    Registry::seeded()
        .materialize_distinct(&MaterializerContext {
            grammar,
            baseline: &baseline,
        })
        .map(|candidates| candidates.into_iter().map(|(_, plan)| plan).collect())
        .unwrap_or_default()
}

struct FixtureDivergence {
    label: String,
    divergence: IdentityDivergence,
}

fn census(
    include: impl Fn(&FixtureRef) -> bool,
    select_plans: impl Fn(&Grammar) -> Vec<LoweredCandidate>,
) -> (Vec<FixtureDivergence>, Vec<String>) {
    let mut measured = Vec::new();
    let mut skipped = Vec::new();
    for fixture in discover().into_iter().filter(include) {
        // Caught, not fixed here: a panicking fixture is a real compiler defect but must not erase every other fixture's evidence.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            measure_one_fixture(&fixture, &select_plans)
        }));
        match outcome {
            Ok(Ok(divergence)) => measured.push(divergence),
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
    (measured, skipped)
}

/// One fixture's measurement; `Err` is a human-readable skip reason, and the caller turns a panic into one too.
fn measure_one_fixture(
    fixture: &FixtureRef,
    select_plans: &impl Fn(&Grammar) -> Vec<LoweredCandidate>,
) -> Result<FixtureDivergence, String> {
    // Logged before any work, so a process abort's last output line still names the culprit fixture.
    eprintln!("census: entering {}", fixture.label());
    let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
        return Err(format!("{}: grammar failed to load", fixture.label()));
    };
    let words: Vec<String> = fixture
        .load_words_yaml()
        .words
        .into_iter()
        .map(|word| word.word)
        .take(OCCURRENCES_PER_FIXTURE)
        .collect();
    if words.is_empty() {
        return Err(format!("{}: no corpus words", fixture.label()));
    }
    let plans = select_plans(&grammar);
    if plans.is_empty() {
        return Err(format!("{}: no candidate materialized", fixture.label()));
    }
    let Ok(mut cache) = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
    else {
        // An oracle preparation fault is a whole-run abort, recorded as "could not look", never a measurement.
        return Err(format!("{}: oracle preparation faulted", fixture.label()));
    };
    evaluate_plans_with_cache(
        &grammar,
        &plans,
        &words,
        RuntimeBudget::default(),
        &mut cache,
    );
    Ok(FixtureDivergence {
        label: fixture.label(),
        divergence: cache.identity_divergence(),
    })
}

fn report(label: &str, measured: &[FixtureDivergence], skipped: &[String]) {
    eprintln!("--- {label} ---");
    for fixture in measured {
        let d = &fixture.divergence;
        eprintln!(
            "{}: compared={} not_compared={} oracle_ids={} candidate_ids={} oracle_only={} \
             CANDIDATE_ONLY={} occurrences_with_candidate_only={} oracle_key_collisions={} \
             candidate_key_collisions={}",
            fixture.label,
            d.occurrences_compared,
            d.occurrences_not_compared,
            d.oracle_identities,
            d.candidate_identities,
            d.oracle_only_identities,
            d.candidate_only_identities,
            d.occurrences_with_candidate_only,
            d.oracle_admission_key_collisions,
            d.candidate_admission_key_collisions,
        );
    }
    for skip in skipped {
        eprintln!("skipped {skip}");
    }
    let compared: u64 = measured
        .iter()
        .map(|fixture| fixture.divergence.occurrences_compared)
        .sum();
    let candidate_only: u64 = measured
        .iter()
        .map(|fixture| fixture.divergence.candidate_only_identities)
        .sum();
    eprintln!(
        "{label} TOTAL: {} fixtures measured, {} skipped, {compared} occurrences compared, \
         {candidate_only} candidate-only identities",
        measured.len(),
        skipped.len()
    );
}

fn assert_no_candidate_only_identity(label: &str, measured: &[FixtureDivergence]) {
    let offenders: Vec<&FixtureDivergence> = measured
        .iter()
        .filter(|fixture| fixture.divergence.candidate_only_identities > 0)
        .collect();
    assert!(
        offenders.is_empty(),
        "{label}: a candidate produced identities the oracle's set does not contain. This \
         INVALIDATES the free-containment argument the confirmation-free accuracy path rests on \
         (see this file's module doc) -- do not reason past it, and do not weaken the accuracy \
         verdict to accommodate it; the witness IS the finding. Offenders: {}",
        offenders
            .iter()
            .map(|fixture| format!(
                "{}: {} candidate-only identities over {} compared occurrences",
                fixture.label,
                fixture.divergence.candidate_only_identities,
                fixture.divergence.occurrences_compared
            ))
            .collect::<Vec<_>>()
            .join("; ")
    );
    let compared: u64 = measured
        .iter()
        .map(|fixture| fixture.divergence.occurrences_compared)
        .sum();
    assert!(
        compared > 0,
        "{label}: zero occurrences were compared, so a zero candidate-only count is the absence of \
         evidence rather than evidence of absence"
    );
    assert!(
        measured
            .iter()
            .any(|fixture| fixture.divergence.supports_free_containment()),
        "{label}: no fixture produced a positively clean comparison -- every one of them either \
         compared nothing or diverged"
    );
}

/// Every discoverable fixture, one candidate each: the default compilation a regression screen would actually run on.
#[test]
fn no_fixture_produces_a_candidate_only_identity() {
    let (measured, skipped) = census(|_| true, baseline_only);
    report(
        "baseline census over every discoverable fixture",
        &measured,
        &skipped,
    );
    assert!(
        measured.len() >= 10,
        "the census must actually span the fixture corpus; measured only {} fixtures ({} skipped) \
         -- a census that measures almost nothing proves almost nothing",
        measured.len(),
        skipped.len()
    );
    assert_no_candidate_only_identity("baseline census", &measured);
}

/// The same measurement with the full registry candidate set, restricted to the hazard-bearing fixtures.
#[test]
fn no_registry_candidate_produces_a_candidate_only_identity() {
    let (measured, skipped) = census(
        |fixture| {
            fixture.root == Root::Staging
                && REGISTRY_CENSUS_FIXTURES.contains(&fixture.name.as_str())
        },
        registry_plans,
    );
    report("full registry census", &measured, &skipped);
    assert_eq!(
        measured.len(),
        REGISTRY_CENSUS_FIXTURES.len(),
        "every pinned registry-census fixture must be measured, not skipped: skipped={skipped:?}"
    );
    assert_no_candidate_only_identity("registry census", &measured);
}
