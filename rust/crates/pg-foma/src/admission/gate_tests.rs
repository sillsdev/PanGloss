use super::decide::decide_with;
use super::{decide, parse_cost, restricted_parse, GateError, Prefilter};
use crate::admission::tests::{copy, load, prefix, rule, spec, suffix, ANY_ONE};
use pg_featstruct::FeatureStruct;
use pg_grammar::model::{Grammar, MprSet, StratumId};
use pg_parse::{Morpher, ParseOptions, RootAuthority, SuppliedRoot, SuppliedRootOverlay};

/// Words in a batch long enough that the budget, a share of projected parse cost, affords the small grammars' builds.
pub(super) const ON: usize = 64000;

pub(super) const LETTERS: &str = "abdgiklmnoprtu";

/// Twelve suffix rules and three prefix rules over two-letter literals, and five roots.
pub(super) fn affix_grammar() -> Grammar {
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
    load(&spec(LETTERS, rules, &["pat", "pit", "put", "lat", "lit"]))
}

#[global_allocator]
static COUNTING: pg_cost::CountingAllocator = pg_cost::CountingAllocator;

/// Turns the pg-cost meter on, as `pangloss batch --prefilter auto` does, so the gate prices parses in full parser work.
fn meter_on() {
    pg_cost::enable().expect("the counting allocator is installed");
}

/// Bare roots, one-affix words, two-affix words and words no analysis explains.
pub(super) fn batch(count: usize) -> Vec<String> {
    meter_on();
    let roots = ["pat", "pit", "put", "lat", "lit"];
    let suffixes = ["ba", "di", "go", "ka"];
    let prefixes = ["na", "ni", "no"];
    (0..count)
        .map(|i| {
            let root = roots[i % roots.len()];
            match i % 6 {
                0 => root.to_owned(),
                1 => format!("{root}{}", suffixes[i % suffixes.len()]),
                2 => format!("{}{root}", prefixes[i % prefixes.len()]),
                3 => format!("{}{root}{}", prefixes[i % 3], suffixes[i % 4]),
                4 => format!("{root}{root}"),
                _ => format!("{}{}", suffixes[i % 4], root),
            }
        })
        .collect()
}

pub(super) fn plain_signatures(
    morpher: &Morpher,
    words: &[String],
    opts: &ParseOptions,
) -> Vec<String> {
    words
        .iter()
        .map(|w| morpher.parse_word_opts(w, opts).signature())
        .collect()
}

/// The words the gate did not parse, with their indices.
pub(super) fn unsampled<'a>(
    words: &'a [String],
    decided: &super::Decided,
) -> Vec<(usize, &'a str)> {
    let sampled: std::collections::HashSet<usize> =
        decided.outcomes.iter().map(|(j, _)| *j).collect();
    (0..words.len())
        .filter(|j| !sampled.contains(j))
        .map(|j| (j, words[j].as_str()))
        .collect()
}

