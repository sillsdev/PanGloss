use super::*;
use crate::trail::Trail;
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
fn analysis_allomorphs_survive_dedup_and_alternative_expansion() {
    let mut source = w();
    source.trail = Trail::from_slots([(Some(MRuleId(1)), Some(AllomorphId(10)))]);
    let mut alternative = source.clone();
    alternative.trail = Trail::from_slots([(Some(MRuleId(1)), Some(AllomorphId(11)))]);
    assert_ne!(source.dedup_key(), alternative.dedup_key());
    source.fold_alternative(alternative);

    let mut descendant = source.clone_without_alternatives();
    descendant.source = Some(Rc::new(source));
    descendant
        .trail
        .push(Some(MRuleId(2)), Some(AllomorphId(12)));
    let expanded = descendant.expand_alternatives();
    assert_eq!(expanded.len(), 2);
    let mut trails: Vec<_> = expanded.into_iter().map(|w| w.trail.allomorphs()).collect();
    trails.sort();
    assert_eq!(
        trails,
        vec![
            vec![Some(AllomorphId(10)), Some(AllomorphId(12))],
            vec![Some(AllomorphId(11)), Some(AllomorphId(12))]
        ]
    );
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
        let child = branch;
        branch = w();
        branch.fold_alternative(child.clone());
        branch.fold_alternative(child);
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

/// `WordKey::matches` replaces building `w.dedup_key()` just to compare it, so it must say exactly what `==` on the keys says, field by field.
#[test]
fn matches_agrees_with_key_equality() {
    use pg_shape::ShapeBuilder;
    let mut base = w();
    base.morphs = vec![MorphRecord::new(AllomorphId(1), MorphemeId(2), 0)];
    base.trail = Trail::from_slots([(Some(MRuleId(3)), None)]);
    let mut nested = w();
    nested.morphs = vec![MorphRecord::new(AllomorphId(9), MorphemeId(8), 1)];
    *base.non_heads_mut() = vec![nested];

    let mut variants: Vec<Word> = vec![base.clone()];
    let mut push = |edit: &dyn Fn(&mut Word)| {
        let mut v = base.clone();
        edit(&mut v);
        variants.push(v);
    };
    push(&|v| v.non_head_app_index = 4);
    push(&|v| v.stratum = StratumId(1));
    push(&|v| v.root_allomorph = Some(AllomorphId(5)));
    push(&|v| v.mrule_app_index = 7);
    push(&|v| v.flags.is_last_applied_rule_final = Some(true));
    push(&|v| v.flags.final_template_state = FinalTemplateState::NonTemplate);
    push(&|v| v.set_root_runtime_id(Some("r".to_owned())));
    push(&|v| v.trail.push(None, None));
    push(&|v| v.trail = Trail::from_slots([(Some(MRuleId(3)), Some(AllomorphId(2)))]));
    push(&|v| v.trail.push_count_only(MRuleId(8)));
    push(&|v| v.applied_realizational_rules_mut().push(MorphemeId(6)));
    push(&|v| v.morphs[0].order = 2);
    push(&|v| v.morphs[0].allomorph = AllomorphId(4));
    push(&|v| {
        v.morphs
            .push(MorphRecord::new(AllomorphId(1), MorphemeId(2), 3))
    });
    push(&|v| v.non_heads_mut()[0].morphs[0].order = 5);
    push(&|v| v.non_heads_mut().clear());
    push(&|v| {
        let mut sb = ShapeBuilder::new();
        sb.push_segment(1);
        v.shape = sb.finish();
    });
    push(&|v| v.syn_fs = FeatureStruct::EMPTY);
    push(&|v| *v.applied_phonological_rules_mut() = vec![(StratumId(0), PRuleId(1))]);
    for a in &variants {
        for b in &variants {
            assert_eq!(
                a.dedup_key().matches(b),
                a.dedup_key() == b.dedup_key(),
                "matches disagrees with key equality"
            );
        }
    }
    assert!(variants.len() > 15);
}
