use super::*;
use crate::cache::StatsCache;
use crate::model::{
    Direction, FactRecord, IdentityQuality, ObjectKind, RunMetadata, StructuralLocator, WordRecord,
};

fn run() -> RunMetadata {
    RunMetadata {
        build_info: "test-build".to_string(),
        fwdata_path: "C:/x/project.fwdata".to_string(),
        grammar_hash: "hash-a".to_string(),
        engine: "hc".to_string(),
        options_hash: "opts-a".to_string(),
        options_json: "{}".to_string(),
        created_utc: "2026-08-22T00:00:00Z".to_string(),
        step_cap: None,
    }
}

fn fact(object_key: &str, kind: ObjectKind, attempts: u64, work: u64, no_root: u64) -> FactRecord {
    fact_with_direction(
        object_key,
        kind,
        Direction::Analysis,
        attempts,
        work,
        no_root,
    )
}

fn fact_with_direction(
    object_key: &str,
    kind: ObjectKind,
    direction: Direction,
    attempts: u64,
    work: u64,
    no_root: u64,
) -> FactRecord {
    FactRecord {
        object_key: object_key.to_string(),
        object_kind: kind,
        object_label: object_key.to_string(),
        identity_quality: IdentityQuality::Authored,
        stratum: Some(StructuralLocator::new("0:Root", "Root")),
        allomorph: None,
        morpheme: None,
        direction,
        attempts,
        work,
        outputs: attempts,
        not_applied: 0,
        no_root,
        surface_mismatch: 0,
        uses: 0,
        self_time_ns: 0,
    }
}

fn fact_with_self_time(
    object_key: &str,
    kind: ObjectKind,
    attempts: u64,
    work: u64,
    no_root: u64,
    self_time_ns: u64,
) -> FactRecord {
    FactRecord {
        self_time_ns,
        ..fact(object_key, kind, attempts, work, no_root)
    }
}

/// `attempts` is a word-level placeholder here -- no test in this module reads it.
fn word_record(form: &str, elapsed_ns: u64, facts: Vec<FactRecord>) -> WordRecord {
    WordRecord {
        form: form.to_string(),
        elapsed_ns,
        attempts: 1,
        passes: 1,
        capped: false,
        timed_out: false,
        invalid_shape: false,
        facts,
    }
}

fn seeded_cache() -> StatsCache {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();

    let words = vec![
        word_record(
            "apu",
            500,
            vec![
                fact("rule-a", ObjectKind::MorphRule, 5, 20, 1),
                fact("root-a", ObjectKind::LexEntry, 2, 4, 5),
            ],
        ),
        WordRecord {
            capped: true,
            ..word_record(
                "beta",
                1_500,
                vec![fact("rule-a", ObjectKind::MorphRule, 9, 40, 0)],
            )
        },
    ];
    outcome.cache.flush(&run(), &words).unwrap();
    outcome.cache
}

#[test]
fn per_word_orders_by_elapsed_descending() {
    let cache = seeded_cache();
    let rows = per_word_report(cache.connection()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].form, "beta");
    assert_eq!(rows[1].form, "apu");
    assert!(rows[0].capped);
    assert!(!rows[1].capped);
}

#[test]
fn per_object_kind_filter_narrows_rows() {
    let cache = seeded_cache();
    let filter = PerObjectFilter {
        kind: Some("lex_entry".to_string()),
        ..Default::default()
    };
    let rows = per_object_report(cache.connection(), &filter).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "lex_entry");
}

#[test]
fn per_object_word_filter_narrows_to_one_words_objects() {
    let cache = seeded_cache();
    // "beta" only ever attempted rule-a; root-a belongs entirely to "apu".
    let filter = PerObjectFilter {
        word: Some("beta".to_string()),
        ..Default::default()
    };
    let rows = per_object_report(cache.connection(), &filter).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "rule-a");
    assert_eq!(
        rows[0].attempts, 9,
        "only beta's own contribution, not apu's"
    );
}

