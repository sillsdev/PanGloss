use super::*;
use pg_grammar::model::MorphemeId as Mid;

fn sample_path(name: &str) -> Option<std::path::PathBuf> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn load_indonesian() -> Option<Grammar> {
    let path = sample_path("indonesian-hc.xml")?;
    let xml = std::fs::read_to_string(&path).expect("read grammar");
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load grammar: {e}")))
}

/// A bare-root word confirms to a non-empty set of matches, all sharing the expected root entry.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn confirm_bare_root_word_verifies() {
    let Some(g) = load_indonesian() else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let morpher = Morpher::new(&g, usize::MAX);
    let owners = build_morpheme_owners(&g);
    // Finds "ajar"'s morpheme id from the grammar itself (entry25/entry26 homograph) rather than hard-coding one that might drift.
    let entry = g
        .entries
        .iter()
        .enumerate()
        .find(|(_, e)| {
            g.morphemes[e.morpheme.0 as usize].xml_key == "entry25"
                || g.morphemes[e.morpheme.0 as usize].xml_key == "entry26"
        })
        .map(|(i, e)| (i, e.morpheme));
    let Some((_idx, morpheme)) = entry else {
        eprintln!("skipping: entry25/entry26 not found in indonesian-hc.xml");
        return;
    };
    let candidate = Candidate {
        morphemes: vec![morpheme],
        root_index: 0,
    };
    let matches = confirm_all(&g, &owners, &morpher, &candidate, "ajar");
    assert!(
        !matches.is_empty(),
        "\"ajar\" must confirm to at least one analysis"
    );
    for (wa, _, _) in &matches {
        assert_eq!(wa.root_morpheme_index, 0);
        assert_eq!(wa.morpheme_ids, vec![morpheme.0]);
    }
}

/// A candidate whose root position is out of range (or empty) must confirm to nothing, never panic.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn confirm_rejects_out_of_range_root_index() {
    let Some(g) = load_indonesian() else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let morpher = Morpher::new(&g, usize::MAX);
    let owners = build_morpheme_owners(&g);
    let bogus = Candidate {
        morphemes: vec![],
        root_index: 0,
    };
    assert!(confirm_all(&g, &owners, &morpher, &bogus, "ajar").is_empty());
}

/// A non-root morpheme id owned by neither a `LexEntry` nor an `MRule` must confirm to nothing.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn confirm_rejects_unowned_non_root_morpheme() {
    let Some(g) = load_indonesian() else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let morpher = Morpher::new(&g, usize::MAX);
    let owners = build_morpheme_owners(&g);
    let root = g.entries[0].morpheme;
    let candidate = Candidate {
        morphemes: vec![root, Mid(u32::MAX - 5)],
        root_index: 0,
    };
    assert!(confirm_all(&g, &owners, &morpher, &candidate, "ajar").is_empty());
}

// ---- confirm_pin_groups, synthetic grammar -------------------------------------------------

mod pin_groups {
    use super::*;
    use pg_rules::stratum::WordBudget;

    /// Roots `k` (twice, homographs), `l`; unordered suffix rules `p` and `q`.
    fn fixture() -> Grammar {
        let rule = |id: &str, name: &str, seg: &str| {
            format!(
                r#"<MorphologicalRule id="{id}" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV">
            <Name>{name}</Name>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="sub{id}">
                <MorphologicalInput><PhoneticSequence id="stem{id}"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
                <MorphologicalOutput><CopyFromInput index="stem{id}" /><InsertSegments><PhoneticShape>{seg}</PhoneticShape></InsertSegments></MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
            <MorphemeId>{name}</MorphemeId>
          </MorphologicalRule>"#
            )
        };
        let entry = |id: &str, shape: &str| {
            format!(
                r#"<LexicalEntry id="e{id}" partOfSpeech="posV"><Allomorphs><Allomorph id="a{id}"><PhoneticShape>{shape}</PhoneticShape></Allomorph></Allomorphs><MorphemeId>{id}</MorphemeId></LexicalEntry>"#
            )
        };
        let xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput><Language><Name>PinGroupsFixture</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>
<SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cl"><Representations><Representation>l</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cp"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cq"><Representations><Representation>q</Representation></Representations></SegmentDefinition>
</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
<Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrP mrQ"><Name>Main</Name>
<MorphologicalRuleDefinitions>{p}{q}</MorphologicalRuleDefinitions>
<LexicalEntries>{k1}{k2}{l}</LexicalEntries>
</Stratum></Strata></Language></HermitCrabInput>"#,
            p = rule("mrP", "P", "p"),
            q = rule("mrQ", "Q", "q"),
            k1 = entry("K1", "k"),
            k2 = entry("K2", "k"),
            l = entry("L", "l"),
        );
        pg_grammar::load(&xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
    }

