//! The cascade backend against plain HC over generated grammars: the W5 candidate is
//! `CascadeAnalyzer` followed by `confirm_pin_groups`. Replay a seed with
//! `CASCADE_FUZZ_REPLAY=<seed> cargo test -p pg-foma-backend --test cascade_w5_fuzz -- --nocapture`.

mod cascade_fuzz;

use std::collections::BTreeSet;

use cascade_fuzz::features::Feature;
use cascade_fuzz::gate::{run, Admission, Candidate, GateConfig, Report};
use cascade_fuzz::gen::Generated;
use cascade_fuzz::hc::{self, Outcome, ReadoutMode, Status};
use pg_foma::cascade_intersect::{build_cascade, check_all, Accounting};
use pg_foma::differential::multiset;
use pg_foma_runtime::cascade::analyzer::{CascadeAnalyzer, CascadeStatus};
use pg_foma_runtime::cascade::BuildMeter;
use pg_foma_runtime::confirm::{confirm_pin_groups, PinGroup};
use pg_grammar::model::{Grammar, LexEntryId, MRuleId};
use pg_parse::Morpher;

const NAMED: [&str; 8] = [
    "families with a blockable rule",
    "supplied roots",
    "several character tables",
    "stem-editing compounding",
    "metathesis with a middle node",
    "overlapping simultaneous multi-token focus",
    "class Modify without feature lanes",
    "unbounded truncation",
];

struct CascadeCandidate {
    grammar: &'static Grammar,
    morpher: Morpher<'static>,
    admission: Admission,
    analyzer: Option<CascadeAnalyzer>,
    accounting: Option<Accounting>,
}

impl CascadeCandidate {
    fn build(generated: &Generated) -> Result<Self, String> {
        let grammar: &'static Grammar = Box::leak(Box::new(hc::load(generated)?));
        let morpher = hc::morpher(grammar, &generated.morpher);
        let refusals = check_all(grammar, generated.supplied_roots);
        let (admission, analyzer, accounting) = if refusals.is_empty() {
            let mut meter = BuildMeter::default_limits();
            match build_cascade(grammar, generated.supplied_roots, &mut meter) {
                Ok((layers, accounting)) => (
                    Admission::Admitted,
                    Some(CascadeAnalyzer::new(layers)),
                    Some(accounting),
                ),
                Err(e) => return Err(e.text()),
            }
        } else {
            let text = refusals
                .iter()
                .map(|r| r.text())
                .collect::<Vec<_>>()
                .join(" | ");
            if NAMED.iter().any(|n| text.contains(n)) {
                (Admission::Refused(text), None, None)
            } else {
                (Admission::NotModelled(text), None, None)
            }
        };
        Ok(CascadeCandidate {
            grammar,
            morpher,
            admission,
            analyzer,
            accounting,
        })
    }
}

fn outcome_of(grammar: &Grammar, status: Status, analyses: &[pg_parse::WordAnalysis]) -> Outcome {
    Outcome {
        status,
        multiset: multiset(analyses, grammar).unwrap_or_default(),
    }
}

impl Candidate for CascadeCandidate {
    fn admission(&self) -> Admission {
        self.admission.clone()
    }

    fn modes(&self) -> &[ReadoutMode] {
        &[ReadoutMode::Single]
    }

    fn analyze(&self, word: &str, _mode: ReadoutMode) -> Outcome {
        let analyzer = self.analyzer.as_ref().expect("admitted grammars analyse");
        let result = analyzer.analyze(self.grammar, &self.morpher, word);
        let status = match result.status {
            CascadeStatus::Complete => Status::Complete,
            CascadeStatus::InvalidShape => Status::InvalidShape,
            CascadeStatus::Incomplete(why) => Status::Incomplete(why),
        };
        let analyses: Vec<_> = result.analyses.into_iter().map(|(a, _, _)| a).collect();
        outcome_of(self.grammar, status, &analyses)
    }

    fn restricted(
        &self,
        word: &str,
        roots: &BTreeSet<u32>,
        rules: &BTreeSet<u32>,
    ) -> Option<Outcome> {
        let group = PinGroup {
            roots: roots.iter().map(|&r| LexEntryId(r)).collect(),
            rules: rules.iter().map(|&r| MRuleId(r)).collect(),
        };
        let budget = self.morpher.word_budget();
        let confirmed = confirm_pin_groups(self.grammar, &self.morpher, word, &[group], &budget);
        let status = if confirmed.incomplete() {
            Status::Incomplete("confirmation".into())
        } else {
            Status::Complete
        };
        let analyses: Vec<_> = confirmed.analyses.into_iter().map(|(a, _, _)| a).collect();
        Some(outcome_of(self.grammar, status, &analyses))
    }
}

fn assert_green(report: &Report) {
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

/// The switches W5 builds; each must be exercised by grammars and by words HC applied it on.
const W5_FEATURES: [Feature; 13] = [
    Feature::Prefix,
    Feature::Suffix,
    Feature::Circumfix,
    Feature::NullAffix,
    Feature::TemplateOptionalSlot,
    Feature::TemplateRequiredSlot,
    Feature::RealizationalRule,
    Feature::RealizationalInRuleList,
    Feature::LinearStratum,
    Feature::UnorderedStratum,
    Feature::MultipleStrata,
    Feature::MaxAppsTwo,
    Feature::TwoNullAffixesOneZone,
];

#[test]
fn the_cascade_agrees_with_plain_hc_on_the_w5_feature_set() {
    let mut cfg = GateConfig::from_env();
    let full = cfg.seeds >= 250;
    cfg.min_inbound_grammars_per_feature = 0;
    cfg.min_applied_words_per_feature = 0;
    let report = run(&cfg, CascadeCandidate::build);
    assert_green(&report);
    assert!(report.t2_checked > 0 || cfg.seeds < 20, "no T2 word ran");
    if full {
        for f in W5_FEATURES {
            let c = report.per_feature.get(&f).cloned().unwrap_or_default();
            assert!(
                c.inbound_grammars + c.wide_grammars >= 1 && c.applied_words >= 3,
                "{f:?}: {c:?}"
            );
        }
        assert!(report.inbound > 0 && report.words_with_analysis > 0);
    }
}

#[test]
fn every_admitted_grammar_has_a_complete_accounting() {
    let cfg = GateConfig {
        seeds: 120,
        ..GateConfig::from_env()
    };
    let mut admitted = 0;
    for seed in cfg.start..cfg.start + cfg.seeds {
        let generated = cascade_fuzz::gen::generate(&cascade_fuzz::gen::Plan::for_seed(seed));
        let c = CascadeCandidate::build(&generated).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        if let Some(a) = &c.accounting {
            admitted += 1;
            assert!(a.layers >= 3 && !a.zones.is_empty(), "seed {seed}");
        }
    }
    assert!(admitted > 0);
}
