//! Rule text foma cannot parse is a skipped rule (`None`) rather than a panic.

use super::*;

#[test]
fn unparseable_rule_text_is_none_and_valid_text_is_some() {
    let opts = FomaOptions::default();
    assert!(parse_rule_regex(&opts, "[a").is_none());
    assert!(parse_rule_regex(&opts, "a -> b").is_some());
}
