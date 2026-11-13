use super::*;

fn group(match_type: MprGroupMatchType, output: MprGroupOutput, members: &[u16]) -> MprGroup {
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

fn set(bits: &[u16]) -> MprSet {
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

fn high(bits: &[u16], base: u16) -> Vec<u16> {
    bits.iter().map(|b| b + base).collect()
}

#[test]
fn groups_behave_identically_for_members_beyond_the_inline_word() {
    for base in [0u16, 60, 63, 64, 130, 300] {
        let g = |t, o, m: &[u16]| group(t, o, &high(m, base));
        let s = |b: &[u16]| set(&high(b, base));
        let all = [g(MprGroupMatchType::All, MprGroupOutput::Append, &[0, 1])];
        assert!(!mpr_required_ok(&all, s(&[0, 1]), s(&[0])), "base {base}");
        assert!(mpr_required_ok(&all, s(&[0, 1]), s(&[0, 1])), "base {base}");
        let any = [g(MprGroupMatchType::Any, MprGroupOutput::Append, &[0, 1])];
        assert!(mpr_required_ok(&any, s(&[0, 1]), s(&[1])), "base {base}");
        assert!(!mpr_required_ok(&any, s(&[0, 1]), s(&[])), "base {base}");
        assert!(mpr_excluded_ok(&any, s(&[0, 1]), s(&[0])), "base {base}");
        assert!(
            !mpr_excluded_ok(&any, s(&[0, 1]), s(&[0, 1])),
            "base {base}"
        );
        let ow = [g(
            MprGroupMatchType::All,
            MprGroupOutput::Overwrite,
            &[0, 1],
        )];
        assert_eq!(
            mpr_add_output(&ow, s(&[0, 5]), s(&[1])),
            s(&[1, 5]),
            "base {base}"
        );
    }
}

#[test]
fn set_algebra_matches_a_reference_set_across_the_inline_and_wide_forms() {
    use std::collections::BTreeSet;
    let universe = [0u16, 1, 62, 63, 64, 65, 127, 128, 129, 200, 255, 256, 700];
    let mut seed = 0x9e37_79b9_u64;
    let mut pick = || {
        let mut v = BTreeSet::new();
        for &u in &universe {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            if seed >> 62 & 1 == 1 {
                v.insert(u);
            }
        }
        v
    };
    let build = |v: &BTreeSet<u16>| set(&v.iter().copied().collect::<Vec<_>>());
    for _ in 0..300 {
        let (a, b) = (pick(), pick());
        let (sa, sb) = (build(&a), build(&b));
        assert_eq!(sa.union(sb), build(&a.union(&b).copied().collect()));
        assert_eq!(
            sa.intersection(sb),
            build(&a.intersection(&b).copied().collect())
        );
        assert_eq!(
            sa.difference(sb),
            build(&a.difference(&b).copied().collect())
        );
        assert_eq!(sa.overlaps(sb), a.intersection(&b).next().is_some());
        assert_eq!(sa.is_subset_of(sb), a.is_subset(&b));
        assert_eq!(sa.is_empty(), a.is_empty());
        assert_eq!(sa.iter().map(|i| i.0).collect::<BTreeSet<_>>(), a);
        for &u in &universe {
            assert_eq!(sa.contains(MprId(u)), a.contains(&u));
        }
        // Canonical: the same members are the same set however they were reached.
        let high_only: BTreeSet<u16> = a.iter().copied().filter(|&x| x >= 63).collect();
        let low_only: BTreeSet<u16> = a.iter().copied().filter(|&x| x < 63).collect();
        assert_eq!(build(&low_only).union(build(&high_only)), sa);
        assert_eq!(sa.difference(build(&high_only)), build(&low_only));
    }
}

#[test]
fn serde_keeps_the_legacy_number_form_and_round_trips_wide_sets() {
    let small = set(&[0, 3, 62]);
    assert_eq!(
        serde_json::to_string(&small).unwrap(),
        (1u64 | 8 | 1 << 62).to_string()
    );
    assert_eq!(serde_json::from_str::<MprSet>("9").unwrap(), set(&[0, 3]));
    // A number written when ids up to 63 were stored in one word still reads as the same members.
    let legacy = serde_json::from_str::<MprSet>(&(1u64 << 63 | 1).to_string()).unwrap();
    assert_eq!(legacy, set(&[0, 63]));
    let wide = set(&[1, 64, 300]);
    let json = serde_json::to_string(&wide).unwrap();
    assert!(json.starts_with('['));
    assert_eq!(serde_json::from_str::<MprSet>(&json).unwrap(), wide);
}

#[test]
fn debug_prints_the_legacy_form_for_inline_sets() {
    assert_eq!(format!("{:?}", set(&[0, 2])), "MprSet(5)");
}
