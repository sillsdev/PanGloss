//! Canonical, replayable optimization evidence.
use crate::recipe_optimizer::{
    Budget, BudgetUsage, Certification, Score, SearchQuality, Strategy, Termination,
};
use crate::recipe_space::FeasibleCount;
use crate::recipe_space::PilotSummary;
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SpaceCounts {
    pub syntactic: u64,
    pub attested: u64,
    pub static_count: u64,
    pub feasible: FeasibleCount,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CandidateReport {
    pub id: String,
    pub recipe_id: String,
    pub certification: Certification,
    pub score: Option<Score>,
    pub pruning_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct PruningWaterfall {
    pub generated: u64,
    pub inapplicable: u64,
    pub duplicates: u64,
    pub materialization_rejects: u64,
    pub capability_rejected: u64,
    pub build_failures: u64,
    pub evaluated: u64,
    pub confirmed: u64,
    pub unvisited: u64,
    pub budget_pruned: u64,
}

impl PruningWaterfall {
    /// Every field here is a disjoint bucket of `generated` and is checked by [`Self::reconciles`].
    /// D1's `N_syntactic`/`N_attested`/`N_static` deliberately do NOT appear: they are upstream
    /// space sizes, not buckets of this funnel, they live in [`SpaceCounts`], and when they were
    /// mirrored here nothing populated them — every real report rendered them as a false `0` that
    /// `reconciles()` could not catch, since the balance equation never referenced them.
    pub fn reconciles(&self) -> bool {
        self.generated
            == self
                .inapplicable
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
    pub pruned: u64,
    pub unexplored: u64,
    pub unexplored_method: String,
    pub overflowed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecipeOptimizationReport {
    pub schema_version: u32,
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
    pub pilot: PilotSummary,
    pub pruning: PruningWaterfall,
    pub search: SearchAccounting,
    pub termination: Termination,
    pub baseline: Option<String>,
    pub winner: Option<String>,
    /// The winning candidate's `EmissionStrategy` label, when there is a winner.
    ///
    /// Load-bearing for reading `winner_plan_json`/`winner_mermaid` correctly. Those artifacts render
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
    pub baseline_plan_json: Option<String>,
    pub baseline_mermaid: Option<String>,
    pub baseline_plan_json_path: Option<String>,
    pub baseline_plan_mermaid_path: Option<String>,
    pub winner_plan_json: Option<String>,
    pub winner_mermaid: Option<String>,
    pub winner_plan_json_path: Option<String>,
    pub winner_plan_mermaid_path: Option<String>,
}
impl RecipeOptimizationReport {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.pruning.reconciles() {
            return Err("pruning waterfall does not reconcile");
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
        format!("# Recipe optimization\n\n## Metadata\n\n- Schema version: {}\n- Input hash: {}\n- Registry version: {}\n- Registry hash: {}\n- Tool version: {}\n- Tool hash: {}\n- Seed: {}\n- Strategy: {:?}\n- Quality: {}\n- Non-optimality: {} search; unexplored space is quantified as {}.\n\n## Budgets and usage\n\n- Budgets: {:?}\n- Actual usage: {:?}\n- Replay parameters: {:?}\n\n## Feasible space\n\n- Serialized count: {:?}\n\n## Pilot\n\n{:?}\n\n## Pruning waterfall\n\n{:?}\n\n## Search accounting\n\n{:?}\n\n## Artifacts\n\n- Baseline plan: {}\n- Winner plan: {}\n\n## Candidates\n\n| ID | Certification | Score |\n|---|---|---|\n{}\n\n## Result\n\n- Termination: {:?}\n- Baseline: {}\n- Winner: {}\n- Winner confirmed: {}\n- Pareto frontier: {}\n", r.schema_version,r.input_hash,r.registry_version,r.registry_hash,r.tool_version,r.tool_hash,r.seed,r.strategy,exactness,if r.quality == SearchQuality::Exact { "none" } else { "not proven optimal" },r.search.unexplored,r.budgets,r.usage,r.replay_parameters,r.counts.feasible,r.pilot,r.pruning,r.search,r.baseline_plan_json_path.as_deref().unwrap_or("none"),r.winner_plan_json_path.as_deref().unwrap_or("none"),r.candidates.iter().map(|c| format!("| {} | {:?} | {:?} |",c.id,c.certification,c.score)).collect::<Vec<_>>().join("\n"),r.termination,r.baseline.as_deref().unwrap_or("none"),winner,winner_confirmed,r.frontier.join(", "))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn json_round_trips() {
        let r = RecipeOptimizationReport {
            schema_version: 1,
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
            },
            termination: Termination::Complete,
            baseline: Some("b".into()),
            winner: Some("b".into()),
            winner_strategy: Some("plan-composed".into()),
            frontier: vec!["b".into()],
            candidates: vec![],
            baseline_plan_json: None,
            baseline_mermaid: None,
            baseline_plan_json_path: None,
            baseline_plan_mermaid_path: None,
            winner_plan_json: None,
            winner_mermaid: None,
            winner_plan_json_path: None,
            winner_plan_mermaid_path: None,
        };
        assert_eq!(
            RecipeOptimizationReport::from_json(&r.canonical_json()).unwrap(),
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
            generated: 10,
            inapplicable: 1,
            duplicates: 1,
            materialization_rejects: 1,
            capability_rejected: 1,
            build_failures: 1,
            evaluated: 3,
            confirmed: 2,
            unvisited: 1,
            budget_pruned: 1,
            ..Default::default()
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
        });
        assert!(r.validate().is_ok());
        assert!(r.replay_parameters.contains_key("seed") || r.seed == 0);
    }

    fn candidate(id: &str) -> CandidateReport {
        CandidateReport {
            id: id.into(),
            recipe_id: format!("recipe-{id}"),
            certification: Certification::EstimateOnly,
            score: None,
            pruning_reason: None,
        }
    }

    fn sample() -> RecipeOptimizationReport {
        RecipeOptimizationReport {
            schema_version: 1,
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
            pilot: PilotSummary::default(),
            pruning: PruningWaterfall::default(),
            search: SearchAccounting {
                unexplored_method: "none".into(),
                ..SearchAccounting::default()
            },
            termination: Termination::NoCandidates,
            baseline: None,
            winner: None,
            winner_strategy: None,
            frontier: vec![],
            candidates: vec![],
            baseline_plan_json: None,
            baseline_mermaid: None,
            baseline_plan_json_path: None,
            baseline_plan_mermaid_path: None,
            winner_plan_json: None,
            winner_mermaid: None,
            winner_plan_json_path: None,
            winner_plan_mermaid_path: None,
        }
    }
}
