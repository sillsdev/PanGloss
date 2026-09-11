//! Reader-audit trip-wire: `stratum.rs`'s `state_key` saturation is only sound while `apply_one_mrule`'s `>= max_apps` gate stays the field's one consuming reader.

#[test]
fn unapplied_rule_counts_has_no_reader_outside_state_key_and_the_max_apps_gate() {
    let stratum_src = include_str!("../src/stratum.rs");
    let morph_src = include_str!("../src/morph.rs");
    let count = |src: &str| src.matches("unapplied_rule_counts").count();

    assert_eq!(
        count(morph_src),
        0,
        "morph.rs must never reference unapplied_rule_counts -- analysis rule application reads \
         it only through stratum.rs's max_apps gate"
    );
    assert_eq!(
        count(stratum_src),
        2,
        "stratum.rs's current occurrences of `unapplied_rule_counts`: state_key's own build of \
         the key (1), and apply_one_mrule's `>= max_apps` gate (1). A changed count here means a \
         reader appeared or disappeared -- audit by hand whether state_key's max_apps saturation \
         (pg_memo's module doc; this file's state_key doc comment) is still recall-safe before \
         updating this number."
    );
}
