use super::*;

#[test]
fn decomposes_precomposed_latin() {
    // U+00E9 (é) decomposes to U+0065 U+0301 (e + combining acute).
    assert_eq!(nfd("\u{00e9}"), "e\u{0301}");
}

#[test]
fn ascii_is_unchanged() {
    assert_eq!(nfd("cinacemerwa"), "cinacemerwa");
}

#[test]
fn is_nfd_detects_precomposed_as_not_nfd() {
    assert!(!is_nfd("\u{00e9}")); // é (precomposed) is NFC, not NFD
    assert!(is_nfd("e\u{0301}")); // e + combining acute is already NFD
    assert!(is_nfd("cinacemerwa")); // pure ASCII is trivially NFD
}
