use super::*;

fn lexicon_leaf() -> PlanNodeKind {
    PlanNodeKind::Leaf {
        fragment: FragmentSpec::LexiconFragment { entries: None },
        provenance: Provenance::Lexicon,
    }
}

fn rule_leaf(id: u32) -> PlanNodeKind {
    PlanNodeKind::Leaf {
        fragment: FragmentSpec::RewriteRule { rule: PRuleId(id) },
        provenance: Provenance::RewriteRule(PRuleId(id)),
    }
}

/// The core dedup claim: constructing the identical subtree twice yields the same `NodeId` and stores it once.
#[test]
fn identical_subtree_interned_once() {
    let mut plan = Plan::new();
    let a = plan.add_node(lexicon_leaf());
    let b = plan.add_node(lexicon_leaf());
    assert_eq!(
        a, b,
        "constructing the same leaf twice must yield the same NodeId"
    );
    assert_eq!(plan.len(), 1, "must be stored exactly once, not duplicated");
}

/// The core content-addressing claim at the `ReplaceCascadeSpec` level: two `Replace` nodes with the same `rules`/`gated_subrules` but different `group_key` get different `NodeId`s, while identical ones dedup to one.
#[test]
fn replace_nodes_differing_only_in_group_key_yield_different_ids_but_identical_keys_dedup() {
    let mut plan = Plan::new();
    let leaf = plan.add_node(rule_leaf(1));
    let gated_subrules = vec![GatedSubruleRef {
        rule_pos: 0,
        sub_idx: 0,
    }];

    let replace_true = plan.add_node(PlanNodeKind::Replace {
        cascade: ReplaceCascadeSpec {
            rules: vec![PRuleId(1)],
            gated_subrules: gated_subrules.clone(),
            group_key: vec![true],
        },
        children: vec![leaf],
    });
    let replace_false = plan.add_node(PlanNodeKind::Replace {
        cascade: ReplaceCascadeSpec {
            rules: vec![PRuleId(1)],
            gated_subrules: gated_subrules.clone(),
            group_key: vec![false],
        },
        children: vec![leaf],
    });
    assert_ne!(
        replace_true, replace_false,
        "two Replace nodes differing only in group_key are different candidate compiled \
         cascades (different subrule_ok), and must get different NodeIds -- this is the task \
         1.4 fix's whole point"
    );

    let replace_true_again = plan.add_node(PlanNodeKind::Replace {
        cascade: ReplaceCascadeSpec {
            rules: vec![PRuleId(1)],
            gated_subrules,
            group_key: vec![true],
        },
        children: vec![leaf],
    });
    assert_eq!(
        replace_true, replace_true_again,
        "two Replace nodes with an IDENTICAL group_key (same rules, same gated_subrules) must \
         still dedup to the SAME NodeId -- the fix must not break sound sharing"
    );
    assert_eq!(
        plan.len(),
        3,
        "leaf + 2 distinct Replace nodes (true/false), the redundant true-again NOT duplicated"
    );
}

/// Two parents sharing one child leaf: the leaf is stored once, not once per parent.
#[test]
fn shared_child_leaf_stored_once_across_two_parents() {
    let mut plan = Plan::new();
    let shared_leaf = plan.add_node(lexicon_leaf());
    let rule_a = plan.add_node(rule_leaf(1));
    let rule_b = plan.add_node(rule_leaf(2));

    let parent_a = plan.add_node(PlanNodeKind::Compose {
        children: vec![shared_leaf, rule_a],
        strategy: ComposeStrategy::Static,
    });
    let parent_b = plan.add_node(PlanNodeKind::Compose {
        children: vec![shared_leaf, rule_b],
        strategy: ComposeStrategy::Static,
    });

    assert_ne!(
        parent_a, parent_b,
        "the two parents differ in one child, so must differ"
    );
    // 1 shared leaf + 2 rule leaves + 2 parents = 5 stored nodes, not 6 (a tree would store the shared leaf twice).
    assert_eq!(plan.len(), 5);
    assert!(plan.contains(shared_leaf));
    assert_eq!(plan.get(parent_a).unwrap().children()[0], shared_leaf);
    assert_eq!(plan.get(parent_b).unwrap().children()[0], shared_leaf);
}

/// Pins Static's legacy hash tag (from when Lazy variants existed); changing it moves every Compose node's `NodeId` and can alter candidate ordering.
#[test]
fn static_compose_strategy_preserves_legacy_hash_tag() {
    let mut hasher = StableHasher::new();
    ComposeStrategy::Static.hash(&mut hasher);
    assert_eq!(hasher.finish(), 0xa8c7_f832_281a_39c5);
}

