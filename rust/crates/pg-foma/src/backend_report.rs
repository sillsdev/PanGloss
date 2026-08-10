//! Canonical, replayable optimization evidence.
use crate::backend_optimizer::{
    Budget, BudgetUsage, Certification, CorpusCompletenessEvidence, Score, SearchQuality, Strategy,
    Termination,
};

/// Bumped 1 -> 2 when `BackendOptimizationReport::corpus` and the oracle-configuration fields on
/// `CorpusCompletenessEvidence` became part of the artifact. A version-1 report states a
/// certification without stating the corpus it certified or the bounds it was derived under, so it
/// is not comparable with a version-2 one and must not silently deserialize as if it were.
pub const BACKEND_REPORT_SCHEMA_VERSION: u32 = 2;
pub const DETERMINISTIC_SCORE_SCHEMA_VERSION: u32 = 2;
use crate::backend_space::FeasibleCount;
use crate::backend_space::PilotSummary;
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpaceCounts {
    pub syntactic: u64,
    pub attested: u64,
    pub static_count: u64,
    pub feasible: FeasibleCount,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CandidateReport {
    pub id: String,
    pub backend_id: String,
    pub certification: Certification,
    pub score: Option<Score>,
    pub pruning_reason: Option<String>,
    /// Deserialization provenance for the deterministic coordinates that formerly defaulted.
    /// Not serialized: it is recomputed from field presence every time JSON is read.
    #[serde(skip)]
    pub score_fields_complete: bool,
}

impl<'de> serde::Deserialize<'de> for CandidateReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct ScoreWire {
            states: u64,
            arcs: u64,
            build: u64,
            apply: u64,
            proposals: u64,
            confirmation: u64,
            #[serde(default)]
            confirmation_steps: Option<u64>,
            #[serde(default)]
            raw_paths: Option<u64>,
        }

        #[derive(serde::Deserialize)]
        struct CandidateWire {
            id: String,
            backend_id: String,
            certification: Certification,
            score: Option<ScoreWire>,
            pruning_reason: Option<String>,
        }

        let wire = CandidateWire::deserialize(deserializer)?;
        let (score, score_fields_complete) = match wire.score {
            None => (None, true),
            Some(score) => {
                let complete = score.confirmation_steps.is_some() && score.raw_paths.is_some();
                (
                    Some(Score {
                        states: score.states,
                        arcs: score.arcs,
                        build: score.build,
                        apply: score.apply,
                        proposals: score.proposals,
                        confirmation: score.confirmation,
                        confirmation_steps: score.confirmation_steps.unwrap_or_default(),
                        raw_paths: score.raw_paths.unwrap_or_default(),
                    }),
                    complete,
                )
            }
        };
        Ok(Self {
            id: wire.id,
            backend_id: wire.backend_id,
            certification: wire.certification,
            score,
            pruning_reason: wire.pruning_reason,
            score_fields_complete,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct PruningWaterfall {
    pub generated: u64,
    pub inapplicable: u64,
    pub duplicates: u64,
    #[serde(default)]
    pub declared_not_searched: u64,
    pub materialization_rejects: u64,
    pub capability_rejected: u64,
    pub build_failures: u64,
    pub evaluated: u64,
    pub confirmed: u64,
    pub unvisited: u64,
    pub budget_pruned: u64,
}

impl PruningWaterfall {
    /// Every field here is a disjoint bucket of `generated` and is checked by `Self::reconciles`.
    /// D1's `N_syntactic`/`N_attested`/`N_static` deliberately do NOT appear: they are upstream
    /// space sizes, not buckets of this funnel, they live in `SpaceCounts`, and when they were
    /// mirrored here nothing populated them — every real report rendered them as a false `0` that
    /// `reconciles()` could not catch, since the balance equation never referenced them.
    pub fn reconciles(&self) -> bool {
        self.generated
            == self
                .inapplicable
                .saturating_add(self.declared_not_searched)
                .saturating_add(self.duplicates)
                .saturating_add(self.materialization_rejects)
                .saturating_add(self.capability_rejected)
                .saturating_add(self.build_failures)
                .saturating_add(self.evaluated)
                .saturating_add(self.unvisited)
                .saturating_add(self.budget_pruned)
            && self.confirmed <= self.evaluated
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct SearchAccounting {
    pub generated: u64,
    pub expanded: u64,
    pub explored: u64,
    /// Candidates rejected by `backend_optimizer`'s branch-and-bound because their `lower_bound`
    /// exceeded the running incumbent. This is **structurally always
    /// zero in production today**: both call sites that build a `BranchAndBoundCandidate` in
    /// `pg-cli/src/backend_optimize.rs` set `exact_objective: None`, so `incumbent` (initialized to
    /// `u64::MAX`) never drops and no candidate's `lower_bound` can ever exceed it. Do not read
    /// this field as a live "the search pruned N candidates" signal until a real admissible bound
    /// is wired (`exact_objective` populated from an actual completed-evaluation cost) — an open
    /// question, deferred pending a cost model. See
    /// `backend_optimizer::tests::pruned_is_structurally_zero_in_production_shaped_run` for the
    /// pinning test.
    pub pruned: u64,
    pub unexplored: u64,
    pub unexplored_method: String,
    pub overflowed: bool,
    #[serde(default)]
    pub declared_not_searched: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BackendOptimizationReport {
    pub schema_version: u32,
    /// Version of the deterministic score evidence carried by every scored candidate.
    ///
    /// Reports predating `raw_paths` omit this field and deserialize as `0`. They remain readable,
    /// but validation refuses to rank their unmeasured proposer work as if it were a measured zero.
    #[serde(default)]
    pub score_schema_version: u32,
    pub input_hash: String,
    pub registry_version: String,
    pub registry_hash: String,
    pub tool_version: String,
    pub tool_hash: String,
    pub seed: u64,
    pub budgets: Budget,
    pub usage: BudgetUsage,
    pub replay_parameters: std::collections::BTreeMap<String, String>,
    pub strategy: Strategy,
    pub quality: SearchQuality,
    pub counts: SpaceCounts,
    /// The run-scoped eligibility ledger: which requested occurrences the oracle could adjudicate,
    /// which it could not and why, and the configuration all of that was derived under.
    ///
    /// # Why it lives on the RUN and not on each candidate
    /// Eligibility is decided by the ORACLE, before any candidate is built, and every candidate in
    /// a run is judged against the same included set. Storing one ledger per run makes
    /// candidate-independence a property of the schema instead of a property somebody has to keep
    /// re-verifying across N per-candidate copies.
    ///
    /// # Why it is required for a certifying report
    /// A report with a selectable candidate but no ledger is refused, pinned by
    /// `a_certifying_report_must_carry_its_corpus_eligibility_ledger`. A run that
    /// excluded nothing must still SAY so, over a named requested corpus, or a hand-filtered word
    /// list fed in from outside is indistinguishable from an honest full-corpus run — which is
    /// exactly how a certification came to be labelled provisional after a 653-row filtered list
    /// could not be derived from the raw 673.
    ///
    /// `Option` + `#[serde(default)]` only so a non-certifying partial artifact (a run that died
    /// before preparation finished) can still be written; it is not an escape hatch for a
    /// certifying one.
    #[serde(default)]
    pub corpus: Option<CorpusCompletenessEvidence>,
    pub pilot: PilotSummary,
    pub pruning: PruningWaterfall,
    pub search: SearchAccounting,
    pub termination: Termination,
    pub baseline: Option<String>,
    pub winner: Option<String>,
    /// The winning candidate's `EmissionStrategy` label, when there is a winner.
    ///
    /// Load-bearing for reading `winner.plan.json`/`winner.plan.mmd` correctly. Those artifacts render
    /// the winning candidate's `Plan`, and for a plan-composed winner that plan IS what got compiled.
    /// For a whole-grammar strategy it is NOT: that compiler derives its own topology and never
    /// interprets the plan, which it carries only because a candidate must have one. Without this
    /// field a reader opening `winner.plan.mmd` would reasonably believe the diagram depicts what the
    /// winner compiled, and for such a winner it does not. `#[serde(default)]` so reports written
    /// before this field existed still parse.
    #[serde(default)]
    pub winner_strategy: Option<String>,
    pub frontier: Vec<String>,
    pub candidates: Vec<CandidateReport>,
    /// Only paths are carried here, never the FULL TEXT of these artifacts
    /// (`baseline_plan_json`, `baseline_mermaid`, `winner_plan_json`, `winner_mermaid` do not
    /// exist as fields): writing each artifact twice -- once as a file in the run directory, once
    /// verbatim inside `report.json` -- would let two copies of one artifact disagree with no way
    /// to say which is authoritative. These paths, relative to the run directory, name the single
    /// copy that is.
    pub baseline_plan_json_path: Option<String>,
    pub baseline_plan_mermaid_path: Option<String>,
    pub winner_plan_json_path: Option<String>,
    pub winner_plan_mermaid_path: Option<String>,
}
impl BackendOptimizationReport {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != BACKEND_REPORT_SCHEMA_VERSION {
            return Err("unsupported backend report schema version");
        }
        if !self.pruning.reconciles() {
            return Err("pruning waterfall does not reconcile");
        }
        let has_scores = self
            .candidates
            .iter()
            .any(|candidate| candidate.score.is_some());
        if has_scores && self.score_schema_version != DETERMINISTIC_SCORE_SCHEMA_VERSION {
            return Err("unsupported deterministic score schema version");
        }
        if self
            .candidates
            .iter()
            .any(|candidate| candidate.score.is_some() && !candidate.score_fields_complete)
        {
            return Err("deterministic score is missing measurement provenance");
        }
        if self
            .candidates
            .iter()
            .any(|candidate| candidate.certification.selectable() && candidate.score.is_none())
        {
            return Err("selectable candidate is missing a score");
        }
        // A confirmed candidate is a claim about a specific requested corpus under specific oracle bounds; without the ledger the artifact cannot say which corpus it saw or be compared against a run at a different cap.
        let certifies = self
            .candidates
            .iter()
            .any(|candidate| candidate.certification.selectable());
        match &self.corpus {
            None if certifies => {
                return Err("certifying report is missing its corpus eligibility ledger")
            }
            Some(corpus) if !corpus.reconciles() => {
                return Err(
                    "corpus eligibility ledger does not account for every requested occurrence",
                )
            }
            _ => {}
        }
        let candidate_ids: std::collections::BTreeSet<_> = self
            .candidates
            .iter()
            .map(|candidate| &candidate.id)
            .collect();
        if candidate_ids.len() != self.candidates.len() {
            return Err("candidate ids are not unique");
        }
        let frontier_ids: std::collections::BTreeSet<_> = self.frontier.iter().collect();
        if frontier_ids.len() != self.frontier.len() {
            return Err("frontier ids are not unique");
        }
        if let Some(winner) = &self.winner {
            let candidate = self
                .candidates
                .iter()
                .find(|candidate| &candidate.id == winner)
                .ok_or("winner is not an evaluated candidate")?;
            if !candidate.certification.selectable() || candidate.score.is_none() {
                return Err("winner is not fully confirmed and scored");
            }
        }
        if self.frontier.iter().any(|id| {
            !self
                .candidates
                .iter()
                .any(|c| &c.id == id && c.certification.selectable() && c.score.is_some())
        }) {
            return Err("frontier contains a candidate that is not fully confirmed and scored");
        }
        let ranking: Vec<_> = self
            .candidates
            .iter()
            .filter_map(|candidate| {
                candidate
                    .score
                    .map(|score| (candidate.id.clone(), candidate.certification.clone(), score))
            })
            .collect();
        let expected_frontier = crate::backend_optimizer::pareto_frontier(&ranking);
        if self.frontier != expected_frontier {
            return Err("serialized frontier does not match recomputed frontier");
        }
        let expected_winner = crate::backend_optimizer::select_confirmed(&ranking);
        if self.winner != expected_winner {
            return Err("serialized winner does not match recomputed winner");
        }
        if self.quality == SearchQuality::Approximate && self.search.unexplored == 0 {
            return Err("approximate search must quantify unexplored space");
        }
        Ok(())
    }

    pub fn canonical(&self) -> Self {
        let mut report = self.clone();
        report.candidates.sort_by(|a, b| a.id.cmp(&b.id));
        report.frontier.sort();
        report
    }

    pub fn canonical_json(&self) -> String {
        serde_json::to_string(&self.canonical()).expect("report is serializable")
    }
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
    pub fn markdown(&self) -> String {
        let r = self.canonical();
        let winner = r.winner.as_deref().unwrap_or("none");
        let winner_confirmed = r.winner.as_ref().is_some_and(|id| {
            r.candidates
                .iter()
                .any(|c| &c.id == id && c.certification.selectable())
        });
        let exactness = if r.quality == SearchQuality::Exact {
            "exact"
        } else {
            "approximate"
        };
        let corpus = match &r.corpus {
            Some(c) => format!(
                "- Requested: {}\n- Included: {}\n- Excluded: {}\n- Requested hash: {}\n- Included \
                 hash: {}\n- Exclusion ledger hash: {}\n- Oracle step cap: {}\n- Oracle memory \
                 ceiling (bytes): {}\n- Oracle liveness net (ns): {}\n- Exclusions: {:?}",
                c.requested,
                c.included,
                c.excluded,
                c.requested_hash,
                c.included_hash,
                c.excluded_hash,
                c.oracle_step_cap,
                c.oracle_memory_ceiling_bytes,
                c.oracle_liveness_net_ns,
                c.exclusions
            ),
            None => "- Not recorded (non-certifying artifact).".to_string(),
        };
        format!("# Backend optimization\n\n## Metadata\n\n- Schema version: {}\n- Input hash: {}\n- Registry version: {}\n- Registry hash: {}\n- Tool version: {}\n- Tool hash: {}\n- Seed: {}\n- Strategy: {:?}\n- Quality: {}\n- Non-optimality: {} search; unexplored space is quantified as {}.\n\n## Budgets and usage\n\n- Budgets: {:?}\n- Actual usage: {:?}\n- Replay parameters: {:?}\n\n## Feasible space\n\n- Serialized count: {:?}\n\n## Corpus eligibility\n\n{}\n\n## Pilot\n\n{:?}\n\n## Pruning waterfall\n\n{:?}\n\n## Search accounting\n\n{:?}\n\n## Artifacts\n\n- Baseline plan: {}\n- Winner plan: {}\n\n## Candidates\n\n| ID | Certification | Score |\n|---|---|---|\n{}\n\n## Result\n\n- Termination: {:?}\n- Baseline: {}\n- Winner: {}\n- Winner confirmed: {}\n- Pareto frontier: {}\n", r.schema_version,r.input_hash,r.registry_version,r.registry_hash,r.tool_version,r.tool_hash,r.seed,r.strategy,exactness,if r.quality == SearchQuality::Exact { "none" } else { "not proven optimal" },r.search.unexplored,r.budgets,r.usage,r.replay_parameters,r.counts.feasible,corpus,r.pilot,r.pruning,r.search,r.baseline_plan_json_path.as_deref().unwrap_or("none"),r.winner_plan_json_path.as_deref().unwrap_or("none"),r.candidates.iter().map(|c| format!("| {} | {:?} | {:?} |",c.id,c.certification,c.score)).collect::<Vec<_>>().join("\n"),r.termination,r.baseline.as_deref().unwrap_or("none"),winner,winner_confirmed,r.frontier.join(", "))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_round_trips() {
        let r = BackendOptimizationReport {
            schema_version: BACKEND_REPORT_SCHEMA_VERSION,
            score_schema_version: DETERMINISTIC_SCORE_SCHEMA_VERSION,
            input_hash: "x".into(),
            registry_version: "r".into(),
            registry_hash: "registry-hash".into(),
            tool_version: "t".into(),
            tool_hash: "h".into(),
            seed: 1,
            budgets: Budget::default(),
            usage: BudgetUsage::default(),
            replay_parameters: std::collections::BTreeMap::new(),
            strategy: Strategy::Exhaustive,
            quality: SearchQuality::Exact,
            counts: SpaceCounts {
                syntactic: 1,
                attested: 1,
                static_count: 1,
                feasible: FeasibleCount::Exact {
                    value: 1,
                    overflowed: false,
                },
            },
            corpus: Some(ledger()),
            pilot: PilotSummary::default(),
            pruning: PruningWaterfall::default(),
            search: SearchAccounting {
                generated: 1,
                expanded: 1,
                explored: 1,
                unexplored: 0,
                pruned: 0,
                unexplored_method: "none".into(),
                overflowed: false,
                declared_not_searched: 0,
            },
            termination: Termination::Complete,
            baseline: Some("b".into()),
            winner: Some("b".into()),
            winner_strategy: Some("plan-composed".into()),
            frontier: vec!["b".into()],
            candidates: vec![],
            baseline_plan_json_path: None,
            baseline_plan_mermaid_path: None,
            winner_plan_json_path: None,
            winner_plan_mermaid_path: None,
        };
        assert_eq!(
            BackendOptimizationReport::from_json(&r.canonical_json()).unwrap(),
            r
        );
        assert!(r.markdown().contains("Search accounting"));
    }

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
            generated: 12,
            inapplicable: 1,
            duplicates: 1,
            declared_not_searched: 2,
            materialization_rejects: 1,
            capability_rejected: 1,
            build_failures: 1,
            evaluated: 3,
            confirmed: 2,
            unvisited: 1,
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
        r.candidates[0].score_fields_complete = true;
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
    fn legacy_report_without_raw_paths_deserializes_but_cannot_rank_as_measured_zero() {
        let mut value = serde_json::to_value(sample()).unwrap();
        value["candidates"] = serde_json::json!([{
            "id": "a",
            "backend_id": "backend-a",
            "certification": {"status": "full-hc-confirmed", "words": 1, "corpus_hash": "c"},
            "score": {"states": 1, "arcs": 1, "build": 1, "apply": 1, "proposals": 1, "confirmation": 1, "confirmation_steps": 1},
            "pruning_reason": null
        }]);
        value["frontier"] = serde_json::json!(["a"]);
        value["winner"] = serde_json::json!("a");
        value
            .as_object_mut()
            .unwrap()
            .remove("score_schema_version");

        let parsed = BackendOptimizationReport::from_json(&value.to_string())
            .expect("legacy reports remain readable");
        assert_eq!(parsed.score_schema_version, 0);
        assert!(!parsed.candidates[0].score_fields_complete);
        assert_eq!(
            parsed.validate(),
            Err("unsupported deterministic score schema version")
        );
    }

    #[test]
    fn current_score_schema_cannot_claim_a_missing_raw_paths_measurement() {
        let mut value = serde_json::to_value(sample()).unwrap();
        value["candidates"] = serde_json::json!([{
            "id": "a",
            "backend_id": "backend-a",
            "certification": {"status": "full-hc-confirmed", "words": 1, "corpus_hash": "c"},
            "score": {"states": 1, "arcs": 1, "build": 1, "apply": 1, "proposals": 1, "confirmation": 1, "confirmation_steps": 1},
            "pruning_reason": null
        }]);
        value["frontier"] = serde_json::json!(["a"]);
        value["winner"] = serde_json::json!("a");

        let parsed = BackendOptimizationReport::from_json(&value.to_string()).unwrap();
        assert_eq!(
            parsed.validate(),
            Err("deterministic score is missing measurement provenance")
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

    /// A complete, reconciling, zero-exclusion ledger over a one-word corpus; every sample report carries one, since a certifying report without one is invalid by construction.
    fn ledger() -> CorpusCompletenessEvidence {
        CorpusCompletenessEvidence::from_selection(
            &["w".to_string()],
            &["w".to_string()],
            vec![],
            crate::backend_optimizer::OracleEligibilityConfig {
                step_cap: 20_000,
                memory_ceiling_bytes: 1 << 33,
                liveness_net_ns: 300_000_000_000,
            },
        )
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
            pruning_reason: None,
            score_fields_complete: true,
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
            pruning_reason: None,
            score_fields_complete: true,
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
}
