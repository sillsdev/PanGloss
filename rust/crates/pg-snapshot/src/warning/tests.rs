use super::*;

mod v2_port_tests;

#[test]
fn display_is_exactly_the_message() {
    let w = Warning::new(
        ImportWarningCode::FwdataDanglingReference,
        "phoneme \"00...\" does not resolve",
    );
    assert_eq!(w.to_string(), "phoneme \"00...\" does not resolve");
}

#[test]
fn deref_supports_str_methods_like_contains() {
    let w = Warning::new(
        ImportWarningCode::FwdataDanglingReference,
        "dangling reference to Foo abc-123",
    );
    assert!(w.contains("abc-123"));
}

#[test]
fn code_is_independent_of_message_reword() {
    let original = Warning::new(
        ImportWarningCode::FwdataDanglingReference,
        "old wording of the same fact",
    );
    let reworded = Warning::new(
        ImportWarningCode::FwdataDanglingReference,
        "new wording, same situation",
    );
    assert_eq!(original.code, reworded.code);
    assert_ne!(original.message, reworded.message);
}

#[test]
fn different_situations_get_different_codes() {
    let dangling = Warning::new(
        ImportWarningCode::FwdataDanglingReference,
        "X does not resolve",
    );
    let unexpected_class = Warning::new(
        ImportWarningCode::FwdataUnexpectedClass,
        "X has unexpected class Y",
    );
    assert_ne!(dangling.code, unexpected_class.code);
}
