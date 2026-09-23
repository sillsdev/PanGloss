use super::*;

#[test]
fn empty_result_is_dash() {
    assert_eq!(result_signature(&[]), "-");
}

#[test]
fn signatures_are_sorted_and_joined() {
    let got = result_signature(&[("b+c".into(), "surf".into()), ("a+c".into(), "surf".into())]);
    assert_eq!(got, "a+c|surf;b+c|surf");
}

#[test]
fn identical_rendered_signatures_are_kept_not_deduped() {
    // Duplicates here can be genuinely distinct analyses that happen to render identically, not noise.
    let got = result_signature(&[
        ("a+c".into(), "surf".into()),
        ("b+c".into(), "surf".into()),
        ("a+c".into(), "surf".into()),
    ]);
    assert_eq!(got, "a+c|surf;a+c|surf;b+c|surf");
}
