//! The shadowed-allomorph and dead-continuation reports: unit grammars for each class, and an oracle over every conformance fixture that no allomorph reported `shadowed` or `proven` dead is ever selected.

use std::collections::HashSet;

use pg_conformance_fixtures::{discover_scoped, ConformanceScope};
use pg_grammar_model::model::{AllomorphId, Grammar};
use pg_parse::morpher::TracedGenerateLimits;
use pg_parse::{GenMorpheme, Morpher};
use pg_rules::dead::{dead_continuations, Continuation};
use pg_rules::shadow::{shadowed_allomorphs, Owner, Verdict};

const ANY: &str = r#"<OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>"#;
const ANY_L: &str = r#"<OptionalSegmentSequence min="0" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence><SimpleContext naturalClass="ncL" />"#;
const RIGHT_I: &str = r#"<RequiredEnvironments><Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncI" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment></RequiredEnvironments>"#;
const RIGHT_T: &str = r#"<RequiredEnvironments><Environment><RightEnvironment><PhoneticTemplate><PhoneticSequence><SimpleContext naturalClass="ncT" /></PhoneticSequence></PhoneticTemplate></RightEnvironment></Environment></RequiredEnvironments>"#;

/// One suffix subrule: input pattern, input attributes, output features, inserted shape, environment.
fn sub(id: &str, in_attr: &str, lhs: &str, out_attr: &str, shape: &str, env: &str) -> String {
    format!(
        r#"<MorphologicalSubrule id="{id}"><MorphologicalInput {in_attr}><PhoneticSequence id="{id}_s">{lhs}</PhoneticSequence></MorphologicalInput><MorphologicalOutput {out_attr}><CopyFromInput index="{id}_s" /><InsertSegments><PhoneticShape>{shape}</PhoneticShape></InsertSegments></MorphologicalOutput>{env}</MorphologicalSubrule>"#
    )
}

fn rule(id: &str, subs: &[String]) -> String {
    format!(
        r#"<MorphologicalRule id="{id}" requiredPartsOfSpeech="posRoot" outputPartOfSpeech="posRoot"><Name>{id}</Name><MorphologicalSubrules>{}</MorphologicalSubrules><MorphemeId>{id}</MorphemeId></MorphologicalRule>"#,
        subs.concat()
    )
}

fn entry(id: &str, attr: &str, allomorphs: &[(&str, &str)]) -> String {
    let allos: String = allomorphs
        .iter()
        .map(|(shape, extra)| {
            format!(r#"<Allomorph id="{id}_{shape}"><PhoneticShape>{shape}</PhoneticShape>{extra}</Allomorph>"#)
        })
        .collect();
    format!(
        r#"<LexicalEntry id="{id}" partOfSpeech="posRoot" {attr}><Allomorphs>{allos}</Allomorphs><MorphemeId>{id}</MorphemeId><Gloss>{id}</Gloss></LexicalEntry>"#
    )
}

fn grammar(rules: &[String], rule_ids: &str, entries: &[String]) -> Grammar {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput><Language><Name>Shadow</Name>
<PartsOfSpeech><PartOfSpeech id="posRoot"><Name>root</Name></PartOfSpeech></PartsOfSpeech>
<MorphologicalPhonologicalRuleFeatures>
<MorphologicalPhonologicalRuleFeature id="vcA">VC-A</MorphologicalPhonologicalRuleFeature>
<MorphologicalPhonologicalRuleFeature id="vcB">VC-B</MorphologicalPhonologicalRuleFeature>
<MorphologicalPhonologicalRuleFeatureGroup matchType="any" outputType="overwrite" features="vcA vcB"><Name>inflClasses</Name></MorphologicalPhonologicalRuleFeatureGroup>
</MorphologicalPhonologicalRuleFeatures>
<CharacterDefinitionTable id="table1"><Name>Main</Name><SegmentDefinitions>
<SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cL"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cI"><Representations><Representation>i</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cW"><Representations><Representation>w</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses>
<FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
<SegmentNaturalClass id="ncL"><Name>L</Name><Segment segment="cL" /></SegmentNaturalClass>
<SegmentNaturalClass id="ncI"><Name>I</Name><Segment segment="cI" /></SegmentNaturalClass>
<SegmentNaturalClass id="ncT"><Name>T</Name><Segment segment="cT" /></SegmentNaturalClass>
</NaturalClasses>
<Strata><Stratum characterDefinitionTable="table1" morphologicalRuleOrder="unordered" morphologicalRules="{rule_ids}"><Name>Main</Name>
<MorphologicalRuleDefinitions>{}</MorphologicalRuleDefinitions>
<LexicalEntries>{}</LexicalEntries>
</Stratum></Strata></Language></HermitCrabInput>"#,
        rules.concat(),
        entries.concat()
    );
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("test grammar failed to load: {e}"))
}

