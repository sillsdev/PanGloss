//! `PhMetathesisRule.StrucChange` is read the way LibLCM's `GetStrucChangeIndices` reads it.

use pg_snapshot::phonology::PhonologicalRule;

const PROJECT: &str = include_str!("../../pg-parse/tests/data/fw-metathesis/project.fwdata");
const AUTHORED: &str = "<Run ws=\"en\">0 2 -1 1 -1</Run>";

fn import(struc_change: &str) -> (Vec<PhonologicalRule>, Vec<String>) {
    assert!(PROJECT.contains(AUTHORED), "fixture StrucChange moved");
    let source = PROJECT.replace(AUTHORED, &format!("<Run ws=\"en\">{struc_change}</Run>"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.fwdata");
    std::fs::write(&path, source).unwrap();
    let (snapshot, report) = pg_fwdata::import_file(&path).unwrap();
    let warnings = report.warnings.iter().map(|w| w.code.clone()).collect();
    (snapshot.phonology.rules, warnings)
}

fn switches(rules: &[PhonologicalRule]) -> (i32, i32) {
    let [PhonologicalRule::Metathesis(rule)] = rules else {
        panic!("expected one metathesis rule, got {}", rules.len())
    };
    (rule.left_switch_index, rule.right_switch_index)
}

#[test]
fn switches_come_from_fields_three_and_one() {
    let (rules, warnings) = import("0 2 -1 1 -1");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(switches(&rules), (1, 2));
}

#[test]
fn middle_side_suffix_is_stripped_like_liblcm() {
    let (rules, warnings) = import("-1 2 1:L 0 -1");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(switches(&rules), (0, 2));
}

#[test]
fn an_unset_switch_is_kept_so_the_compiler_can_skip_it_as_hcloader_does() {
    let (rules, _) = import("0 -1 -1 -1 -1");
    assert_eq!(switches(&rules), (-1, -1));
}

#[test]
fn a_malformed_struc_change_skips_the_rule_with_a_warning() {
    let (rules, warnings) = import("0 2 -1");
    assert!(rules.is_empty());
    assert_eq!(warnings, ["fwdata.missing-required-field"]);
}
