use super::*;

#[test]
fn a_signature_counts_its_entries_and_a_dash_is_none() {
    assert_eq!(reading_count("-"), 0);
    assert_eq!(reading_count("a|x"), 1);
    assert_eq!(reading_count("a|x;a|x;b|y"), 3, "repeats are Readings");
}

#[test]
fn ok_words_are_binned_and_other_statuses_are_counted_apart() {
    let tsv = "0\tw0\t1\tok\t-\n1\tw1\t1\tok\ta|x\n2\tw2\t1\tok\ta|x;b|y\n3\tw3\t1\tok\tc|z\n4\tw4\t1\tCAP\ta|x\n5\tw5\t1\tSKIPPED\t-\n6\tSTARTED\n";
    let (h, words) = tally(tsv, "t").unwrap();
    assert_eq!(words, 6);
    assert_eq!(h.by_readings, BTreeMap::from([(0, 1), (1, 2), (2, 1)]));
    assert_eq!(
        h.not_ok,
        BTreeMap::from([("CAP".to_string(), 1), ("SKIPPED".to_string(), 1)])
    );
}

#[test]
fn trailing_columns_do_not_change_the_count() {
    let (h, _) = tally("0\tw\t1\tok\ta|x;b|y\ttrue\t3\t9\n", "t").unwrap();
    assert_eq!(h.by_readings, BTreeMap::from([(2, 1)]));
}

#[test]
fn a_file_without_rows_or_with_a_bad_status_is_refused() {
    assert!(tally("0\tw\tSTARTED\n", "t")
        .unwrap_err()
        .contains("no result rows"));
    assert!(tally("0\tw\t1\tbogus\t-\n", "t")
        .unwrap_err()
        .contains("unexpected shape"));
}
