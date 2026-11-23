//! Cascade fuzz gate. The candidate is plain HC against plain HC until the cascade backend
//! exists; see `cascade_fuzz/mod.rs` for replay by seed.

mod cascade_fuzz;

use cascade_fuzz::features::{CharKind, Feature, Refusal};
use cascade_fuzz::gate::{run, Admission, Candidate, GateConfig, PlainHc};
use cascade_fuzz::gen::{generate, AlphabetKind, Plan};
use cascade_fuzz::hc;
use cascade_fuzz::hc::{Outcome, ReadoutMode};
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

/// Plain HC with a defect injected, to prove the gate can fail.
struct Defective {
    inner: PlainHc,
    drop_analysis: bool,
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
        if self.drop_analysis {
            out.multiset.pop();
        }
        out
    }
    fn restricted(
        &self,
        word: &str,
        roots: &BTreeSet<u32>,
        rules: &BTreeSet<u32>,
    ) -> Option<Outcome> {
        self.inner.restricted(word, roots, rules)
    }
}

fn small_config() -> GateConfig {
    GateConfig {
        start: 0,
        seeds: 12,
        ..GateConfig::from_env()
    }
}

#[test]
fn a_candidate_that_loses_an_analysis_fails_the_gate() {
    let cfg = small_config();
    let report = run(&cfg, |g| {
        Ok(Defective {
            inner: PlainHc::build(g)?,
            drop_analysis: true,
            refusal_text: None,
        })
    });
    assert!(
        report
            .failures
            .iter()
            .any(|f| f.contains("replay: CASCADE_FUZZ_REPLAY=")),
        "no replayable failure: {:?}",
        report.failures
    );
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
            drop_analysis: false,
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
            drop_analysis: false,
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
