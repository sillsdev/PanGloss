use foma::options::FomaOptions;
use pg_conformance_fixtures::require_fixture;
use pg_foma::replace::{compile_rewrite_rule, rewrite_rule_is_lowerable};
use pg_grammar::model::{Grammar, PatternNode, PhonRuleDef};

fn grammar() -> Grammar {
    let grammar = pg_grammar::load(
        &require_fixture("edge-cases", "right-to-left-bounded-quantifier-rewrite")
            .load_grammar_xml(),
    )
    .unwrap();
    let PhonRuleDef::Rewrite(rule) = &grammar.prules[0] else {
        panic!("rewrite fixture")
    };
    assert!(rewrite_rule_is_lowerable(&grammar, rule));
    assert!(compile_rewrite_rule(&FomaOptions::default(), &grammar, rule).is_some());
    grammar
}

fn repeat(grammar: &mut Grammar) -> &mut PatternNode {
    let PhonRuleDef::Rewrite(rule) = &mut grammar.prules[0] else {
        panic!("rewrite fixture")
    };
    rule.subrules[0]
        .right_env
        .as_mut()
        .unwrap()
        .nodes
        .iter_mut()
        .find(|node| matches!(node, PatternNode::Quantifier { .. }))
        .expect("environment repetition")
}

fn assert_refused(grammar: &Grammar) {
    let PhonRuleDef::Rewrite(rule) = &grammar.prules[0] else {
        panic!("rewrite fixture")
    };
    assert!(!rewrite_rule_is_lowerable(grammar, rule));
    assert!(compile_rewrite_rule(&FomaOptions::default(), grammar, rule).is_none());
}

#[test]
fn inverted_repeat_is_permanently_refused() {
    let mut grammar = grammar();
    let PatternNode::Quantifier { min, max, .. } = repeat(&mut grammar) else {
        unreachable!()
    };
    *min = 3;
    *max = Some(2);
    assert_refused(&grammar);
}

#[test]
fn empty_repeat_is_permanently_refused() {
    let mut grammar = grammar();
    let PatternNode::Quantifier { children, .. } = repeat(&mut grammar) else {
        unreachable!()
    };
    children.clear();
    assert_refused(&grammar);
}

#[test]
fn no_owning_table_is_permanently_refused() {
    let mut grammar = grammar();
    for stratum in &mut grammar.strata {
        stratum.prules.clear();
    }
    assert_refused(&grammar);
}
