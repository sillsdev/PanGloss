//! The cascade analyzer plus `confirm_pin_groups` against plain HC over generated grammars.

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
                if generated.features.iter().all(|f| W5_FEATURES.contains(f)) {
                    return Err(format!(
                        "not modelled ({text}) but every feature is in the W5 set: {:?}",
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
    let (min_inbound, min_applied) = (2, 3);
    let report = run(&cfg, CascadeCandidate::build);
    assert_green(&report);
    assert!(report.t2_checked > 0 || cfg.seeds < 20, "no T2 word ran");
    if full {
        for f in W5_FEATURES {
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

/// `(pre, suf)` pairs make one subrule each: an edge-only allomorph; both empty is a null one.
struct RuleSpec {
    allomorphs: Vec<(&'static str, &'static str)>,
    max_apps: u32,
}

struct StratumSpec {
    order: &'static str,
    rules: Vec<RuleSpec>,
    entries: Vec<&'static str>,
}

fn rule(allomorphs: &[(&'static str, &'static str)], max_apps: u32) -> RuleSpec {
    RuleSpec {
        allomorphs: allomorphs.to_vec(),
        max_apps,
    }
}

fn xml(segments: &[&[&str]], boundary: Option<&str>, strata: &[StratumSpec]) -> String {
    let mut x = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><HermitCrabInput><Language><Name>named</Name>\
<PartsOfSpeech><PartOfSpeech id=\"posV\"><Name>v</Name></PartOfSpeech></PartsOfSpeech>\
<PhonologicalFeatureSystem>",
    );
    for j in 0..segments.len() {
        x += &format!(
            "<SymbolicFeature id=\"f{j}\"><Name>f{j}</Name><Symbols><Symbol id=\"f{j}P\">+</Symbol>\
<Symbol id=\"f{j}M\">-</Symbol></Symbols></SymbolicFeature>"
        );
    }
    x += "</PhonologicalFeatureSystem><CharacterDefinitionTable id=\"t1\"><Name>t1</Name><SegmentDefinitions>";
    for (i, reps) in segments.iter().enumerate() {
        let reps: String = reps
            .iter()
            .map(|r| format!("<Representation>{r}</Representation>"))
            .collect();
        let values: String = (0..segments.len())
            .map(|j| {
                let v = if i == j { "P" } else { "M" };
                format!("<FeatureValue feature=\"f{j}\" symbolValues=\"f{j}{v}\" />")
            })
            .collect();
        x += &format!(
            "<SegmentDefinition id=\"c{i}\"><Representations>{reps}</Representations>{values}</SegmentDefinition>"
        );
    }
    x += "</SegmentDefinitions>";
    if let Some(b) = boundary {
        x += &format!(
            "<BoundaryDefinitions><BoundaryDefinition id=\"bnd\"><Representations>\
<Representation>{b}</Representation></Representations></BoundaryDefinition></BoundaryDefinitions>"
        );
    }
    x += "</CharacterDefinitionTable><NaturalClasses><FeatureNaturalClass id=\"ncAny\"><Name>Any</Name>\
</FeatureNaturalClass></NaturalClasses><PhonologicalRuleDefinitions></PhonologicalRuleDefinitions><Strata>";
    let (mut next_rule, mut next_entry) = (0, 0);
    for s in strata {
        let ids: Vec<String> = (0..s.rules.len())
            .map(|k| format!("mr{}", next_rule + k))
            .collect();
        x += &format!(
            "<Stratum characterDefinitionTable=\"t1\" morphologicalRuleOrder=\"{}\" morphologicalRules=\"{}\"><Name>S</Name><MorphologicalRuleDefinitions>",
            s.order,
            ids.join(" ")
        );
        for (k, r) in s.rules.iter().enumerate() {
            let id = &ids[k];
            x += &format!(
                "<MorphologicalRule id=\"{id}\" requiredPartsOfSpeech=\"posV\" outputPartOfSpeech=\"posV\" multipleApplication=\"{}\"><Name>{id}</Name><MorphologicalSubrules>",
                r.max_apps
            );
            for (j, (pre, suf)) in r.allomorphs.iter().enumerate() {
                let ins = |t: &str| {
                    if t.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "<InsertSegments><PhoneticShape>{t}</PhoneticShape></InsertSegments>"
                        )
                    }
                };
                x += &format!(
                    "<MorphologicalSubrule id=\"{id}s{j}\"><MorphologicalInput><PhoneticSequence id=\"{id}in{j}\">\
<OptionalSegmentSequence min=\"1\" max=\"-1\"><SimpleContext naturalClass=\"ncAny\" /></OptionalSegmentSequence>\
</PhoneticSequence></MorphologicalInput><MorphologicalOutput>{}<CopyFromInput index=\"{id}in{j}\" />{}</MorphologicalOutput></MorphologicalSubrule>",
                    ins(pre),
                    ins(suf)
                );
            }
            x += &format!(
                "</MorphologicalSubrules><MorphemeId>M{id}</MorphemeId></MorphologicalRule>"
            );
        }
        x += "</MorphologicalRuleDefinitions><LexicalEntries>";
        for text in &s.entries {
            let id = format!("e{next_entry}");
            next_entry += 1;
            let allomorphs: String = text
                .split('|')
                .enumerate()
                .map(|(k, t)| {
                    format!(
                        "<Allomorph id=\"{id}a{k}\"><PhoneticShape>{t}</PhoneticShape></Allomorph>"
                    )
                })
                .collect();
            x += &format!(
                "<LexicalEntry id=\"{id}\" partOfSpeech=\"posV\"><Allomorphs>{allomorphs}</Allomorphs>\
<MorphemeId>R{id}</MorphemeId></LexicalEntry>"
            );
        }
        x += "</LexicalEntries></Stratum>";
        next_rule += s.rules.len();
    }
    x += "</Strata></Language></HermitCrabInput>";
    x
}

/// Cascade and plain HC give the same multiset on every word; returns how many words HC analysed.
fn agree(xml: &str, words: &[&str]) -> usize {
    let grammar: &'static Grammar = Box::leak(Box::new(
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("{e}\n{xml}")),
    ));
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

