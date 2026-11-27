//! The proposer offers, and confirmation keeps exactly, every HC identity for words whose tag paths repeat a morpheme; expected signatures come from the pinned C# oracle in each fixture's `words.yaml`.

use std::sync::Arc;

use pg_foma::candidate_filter::index::FilterIndex;
use pg_foma::candidate_filter::pipeline::FilterMode;
use pg_foma::candidate_filter::shadow::CandidateFilterSettings;
use pg_foma::candidate_filter::test_support::filter_of;
use pg_foma::candidate_filter::{OwnershipPass, StructuralTransitionPass};
use pg_foma::compose_budget::{ApplyBudget, ApplyDimension, ApplyOutcome};
use pg_foma::differential::{measure_word, DifferentialRow};
use pg_foma::tags::{to_candidates, RawPath};
use pg_grammar::model::{Grammar, MorphemeId};
use pg_parse::Morpher;

const TWO: &str = "compound-allomorph-identity";
const DISTINCT: &str = "compound-allomorph-identity-distinct-entries";
const SEPARATED: &str = "compound-allomorph-identity-separated";
const AFFIX: &str = "compound-allomorph-identity-affix-reuse";
const SAME_SPELLING: &str = "compound-allomorph-identity-same-spelling";
const ALL: [&str; 5] = [TWO, DISTINCT, SEPARATED, AFFIX, SAME_SPELLING];

fn fixture(name: &str) -> pg_conformance_fixtures::FixtureRef {
    pg_conformance_fixtures::require_fixture("edge-cases", name)
}

/// The fixture's own words plus every string of length 1..=4 over the two-letter inventory, so the neighbors of each declared word are measured too.
fn words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = fixture(name)
        .load_words_yaml()
        .words
        .into_iter()
        .map(|w| w.word)
        .collect();
    let mut frontier = vec![String::new()];
    for _ in 0..4 {
        let mut next = Vec::new();
        for base in &frontier {
            for letter in ["a", "p"] {
                next.push(format!("{base}{letter}"));
            }
        }
        words.extend(next.iter().cloned());
        frontier = next;
    }
    words.sort_unstable();
    words.dedup();
    words
}

/// Morphemes joined by `+`, head position and occurrence count.
type Shape = (String, i32, usize);

