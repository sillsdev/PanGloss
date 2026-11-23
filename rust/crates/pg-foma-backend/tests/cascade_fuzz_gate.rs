//! Cascade fuzz gate self-checks: plain HC against plain HC, and injected defects that must fail it.

mod cascade_fuzz;

use cascade_fuzz::features::{CharKind, Feature, Refusal};
use cascade_fuzz::gate::{
    check_minimums, run, Admission, Candidate, GateConfig, PlainHc, Report, MIN_SEEDS_FOR_COVERAGE,
};
use cascade_fuzz::gen::{generate, AlphabetKind, Plan};
use cascade_fuzz::hc;
use cascade_fuzz::hc::{Outcome, ReadoutMode, Status};
use std::collections::BTreeSet;

#[test]
fn plain_hc_against_plain_hc_is_green() {
    let cfg = GateConfig::from_env();
    let report = run(&cfg, PlainHc::build);
    println!("{}", report.summary());
    assert!(
        report.failures.is_empty(),
        "{} failures, first 40:\n{}",
        report.failures.len(),
        report
            .failures
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn every_seed_renders_the_same_grammar_twice() {
    for seed in 0..60 {
        let plan = Plan::for_seed(seed);
        assert_eq!(generate(&plan).xml, generate(&plan).xml, "seed {seed}");
    }
}

#[test]
fn the_generator_can_draw_every_character_kind_and_the_grammar_loads() {
    let plan = Plan {
        seed: 1,
        features: [Feature::Suffix, Feature::Prefix].into_iter().collect(),
        refusal: None,
        alphabet: AlphabetKind::SpellerKinds,
        wide: false,
    };
    let generated = generate(&plan);
    assert_eq!(generated.kinds.len(), CharKind::ALL.len());
    let grammar = hc::load(&generated).unwrap_or_else(|e| panic!("{e}\n{}", generated.xml));
    let morpher = hc::morpher(&grammar, &generated.morpher);
    let words = hc::generated_words(&morpher, &grammar, &generated, 400);
    assert!(words.len() > 20, "only {} words", words.len());
}

#[test]
fn every_refused_construct_is_generated_and_loads_in_plain_hc() {
    for (i, refusal) in Refusal::ALL.into_iter().enumerate() {
        let plan = Plan::for_seed(4 + 5 * i as u64);
        assert_eq!(plan.refusal, Some(refusal));
        let generated = generate(&plan);
        hc::load(&generated).unwrap_or_else(|e| panic!("{refusal:?}: {e}\n{}", generated.xml));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Defect {
    None,
    DropAnalysis,
    InBoundIncomplete,
    RestrictedDropsAnalysis,
    RestrictedInventsAnalysis,
}

/// Plain HC with a defect injected, to prove the gate can fail.
struct Defective {
    inner: PlainHc,
    defect: Defect,
    refusal_text: Option<String>,
}

impl Candidate for Defective {
    fn admission(&self) -> Admission {
        match &self.refusal_text {
            Some(t) => Admission::Refused(t.clone()),
            None => Admission::NotApplicable,
        }
    }
    fn modes(&self) -> &[ReadoutMode] {
        self.inner.modes()
    }
    fn analyze(&self, word: &str, mode: ReadoutMode) -> Outcome {
        let mut out = self.inner.analyze(word, mode);
        match self.defect {
            Defect::DropAnalysis => {
                out.multiset.pop();
            }
            Defect::InBoundIncomplete => out.status = Status::Incomplete("injected".into()),
            _ => {}
        }
        out
    }
    fn restricted(
        &self,
        word: &str,
        roots: &BTreeSet<u32>,
        rules: &BTreeSet<u32>,
    ) -> Option<Outcome> {
        let mut out = self.inner.restricted(word, roots, rules)?;
        match self.defect {
            Defect::RestrictedDropsAnalysis => out.multiset.clear(),
            Defect::RestrictedInventsAnalysis => {
                if let Some(first) = out.multiset.first().cloned() {
                    let mut invented = first;
                    invented.identity.morphemes.push(Some("invented".into()));
                    out.multiset.push(invented);
                }
            }
            _ => {}
        }
        Some(out)
    }
}

fn small_config() -> GateConfig {
    GateConfig {
        start: 0,
        seeds: 12,
        ..GateConfig::from_env()
    }
}

fn defective(defect: Defect) -> Vec<String> {
    run(&small_config(), |g| {
        Ok(Defective {
            inner: PlainHc::build(g)?,
            defect,
            refusal_text: None,
        })
    })
    .failures
}

#[test]
fn a_candidate_that_loses_an_analysis_fails_the_gate() {
    let failures = defective(Defect::DropAnalysis);
    assert!(
        failures
            .iter()
            .any(|f| f.contains("replay: CASCADE_FUZZ_REPLAY=") && f.contains("analyses")),
        "{failures:?}"
    );
}

#[test]
fn an_incomplete_in_bound_analysed_word_fails_the_gate() {
    let failures = defective(Defect::InBoundIncomplete);
    assert!(
        failures
            .iter()
            .any(|f| f.contains("in-bound word incomplete")),
        "{failures:?}"
    );
}

#[test]
fn a_restricted_parse_that_loses_an_analysis_trips_l1() {
    let failures = defective(Defect::RestrictedDropsAnalysis);
    assert!(failures.iter().any(|f| f.contains("(L1)")), "{failures:?}");
}

#[test]
fn a_restricted_parse_that_invents_an_analysis_trips_l2() {
    let failures = defective(Defect::RestrictedInventsAnalysis);
    assert!(failures.iter().any(|f| f.contains("(L2)")), "{failures:?}");
}

#[test]
fn the_gate_without_a_defect_has_no_failures() {
    assert_eq!(defective(Defect::None), Vec::<String>::new());
}

#[test]
fn the_coverage_minimums_fail_when_a_switch_is_under_covered() {
    let cfg = GateConfig {
        seeds: MIN_SEEDS_FOR_COVERAGE,
        ..GateConfig::from_env()
    };
    let mut report = Report::default();
    check_minimums(&cfg, &mut report);
    assert!(
        report
            .failures
            .iter()
            .any(|f| f.contains("RealizationalRule") && f.contains("applied")),
        "no exemption may remain: {:?}",
        report.failures.len()
    );
    assert!(report
        .failures
        .iter()
        .any(|f| f.contains("in-bound grammars")));
    assert!(report
        .failures
        .iter()
        .any(|f| f.contains("character kinds")));
    let short = GateConfig { seeds: 12, ..cfg };
    let mut report = Report::default();
    check_minimums(&short, &mut report);
    assert!(report.failures.is_empty(), "short runs skip the minimums");
}

#[test]
fn a_refusal_must_name_the_construct() {
    let cfg = GateConfig {
        start: 4,
        seeds: 1,
        ..small_config()
    };
    let unnamed = run(&cfg, |g| {
        Ok(Defective {
            inner: PlainHc::build(g)?,
            defect: Defect::None,
            refusal_text: Some("unsupported".into()),
        })
    });
    assert_eq!(unnamed.failures.len(), 1, "{:?}", unnamed.failures);
    let named = run(&cfg, |g| {
        let text = g
            .refusal
            .expect("seed 4 is a refused construct")
            .named_construct()
            .to_string();
        Ok(Defective {
            inner: PlainHc::build(g)?,
            defect: Defect::None,
            refusal_text: Some(text),
        })
    });
    assert!(named.failures.is_empty(), "{:?}", named.failures);
}

#[test]
#[ignore = "waits for confirm_pin_groups (not yet in pg-foma-runtime): the cascade restricted parse is the candidate this runs against"]
fn t2_restricted_parse_through_the_cascade_confirm() {
    unimplemented!("wire the cascade candidate's `restricted` to confirm_pin_groups, then run `gate::run` with it");
}
