//! A rule selector narrows what a parse pays for; excluding rules the analyses never use leaves them unchanged.

use pg_conformance_fixtures::{discover_scoped, ConformanceScope};
use pg_grammar::model::{MorphRuleDef, MorphemeId};
use pg_parse::{Morpher, ParseOptions, ParseOutcome};
use pg_rules::stratum::RuleRef;
use std::io::Write;

const WORK_CAP: usize = 2_000_000;
const WORDS_PER_FIXTURE: usize = 12;
const SELECTOR_SEEDS: u64 = 4;

fn signature(outcome: &ParseOutcome) -> Vec<(Vec<u32>, i32)> {
    let mut keys: Vec<_> = outcome
        .structured
        .iter()
        .map(|analysis| (analysis.morpheme_ids.clone(), analysis.root_morpheme_index))
        .collect();
    keys.sort();
    keys
}

/// Deterministic per-(seed, rule) coin: a selector that drops about a third of the morphological rules.
fn keeps(seed: u64, rule: u32) -> bool {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ u64::from(rule).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 29;
    !x.is_multiple_of(3)
}

/// Every local fixture word parsed plain, under admit-all, and under seeded random selectors, which may drop analyses.
#[test]
fn a_random_selector_returns_a_subset_of_the_plain_analyses_at_no_greater_cost() {
    // Setting PG_RESTRICTED_DUMP writes each parse's analyses so two builds can be diffed.
    let mut dump = std::env::var_os("PG_RESTRICTED_DUMP")
        .map(|path| std::fs::File::create(path).expect("dump file"));
    let (mut words, mut narrowed_cheaper) = (0usize, 0usize);
    for fixture in discover_scoped(ConformanceScope::Local) {
        let spec = fixture.load_words_yaml();
        if spec.expect_crash {
            continue;
        }
        let Ok(mut g) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        spec.morpher.apply_to(&mut g);
        let morpher = Morpher::new(&g, WORK_CAP);
        for entry in spec.words.iter().take(WORDS_PER_FIXTURE) {
            let word = entry.word.as_str();
            let opts = ParseOptions::default();
            let plain = morpher.parse_word_selected(word, &opts, None, None);
            if plain.capped {
                continue;
            }
            words += 1;
            let all = |_: RuleRef| true;
            let everything = morpher.parse_word_selected(word, &opts, None, Some(&all));
            assert_eq!(
                signature(&everything),
                signature(&plain),
                "{} {word}: admit-all analyses",
                fixture.label()
            );
            assert_eq!(
                everything.work_steps,
                plain.work_steps,
                "{} {word}: admit-all work",
                fixture.label()
            );
            if let Some(file) = dump.as_mut() {
                writeln!(
                    file,
                    "{}\t{word}\tplain\t{:?}",
                    fixture.label(),
                    signature(&plain)
                )
                .expect("dump");
            }
            for seed in 0..SELECTOR_SEEDS {
                let filter = |rule: RuleRef| match rule {
                    RuleRef::MRule(id) => keeps(seed, id.0),
                    _ => true,
                };
                let narrowed = morpher.parse_word_selected(word, &opts, None, Some(&filter));
                if let Some(file) = dump.as_mut() {
                    writeln!(
                        file,
                        "{}\t{word}\tseed{seed}\t{:?}",
                        fixture.label(),
                        signature(&narrowed)
                    )
                    .expect("dump");
                }
                if narrowed.capped {
                    continue;
                }
                let kept = signature(&narrowed);
                let full = signature(&plain);
                assert!(
                    kept.iter().all(|key| full.contains(key)),
                    "{} {word} seed {seed}: the selector produced an analysis the plain parse lacks",
                    fixture.label()
                );
                assert!(
                    narrowed.work_steps <= plain.work_steps,
                    "{} {word} seed {seed}: a selector made the parse cost more",
                    fixture.label()
                );
                narrowed_cheaper += usize::from(narrowed.work_steps < plain.work_steps);
            }
        }
    }
    assert!(words > 50, "only {words} words were compared");
    assert!(narrowed_cheaper > 0, "no selector ever saved work");
}

/// The morphological rules that own a morpheme of some analysis, plus every compounding rule.
fn rules_the_analyses_use(g: &pg_grammar::model::Grammar, plain: &ParseOutcome) -> Vec<bool> {
    let mut used: std::collections::HashSet<MorphemeId> = std::collections::HashSet::new();
    for analysis in &plain.structured {
        used.extend(analysis.morpheme_ids.iter().map(|&id| MorphemeId(id)));
    }
    g.mrules
        .iter()
        .map(|rule| match rule {
            MorphRuleDef::AffixProcess(def) => used.contains(&def.morpheme),
            MorphRuleDef::Realizational(def) => used.contains(&def.morpheme),
            MorphRuleDef::Compounding(_) => true,
        })
        .collect()
}

/// Excluding every rule the plain analyses do not use returns those analyses, each as often as before.
#[test]
fn excluding_unused_rules_returns_the_plain_analyses_with_the_same_multiplicity() {
    let (mut words, mut narrower, mut analysed) = (0usize, 0usize, 0usize);
    for fixture in discover_scoped(ConformanceScope::Local) {
        let spec = fixture.load_words_yaml();
        if spec.expect_crash {
            continue;
        }
        let Ok(mut g) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue;
        };
        spec.morpher.apply_to(&mut g);
        let morpher = Morpher::new(&g, WORK_CAP);
        for entry in spec.words.iter().take(WORDS_PER_FIXTURE) {
            let word = entry.word.as_str();
            let opts = ParseOptions::default();
            let plain = morpher.parse_word_selected(word, &opts, None, None);
            if plain.capped {
                continue;
            }
            words += 1;
            analysed += usize::from(!plain.structured.is_empty());
            let admitted = rules_the_analyses_use(&g, &plain);
            narrower += usize::from(admitted.iter().any(|&rule| !rule));
            let filter = |rule: RuleRef| match rule {
                RuleRef::MRule(id) => admitted[id.0 as usize],
                _ => true,
            };
            let restricted = morpher.parse_word_selected(word, &opts, None, Some(&filter));
            assert!(
                !restricted.capped,
                "{} {word}: restricted parse capped",
                fixture.label()
            );
            assert_eq!(
                signature(&restricted),
                signature(&plain),
                "{} {word}: restricted analyses",
                fixture.label()
            );
            assert!(
                restricted.work_steps <= plain.work_steps,
                "{} {word}: the restricted parse cost more",
                fixture.label()
            );
        }
    }
    assert!(words > 50, "only {words} words were compared");
    assert!(analysed > 20, "only {analysed} words had an analysis");
    assert!(narrower > 20, "only {narrower} words excluded a rule");
}
