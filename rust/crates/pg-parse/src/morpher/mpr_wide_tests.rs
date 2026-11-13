//! #150/#291: a 140-MPR-feature grammar parses, owns its wide sets, and frees them when dropped.

use super::*;
use pg_grammar_model::model::mpr_table_is_live;

const ACCEPTED: [&str; 6] = ["kama", "kamako", "kamaku", "kamaru", "kamarubu", "lobefu"];
const REFUSED: [&str; 3] = ["kamafu", "kamari", "kamabu"];

fn load() -> pg_grammar::model::Grammar {
    let grammar = pg_grammar::load(include_str!("mpr_wide_test_grammar.xml")).unwrap();
    assert!(grammar.mpr_names.len() > 128);
    grammar
}

fn signatures(morpher: &Morpher) -> Vec<String> {
    ACCEPTED
        .iter()
        .chain(&REFUSED)
        .map(|w| morpher.parse_word(w).signature())
        .collect()
}

#[test]
fn parses_and_a_second_pass_adds_nothing_to_the_grammars_table() {
    let grammar = load();
    let morpher = Morpher::new(&grammar, usize::MAX);
    let first = signatures(&morpher);
    for (w, sig) in ACCEPTED.iter().zip(&first) {
        assert_ne!(sig, "-", "{w} must parse");
    }
    for (w, sig) in REFUSED.iter().zip(&first[ACCEPTED.len()..]) {
        assert_eq!(sig, "-", "{w} must be refused");
    }
    let table = &grammar.mpr_table;
    assert!(
        !table.is_empty(),
        "the 140-feature grammar must exercise wide sets"
    );
    let (len, bytes) = (table.len(), table.retained_bytes());
    assert_eq!(signatures(&morpher), first);
    assert_eq!((table.len(), table.retained_bytes()), (len, bytes));
}

#[test]
fn loading_and_dropping_wide_grammars_retains_nothing() {
    let mut sizes = Vec::new();
    for _ in 0..5 {
        let grammar = load();
        let morpher = Morpher::new(&grammar, usize::MAX);
        signatures(&morpher);
        let generation = grammar.mpr_table.generation();
        assert!(mpr_table_is_live(generation));
        sizes.push((grammar.mpr_table.len(), grammar.mpr_table.retained_bytes()));
        drop(morpher);
        drop(grammar);
        assert!(
            !mpr_table_is_live(generation),
            "a dropped grammar frees its table"
        );
    }
    assert!(
        sizes.windows(2).all(|w| w[0] == w[1]),
        "each load starts from nothing: {sizes:?}"
    );
}

#[test]
fn two_wide_grammars_parse_side_by_side_on_separate_threads() {
    let (a, b) = (load(), load());
    assert_ne!(a.mpr_table.generation(), b.mpr_table.generation());
    let expected = signatures(&Morpher::new(&a, usize::MAX));
    let (from_a, from_b) = std::thread::scope(|s| {
        let ta = s.spawn(|| {
            let m = Morpher::new(&a, usize::MAX);
            (0..20).map(|_| signatures(&m)).collect::<Vec<_>>()
        });
        let tb = s.spawn(|| {
            let m = Morpher::new(&b, usize::MAX);
            (0..20).map(|_| signatures(&m)).collect::<Vec<_>>()
        });
        (ta.join().unwrap(), tb.join().unwrap())
    });
    assert!(from_a.iter().chain(&from_b).all(|s| *s == expected));
    drop(a);
    assert_eq!(signatures(&Morpher::new(&b, usize::MAX)), expected);
}
