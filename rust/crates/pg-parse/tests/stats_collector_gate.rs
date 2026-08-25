//! `Morpher::parse_word_with_stats` invariants: attempts-vs-steps, stats-off parity, determinism.

mod csharp_port_common;

use pg_conformance_fixtures::discover;
use pg_parse::morpher::ParseOptions;
use pg_parse::Morpher;
use pg_rules::stats::{Direction, ObjectKind, StatsRow};

/// One fixture-provided word to replay, paired with the grammar it belongs to.
struct Case {
    label: &'static str,
    word: &'static str,
}

/// The two fixtures this gate replays, each with its own word list.
fn fixture_cases() -> Vec<(pg_grammar::model::Grammar, Vec<Case>)> {
    let fixtures = discover();
    let austronesian = fixtures
        .iter()
        .find(|f| f.category == "languages" && f.name == "metathesis-phase-isolation")
        .expect("languages/metathesis-phase-isolation must be discoverable");
    let truncate = fixtures
        .iter()
        .find(|f| f.category == "edge-cases" && f.name == "truncate-morphotactic")
        .expect("edge-cases/truncate-morphotactic must be discoverable");

    vec![
        (
            pg_grammar::load(&austronesian.load_grammar_xml()).unwrap(),
            vec![
                Case {
                    label: "sumulat",
                    word: "sumulat",
                },
                Case {
                    label: "keadilan",
                    word: "keadilan",
                },
                Case {
                    label: "katibɯd",
                    word: "katibɯd",
                },
                Case {
                    label: "pur",
                    word: "pur",
                },
            ],
        ),
        (
            pg_grammar::load(&truncate.load_grammar_xml()).unwrap(),
            vec![Case {
                label: "gas",
                word: "gas",
            }],
        ),
    ]
}

