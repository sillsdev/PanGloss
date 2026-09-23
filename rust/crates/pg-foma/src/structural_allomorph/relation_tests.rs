use super::*;
use foma::apply::{apply_down, apply_init};

fn marked(
    id: u32,
    shape_id: &'static str,
    members: Vec<Vec<&str>>,
    runs: Vec<Vec<&str>>,
    output_segments: Vec<&str>,
    zone: MarkerZone,
) -> (MorphologyRewrite, MarkerZone) {
    (
        MorphologyRewrite::MarkedStructural {
            shape_id,
            recipe: MorphologyRecipe {
                refs: match shape_id {
                    "AmharicInteriorInsertion" => vec![0, 1, 2],
                    "AmharicInitialVowelReplacement" | "AdjacentInitialDrop" => vec![1],
                    "AdjacentTerminalDrop" => vec![0],
                    "AmharicTerminalModify" => vec![0, 1],
                    _ => vec![],
                },
                literal_runs: runs
                    .into_iter()
                    .map(|run| run.into_iter().map(str::to_owned).collect())
                    .collect(),
                output_segments: output_segments.into_iter().map(str::to_owned).collect(),
                translated_input_members: members
                    .into_iter()
                    .map(|part| part.into_iter().map(str::to_owned).collect())
                    .collect(),
            },
            zone_requirement: ZoneRequirement::Caller,
            provenance: RewriteProvenance {
                allomorph: AllomorphId(id),
                source_table: TableId(0),
                active_table: TableId(0),
            },
        },
        zone,
    )
}

#[test]
fn technical_marker_predicate_accepts_high_plane_and_rejects_foreign_inputs() {
    let relation = CompiledMorphologyRelation::from_classified([marked(
        0x8000,
        "AdjacentInitialDrop",
        vec![vec!["a"], vec!["b"]],
        vec![],
        vec![],
        MarkerZone::Prefix,
    )])
    .unwrap();
    let known = relation.marked_input(AllomorphId(0x8000), "ab").unwrap();
    assert!(matches!(
        relation.apply(&known),
        MorphologyRelationResult::Recipe { .. }
    ));
    let foreign = format!("a{}", char::from_u32(0x100001).unwrap());
    assert!(matches!(
        relation.apply(&foreign),
        MorphologyRelationResult::Rejected {
            reason_id: "foreign-marker",
            ..
        }
    ));
}

