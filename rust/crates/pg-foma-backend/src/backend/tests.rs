use super::*;

/// `backend_for` must realize every `ALL_STRATEGIES` member as itself, and `ALL_BACKENDS` must cover each exactly once -- the closed-table half of the adapter/strategy correspondence.
#[test]
fn backend_for_and_all_backends_cover_every_strategy_exactly_once() {
    for &strategy in crate::strategy_coverage::ALL_STRATEGIES {
        assert_eq!(backend_for(strategy).strategy(), strategy);
    }
    let mut covered: Vec<EmissionStrategy> = ALL_BACKENDS
        .iter()
        .map(|backend| backend.strategy())
        .collect();
    covered.sort_by_key(|strategy| strategy.label());
    let mut expected: Vec<EmissionStrategy> = crate::strategy_coverage::ALL_STRATEGIES.to_vec();
    expected.sort_by_key(|strategy| strategy.label());
    assert_eq!(
        covered, expected,
        "ALL_BACKENDS must cover every strategy exactly once"
    );
}