fn linear(rules: Vec<RuleSpec>, entries: &[&'static str]) -> StratumSpec {
    StratumSpec {
        order: "linear",
        rules,
        entries: entries.to_vec(),
    }
}

#[test]
fn prefixes_suffixes_circumfixes_and_null_allomorphs_agree_with_hc() {
    let strata = [linear(
        vec![
            rule(&[("", "a")], 1),
            rule(&[("t", "")], 1),
            rule(&[("s", "k")], 1),
            rule(&[("", ""), ("k", "")], 1),
        ],
        &["ta", "ka"],
    )];
    let x = xml(&[&["a"], &["t"], &["k"], &["s"]], None, &strata);
    let analysed = agree(
        &x,
        &[
            "ta", "taa", "tta", "ttaa", "kta", "ktaa", "skta", "ska", "stak", "skak", "sktak",
            "kka", "aka", "tak", "s", "",
        ],
    );
    assert!(analysed >= 8, "{analysed}");
}

#[test]
fn a_digraph_and_a_second_representation_are_explored_in_every_segmentation() {
    let strata = [linear(vec![rule(&[("", "s")], 1)], &["ta", "tsa"])];
    let x = xml(
        &[&["a"], &["t"], &["s"], &["ts", "c"], &["k", "q"]],
        None,
        &strata,
    );
    let analysed = agree(
        &x,
        &["ta", "tas", "tsa", "tsas", "ca", "cas", "ka", "qa", "tats"],
    );
    assert!(analysed >= 4, "{analysed}");
}

#[test]
fn an_empty_representation_matches_without_consuming() {
    let strata = [linear(vec![rule(&[("", "s")], 1)], &["ta"])];
    let x = xml(&[&["a"], &["t"], &["s"], &["", "h"]], None, &strata);
    let analysed = agree(&x, &["ta", "tas", "tha", "tahs", "h", ""]);
    assert!(analysed >= 2, "{analysed}");
}

#[test]
fn a_boundary_in_an_affix_insert_or_at_a_junction_is_written_or_dropped() {
    let strata = [
        linear(vec![rule(&[("", "+s")], 1)], &["ta"]),
        linear(vec![rule(&[("k+", "")], 1)], &[]),
    ];
    let x = xml(&[&["a"], &["t"], &["k"], &["s"]], Some("+"), &strata);
    let analysed = agree(
        &x,
        &["ta", "tas", "ta+s", "kta", "ktas", "k+tas", "kta+s", "+"],
    );
    assert!(analysed >= 2, "{analysed}");
}

#[test]
fn a_rule_repeats_exactly_max_apps_times_in_a_chain_and_an_unordered_pool() {
    for order in ["linear", "unordered"] {
        let strata = [StratumSpec {
            order,
            rules: vec![
                rule(&[("", "s")], 3),
                rule(&[("", "k")], 2),
                rule(&[("s", "")], 2),
            ],
            entries: vec!["ta"],
        }];
        let x = xml(&[&["a"], &["t"], &["k"], &["s"]], None, &strata);
        let analysed = agree(
            &x,
            &[
                "ta", "tas", "tass", "tasss", "tassss", "tak", "takk", "takkk", "taks", "tasks",
                "taskk", "sta", "ssta", "sssta", "stas",
            ],
        );
        assert!(analysed >= 6, "{order}: {analysed}");
    }
}

#[test]
fn roots_of_a_later_stratum_pass_through_the_earlier_zones_unchanged() {
    let strata = [
        linear(vec![rule(&[("", "s")], 1)], &["ta"]),
        linear(vec![rule(&[("k", "")], 1)], &["ka"]),
    ];
    let x = xml(&[&["a"], &["t"], &["k"], &["s"]], None, &strata);
    let analysed = agree(&x, &["ta", "tas", "ktas", "ka", "kka", "kas", "kkas"]);
    assert!(analysed >= 4, "{analysed}");
}

#[test]
fn each_refused_construct_is_named_when_the_candidate_is_built_from_its_seed() {
    use cascade_fuzz::features::Refusal;
    for (i, refusal) in Refusal::ALL.into_iter().enumerate() {
        let generated =
            cascade_fuzz::gen::generate(&cascade_fuzz::gen::Plan::for_seed(4 + 5 * i as u64));
        let c = CascadeCandidate::build(&generated).unwrap();
        match c.admission() {
            Admission::Refused(t) => assert!(t.contains(refusal.named_construct()), "{t}"),
            other => panic!("{refusal:?}: {other:?}"),
        }
    }
}

#[test]
fn a_root_with_a_natural_class_node_is_refused_by_name() {
    let strata = [linear(vec![rule(&[("", "s")], 1)], &["t[Any]"])];
    let x = xml(&[&["a"], &["t"], &["s"]], None, &strata);
    let grammar = pg_grammar::load(&x).unwrap();
    let refusals = check_all(&grammar, false);
    let text: Vec<String> = refusals.iter().map(|r| r.text()).collect();
    assert!(
        text.iter().any(|t| t.contains("root natural-class node")),
        "{text:?}"
    );
    let mut meter = BuildMeter::default_limits();
    assert!(build_cascade(&grammar, false, &mut meter).is_err());
}

#[test]
fn a_morpher_with_supplied_roots_is_refused_when_the_cascade_was_built_without() {
    let strata = [linear(vec![rule(&[("", "s")], 1)], &["ta"])];
    let x = xml(&[&["a"], &["t"], &["s"]], None, &strata);
    let grammar = pg_grammar::load(&x).unwrap();
    let root = pg_parse::SuppliedRoot {
        entry_id: "pgl_x".into(),
        realization_id: "pgl_x:sig0".into(),
        lexical_spelling: "t".into(),
        gloss: String::new(),
        syn_fs: pg_featstruct::FeatureStruct::EMPTY,
        mpr: pg_grammar::model::MprSet::EMPTY,
        stratum: pg_grammar::model::StratumId(0),
        authority: pg_parse::RootAuthority::Supplied,
    };
    let overlay = pg_parse::SuppliedRootOverlay::build(&grammar, vec![root]).unwrap();
    let with = Morpher::new_with_overlay(&grammar, 2_000_000, &overlay);
    let without = Morpher::new(&grammar, 2_000_000);
    assert!(with.has_supplied_roots() && !without.has_supplied_roots());
    let mut meter = BuildMeter::default_limits();
    let (layers, _) = build_cascade(&grammar, false, &mut meter).unwrap();
    let analyzer = CascadeAnalyzer::new(layers);
    let got = analyzer.analyze(&grammar, &with, "ts");
    assert!(
        matches!(got.status, CascadeStatus::Refused(_)),
        "{:?}",
        got.status
    );
    assert!(got.analyses.is_empty());
    let ok = analyzer.analyze(&grammar, &without, "tas");
    assert_eq!(ok.status, CascadeStatus::Complete);
}

#[test]
fn the_accounting_check_reads_the_grammar_and_the_built_layers_not_the_builders_own_loop() {
    let strata = [linear(vec![rule(&[("", "s")], 1)], &["ta"])];
    let x = xml(&[&["a"], &["t"], &["s"]], None, &strata);
    let grammar = pg_grammar::load(&x).unwrap();
    let mut meter = BuildMeter::default_limits();
    let (_, full) = build_cascade(&grammar, false, &mut meter).unwrap();
    assert!(!full.emitted_rules.is_empty());
    let mut a = full.clone();
    a.emitted_rules.clear();
    assert!(pg_foma::cascade_intersect::build::verify_for_test(&grammar, &a).is_err());
    let mut b = full.clone();
    b.unreached_rules.insert(MRuleId(0));
    assert!(pg_foma::cascade_intersect::build::verify_for_test(&grammar, &b).is_err());
}

#[test]
fn the_accounting_check_names_a_dropped_allomorph_slot_stratum_or_root_allomorph() {
    let strata = [linear(vec![rule(&[("", "s"), ("k", "")], 1)], &["ta"])];
    let x = xml(&[&["a"], &["t"], &["k"], &["s"]], None, &strata);
    let grammar = pg_grammar::load(&x).unwrap();
    let mut meter = BuildMeter::default_limits();
    let (_, full) = build_cascade(&grammar, false, &mut meter).unwrap();
    assert_eq!(full.placed_allomorphs.len(), 2);
    for drop in 0..3 {
        let mut a = full.clone();
        match drop {
            0 => {
                let first = *a.placed_allomorphs.iter().next().unwrap();
                a.placed_allomorphs.remove(&first);
            }
            1 => a.zones.clear(),
            _ => a.root_allomorphs.clear(),
        }
        let err = pg_foma::cascade_intersect::build::verify_for_test(&grammar, &a);
        assert!(err.is_err(), "dropping part {drop} must fail the check");
    }
    assert!(pg_foma::cascade_intersect::build::verify_for_test(&grammar, &full).is_ok());
}

#[test]
fn confirmation_runs_under_the_reference_morphers_own_limits() {
    let strata = [linear(vec![rule(&[("", "s")], 1)], &["ta", "ka"])];
    let x = xml(&[&["a"], &["t"], &["k"], &["s"]], None, &strata);
    let grammar: &'static Grammar = Box::leak(Box::new(pg_grammar::load(&x).unwrap()));
    let mut meter = BuildMeter::default_limits();
    let (layers, _) = build_cascade(grammar, false, &mut meter).unwrap();
    let analyzer = CascadeAnalyzer::new(layers);
    let roomy = Morpher::new(grammar, 2_000_000);
    let done = analyzer.analyze(grammar, &roomy, "tas");
    assert_eq!(done.status, CascadeStatus::Complete);
    assert!(!done.analyses.is_empty());
    let tiny = Morpher::new(grammar, 2);
    let stopped = analyzer.analyze(grammar, &tiny, "tas");
    assert!(
        matches!(stopped.status, CascadeStatus::Incomplete(_)),
        "{:?}",
        stopped.status
    );
    assert!(stopped.analyses.is_empty());
}

#[test]
fn every_allomorph_of_a_root_and_an_unspellable_segment_are_modelled() {
    let strata = [linear(vec![rule(&[("", "s")], 1)], &["ta|ka", "tsa"])];
    let x = xml(
        &[&["a"], &["t"], &["k"], &["s"], &["ts"], &[]],
        None,
        &strata,
    );
    let analysed = agree(&x, &["ta", "ka", "tas", "kas", "tsa", "tsas", "a"]);
    assert!(analysed >= 5, "{analysed}");
}

#[test]
fn a_circumfix_is_found_only_with_both_halves_in_a_chain_and_a_pool() {
    for order in ["linear", "unordered"] {
        let strata = [StratumSpec {
            order,
            rules: vec![rule(&[("s", "k")], 1)],
            entries: vec!["ta"],
        }];
        let x = xml(&[&["a"], &["t"], &["k"], &["s"]], None, &strata);
        let analysed = agree(&x, &["ta", "stak", "sta", "tak", "sstakk"]);
        assert_eq!(analysed, 2, "{order}");
    }
}

#[test]
fn every_tag_lex_and_the_zones_emit_decodes_to_the_kind_it_was_built_as() {
    use pg_foma_runtime::cascade::{Layer, LayerScratch, TagKind};
    let strata = [
        linear(
            vec![rule(&[("", "s"), ("", "")], 1), rule(&[("k", "")], 1)],
            &["ta|ka", "tsa"],
        ),
        linear(vec![rule(&[("s", "k")], 1)], &["ka"]),
    ];
    let x = xml(&[&["a"], &["t"], &["k"], &["s"], &["ts"]], None, &strata);
    let grammar = pg_grammar::load(&x).unwrap();
    let mut meter = BuildMeter::default_limits();
    let (layers, _) = build_cascade(&grammar, false, &mut meter).unwrap();
    let mut tags_of = |layer: usize| {
        let l = &layers.layers[layer];
        let mut scratch = LayerScratch::default();
        let mut out = Vec::new();
        for s in 0..l.state_count() as u32 {
            l.arcs_free(&mut scratch, s, &mut |a| out.extend_from_slice(a.tags));
        }
        out
    };
    let lex: std::collections::HashSet<_> = tags_of(0)
        .into_iter()
        .map(|t| t.kind().expect("a built tag"))
        .collect();
    let want: std::collections::HashSet<_> = (0..grammar.entries.len() as u32)
        .map(TagKind::Root)
        .collect();
    assert_eq!(lex, want, "Lex emits exactly one root tag per entry");
    let zone_tags: std::collections::HashSet<_> = (1..layers.layers.len() - 1)
        .flat_map(&mut tags_of)
        .map(|t| t.kind().expect("a built tag"))
        .collect();
    assert_eq!(zone_tags.len(), 3);
    assert!(zone_tags.iter().all(|t| matches!(t, TagKind::Rule(_))));
    assert!(
        tags_of(layers.layers.len() - 1).is_empty(),
        "Repr carries no tags"
    );
}

#[test]
fn a_zone_drops_a_boundary_token_it_reads_and_spelling_gives_an_unspellable_row_an_epsilon() {
    use pg_foma_runtime::cascade::{Layer, LayerScratch};
    let mut meter = BuildMeter::default_limits();
    let zone = pg_foma::cascade_intersect::zone::build_zone(
        "z",
        6,
        &[5],
        pg_foma::cascade_intersect::zone::ZoneKind::Pool,
        &[],
        &mut meter,
    )
    .unwrap()
    .layer;
    let mut scratch = LayerScratch::default();
    let start = zone.start(&mut scratch);
    let (mut kept, mut dropped) = (false, false);
    zone.arcs_from(&mut scratch, start, Some(5), &mut |a| match a.output {
        Some(5) => kept = true,
        None => dropped = true,
        _ => {}
    });
    assert!(kept && dropped, "a boundary is kept or dropped");

    let strata = [linear(vec![], &["ta"])];
    let grammar = pg_grammar::load(&xml(&[&["a"], &["t"], &[]], None, &strata)).unwrap();
    let spelling =
        pg_foma::cascade_intersect::spell::spell_table(&grammar.char_tables[0], &mut meter)
            .unwrap();
    assert_eq!(spelling.forms[2], vec![Vec::<char>::new()]);
    assert_eq!(spelling.epsilon_only, 1);
}
