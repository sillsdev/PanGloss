//! Turning cost measurement on cannot change a parse, and the cost it reports follows the work done.

use pg_parse::Morpher;

#[global_allocator]
static ALLOC: pg_cost::CountingAllocator = pg_cost::CountingAllocator;

const GRAMMAR: &str = include_str!("../../../tools/fixtures/supplied-lexicon-host-smoke.xml");
const WORDS: [&str; 4] = ["milu", "milusi", "kolota", "zzz"];

#[test]
fn parses_and_work_steps_are_identical_with_measurement_on_and_off() {
    let grammar = pg_grammar::load(GRAMMAR).unwrap();
    let morpher = Morpher::new(&grammar, 10_000_000);

    let off: Vec<_> = WORDS.iter().map(|word| morpher.parse_word(word)).collect();
    assert!(off.iter().all(|outcome| outcome.cost.is_none()));

    pg_cost::enable().expect("this binary installs the counting allocator");
    let on: Vec<_> = WORDS.iter().map(|word| morpher.parse_word(word)).collect();

    for ((word, off), on) in WORDS.iter().zip(&off).zip(&on) {
        assert_eq!(on.analyses, off.analyses, "{word}");
        assert_eq!(on.work_steps, off.work_steps, "{word}");
        assert_eq!(on.capped, off.capped, "{word}");
        assert_eq!(on.invalid_shape, off.invalid_shape, "{word}");
        let counts = on.cost.expect("measurement is on");
        assert_eq!(counts.work, on.work_steps as u64, "{word}");
        assert!(counts.cost() > 0, "{word}");
    }
    assert!(!on[1].analyses.is_empty(), "the suffixed word parses");
    assert!(
        on[1].cost.unwrap().cost() > on[0].cost.unwrap().cost(),
        "a word that applies a rule costs more than a bare root"
    );
}
