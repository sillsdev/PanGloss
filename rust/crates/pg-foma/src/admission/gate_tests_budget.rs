use super::tests::{
    affix_grammar, batch, gated_signatures, plain_signatures, unsampled, LETTERS, ON,
};
use super::{decide, total_line, GateReport, Tally};
use crate::admission::tests::{load, prefix, rule, spec, suffix, CTX_ANY};
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

/// Words that all have an analysis but one, each with several affixes so a batch this short still affords the builds; the tiled stage has little to win.
fn words_with_one_miss(count: usize) -> Vec<String> {
    let roots = ["pat", "pit", "put", "lat", "lit"];
    let mut words: Vec<String> = (0..count)
        .map(|i| {
            let (p, s) = (["na", "ni", "no"], ["ba", "di", "go", "ka"]);
            format!(
                "{}{}{}{}{}",
                p[i % 3],
                p[(i + 1) % 3],
                roots[i % 5],
                s[i % 4],
                s[(i + 1) % 4]
            )
        })
        .collect();
    words[0] = "t".to_owned();
    words
}

fn report_of(g: &Grammar, words: &[String]) -> (GateReport, Vec<String>) {
    let morpher = Morpher::new(g, 2_000_000);
    let (gated, decided) = gated_signatures(g, &morpher, words, (ParseOptions::default(), 1));
    (decided.report, gated)
}

#[test]
fn a_build_over_its_budget_stays_off_reports_its_units_and_returns_the_plain_outcomes() {
    let g = affix_grammar();
    let words = batch(3000);
    let morpher = Morpher::new(&g, 2_000_000);
    let (report, gated) = report_of(&g, &words);
    let line = report.to_string();
    assert!(
        matches!(&report.admission, super::StageState::Off(why) if why.starts_with("construction-budget:")),
        "{line}"
    );
    assert!(report.admission_units > 0, "{line}");
    assert_eq!(
        report.construction_cost,
        report.admission_units * crate::admission::ADMISSION_UNIT_WEIGHT
    );
    assert!(report.net < 0, "{line}");
    assert_eq!(
        gated,
        plain_signatures(&morpher, &words, &ParseOptions::default())
    );
}

#[test]
fn a_tiled_build_over_its_budget_keeps_the_admission_and_reports_the_units_it_spent() {
    let g = affix_grammar();
    let words = words_with_one_miss(ON / 4);
    let morpher = Morpher::new(&g, 2_000_000);
    let (report, gated) = report_of(&g, &words);
    let line = report.to_string();
    assert!(report.admission.is_on(), "{line}");
    assert!(
        matches!(&report.tiled, super::StageState::Off(why) if why.contains("budget")),
        "{line}"
    );
    assert!(report.tiled_units > 0, "{line}");
    assert!(
        report.construction_cost
            >= report.admission_units * crate::admission::ADMISSION_UNIT_WEIGHT
                + report.tiled_units * crate::admission::TILED_UNIT_WEIGHT,
        "{line}"
    );
    assert_eq!(
        gated,
        plain_signatures(&morpher, &words, &ParseOptions::default())
    );
}

fn free_rule_grammar() -> Grammar {
    let mut rules = Vec::new();
    for (i, lit) in [
        "ba", "bi", "bo", "da", "di", "do", "ga", "gi", "go", "ka", "ki", "ko",
    ]
    .iter()
    .enumerate()
    {
        rules.push(suffix(i, lit));
    }
    for (i, lit) in ["na", "ni", "no"].iter().enumerate() {
        rules.push(prefix(12 + i, lit));
    }
    let modify =
        r#"<ModifyFromInput index="p0"><SimpleContext naturalClass="ncAny" /></ModifyFromInput>"#;
    rules.push(rule(15, &[CTX_ANY], &[modify.to_owned()]));
    load(&spec(LETTERS, rules, &["pat", "pit", "put", "lat", "lit"]))
}

#[test]
fn a_prospect_that_cannot_be_active_builds_nothing() {
    let g = free_rule_grammar();
    let words = batch(ON);
    let morpher = Morpher::new(&g, 2_000_000);
    let (report, gated) = report_of(&g, &words);
    let line = report.to_string();
    assert!(report.admission.is_on(), "{line}");
    assert!(
        matches!(&report.tiled, super::StageState::Off(why) if why.contains("admitted for every word")),
        "{line}"
    );
    assert_eq!(report.tiled_units, 0, "{line}");
    assert_eq!(
        gated,
        plain_signatures(&morpher, &words, &ParseOptions::default())
    );
}

#[test]
fn the_total_line_states_the_whole_cost_and_does_not_depend_on_the_threads() {
    let g = affix_grammar();
    let words = batch(ON);
    let morpher = Morpher::new(&g, 2_000_000);
    let opts = ParseOptions::default();
    let lines: Vec<String> = [1, 4]
        .iter()
        .map(|&threads| {
            let decided = decide(&g, &morpher, &opts, &words).expect("sound");
            let mut tally = Tally::default();
            decided
                .outcomes
                .iter()
                .for_each(|(_, o)| tally.add_plain(o));
            let todo = unsampled(&words, &decided);
            let rest: Vec<Tally> = std::thread::scope(|scope| {
                let chunks: Vec<_> = todo
                    .chunks(words.len().div_ceil(threads))
                    .map(|chunk| {
                        let prefilter = &decided.prefilter;
                        let morpher = &morpher;
                        scope.spawn(move || {
                            let mut tally = Tally::default();
                            for &(_, w) in chunk {
                                prefilter.parse_counted(morpher, w, &opts, &mut tally);
                            }
                            tally
                        })
                    })
                    .collect();
                chunks
                    .into_iter()
                    .map(|c| c.join().expect("thread"))
                    .collect()
            });
            rest.iter().for_each(|t| tally.add(t));
            assert_eq!(tally.words, words.len() as u64);
            assert_eq!(
                tally.restricted + tally.rejected,
                words.len() as u64 - decided.outcomes.len() as u64
            );
            total_line(&decided.report, &tally)
        })
        .collect();
    assert_eq!(lines[0], lines[1]);
    for field in [
        &format!("words={ON}"),
        "restricted=",
        "rejected=",
        "admit_steps=",
        "tiled_steps=",
        "filter_cost=",
        "outcome_cost=",
        "construction_cost=",
        "calibration_cost=",
        "total_cost=",
    ] {
        assert!(lines[0].contains(field), "{field} in {}", lines[0]);
    }
    assert!(!lines[0].contains("admit_steps=0\t"), "{}", lines[0]);
}

#[test]
fn a_gate_that_is_off_still_states_its_total_with_zero_filter_fields() {
    let g = affix_grammar();
    let words = batch(150);
    let morpher = Morpher::new(&g, 2_000_000);
    let decided = decide(&g, &morpher, &ParseOptions::default(), &words).expect("sound");
    let mut tally = Tally::default();
    for w in &words {
        decided
            .prefilter
            .parse_counted(&morpher, w, &ParseOptions::default(), &mut tally);
    }
    let line = total_line(&decided.report, &tally);
    assert!(
        line.contains(
            "words=150\trestricted=0\trejected=0\tadmit_steps=0\ttiled_steps=0\tfilter_cost=0\t"
        ),
        "{line}"
    );
}
