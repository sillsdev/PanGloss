//! Offline, evidence-backed recipe optimization.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pg_foma::capability::{compose_envelope, default_registry, CompileDecision};
use pg_foma::enumerate::CandidatePlan;
use pg_foma::plan_diagram::{render_mermaid, RenderMode};
use pg_foma::recipe_optimizer::{
    choose_strategy_with_policy, optimize_with_evaluator, AdaptivePolicy, Budget, BudgetUsage,
    CandidateEvaluator, CandidateState, ConfirmationEvidence, ConstraintTopology,
    DefaultStrategyRegistry, PilotCosts, StrategyRegistry,
};
use pg_foma::recipe_registry::{Registry, REGISTRY_SCHEMA_VERSION};
use pg_foma::recipe_report::{
    CandidateReport, PruningWaterfall, RecipeOptimizationReport, SearchAccounting,
};
use pg_foma::recipe_runtime::{evaluate_plans, RuntimeBudget};
use pg_foma::recipe_space::StageMeasurement;
use pg_foma::recipe_space::{characterize, summarize_pilot};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeOptimizeArgs {
    pub grammar: String,
    pub words: String,
    pub out_dir: String,
    pub seed: u64,
    pub budget: Budget,
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn hash_current_executable() -> Result<String, RecipeOptimizeError> {
    let path = std::env::current_exe()
        .map_err(|e| RecipeOptimizeError::Io(format!("locate current executable: {e}")))?;
    let bytes = fs::read(&path)
        .map_err(|e| RecipeOptimizeError::Io(format!("read {}: {e}", path.display())))?;
    Ok(hash_bytes(&bytes))
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeOptimizeError {
    Usage(String),
    InvalidValue { option: String, value: String },
    Io(String),
    Runtime(String),
    Timeout(String),
}
impl std::fmt::Display for RecipeOptimizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(s) | Self::Io(s) | Self::Runtime(s) | Self::Timeout(s) => f.write_str(s),
            Self::InvalidValue { option, value } => write!(f, "invalid {option}: {value}"),
        }
    }
}
impl std::error::Error for RecipeOptimizeError {}

pub fn parse_args(args: &[String]) -> Result<RecipeOptimizeArgs, RecipeOptimizeError> {
    if args.len() < 3 {
        return Err(RecipeOptimizeError::Usage("usage: recipe-optimize <grammar> <words.txt> <out-dir> [--seed N] [--candidates N] [--evaluations N] [--elapsed-ns N] [--build-ns N] [--memory-bytes N] [--confirmation-work N] [--reserve-ns N]".into()));
    }
    let mut r = RecipeOptimizeArgs {
        grammar: args[0].clone(),
        words: args[1].clone(),
        out_dir: args[2].clone(),
        seed: 0,
        budget: Budget::default(),
    };
    let mut i = 3;
    while i < args.len() {
        let key = args[i].strip_prefix("--").ok_or_else(|| {
            RecipeOptimizeError::Usage(format!("unexpected argument: {}", args[i]))
        })?;
        let value = args
            .get(i + 1)
            .ok_or_else(|| RecipeOptimizeError::Usage(format!("missing value for --{key}")))?;
        let n = value
            .parse()
            .map_err(|_| RecipeOptimizeError::InvalidValue {
                option: format!("--{key}"),
                value: value.clone(),
            })?;
        match key {
            "seed" => r.seed = n,
            "candidates" => r.budget.candidates = n,
            "evaluations" => r.budget.evaluations = n,
            "elapsed-ns" => r.budget.elapsed = n,
            "build-ns" => r.budget.build = n,
            "memory-bytes" => r.budget.memory = n,
            "confirmation-work" | "confirmation-ns" => r.budget.confirmation = n,
            "reserve-ns" => r.budget.reserve = n,
            _ => {
                return Err(RecipeOptimizeError::Usage(format!(
                    "unknown option: --{key}"
                )))
            }
        }
        i += 2;
    }
    if r.budget.reserve > r.budget.elapsed {
        return Err(RecipeOptimizeError::InvalidValue {
            option: "--reserve-ns".into(),
            value: r.budget.reserve.to_string(),
        });
    }
    Ok(r)
}

