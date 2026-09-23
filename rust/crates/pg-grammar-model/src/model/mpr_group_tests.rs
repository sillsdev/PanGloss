use super::*;

fn group(match_type: MprGroupMatchType, output: MprGroupOutput, members: &[u8]) -> MprGroup {
    let mut s = MprSet::EMPTY;
    for &m in members {
        s.insert(MprId(m));
    }
    MprGroup {
        name: None,
        match_type,
        output,
        members: s,
    }
}

fn set(bits: &[u8]) -> MprSet {
    let mut s = MprSet::EMPTY;
    for &b in bits {
        s.insert(MprId(b));
    }
    s
}

#[test]
fn all_type_group_requires_every_member_present() {
    let groups = [group(
        MprGroupMatchType::All,
        MprGroupOutput::Append,
        &[0, 1],
    )];
    // Rule requires both bit 0 and bit 1 (the whole group) -- only bit 0 present -> fail.
    assert!(!mpr_required_ok(&groups, set(&[0, 1]), set(&[0])));
    // Both present -> pass.
    assert!(mpr_required_ok(&groups, set(&[0, 1]), set(&[0, 1])));
}

#[test]
fn any_type_group_requires_only_one_member_present() {
    let groups = [group(
        MprGroupMatchType::Any,
        MprGroupOutput::Append,
        &[0, 1],
    )];
    assert!(mpr_required_ok(&groups, set(&[0, 1]), set(&[0])));
    assert!(mpr_required_ok(&groups, set(&[0, 1]), set(&[1])));
    assert!(!mpr_required_ok(&groups, set(&[0, 1]), set(&[])));
}

#[test]
fn ungrouped_required_features_use_all_semantics() {
    // Ungrouped required features use All semantics: both must be present, not just one.
    let groups: [MprGroup; 0] = [];
    assert!(!mpr_required_ok(&groups, set(&[0, 1]), set(&[0])));
    assert!(mpr_required_ok(&groups, set(&[0, 1]), set(&[0, 1])));
}

#[test]
fn excluded_any_type_group_fails_only_when_every_member_present() {
    let groups = [group(
        MprGroupMatchType::Any,
        MprGroupOutput::Append,
        &[0, 1],
    )];
    // Only one of the two excluded features present -> still passes (Any-excluded fails only when ALL members are present).
    assert!(mpr_excluded_ok(&groups, set(&[0, 1]), set(&[0])));
    assert!(!mpr_excluded_ok(&groups, set(&[0, 1]), set(&[0, 1])));
}

#[test]
fn overwrite_output_group_drops_unmentioned_members_append_does_not() {
    let groups = [
        group(MprGroupMatchType::All, MprGroupOutput::Overwrite, &[0, 1]),
        group(MprGroupMatchType::All, MprGroupOutput::Append, &[2, 3]),
    ];
    // current has bit 0 (from the overwrite group) and bit 2 (from the append group).
    let current = set(&[0, 2]);
    // Output sets bit 1 (same overwrite group) and bit 3 (same append group).
    let output = set(&[1, 3]);
    let result = mpr_add_output(&groups, current, output);
    // Overwrite group drops bit 0 (not in output) and adds bit 1; append group keeps bit 2 and adds bit 3.
    assert_eq!(result, set(&[1, 2, 3]));
}

#[test]
fn output_touching_no_group_is_a_plain_union() {
    let groups = [group(
        MprGroupMatchType::All,
        MprGroupOutput::Overwrite,
        &[0, 1],
    )];
    // Output bit 5 belongs to no group at all -> the overwrite group is untouched.
    let result = mpr_add_output(&groups, set(&[0]), set(&[5]));
    assert_eq!(result, set(&[0, 5]));
}