#[test]
fn marker_namespace_boundaries_are_closed_and_all_u16_bindings_are_injective() {
    let first_a = marker_binding_for(
        MarkerKey {
            allomorph: AllomorphId(0),
            zone: MarkerZone::Prefix,
        },
        ZoneRequirement::Caller,
    )
    .unwrap();
    let final_a = marker_binding_for(
        MarkerKey {
            allomorph: AllomorphId(0xFFFE),
            zone: MarkerZone::Prefix,
        },
        ZoneRequirement::Caller,
    )
    .unwrap();
    let final_a_suffix = marker_binding_for(
        MarkerKey {
            allomorph: AllomorphId(0xFFFE),
            zone: MarkerZone::Suffix,
        },
        ZoneRequirement::Caller,
    )
    .unwrap();
    let final_b = marker_binding_for(
        MarkerKey {
            allomorph: AllomorphId(0xFFFF),
            zone: MarkerZone::Prefix,
        },
        ZoneRequirement::Caller,
    )
    .unwrap();
    let final_b_suffix = marker_binding_for(
        MarkerKey {
            allomorph: AllomorphId(0xFFFF),
            zone: MarkerZone::Suffix,
        },
        ZoneRequirement::Caller,
    )
    .unwrap();
    let last_a = marker_binding_for(
        MarkerKey {
            allomorph: AllomorphId(0x7FFE),
            zone: MarkerZone::Suffix,
        },
        ZoneRequirement::Caller,
    )
    .unwrap();
    let first_b = marker_binding_for(
        MarkerKey {
            allomorph: AllomorphId(0x7FFF),
            zone: MarkerZone::Prefix,
        },
        ZoneRequirement::Caller,
    )
    .unwrap();
    let last_b = marker_binding_for(
        MarkerKey {
            allomorph: AllomorphId(0xFFFD),
            zone: MarkerZone::Suffix,
        },
        ZoneRequirement::Caller,
    )
    .unwrap();

    assert_eq!(first_a.symbol as u32, 0xF0000);
    assert_eq!(last_a.symbol as u32, 0xFFFFD);
    assert_eq!(first_b.symbol as u32, 0x100000);
    assert_eq!(last_b.symbol as u32, 0x10FFFD);
    assert_eq!(final_a.symbol as u32, 0xFFFFE);
    assert_eq!(final_a_suffix.symbol as u32, 0xFFFFF);
    assert_eq!(final_b.symbol as u32, 0x10FFFE);
    assert_eq!(final_b_suffix.symbol as u32, 0x10FFFF);
    assert!(is_technical_marker(char::from_u32(0xFFFFE).unwrap()));
    assert!(is_technical_marker(char::from_u32(0xFFFFF).unwrap()));
    assert!(is_technical_marker(char::from_u32(0x10FFFE).unwrap()));
    assert!(is_technical_marker(char::from_u32(0x10FFFF).unwrap()));

    for (id, expected_prefix, expected_suffix) in [
        (0_u32, 0xF0000_u32, 0xF0001_u32),
        (1, 0xF0002, 0xF0003),
        (0x7FFE, 0xFFFFC, 0xFFFFD),
        (0x7FFF, 0x100000, 0x100001),
        (0xFFFD, 0x10FFFC, 0x10FFFD),
    ] {
        assert_eq!(
            marker_binding_for(
                MarkerKey {
                    allomorph: AllomorphId(id),
                    zone: MarkerZone::Prefix,
                },
                ZoneRequirement::Caller,
            )
            .unwrap()
            .symbol as u32,
            expected_prefix
        );
        assert_eq!(
            marker_binding_for(
                MarkerKey {
                    allomorph: AllomorphId(id),
                    zone: MarkerZone::Suffix,
                },
                ZoneRequirement::Caller,
            )
            .unwrap()
            .symbol as u32,
            expected_suffix
        );
    }
    assert_eq!(
        marker_binding_for(
            MarkerKey {
                allomorph: AllomorphId(0x10000),
                zone: MarkerZone::Prefix,
            },
            ZoneRequirement::Caller,
        ),
        Err(MarkerBindingError::InvalidScalar)
    );

    let mut symbols = HashSet::new();
    for id in 0..=u16::MAX {
        for zone in [MarkerZone::Prefix, MarkerZone::Suffix] {
            let binding = marker_binding_for(
                MarkerKey {
                    allomorph: AllomorphId(id as u32),
                    zone,
                },
                ZoneRequirement::Caller,
            )
            .unwrap();
            assert!(symbols.insert(binding.symbol));
        }
    }
    assert_eq!(symbols.len(), (u16::MAX as usize + 1) * 2);
}

#[test]
fn seg_alphabet_bmp_tokens_are_not_relation_markers() {
    for code in 0xE000..=0xE003 {
        assert!(!is_technical_marker(char::from_u32(code).unwrap()));
    }
}