    const WORDS: [&str; 9] = ["k", "l", "kp", "kq", "kpq", "kqp", "kpp", "lp", "lqp"];

    type Key = (Vec<u32>, i32);

    fn keys(analyses: &[(EngineAnalysis, String, String)]) -> Vec<Key> {
        analyses
            .iter()
            .map(|(wa, _, _)| (wa.morpheme_ids.clone(), wa.root_morpheme_index))
            .collect()
    }

    fn sorted(mut v: Vec<Key>) -> Vec<Key> {
        v.sort();
        v
    }

    fn plain(morpher: &Morpher, word: &str) -> Vec<Key> {
        let outcome = morpher.parse_word_opts(word, &ParseOptions::default());
        sorted(
            outcome
                .structured
                .iter()
                .map(|wa| (wa.morpheme_ids.clone(), wa.root_morpheme_index))
                .collect(),
        )
    }

    /// Head root and non-root rules of one analysis.
    fn pins_of(owners: &[Option<MorphemeOwner>], key: &Key) -> (LexEntryId, Vec<MRuleId>) {
        let mut head = None;
        let mut rules = Vec::new();
        for (i, m) in key.0.iter().enumerate() {
            match owners[*m as usize] {
                Some(MorphemeOwner::LexEntry(le)) if i as i32 == key.1 => head = Some(le),
                Some(MorphemeOwner::MRule(r)) => rules.push(r),
                _ => {}
            }
        }
        (head.expect("head root"), rules)
    }

    fn all_roots(g: &Grammar) -> Vec<LexEntryId> {
        (0..g.entries.len() as u32).map(LexEntryId).collect()
    }

    fn all_rules(g: &Grammar) -> Vec<MRuleId> {
        (0..g.mrules.len() as u32).map(MRuleId).collect()
    }

    fn group(roots: &[LexEntryId], rules: &[MRuleId]) -> PinGroup {
        PinGroup {
            roots: roots.iter().copied().collect(),
            rules: rules.iter().copied().collect(),
        }
    }

    fn fresh(morpher: &Morpher) -> WordBudget {
        morpher.word_budget()
    }

