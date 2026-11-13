//! #150: a 140-MPR-feature grammar parses, and a second pass interns no new wide sets.

use super::*;
use pg_grammar_model::model::MprSet;

#[test]
fn grammar_with_140_mpr_features_parses_and_a_second_pass_interns_nothing() {
    let grammar = pg_grammar::load(include_str!("mpr_wide_test_grammar.xml")).unwrap();
    assert!(grammar.mpr_names.len() > 128);
    let morpher = Morpher::new(&grammar, usize::MAX);
    let accepted = ["kama", "kamako", "kamaku", "kamaru", "kamarubu", "lobefu"];
    let rejected = ["kamafu", "kamari", "kamabu"];
    let pass = || {
        for w in accepted {
            let r = morpher.parse_word(w);
            assert!(!r.analyses.is_empty(), "{w} must parse");
        }
        for w in rejected {
            assert!(
                morpher.parse_word(w).analyses.is_empty(),
                "{w} must be refused"
            );
        }
    };
    pass();
    let interned = MprSet::interned_wide_sets();
    assert!(
        interned > 0,
        "the 140-feature grammar must exercise wide sets"
    );
    pass();
    assert_eq!(MprSet::interned_wide_sets(), interned);
}
