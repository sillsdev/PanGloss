use super::*;

#[test]
fn snapshot_of_untouched_counters_is_zero() {
    // Independent of `enabled()` state: a fresh thread's counters start zeroed.
    let s = AltYieldSnapshot::default();
    assert_eq!(s.canonical_alt_total, 0);
    assert_eq!(s.distinct_identities, 0);
}

#[test]
fn recorders_are_no_ops_when_env_var_is_unset() {
    // No `HC_ALT_YIELD` here, so `enabled()` caches `false` and every recorder is a no-op.
    record_canonical(1919);
    record_expansion(1919);
    let s = snapshot();
    assert_eq!(s.canonical_alt_total, 0);
    assert_eq!(s.expanded_total, 0);
}