struct Evaluator<'a> {
    grammar: &'a pg_grammar::model::Grammar,
    words: &'a [String],
    plans: BTreeMap<String, CandidatePlan>,
    capability: pg_foma::capability::PredicateRegistry,
}
impl CandidateEvaluator for Evaluator<'_> {
    fn evaluate(&mut self, c: &CandidateState, remaining: Budget) -> ConfirmationEvidence {
        let plan = self
            .plans
            .get(&c.id)
            .expect("selected recipe has materialized plan");
        if let CompileDecision::Refuse(diagnostics) =
            compose_envelope(self.grammar, &plan.plan, &self.capability)
        {
            return ConfirmationEvidence {
                certification: pg_foma::recipe_optimizer::Certification::CapabilityRejected {
                    reason: format!("{diagnostics:?}"),
                },
                score: None,
                usage: BudgetUsage::default(),
            };
        }
        let e = evaluate_plans(
            self.grammar,
            std::slice::from_ref(plan),
            self.words,
            RuntimeBudget {
                build: Some(remaining.build),
                confirmation: Some(remaining.confirmation),
                ..Default::default()
            },
        )
        .remove(0);
        ConfirmationEvidence {
            certification: e.certification,
            score: Some(e.score),
            usage: BudgetUsage {
                elapsed: e.score.build.saturating_add(e.score.apply),
                build: e.score.build,
                confirmation: e.score.confirmation,
                ..Default::default()
            },
        }
    }
}

fn hash_inputs(grammar: &str, words: &str) -> Result<String, RecipeOptimizeError> {
    let mut h = Sha256::new();
    h.update(
        fs::read(grammar).map_err(|e| RecipeOptimizeError::Io(format!("read {grammar}: {e}")))?,
    );
    h.update(fs::read(words).map_err(|e| RecipeOptimizeError::Io(format!("read {words}: {e}")))?);
    Ok(format!("{:x}", h.finalize()))
}