fn verdicts_of_rule(g: &Grammar, name: &str) -> Vec<Verdict> {
    let found = shadowed_allomorphs(g)
        .into_iter()
        .find(|m| matches!(m.owner, Owner::Rule(r) if g.mrules[r.0 as usize].authored_name() == Some(name)))
        .unwrap_or_else(|| panic!("no report for rule {name}"));
    found.alternates.iter().map(|a| a.verdict).collect()
}

fn verdicts_of_entry(g: &Grammar, id: &str) -> Vec<Verdict> {
    let found = shadowed_allomorphs(g)
        .into_iter()
        .find(|m| matches!(m.owner, Owner::Entry(e) if g.entries[e.0 as usize].authored_id == id))
        .unwrap_or_else(|| {
            panic!(
                "no report for entry {id}: {:?}",
                g.entries
                    .iter()
                    .map(|e| (e.authored_id.clone(), e.allomorphs.len()))
                    .collect::<Vec<_>>()
            )
        });
    found.alternates.iter().map(|a| a.verdict).collect()
}

#[test]
fn an_unconstrained_w_shadows_liw() {
    let g = grammar(
        &[rule(
            "PASS",
            &[
                sub("w", "", ANY, "", "w", ""),
                sub("liw", "", ANY_L, "", "iw", ""),
            ],
        )],
        "PASS",
        &[entry("eK", "", &[("bak", "")])],
    );
    assert_eq!(
        verdicts_of_rule(&g, "PASS"),
        [Verdict::Selectable, Verdict::Shadowed]
    );
    let report = shadowed_allomorphs(&g);
    let pass = report
        .iter()
        .find(|m| matches!(m.owner, Owner::Rule(_)))
        .unwrap();
    assert_eq!(pass.alternates[1].blockers, [pass.alternates[0].allomorph]);
    assert_eq!(pass.alternates[1].reasons, ["unconstrained_earlier"]);
}

#[test]
fn a_narrow_first_alternate_does_not_shadow_a_general_one() {
    let g = grammar(
        &[rule(
            "PASS",
            &[
                sub("liw", "", ANY_L, "", "iw", ""),
                sub("w", "", ANY, "", "w", ""),
            ],
        )],
        "PASS",
        &[entry("eK", "", &[("bak", "")])],
    );
    assert_eq!(verdicts_of_rule(&g, "PASS"), [Verdict::Selectable; 2]);
}

#[test]
fn free_fluctuating_alternates_are_both_selectable() {
    let g = grammar(
        &[rule(
            "PASS",
            &[
                sub("w1", "", ANY, "", "w", ""),
                sub("w2", "", ANY, "", "iw", ""),
            ],
        )],
        "PASS",
        &[entry("eK", "", &[("bak", ""), ("bal", "")])],
    );
    assert_eq!(verdicts_of_rule(&g, "PASS"), [Verdict::Selectable; 2]);
    assert_eq!(verdicts_of_entry(&g, "eK"), [Verdict::Selectable; 2]);
}

#[test]
fn a_required_environment_on_the_first_alternate_does_not_shadow() {
    let g = grammar(
        &[rule(
            "PASS",
            &[
                sub("w", "", ANY, "", "w", RIGHT_T),
                sub("iw", "", ANY, "", "iw", ""),
            ],
        )],
        "PASS",
        &[entry("eK", "", &[("bak", "")])],
    );
    assert_eq!(verdicts_of_rule(&g, "PASS"), [Verdict::Selectable; 2]);
}

