use super::*;

fn rows(lines: &[&str]) -> BTreeMap<usize, Row> {
    parse_rows(&lines.join("\n"), "test").unwrap()
}

#[test]
fn the_cost_is_the_last_field_of_every_row_layout() {
    for line in [
        "0\tw\t1\tok\tsig\t9",
        "0\tw\t1\tok\tsig\ttrue\t9",
        "0\tw\t1\tok\tsig\t3\t4\t9",
        "0\tw\t1\tok\tsig\tfalse\t3\t4\t9",
    ] {
        assert_eq!(parse_row(line).unwrap().unwrap().1.cost, 9, "{line}");
    }
}

#[test]
fn a_row_without_a_cost_is_refused_rather_than_read_as_work_steps() {
    for line in [
        "0\tw\t1\tok\tsig",
        "0\tw\t1\tok\tsig\ttrue",
        "0\tw\t1\tok\tsig\t3\t4",
        "0\tw\t1\tok\tsig\ttrue\t3\t4",
    ] {
        assert!(parse_row(line).is_err(), "{line}");
    }
}

#[test]
fn a_started_sentinel_is_not_a_row() {
    assert_eq!(parse_row("0\tw\tSTARTED").unwrap(), None);
}

#[test]
fn a_timed_out_word_refuses_the_comparison() {
    let err = parse_rows("0\tw\t1\tTIMEOUT\t-\t9", "run.tsv").unwrap_err();
    assert!(err.contains("timed out"), "{err}");
}

#[test]
fn the_band_edges_read_as_documented() {
    assert_eq!(verdict(-5.0), "better");
    assert_eq!(verdict(5.0), "worse");
    assert_eq!(verdict(-4.99), "probably better");
    assert_eq!(verdict(-2.0), "probably better");
    assert_eq!(verdict(2.0), "probably worse");
    assert_eq!(verdict(1.99), "no clear change");
    assert_eq!(verdict(0.0), "no clear change");
}

#[test]
fn totals_are_compared_so_one_expensive_word_outweighs_many_cheap_ones() {
    let before = rows(&[
        "0\ta\t1\tok\ts\t100",
        "1\tb\t1\tok\ts\t100",
        "2\tc\t1\tok\ts\t10000",
    ]);
    let after = rows(&[
        "0\ta\t1\tok\ts\t50",
        "1\tb\t1\tok\ts\t50",
        "2\tc\t1\tok\ts\t11000",
    ]);
    let result = compare(&before, &after).unwrap();
    assert_eq!(result.before, 10200);
    assert_eq!(result.after, 11100);
    assert_eq!(result.verdict, "worse");
}

#[test]
fn parse_changes_and_capped_words_are_reported_beside_the_verdict() {
    let before = rows(&["0\ta\t1\tok\tx\t100", "1\tb\t1\tCAP\ty\t100"]);
    let after = rows(&["0\ta\t1\tok\tz\t100", "1\tb\t1\tCAP\ty\t100"]);
    let result = compare(&before, &after).unwrap();
    assert_eq!(result.parse_changes, 1);
    assert_eq!(result.capped_words, 1);
    assert_eq!(result.verdict, "no clear change");
}

#[test]
fn different_word_lists_are_refused() {
    let before = rows(&["0\ta\t1\tok\tx\t100"]);
    let after = rows(&["0\tb\t1\tok\tx\t100"]);
    assert!(compare(&before, &after).is_err());
}
