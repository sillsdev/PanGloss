use super::*;

mod v2_port_tests;

#[test]
fn display_is_exactly_the_message() {
    let w = Warning::new(
        "fwdata.dangling-reference",
        "phoneme \"00...\" does not resolve",
    );
    assert_eq!(w.to_string(), "phoneme \"00...\" does not resolve");
}

#[test]
fn deref_supports_str_methods_like_contains() {
    let w = Warning::new(
        "fwdata.dangling-reference",
        "dangling reference to Foo abc-123",
    );
    assert!(w.contains("abc-123"));
}

#[test]
fn code_is_independent_of_message_reword() {
    let original = Warning::new("fwdata.dangling-reference", "old wording of the same fact");
    let reworded = Warning::new("fwdata.dangling-reference", "new wording, same situation");
    assert_eq!(original.code, reworded.code);
    assert_ne!(original.message, reworded.message);
}

#[test]
fn different_situations_get_different_codes() {
    let dangling = Warning::new("fwdata.dangling-reference", "X does not resolve");
    let unexpected_class = Warning::new("fwdata.unexpected-class", "X has unexpected class Y");
    assert_ne!(dangling.code, unexpected_class.code);
}
