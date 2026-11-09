use super::*;

/// `Visited::insert` gives the same answer as a `HashSet` of `(state, ann_index, registers)` keys for every call, at every byte width, for keys reached through many different parent chains (diff records and snapshots), including keys that differ only in the high bytes of a wide value.
#[test]
fn visited_answers_exactly_as_a_hash_set_of_keys() {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move |bound: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % bound as u64) as usize
    };
    for (states, n, slots, hash_mask, keys) in [
        (3, 5, 4, u32::MAX, 20_000),
        (200, 63, 6, u32::MAX, 20_000),
        (300, 64, 6, u32::MAX, 20_000),
        (70_000, 16_383, 3, u32::MAX, 20_000),
        (5, 16_384, 300, u32::MAX, 20_000),
        (2, 100_000, 1, u32::MAX, 20_000),
        (4, 3, 0, u32::MAX, 20_000),
        // Colliding hashes: every candidate with the same state and position is compared by walking its records.
        (2, 4, 6, 0, 4_000),
        (2, 70, 9, 1, 4_000),
    ] {
        let budget = crate::work::WorkBudget::new(usize::MAX / 2).with_retained_cap(None);
        let _scope = budget.enter();
        let mut visited = Visited::new(states, n, slots);
        visited.hash_mask = hash_mask;
        let mut reference: HashSet<(usize, usize, Vec<Packed>)> = HashSet::new();
        // Values clustered at the extremes so near-equal keys and width boundaries both occur.
        let pick = |next: &mut dyn FnMut(usize) -> usize, max: usize| match next(4) {
            0 => next(3).min(max),
            1 => max - next(3).min(max),
            _ => next(max + 1),
        };
        let root_registers = vec![Packed::UNSET; slots];
        let root = visited.root(&root_registers).expect("unbounded");
        let mut kept: Vec<(VisitedRecord, Vec<Packed>)> = vec![(root, root_registers)];
        for _ in 0..keys {
            // Mostly extend recent (deep) keys, sometimes branch from anywhere.
            let from = if next(4) == 0 {
                next(kept.len())
            } else {
                kept.len() - 1 - next(kept.len().min(8))
            };
            let (parent, parent_registers) = kept[from].clone();
            let mut registers = parent_registers.clone();
            for _ in 0..next(4) {
                if slots > 0 {
                    let slot = next(slots);
                    registers[slot] = match next(3) {
                        0 => Packed::UNSET,
                        _ => Packed::at(pick(&mut next, n) as i32, next(2) == 0),
                    };
                }
            }
            let state = pick(&mut next, states - 1);
            let ann = pick(&mut next, n);
            let expected = reference.insert((state, ann, registers.clone()));
            let got = visited
                .insert(state, ann, &registers, Some((parent, &parent_registers)))
                .expect("unbounded");
            assert_eq!(
                got.is_some(),
                expected,
                "states {states}, n {n}: key ({state}, {ann}, {registers:?})"
            );
            if let Some(record) = got {
                assert!(record.hops < SNAPSHOT_HOPS);
                kept.push((record, registers));
            }
        }
        assert_eq!(visited.len(), reference.len());
    }
}

/// `Seen` answers exactly as a `HashSet` of keys does across the switch from the plain set to compact records, including keys spawned before the switch (no kept parent) and after it.
#[test]
fn seen_answers_as_a_hash_set_across_the_switch() {
    let slots = 6;
    let budget = crate::work::WorkBudget::new(usize::MAX / 2).with_retained_cap(None);
    let _scope = budget.enter();
    let mut seen = Seen::new();
    let mut reference = std::collections::HashSet::new();
    let mut x: u64 = 0x1234_5678_9abc_def1;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut parent: Option<(VisitedRecord, Vec<Packed>)> = None;
    let mut switched_at = None;
    for step in 0..40_000 {
        let mut registers: Vec<Packed> = match &parent {
            Some((_, p)) if next() % 4 != 0 => p.clone(),
            _ => vec![Packed::UNSET; slots],
        };
        for _ in 0..(next() % 3) {
            let slot = (next() % slots as u64) as usize;
            registers[slot] = Packed::at((next() % 6) as i32, next() % 2 == 0);
        }
        let state = (next() % 40) as usize;
        let inst = Inst {
            state,
            ann_index: (next() % 4) as usize,
            registers: Rc::from(registers.clone()),
            record: None,
        };
        let expected = reference.insert((inst.state, inst.ann_index, registers.clone()));
        let given = parent.as_ref().map(|(r, p)| (*r, &p[..]));
        match seen.insert(&inst, given, 40, 8, slots) {
            Insert::Present => assert!(!expected, "step {step}"),
            Insert::New(record) => {
                assert!(expected, "step {step}");
                parent = record.map(|r| (r, registers));
            }
            Insert::Ceiling => panic!("unbounded"),
        }
        if switched_at.is_none() && matches!(seen, Seen::Large(_)) {
            switched_at = Some(step);
        }
    }
    assert!(switched_at.is_some(), "the set grew past the switch");
    assert_eq!(seen.len(), reference.len());
}