/// Two independently built `Plan`s with identical content produce equal `NodeId`s for corresponding nodes.
#[test]
fn content_addresses_are_stable_across_independently_built_plans() {
    let mut plan_1 = Plan::new();
    let leaf_1 = plan_1.add_node(lexicon_leaf());
    let root_1 = plan_1.add_node(PlanNodeKind::Compose {
        children: vec![leaf_1],
        strategy: ComposeStrategy::Static,
    });

    let mut plan_2 = Plan::new();
    let leaf_2 = plan_2.add_node(lexicon_leaf());
    let root_2 = plan_2.add_node(PlanNodeKind::Compose {
        children: vec![leaf_2],
        strategy: ComposeStrategy::Static,
    });

    assert_eq!(
        leaf_1, leaf_2,
        "identical leaf content built in two independent plans must hash identically"
    );
    assert_eq!(
        root_1, root_2,
        "identical Compose content (same child id, same strategy) built independently must \
         hash identically"
    );
}

/// A well-formed `Gate` node (children length matches partition group count) interns cleanly.
#[test]
fn gate_node_with_matching_children_and_groups_interns() {
    let mut plan = Plan::new();
    let group_a = plan.add_node(rule_leaf(1));
    let group_b = plan.add_node(rule_leaf(2));
    let partition = GatePartitionSpec {
        gated_subrules: vec![GatedSubruleRef {
            rule_pos: 0,
            sub_idx: 0,
        }],
        groups: vec![
            GateGroupSpec { key: vec![true] },
            GateGroupSpec { key: vec![false] },
        ],
    };
    let gate = plan.add_node(PlanNodeKind::Gate {
        partition,
        children: vec![group_a, group_b],
    });
    assert!(plan.get(gate).is_some());
    assert_eq!(plan.get(gate).unwrap().kind_name(), "Gate");
}

/// `Plan::add_node`'s debug-only invariant on a `Gate` node whose `children` count doesn't match its partition's group count; gated on `debug_assertions` because `debug_assert!` is stripped in release, where a `#[should_panic]` test of it would fail.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "one child per partition group")]
fn gate_node_invariant_panics_in_debug_on_mismatch() {
    let mut plan = Plan::new();
    let group_a = plan.add_node(rule_leaf(1));
    let partition = GatePartitionSpec {
        gated_subrules: vec![],
        groups: vec![
            GateGroupSpec { key: vec![true] },
            GateGroupSpec { key: vec![false] },
        ],
    };
    plan.add_node(PlanNodeKind::Gate {
        partition,
        // Only 1 child for 2 groups -- must trip the invariant.
        children: vec![group_a],
    });
}

/// Exercises `PlanNodeKind::children`/`kind_name`'s exhaustive matches over every variant; a new variant fails the FILE to compile, not this test.
#[test]
fn kind_name_and_children_cover_every_node_kind() {
    let mut plan = Plan::new();

    let leaf = plan.add_node(lexicon_leaf());
    assert_eq!(plan.get(leaf).unwrap().kind_name(), "Leaf");
    assert!(plan.get(leaf).unwrap().children().is_empty());

    let compose = plan.add_node(PlanNodeKind::Compose {
        children: vec![leaf],
        strategy: ComposeStrategy::Static,
    });
    assert_eq!(plan.get(compose).unwrap().kind_name(), "Compose");
    assert_eq!(plan.get(compose).unwrap().children(), &[leaf]);

    let union = plan.add_node(PlanNodeKind::Union {
        children: vec![leaf, compose],
    });
    assert_eq!(plan.get(union).unwrap().kind_name(), "Union");
    assert_eq!(plan.get(union).unwrap().children(), &[leaf, compose]);

    let replace = plan.add_node(PlanNodeKind::Replace {
        cascade: ReplaceCascadeSpec {
            rules: vec![PRuleId(0), PRuleId(1)],
            gated_subrules: vec![],
            group_key: vec![],
        },
        children: vec![leaf],
    });
    assert_eq!(plan.get(replace).unwrap().kind_name(), "Replace");
    assert_eq!(plan.get(replace).unwrap().children(), &[leaf]);

    let gate = plan.add_node(PlanNodeKind::Gate {
        partition: GatePartitionSpec {
            gated_subrules: vec![],
            groups: vec![GateGroupSpec { key: vec![] }],
        },
        children: vec![leaf],
    });
    assert_eq!(plan.get(gate).unwrap().kind_name(), "Gate");
    assert_eq!(plan.get(gate).unwrap().children(), &[leaf]);

    assert_eq!(plan.len(), 5);
    assert_eq!(plan.iter().count(), 5);
}

/// `Plan::set_root`/`Plan::root` round-trip.
#[test]
fn root_round_trips() {
    let mut plan = Plan::new();
    assert_eq!(plan.root(), None);
    let leaf = plan.add_node(lexicon_leaf());
    plan.set_root(leaf);
    assert_eq!(plan.root(), Some(leaf));
}