#[test]
fn terminal_markers_parse_delete_and_compose_on_both_sides() {
    let opts = FomaOptions::default();
    for id in [0xFFFE_u32, 0xFFFF_u32] {
        for zone in [MarkerZone::Prefix, MarkerZone::Suffix] {
            let marker = marker_binding_for(
                MarkerKey {
                    allomorph: AllomorphId(id),
                    zone,
                },
                ZoneRequirement::Caller,
            )
            .unwrap()
            .symbol;
            let marker_text = marker.to_string();
            let delete = fsm_parse_regex(&opts, &format!("{marker} -> 0"), None, None)
                .expect("terminal marker must be accepted as UTF-8 Foma input");
            let mut delete_handle = apply_init(&delete);
            assert_eq!(
                apply_down(&mut delete_handle, Some(&marker_text)),
                Some(String::new()),
                "marker deletion must consume terminal marker {id:#X}/{zone:?}"
            );

            let rewrite_input = match zone {
                MarkerZone::Prefix => format!("{marker} a -> {marker} b"),
                MarkerZone::Suffix => format!("a {marker} -> b {marker}"),
            };
            let rewrite = fsm_parse_regex(&opts, &rewrite_input, None, None)
                .expect("both-side terminal marker rewrite must parse");
            let input = match zone {
                MarkerZone::Prefix => format!("{marker}a"),
                MarkerZone::Suffix => format!("a{marker}"),
            };
            let direct_output = match zone {
                MarkerZone::Prefix => format!("{marker}b"),
                MarkerZone::Suffix => format!("b{marker}"),
            };
            let mut direct_handle = apply_init(&rewrite);
            assert_eq!(
                apply_down(&mut direct_handle, Some(&input)),
                Some(direct_output),
                "direct rewrite must preserve terminal marker {id:#X}/{zone:?}"
            );
            let composed = fsm_compose(&opts, rewrite, delete);
            let mut composed_handle = apply_init(&composed);
            assert_eq!(
                apply_down(&mut composed_handle, Some(&input)),
                Some("b".to_owned()),
                "composed terminal marker rewrite must realize {id:#X}/{zone:?}"
            );
        }
    }
}

#[test]
fn terminal_markers_drive_marker_free_semantic_recipes() {
    for id in [0xFFFE_u32, 0xFFFF_u32] {
        let relation = CompiledMorphologyRelation::from_classified([marked(
            id,
            "AdjacentTerminalDrop",
            vec![vec!["a"], vec!["b"]],
            vec![vec!["x"]],
            vec![],
            MarkerZone::Suffix,
        )])
        .unwrap();
        let input = relation.marked_input(AllomorphId(id), "ab").unwrap();
        let MorphologyRelationResult::Recipe {
            outputs,
            consumed_markers,
            ..
        } = relation.apply(&input)
        else {
            panic!("terminal marker {id:#X} must select its recipe");
        };
        assert_eq!(outputs, BTreeSet::from(["ax".to_owned()]));
        assert_eq!(consumed_markers, 1);
        assert_eq!(
            relation.fired_recipe_count_for(AllomorphId(id), MarkerZone::Suffix),
            1
        );
        assert!(outputs.iter().all(|output| {
            output
                .chars()
                .all(|character| !is_technical_marker(character))
        }));
    }
}

#[test]
fn terminal_marker_relation_rejects_foreign_and_multiple_markers() {
    let relation = CompiledMorphologyRelation::from_classified([marked(
        0xFFFE,
        "AdjacentTerminalDrop",
        vec![vec!["a"], vec!["b"]],
        vec![vec!["x"]],
        vec![],
        MarkerZone::Suffix,
    )])
    .unwrap();
    let marker = relation
        .marker_binding_for_zone(AllomorphId(0xFFFE), MarkerZone::Suffix)
        .unwrap()
        .symbol;
    assert!(matches!(
        relation.apply(&format!("ab{marker}{marker}")),
        MorphologyRelationResult::Rejected {
            reason_id: "duplicate-marker",
            ..
        }
    ));
    assert!(matches!(
        relation.apply(&format!("ab{marker}{}", char::from_u32(0x100001).unwrap())),
        MorphologyRelationResult::Rejected {
            reason_id: "multiple-markers",
            ..
        }
    ));
    assert!(matches!(
        relation.apply(&format!("ab{}", char::from_u32(0x100001).unwrap())),
        MorphologyRelationResult::Rejected {
            reason_id: "foreign-marker",
            ..
        }
    ));
}

