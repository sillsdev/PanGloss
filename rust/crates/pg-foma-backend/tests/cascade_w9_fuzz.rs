//! W9: interior rules in both schedules, the cascade analyzer against plain HC over generated grammars.

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

const NAMED: [&str; 9] = [
    "families with a blockable rule",
    "supplied roots",
    "several character tables",
    "stem-editing compounding",
    "metathesis with a middle node",
    "overlapping simultaneous multi-token focus",
    "class Modify without feature lanes",
    "unbounded truncation",
    "root natural-class node",
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
                if generated.features.iter().all(|f| W9_FEATURES.contains(f)) {
                    return Err(format!(
                        "not modelled ({text}) but every feature is in the W9 set: {:?}",
                        generated.features
                    ));
                }
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
            CascadeStatus::Incomplete(why) | CascadeStatus::Refused(why) => Status::Incomplete(why),
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

/// The switches W5 and W9 build; each must be exercised by grammars and by words HC applied it on.
const W9_FEATURES: [Feature; 16] = [
    Feature::Infix,
    Feature::BoundedTruncation,
    Feature::Reordering,
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
fn the_cascade_agrees_with_plain_hc_on_the_w9_feature_set() {
    let mut cfg = GateConfig::from_env();
    let full = cfg.seeds >= 250;
    cfg.min_inbound_grammars_per_feature = 0;
    cfg.min_applied_words_per_feature = 0;
    let (min_inbound, min_applied) = (2, 3);
    let report = run(&cfg, CascadeCandidate::build);
    assert_green(&report);
    assert!(report.t2_checked > 0 || cfg.seeds < 20, "no T2 word ran");
    if full {
        for f in W9_FEATURES {
            let c = report.per_feature.get(&f).cloned().unwrap_or_default();
            assert!(
                c.inbound_grammars >= min_inbound && c.applied_words >= min_applied,
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

// Named cases: small hand-written grammars compared with plain HC word by word.

/// A subrule as `(left-hand parts, right-hand actions)`; parts are `(min, max)` over any segment.
struct Sub {
    parts: Vec<(i32, i32)>,
    /// `C0` copies part 0, `I:s` inserts `s`.
    out: Vec<String>,
}

fn sub(parts: &[(i32, i32)], out: &[&str]) -> Sub {
    Sub {
        parts: parts.to_vec(),
        out: out.iter().map(|s| s.to_string()).collect(),
    }
}

struct R {
    subs: Vec<Sub>,
    max_apps: u32,
}

fn r(subs: Vec<Sub>, max_apps: u32) -> R {
    R { subs, max_apps }
}

fn w9_xml(order: &str, rules: &[R], entries: &[&str], slot_rules: &[usize]) -> String {
    let segs = ["a", "t", "k", "s", "i"];
    let mut x = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><HermitCrabInput><Language><Name>named</Name>\
<PartsOfSpeech><PartOfSpeech id=\"posV\"><Name>v</Name></PartOfSpeech></PartsOfSpeech><PhonologicalFeatureSystem>",
    );
    for j in 0..segs.len() {
        x += &format!(
            "<SymbolicFeature id=\"f{j}\"><Name>f{j}</Name><Symbols><Symbol id=\"f{j}P\">+</Symbol>\
<Symbol id=\"f{j}M\">-</Symbol></Symbols></SymbolicFeature>"
        );
    }
    x += "</PhonologicalFeatureSystem><CharacterDefinitionTable id=\"t1\"><Name>t1</Name><SegmentDefinitions>";
    for (i, rep) in segs.iter().enumerate() {
        let values: String = (0..segs.len())
            .map(|j| {
                let v = if i == j { "P" } else { "M" };
                format!("<FeatureValue feature=\"f{j}\" symbolValues=\"f{j}{v}\" />")
            })
            .collect();
        x += &format!(
            "<SegmentDefinition id=\"c{i}\"><Representations><Representation>{rep}</Representation></Representations>{values}</SegmentDefinition>"
        );
    }
    x += "</SegmentDefinitions></CharacterDefinitionTable><NaturalClasses><FeatureNaturalClass id=\"ncAny\"><Name>Any</Name>\
</FeatureNaturalClass></NaturalClasses><PhonologicalRuleDefinitions></PhonologicalRuleDefinitions><Strata>";
    let ids: Vec<String> = (0..rules.len()).map(|k| format!("mr{k}")).collect();
    let listed: Vec<String> = (0..rules.len())
        .filter(|k| !slot_rules.contains(k))
        .map(|k| ids[k].clone())
        .collect();
    x += &format!(
        "<Stratum characterDefinitionTable=\"t1\" morphologicalRuleOrder=\"{order}\" morphologicalRules=\"{}\"><Name>S</Name><MorphologicalRuleDefinitions>",
        listed.join(" ")
    );
    for (k, rule) in rules.iter().enumerate() {
        let id = &ids[k];
        x += &format!(
            "<MorphologicalRule id=\"{id}\" requiredPartsOfSpeech=\"posV\" outputPartOfSpeech=\"posV\" multipleApplication=\"{}\"><Name>{id}</Name><MorphologicalSubrules>",
            rule.max_apps
        );
        for (j, s) in rule.subs.iter().enumerate() {
            let parts: String = s
                .parts
                .iter()
                .enumerate()
                .map(|(p, (min, max))| {
                    format!(
                        "<PhoneticSequence id=\"{id}s{j}p{p}\"><OptionalSegmentSequence min=\"{min}\" max=\"{max}\"><SimpleContext naturalClass=\"ncAny\" /></OptionalSegmentSequence></PhoneticSequence>"
                    )
                })
                .collect();
            let out: String = s
                .out
                .iter()
                .map(|a| match a.split_once(':') {
                    Some(("I", t)) => format!(
                        "<InsertSegments><PhoneticShape>{t}</PhoneticShape></InsertSegments>"
                    ),
                    _ => format!("<CopyFromInput index=\"{id}s{j}p{}\" />", &a[1..]),
                })
                .collect();
            x += &format!(
                "<MorphologicalSubrule id=\"{id}s{j}\"><MorphologicalInput>{parts}</MorphologicalInput><MorphologicalOutput>{out}</MorphologicalOutput></MorphologicalSubrule>"
            );
        }
        x += &format!("</MorphologicalSubrules><MorphemeId>M{id}</MorphemeId></MorphologicalRule>");
    }
    x += "</MorphologicalRuleDefinitions>";
    if !slot_rules.is_empty() {
        x += "<AffixTemplates><AffixTemplate><Name>t</Name>";
        for k in slot_rules {
            x += &format!(
                "<Slot optional=\"true\" morphologicalRules=\"{}\"><Name>s{k}</Name></Slot>",
                ids[*k]
            );
        }
        x += "</AffixTemplate></AffixTemplates>";
    }
    x += "<LexicalEntries>";
    for (k, text) in entries.iter().enumerate() {
        x += &format!(
            "<LexicalEntry id=\"e{k}\" partOfSpeech=\"posV\"><Allomorphs><Allomorph id=\"e{k}a\"><PhoneticShape>{text}</PhoneticShape></Allomorph></Allomorphs><MorphemeId>Re{k}</MorphemeId></LexicalEntry>"
        );
    }
    x += "</LexicalEntries></Stratum></Strata></Language></HermitCrabInput>";
    x
}

fn load_checked(xml: &str) -> &'static Grammar {
    Box::leak(Box::new(
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("{e}\n{xml}")),
    ))
}

/// Cascade and plain HC give the same multiset on every word; returns how many words HC analysed.
fn agree(xml: &str, words: &[&str]) -> usize {
    let grammar = load_checked(xml);
    let morpher = Morpher::new(grammar, 2_000_000);
    let mut meter = BuildMeter::default_limits();
    let (layers, _) =
        build_cascade(grammar, false, &mut meter).unwrap_or_else(|e| panic!("{}", e.text()));
    let analyzer = CascadeAnalyzer::new(layers);
    let mut analysed = 0;
    for w in words {
        let want = hc::parse(&morpher, grammar, w);
        let got = analyzer.analyze(grammar, &morpher, w);
        assert_eq!(got.status, CascadeStatus::Complete, "{w:?}");
        let got_analyses: Vec<_> = got.analyses.into_iter().map(|(a, _, _)| a).collect();
        let got = outcome_of(grammar, Status::Complete, &got_analyses);
        assert_eq!(got.multiset, want.multiset, "word {w:?}");
        if want.analysis_count() > 0 {
            analysed += 1;
        }
    }
    analysed
}

const ONE: (i32, i32) = (1, 1);
const SOME: (i32, i32) = (1, -1);
const ANY: (i32, i32) = (0, -1);

fn infix() -> Sub {
    sub(&[ONE, SOME], &["C0", "I:s", "C1"])
}
fn trunc() -> Sub {
    sub(&[SOME, ONE], &["C0", "I:k"])
}
fn reorder() -> Sub {
    sub(&[ONE, ONE, ANY], &["C1", "C0", "C2"])
}
fn suffix(t: &str) -> Sub {
    sub(&[SOME], &["C0", &format!("I:{t}")])
}

#[test]
fn an_infix_a_truncation_and_a_reordering_agree_with_hc_in_both_schedules() {
    for order in ["linear", "unordered"] {
        for (name, rule, words) in [
            (
                "infix",
                infix(),
                vec!["tak", "tsak", "tsaki", "ta", "tsa", "tasik", "tsi", "kitsa"],
            ),
            (
                "truncation",
                trunc(),
                vec!["tak", "tak", "tk", "ta", "taki", "tai", "tik", "t", "tki"],
            ),
            (
                "reordering",
                reorder(),
                vec!["tak", "atk", "kat", "tka", "akt", "ta", "at", "tat"],
            ),
        ] {
            let x = w9_xml(order, &[r(vec![rule], 1)], &["tak", "kit", "ta"], &[]);
            let analysed = agree(&x, &words);
            assert!(analysed >= 2, "{order} {name}: {analysed}");
        }
    }
}

#[test]
fn interior_and_edge_rules_in_every_order_agree_with_hc() {
    for order in ["linear", "unordered"] {
        let x = w9_xml(
            order,
            &[
                r(vec![suffix("s")], 1),
                r(vec![infix()], 1),
                r(vec![sub(&[SOME], &["I:k", "C0"])], 1),
                r(vec![trunc()], 1),
            ],
            &["tak", "ti"],
            &[],
        );
        let analysed = agree(
            &x,
            &[
                "tak", "tsak", "tsaks", "taks", "tsaksk", "ktak", "ktsak", "ktaks", "tsa", "tsaks",
                "tk", "tkk", "ktk", "ti", "tsi", "tsis", "ktsis", "tk",
            ],
        );
        assert!(analysed >= 8, "{order}: {analysed}");
    }
}

#[test]
fn a_rule_that_is_both_interior_and_edge_keeps_every_allomorph_in_its_own_layers() {
    // Plan 2.4.3: no allomorph of an interior Linear rule may sit in a zone chain.
    let x = w9_xml(
        "linear",
        &[r(vec![infix(), suffix("k")], 2)],
        &["tak", "ta"],
        &[],
    );
    let analysed = agree(
        &x,
        &[
            "tak", "tsak", "taks", "tak", "takk", "tsakk", "tsaskk", "tsskk", "tsask", "ta", "tsa",
            "tak", "taksk", "tsaks",
        ],
    );
    assert!(analysed >= 6, "{analysed}");
    let grammar = load_checked(&x);
    let mut meter = BuildMeter::default_limits();
    let (_, acct) = build_cascade(grammar, false, &mut meter).unwrap();
    assert!(
        acct.zone_allomorphs.is_empty(),
        "{:?}",
        acct.zone_allomorphs
    );
    assert_eq!(acct.h_allomorphs.len(), 2);
    assert_eq!(acct.h_layers, 2, "max_apps 2 gives two layers");
    // The check refuses an edge allomorph placed in a zone.
    let mut bad = acct.clone();
    bad.zone_allomorphs.insert((MRuleId(0), 1));
    assert!(pg_foma::cascade_intersect::build::verify_for_test(grammar, &bad).is_err());
}

#[test]
fn an_unordered_block_repeats_h_a_s_times_and_a_s_counts_every_interior_rule() {
    // One rule with an interior and an edge allomorph, max_apps 2, plus one interior rule: A_s = 3.
    let x = w9_xml(
        "unordered",
        &[r(vec![infix(), suffix("k")], 2), r(vec![trunc()], 1)],
        &["tak"],
        &[],
    );
    let grammar = load_checked(&x);
    let mut meter = BuildMeter::default_limits();
    let (layers, acct) = build_cascade(grammar, false, &mut meter).unwrap();
    assert_eq!(acct.a_s, vec![(0, 3, 3)]);
    assert_eq!(acct.h_layers, 3);
    // Lex, Z (H Z)^3, Repr
    assert_eq!(layers.layers.len(), 2 + 1 + 6);
    // The edge allomorph of the interior rule sits in the zone, the interior one in H.
    assert!(acct.zone_allomorphs.contains(&(MRuleId(0), 1)));
    assert!(acct.h_allomorphs.contains(&(MRuleId(0), 0)));
    let analysed = agree(
        &x,
        &[
            "tak", "tsak", "taks", "tsaksk", "tk", "tkk", "tsaskk", "takk",
        ],
    );
    assert!(analysed >= 2, "{analysed}");
}

#[test]
fn interior_rules_reachable_only_through_a_template_slot_are_modelled() {
    for order in ["linear", "unordered"] {
        let x = w9_xml(
            order,
            &[r(vec![suffix("s")], 1), r(vec![infix()], 1)],
            &["tak"],
            &[1],
        );
        let analysed = agree(&x, &["tak", "tsak", "taks", "tsaks", "tsa", "tkas"]);
        assert!(analysed >= 3, "{order}: {analysed}");
    }
}

#[test]
fn a_boundary_survives_an_infix_and_a_truncation_or_is_dropped() {
    // A grammar with a boundary character: the root carries it.
    let x = w9_xml("linear", &[r(vec![infix()], 1)], &["ta+k"], &[]);
    let x = x.replace(
        "</SegmentDefinitions></CharacterDefinitionTable>",
        "</SegmentDefinitions><BoundaryDefinitions><BoundaryDefinition id=\"bnd\"><Representations>\
<Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions></CharacterDefinitionTable>",
    );
    let analysed = agree(&x, &["tak", "tsak", "tsa+k", "ta+sk", "ta+k", "tsk"]);
    assert!(analysed >= 1, "{analysed}");
}

#[test]
fn unmodelled_interior_shapes_are_still_refused_by_name() {
    // Reduplication (a part copied twice), unbounded truncation and a Modify action.
    let redup = w9_xml(
        "linear",
        &[r(vec![sub(&[SOME], &["C0", "C0"])], 1)],
        &["ta"],
        &[],
    );
    let trunc_unb = w9_xml(
        "linear",
        &[r(vec![sub(&[ONE, SOME], &["C0"])], 1)],
        &["ta"],
        &[],
    );
    for (x, name) in [
        (redup, "reduplication"),
        (trunc_unb, "unbounded truncation"),
    ] {
        let g = load_checked(&x);
        let text: Vec<String> = check_all(g, false).iter().map(|r| r.text()).collect();
        assert!(text.iter().any(|t| t.contains(name)), "{name}: {text:?}");
        let mut meter = BuildMeter::default_limits();
        assert!(build_cascade(g, false, &mut meter).is_err());
    }
}

#[test]
fn an_interior_rule_with_a_huge_max_apps_is_refused_for_its_layer_count() {
    let x = w9_xml("linear", &[r(vec![infix()], 300)], &["tak"], &[]);
    let g = load_checked(&x);
    let mut meter = BuildMeter::default_limits();
    let err = build_cascade(g, false, &mut meter).err().expect("refused");
    assert!(err.text().contains("layer count"), "{}", err.text());
}
