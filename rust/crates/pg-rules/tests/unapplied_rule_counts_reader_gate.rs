//! Reader-audit trip-wire: the state key's saturation of rule counts is only sound while `apply_one_mrule`'s `>= max_apps` gate stays the counts' one consuming reader.

#[test]
fn rule_counts_has_no_reader_outside_the_state_key_and_the_max_apps_gate() {
    let stratum_src = include_str!("../src/stratum.rs");
    let morph_src = include_str!("../src/morph.rs");
    let key_src = include_str!("../src/analysis_state_key.rs");
    let key_src = key_src
        .split("#[cfg(test)]")
        .next()
        .expect("split yields the text before the tests");
    let count = |src: &str| {
        src.matches("count_of(").count()
            + src.matches(".rule_counts()").count()
            + src.matches(".counted_rules()").count()
    };

    assert_eq!(
        count(morph_src),
        0,
        "morph.rs must never read rule counts -- analysis rule application reads them only \
         through stratum.rs's max_apps gate"
    );
    assert_eq!(
        count(stratum_src),
        1,
        "stratum.rs reads rule counts once: apply_one_mrule's `>= max_apps` gate. A changed \
         count means a reader appeared or disappeared -- audit by hand whether the state key's \
         max_apps saturation (`crate::analysis_state_key`'s module doc) is still recall-safe \
         before updating this number."
    );
    assert_eq!(
        count(key_src),
        3,
        "analysis_state_key.rs reads rule counts three times: `key_of` builds the saturated \
         counts (1) and `matches_word` compares a stored word against them in both directions \
         (2). Both saturate at max_apps; a new reader here must too."
    );
}
