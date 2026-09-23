use super::*;

#[test]
fn policy_defaults_to_pruning_off() {
    let policy = FinalTemplateAnalysisPolicy::default();
    assert!(!policy.enforce);
    assert!(!policy.all_templates_final);
}

#[test]
fn analysis_state_uses_the_actual_invocation_role() {
    assert_eq!(
        analysis_state_after_mrule(RuleInvocationRole::Ordinary, false),
        FinalTemplateState::NonTemplate
    );
    assert_eq!(
        analysis_state_after_mrule(RuleInvocationRole::TemplateSlot, false),
        FinalTemplateState::None
    );
}

#[test]
fn realizational_rules_clear_the_analysis_state_in_either_role() {
    assert_eq!(
        analysis_state_after_mrule(RuleInvocationRole::Ordinary, true),
        FinalTemplateState::None
    );
    assert_eq!(
        analysis_state_after_mrule(RuleInvocationRole::TemplateSlot, true),
        FinalTemplateState::None
    );
}