/// Storage is charged to the active budget and returned when the set is dropped; a ceiling below the first chunk refuses instead of storing.
#[test]
fn visited_charges_its_storage_and_refuses_past_the_ceiling() {
    let budget = crate::work::WorkBudget::new(usize::MAX / 2).with_retained_cap(None);
    {
        let _scope = budget.enter();
        let mut visited = Visited::new(4, 10, 2);
        let regs = [Packed::UNSET; 2];
        let root = visited.root(&regs).expect("unbounded");
        assert!(visited
            .insert(1, 2, &regs, Some((root, &regs)))
            .expect("unbounded")
            .is_some());
        assert!(budget.retained() >= FIRST_CHUNK_BYTES);
    }
    assert_eq!(budget.retained(), 0);

    let tight =
        crate::work::WorkBudget::new(usize::MAX / 2).with_retained_cap(Some(FIRST_CHUNK_BYTES - 1));
    let _scope = tight.enter();
    let mut visited = Visited::new(4, 10, 2);
    assert!(visited.root(&[Packed::UNSET; 2]).is_none());
    assert!(tight.retained_capped());
}

fn result(id: &str, registers: Vec<Register>, order: usize) -> FstResult {
    FstResult {
        id: Some(id.to_string()),
        registers,
        priority: order as i32,
        is_lazy: false,
        next_ann: 0,
        order,
    }
}

/// `distinct` keeps the first occurrence of each `result_eq` class in input order, ignores the fields `result_eq` ignores, and treats every `has == false` register as equal whatever its stale offset.
#[test]
fn distinct_keeps_first_occurrences_in_input_order() {
    let stale_unset = Register {
        offset: 7,
        start: true,
        has: false,
    };
    let input = vec![
        result("b", vec![Register::at(1, true), Register::unset()], 0),
        result("a", vec![Register::at(1, true), Register::unset()], 1),
        result("b", vec![Register::at(1, true), stale_unset], 2), // equal to #0
        result("b", vec![Register::at(2, true), Register::unset()], 3),
        result("a", vec![Register::at(1, true), Register::unset()], 4), // equal to #1
        result("b", vec![Register::at(1, true)], 5),                    // different register count
    ];
    let kept: Vec<usize> = distinct(input).into_iter().map(|r| r.order).collect();
    assert_eq!(kept, vec![0, 1, 3, 5]);
}

/// A larger input whose results share registers across many ids: every distinct id survives once, in order, and the two repeat rounds are dropped.
#[test]
fn distinct_resolves_many_results_by_result_eq() {
    let mut input = Vec::new();
    for round in 0..3 {
        for i in 0..500 {
            input.push(result(
                &format!("id{i}"),
                vec![Register::at(i as i32 % 7, true)],
                round * 500 + i,
            ));
        }
    }
    let kept: Vec<usize> = distinct(input).into_iter().map(|r| r.order).collect();
    assert_eq!(kept, (0..500).collect::<Vec<_>>());
}

#[test]
fn packed_registers_round_trip_every_flag_and_offset_boundary() {
    assert_eq!(Packed::UNSET.unpack(), Register::unset());
    for offset in [0, 1, 4096, (1 << 30) - 1] {
        for start in [false, true] {
            assert_eq!(
                Packed::at(offset, start).unpack(),
                Register::at(offset, start)
            );
        }
    }
}
