use super::*;
#[test]
fn rendering_is_deterministic_and_sorted() {
    let mut r = sample();
    r.candidates = vec![candidate("z"), candidate("a")];
    r.frontier = vec!["z".into(), "a".into()];
    assert_eq!(r.canonical_json(), r.canonical_json());
    assert!(
        r.canonical_json().find("\"id\":\"a\"").unwrap()
            < r.canonical_json().find("\"id\":\"z\"").unwrap()
    );
    assert_eq!(r.markdown(), r.markdown());
}

#[test]
fn approximate_markdown_quantifies_unexplored_space() {
    let mut r = sample();
    r.quality = SearchQuality::Approximate;
    r.search.unexplored = 7;
    let markdown = r.markdown();
    assert!(markdown.contains("approximate"));
    assert!(markdown.contains("unexplored space is quantified as 7"));
    assert!(markdown.contains("not proven optimal"));
}

#[test]
fn waterfall_reconciles_without_counting_confirmation_twice() {
    let waterfall = PruningWaterfall {
        generated: 10,
        inapplicable: 1,
        duplicates: 1,
        declared_not_searched: 2,
        materialization_rejects: 1,
        capability_rejected: 1,
        evaluated: 3,
        confirmed: 2,
        budget_pruned: 1,
    };
    assert!(waterfall.reconciles());
}

#[test]
fn winner_requires_confirmation_and_score_for_replay() {
    let mut r = sample();
    r.winner = Some("a".into());
    r.candidates = vec![candidate("a")];
    assert_eq!(
        r.validate(),
        Err("winner is not fully confirmed and scored")
    );
    r.candidates[0].certification = Certification::FullHcConfirmed {
        words: 1,
        corpus_hash: "c".into(),
    };
    r.candidates[0].score = Some(Score {
        states: 1,
        arcs: 1,
        build: 1,
        apply: 1,
        proposals: 1,
        confirmation: 1,
        confirmation_steps: 1,
        raw_paths: 0,
    });
    r.frontier = vec!["a".into()];
    assert!(r.validate().is_ok());
    assert!(r.replay_parameters.contains_key("seed") || r.seed == 0);
}

#[test]
fn selectable_candidate_cannot_be_omitted_from_report_integrity() {
    let mut r = sample();
    r.candidates = vec![candidate("a"), confirmed_candidate("b", 1)];
    r.candidates[0].certification = Certification::FullHcConfirmed {
        words: 1,
        corpus_hash: "c".into(),
    };
    r.frontier = vec!["b".into()];
    r.winner = Some("b".into());

    assert_eq!(r.validate(), Err("selectable candidate is missing a score"));
}

/// A `selectable()` (confirmed, scored) candidate blocked from publication cannot be named `winner`.
#[test]
fn a_production_blocked_candidate_cannot_be_named_winner() {
    let mut r = sample();
    let mut blocked = confirmed_candidate("a", 1);
    blocked.production_blocks_publication = true;
    r.candidates = vec![blocked];
    r.frontier = vec![];
    r.winner = Some("a".into());

    assert_eq!(
        r.validate(),
        Err("winner's production health blocks publication")
    );
}

/// A cheaper-but-blocked candidate must not steal the frontier/winner from a pricier clean one.
#[test]
fn a_production_blocked_candidate_is_excluded_from_frontier_and_winner_selection() {
    let mut r = sample();
    let mut blocked_but_cheaper = confirmed_candidate("a", 1);
    blocked_but_cheaper.production_blocks_publication = true;
    let clean_but_pricier = confirmed_candidate("b", 2);
    r.candidates = vec![blocked_but_cheaper, clean_but_pricier];
    r.frontier = vec!["b".into()];
    r.winner = Some("b".into());

    assert_eq!(r.validate(), Ok(()));
}

/// The gate blocks only the specific candidate whose health names it, never every candidate.
#[test]
fn a_clean_production_health_candidate_remains_selectable() {
    let mut r = sample();
    r.candidates = vec![confirmed_candidate("a", 1)];
    assert!(!r.candidates[0].production_blocks_publication);
    r.frontier = vec!["a".into()];
    r.winner = Some("a".into());

    assert_eq!(r.validate(), Ok(()));
}

/// The new field round-trips through JSON, and a pre-existing report without it defaults to `false`.
#[test]
fn production_blocks_publication_round_trips_and_legacy_reports_default_to_false() {
    let mut r = sample();
    let mut blocked = confirmed_candidate("a", 1);
    blocked.production_blocks_publication = true;
    r.candidates = vec![blocked];
    r.frontier = vec![];
    r.winner = None;
    assert_eq!(r.validate(), Ok(()));

    let json = r.canonical_json();
    let restored: BackendOptimizationReport =
        serde_json::from_str(&json).expect("round trip must parse");
    assert!(restored.candidates[0].production_blocks_publication);
    assert_eq!(restored.validate(), Ok(()));

    // Simulate a report written before this field existed: strip it out of the JSON entirely.
    let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
    legacy["candidates"][0]
        .as_object_mut()
        .unwrap()
        .remove("production_blocks_publication");
    let legacy_report: BackendOptimizationReport = serde_json::from_value(legacy).unwrap();
    assert!(
        !legacy_report.candidates[0].production_blocks_publication,
        "a field-less legacy candidate must default to not-known-blocked"
    );
}

#[test]
fn validation_recomputes_the_serialized_frontier() {
    let mut r = sample();
    r.candidates = vec![confirmed_candidate("a", 1), confirmed_candidate("b", 2)];
    r.frontier = vec!["b".into()];
    r.winner = Some("a".into());

    assert_eq!(
        r.validate(),
        Err("serialized frontier does not match recomputed frontier")
    );
}