/// The gate's outcomes for every word: the sampled words at their indices, then `Prefilter::parse` on `threads` threads for the rest.
pub(super) fn gated_signatures(
    g: &Grammar,
    morpher: &Morpher,
    words: &[String],
    (opts, threads): (ParseOptions, usize),
) -> (Vec<String>, super::Decided) {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .stack_size(1 << 28)
        .build()
        .unwrap();
    let decided = pool
        .install(|| decide(g, morpher, &opts, words))
        .expect("sound");
    let mut out: Vec<String> = vec![String::new(); words.len()];
    for (j, outcome) in &decided.outcomes {
        out[*j] = outcome.signature();
    }
    let todo = unsampled(words, &decided);
    let rest: Vec<(usize, String)> = std::thread::scope(|scope| {
        let chunks: Vec<_> = todo
            .chunks(todo.len().div_ceil(threads).max(1))
            .map(|chunk| {
                let prefilter = &decided.prefilter;
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|&(j, w)| (j, prefilter.parse(morpher, w, &opts).signature()))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        chunks.into_iter().flat_map(|c| c.join().unwrap()).collect()
    });
    for (j, signature) in rest {
        out[j] = signature;
    }
    (out, decided)
}

#[test]
fn prefilter_is_shared_across_threads() {
    fn sync<T: Sync + Send>() {}
    sync::<Prefilter>();
}

#[test]
fn a_helpful_grammar_turns_on_and_keeps_every_analysis() {
    let g = affix_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let words = batch(ON);
    let plain = plain_signatures(&morpher, &words, &ParseOptions::default());
    assert!(plain.iter().any(|s| s != "-"), "the batch has analyses");
    for threads in [1, 4] {
        let (gated, decided) =
            gated_signatures(&g, &morpher, &words, (ParseOptions::default(), threads));
        assert!(decided.report.admission.is_on(), "{}", decided.report);
        assert!(decided.prefilter.admission_on());
        assert_eq!(gated, plain, "threads {threads}: {}", decided.report);
        assert!(decided.report.net > 0, "{}", decided.report);
    }
}

#[test]
fn the_decision_and_its_report_line_do_not_depend_on_the_run_or_the_threads() {
    let g = affix_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let words = batch(ON);
    let lines: Vec<String> = [1, 4, 1]
        .iter()
        .map(|&t| {
            gated_signatures(&g, &morpher, &words, (ParseOptions::default(), t))
                .1
                .report
                .to_string()
        })
        .collect();
    assert_eq!(lines[0], lines[1]);
    assert_eq!(lines[0], lines[2]);
    assert!(lines[0].starts_with("admission=on\t"), "{}", lines[0]);
}

#[test]
fn a_short_batch_is_off_with_plain_outcomes() {
    let g = affix_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let words = batch(511);
    let (gated, decided) = gated_signatures(&g, &morpher, &words, (ParseOptions::default(), 1));
    assert!(!decided.prefilter.admission_on());
    assert!(
        decided
            .report
            .to_string()
            .starts_with("admission=off(batch of 511 words is shorter than 512"),
        "{}",
        decided.report
    );
    assert!(decided.outcomes.is_empty());
    assert_eq!(
        gated,
        plain_signatures(&morpher, &words, &ParseOptions::default())
    );
}

pub(super) fn supplied_root(g: &Grammar) -> SuppliedRootOverlay {
    let root = SuppliedRoot {
        entry_id: "supplied".into(),
        realization_id: "supplied:sig".into(),
        lexical_spelling: "lat".into(),
        gloss: "supplied".into(),
        syn_fs: FeatureStruct::EMPTY,
        mpr: MprSet::EMPTY,
        stratum: StratumId(0),
        authority: RootAuthority::Supplied,
    };
    SuppliedRootOverlay::build(g, vec![root]).expect("overlay")
}

#[test]
fn a_supplied_root_turns_the_gate_off_and_an_empty_overlay_does_not() {
    let g = affix_grammar();
    let words = batch(ON);
    let opts = ParseOptions::default();
    let overlay = supplied_root(&g);
    let morpher = Morpher::new_with_overlay(&g, 2_000_000, &overlay);
    let decided = decide(&g, &morpher, &opts, &words).expect("sound");
    assert!(!decided.prefilter.admission_on());
    assert!(decided.report.to_string().contains("supplied-root overlay"));
    for word in &words[..8] {
        assert_eq!(
            decided.prefilter.parse(&morpher, word, &opts).signature(),
            morpher.parse_word_opts(word, &opts).signature()
        );
    }
    let empty = SuppliedRootOverlay::empty(&g);
    let morpher = Morpher::new_with_overlay(&g, 2_000_000, &empty);
    let decided = decide(&g, &morpher, &opts, &words).expect("sound");
    assert!(decided.report.admission.is_on(), "{}", decided.report);
}

#[test]
fn a_grammar_whose_rules_are_all_free_is_off_with_the_refusal() {
    let g = load(&spec(
        LETTERS,
        vec![rule(0, &[ANY_ONE], &[copy(0), copy(0)])],
        &["pat", "lit"],
    ));
    let morpher = Morpher::new(&g, 2_000_000);
    let words = batch(ON);
    let (gated, decided) = gated_signatures(&g, &morpher, &words, (ParseOptions::default(), 1));
    assert!(!decided.prefilter.admission_on());
    assert!(
        decided
            .report
            .to_string()
            .starts_with("admission=off(no-usable-piece:"),
        "{}",
        decided.report
    );
    assert_eq!(
        gated,
        plain_signatures(&morpher, &words, &ParseOptions::default())
    );
}

#[test]
fn a_filter_that_saves_nothing_stays_off_and_keeps_the_analyses() {
    let g = load(&spec(LETTERS, vec![suffix(0, "a")], &["pat", "lat", "mba"]));
    let morpher = Morpher::new(&g, 2_000_000);
    let words: Vec<String> = (0..ON)
        .map(|i| ["pata", "lata", "mbaa"][i % 3].to_owned())
        .collect();
    let (gated, decided) = gated_signatures(&g, &morpher, &words, (ParseOptions::default(), 1));
    assert!(!decided.prefilter.admission_on(), "{}", decided.report);
    assert!(
        decided.report.to_string().contains("calibration saving"),
        "{}",
        decided.report
    );
    assert!(
        decided.report.net < 0 && decided.report.saving == 0,
        "{}",
        decided.report
    );
    assert_eq!(
        gated,
        plain_signatures(&morpher, &words, &ParseOptions::default())
    );
}

#[test]
fn a_restricted_parse_that_loses_an_analysis_is_a_hard_error() {
    let g = affix_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let words = batch(ON);
    let lossy = |m: &Morpher, word: &str, admitted: &[bool], opts: &ParseOptions| {
        let mut outcome = restricted_parse(m, word, admitted, opts);
        outcome.analyses.clear();
        outcome.structured.clear();
        outcome
    };
    let error = decide_with(&g, &morpher, &ParseOptions::default(), &words, &lossy)
        .err()
        .expect("disagreement");
    let GateError::Disagreement { word, stage } = &error;
    assert!(words.contains(word));
    assert_eq!(*stage, "restricted parse");
    assert!(error.to_string().contains(word.as_str()));
}

#[test]
fn a_word_that_reaches_the_work_cap_takes_the_restricted_outcome_with_every_plain_analysis() {
    let g = affix_grammar();
    let word = "nipatgo";
    let big = Morpher::new(&g, 2_000_000);
    let opts = ParseOptions::default();
    let probe = |m: &Morpher| m.parse_word_opts(word, &opts);
    let full = probe(&big);
    let admitted = {
        let words = batch(ON);
        let decided = decide(&g, &big, &opts, &words).expect("sound");
        assert!(decided.prefilter.admission_on());
        decided
    };
    let narrowed = admitted.prefilter.parse(&big, word, &opts);
    assert!(
        narrowed.work_steps < full.work_steps,
        "{} {}",
        narrowed.work_steps,
        full.work_steps
    );
    let cap = (narrowed.work_steps + full.work_steps) / 2;
    let capped = Morpher::new(&g, cap);
    let plain_capped = probe(&capped);
    assert!(plain_capped.capped, "cap {cap} of {}", full.work_steps);
    let used = admitted.prefilter.parse(&capped, word, &opts);
    assert!(!used.capped);
    assert!(plain_capped
        .analyses
        .iter()
        .all(|a| used.analyses.contains(a)));
    assert_eq!(used.signature(), full.signature());
    assert!(parse_cost(&used) < parse_cost(&plain_capped));
}

#[test]
fn a_batch_of_capped_words_decides_on_the_restricted_outcomes() {
    let g = affix_grammar();
    let opts = ParseOptions::default();
    let big = Morpher::new(&g, 2_000_000);
    let words: Vec<String> = (0..ON).map(|_| "nipatgo".to_owned()).collect();
    let full = big.parse_word_opts(&words[0], &opts);
    let restricted_steps = {
        let decided = decide(&g, &big, &opts, &batch(ON)).expect("sound");
        decided.prefilter.parse(&big, &words[0], &opts).work_steps
    };
    let capped = Morpher::new(&g, (full.work_steps + restricted_steps) / 2);
    let decided = decide(&g, &capped, &opts, &words).expect("sound");
    assert!(decided.report.admission.is_on(), "{}", decided.report);
    assert!(
        decided.outcomes.iter().all(|(_, o)| o.capped),
        "calibration words keep their plain outcome"
    );
    let used = decided.prefilter.parse(&capped, &words[0], &opts);
    assert!(!used.capped);
    assert_eq!(used.signature(), full.signature());
}