#[test]
fn probe_shapes_preserve_arbitrary_multi_segment_sequences() {
    let cases = [
        (
            1,
            marked(
                1,
                "AdjacentTerminalDrop",
                vec![vec!["a"], vec!["b"]],
                vec![vec!["x"]],
                vec![],
                MarkerZone::Suffix,
            ),
            "acb",
            "acx",
            MarkerZone::Suffix,
        ),
        (
            2,
            marked(
                2,
                "AdjacentInitialDrop",
                vec![vec!["a"], vec!["b"]],
                vec![],
                vec![],
                MarkerZone::Prefix,
            ),
            "abc",
            "bc",
            MarkerZone::Prefix,
        ),
        (
            3,
            marked(
                3,
                "AmharicInitialVowelReplacement",
                vec![vec!["a"], vec!["b"]],
                vec![vec!["p"]],
                vec![],
                MarkerZone::Prefix,
            ),
            "abc",
            "pbc",
            MarkerZone::Prefix,
        ),
        (
            4,
            marked(
                4,
                "AmharicTerminalModify",
                vec![vec!["a"], vec!["c"]],
                vec![],
                vec!["x", "y"],
                MarkerZone::Suffix,
            ),
            "abc",
            "abx",
            MarkerZone::Suffix,
        ),
    ];
    for (id, input, base, expected, zone) in cases {
        let relation = CompiledMorphologyRelation::from_classified([input]).unwrap();
        let marked = relation
            .marked_input_for_zone(AllomorphId(id), zone, base)
            .unwrap();
        let result = relation.apply(&marked);
        let outputs = match result {
            MorphologyRelationResult::Recipe { outputs, .. } => outputs,
            other => panic!("expected recipe result, got {other:?}"),
        };
        assert!(outputs.contains(expected));
    }
}

#[test]
fn relation_enumerates_multicodepoint_active_members() {
    let cases = [
        (
            12,
            marked(
                12,
                "AdjacentInitialDrop",
                vec![vec!["sy"], vec!["a"]],
                vec![],
                vec![],
                MarkerZone::Prefix,
            ),
            "sya",
            BTreeSet::from(["a".to_owned()]),
        ),
        (
            13,
            marked(
                13,
                "AdjacentTerminalDrop",
                vec![vec!["a"], vec!["sy"]],
                vec![vec!["x"]],
                vec![],
                MarkerZone::Suffix,
            ),
            "asy",
            BTreeSet::from(["ax".to_owned()]),
        ),
        (
            14,
            marked(
                14,
                "AmharicInitialVowelReplacement",
                vec![vec!["sy"], vec!["a"]],
                vec![vec!["p"]],
                vec![],
                MarkerZone::Prefix,
            ),
            "sya",
            BTreeSet::from(["pa".to_owned()]),
        ),
        (
            15,
            marked(
                15,
                "AmharicTerminalModify",
                vec![vec!["a"], vec!["sy"]],
                vec![],
                vec!["x"],
                MarkerZone::Suffix,
            ),
            "asyb",
            BTreeSet::from(["axb".to_owned()]),
        ),
    ];
    for (id, classified, base, expected) in cases {
        let relation = CompiledMorphologyRelation::from_classified([classified]).unwrap();
        let input = relation
            .marked_input_for_zone(AllomorphId(id), MarkerZone::Prefix, base)
            .or_else(|_| relation.marked_input_for_zone(AllomorphId(id), MarkerZone::Suffix, base))
            .unwrap();
        let MorphologyRelationResult::Recipe { outputs, .. } = relation.apply(&input) else {
            panic!("multi-codepoint member must produce a recipe");
        };
        assert_eq!(outputs, expected);
    }
}

#[test]
fn interior_relation_partitions_multicodepoint_members_as_tokens() {
    let relation = CompiledMorphologyRelation::from_classified([marked(
        16,
        "AmharicInteriorInsertion",
        vec![vec!["a"], vec!["sy"], vec!["b"]],
        vec![vec!["x"], vec!["y"]],
        vec![],
        MarkerZone::Suffix,
    )])
    .unwrap();
    let input = relation.marked_input(AllomorphId(16), "asyb").unwrap();
    let MorphologyRelationResult::Recipe { outputs, .. } = relation.apply(&input) else {
        panic!("multi-codepoint interior member must produce a recipe");
    };
    assert!(outputs.contains("axsyyb"));
}