#[test]
fn identical_environments_shadow_and_a_different_one_is_only_maybe() {
    let same = grammar(
        &[rule(
            "PASS",
            &[
                sub("w", "", ANY, "", "w", RIGHT_T),
                sub("lw", "", ANY_L, "", "lw", RIGHT_T),
            ],
        )],
        "PASS",
        &[entry("eK", "", &[("bak", "")])],
    );
    assert_eq!(
        verdicts_of_rule(&same, "PASS"),
        [Verdict::Selectable, Verdict::Shadowed]
    );
    let different = grammar(
        &[rule(
            "PASS",
            &[
                sub("w", "", ANY, "", "w", RIGHT_T),
                sub("lw", "", ANY_L, "", "lw", RIGHT_I),
            ],
        )],
        "PASS",
        &[entry("eK", "", &[("bak", "")])],
    );
    let report = shadowed_allomorphs(&different);
    let alt = &report
        .iter()
        .find(|m| matches!(m.owner, Owner::Rule(_)))
        .unwrap()
        .alternates[1];
    assert_eq!(alt.verdict, Verdict::MaybeShadowed);
    assert_eq!(alt.reasons, ["environment_subsumption_undecided"]);
}

#[test]
fn rule_feature_gates_decide_between_a_cover_and_no_cover() {
    let gated_first = grammar(
        &[rule(
            "PASS",
            &[
                sub("w", r#"requiredMPRFeatures="vcA""#, ANY, "", "w", ""),
                sub("iw", "", ANY, "", "iw", ""),
            ],
        )],
        "PASS",
        &[entry("eK", "", &[("bak", "")])],
    );
    assert_eq!(
        verdicts_of_rule(&gated_first, "PASS"),
        [Verdict::Selectable; 2]
    );
    let gated_later = grammar(
        &[rule(
            "PASS",
            &[
                sub("w", "", ANY, "", "w", ""),
                sub("iw", r#"requiredMPRFeatures="vcA""#, ANY, "", "iw", ""),
            ],
        )],
        "PASS",
        &[entry("eK", "", &[("bak", "")])],
    );
    assert_eq!(
        verdicts_of_rule(&gated_later, "PASS"),
        [Verdict::Selectable, Verdict::Shadowed]
    );
}

#[test]
fn stem_alternates_follow_the_engine_order_check() {
    // An environment-constrained first stem alternate leaves an unconstrained second one selectable.
    let g = grammar(
        &[rule("T", &[sub("t", "", ANY, "", "t", "")])],
        "T",
        &[
            entry("eEnvFirst", "", &[("wak", RIGHT_T), ("wik", "")]),
            entry("eEnvSecond", "", &[("wik", ""), ("wak", RIGHT_T)]),
            entry("eBound", "", &[("pa", ""), ("pak", "")]),
        ],
    );
    assert_eq!(verdicts_of_entry(&g, "eEnvFirst"), [Verdict::Selectable; 2]);
    // The unconstrained first alternate is eligible wherever the environment one is, so the later one is covered.
    assert_eq!(
        verdicts_of_entry(&g, "eEnvSecond"),
        [Verdict::Selectable, Verdict::Shadowed]
    );
}

fn dead_of(g: &Grammar, entry: &str) -> Vec<(String, Continuation)> {
    dead_continuations(g)
        .into_iter()
        .filter(|r| g.entries[r.entry.0 as usize].authored_id == entry)
        .map(|r| {
            (
                g.entries[r.entry.0 as usize].allomorphs[r.index]
                    .shape
                    .text
                    .clone(),
                r.verdict,
            )
        })
        .collect()
}

fn dead_grammar(with_converter: bool) -> Grammar {
    let appl = rule(
        "APPL",
        &[sub(
            "appl",
            r#"requiredMPRFeatures="vcB""#,
            ANY,
            "",
            "i",
            "",
        )],
    );
    let conv = rule(
        "CONV",
        &[sub(
            "conv",
            r#"requiredMPRFeatures="vcA""#,
            ANY,
            r#"MPRFeatures="vcB""#,
            "k",
            "",
        )],
    );
    let (rules, ids) = if with_converter {
        (vec![appl, conv], "APPL CONV")
    } else {
        (vec![appl], "APPL")
    };
    grammar(
        &rules,
        ids,
        &[
            entry(
                "eA",
                r#"ruleFeatures="vcA""#,
                &[("bal", ""), ("ba", RIGHT_I)],
            ),
            entry("eB", r#"ruleFeatures="vcB""#, &[("tal", "")]),
        ],
    )
}

#[test]
fn a_vc_a_root_is_dead_without_the_converter_and_live_with_it() {
    let without = dead_grammar(false);
    assert_eq!(
        dead_of(&without, "eA"),
        [
            ("bal".into(), Continuation::Maybe),
            ("ba".into(), Continuation::Proven)
        ]
    );
    assert_eq!(
        dead_of(&without, "eB"),
        [("tal".into(), Continuation::Live)]
    );
    let with = dead_grammar(true);
    assert_eq!(
        dead_of(&with, "eA"),
        [
            ("bal".into(), Continuation::Live),
            ("ba".into(), Continuation::Live)
        ]
    );
    let blocked = dead_continuations(&without)
        .into_iter()
        .find(|r| r.verdict == Continuation::Proven)
        .unwrap();
    assert_eq!(blocked.blocked_by[0].blocking.kind, "required_mpr");
    assert_eq!(blocked.blocked_by[0].blocking.names, ["VC-B"]);
    assert_eq!(blocked.features, ["VC-A"]);
}

#[test]
fn a_root_needing_a_following_i_is_maybe_dead_while_every_i_affix_rejects_its_class() {
    let build = |converter: bool| {
        let mut rules = vec![
            rule(
                "APPL",
                &[sub(
                    "appl",
                    r#"requiredMPRFeatures="vcB""#,
                    ANY,
                    "",
                    "i",
                    "",
                )],
            ),
            rule("PASS", &[sub("w", "", ANY, "", "w", "")]),
        ];
        let mut ids = "APPL PASS";
        if converter {
            rules.push(rule(
                "CONV",
                &[sub(
                    "conv",
                    r#"requiredMPRFeatures="vcA""#,
                    ANY,
                    r#"MPRFeatures="vcB""#,
                    "k",
                    "",
                )],
            ));
            ids = "APPL PASS CONV";
        }
        grammar(
            &rules,
            ids,
            &[entry(
                "eA",
                r#"ruleFeatures="vcA""#,
                &[("bal", ""), ("ba", RIGHT_I)],
            )],
        )
    };
    assert_eq!(
        dead_of(&build(false), "eA"),
        [
            ("bal".into(), Continuation::Live),
            ("ba".into(), Continuation::Maybe)
        ]
    );
    let blocked = dead_continuations(&build(false))
        .into_iter()
        .find(|r| r.verdict == Continuation::Maybe)
        .unwrap();
    assert_eq!(
        blocked.reasons,
        ["environment_needs_class_rejected_neighbour"]
    );
    assert_eq!(blocked.blocked_by[0].blocking.names, ["VC-B"]);
    assert_eq!(
        dead_of(&build(true), "eA"),
        [
            ("bal".into(), Continuation::Live),
            ("ba".into(), Continuation::Live)
        ]
    );
}

#[test]
fn an_ungated_i_suffix_keeps_the_root_live_but_the_gated_one_is_reported() {
    let g = grammar(
        &[
            rule(
                "APPL",
                &[sub(
                    "appl",
                    r#"requiredMPRFeatures="vcB""#,
                    ANY,
                    "",
                    "i",
                    "",
                )],
            ),
            rule("IK", &[sub("ik", "", ANY, "", "ik", "")]),
        ],
        "APPL IK",
        &[entry("eA", r#"ruleFeatures="vcA""#, &[("ba", RIGHT_I)])],
    );
    let row = dead_continuations(&g).into_iter().next().unwrap();
    assert_eq!(row.verdict, Continuation::Live);
    assert_eq!(row.neighbour_rejections[0].blocking.names, ["VC-B"]);
    assert!(!row.via_converter);
}

/// Every allomorph a fixture word's parses or generations select.
fn selected_by(g: &Grammar, words: &[String]) -> HashSet<u32> {
    let morpher = Morpher::new(g, 200_000);
    let mut selected = HashSet::new();
    for word in words {
        let outcome = morpher.parse_word(word);
        for analysis in &outcome.structured {
            selected.extend(analysis.morph_occurrences.iter().map(|o| o.allomorph_id));
        }
        for signature in outcome.analyses.iter().map(|(s, _)| s) {
            let Some((root, others)) = seed_of(g, signature) else {
                continue;
            };
            let traced = morpher.generate_words_traced(
                root,
                &others,
                pg_featstruct::FeatureStruct::EMPTY,
                &TracedGenerateLimits {
                    work_limit: 100_000,
                    timeout: None,
                    max_seeds: 50,
                    max_trace_nodes: Some(100_000),
                },
            );
            for seed in &traced.seeds {
                for surface in &seed.surfaces {
                    if let Some(item) = seed.items.first() {
                        selected.extend(item.allomorph_id);
                    }
                    for step in &surface.steps {
                        if step.rule.kind != "morph" {
                            continue;
                        }
                        let Some(sub) = step.subrule else { continue };
                        for rule in &g.mrules {
                            let owns = rule
                                .morpheme()
                                .is_some_and(|m| g.morphemes[m.0 as usize].xml_key == step.rule.id);
                            if let (true, Some(allos)) = (owns, rule.affix_allomorphs()) {
                                if let Some(a) = allos.get(sub as usize) {
                                    selected.insert(a.id.0);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    selected
}

fn seed_of(
    g: &Grammar,
    signature: &str,
) -> Option<(pg_grammar_model::model::LexEntryId, Vec<GenMorpheme>)> {
    let mut root = None;
    let mut others = Vec::new();
    for token in signature.split('|').next()?.split('+') {
        let entry = g
            .entries
            .iter()
            .position(|e| g.morphemes[e.morpheme.0 as usize].morph_id.as_deref() == Some(token));
        let rule = g.mrules.iter().position(|r| {
            r.morpheme()
                .is_some_and(|m| g.morphemes[m.0 as usize].morph_id.as_deref() == Some(token))
        });
        match (entry, rule) {
            (Some(e), None) if root.is_none() => {
                root = Some(pg_grammar_model::model::LexEntryId(e as u32))
            }
            (None, Some(r)) => others.push(GenMorpheme::Rule(pg_grammar_model::model::MRuleId(
                r as u32,
            ))),
            _ => return None,
        }
    }
    Some((root?, others))
}

#[test]
fn no_fixture_word_selects_a_shadowed_or_dead_allomorph() {
    let mut fixtures = 0;
    let mut selected_total = 0;
    let (mut sh, mut maybe, mut sel) = (0, 0, 0);
    let (mut dead_proven, mut dead_maybe, mut live) = (0, 0, 0);
    let mut violations = Vec::new();
    for fixture in discover_scoped(ConformanceScope::All) {
        let words = fixture.load_words_yaml();
        if words.skip_in_generic_replay().is_some() {
            continue;
        }
        let Ok(mut g) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        words.morpher.apply_to(&mut g);
        fixtures += 1;
        let shadowed: HashSet<AllomorphId> = shadowed_allomorphs(&g)
            .iter()
            .flat_map(|m| m.alternates.iter())
            .inspect(|a| match a.verdict {
                Verdict::Shadowed => sh += 1,
                Verdict::MaybeShadowed => maybe += 1,
                Verdict::Selectable => sel += 1,
            })
            .filter(|a| a.verdict == Verdict::Shadowed)
            .map(|a| a.allomorph)
            .collect();
        let dead: HashSet<AllomorphId> = dead_continuations(&g)
            .iter()
            .inspect(|r| match r.verdict {
                Continuation::Proven => dead_proven += 1,
                Continuation::Maybe => dead_maybe += 1,
                Continuation::Live => live += 1,
            })
            .filter(|r| r.verdict == Continuation::Proven)
            .map(|r| r.allomorph)
            .collect();
        if shadowed.is_empty() && dead.is_empty() {
            continue;
        }
        let list: Vec<String> = words.words.iter().map(|w| w.word.clone()).collect();
        let selected = selected_by(&g, &list);
        selected_total += selected.len();
        for id in selected {
            if shadowed.contains(&AllomorphId(id)) {
                violations.push(format!(
                    "{}: shadowed allomorph {id} was selected",
                    fixture.label()
                ));
            }
            if dead.contains(&AllomorphId(id)) {
                violations.push(format!(
                    "{}: dead allomorph {id} was selected",
                    fixture.label()
                ));
            }
        }
    }
    let summary = format!(
        "fixtures {fixtures}: shadowed {sh}, maybe_shadowed {maybe}, selectable {sel}; dead proven {dead_proven}, maybe {dead_maybe}, live {live}; selections checked {selected_total}"
    );
    eprintln!("{summary}");
    if let Ok(path) = std::env::var("SHADOWED_SUMMARY_FILE") {
        std::fs::write(path, &summary).expect("summary file is writable");
    }
    assert!(fixtures > 100, "only {fixtures} fixtures were analysed");
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}