    #[test]
    fn the_fixture_has_real_ambiguity_and_homograph_multiplicity() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        assert_eq!(plain(&morpher, "k").len(), 2, "two homograph roots");
        assert!(!plain(&morpher, "kpq").is_empty());
        assert!(plain(&morpher, "kqq").is_empty());
    }

    #[test]
    fn one_all_covering_group_returns_exactly_plain_hc_with_multiplicity() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        let everything = group(&all_roots(&g), &all_rules(&g));
        for word in WORDS {
            let got = confirm_pin_groups(
                &g,
                &morpher,
                word,
                std::slice::from_ref(&everything),
                &fresh(&morpher),
            );
            assert!(!got.incomplete(), "{word}");
            assert_eq!(got.reparses, 1);
            assert_eq!(sorted(keys(&got.analyses)), plain(&morpher, word), "{word}");
        }
    }

    #[test]
    fn groups_split_by_head_root_concatenate_to_plain_hc_and_keep_multiplicity() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        let rules = all_rules(&g);
        let groups: Vec<PinGroup> = all_roots(&g).iter().map(|r| group(&[*r], &rules)).collect();
        for word in WORDS {
            let got = confirm_pin_groups(&g, &morpher, word, &groups, &fresh(&morpher));
            assert_eq!(got.reparses, groups.len());
            assert_eq!(sorted(keys(&got.analyses)), plain(&morpher, word), "{word}");
        }
    }

    #[test]
    fn group_order_is_ascending_smallest_root_whatever_the_input_order() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        let rules = all_rules(&g);
        let roots = all_roots(&g);
        let forward: Vec<PinGroup> = roots.iter().map(|r| group(&[*r], &rules)).collect();
        let reversed: Vec<PinGroup> = forward.iter().rev().cloned().collect();
        let a = confirm_pin_groups(&g, &morpher, "k", &forward, &fresh(&morpher));
        let b = confirm_pin_groups(&g, &morpher, "k", &reversed, &fresh(&morpher));
        assert_eq!(keys(&a.analyses), keys(&b.analyses));
        let owners = build_morpheme_owners(&g);
        let heads: Vec<u32> = keys(&a.analyses)
            .iter()
            .map(|k| pins_of(&owners, k).0 .0)
            .collect();
        let mut ascending = heads.clone();
        ascending.sort_unstable();
        assert_eq!(
            heads, ascending,
            "results follow ascending smallest root id"
        );
        assert_eq!(heads.len(), 2);
    }

    #[test]
    #[should_panic(expected = "more than one pin group")]
    fn a_head_root_in_two_groups_is_refused() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        let r = LexEntryId(0);
        let groups = [group(&[r], &[]), group(&[r, LexEntryId(1)], &[])];
        let _ = confirm_pin_groups(&g, &morpher, "k", &groups, &fresh(&morpher));
    }

    #[test]
    fn no_groups_means_no_reparse() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        let budget = fresh(&morpher);
        let got = confirm_pin_groups(&g, &morpher, "k", &[], &budget);
        assert!(got.analyses.is_empty() && !got.incomplete());
        assert_eq!((got.reparses, got.work_steps), (0, 0));
    }

    #[test]
    fn work_steps_is_the_shared_allowances_cumulative_count_not_a_sum() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        let rules = all_rules(&g);
        let groups: Vec<PinGroup> = all_roots(&g).iter().map(|r| group(&[*r], &rules)).collect();
        let budget = fresh(&morpher);
        let got = confirm_pin_groups(&g, &morpher, "kpq", &groups, &budget);
        assert_eq!(got.work_steps, budget.work_steps() as u64);
        assert!(got.work_steps > 0);
    }

    #[test]
    fn unfiltered_call_equals_plain_hc() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        for word in WORDS {
            let got = confirm_unfiltered(&morpher, word, &fresh(&morpher));
            assert_eq!(got.reparses, 1);
            assert!(!got.incomplete());
            assert_eq!(sorted(keys(&got.analyses)), plain(&morpher, word), "{word}");
        }
    }

    fn stopping_morpher<'g>(g: &'g Grammar, stop: &str) -> Morpher<'g> {
        Morpher::new(g, if stop == "work" { 1 } else { usize::MAX })
            .with_word_timeout((stop == "time").then_some(std::time::Duration::ZERO))
            .with_word_mem_cap((stop == "memory").then_some(1))
    }

    /// C29-style: a stopped confirmation must carry its flag, from both entries, identically.
    #[test]
    fn a_stopped_confirmation_sets_the_stop_flag_and_agrees_with_confirm_batch_checked() {
        let g = fixture();
        let owners = build_morpheme_owners(&g);
        let k1 = g.entries[0].morpheme;
        let candidate = Candidate {
            morphemes: vec![k1],
            root_index: 0,
        };
        let groups = [group(&all_roots(&g), &all_rules(&g))];
        let ok = stopping_morpher(&g, "none");
        let got = confirm_pin_groups(&g, &ok, "k", &groups, &fresh(&ok));
        assert!(!got.stop.any() && !got.analyses.is_empty());

        for stop in ["work", "time", "memory"] {
            let morpher = stopping_morpher(&g, stop);
            let pinned = confirm_pin_groups(&g, &morpher, "k", &groups, &fresh(&morpher));
            assert!(pinned.incomplete(), "stop={stop}: stop flag ignored");
            let (_, batch_stop) =
                confirm_batch_checked(&g, &owners, &morpher, std::slice::from_ref(&candidate), "k");
            assert_eq!(pinned.stop, batch_stop, "stop={stop}: entries disagree");
            let unfiltered = confirm_unfiltered(&morpher, "k", &fresh(&morpher));
            assert!(unfiltered.incomplete(), "stop={stop}: unfiltered");
        }

        let (_, batch_stop) = confirm_batch_checked(&g, &owners, &ok, &[candidate], "k\u{1F600}");
        let pinned = confirm_pin_groups(&g, &ok, "k\u{1F600}", &groups, &fresh(&ok));
        assert!(pinned.stop.invalid_shape);
        assert_eq!(pinned.stop, batch_stop);
        assert!(pinned.analyses.is_empty());
    }

    /// A shared allowance that latches in one group stops the next reparse too.
    #[test]
    fn a_latched_shared_allowance_stops_later_groups() {
        let g = fixture();
        let morpher = Morpher::new(&g, 1);
        let rules = all_rules(&g);
        let groups: Vec<PinGroup> = all_roots(&g).iter().map(|r| group(&[*r], &rules)).collect();
        let got = confirm_pin_groups(&g, &morpher, "kpq", &groups, &fresh(&morpher));
        assert!(got.stop.capped);
        assert_eq!(got.reparses, groups.len());
    }

    /// L1/L2 (T2) over seeded random root and rule supersets: whatever lies inside the filters comes back with HC's multiplicity, nothing outside HC's multiset does.
    #[test]
    fn l1_l2_property_over_random_supersets() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        let owners = build_morpheme_owners(&g);
        let roots = all_roots(&g);
        let rules = all_rules(&g);
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut checked_l1 = 0usize;
        for word in WORDS {
            let want = plain(&morpher, word);
            for _ in 0..40 {
                let rs: Vec<LexEntryId> =
                    roots.iter().copied().filter(|_| next() & 1 == 1).collect();
                let ru: Vec<MRuleId> = rules.iter().copied().filter(|_| next() & 1 == 1).collect();
                let got =
                    confirm_pin_groups(&g, &morpher, word, &[group(&rs, &ru)], &fresh(&morpher));
                assert!(!got.incomplete());
                let got = sorted(keys(&got.analyses));
                // L2: sub-multiset of plain HC.
                let mut pool = want.clone();
                for k in &got {
                    let pos = pool
                        .iter()
                        .position(|w| w == k)
                        .unwrap_or_else(|| panic!("{word}: {k:?} is not a plain HC analysis"));
                    pool.remove(pos);
                }
                // L1: every analysis inside the filters is there, with its multiplicity.
                for k in &want {
                    let (head, used) = pins_of(&owners, k);
                    if rs.contains(&head) && used.iter().all(|r| ru.contains(r)) {
                        let have = got.iter().filter(|x| *x == k).count();
                        let need = want.iter().filter(|x| *x == k).count();
                        assert_eq!(have, need, "{word}: {k:?} lost under {rs:?} {ru:?}");
                        checked_l1 += 1;
                    }
                }
            }
        }
        assert!(
            checked_l1 > 100,
            "the property must exercise L1: {checked_l1}"
        );
    }

    fn dup_fixture() -> Grammar {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput><Language><Name>DupFixture</Name>
<PartsOfSpeech><PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="t1"><Name>Main</Name><SegmentDefinitions>
<SegmentDefinition id="ck"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cr"><Representations><Representation>r</Representation></Representations></SegmentDefinition>
</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
<Strata><Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered" morphologicalRules="mrR"><Name>Main</Name>
<MorphologicalRuleDefinitions>
<MorphologicalRule id="mrR" requiredPartsOfSpeech="posV" outputPartOfSpeech="posV"><Name>R</Name>
<MorphologicalSubrules>
<MorphologicalSubrule id="subR1"><MorphologicalInput><PhoneticSequence id="stemR1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
<MorphologicalOutput><CopyFromInput index="stemR1" /><InsertSegments><PhoneticShape>r</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
<MorphologicalSubrule id="subR2"><MorphologicalInput><PhoneticSequence id="stemR2"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
<MorphologicalOutput><CopyFromInput index="stemR2" /><InsertSegments><PhoneticShape>r</PhoneticShape></InsertSegments></MorphologicalOutput></MorphologicalSubrule>
</MorphologicalSubrules><MorphemeId>R</MorphemeId></MorphologicalRule>
</MorphologicalRuleDefinitions>
<LexicalEntries><LexicalEntry id="eK" partOfSpeech="posV"><Allomorphs><Allomorph id="aK"><PhoneticShape>k</PhoneticShape></Allomorph></Allomorphs><MorphemeId>K</MorphemeId></LexicalEntry></LexicalEntries>
</Stratum></Strata></Language></HermitCrabInput>"#;
        pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
    }

    /// Two derivations spelling the same morpheme sequence: HC returns the key twice and so must the pinned entry.
    #[test]
    fn identical_key_duplicate_analyses_are_both_kept() {
        let g = dup_fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        let want = plain(&morpher, "kr");
        assert_eq!(want.len(), 2, "fixture must give HC a duplicate: {want:?}");
        assert_eq!(want[0], want[1]);
        let all = group(&all_roots(&g), &all_rules(&g));
        let got = confirm_pin_groups(&g, &morpher, "kr", &[all], &fresh(&morpher));
        assert_eq!(sorted(keys(&got.analyses)), want);
        let got = confirm_unfiltered(&morpher, "kr", &fresh(&morpher));
        assert_eq!(sorted(keys(&got.analyses)), want);
    }

    /// Only the first group's allowance stops; the last group completes cleanly. The union must keep the earlier stop.
    #[test]
    fn an_earlier_groups_stop_survives_a_clean_last_group() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        let tight = Morpher::new(&g, 60).word_budget();
        let roomy = morpher.word_budget();
        let rules = all_rules(&g);
        let first = group(&[LexEntryId(0), LexEntryId(1)], &rules);
        let last = group(&[LexEntryId(2)], &rules);
        let budgets = [&tight, &roomy];
        let got = reparse_groups(&g, &morpher, "kpq", &[last, first], &mut |k| budgets[k]);
        assert_eq!(got.reparses, 2);
        assert!(got.stop.capped, "the first group's stop was overwritten");
        assert!(got.incomplete());
        assert!(
            got.analyses.is_empty(),
            "a stopped call returns no analyses"
        );
        let roomy_alone = morpher.word_budget();
        let last_only = group(&[LexEntryId(2)], &rules);
        let clean = confirm_pin_groups(&g, &morpher, "kpq", &[last_only], &roomy_alone);
        assert!(!clean.incomplete(), "the last group alone must complete");
    }

    /// A stop in any one group, first or last, empties the analyses of the groups that completed.
    #[test]
    fn a_stop_in_any_group_empties_the_analyses() {
        let g = fixture();
        let morpher = Morpher::new(&g, usize::MAX);
        let tight = Morpher::new(&g, 1).word_budget();
        let rules = all_rules(&g);
        let ks = group(&[LexEntryId(0), LexEntryId(1)], &rules);
        let l = group(&[LexEntryId(2)], &rules);
        for stop_at in [0usize, 1] {
            let roomy = morpher.word_budget();
            let budgets = if stop_at == 0 {
                [&tight, &roomy]
            } else {
                [&roomy, &tight]
            };
            let got = reparse_groups(&g, &morpher, "k", &[ks.clone(), l.clone()], &mut |k| {
                budgets[k]
            });
            assert!(got.stop.capped, "stop_at={stop_at}");
            assert!(got.analyses.is_empty(), "stop_at={stop_at}: analyses kept");
        }
        let ok = confirm_pin_groups(&g, &morpher, "k", &[ks], &morpher.word_budget());
        assert!(!ok.incomplete() && !ok.analyses.is_empty());
    }
}