#[test]
fn interior_probe_inserts_runs_across_arbitrary_partitions() {
    let relation = CompiledMorphologyRelation::from_classified([marked(
        5,
        "AmharicInteriorInsertion",
        vec![vec!["a"], vec!["b"], vec!["c"]],
        vec![vec!["x"], vec!["y"]],
        vec![],
        MarkerZone::Suffix,
    )])
    .unwrap();
    let input = relation.marked_input(AllomorphId(5), "abcd").unwrap();
    let outputs = match relation.apply(&input) {
        MorphologyRelationResult::Recipe { outputs, .. } => outputs,
        other => panic!("expected recipe result, got {other:?}"),
    };
    assert!(outputs.contains("axbycd"));
}

#[test]
fn interior_recall_keeps_scalar_partition_with_overlapping_members() {
    let relation = CompiledMorphologyRelation::from_classified([marked(
        19,
        "AmharicInteriorInsertion",
        vec![vec!["a", "aa"], vec!["b"], vec!["c"]],
        vec![vec!["x"], vec!["y"]],
        vec![],
        MarkerZone::Suffix,
    )])
    .unwrap();
    let input = relation.marked_input(AllomorphId(19), "aab").unwrap();
    let MorphologyRelationResult::Recipe { outputs, .. } = relation.apply(&input) else {
        panic!("overlapping members must retain scalar fallback recall");
    };
    assert!(outputs.contains("axayb"));
}

#[test]
fn overlapping_interior_probe_rejects_before_unbounded_enumeration() {
    let relation = CompiledMorphologyRelation::from_classified([marked(
        17,
        "AmharicInteriorInsertion",
        vec![vec!["a", "aa"], vec!["a", "aa"], vec!["a", "aa"]],
        vec![vec!["x"], vec!["y"]],
        vec![],
        MarkerZone::Suffix,
    )])
    .unwrap();
    let input = relation
        .marked_input(AllomorphId(17), &"a".repeat(64))
        .unwrap();
    assert!(matches!(
        relation.apply(&input),
        MorphologyRelationResult::ResourceRejected {
            reason_id: "probe-work-budget" | "probe-allocation-bytes",
            consumed_markers: 0,
            work,
            ..
        } if work > 0
    ));
    assert_eq!(
        relation.fired_recipe_count_for(AllomorphId(17), MarkerZone::Suffix),
        0
    );
}

#[test]
fn deep_scalar_segmentation_is_refused_before_stack_growth() {
    let relation = CompiledMorphologyRelation::from_classified([marked(
        24,
        "AmharicInteriorInsertion",
        vec![vec!["needle"], vec!["needle"], vec!["needle"]],
        vec![vec!["x"], vec!["y"]],
        vec![],
        MarkerZone::Suffix,
    )])
    .unwrap();
    let input = relation
        .marked_input(AllomorphId(24), &"a".repeat(RELATION_PROBE_DEPTH_CAP + 8))
        .unwrap();
    assert!(matches!(
        relation.apply(&input),
        MorphologyRelationResult::ResourceRejected {
            reason_id: "probe-depth-budget",
            consumed_markers: 0,
            depth,
            ..
        } if depth == RELATION_PROBE_DEPTH_CAP
    ));
    assert_eq!(
        relation.fired_recipe_count_for(AllomorphId(24), MarkerZone::Suffix),
        0
    );
}

