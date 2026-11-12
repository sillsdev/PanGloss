use super::tests::{affix_grammar, batch, plain_signatures, supplied_root, LETTERS};
use super::{decide_forced, total_line, Tally};
use crate::admission::tests::{copy, load, rule, spec, ANY_ONE};
use pg_parse::{Morpher, ParseOptions};

/// A batch far below the shortest gated length, which the forced gate takes anyway.
const SHORT: usize = 60;

fn tally_of(decided: &super::Decided, morpher: &Morpher, words: &[String]) -> (Vec<String>, Tally) {
    let opts = ParseOptions::default();
    let mut tally = Tally::default();
    let signatures = words
        .iter()
        .map(|w| {
            decided
                .prefilter
                .parse_counted(morpher, w, &opts, &mut tally)
                .signature()
        })
        .collect();
    (signatures, tally)
}

#[test]
fn forcing_the_stages_equals_plain_on_a_short_batch_and_counts_restricted_and_rejected_words() {
    let g = affix_grammar();
    let morpher = Morpher::new(&g, 2_000_000);
    let words = batch(SHORT);
    let decided = decide_forced(&g, &morpher, &ParseOptions::default(), words.len());
    let report = &decided.report;
    assert!(report.admission.is_on(), "{report}");
    assert!(decided.outcomes.is_empty(), "no word is parsed to decide");
    assert_eq!((report.warmup_words, report.calibration_words), (0, 0));
    let (signatures, tally) = tally_of(&decided, &morpher, &words);
    assert_eq!(
        signatures,
        plain_signatures(&morpher, &words, &ParseOptions::default())
    );
    assert_eq!(tally.words as usize, SHORT);
    assert_eq!(tally.restricted + tally.rejected, SHORT as u64);
    assert!(tally.restricted > 0, "{tally:?}");
    assert_eq!(
        tally.rejected > 0,
        report.tiled.is_on(),
        "{report} {tally:?}"
    );
    let line = total_line(report, &tally);
    assert!(line.contains(&format!("words={SHORT}\t")), "{line}");
}

#[test]
fn forcing_with_a_supplied_root_leaves_both_stages_off_with_the_reason() {
    let g = affix_grammar();
    let overlay = supplied_root(&g);
    let morpher = Morpher::new_with_overlay(&g, 2_000_000, &overlay);
    let words = batch(SHORT);
    let decided = decide_forced(&g, &morpher, &ParseOptions::default(), words.len());
    for state in [&decided.report.admission, &decided.report.tiled] {
        assert!(
            state.to_string().contains("supplied-root overlay"),
            "{state}"
        );
    }
    let (signatures, tally) = tally_of(&decided, &morpher, &words);
    assert_eq!(
        signatures,
        plain_signatures(&morpher, &words, &ParseOptions::default())
    );
    assert_eq!((tally.restricted, tally.rejected), (0, 0));
}

#[test]
fn a_refused_admission_build_leaves_the_batch_plain_with_the_reason() {
    let g = load(&spec(
        LETTERS,
        vec![rule(0, &[ANY_ONE], &[copy(0), copy(0)])],
        &["pat", "lit"],
    ));
    let morpher = Morpher::new(&g, 2_000_000);
    let words = batch(SHORT);
    let decided = decide_forced(&g, &morpher, &ParseOptions::default(), words.len());
    assert!(!decided.prefilter.admission_on());
    assert!(
        decided
            .report
            .admission
            .to_string()
            .starts_with("off(no-usable-piece:"),
        "{}",
        decided.report
    );
    let (signatures, _) = tally_of(&decided, &morpher, &words);
    assert_eq!(
        signatures,
        plain_signatures(&morpher, &words, &ParseOptions::default())
    );
}
