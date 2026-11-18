use super::*;

fn group(match_type: MprGroupMatchType, output: MprGroupOutput, members: &[u32]) -> MprGroup {
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

fn set(bits: &[u32]) -> MprSet {
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
    assert!(!mpr_required_ok(&groups, &set(&[0, 1]), &set(&[0])));
    // Both present -> pass.
    assert!(mpr_required_ok(&groups, &set(&[0, 1]), &set(&[0, 1])));
}

#[test]
fn any_type_group_requires_only_one_member_present() {
    let groups = [group(
        MprGroupMatchType::Any,
        MprGroupOutput::Append,
        &[0, 1],
    )];
    assert!(mpr_required_ok(&groups, &set(&[0, 1]), &set(&[0])));
    assert!(mpr_required_ok(&groups, &set(&[0, 1]), &set(&[1])));
    assert!(!mpr_required_ok(&groups, &set(&[0, 1]), &set(&[])));
}

#[test]
fn ungrouped_required_features_use_all_semantics() {
    // Ungrouped required features use All semantics: both must be present, not just one.
    let groups: [MprGroup; 0] = [];
    assert!(!mpr_required_ok(&groups, &set(&[0, 1]), &set(&[0])));
    assert!(mpr_required_ok(&groups, &set(&[0, 1]), &set(&[0, 1])));
}

#[test]
fn excluded_any_type_group_fails_only_when_every_member_present() {
    let groups = [group(
        MprGroupMatchType::Any,
        MprGroupOutput::Append,
        &[0, 1],
    )];
    // Only one of the two excluded features present -> still passes (Any-excluded fails only when ALL members are present).
    assert!(mpr_excluded_ok(&groups, &set(&[0, 1]), &set(&[0])));
    assert!(!mpr_excluded_ok(&groups, &set(&[0, 1]), &set(&[0, 1])));
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
    let result = mpr_add_output(&groups, &current, &output);
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
    let result = mpr_add_output(&groups, &set(&[0]), &set(&[5]));
    assert_eq!(result, set(&[0, 5]));
}

#[test]
fn observed_mpr_gates_attribute_members_and_stop_before_unreached_groups() {
    let groups = [group(
        MprGroupMatchType::Any,
        MprGroupOutput::Append,
        &[1, 2],
    )];
    let mut seen = Vec::new();
    assert!(!mpr_match_observed(
        &groups,
        &set(&[0, 1, 2]),
        &set(&[0]),
        true,
        |bucket, blocked| seen.push((bucket, blocked))
    ));
    assert_eq!(seen, vec![(set(&[0]), set(&[0]))]);
    seen.clear();
    assert!(mpr_match_observed(
        &groups,
        &set(&[0, 1, 2]),
        &set(&[1]),
        true,
        |bucket, blocked| seen.push((bucket, blocked))
    ));
    assert_eq!(seen, vec![(set(&[0]), set(&[])), (set(&[1, 2]), set(&[]))]);
    seen.clear();
    assert!(!mpr_match_observed(
        &groups,
        &set(&[1, 2]),
        &set(&[1, 2]),
        true,
        |bucket, blocked| seen.push((bucket, blocked))
    ));
    assert_eq!(seen.last(), Some(&(set(&[1, 2]), set(&[1, 2]))));
}

#[test]
fn observed_wide_mpr_buckets_preserve_member_attribution_and_short_circuits() {
    let groups = [
        group(MprGroupMatchType::Any, MprGroupOutput::Append, &[64, 299]),
        group(
            MprGroupMatchType::All,
            MprGroupOutput::Append,
            &[64, 128, 299, 300],
        ),
    ];
    let test = set(&[1, 64, 128, 299, 300]);
    for (have, excluded, expected) in [
        (
            set(&[1, 64, 128]),
            false,
            vec![
                (None, set(&[1]), MprSet::EMPTY),
                (Some(0), set(&[64, 299]), MprSet::EMPTY),
                (Some(1), set(&[128, 300]), set(&[300])),
            ],
        ),
        (
            set(&[128]),
            true,
            vec![
                (None, set(&[1]), MprSet::EMPTY),
                (Some(0), set(&[64, 299]), MprSet::EMPTY),
                (Some(1), set(&[128, 300]), set(&[128])),
            ],
        ),
        (
            set(&[64, 299]),
            true,
            vec![
                (None, set(&[1]), MprSet::EMPTY),
                (Some(0), set(&[64, 299]), set(&[64, 299])),
            ],
        ),
    ] {
        let mut seen = Vec::new();
        assert!(!mpr_match_grouped_observed(
            &groups,
            &test,
            &have,
            excluded,
            |group, bucket, blocked| seen.push((group, bucket, blocked))
        ));
        assert_eq!(seen, expected);
    }
}

#[test]
fn wide_groups_partition_match_exclude_and_overwrite_without_aliases() {
    let groups = [
        group(
            MprGroupMatchType::Any,
            MprGroupOutput::Overwrite,
            &[63, 64, 255, 256],
        ),
        group(MprGroupMatchType::All, MprGroupOutput::Append, &[299, 300]),
    ];
    let required = set(&[63, 64, 299, 300]);
    assert!(mpr_required_ok(&groups, &required, &set(&[64, 299, 300])));
    assert!(!mpr_required_ok(&groups, &required, &set(&[0, 299, 300])));
    assert!(!mpr_required_ok(&groups, &required, &set(&[64, 299])));
    assert!(mpr_excluded_ok(&groups, &set(&[63, 64]), &set(&[63])));
    assert!(!mpr_excluded_ok(&groups, &set(&[63, 64]), &set(&[63, 64])));
    assert!(!mpr_excluded_ok(&groups, &set(&[299, 300]), &set(&[299])));
    assert_eq!(
        mpr_add_output(&groups, &set(&[0, 63, 255, 299]), &set(&[256, 300])),
        set(&[0, 256, 299, 300])
    );
}

#[test]
fn overlapping_wide_groups_keep_first_group_ownership() {
    let groups = [
        group(MprGroupMatchType::Any, MprGroupOutput::Append, &[64, 299]),
        group(
            MprGroupMatchType::All,
            MprGroupOutput::Overwrite,
            &[64, 128, 299],
        ),
    ];
    let required = set(&[64, 128, 299]);
    assert!(mpr_required_ok(&groups, &required, &set(&[64, 128])));
    assert!(!mpr_required_ok(&groups, &required, &set(&[64, 299])));
    assert!(mpr_excluded_ok(&groups, &set(&[64, 299]), &set(&[64])));
    assert!(!mpr_excluded_ok(
        &groups,
        &set(&[64, 299]),
        &set(&[64, 299])
    ));
}