#[test]
fn oversized_no_match_input_is_resource_rejected_without_fire() {
    let relation = CompiledMorphologyRelation::from_classified([marked(
        20,
        "AmharicTerminalModify",
        vec![vec!["a"], vec!["needle"]],
        vec![],
        vec!["x"],
        MarkerZone::Suffix,
    )])
    .unwrap();
    let input = relation
        .marked_input(AllomorphId(20), &"z".repeat(RELATION_PROBE_WORK_CAP + 1))
        .unwrap();
    assert!(matches!(
        relation.apply(&input),
        MorphologyRelationResult::ResourceRejected {
            reason_id: "probe-work-budget",
            consumed_markers: 0,
            ..
        }
    ));
    assert_eq!(
        relation.fired_recipe_count_for(AllomorphId(20), MarkerZone::Suffix),
        0
    );
}

#[test]
fn oversized_no_match_member_table_is_resource_rejected_without_fire() {
    let (
        MorphologyRewrite::MarkedStructural {
            shape_id,
            mut recipe,
            zone_requirement,
            provenance,
        },
        zone,
    ) = marked(
        21,
        "AmharicTerminalModify",
        vec![vec!["a"], vec!["needle"]],
        vec![],
        vec!["x"],
        MarkerZone::Suffix,
    )
    else {
        unreachable!();
    };
    recipe.translated_input_members[1] = (0..=RELATION_PROBE_WORK_CAP)
        .map(|index| format!("member-{index}"))
        .collect();
    let relation = CompiledMorphologyRelation::from_classified([(
        MorphologyRewrite::MarkedStructural {
            shape_id,
            recipe,
            zone_requirement,
            provenance,
        },
        zone,
    )])
    .unwrap();
    let input = relation.marked_input(AllomorphId(21), "z").unwrap();
    assert!(matches!(
        relation.apply(&input),
        MorphologyRelationResult::ResourceRejected {
            reason_id: "probe-work-budget",
            consumed_markers: 0,
            ..
        }
    ));
    assert_eq!(
        relation.fired_recipe_count_for(AllomorphId(21), MarkerZone::Suffix),
        0
    );
}

#[test]
fn member_sort_is_refused_before_sorting_when_its_bound_exceeds_budget() {
    let member_sets = vec![vec!["a".to_owned(), "b".to_owned()]];
    let mut budget = ProbeBudget {
        work: RELATION_PROBE_WORK_CAP - 4,
        outputs: 0,
        allocation_bytes: 0,
        depth: 0,
    };
    let mut callbacks = 0;
    let result = visit_segmentations("", &member_sets, &mut budget, &mut |_, _| {
        callbacks += 1;
        Ok(())
    });
    assert!(matches!(
        result,
        Err(ProbeBudgetReached {
            reason_id: "probe-work-budget",
            ..
        })
    ));
    assert_eq!(callbacks, 0);
}

#[test]
fn empty_member_entries_are_charged_before_segmentation() {
    let member_sets = vec![vec![String::new(), String::new()]];
    let mut budget = ProbeBudget {
        work: RELATION_PROBE_WORK_CAP - 2,
        outputs: 0,
        allocation_bytes: 0,
        depth: 0,
    };
    let mut callbacks = 0;
    let result = visit_segmentations("", &member_sets, &mut budget, &mut |_, _| {
        callbacks += 1;
        Ok(())
    });
    assert!(matches!(
        result,
        Err(ProbeBudgetReached {
            reason_id: "probe-work-budget",
            ..
        })
    ));
    assert_eq!(callbacks, 0);
}

#[test]
fn partition_clone_is_refused_before_slice_copy() {
    let tokens = vec!["a".to_owned(), "b".to_owned()];
    let mut budget = ProbeBudget {
        work: RELATION_PROBE_WORK_CAP - 2,
        outputs: 0,
        allocation_bytes: 0,
        depth: 0,
    };
    let mut current = Vec::new();
    let mut callbacks = 0;
    let result = visit_partitions(&tokens, 1, &mut budget, &mut current, &mut |_, _| {
        callbacks += 1;
        Ok(())
    });
    assert!(matches!(
        result,
        Err(ProbeBudgetReached {
            reason_id: "probe-work-budget",
            ..
        })
    ));
    assert_eq!(callbacks, 0);
    assert!(current.is_empty());
}