pub fn run_recipe_optimize(args: &[String]) -> Result<(), RecipeOptimizeError> {
    if std::env::var_os("PANGLOSS_RECIPE_OPTIMIZE_CHILD").is_none() {
        return run_recipe_optimize_supervised(args);
    }
    if let Some(ms) = std::env::var_os("PANGLOSS_RECIPE_OPTIMIZE_TEST_SLEEP_MS")
        .and_then(|v| v.to_string_lossy().parse::<u64>().ok())
    {
        std::thread::sleep(Duration::from_millis(ms));
    }
    let a = parse_args(args)?;
    let run_started = Instant::now();
    let (grammar, warnings) =
        crate::load_grammar(&a.grammar).map_err(RecipeOptimizeError::Runtime)?;
    crate::print_grammar_warnings(&warnings);
    let words = fs::read_to_string(&a.words)
        .map_err(|e| RecipeOptimizeError::Io(format!("read {}: {e}", a.words)))?
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let alphabet = pg_foma::replace::SegAlphabet::new(&grammar.char_tables[0]);
    let prules = grammar
        .strata
        .iter()
        .flat_map(|s| &s.prules)
        .map(|id| &grammar.prules[id.0 as usize])
        .collect::<Vec<_>>();
    let phon = pg_foma::junctions::PhonologyProbe::new(&grammar);
    let baseline_started = Instant::now();
    let baseline =
        pg_foma::enumerate::enumerate_default(&grammar, &alphabet, &prules, phon.as_ref());
    let baseline_materialization_ns = elapsed_ns(baseline_started).max(1);
    let registry = Registry::seeded();
    registry
        .validate_ready()
        .map_err(|error| RecipeOptimizeError::Runtime(error.to_string()))?;
    let characterization =
        characterize(&grammar, &registry, &baseline, a.budget.candidates, a.seed)
            .map_err(|e| RecipeOptimizeError::Runtime(e.to_string()))?;
    let policy = AdaptivePolicy::default();
    let mut instances = registry.instances_for_grammar(&grammar);
    instances.sort_by_key(|instance| {
        let baseline = instance.family_id == "ordered-morphophonology"
            && instance
                .parameters
                .get("topology")
                .is_some_and(|value| value == "baseline");
        (!baseline, instance.canonical_key())
    });
    let mut plans = BTreeMap::new();
    let mut states = Vec::new();
    let mut measurements = Vec::new();
    let mut pilot_build = 0u64;
    let mut pilot_confirmation = 0u64;
    let mut pilot_evaluations = 0u64;
    let mut materialization_times = BTreeMap::new();
    let capability = default_registry();
    let mut capability_rejected = 0u64;
    let mut materialization_rejects = 0u64;
    let mut duplicates = 0u64;
    let mut roots = std::collections::BTreeSet::new();
    let baseline_root = baseline.root().ok_or_else(|| {
        RecipeOptimizeError::Runtime("enumerate_default produced a rootless Plan".into())
    })?;
    let baseline_id = baseline_root.to_string();
    roots.insert(baseline_root);
    materialization_times.insert(baseline_id.clone(), baseline_materialization_ns);
    states.push(CandidateState {
        id: baseline_id.clone(),
        family: "ordered-morphophonology".into(),
        signature: "ordered-morphophonology|topology=baseline".into(),
        lower_bound: baseline.len() as u64,
        exact_objective: None,
        baseline: true,
    });
    plans.insert(
        baseline_id,
        CandidatePlan {
            label: "baseline",
            plan: baseline.clone(),
        },
    );
    let production_generated = 1u64.saturating_add(instances.len() as u64);
    for instance in instances {
        let materialize_started = Instant::now();
        let plan = match registry.materialize(
            &instance,
            &pg_foma::recipe_registry::MaterializerContext {
                grammar: &grammar,
                baseline: &baseline,
            },
        ) {
            Ok(plan) => plan,
            Err(pg_foma::recipe_registry::MaterializeError::Inapplicable(_))
            | Err(pg_foma::recipe_registry::MaterializeError::Invalid(_)) => {
                materialization_rejects = materialization_rejects.saturating_add(1);
                continue;
            }
            Err(error) => return Err(RecipeOptimizeError::Runtime(error.to_string())),
        };
        let materialize_ns = elapsed_ns(materialize_started).max(1);
        let recipe_id = instance.canonical_key();
        let root = plan.plan.root().ok_or_else(|| {
            RecipeOptimizeError::Runtime("materialized recipe has no root".into())
        })?;
        let id = root.to_string();
        materialization_times.insert(id.clone(), materialize_ns);
        if !roots.insert(root) {
            duplicates = duplicates.saturating_add(1);
            continue;
        }
        if matches!(
            compose_envelope(&grammar, &plan.plan, &capability),
            CompileDecision::Refuse(_)
        ) {
            capability_rejected = capability_rejected.saturating_add(1);
            continue;
        }
        let lower = plan.plan.len() as u64;
        states.push(CandidateState {
            id: id.clone(),
            family: instance.family_id,
            signature: recipe_id,
            lower_bound: lower,
            exact_objective: None,
            baseline: false,
        });
        plans.insert(id, plan);
    }
    let pilot_words = words
        .iter()
        .take(policy.pilot_word_cap)
        .cloned()
        .collect::<Vec<_>>();
    let pilot_limit = a
        .budget
        .candidates
        .min(a.budget.evaluations)
        .min(states.len() as u64)
        .min(policy.pilot_candidate_cap as u64) as usize;
    let pilot_ids =
        pg_foma::recipe_space::deterministic_sample_indices(states.len(), pilot_limit, a.seed);
    for index in pilot_ids {
        let state = &states[index];
        let plan = &plans[&state.id];
        let cap_started = Instant::now();
        let decision = compose_envelope(&grammar, &plan.plan, &capability);
        let capability_ns = elapsed_ns(cap_started).max(1);
        if matches!(decision, CompileDecision::Refuse(_)) {
            measurements.push(StageMeasurement {
                materialize: materialization_times[&state.id],
                capability: capability_ns,
                build: 0,
                evaluation: 0,
                pruned: true,
            });
            continue;
        }
        let eval = evaluate_plans(
            &grammar,
            std::slice::from_ref(plan),
            &pilot_words,
            RuntimeBudget {
                build: Some(a.budget.build / 4),
                confirmation: Some(a.budget.confirmation / 4),
                ..Default::default()
            },
        )
        .remove(0);
        pilot_build = pilot_build.saturating_add(eval.score.build);
        pilot_confirmation = pilot_confirmation.saturating_add(eval.score.confirmation);
        pilot_evaluations = pilot_evaluations.saturating_add(1);
        measurements.push(StageMeasurement {
            materialize: materialization_times[&state.id],
            capability: capability_ns,
            build: eval.score.build.max(1),
            evaluation: eval.score.apply.max(1),
            pruned: false,
        });
    }
    let pilot = summarize_pilot(&measurements, a.seed);
    let strong_pruning = pilot.pruning_ratio_ppm >= policy.strong_pruning_ppm;
    let facts = &characterization.facts;
    let topology = ConstraintTopology {
        strong_pruning,
        compositional: facts.ordering_dependencies == 0
            && facts.gated_subrules == 0
            && facts.partitions <= 1
            && facts.morphology_layers <= 1,
    };
    let presearch_elapsed = elapsed_ns(run_started);
    let mut search_budget = a.budget;
    search_budget.elapsed = search_budget.elapsed.saturating_sub(presearch_elapsed);
    search_budget.build = search_budget.build.saturating_sub(pilot_build);
    search_budget.confirmation = search_budget
        .confirmation
        .saturating_sub(pilot_confirmation);
    search_budget.evaluations = search_budget.evaluations.saturating_sub(pilot_evaluations);
    let strategy = choose_strategy_with_policy(
        states.len() as u64,
        PilotCosts {
            p50: pilot
                .materialize
                .p50
                .saturating_add(pilot.capability.p50)
                .saturating_add(pilot.build.p50)
                .saturating_add(pilot.evaluation.p50),
            p95: pilot
                .materialize
                .p95
                .saturating_add(pilot.capability.p95)
                .saturating_add(pilot.build.p95)
                .saturating_add(pilot.evaluation.p95),
        },
        search_budget,
        topology,
        policy,
    );
    let strategy_impl = DefaultStrategyRegistry {
        beam_width: policy.beam_width,
    }
    .get(strategy)
    .unwrap();
    let mut evaluator = Evaluator {
        grammar: &grammar,
        words: &words,
        plans,
        capability,
    };
    let mut outcome = optimize_with_evaluator(
        &states,
        search_budget,
        a.seed,
        strategy_impl.as_ref(),
        &mut evaluator,
    );
    outcome.usage.elapsed = outcome.usage.elapsed.saturating_add(presearch_elapsed);
    outcome.usage.build = outcome.usage.build.saturating_add(pilot_build);
    outcome.usage.confirmation = outcome
        .usage
        .confirmation
        .saturating_add(pilot_confirmation);
    outcome.usage.evaluations = outcome.usage.evaluations.saturating_add(pilot_evaluations);
    let evaluated = outcome
        .evaluated
        .iter()
        .map(|e| CandidateReport {
            id: e.candidate.id.clone(),
            recipe_id: e.candidate.signature.clone(),
            certification: e.evidence.certification.clone(),
            score: e.evidence.score,
            pruning_reason: None,
        })
        .collect::<Vec<_>>();
    let baseline_id = states.iter().find(|s| s.baseline).map(|s| s.id.clone());
    let winner = outcome.winner.clone();
    fs::create_dir_all(Path::new(&a.out_dir))
        .map_err(|e| RecipeOptimizeError::Io(format!("create {}: {e}", a.out_dir)))?;
    let out = Path::new(&a.out_dir);
    let base_doc = pg_foma::plan_diagram::build_plan_document_for_plan(
        &grammar,
        &evaluator.plans[baseline_id
            .as_ref()
            .ok_or_else(|| RecipeOptimizeError::Runtime("baseline was not materialized".into()))?]
        .plan,
    );
    let base_json = base_doc
        .to_json()
        .map_err(|e| RecipeOptimizeError::Runtime(e.to_string()))?;
    let base_mmd = render_mermaid(&base_doc, RenderMode::Full).mermaid;
    fs::write(out.join("baseline.plan.json"), &base_json)
        .map_err(|e| RecipeOptimizeError::Io(e.to_string()))?;
    fs::write(out.join("baseline.plan.mmd"), &base_mmd)
        .map_err(|e| RecipeOptimizeError::Io(e.to_string()))?;
    let winner_doc = winner
        .as_ref()
        .and_then(|id| evaluator.plans.get(id))
        .map(|p| pg_foma::plan_diagram::build_plan_document_for_plan(&grammar, &p.plan));
    let (winner_json, winner_mmd, winner_json_path, winner_mmd_path) = if let Some(d) = winner_doc {
        let j = d
            .to_json()
            .map_err(|e| RecipeOptimizeError::Runtime(e.to_string()))?;
        let m = render_mermaid(&d, RenderMode::Full).mermaid;
        fs::write(out.join("winner.plan.json"), &j)
            .map_err(|e| RecipeOptimizeError::Io(e.to_string()))?;
        fs::write(out.join("winner.plan.mmd"), &m)
            .map_err(|e| RecipeOptimizeError::Io(e.to_string()))?;
        (
            Some(j),
            Some(m),
            Some("winner.plan.json".into()),
            Some("winner.plan.mmd".into()),
        )
    } else {
        (None, None, None, None)
    };
    let c = characterization.counts;
    let built_count = outcome
        .evaluated
        .iter()
        .filter(|evaluated| {
            evaluated
                .evidence
                .score
                .is_some_and(|score| score.states > 0 || score.arcs > 0)
        })
        .count() as u64;
    let all_evaluated = outcome.search.quality == pg_foma::recipe_optimizer::SearchQuality::Exact
        && outcome.search.termination == pg_foma::recipe_optimizer::Termination::Complete
        && outcome.evaluated.len() == states.len();
    let feasible = if all_evaluated {
        pg_foma::recipe_space::FeasibleCount::Exact {
            value: built_count,
            overflowed: false,
        }
    } else {
        let upper = states.len() as u64;
        pg_foma::recipe_space::FeasibleCount::Estimate {
            lower: built_count,
            upper,
            sample_size: outcome.evaluated.len() as u64,
            uncertainty: upper.saturating_sub(built_count),
            method: "bounded-production-build-evaluation".into(),
            overflowed: false,
        }
    };
    let counts = pg_foma::recipe_report::SpaceCounts {
        syntactic: c.syntactic.value,
        attested: c.attested.value,
        static_count: states.len() as u64,
        feasible,
    };
    let input_hash = hash_inputs(&a.grammar, &a.words)?;
    let registry_hash = hash_bytes(registry.canonical_json().as_bytes());
    let tool_hash = hash_current_executable()?;
    let report = RecipeOptimizationReport {
        schema_version: 1,
        input_hash,
        registry_version: REGISTRY_SCHEMA_VERSION.to_string(),
        registry_hash,
        tool_version: env!("CARGO_PKG_VERSION").into(),
        tool_hash,
        seed: a.seed,
        budgets: a.budget,
        usage: outcome.usage,
        replay_parameters: BTreeMap::from([
            ("seed".into(), a.seed.to_string()),
            (
                "registry_schema_version".into(),
                REGISTRY_SCHEMA_VERSION.to_string(),
            ),
            (
                "exhaustive_budget_numerator".into(),
                policy.exhaustive_budget_numerator.to_string(),
            ),
            (
                "exhaustive_budget_denominator".into(),
                policy.exhaustive_budget_denominator.to_string(),
            ),
            ("beam_width".into(), policy.beam_width.to_string()),
            (
                "pilot_candidate_cap".into(),
                policy.pilot_candidate_cap.to_string(),
            ),
            ("pilot_word_cap".into(), policy.pilot_word_cap.to_string()),
            (
                "strong_pruning_ppm".into(),
                policy.strong_pruning_ppm.to_string(),
            ),
        ]),
        strategy: outcome.search.strategy,
        quality: outcome.search.quality,
        counts,
        pilot,
        pruning: PruningWaterfall {
            generated: production_generated,
            inapplicable: 0,
            duplicates,
            materialization_rejects,
            capability_rejected,
            evaluated: outcome.evaluated.len() as u64,
            confirmed: outcome
                .evaluated
                .iter()
                .filter(|e| e.evidence.certification.selectable())
                .count() as u64,
            unvisited: 0,
            budget_pruned: outcome.search.unexplored,
            ..Default::default()
        },
        search: SearchAccounting {
            generated: outcome.search.generated,
            expanded: outcome.search.expanded,
            explored: outcome.search.explored,
            pruned: outcome.search.pruned,
            unexplored: outcome.search.unexplored,
            unexplored_method: "search accounting".into(),
            overflowed: false,
        },
        termination: outcome.search.termination,
        baseline: baseline_id,
        winner,
        frontier: outcome.frontier,
        candidates: evaluated,
        baseline_plan_json: Some(base_json),
        baseline_mermaid: Some(base_mmd),
        baseline_plan_json_path: Some("baseline.plan.json".into()),
        baseline_plan_mermaid_path: Some("baseline.plan.mmd".into()),
        winner_plan_json: winner_json,
        winner_mermaid: winner_mmd,
        winner_plan_json_path: winner_json_path,
        winner_plan_mermaid_path: winner_mmd_path,
    };
    if !report.pruning.reconciles() {
        eprintln!("recipe optimizer pruning mismatch: {:?}", report.pruning);
    }
    report
        .validate()
        .map_err(|e| RecipeOptimizeError::Runtime(e.into()))?;
    fs::write(out.join("report.json"), report.canonical_json())
        .map_err(|e| RecipeOptimizeError::Io(e.to_string()))?;
    fs::write(out.join("report.md"), report.markdown())
        .map_err(|e| RecipeOptimizeError::Io(e.to_string()))?;
    Ok(())
}