#[test]
fn per_object_exclude_censored_words_drops_their_contribution() {
    let cache = seeded_cache();
    let unfiltered = per_object_report(cache.connection(), &PerObjectFilter::default()).unwrap();
    let rule_row = unfiltered.iter().find(|r| r.label == "rule-a").unwrap();
    assert_eq!(rule_row.attempts, 14, "sanity: both words' facts summed");

    let filter = PerObjectFilter {
        exclude_censored_words: true,
        ..Default::default()
    };
    let filtered = per_object_report(cache.connection(), &filter).unwrap();
    let rule_row = filtered.iter().find(|r| r.label == "rule-a").unwrap();
    assert_eq!(rule_row.attempts, 5, "beta is capped and must be excluded");
}

#[test]
fn per_object_sort_key_changes_order() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let words = vec![word_record(
        "apu",
        500,
        vec![
            // rule-a has more measured self time (6000ns vs 400ns) but root-a has more no_root (5 vs 1) -- the sort keys disagree.
            fact_with_self_time("rule-a", ObjectKind::MorphRule, 5, 20, 1, 6_000),
            fact_with_self_time("root-a", ObjectKind::LexEntry, 2, 4, 5, 400),
        ],
    )];
    outcome.cache.flush(&run(), &words).unwrap();
    let cache = outcome.cache;

    let by_self_time = per_object_report(cache.connection(), &PerObjectFilter::default()).unwrap();
    assert_eq!(by_self_time[0].label, "rule-a");
    assert_eq!(by_self_time[1].label, "root-a");

    let filter = PerObjectFilter {
        sort: SortKey::NoRoot,
        ..Default::default()
    };
    let by_no_root = per_object_report(cache.connection(), &filter).unwrap();
    assert_eq!(by_no_root[0].label, "root-a");
    assert_eq!(by_no_root[0].no_root, 5);
    assert_eq!(by_no_root[1].label, "rule-a");
}