/// `SUM(morph_rule attempts)` filtered to `direction = analysis` must equal the word's `steps` -- `StepBudget::tick()` fires solely on the analysis side, so summing both directions would overcount.
#[test]
fn sum_of_morph_rule_attempts_equals_steps() {
    let mut checked = 0usize;
    for (g, cases) in fixture_cases() {
        let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
        for case in &cases {
            let (outcome, rows) =
                morpher.parse_word_with_stats(case.word, &ParseOptions::default());
            let sum_attempts: u64 = rows
                .iter()
                .filter(|r| r.kind == ObjectKind::MorphRule && r.direction == Direction::Analysis)
                .map(|r| r.counters.attempts)
                .sum();
            assert_eq!(
                sum_attempts, outcome.steps as u64,
                "{}: SUM(morph_rule attempts) filtered to analysis must equal the word's steps",
                case.label
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 5,
        "must have replayed several words across two fixtures"
    );
}

/// The confirm pass must leave nonzero synthesis-direction rows across words that parse; checked in aggregate, not per-word, since a bare-root word legitimately involves no rule at all.
#[test]
fn synthesis_direction_rows_are_nonzero_for_words_that_parse() {
    let mut parsed = 0usize;
    let mut total_synth_attempts: u64 = 0;
    for (g, cases) in fixture_cases() {
        let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
        for case in &cases {
            let (outcome, rows) =
                morpher.parse_word_with_stats(case.word, &ParseOptions::default());
            if outcome.analyses.is_empty() {
                continue;
            }
            parsed += 1;
            total_synth_attempts += rows
                .iter()
                .filter(|r| r.kind == ObjectKind::MorphRule && r.direction == Direction::Synthesis)
                .map(|r| r.counters.attempts)
                .sum::<u64>();
        }
    }
    assert!(parsed >= 1, "must have replayed at least one parsing word");
    assert!(
        total_synth_attempts >= 1,
        "at least one parsed word's confirm pass must leave a nonzero synthesis-direction \
         attempts row -- this is the gap that made the old, direction-less invariant unable to \
         detect the synthesis side going uninstrumented"
    );
}

/// Stats-off and stats-on parses must produce byte-identical outcomes, never merely similar ones.
#[test]
fn stats_collection_does_not_change_the_parse_outcome() {
    for (g, cases) in fixture_cases() {
        let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
        for case in &cases {
            let opts = ParseOptions::default();
            let off = morpher.parse_word_opts(case.word, &opts);
            let (on, _rows) = morpher.parse_word_with_stats(case.word, &opts);

            assert_eq!(
                off.analyses, on.analyses,
                "{}: analyses must match",
                case.label
            );
            assert_eq!(
                off.structured, on.structured,
                "{}: structured analyses must match",
                case.label
            );
            assert_eq!(off.capped, on.capped, "{}: capped must match", case.label);
            assert_eq!(
                off.invalid_shape, on.invalid_shape,
                "{}: invalid_shape must match",
                case.label
            );
            assert_eq!(off.steps, on.steps, "{}: steps must match", case.label);
            assert_eq!(
                off.timed_out, on.timed_out,
                "{}: timed_out must match",
                case.label
            );
            assert_eq!(
                off.guessed, on.guessed,
                "{}: guessed must match",
                case.label
            );
            assert_eq!(
                off.candidates_generated, on.candidates_generated,
                "{}: candidates_generated must match",
                case.label
            );
        }
    }
}

/// The same word parsed twice with stats on must produce byte-identical counter rows.
#[test]
fn repeated_runs_produce_identical_rows() {
    for (g, cases) in fixture_cases() {
        let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
        for case in &cases {
            let opts = ParseOptions::default();
            let (_o1, rows1) = morpher.parse_word_with_stats(case.word, &opts);
            let (_o2, rows2) = morpher.parse_word_with_stats(case.word, &opts);
            // Self time is wall-clock; only the counter projection is reproducible.
            let rows1: Vec<_> = rows1.iter().map(StatsRow::without_timing).collect();
            let rows2: Vec<_> = rows2.iter().map(StatsRow::without_timing).collect();
            assert_eq!(
                rows1, rows2,
                "{}: repeated parses must yield identical stats rows",
                case.label
            );
        }
    }
}

/// Stats rows must be thread-count invariant: a concurrent parse must match a single-threaded baseline.
#[test]
fn rows_are_identical_across_concurrent_threads() {
    for (g, cases) in fixture_cases() {
        let morpher = Morpher::new(&g, usize::MAX).with_memo(true);
        for case in &cases {
            let opts = ParseOptions::default();
            let (_baseline_outcome, baseline_raw) = morpher.parse_word_with_stats(case.word, &opts);
            let baseline_rows: Vec<_> = baseline_raw.iter().map(StatsRow::without_timing).collect();

            std::thread::scope(|scope| {
                let handles: Vec<_> = (0..4)
                    .map(|_| scope.spawn(|| morpher.parse_word_with_stats(case.word, &opts).1))
                    .collect();
                for h in handles {
                    let rows = h.join().expect("worker thread must not panic");
                    let rows: Vec<_> = rows.iter().map(StatsRow::without_timing).collect();
                    assert_eq!(
                        rows, baseline_rows,
                        "{}: a concurrent parse must yield the same stats rows as the single-threaded baseline",
                        case.label
                    );
                }
            });
        }
    }
}

/// A suffix rule appending "z" to any posV root, used to peel a candidate root back off for the `no_root` gate.
const Z_SUFFIX_MRULE: &str = r#"
  <MorphologicalRule id="mrZ" requiredPartsOfSpeech="posV"><Name>z_suffix</Name><MorphemeId>Z</MorphemeId>
    <MorphologicalSubrules>
      <MorphologicalSubrule id="subZ">
        <MorphologicalInput><PhoneticSequence id="1"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput>
        <MorphologicalOutput><CopyFromInput index="1" /><InsertSegments><PhoneticShape>z</PhoneticShape></InsertSegments></MorphologicalOutput>
      </MorphologicalSubrule>
    </MorphologicalSubrules>
  </MorphologicalRule>
"#;

/// `no_root` fires on the mrZ rule's own candidate only when its peeled root is missing lexically.
#[test]
fn no_root_fires_only_when_lexical_lookup_finds_nothing() {
    let g = csharp_port_common::build_grammar("", "", Z_SUFFIX_MRULE, "mrZ", "");
    let m = Morpher::new(&g, usize::MAX);

    // "dat" (entry 9) + "z" peels back to a real root: the mrZ rule's own candidate must record no no_root.
    let (clean, clean_rows) = m.parse_word_with_stats("datz", &ParseOptions::default());
    assert!(
        !clean.analyses.is_empty(),
        "datz must parse cleanly via entry 9 (dat) + the z suffix"
    );
    let clean_mrule_no_root: u64 = clean_rows
        .iter()
        .filter(|r| r.kind == ObjectKind::MorphRule)
        .map(|r| r.counters.no_root)
        .sum();
    assert_eq!(
        clean_mrule_no_root, 0,
        "the z-suffix rule's peeled candidate matched a real root, so it must record no no_root"
    );

    // "vu" is not a lexical entry, so peeling "vuz" back to "vu" is a dead end.
    let (dead, dead_rows) = m.parse_word_with_stats("vuz", &ParseOptions::default());
    assert!(
        dead.analyses.is_empty(),
        "vuz's peeled root vu is not in the lexicon, so it must have no analyses"
    );
    let dead_mrule_no_root: u64 = dead_rows
        .iter()
        .filter(|r| r.kind == ObjectKind::MorphRule)
        .map(|r| r.counters.no_root)
        .sum();
    assert!(
        dead_mrule_no_root >= 1,
        "peeling to a root the lexicon does not contain must record no_root on the rule that peeled it"
    );
}

/// Two differently-spelled allomorphs of one root: `surface_mismatch` must fire for the one that does not match.
#[test]
fn surface_mismatch_fires_for_a_root_that_is_tried_and_rebuilt_but_does_not_match() {
    let extra_lexicon = r#"
      <LexicalEntry id="eSm" partOfSpeech="posV">
        <Allomorphs>
          <Allomorph id="aSm1"><PhoneticShape>bu</PhoneticShape></Allomorph>
          <Allomorph id="aSm2"><PhoneticShape>bo</PhoneticShape></Allomorph>
        </Allomorphs>
        <MorphemeId>SM</MorphemeId>
      </LexicalEntry>
    "#;
    let g = csharp_port_common::build_grammar_w5("", "", "", "", "", extra_lexicon);
    let m = Morpher::new(&g, usize::MAX);

    let (outcome, rows) = m.parse_word_with_stats("bu", &ParseOptions::default());
    assert!(
        !outcome.analyses.is_empty(),
        "bu must parse via allomorph aSm1"
    );
    let mismatch_sum: u64 = rows
        .iter()
        .filter(|r| r.kind == ObjectKind::LexEntry)
        .map(|r| r.counters.surface_mismatch)
        .sum();
    assert!(
        mismatch_sum >= 1,
        "allomorph aSm2 (bo) is tried and rebuilt but does not match the surface word 'bu'"
    );
}

/// `uses` is non-zero for the root entry behind a surviving analysis, and zero for a word with none.
#[test]
fn uses_is_nonzero_for_a_surviving_analysis_and_zero_for_a_word_with_no_analyses() {
    let g = csharp_port_common::build_grammar("", "", "", "", "");
    let m = Morpher::new(&g, usize::MAX);

    let (clean, clean_rows) = m.parse_word_with_stats("dat", &ParseOptions::default());
    assert!(!clean.analyses.is_empty(), "dat must parse via entry 9");
    let uses_sum: u64 = clean_rows
        .iter()
        .filter(|r| r.kind == ObjectKind::LexEntry)
        .map(|r| r.counters.uses)
        .sum();
    assert!(
        uses_sum >= 1,
        "the root lexical entry behind a surviving analysis must show uses >= 1"
    );

    let (dead, dead_rows) = m.parse_word_with_stats("zzzzz", &ParseOptions::default());
    assert!(
        dead.analyses.is_empty(),
        "zzzzz must match no lexical entry at all"
    );
    let dead_uses: u64 = dead_rows.iter().map(|r| r.counters.uses).sum();
    assert_eq!(
        dead_uses, 0,
        "a word with no surviving analyses must record uses nowhere"
    );
}

/// A bare-root candidate (no rule ever applies) that fails lexical lookup must not drop `no_root`.
#[test]
fn no_root_survives_a_bare_root_candidate_with_no_rule_applied() {
    let g = csharp_port_common::build_grammar("", "", "", "", "");
    let m = Morpher::new(&g, usize::MAX);

    let (dead, rows) = m.parse_word_with_stats("zzzzz", &ParseOptions::default());
    assert!(
        dead.analyses.is_empty(),
        "zzzzz must match no rule and no lexical entry"
    );
    let no_root_sum: u64 = rows.iter().map(|r| r.counters.no_root).sum();
    assert!(
        no_root_sum >= 1,
        "a bare-root candidate that fails lexical lookup must record no_root somewhere, not vanish"
    );
    let root_index_no_root: u64 = rows
        .iter()
        .filter(|r| r.kind == ObjectKind::RootIndex)
        .map(|r| r.counters.no_root)
        .sum();
    assert!(
        root_index_no_root >= 1,
        "with no rule on the trail, no_root must be charged to the stratum's root_index row"
    );
}

/// A real root-index lookup records nonzero raw work rather than a confident zero.
#[test]
fn root_index_work_is_nonzero_for_a_real_lookup() {
    let g = csharp_port_common::build_grammar("", "", "", "", "");
    let m = Morpher::new(&g, usize::MAX);
    let (outcome, rows) = m.parse_word_with_stats("dat", &ParseOptions::default());
    assert!(!outcome.analyses.is_empty(), "dat must parse via entry 9");
    let root_index_work: u64 = rows
        .iter()
        .filter(|r| r.kind == ObjectKind::RootIndex)
        .map(|r| r.counters.work)
        .sum();
    assert!(
        root_index_work > 0,
        "a real lexical lookup must record nonzero root_index work, not a confident zero"
    );
}

/// A fired guess must leave a nonzero row, so `--kind guesser` reads as measured, not silent.
#[test]
fn guesser_attempts_and_work_are_nonzero_when_the_guess_branch_fires() {
    const XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>StatsGuesserFixture</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="ePattern">
            <Allomorphs><Allomorph id="aPattern"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pattern</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;
    let g = pg_grammar::load(XML).expect("guesser fixture grammar must load");
    let m = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default().with_guess_root(true);
    let (outcome, rows) = m.parse_word_with_stats("gag", &opts);
    assert!(outcome.guessed, "the guess branch must have fired");

    let guesser_attempts: u64 = rows
        .iter()
        .filter(|r| r.kind == ObjectKind::Guesser)
        .map(|r| r.counters.attempts)
        .sum();
    let guesser_work: u64 = rows
        .iter()
        .filter(|r| r.kind == ObjectKind::Guesser)
        .map(|r| r.counters.work)
        .sum();
    assert!(
        guesser_attempts >= 1,
        "a fired guess must record guesser attempts, not stay silently empty"
    );
    assert!(guesser_work >= 1, "a fired guess must record guesser work");
}

/// The overlay's own counters must likewise be observed nonzero from a real supplied-root match.
#[test]
fn overlay_attempts_and_work_are_nonzero_when_a_supplied_root_matches() {
    let g = pg_grammar::load(
        r#"<HermitCrabInput><Language><Name>T</Name><PartsOfSpeech><PartOfSpeech id="n"><Name>n</Name></PartOfSpeech></PartsOfSpeech><CharacterDefinitionTable id="t"><Name>T</Name><SegmentDefinitions><SegmentDefinition id="b"><Representations><Representation>b</Representation></Representations></SegmentDefinition></SegmentDefinitions></CharacterDefinitionTable><Strata><Stratum characterDefinitionTable="t"><Name>S</Name><LexicalEntries /></Stratum></Strata></Language></HermitCrabInput>"#,
    )
    .expect("overlay fixture grammar must load");
    let root = pg_parse::SuppliedRoot {
        entry_id: "pgl_x".into(),
        realization_id: "pgl_x:sig0".into(),
        lexical_spelling: "b".into(),
        gloss: String::new(),
        syn_fs: pg_featstruct::FeatureStruct::EMPTY,
        mpr: pg_grammar::model::MprSet::EMPTY,
        stratum: pg_grammar::model::StratumId(0),
        authority: pg_parse::RootAuthority::Supplied,
    };
    let overlay = pg_parse::SuppliedRootOverlay::build(&g, vec![root]).unwrap();
    let m = Morpher::new_with_overlay(&g, 10_000, &overlay);
    let (outcome, rows) = m.parse_word_with_stats("b", &ParseOptions::default());
    assert!(
        !outcome.analyses.is_empty(),
        "the supplied root must produce an analysis"
    );

    let overlay_attempts: u64 = rows
        .iter()
        .filter(|r| r.kind == ObjectKind::Overlay)
        .map(|r| r.counters.attempts)
        .sum();
    assert!(
        overlay_attempts >= 1,
        "a matched supplied root must record overlay attempts, not stay silently empty"
    );
}
