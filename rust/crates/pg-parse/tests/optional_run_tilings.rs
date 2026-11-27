//! Runs of identical optional nodes (a deleted vowel between consonants) in an analysis affix match are enumerated once per distinct tiling, with the same readings and far less work.

use pg_parse::morpher::Morpher;

fn parse(word: &str) -> (Vec<(String, String)>, usize) {
    let g = pg_grammar::load(include_str!("data/optional-run-tilings.xml")).expect("grammar loads");
    let out = Morpher::new(&g, usize::MAX / 2).parse_word(word);
    assert!(!out.capped && !out.capture_capped);
    (out.analyses, out.work_steps)
}

#[test]
fn canonical_tilings_keep_the_reduplicated_reading() {
    let (analyses, _) = parse("tuutuu");
    assert_eq!(analyses.len(), 1, "{analyses:?}");
    assert!(analyses[0].0.contains("REDV"), "{analyses:?}");
}

#[test]
fn canonical_tilings_keep_the_empty_result_and_cut_the_work() {
    // 35,534 steps before the change.
    let (analyses, work) = parse("piNtqa");
    assert!(analyses.is_empty(), "{analyses:?}");
    assert!(work < 10_000, "work {work}");
}

#[test]
fn longer_runs_of_restored_vowels_keep_the_empty_result_and_cut_the_work() {
    // 167,200 and 858,997 steps before the change.
    for word in ["ttuu", "thhaa"] {
        let (analyses, work) = parse(word);
        assert!(analyses.is_empty(), "{word}: {analyses:?}");
        assert!(work < 10_000, "{word}: work {work}");
    }
}