fn write_supervisor_failure_report(
    parsed: &RecipeOptimizeArgs,
    reason: &str,
    usage: serde_json::Value,
) {
    let out = Path::new(&parsed.out_dir);
    let _ = fs::create_dir_all(out);
    let partial = serde_json::json!({
        "schema_version": 1,
        "status": "budget-exhausted",
        "certifying": false,
        "termination": "budget-exhausted",
        "reason": reason,
        "seed": parsed.seed,
        "budgets": parsed.budget,
        "usage": usage,
        "inputs": { "grammar": parsed.grammar, "words": parsed.words },
        "counts": null,
        "pilot": null,
        "pruning": null,
        "search": {
            "explored": null,
            "unexplored": null,
            "unexplored_method": "worker terminated before a final checkpoint"
        },
        "candidates": [],
        "frontier": [],
        "winner": null
    });
    if let Ok(json) = serde_json::to_string_pretty(&partial) {
        let _ = fs::write(out.join("partial-report.json"), &json);
        let _ = fs::write(out.join("status.json"), json);
    }
}

fn merge_supervisor_memory_peak(
    parsed: &RecipeOptimizeArgs,
    observed_peak: u64,
) -> Result<(), RecipeOptimizeError> {
    let path = Path::new(&parsed.out_dir).join("report.json");
    let json = fs::read_to_string(&path)
        .map_err(|e| RecipeOptimizeError::Io(format!("read {}: {e}", path.display())))?;
    let mut report: RecipeOptimizationReport = serde_json::from_str(&json)
        .map_err(|e| RecipeOptimizeError::Runtime(format!("parse {}: {e}", path.display())))?;
    report.usage.memory_peak = report.usage.memory_peak.max(observed_peak);
    report
        .validate()
        .map_err(|e| RecipeOptimizeError::Runtime(e.into()))?;
    fs::write(&path, report.canonical_json())
        .map_err(|e| RecipeOptimizeError::Io(format!("write {}: {e}", path.display())))?;
    fs::write(
        Path::new(&parsed.out_dir).join("report.md"),
        report.markdown(),
    )
    .map_err(|e| RecipeOptimizeError::Io(e.to_string()))?;
    Ok(())
}
fn run_recipe_optimize_supervised(args: &[String]) -> Result<(), RecipeOptimizeError> {
    let parsed = parse_args(args)?;
    let timeout = Duration::from_nanos(parsed.budget.elapsed);
    let mut command =
        Command::new(std::env::current_exe().map_err(|e| {
            RecipeOptimizeError::Runtime(format!("locate pangloss executable: {e}"))
        })?);
    command
        .arg("__recipe-optimize-child")
        .args(args)
        .env("PANGLOSS_RECIPE_OPTIMIZE_CHILD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let mut child = command
        .spawn()
        .map_err(|e| RecipeOptimizeError::Runtime(format!("spawn recipe worker: {e}")))?;
    let started = Instant::now();
    let mut system = sysinfo::System::new();
    let memory_limit = parsed.budget.memory;
    let mut observed_peak = 0u64;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| RecipeOptimizeError::Runtime(format!("wait for recipe worker: {e}")))?
        {
            return if status.success() {
                merge_supervisor_memory_peak(&parsed, observed_peak)
            } else {
                Err(RecipeOptimizeError::Runtime(format!(
                    "recipe worker exited with {status}"
                )))
            };
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            write_supervisor_failure_report(
                &parsed,
                "elapsed deadline exceeded",
                serde_json::json!({ "elapsed_ns": elapsed_ns(started) }),
            );
            return Err(RecipeOptimizeError::Timeout(format!(
                "recipe optimization exceeded elapsed deadline of {} ns",
                parsed.budget.elapsed
            )));
        }
        if memory_limit > 0 {
            let pid = sysinfo::Pid::from_u32(child.id());
            system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), false);
            if let Some(process) = system.process(pid) {
                let bytes = process.memory();
                observed_peak = observed_peak.max(bytes);
                if bytes > memory_limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    write_supervisor_failure_report(
                        &parsed,
                        "memory limit exceeded",
                        serde_json::json!({
                            "elapsed_ns": elapsed_ns(started),
                            "memory_bytes": bytes,
                            "limit_bytes": memory_limit
                        }),
                    );
                    return Err(RecipeOptimizeError::Runtime(format!(
                        "recipe optimization exceeded memory limit of {memory_limit} bytes"
                    )));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
