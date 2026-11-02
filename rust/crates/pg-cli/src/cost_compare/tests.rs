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
fn a_signature_change_on_an_ok_word_is_reported_beside_the_verdict() {
    let before = rows(&["0\ta\t1\tok\tx\t100", "1\tb\t1\tCAP\ty\t100"]);
    let after = rows(&["0\ta\t1\tok\tz\t100", "1\tb\t1\tCAP\ty\t100"]);
    let result = compare(&before, &after).unwrap();
    assert_eq!(result.parse_changes, 1);
    assert_eq!(result.excluded_words, 1);
    assert_eq!(result.status_changes, 0);
    assert_eq!((result.before, result.after), (100, 100));
    assert_eq!(result.verdict, "no clear change");
}

#[test]
fn a_word_that_newly_caps_makes_the_verdict_inconclusive_not_better() {
    let before = rows(&["0\ta\t1\tok\tx\t100", "1\tb\t1\tok\ty\t10000"]);
    let after = rows(&["0\ta\t1\tok\tx\t100", "1\tb\t1\tCAP\ty\t10"]);
    let result = compare(&before, &after).unwrap();
    assert_eq!(result.status_changes, 1);
    assert_eq!(result.excluded_words, 1);
    assert_eq!((result.before, result.after), (100, 100));
    assert_eq!(result.verdict, "inconclusive");
}

#[test]
fn an_all_changed_status_run_is_inconclusive_rather_than_an_error() {
    let before = rows(&["0\ta\t1\tok\tx\t100"]);
    let after = rows(&["0\ta\t1\tCAP\tx\t100"]);
    assert_eq!(compare(&before, &after).unwrap().verdict, "inconclusive");
}

#[test]
fn a_word_capped_on_both_sides_is_excluded_from_the_totals() {
    let before = rows(&["0\ta\t1\tok\tx\t100", "1\tb\t1\tCAP\ty\t5000"]);
    let after = rows(&["0\ta\t1\tok\tx\t50", "1\tb\t1\tCAP\ty\t9000"]);
    let result = compare(&before, &after).unwrap();
    assert_eq!((result.before, result.after), (100, 50));
    assert_eq!(result.excluded_words, 1);
    assert_eq!(result.verdict, "better");
}

#[test]
fn a_total_beyond_u64_is_refused() {
    let big = u64::MAX;
    let before = rows(&[
        &format!("0\ta\t1\tok\tx\t{big}"),
        &format!("1\tb\t1\tok\tx\t{big}"),
    ]);
    let err = compare(&before, &before).unwrap_err();
    assert!(err.contains("too large"), "{err}");
}

#[test]
fn a_row_with_a_tab_inside_the_word_is_refused_without_quoting_it() {
    let err = parse_rows("0\tsecret\tword\t1\tok\tsig\t9", "run.tsv").unwrap_err();
    assert!(err.contains("unexpected shape"), "{err}");
    assert!(!err.contains("secret") && !err.contains("word"), "{err}");
}
#[test]
fn different_word_lists_are_refused() {
    let before = rows(&["0\ta\t1\tok\tx\t100"]);
    let after = rows(&["0\tb\t1\tok\tx\t100"]);
    assert!(compare(&before, &after).is_err());
}