#[test]
fn validation_recomputes_the_serialized_winner() {
    let mut r = sample();
    r.candidates = vec![confirmed_candidate("a", 1), confirmed_candidate("b", 2)];
    r.frontier = vec!["a".into()];
    r.winner = Some("b".into());

    assert_eq!(
        r.validate(),
        Err("serialized winner does not match recomputed winner")
    );
}

#[test]
fn validation_rejects_unknown_report_and_score_schema_versions() {
    let mut report = sample();
    report.schema_version = BACKEND_REPORT_SCHEMA_VERSION + 1;
    assert_eq!(
        report.validate(),
        Err("unsupported backend report schema version")
    );

    let mut report = sample();
    report.candidates = vec![confirmed_candidate("a", 1)];
    report.frontier = vec!["a".into()];
    report.winner = Some("a".into());
    report.score_schema_version = DETERMINISTIC_SCORE_SCHEMA_VERSION + 1;
    assert_eq!(
        report.validate(),
        Err("unsupported deterministic score schema version")
    );
}

#[test]
fn validation_rejects_duplicate_candidate_and_frontier_ids() {
    let mut report = sample();
    report.candidates = vec![confirmed_candidate("a", 1), confirmed_candidate("a", 1)];
    report.frontier = vec!["a".into()];
    report.winner = Some("a".into());
    assert_eq!(report.validate(), Err("candidate ids are not unique"));

    let mut report = sample();
    report.candidates = vec![confirmed_candidate("a", 1)];
    report.frontier = vec!["a".into(), "a".into()];
    report.winner = Some("a".into());
    assert_eq!(report.validate(), Err("frontier ids are not unique"));
}

/// A reconciling, zero-exclusion one-word ledger, as a struct literal since `reconciles()` never checks the hashes.
fn ledger() -> CorpusCompletenessEvidence {
    CorpusCompletenessEvidence {
        requested: 1,
        included: 1,
        excluded: 0,
        requested_hash: "requested".into(),
        included_hash: "included".into(),
        excluded_hash: "excluded".into(),
        oracle_step_cap: 20_000,
        oracle_memory_ceiling_bytes: 1 << 33,
        oracle_liveness_net_ns: 300_000_000_000,
        exclusions: vec![],
    }
}

/// A report that names a confirmed candidate but cannot say which requested corpus that confirmation covers is refused, at the artifact boundary.
#[test]
fn a_certifying_report_must_carry_its_corpus_eligibility_ledger() {
    let mut report = sample();
    report.candidates = vec![confirmed_candidate("a", 1)];
    report.frontier = vec!["a".into()];
    report.winner = Some("a".into());
    report.pruning.generated = 1;
    report.pruning.evaluated = 1;
    report.pruning.confirmed = 1;
    assert_eq!(report.validate(), Ok(()));

    report.corpus = None;
    assert_eq!(
        report.validate(),
        Err("certifying report is missing its corpus eligibility ledger")
    );
}

/// A ledger that does not account for every requested occurrence is an internally inconsistent artifact, not a record of exclusions, and is refused regardless of certification.
#[test]
fn a_ledger_that_does_not_reconcile_is_refused() {
    let mut report = sample();
    let mut corpus = ledger();
    corpus.requested = 673;
    report.corpus = Some(corpus);
    assert_eq!(
        report.validate(),
        Err("corpus eligibility ledger does not account for every requested occurrence")
    );
}

fn candidate(id: &str) -> CandidateReport {
    CandidateReport {
        id: id.into(),
        backend_id: format!("backend-{id}"),
        certification: Certification::EstimateOnly,
        score: None,
        production_blocks_publication: false,
    }
}

fn confirmed_candidate(id: &str, work: u64) -> CandidateReport {
    CandidateReport {
        id: id.into(),
        backend_id: format!("backend-{id}"),
        certification: Certification::FullHcConfirmed {
            words: 1,
            corpus_hash: "c".into(),
        },
        score: Some(Score {
            states: work,
            arcs: work,
            build: 100,
            apply: 100,
            proposals: work,
            confirmation: work,
            confirmation_steps: work,
            raw_paths: work,
        }),
        production_blocks_publication: false,
    }
}

fn sample() -> BackendOptimizationReport {
    BackendOptimizationReport {
        schema_version: BACKEND_REPORT_SCHEMA_VERSION,
        score_schema_version: DETERMINISTIC_SCORE_SCHEMA_VERSION,
        input_hash: "x".into(),
        registry_version: "r".into(),
        registry_hash: "registry-hash".into(),
        tool_version: "t".into(),
        tool_hash: "h".into(),
        seed: 0,
        budgets: Budget::default(),
        usage: BudgetUsage::default(),
        replay_parameters: std::collections::BTreeMap::new(),
        strategy: Strategy::Exhaustive,
        quality: SearchQuality::Exact,
        counts: SpaceCounts {
            syntactic: 0,
            attested: 0,
            static_count: 0,
            feasible: FeasibleCount::Exact {
                value: 0,
                overflowed: false,
            },
        },
        corpus: Some(ledger()),
        pilot: PilotSummary::default(),
        pruning: PruningWaterfall::default(),
        search: SearchAccounting {
            unexplored_method: "none".into(),
            declared_not_searched: 0,
            ..SearchAccounting::default()
        },
        termination: Termination::NoCandidates,
        baseline: None,
        winner: None,
        winner_strategy: None,
        frontier: vec![],
        candidates: vec![],
        baseline_plan_json_path: None,
        baseline_plan_mermaid_path: None,
        winner_plan_json_path: None,
        winner_plan_mermaid_path: None,
    }
}