#[test]
fn per_object_sort_by_amp_uses_and_attempts() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let mut low_amp = fact("rule-low-amp", ObjectKind::MorphRule, 10, 10, 0);
    low_amp.outputs = 1; // amp 0.1
    low_amp.uses = 1;
    let mut high_amp = fact("rule-high-amp", ObjectKind::MorphRule, 10, 10, 0);
    high_amp.outputs = 9; // amp 0.9
    high_amp.uses = 9;
    let words = vec![word_record("w", 1, vec![low_amp, high_amp])];
    outcome.cache.flush(&run(), &words).unwrap();
    let cache = outcome.cache;

    let by_amp = per_object_report(
        cache.connection(),
        &PerObjectFilter {
            sort: SortKey::Amp,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(by_amp[0].label, "rule-high-amp");

    let by_uses = per_object_report(
        cache.connection(),
        &PerObjectFilter {
            sort: SortKey::Uses,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(by_uses[0].label, "rule-high-amp");

    let by_attempts = per_object_report(
        cache.connection(),
        &PerObjectFilter {
            sort: SortKey::Attempts,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        by_attempts.len(),
        2,
        "both rules tie on attempts; both must still appear"
    );
}

/// A per-kind total must be the exact sum of its objects' measured self times, never apportioned.
#[test]
fn per_kind_total_equals_sum_of_objects_measured_self_times() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let words = vec![word_record(
        "apu",
        500,
        vec![
            fact_with_self_time("rule-a", ObjectKind::MorphRule, 5, 20, 0, 1_000),
            fact_with_self_time("rule-b", ObjectKind::MorphRule, 3, 12, 0, 2_500),
            fact_with_self_time("root-a", ObjectKind::LexEntry, 2, 4, 0, 700),
        ],
    )];
    outcome.cache.flush(&run(), &words).unwrap();

    let morph_rows = per_object_report(
        outcome.cache.connection(),
        &PerObjectFilter {
            kind: Some("morph_rule".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let per_kind_total: i64 = morph_rows.iter().map(|r| r.self_time_ns).sum();
    assert_eq!(
        per_kind_total,
        1_000 + 2_500,
        "the per-kind total must be the exact sum of its objects' measured self times"
    );
}

fn fact_with_allomorph(
    object_key: &str,
    kind: ObjectKind,
    allomorph: Option<(&str, &str)>,
    attempts: u64,
    work: u64,
) -> FactRecord {
    FactRecord {
        object_key: object_key.to_string(),
        object_kind: kind,
        object_label: object_key.to_string(),
        identity_quality: IdentityQuality::Authored,
        stratum: Some(StructuralLocator::new("0:Root", "Root")),
        allomorph: allomorph.map(|(k, l)| StructuralLocator::new(k, l)),
        morpheme: None,
        direction: Direction::Analysis,
        attempts,
        work,
        outputs: attempts,
        not_applied: 0,
        no_root: 0,
        surface_mismatch: 0,
        uses: 0,
        self_time_ns: 0,
    }
}

fn seeded_allomorph_cache() -> StatsCache {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let words = vec![word_record(
        "gamma",
        700,
        vec![
            fact_with_allomorph(
                "rule-b",
                ObjectKind::MorphRule,
                Some(("rule-b:0", "Allo 0")),
                4,
                8,
            ),
            fact_with_allomorph(
                "rule-b",
                ObjectKind::MorphRule,
                Some(("rule-b:1", "Allo 1")),
                3,
                6,
            ),
            fact_with_allomorph("rule-b", ObjectKind::MorphRule, None, 2, 5),
            fact_with_allomorph(
                "rule-c",
                ObjectKind::MorphRule,
                Some(("rule-c:0", "C Allo 0")),
                1,
                1,
            ),
        ],
    )];
    outcome.cache.flush(&run(), &words).unwrap();
    outcome.cache
}

#[test]
fn per_allomorph_rows_sum_to_object_total_including_none_sentinel() {
    let cache = seeded_allomorph_cache();
    let object_rows = per_object_report(
        cache.connection(),
        &PerObjectFilter {
            object_key: Some("rule-b".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(object_rows.len(), 1);
    let object_row = &object_rows[0];

    let allomorph_rows = per_allomorph_report(
        cache.connection(),
        &PerAllomorphFilter {
            object_key: Some("rule-b".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        allomorph_rows.len(),
        3,
        "two named allomorphs plus the NONE sentinel"
    );

    let summed_attempts: i64 = allomorph_rows.iter().map(|r| r.attempts).sum();
    let summed_work: i64 = allomorph_rows.iter().map(|r| r.work).sum();
    assert_eq!(summed_attempts, object_row.attempts);
    assert_eq!(summed_work, object_row.work);

    let none_row = allomorph_rows
        .iter()
        .find(|r| r.allomorph_key.is_none())
        .expect("NONE sentinel row must be present, not filtered out");
    assert_eq!(none_row.allomorph_label, "NONE");
    assert_eq!(none_row.attempts, 2);
    assert_eq!(none_row.work, 5);
}

#[test]
fn per_allomorph_object_filter_narrows_rows() {
    let cache = seeded_allomorph_cache();
    let narrowed = per_allomorph_report(
        cache.connection(),
        &PerAllomorphFilter {
            object_key: Some("rule-b".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(narrowed.len(), 3);
    assert!(narrowed.iter().all(|r| r.object_label == "rule-b"));

    let unfiltered =
        per_allomorph_report(cache.connection(), &PerAllomorphFilter::default()).unwrap();
    assert_eq!(unfiltered.len(), 4, "rule-c's allomorph must also appear");
}

#[test]
fn per_allomorph_orders_deterministically_on_ties() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let words = vec![word_record(
        "tie",
        1,
        vec![
            fact_with_allomorph("rule-y", ObjectKind::MorphRule, Some(("a0", "A0")), 1, 5),
            fact_with_allomorph("rule-x", ObjectKind::MorphRule, Some(("a0", "A0")), 1, 5),
        ],
    )];
    outcome.cache.flush(&run(), &words).unwrap();

    let rows =
        per_allomorph_report(outcome.cache.connection(), &PerAllomorphFilter::default()).unwrap();
    assert_eq!(rows.len(), 2);
    let labels: Vec<_> = rows.iter().map(|r| r.object_label.clone()).collect();
    assert_eq!(
        labels,
        vec!["rule-x".to_string(), "rule-y".to_string()],
        "equal measured self time must still order deterministically via the key tie-break"
    );
}

fn fact_with_allomorph_not_applied(
    object_key: &str,
    allomorph: Option<(&str, &str)>,
    attempts: u64,
    not_applied: u64,
) -> FactRecord {
    FactRecord {
        object_key: object_key.to_string(),
        object_kind: ObjectKind::MorphRule,
        object_label: object_key.to_string(),
        identity_quality: IdentityQuality::Authored,
        stratum: Some(StructuralLocator::new("0:Root", "Root")),
        allomorph: allomorph.map(|(k, l)| StructuralLocator::new(k, l)),
        morpheme: None,
        direction: Direction::Analysis,
        attempts,
        work: 1,
        outputs: 0,
        not_applied,
        no_root: 0,
        surface_mismatch: 0,
        uses: 0,
        self_time_ns: 0,
    }
}

/// Two failing allomorphs must not inflate per-object `not_applied` past the one invocation.
#[test]
fn per_object_not_applied_counts_rule_level_invocations_not_per_allomorph_failures() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let words = vec![word_record(
        "granularity",
        1,
        vec![
            // Rule-level residual: one invocation, reached both allomorphs, produced nothing.
            fact_with_allomorph_not_applied("rule-f", None, 1, 1),
            fact_with_allomorph_not_applied("rule-f", Some(("rule-f:0", "Allo 0")), 0, 1),
            fact_with_allomorph_not_applied("rule-f", Some(("rule-f:1", "Allo 1")), 0, 1),
        ],
    )];
    outcome.cache.flush(&run(), &words).unwrap();

    let object_rows =
        per_object_report(outcome.cache.connection(), &PerObjectFilter::default()).unwrap();
    assert_eq!(object_rows.len(), 1);
    assert_eq!(
        object_rows[0].not_applied, 1,
        "one failed invocation must read as 1, not 3 -- summing every allomorph's own failure \
             mixes a per-allomorph count into a per-invocation column"
    );

    let allomorph_rows =
        per_allomorph_report(outcome.cache.connection(), &PerAllomorphFilter::default()).unwrap();
    assert_eq!(
        allomorph_rows.len(),
        3,
        "the per-allomorph report keeps all three rows, including the NONE sentinel"
    );
    assert!(
        allomorph_rows.iter().all(|r| r.not_applied == 1),
        "the per-allomorph report is unaffected -- each row still reports its own failure"
    );
}

/// One rule fact recorded under both directions, sharing `(word, object, stratum, allomorph)`.
fn seeded_direction_cache() -> StatsCache {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let words = vec![word_record(
        "dirword",
        1_000,
        vec![
            fact_with_direction(
                "rule-dir",
                ObjectKind::MorphRule,
                Direction::Analysis,
                5,
                20,
                2,
            ),
            fact_with_direction(
                "rule-dir",
                ObjectKind::MorphRule,
                Direction::Synthesis,
                3,
                12,
                0,
            ),
        ],
    )];
    outcome.cache.flush(&run(), &words).unwrap();
    outcome.cache
}

#[test]
fn per_object_direction_filter_narrows_to_one_direction() {
    let cache = seeded_direction_cache();

    let analysis_only = per_object_report(
        cache.connection(),
        &PerObjectFilter {
            direction: Some("analysis".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(analysis_only.len(), 1);
    assert_eq!(analysis_only[0].attempts, 5);
    assert_eq!(analysis_only[0].no_root, 2);

    let synthesis_only = per_object_report(
        cache.connection(),
        &PerObjectFilter {
            direction: Some("synthesis".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(synthesis_only.len(), 1);
    assert_eq!(synthesis_only[0].attempts, 3);
    assert_eq!(synthesis_only[0].no_root, 0);

    let unfiltered = per_object_report(cache.connection(), &PerObjectFilter::default()).unwrap();
    assert_eq!(
        unfiltered[0].attempts, 8,
        "no direction filter must sum both directions' attempts, recovering the undirected total"
    );
}

#[test]
fn per_kind_report_sums_across_every_object_of_a_kind() {
    let cache = seeded_cache();
    let rows = per_kind_report(cache.connection(), &PerKindFilter::default()).unwrap();
    let morph_row = rows.iter().find(|r| r.kind == "morph_rule").unwrap();
    assert_eq!(morph_row.attempts, 14, "both words' rule-a attempts summed");
    let lex_row = rows.iter().find(|r| r.kind == "lex_entry").unwrap();
    assert_eq!(lex_row.attempts, 2);
}

#[test]
fn per_kind_report_word_filter_narrows_the_sum() {
    let cache = seeded_cache();
    let rows = per_kind_report(
        cache.connection(),
        &PerKindFilter {
            word: Some("beta".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(rows.len(), 1, "beta only ever touched morph_rule");
    assert_eq!(rows[0].kind, "morph_rule");
    assert_eq!(rows[0].attempts, 9);
}

fn fact_with_morpheme(
    object_key: &str,
    morpheme: Option<(&str, &str)>,
    attempts: u64,
    work: u64,
) -> FactRecord {
    FactRecord {
        object_key: object_key.to_string(),
        object_kind: ObjectKind::LexEntry,
        object_label: object_key.to_string(),
        identity_quality: IdentityQuality::Authored,
        stratum: Some(StructuralLocator::new("0:Root", "Root")),
        allomorph: None,
        morpheme: morpheme.map(|(k, l)| StructuralLocator::new(k, l)),
        direction: Direction::Analysis,
        attempts,
        work,
        outputs: attempts,
        not_applied: 0,
        no_root: 0,
        surface_mismatch: 0,
        uses: 0,
        self_time_ns: 0,
    }
}

#[test]
fn per_morpheme_report_collapses_scattered_entries_to_one_row() {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let words = vec![word_record(
        "w",
        1,
        vec![
            // Two distinct lex_entry objects (different authored ids/allomorphs) sharing one morpheme.
            fact_with_morpheme("entry-1", Some(("cat-morph", "cat")), 3, 3),
            fact_with_morpheme("entry-2", Some(("cat-morph", "cat")), 2, 2),
            fact_with_morpheme("entry-3", Some(("dog-morph", "dog")), 5, 5),
        ],
    )];
    outcome.cache.flush(&run(), &words).unwrap();

    let rows =
        per_morpheme_report(outcome.cache.connection(), &PerMorphemeFilter::default()).unwrap();
    assert_eq!(
        rows.len(),
        2,
        "two morphemes, however many entries realize each"
    );
    let cat_row = rows
        .iter()
        .find(|r| r.morpheme_key.as_deref() == Some("cat-morph"))
        .expect("cat-morph row must exist");
    assert_eq!(
        cat_row.attempts, 5,
        "entry-1 and entry-2 both realize cat-morph and must collapse into one row"
    );
}

fn never_fires_fact(
    object_key: &str,
    direction: Direction,
    attempts: u64,
    outputs: u64,
) -> FactRecord {
    FactRecord {
        object_key: object_key.to_string(),
        object_kind: ObjectKind::MorphRule,
        object_label: object_key.to_string(),
        identity_quality: IdentityQuality::Authored,
        stratum: Some(StructuralLocator::new("0:Root", "Root")),
        allomorph: None,
        morpheme: None,
        direction,
        attempts,
        work: attempts,
        outputs,
        not_applied: attempts.saturating_sub(outputs),
        no_root: 0,
        surface_mismatch: 0,
        uses: 0,
        self_time_ns: 0,
    }
}

fn seeded_never_fires_cache() -> StatsCache {
    let mut outcome = StatsCache::open_in_memory("hash-a").unwrap();
    let words = vec![word_record(
        "neverfiresword",
        1,
        vec![
            // Heavily attempted, zero outputs -- the motivating case.
            never_fires_fact("rule-dead", Direction::Analysis, 2_500, 0),
            // Heavily attempted but productive -- must never appear.
            never_fires_fact("rule-alive", Direction::Analysis, 2_500, 40),
            // Zero outputs but below the sensible default floor -- noise, not signal.
            never_fires_fact("rule-thin", Direction::Analysis, 5, 0),
            // Dead in analysis, alive in synthesis -- direction-aware, only one row must appear.
            never_fires_fact("rule-dir-split", Direction::Analysis, 3_000, 0),
            never_fires_fact("rule-dir-split", Direction::Synthesis, 500, 10),
        ],
    )];
    outcome.cache.flush(&run(), &words).unwrap();
    outcome.cache
}

#[test]
fn never_fires_report_finds_heavily_attempted_zero_output_objects() {
    let cache = seeded_never_fires_cache();
    let rows = never_fires_report(cache.connection(), &NeverFiresFilter::default()).unwrap();
    let labels: Vec<_> = rows.iter().map(|r| r.label.clone()).collect();

    assert!(
        labels.contains(&"rule-dead".to_string()),
        "a heavily-attempted, zero-output rule must be reported"
    );
    assert!(
        !labels.contains(&"rule-alive".to_string()),
        "a rule that produced outputs must never appear, no matter how many attempts"
    );
    assert!(
        !labels.contains(&"rule-thin".to_string()),
        "below the sensible default min_attempts, a zero-output rule is noise, not signal"
    );
}

#[test]
fn never_fires_report_is_direction_aware() {
    let cache = seeded_never_fires_cache();
    let rows = never_fires_report(cache.connection(), &NeverFiresFilter::default()).unwrap();
    let split_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.label == "rule-dir-split")
        .collect();

    assert_eq!(
        split_rows.len(),
        1,
        "the rule fires in synthesis, so only its analysis row may be reported"
    );
    assert_eq!(split_rows[0].direction, "analysis");
}

#[test]
fn never_fires_report_orders_by_attempts_descending_and_respects_explicit_min_attempts() {
    let cache = seeded_never_fires_cache();
    let rows = never_fires_report(
        cache.connection(),
        &NeverFiresFilter {
            min_attempts: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let labels: Vec<_> = rows.iter().map(|r| r.label.clone()).collect();
    assert_eq!(
        labels,
        vec![
            "rule-dir-split".to_string(),
            "rule-dead".to_string(),
            "rule-thin".to_string(),
        ],
        "descending by attempts (3000, 2500, 5), and a low explicit min_attempts admits rule-thin"
    );
}

#[test]
fn word_elapsed_ns_total_sums_all_words_or_narrows_to_one() {
    let cache = seeded_cache();
    let total = word_elapsed_ns_total(cache.connection(), None).unwrap();
    assert_eq!(total, 500 + 1_500);

    let one = word_elapsed_ns_total(cache.connection(), Some("beta")).unwrap();
    assert_eq!(one, 1_500);
}

#[test]
fn kind_has_any_recorded_object_distinguishes_absent_from_present() {
    let cache = seeded_cache();
    assert!(kind_has_any_recorded_object(cache.connection(), "morph_rule").unwrap());
    assert!(
        !kind_has_any_recorded_object(cache.connection(), "phon_rule").unwrap(),
        "no phon_rule fact was ever recorded in this cache"
    );
}
