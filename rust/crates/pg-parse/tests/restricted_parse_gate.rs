//! A rule selector narrows what a parse pays for, never what it returns.

use pg_conformance_fixtures::{discover_scoped, ConformanceScope};
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

/// Every local fixture word parsed plain, under admit-all, and under seeded random selectors.
#[test]
fn a_selector_never_changes_which_analyses_a_parse_returns() {
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
