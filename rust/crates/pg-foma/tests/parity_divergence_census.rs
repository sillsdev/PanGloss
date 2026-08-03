//! **Settles the one soundness hazard the confirmation-free accuracy path rests on.**
//!
//! `pg_foma::recipe_accuracy` detects undergeneration by checking that a candidate proposed the
//! admission key of every oracle analysis, performing NO full-HC confirmation. That is a sound test
//! for undergeneration on its own. It is equivalent to full certification only if the OTHER
//! direction is free — if a candidate's confirmed identity set can never contain an identity the
//! oracle's set lacks.
//!
//! The argument for that direction is strong: the candidate's confirm is a RESTRICTED
//! `Morpher::parse_word_selected` while the oracle is the same engine unrestricted, so the candidate
//! explores a subset of the search space. It is not airtight, and the gap is narrow and specific:
//! `pg_rules::word::WordKey` — the analysis-search dedup key — deliberately EXCLUDES the syntactic
//! feature struct, while `pg_parse::identity::AnalysisIdentity::category` is projected from it via
//! `WordAnalysis::pos_id`. Two search states differing only in `syn_fs` therefore collapse to one
//! map entry, and which one survives is decided first-wins by traversal order — which the
//! restriction perturbs. So a restricted run could in principle surface a category the unrestricted
//! run deduplicated away: a CANDIDATE-ONLY IDENTITY.
//!
//! That was inference, never an observation. This file measures it, because building containment on
//! an unmeasured assumption would silently certify a wrong answer on the day it stopped holding.
//!
//! # What is measured, and what a zero here does and does not license
//!
//! `pg_foma::parity::IdentityDivergence::candidate_only_identities`, counted on the ORDINARY
//! certification path (inside `certify_corpus` itself, sharing its one projection pass — not a
//! second reimplementation that could disagree with the verdict about what it looked at) and
//! accumulated per run by `RunEvaluationCache`.
//!
//! A zero licenses exactly one claim: on these fixtures, at these corpora, confirmation never
//! yielded an identity the oracle lacked, so undergeneration is the only way certification can fail
//! and the containment check detects it. It does NOT license removing confirmation from the
//! certification path, and it does not make the accuracy verdict a certification. It makes the
//! accuracy verdict a trustworthy fast SCREEN.
//!
//! A non-zero is a finding, not a nuisance: it would mean the parity relation and the compilation
//! disagree about analysis identity somewhere, which is worth more than any speedup.
//!
//! # Why "compared nothing" is asserted too
//!
//! `occurrences_compared` is asserted non-zero, and
//! `IdentityDivergence::supports_free_containment` encodes the same rule in the type. A run that was
//! refused (a step-capped oracle occurrence, a build failure) reports zero candidate-only identities
//! because it compared nothing at all, and this repository's standing rule is that "I could not look"
//! must never read as "everything is fine".

use pg_conformance_fixtures::{discover, FixtureRef, Root};
use pg_foma::enumerate::{enumerate_default, CandidatePlan, EmissionStrategy};
use pg_foma::junctions::PhonologyProbe;
use pg_foma::parity::IdentityDivergence;
use pg_foma::recipe_registry::{MaterializerContext, Registry};
use pg_foma::recipe_runtime::{evaluate_plans_marked_with_cache, RunEvaluationCache, RuntimeBudget};
use pg_foma::replace::SegAlphabet;
use pg_grammar::model::{Grammar, PhonRuleDef};

/// How many corpus occurrences of each fixture the census checks.
///
/// A word subset is legitimate here and a proposal-set subset never would be: each corpus row is an
/// independent observation of the divergence, so measuring 8 of them measures 8 real observations,
/// whereas truncating a word's proposal set would fabricate a recall failure. Bounded so the census
/// spans every fixture at a cost the ordinary test suite can carry on every run — an unbounded sweep
/// over every word of every fixture is the corpus battery, which belongs to `-Mode corpus-test`.
const OCCURRENCES_PER_FIXTURE: usize = 8;