#[test]
fn duplicate_output_lookup_is_budgeted_before_contains() {
    let outputs = (0..1024)
        .map(|index| format!("output-{index}"))
        .collect::<BTreeSet<_>>();
    let mut budget = ProbeBudget {
        work: RELATION_PROBE_WORK_CAP - 1,
        outputs: 0,
        allocation_bytes: 0,
        depth: 0,
    };
    let mut outputs = outputs;
    let result = insert_probe_output(&mut outputs, "output-0".to_owned(), &mut budget);
    assert!(matches!(
        result,
        Err(ProbeBudgetReached {
            reason_id: "probe-work-budget",
            ..
        })
    ));
    assert_eq!(outputs.len(), 1024);
}

#[test]
fn giant_direct_output_is_refused_before_formatting() {
    let mut replacement = marked(
        22,
        "AmharicTerminalModify",
        vec![vec!["a"], vec!["a"]],
        vec![],
        vec!["x"],
        MarkerZone::Suffix,
    );
    if let MorphologyRewrite::MarkedStructural { recipe, .. } = &mut replacement.0 {
        recipe.output_segments = vec!["x".repeat(RELATION_PROBE_ALLOCATION_BYTES_CAP + 1)];
    }
    let relation = CompiledMorphologyRelation::from_classified([replacement]).unwrap();
    let input = relation.marked_input(AllomorphId(22), "a").unwrap();
    assert!(matches!(
        relation.apply(&input),
        MorphologyRelationResult::ResourceRejected {
            reason_id: "probe-allocation-bytes",
            consumed_markers: 0,
            allocation_bytes,
            ..
        } if allocation_bytes == RELATION_PROBE_ALLOCATION_BYTES_CAP
    ));
    assert_eq!(
        relation.fired_recipe_count_for(AllomorphId(22), MarkerZone::Suffix),
        0
    );
}

#[test]
fn giant_direct_literal_is_refused_before_formatting() {
    let mut literal = marked(
        23,
        "AdjacentTerminalDrop",
        vec![vec!["a"], vec!["b"]],
        vec![vec!["x"]],
        vec![],
        MarkerZone::Suffix,
    );
    if let MorphologyRewrite::MarkedStructural { recipe, .. } = &mut literal.0 {
        recipe.literal_runs[0] = vec!["x".repeat(RELATION_PROBE_ALLOCATION_BYTES_CAP + 1)];
    }
    let relation = CompiledMorphologyRelation::from_classified([literal]).unwrap();
    let input = relation.marked_input(AllomorphId(23), "ab").unwrap();
    assert!(matches!(
        relation.apply(&input),
        MorphologyRelationResult::ResourceRejected {
            reason_id: "probe-allocation-bytes",
            consumed_markers: 0,
            allocation_bytes,
            ..
        } if allocation_bytes == RELATION_PROBE_ALLOCATION_BYTES_CAP
    ));
    assert_eq!(
        relation.fired_recipe_count_for(AllomorphId(23), MarkerZone::Suffix),
        0
    );
}

#[test]
fn output_budget_rejects_unique_direct_outputs_without_partial_fire() {
    let (
        MorphologyRewrite::MarkedStructural {
            shape_id,
            mut recipe,
            zone_requirement,
            provenance,
        },
        zone,
    ) = marked(
        18,
        "AmharicTerminalModify",
        vec![vec!["a"], vec!["a"]],
        vec![],
        vec!["placeholder"],
        MarkerZone::Suffix,
    )
    else {
        unreachable!();
    };
    recipe.output_segments = (0..=RELATION_PROBE_OUTPUT_CAP)
        .map(|index| format!("output-{index}"))
        .collect();
    let relation = CompiledMorphologyRelation::from_classified([(
        MorphologyRewrite::MarkedStructural {
            shape_id,
            recipe,
            zone_requirement,
            provenance,
        },
        zone,
    )])
    .unwrap();
    let input = relation.marked_input(AllomorphId(18), "a").unwrap();
    assert!(matches!(
        relation.apply(&input),
        MorphologyRelationResult::ResourceRejected {
            reason_id: "probe-output-budget",
            consumed_markers: 0,
            work,
            outputs,
            ..
        } if work < RELATION_PROBE_WORK_CAP && outputs == RELATION_PROBE_OUTPUT_CAP
    ));
    assert_eq!(
        relation.fired_recipe_count_for(AllomorphId(18), MarkerZone::Suffix),
        0
    );
}