/// An expected row: morphemes joined by `+`, head position and occurrence count.
type Row = (&'static str, i32, usize);

/// A pinned word: fixture, word and its expected rows.
type Pin = (&'static str, &'static str, &'static [Row]);

fn shape(multiset: &[pg_foma::differential::IdentityMultiplicity]) -> Vec<Shape> {
    let mut out: Vec<_> = multiset
        .iter()
        .map(|m| {
            let keys: Vec<&str> = m
                .identity
                .morphemes
                .iter()
                .map(|k| k.as_deref().expect("fixture morphemes are keyed"))
                .collect();
            (keys.join("+"), m.identity.root_index, m.count)
        })
        .collect();
    out.sort();
    out
}

fn expected(rows: &[Row]) -> Vec<Shape> {
    let mut out: Vec<_> = rows
        .iter()
        .map(|&(m, r, c)| (m.to_string(), r, c))
        .collect();
    out.sort();
    out
}

fn measure_all(name: &str, grammar: &Grammar, mut each: impl FnMut(&str, &DifferentialRow)) {
    let morpher = Morpher::new(grammar, usize::MAX);
    let mut analyzer = pg_foma::composite::compile_analyzer(grammar).expect("fixture compiles");
    for word in words(name) {
        let row = measure_word(
            grammar,
            &morpher,
            Some(&mut analyzer),
            0,
            &word,
            &ApplyBudget::unbounded(),
            None,
        )
        .unwrap();
        each(&word, &row);
    }
}

/// Complete, identical multiset, no proposal miss, no surviving extra: the hard recall and soundness assertions.
fn assert_exact(name: &str, word: &str, row: &DifferentialRow) {
    assert_eq!(row.hc_status, "complete", "{name}: {word}");
    assert_eq!(row.fst_status, "complete", "{name}: {word}");
    assert!(!row.incomplete, "{name}: {word}: {row:?}");
    assert_eq!(
        row.proposal_misses.as_deref(),
        Some(&[][..]),
        "{name}: {word}: a missing pre-confirmation identity"
    );
    assert_eq!(
        row.spurious_confirmed.as_deref(),
        Some(&[][..]),
        "{name}: {word}: a surviving extra"
    );
    assert_eq!(
        row.confirmed_multiset_equal,
        Some(true),
        "{name}: {word}: {row:?}"
    );
}

#[test]
fn every_fixture_recovers_every_identity_and_multiplicity_with_no_extras() {
    for name in ALL {
        let f = fixture(name);
        let g = f.load_grammar();
        let declared = f.load_words_yaml();
        let mut measured = 0;
        let mut repeated = 0;
        measure_all(name, &g, |word, row| {
            assert_exact(name, word, row);
            measured += 1;
            if let Some(entry) = declared.words.iter().find(|w| w.word == word) {
                let hc: usize = row.oracle.iter().map(|m| m.count).sum();
                assert_eq!(
                    hc,
                    entry.parses.len(),
                    "{name}: {word}: HC occurrence count differs from the C# signature count"
                );
            }
            repeated += row
                .oracle
                .iter()
                .filter(|m| {
                    let mut keys = m.identity.morphemes.clone();
                    keys.sort();
                    keys.windows(2).any(|pair| pair[0] == pair[1])
                })
                .count();
        });
        assert!(measured >= 30, "{name}: only {measured} words measured");
        let needs_repeats = name != AFFIX || repeated > 0;
        assert!(
            needs_repeats && repeated > 0,
            "{name}: no repeated-morpheme identity was exercised"
        );
    }
}

#[test]
fn pinned_identity_multisets_are_recovered_exactly() {
    let pins: &[Pin] = &[
        (TWO, "a", &[("root", 0, 1)]),
        (TWO, "aa", &[("root", 0, 1)]),
        (TWO, "ap", &[("root+root", 0, 1)]),
        (TWO, "pa", &[("root+root", 0, 1)]),
        (TWO, "pp", &[("root", 0, 1)]),
        (TWO, "aaa", &[]),
        (DISTINCT, "pp", &[("x", 0, 2), ("y", 0, 1)]),
        (DISTINCT, "ap", &[("x+x", 0, 1), ("x+x", 1, 1)]),
        (DISTINCT, "app", &[("x+y", 0, 1), ("x+y", 1, 1)]),
        (DISTINCT, "ppa", &[("y+x", 0, 1), ("y+x", 1, 1)]),
        (
            DISTINCT,
            "ppp",
            &[("x+y", 0, 1), ("x+y", 1, 1), ("y+x", 0, 1), ("y+x", 1, 1)],
        ),
        (SEPARATED, "pa", &[("PRE+root", 1, 1), ("root+root", 0, 1)]),
        (SEPARATED, "pp", &[("PRE+root", 1, 1), ("root", 0, 1)]),
        (SEPARATED, "ppa", &[("PRE+root+root", 1, 2)]),
        (SEPARATED, "pap", &[("PRE+root+root", 1, 2)]),
        (SEPARATED, "paa", &[("PRE+root", 1, 2)]),
        (AFFIX, "aa", &[("root+SUF", 0, 1)]),
        (AFFIX, "aaa", &[("root+SUF", 0, 1)]),
        (AFFIX, "aap", &[("root+SUF+SUF", 0, 1)]),
        (AFFIX, "apa", &[("root+SUF+SUF", 0, 1)]),
        (AFFIX, "app", &[("root+SUF", 0, 1)]),
        (SAME_SPELLING, "a", &[("root", 0, 2)]),
        (SAME_SPELLING, "aa", &[("root", 0, 2), ("root+root", 0, 2)]),
    ];
    let mut mismatches = Vec::new();
    for name in ALL {
        let g = fixture(name).load_grammar();
        measure_all(name, &g, |word, row| {
            if let Some((_, _, want)) = pins.iter().find(|(n, w, _)| *n == name && *w == word) {
                for got in [shape(&row.confirmed), shape(&row.oracle)] {
                    if got != expected(want) {
                        mismatches.push(format!("{name}: {word}: {got:?}"));
                    }
                }
            }
        });
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    for (name, word, _) in pins {
        assert!(
            words(name).iter().any(|w| w == word),
            "{name}: pinned word {word} is never measured"
        );
    }
}

/// The legacy normalization cannot represent the second occurrence; the proposer must offer it before confirmation sees anything.
#[test]
fn the_proposer_offers_the_two_occurrence_identity_the_legacy_decoder_erases() {
    let g = fixture(TWO).load_grammar();
    let root = g
        .morphemes
        .iter()
        .position(|m| m.xml_key == "root")
        .expect("root morpheme") as u32;
    let path: RawPath = vec![(true, MorphemeId(root)), (true, MorphemeId(root))];
    assert!(
        to_candidates(&path).iter().all(|c| c.morphemes.len() == 1),
        "the legacy decoder is expected to keep only the merged identity"
    );

    let mut proposer = pg_foma::analyzer::compile_proposer(&g).expect("fixture compiles");
    let mut offered: Vec<(usize, i32)> = proposer
        .propose("ap")
        .into_iter()
        .map(|c| (c.morphemes.len(), c.root_index))
        .collect();
    offered.sort();
    assert_eq!(offered, vec![(1, 0), (2, 0), (2, 1)]);

    let mut distinct = pg_foma::analyzer::compile_proposer(&fixture(DISTINCT).load_grammar())
        .expect("fixture compiles");
    let mut offered: Vec<(usize, i32)> = distinct
        .propose("app")
        .into_iter()
        .map(|c| (c.morphemes.len(), c.root_index))
        .collect();
    offered.sort();
    assert_eq!(
        offered,
        vec![
            (1, 0),
            (2, 0),
            (2, 0),
            (2, 1),
            (2, 1),
            (3, 0),
            (3, 1),
            (3, 2)
        ],
        "the offers for distinct entries under two compounding rules changed"
    );
}

fn filter_settings(g: &Grammar, mode: FilterMode) -> CandidateFilterSettings {
    let index = Arc::new(FilterIndex::build(g));
    CandidateFilterSettings::new(
        mode,
        Arc::new(filter_of(vec![
            Box::new(OwnershipPass::new(Arc::clone(&index))),
            Box::new(StructuralTransitionPass::new(Arc::clone(&index))),
        ])),
        index,
        1,
        1,
    )
}

#[test]
fn off_shadow_and_enforce_filtering_keep_the_same_exact_multisets() {
    for name in ALL {
        let g = fixture(name).load_grammar();
        let morpher = Morpher::new(&g, usize::MAX);
        let mut baseline: Vec<(String, Vec<Shape>)> = Vec::new();
        for mode in [FilterMode::Off, FilterMode::Shadow, FilterMode::Enforce] {
            let mut analyzer = pg_foma::composite::compile_analyzer(&g).unwrap();
            analyzer.set_candidate_filter(filter_settings(&g, mode));
            assert_eq!(analyzer.candidate_filter().mode(), mode);
            let mut seen = Vec::new();
            for word in words(name) {
                let row = measure_word(
                    &g,
                    &morpher,
                    Some(&mut analyzer),
                    0,
                    &word,
                    &ApplyBudget::unbounded(),
                    None,
                )
                .unwrap();
                assert_exact(&format!("{name} [{mode:?}]"), &word, &row);
                if mode != FilterMode::Off {
                    let profiled = analyzer.analyze_word_with_diagnostics(&word);
                    assert_eq!(
                        profiled.diagnostics.filter.shadow_false_rejections, 0,
                        "{name} [{mode:?}]: {word}: a filter pass rejected a confirmed identity"
                    );
                }
                seen.push((word, shape(&row.confirmed)));
            }
            if baseline.is_empty() {
                baseline = seen;
            } else {
                assert_eq!(seen, baseline, "{name} [{mode:?}] differs from Off");
            }
        }
    }
}

/// The word with the most revisited alternatives: three raw paths, 15 distinct candidates, 19 recovered alternatives counted before deduplication.
const REVISITED: &str = "pppp";

#[test]
fn the_normalization_allowance_trips_exactly_one_past_its_cap_in_both_loops() {
    let g = fixture(SEPARATED).load_grammar();
    let mut proposer = pg_foma::analyzer::compile_proposer(&g).unwrap();

    let (outcome, counts) = proposer.propose_budgeted_counted(REVISITED, &ApplyBudget::unbounded());
    assert!(matches!(outcome, ApplyOutcome::Complete(ref c) if c.len() == 15));
    assert_eq!(
        (
            counts.raw_paths,
            counts.unique_candidates,
            counts.normalized_alternatives
        ),
        (3, 15, 19)
    );

    for cap in [19usize, 20, 1_000] {
        let budget = ApplyBudget::with_caps(None, Some(cap));
        let (counted, counts) = proposer.propose_budgeted_counted(REVISITED, &budget);
        assert!(
            matches!(counted, ApplyOutcome::Complete(ref c) if c.len() == 15),
            "cap {cap} is enough and must complete"
        );
        assert_eq!(counts.normalized_alternatives, 19);
    }

    for cap in [15usize, 16, 17, 18] {
        let budget = ApplyBudget::with_caps(None, Some(cap));
        let (counted, counts) = proposer.propose_budgeted_counted(REVISITED, &budget);
        let (diagnostic, diagnostics) =
            proposer.propose_with_diagnostics_budgeted(REVISITED, &budget);
        assert_eq!(
            counted,
            ApplyOutcome::Incomplete {
                dimension: ApplyDimension::NormalizedCandidates,
                value: cap + 1,
                limit: cap
            },
            "cap {cap}: unique candidates (15) never exceed it, so only the pre-deduplication count can trip"
        );
        assert_eq!(counted, diagnostic, "cap {cap}: the two loops disagree");
        assert_eq!(
            (counts.unique_candidates, counts.normalized_alternatives),
            (
                diagnostics.unique_candidates,
                diagnostics.normalized_alternatives
            )
        );
    }

    let budget = ApplyBudget::with_caps(None, Some(14));
    let (outcome, _) = proposer.propose_budgeted_counted(REVISITED, &budget);
    assert!(
        matches!(outcome, ApplyOutcome::Incomplete { limit: 14, .. }),
        "below the distinct count the ordinary candidate allowance trips"
    );
}

#[test]
fn a_tripped_normalization_allowance_is_an_incomplete_row_never_a_verdict() {
    let g = fixture(SEPARATED).load_grammar();
    let morpher = Morpher::new(&g, usize::MAX);
    let mut analyzer = pg_foma::composite::compile_analyzer(&g).unwrap();
    let row = measure_word(
        &g,
        &morpher,
        Some(&mut analyzer),
        0,
        REVISITED,
        &ApplyBudget::with_caps(None, Some(18)),
        None,
    )
    .unwrap();
    assert_eq!(
        row.fst_status,
        "apply_limit:repeat-normalization alternatives:19/18"
    );
    assert!(row.incomplete);
    assert!(row.confirmed.is_empty());
    assert_eq!(row.confirmed_multiset_equal, None);
    assert_eq!(row.proposal_misses, None);
}

#[test]
fn a_stopped_confirmation_is_incomplete_not_a_missing_identity() {
    let g = fixture(TWO).load_grammar();
    let analyzer = pg_foma::composite::compile_analyzer(&g).unwrap();
    let (proposer, peeler, owners, _, filter) = analyzer.into_parts_with_morpher();
    let stopped = Morpher::new(&g, 1);
    let mut analyzer = pg_foma::composite::FomaAnalyzer::from_cached_with_morpher(
        &g, proposer, peeler, owners, stopped, filter,
    );
    let row = measure_word(
        &g,
        &Morpher::new(&g, usize::MAX),
        Some(&mut analyzer),
        0,
        "ap",
        &ApplyBudget::unbounded(),
        None,
    )
    .unwrap();
    assert_eq!(row.hc_status, "complete");
    assert_eq!(row.fst_status, "confirmation_incomplete");
    assert!(row.incomplete);
    assert_eq!(row.confirmed_multiset_equal, None);
}

#[test]
fn the_normalization_allowance_is_the_existing_candidate_allowance_not_a_new_limit() {
    use pg_foma::compose_budget::{
        DEFAULT_EVALUATION_APPLY_CANDIDATE_BUDGET, DEFAULT_EVALUATION_APPLY_PATH_BUDGET,
    };
    assert_eq!(DEFAULT_EVALUATION_APPLY_PATH_BUDGET, 1_000_000);
    assert_eq!(DEFAULT_EVALUATION_APPLY_CANDIDATE_BUDGET, 1_000_000);
    let budget = ApplyBudget::with_caps(Some(7), Some(11));
    assert_eq!(budget.normalization_cap(), Some(11));
    assert_eq!(budget.remaining(2, 3, 5).normalization_cap(), Some(6));
    assert_eq!(ApplyBudget::unbounded().normalization_cap(), None);
}

/// The separated fixture's grammar plus one reduplication rule, so the peel proposes residuals of a doubled word through the same budgeted proposer.
fn grammar_with_reduplication() -> Grammar {
    use pg_grammar::model::{
        AffixAllomorphDef, AffixProcessRuleDef, AllomorphId, MRuleId, MorphRuleDef, MprSet,
        OutputAction, PartRef, Pattern, ReduplicationHint, VarTable,
    };
    let mut g = fixture(SEPARATED).load_grammar();
    let id = MRuleId(g.mrules.len() as u32);
    g.mrules
        .push(MorphRuleDef::AffixProcess(AffixProcessRuleDef {
            morpheme: MorphemeId(g.morphemes.len() as u32),
            name: Some("doubling".to_string()),
            blockable: false,
            partial_reason: None,
            max_apps: 1,
            required_syn_fs: pg_featstruct::FsId(0),
            out_syn_fs: pg_featstruct::FsId(0),
            obligatory_features: vec![],
            required_stem_name: None,
            is_template_rule: false,
            is_clitic_rule: None,
            allomorphs: vec![AffixAllomorphDef {
                xml_key: String::new(),
                id: AllomorphId(0),
                environments: vec![],
                co_occurrence: vec![],
                required_syn_fs: pg_featstruct::FsId(0),
                vars: VarTable::default(),
                required_mpr: MprSet::EMPTY,
                excluded_mpr: MprSet::EMPTY,
                out_mpr: MprSet::EMPTY,
                redup_hint: ReduplicationHint::Suffix,
                lhs: vec![Pattern::default()],
                rhs: vec![
                    OutputAction::Copy(PartRef::Input(0)),
                    OutputAction::Copy(PartRef::Input(0)),
                ],
                properties: vec![],
            }],
        }));
    // The doubling sits in a stratum after every other affix, since the proposer refuses a reduplication before a later affix.
    let first = &g.strata[0];
    let last = pg_grammar::model::StratumDef {
        name: None,
        table: first.table,
        mrule_order: first.mrule_order,
        prules: first.prules.clone(),
        mrules: vec![id],
        templates: vec![],
        entries: vec![],
    };
    let mut owner = g.morphemes[0].clone();
    owner.stratum = pg_grammar::model::StratumId(g.strata.len() as u8);
    g.morphemes.push(owner);
    g.strata.push(last);
    g
}

/// One full analysis of `word` against `cap` on the normalization allowance only.
fn measured_at(
    g: &Grammar,
    analyzer: &mut pg_foma::composite::FomaAnalyzer,
    word: &str,
    cap: usize,
) -> DifferentialRow {
    let morpher = Morpher::new(g, usize::MAX);
    measure_word(
        g,
        &morpher,
        Some(analyzer),
        0,
        word,
        &ApplyBudget::with_caps(None, Some(cap)),
        None,
    )
    .unwrap()
}

/// A word whose direct proposal and peeled residuals together recover more alternatives than the direct proposal alone, with the candidate count never the first to run out.
const PEELED: &str = "pppa";

#[test]
fn the_normalization_allowance_is_spent_across_direct_and_peel_proposals() {
    use pg_foma::compose_budget::ComposeBudget;
    use pg_foma::composite::propose_union_peel_with_diagnostics;

    let g = grammar_with_reduplication();
    let mut proposer = pg_foma::analyzer::compile_proposer(&g).unwrap();
    let peeler = pg_foma::peel::ReduplicationPeeler::new(&g);
    assert!(peeler.has_redup_rules());
    let compose = ComposeBudget::unbounded();

    let (_, _, _, totals, calls) = propose_union_peel_with_diagnostics(
        &g,
        &mut proposer,
        &peeler,
        &compose,
        PEELED,
        &ApplyBudget::unbounded(),
    )
    .expect("unbounded completes");
    let (_, direct) = proposer.propose_with_diagnostics_budgeted(PEELED, &ApplyBudget::unbounded());
    assert!(calls > 1, "the peel must have proposed residuals");
    assert!(
        totals.normalized_alternatives > direct.normalized_alternatives,
        "the peel adds alternatives on top of the direct proposal"
    );

    // Each call alone fits any cap from the direct count upward, so only a remaining() that carries the normalized count across calls can trip here.
    for cap in direct.normalized_alternatives..totals.normalized_alternatives {
        let tripped = propose_union_peel_with_diagnostics(
            &g,
            &mut proposer,
            &peeler,
            &compose,
            PEELED,
            &ApplyBudget::with_caps(None, Some(cap)),
        );
        let (dimension, value, limit, _, trip_calls) =
            tripped.expect_err(&format!("cap {cap} must trip"));
        assert_eq!(
            (dimension, value, limit),
            (ApplyDimension::NormalizedCandidates, cap + 1, cap),
            "cap {cap}"
        );
        assert!(
            trip_calls > 1,
            "cap {cap}: the trip belongs to a peeled residual"
        );
    }
    let exact = ApplyBudget::with_caps(None, Some(totals.normalized_alternatives));
    assert!(propose_union_peel_with_diagnostics(
        &g,
        &mut proposer,
        &peeler,
        &compose,
        PEELED,
        &exact
    )
    .is_ok());

    // The whole analyzer reports the trip as an incomplete row, never a verdict.
    let original = fixture(SEPARATED).load_grammar();
    let base = pg_foma::composite::compile_analyzer(&original).unwrap();
    let (proposer, _, owners, _, filter) = base.into_parts_with_morpher();
    let mut analyzer = pg_foma::composite::FomaAnalyzer::from_cached(
        &g,
        proposer,
        pg_foma::peel::ReduplicationPeeler::new(&g),
        owners,
        filter,
    );
    let one_less = totals.normalized_alternatives - 1;
    let row = measured_at(&g, &mut analyzer, PEELED, one_less);
    assert_eq!(
        row.fst_status,
        format!(
            "apply_limit:repeat-normalization alternatives:{}/{}",
            one_less + 1,
            one_less
        )
    );
    assert!(row.incomplete);
    assert_eq!(row.confirmed_multiset_equal, None);
    assert_eq!(row.proposal_misses, None);
    assert!(row.confirmed.is_empty());
}

#[test]
fn duplicate_raw_paths_are_counted_before_deduplication_at_the_exact_and_one_less_allowance() {
    let g = fixture(SEPARATED).load_grammar();
    let mut proposer = pg_foma::analyzer::compile_proposer(&g).unwrap();
    let (_, counts) = proposer.propose_budgeted_counted(REVISITED, &ApplyBudget::unbounded());
    assert!(counts.raw_paths > 1);
    assert!(
        counts.normalized_alternatives > counts.unique_candidates,
        "several raw paths reach the same candidates, so the pre-deduplication count exceeds the distinct count"
    );
    let exact = counts.normalized_alternatives;
    let one_less = exact - 1;
    assert!(
        counts.unique_candidates <= one_less,
        "one less must still hold every distinct candidate, so only the duplicates can trip it"
    );

    let mut analyzer = pg_foma::composite::compile_analyzer(&g).unwrap();
    let at_exact = measured_at(&g, &mut analyzer, REVISITED, exact);
    assert_eq!(at_exact.fst_status, "complete");
    assert_eq!(at_exact.confirmed_multiset_equal, Some(true));
    assert_eq!(at_exact.proposal_misses, Some(Vec::new()));

    let at_one_less = measured_at(&g, &mut analyzer, REVISITED, one_less);
    assert_eq!(
        at_one_less.fst_status,
        format!("apply_limit:repeat-normalization alternatives:{exact}/{one_less}")
    );
    assert!(at_one_less.incomplete);
    assert_eq!(at_one_less.confirmed_multiset_equal, None);
    assert_eq!(at_one_less.proposal_misses, None);
    assert!(at_one_less.confirmed.is_empty());
}
