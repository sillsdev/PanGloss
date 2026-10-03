use super::*;
use pg_shape::ShapeBuilder;

fn w() -> Word {
    Word::new(ShapeBuilder::new().finish(), StratumId(0))
}

#[test]
fn final_template_state_is_a_two_value_copyable_key_component() {
    let mut a = w();
    let mut b = w();
    a.flags.final_template_state = FinalTemplateState::None;
    b.flags.final_template_state = FinalTemplateState::NonTemplate;
    assert_ne!(a.dedup_key(), b.dedup_key());
    assert_eq!(FinalTemplateState::default(), FinalTemplateState::None);
    let copied = FinalTemplateState::NonTemplate;
    assert_eq!(copied, FinalTemplateState::NonTemplate);
}

#[test]
fn dedup_key_keeps_selected_allomorph_and_annotation_order() {
    let mut first = w();
    let mut second = w();
    first.morphs = vec![MorphRecord::new(AllomorphId(1), MorphemeId(2), 0)];
    second.morphs = vec![MorphRecord::new(AllomorphId(3), MorphemeId(2), 0)];
    assert_ne!(first.dedup_key(), second.dedup_key());

    second.morphs[0].allomorph = AllomorphId(1);
    second.morphs[0].order = 1;
    assert_ne!(first.dedup_key(), second.dedup_key());
}

#[test]
fn dedup_key_ignores_procedural_passed_over_state() {
    let mut first = w();
    let mut second = w();
    first.morphs = vec![MorphRecord::new(AllomorphId(1), MorphemeId(2), 0)];
    second.morphs = first.morphs.clone();
    first.morphs[0].passed_over = Some(vec![2, 4].into_boxed_slice());
    assert_eq!(first.dedup_key(), second.dedup_key());
}

#[test]
fn shared_alternative_tree_cannot_expand_past_the_search_cap() {
    let mut branch = w();
    for _ in 0..12 {
        let child = Rc::new(branch);
        branch = w();
        branch.alternatives = vec![child.clone(), child];
    }
    let budget = pg_fst::work::WorkBudget::new(32);
    let _scope = budget.enter();
    let expanded = branch.expand_alternatives();
    assert!(
        expanded.is_empty(),
        "interrupted expansion must discard its partial list"
    );
    assert!(budget.capped());
    assert_eq!(budget.used(), 32);
}