#[test]
fn terminal_modify_replaces_a_matching_middle_position_and_preserves_suffix() {
    let relation = CompiledMorphologyRelation::from_classified([marked(
        6,
        "AmharicTerminalModify",
        vec![vec!["a"], vec!["b"]],
        vec![],
        vec!["x", "y"],
        MarkerZone::Suffix,
    )])
    .unwrap();
    let input = relation.marked_input(AllomorphId(6), "abc").unwrap();
    let outputs = match relation.apply(&input) {
        MorphologyRelationResult::Recipe { outputs, .. } => outputs,
        other => panic!("expected recipe result, got {other:?}"),
    };
    assert_eq!(
        outputs,
        BTreeSet::from(["axc".to_owned(), "ayc".to_owned()])
    );
}

#[test]
fn misplaced_markers_are_rejected_by_zone() {
    let prefix = CompiledMorphologyRelation::from_classified([marked(
        7,
        "AdjacentInitialDrop",
        vec![vec!["a"], vec!["b"]],
        vec![],
        vec![],
        MarkerZone::Prefix,
    )])
    .unwrap();
    let suffix = CompiledMorphologyRelation::from_classified([marked(
        8,
        "AdjacentTerminalDrop",
        vec![vec!["a"], vec!["b"]],
        vec![vec!["x"]],
        vec![],
        MarkerZone::Suffix,
    )])
    .unwrap();
    let prefix_marker = prefix.marker_binding_for(AllomorphId(7)).unwrap().symbol;
    let suffix_marker = suffix.marker_binding_for(AllomorphId(8)).unwrap().symbol;
    assert!(matches!(
        prefix.apply(&format!("a{}b", prefix_marker)),
        MorphologyRelationResult::Rejected {
            reason_id: "zone-mismatch",
            ..
        }
    ));
    assert!(matches!(
        suffix.apply(&format!("{}ab", suffix_marker)),
        MorphologyRelationResult::Rejected {
            reason_id: "zone-mismatch",
            ..
        }
    ));
}

#[test]
fn cloned_relations_share_recipe_fire_observations() {
    let relation = CompiledMorphologyRelation::from_classified([marked(
        11,
        "AdjacentInitialDrop",
        vec![vec!["a"], vec!["b"]],
        vec![],
        vec![],
        MarkerZone::Prefix,
    )])
    .unwrap();
    let clone = relation.clone();
    let input = clone.marked_input(AllomorphId(11), "ab").unwrap();
    assert!(matches!(
        clone.apply(&input),
        MorphologyRelationResult::Recipe { .. }
    ));
    assert_eq!(relation.fired_recipe_count(), 1);
}

#[test]
fn duplicate_binding_is_rejected_by_relation_construction() {
    let first = marked(
        9,
        "AdjacentTerminalDrop",
        vec![vec!["a"], vec!["b"]],
        vec![vec!["x"]],
        vec![],
        MarkerZone::Suffix,
    );
    let second = marked(
        10,
        "AdjacentInitialDrop",
        vec![vec!["a"], vec!["b"]],
        vec![],
        vec![],
        MarkerZone::Prefix,
    );
    let duplicate = marked(
        9,
        "AdjacentTerminalDrop",
        vec![vec!["a"], vec!["b"]],
        vec![vec!["x"]],
        vec![],
        MarkerZone::Suffix,
    );
    assert!(matches!(
        CompiledMorphologyRelation::from_classified([first, second, duplicate]),
        Err(MorphologyRelationError::DuplicateBinding { .. })
    ));
}