/// Fixtures the FULL registry candidate set is measured on.
///
/// Chosen for the hazard rather than for coverage: the divergence is about how a RESTRICTION
/// perturbs `WordKey` dedup order, so what matters is grammars where one admission key can plausibly
/// carry more than one category, or where several genuinely different restrictions exist.
/// `template-category-sharing` shares categories across templates by construction;
/// `head-ambiguous-compounding` has two readings of one surface form differing only in headedness;
/// `recipe-gated-generic` is the fixture the run-cache gate already uses and materializes several
/// distinct candidates including both whole-grammar compilers.
const REGISTRY_CENSUS_FIXTURES: [&str; 3] = [
    "template-category-sharing",
    "head-ambiguous-compounding",
    "recipe-gated-generic",
];

/// `pg_foma::emit::surface_table`, which is `pub(crate)`: the surface char-def table is the LAST
/// stratum's, not `char_tables[0]`, and on a multi-stratum grammar those differ. Replicated rather
/// than approximated so the census builds its candidates over the same alphabet the evaluator does.
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

fn baseline_only(grammar: &Grammar) -> Vec<CandidatePlan> {
    vec![CandidatePlan {
        label: "divergence-census-baseline",
        plan: baseline_plan(grammar),
        strategy: EmissionStrategy::PlanComposed,
    }]
}

fn registry_plans(grammar: &Grammar) -> Vec<CandidatePlan> {
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
    select_plans: impl Fn(&Grammar) -> Vec<CandidatePlan>,
) -> (Vec<FixtureDivergence>, Vec<String>) {
    let mut measured = Vec::new();
    let mut skipped = Vec::new();
    for fixture in discover().into_iter().filter(include) {
        let Ok(grammar) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            skipped.push(format!("{}: grammar failed to load", fixture.label()));
            continue;
        };
        let words: Vec<String> = fixture
            .load_words_yaml()
            .words
            .into_iter()
            .map(|word| word.word)
            .take(OCCURRENCES_PER_FIXTURE)
            .collect();
        if words.is_empty() {
            skipped.push(format!("{}: no corpus words", fixture.label()));
            continue;
        }
        let plans = select_plans(&grammar);
        if plans.is_empty() {
            skipped.push(format!("{}: no candidate materialized", fixture.label()));
            continue;
        }
        let is_baseline: Vec<bool> = (0..plans.len()).map(|index| index == 0).collect();
        let Ok(mut cache) = RunEvaluationCache::prepare(&grammar, &words, RuntimeBudget::default())
        else {
            // An oracle preparation fault is a whole-run abort, not a per-word outcome. Recorded as
            // "could not look", never folded into the measurement.
            skipped.push(format!("{}: oracle preparation faulted", fixture.label()));
            continue;
        };
        evaluate_plans_marked_with_cache(
            &grammar,
            &plans,
            &words,
            RuntimeBudget::default(),
            &is_baseline,
            &mut cache,
        );
        measured.push(FixtureDivergence {
            label: fixture.label(),
            divergence: cache.identity_divergence(),
        });
    }
    (measured, skipped)
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

/// The census: EVERY discoverable fixture, one candidate each — the baseline, i.e. the default
/// compilation of that grammar, which is what a regression screen would actually be run on.
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

/// The same measurement with the FULL registry candidate set, on the hazard-bearing fixtures.
///
/// Worth having separately because the hazard is about how the RESTRICTION perturbs dedup order, and
/// different candidates restrict differently — one candidate per fixture exercises one restriction
/// shape per grammar. This exercises several per grammar, including both whole-grammar compilers.
/// Bounded to [`REGISTRY_CENSUS_FIXTURES`] rather than swept over everything: a full
/// registry-by-fixture cross product IS the corpus battery, and this must stay runnable on every
/// change.
#[test]
fn no_registry_candidate_produces_a_candidate_only_identity() {
    let (measured, skipped) = census(
        |fixture| {
            fixture.root == Root::Staging && REGISTRY_CENSUS_FIXTURES.contains(&fixture.name.as_str())
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
