//! Offline, evidence-backed recipe optimization.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pg_foma::capability::{compose_envelope, default_registry, CompileDecision};
use pg_foma::enumerate::{CandidateRole, LoweredCandidate};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::plan_diagram::{render_mermaid, RenderMode};
use pg_foma::recipe_optimizer::{
    choose_strategy_with_policy, optimize_with_evaluator, AdaptivePolicy, Budget, BudgetUsage,
    CandidateEvaluator, CandidateState, ConfirmationEvidence, ConstraintTopology,
    DefaultStrategyRegistry, PilotCosts, StrategyRegistry,
};
use pg_foma::recipe_registry::{Registry, FAMILY_ORDERED_MORPHOPHONOLOGY, REGISTRY_SCHEMA_VERSION};
use pg_foma::recipe_report::{
    CandidateReport, PruningWaterfall, RecipeOptimizationReport, SearchAccounting,
    DETERMINISTIC_SCORE_SCHEMA_VERSION, RECIPE_REPORT_SCHEMA_VERSION,
};
use pg_foma::recipe_runtime::{evaluate_plans_with_cache, RunEvaluationCache, RuntimeBudget};
use pg_foma::recipe_space::StageMeasurement;
use pg_foma::recipe_space::{characterize_with_semantics, summarize_pilot};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeOptimizeArgs {
    pub grammar: String,
    pub words: String,
    pub out_dir: String,
    pub seed: u64,
    pub budget: Budget,
    /// `--oracle-step-cap`: overrides `RuntimeBudget::oracle_step_cap`. `None` (the flag omitted)
    /// leaves `recipe_runtime`'s own default (`DEFAULT_ORACLE_STEP_CAP`) in force — NOT unbounded;
    /// see that constant's doc for why an unbounded oracle call is the defect this whole mechanism
    /// exists to prevent.
    pub oracle_step_cap: Option<usize>,
    /// `--oracle-liveness-net-ms` (legacy alias `--oracle-word-timeout-ms`): overrides
    /// `RuntimeBudget::oracle_liveness_net`. Same "`None` = use the default, not unbounded"
    /// convention as `oracle_step_cap` above. This is a LIVENESS NET, not a classifier: tripping it
    /// aborts the run rather than excluding the word it tripped on.
    pub oracle_liveness_net: Option<Duration>,
    /// `--oracle-memory-ceiling-bytes`: overrides `RuntimeBudget::oracle_memory_ceiling`. Same
    /// `None` convention. Also never a classifier -- exceeding it aborts the run.
    pub oracle_memory_ceiling: Option<u64>,
    pub search_all_families: bool,
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

const PROGRESS_FILE_NAME: &str = "progress.jsonl";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CandidateProgressRow {
    #[serde(flatten)]
    report: CandidateReport,
    realized_strategy: String,
}

struct ProgressWriter {
    file: File,
}

impl ProgressWriter {
    fn create(path: &Path) -> Result<Self, RecipeOptimizeError> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|e| RecipeOptimizeError::Io(format!("create {}: {e}", path.display())))?;
        Ok(Self { file })
    }

    fn append(&mut self, row: &CandidateProgressRow) -> std::io::Result<()> {
        let json = serde_json::to_vec(row).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("serialize progress: {e}"),
            )
        })?;
        self.file.write_all(&json)?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        self.file.sync_data()
    }
}

fn read_progress_rows(path: &Path) -> Vec<CandidateProgressRow> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .filter_map(Result::ok)
        .filter_map(|line| serde_json::from_str::<CandidateProgressRow>(&line).ok())
        .filter(|row| {
            !row.report.id.is_empty()
                && !row.report.recipe_id.is_empty()
                && row.report.score.is_some()
                && !row.realized_strategy.is_empty()
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeOptimizeError {
    Usage(String),
    InvalidValue {
        option: String,
        value: String,
    },
    Io(String),
    Runtime(String),
    Timeout(String),
    /// The oracle's liveness net or declared memory ceiling tripped, so the requested corpus's
    /// ELIGIBILITY could not be determined. Deliberately its own variant and not `Runtime`: the
    /// distinction a reader needs is between "a candidate failed" and "we never established which
    /// words were in scope", and only the second one invalidates the whole run's evidence.
    OraclePreparation(String),
}
impl std::fmt::Display for RecipeOptimizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(s) | Self::Io(s) | Self::Runtime(s) | Self::Timeout(s) => f.write_str(s),
            Self::OraclePreparation(s) => f.write_str(s),
            Self::InvalidValue { option, value } => write!(f, "invalid {option}: {value}"),
        }
    }
}
impl std::error::Error for RecipeOptimizeError {}

pub fn parse_args(args: &[String]) -> Result<RecipeOptimizeArgs, RecipeOptimizeError> {
    if args.len() < 3 {
        return Err(RecipeOptimizeError::Usage("usage: recipe-optimize <grammar> <words.txt> <out-dir> [--seed N] [--candidates N] [--evaluations N] [--elapsed-ns N] [--build-ns N] [--memory-bytes N] [--confirmation-work N] [--reserve-ns N] [--oracle-step-cap N] [--oracle-liveness-net-ms N] [--oracle-memory-ceiling-bytes N] [--search-all-families]".into()));
    }
    let mut r = RecipeOptimizeArgs {
        grammar: args[0].clone(),
        words: args[1].clone(),
        out_dir: args[2].clone(),
        seed: 0,
        budget: Budget::default(),
        oracle_step_cap: None,
        oracle_liveness_net: None,
        oracle_memory_ceiling: None,
        search_all_families: false,
    };
    let mut i = 3;
    while i < args.len() {
        let key = args[i].strip_prefix("--").ok_or_else(|| {
            RecipeOptimizeError::Usage(format!("unexpected argument: {}", args[i]))
        })?;
        if key == "search-all-families" {
            r.search_all_families = true;
            i += 1;
            continue;
        }
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
            // No `--confirmation-ns` alias: `Budget::confirmation` is a count of full-HC confirmation
            // calls, not nanoseconds, and every other `*-ns` flag on this command is a real time
            // budget. An `-ns` spelling would invite callers to pass a nanosecond figure and silently
            // get an astronomically large call allowance.
            "confirmation-work" => r.budget.confirmation = n,
            "reserve-ns" => r.budget.reserve = n,
            // `RuntimeBudget`'s own doc explains why `None` here means "use the default", not
            // "unbounded" -- these two flags are the only way to override that default from the
            // CLI, mirroring `pangloss batch --step-cap`/`--word-timeout-ms`.
            "oracle-step-cap" => r.oracle_step_cap = Some(n as usize),
            // Both spellings accepted: the flag was named for a per-word TIMEOUT back when a
            // timeout could exclude a word. It cannot any more, so the new name says what it is,
            // and the old one keeps existing scripts working rather than failing them obscurely.
            "oracle-liveness-net-ms" | "oracle-word-timeout-ms" => {
                r.oracle_liveness_net = Some(Duration::from_millis(n))
            }
            "oracle-memory-ceiling-bytes" => r.oracle_memory_ceiling = Some(n),
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
    plans: BTreeMap<String, LoweredCandidate>,
    /// What actually compiled each candidate, keyed by candidate id, recorded as it is evaluated.
    ///
    /// Necessary because a candidate's DECLARED strategy is not always what ran: a marker-carrying
    /// baseline is evaluated evidence-first and falls back to the tuned emitter only if composing its
    /// plan fails, so it can be measured on a network its declaration does not name. Reporting the
    /// declaration would make `winner_strategy` wrong in precisely the case that field exists to
    /// clarify.
    realized: BTreeMap<String, &'static str>,
    capability: pg_foma::capability::PredicateRegistry,
    oracle_step_cap: Option<usize>,
    oracle_liveness_net: Option<Duration>,
    oracle_memory_ceiling: Option<u64>,
    cache: &'a mut RunEvaluationCache,
    progress: Option<ProgressWriter>,
    progress_error: Option<String>,
}
impl Evaluator<'_> {
    fn append_progress(
        &mut self,
        candidate: &CandidateState,
        certification: &pg_foma::recipe_optimizer::Certification,
        score: pg_foma::recipe_optimizer::Score,
        realized_strategy: &str,
    ) {
        let row = CandidateProgressRow {
            report: CandidateReport {
                id: candidate.id.clone(),
                recipe_id: candidate.signature.clone(),
                certification: certification.clone(),
                score: Some(score),
                pruning_reason: None,
                score_fields_complete: true,
            },
            realized_strategy: realized_strategy.to_owned(),
        };
        if let Some(writer) = self.progress.as_mut() {
            if let Err(error) = writer.append(&row) {
                self.progress_error
                    .get_or_insert_with(|| format!("write progress JSONL: {error}"));
            } else if let Some(ms) =
                std::env::var_os("PANGLOSS_RECIPE_OPTIMIZE_TEST_SLEEP_AFTER_PROGRESS_MS")
                    .and_then(|value| value.to_string_lossy().parse::<u64>().ok())
            {
                std::thread::sleep(Duration::from_millis(ms));
            }
        }
    }
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
        // Task 7.13: no baseline argument. This evaluator is called once per candidate with a
        // single-element slice, so a POSITIONAL test answered `true` for every candidate, and the
        // `&[c.baseline]` slice that replaced it was a second copy of a fact the candidate already
        // owns -- correct only for as long as this call site kept passing the matching element. The
        // role now travels on the `LoweredCandidate` in `self.plans`, set once at materialization.
        let e = evaluate_plans_with_cache(
            self.grammar,
            std::slice::from_ref(plan),
            self.words,
            RuntimeBudget {
                build: Some(remaining.build),
                confirmation: Some(remaining.confirmation),
                oracle_step_cap: self.oracle_step_cap,
                oracle_liveness_net: self.oracle_liveness_net,
                oracle_memory_ceiling: self.oracle_memory_ceiling,
                ..Default::default()
            },
            self.cache,
        )
        .remove(0);
        let realized_strategy = e.realized_strategy.label();
        self.realized.insert(c.id.clone(), realized_strategy);
        self.append_progress(&c, &e.certification, e.score, realized_strategy);
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

/// `BranchAndBound` has no production incumbent today: each `CandidateState` is constructed with
/// `exact_objective: None`, so no candidate can meet the only condition that increments `pruned`.
/// Keep the inert field honest at the report boundary until a real admissible bound is wired.
fn assert_pruned_is_structurally_zero(pruned: u64) {
    assert_eq!(
        pruned, 0,
        "SearchAccounting.pruned must remain zero until production supplies an admissible exact \
         objective to branch-and-bound"
    );
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
    let out = Path::new(&a.out_dir);
    fs::create_dir_all(out)
        .map_err(|e| RecipeOptimizeError::Io(format!("create {}: {e}", a.out_dir)))?;
    let progress = ProgressWriter::create(&out.join(PROGRESS_FILE_NAME))?;
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

    let mut run_cache = RunEvaluationCache::prepare(
        &grammar,
        &words,
        RuntimeBudget {
            oracle_step_cap: a.oracle_step_cap,
            oracle_liveness_net: a.oracle_liveness_net,
            oracle_memory_ceiling: a.oracle_memory_ceiling,
            ..Default::default()
        },
    )
    .map_err(|fault| RecipeOptimizeError::OraclePreparation(fault.to_string()))?;
    // Derived here, immediately after preparation and BEFORE any candidate exists, from the raw
    // `words` slice. Both properties are load-bearing: it is in band (nothing outside this process
    // chose which occurrences count) and it is candidate-independent by construction (no candidate
    // has been materialized, let alone evaluated, at this point in the run).
    let corpus_evidence = run_cache.corpus_evidence(&words);
    // ONE derivation for this whole run.
    // It feeds the baseline enumeration, `recipe_space::characterize`, both instance-selection
    // calls, and -- the one that actually matters for cost -- the per-instance applicability
    // re-check inside the materialization loop below, avoiding a re-walk of the grammar once per
    // candidate instance.
    let semantics = GrammarSemantics::derive(&grammar);
    let alphabet = pg_foma::replace::SegAlphabet::new(&grammar.char_tables[0]);
    let prules = semantics.prules_in_order();
    let phon = pg_foma::junctions::PhonologyProbe::new_with_semantics(&semantics);
    let baseline_started = Instant::now();
    let baseline =
        pg_foma::enumerate::enumerate_default(&grammar, &alphabet, prules, phon.as_ref());
    let baseline_materialization_ns = elapsed_ns(baseline_started).max(1);
    let registry = Registry::seeded();
    registry
        .validate_ready()
        .map_err(|error| RecipeOptimizeError::Runtime(error.to_string()))?;
    let characterization = characterize_with_semantics(
        &semantics,
        &registry,
        &baseline,
        a.budget.candidates,
        a.seed,
    )
    .map_err(|e| RecipeOptimizeError::Runtime(e.to_string()))?;
    let policy = AdaptivePolicy::default();
    // Both denominators, because the pruning waterfall's first bucket is "the registry offered this
    // instance and the grammar rejected it". `PruningWaterfall`'s own doc calls every field "a
    // disjoint bucket of `generated`", so `generated` has to count what the registry OFFERED, not
    // just what survived applicability -- otherwise `inapplicable` is structurally unreachable and
    // renders as a permanent, unfalsifiable `0` (the same false-zero that doc criticises for
    // `N_syntactic`, and `reconciles()` cannot catch it because both sides drop the same term).
    let facts = &characterization.facts;
    let compositional = facts.ordering_dependencies == 0
        && facts.gated_subrules == 0
        && facts.partitions <= 1
        && facts.morphology_layers <= 1;
    let offered_instances = registry.instances().len() as u64;
    let applicable_instances = registry.instances_for_semantics(&semantics);
    let inapplicable = offered_instances.saturating_sub(applicable_instances.len() as u64);
    let (mut instances, declared_not_searched) = registry.instances_for_search_with_semantics(
        &semantics,
        compositional,
        a.search_all_families,
    );
    instances.sort_by_key(|instance| {
        let baseline = instance.family_id == FAMILY_ORDERED_MORPHOPHONOLOGY
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
    // Keyed on (plan root, strategy label), not the root alone: a whole-grammar strategy carries the
    // BASELINE plan because its compiler derives its own topology, so a root-only key would call it a
    // duplicate of the baseline and drop the one candidate whose network can differ for a reason
    // minimization cannot erase. Mirrors `Registry::materialize_distinct`'s own key.
    let mut roots = std::collections::BTreeSet::<(pg_foma::plan::NodeId, &'static str)>::new();
    let baseline_root = baseline.root().ok_or_else(|| {
        RecipeOptimizeError::Runtime("enumerate_default produced a rootless Plan".into())
    })?;
    let baseline_id = baseline_root.to_string();
    roots.insert((
        baseline_root,
        pg_foma::enumerate::EmissionStrategy::PlanComposed.label(),
    ));
    materialization_times.insert(baseline_id.clone(), baseline_materialization_ns);
    states.push(CandidateState {
        id: baseline_id.clone(),
        family: FAMILY_ORDERED_MORPHOPHONOLOGY.into(),
        signature: format!("{FAMILY_ORDERED_MORPHOPHONOLOGY}|topology=baseline"),
        lower_bound: baseline.len() as u64,
        // Always `None`: no cheap/pilot evaluation runs before search here, so this candidate
        // (like every other one built below) can never populate `BranchAndBound`'s incumbent.
        // This is why `SearchAccounting.pruned` is structurally zero in production -- see that
        // field's doc in `pg_foma::recipe_report`.
        exact_objective: None,
        baseline: true,
    });
    plans.insert(
        baseline_id,
        LoweredCandidate {
            label: "baseline",
            plan: baseline.clone(),
            adapter: pg_foma::executable_candidate::LoweringAdapter::ControllablePlanCompose,
            // The one candidate in this run that IS the grammar's default compilation. Stated on the
            // candidate itself so the evaluator can never infer it from position or from a
            // caller-maintained parallel slice.
            role: CandidateRole::Baseline,
        },
    );
    // The `1` is the baseline plan, generated directly rather than through a family.
    let production_generated = 1u64.saturating_add(offered_instances);
    for instance in instances {
        let materialize_started = Instant::now();
        let plan = match registry.materialize_with_semantics(
            &instance,
            &pg_foma::recipe_registry::MaterializerContext {
                grammar: &grammar,
                baseline: &baseline,
            },
            &semantics,
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
        // A plan-composed candidate keeps the bare root as its id (existing reports and gates pin
        // that). A whole-grammar strategy must NOT: it reuses the baseline plan, so a bare-root id
        // would collide with the baseline's own entry in `plans`/`states` and overwrite it.
        let id = if !plan.adapter.interprets_plan() {
            format!("{root}@{}", plan.strategy().label())
        } else {
            root.to_string()
        };
        materialization_times.insert(id.clone(), materialize_ns);
        if !roots.insert((root, plan.strategy().label())) {
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
            // Same structural note as the baseline candidate above: this call site never
            // populates `exact_objective` either, so `SearchAccounting.pruned` stays zero.
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
            // Neither stage RAN -- the capability envelope refused the candidate before any network
            // was built -- so `build`/`evaluation` must stay `None` rather than a literal `0`: a
            // literal zero would fold into `summarize_pilot`'s build/evaluation percentiles and pull
            // a pilot sample with refusals toward a build cost for a stage that never executed.
            // Those percentiles feed `PilotCosts` and therefore the choice of SEARCH STRATEGY, so a
            // fake zero would not be merely cosmetic. `None` says "not measured", which is what
            // happened.
            measurements.push(StageMeasurement {
                materialize: materialization_times[&state.id],
                capability: capability_ns,
                build: None,
                evaluation: None,
                pruned: true,
            });
            continue;
        }
        let eval = evaluate_plans_with_cache(
            &grammar,
            std::slice::from_ref(plan),
            &pilot_words,
            RuntimeBudget {
                build: Some(a.budget.build / 4),
                confirmation: Some(a.budget.confirmation / 4),
                oracle_step_cap: a.oracle_step_cap,
                oracle_liveness_net: a.oracle_liveness_net,
                oracle_memory_ceiling: a.oracle_memory_ceiling,
                ..Default::default()
            },
            &mut run_cache,
        )
        .remove(0);
        pilot_build = pilot_build.saturating_add(eval.score.build);
        pilot_confirmation = pilot_confirmation.saturating_add(eval.score.confirmation);
        pilot_evaluations = pilot_evaluations.saturating_add(1);
        measurements.push(StageMeasurement {
            materialize: materialization_times[&state.id],
            capability: capability_ns,
            build: Some(eval.score.build.max(1)),
            evaluation: Some(eval.score.apply.max(1)),
            pruned: false,
        });
    }
    let pilot = summarize_pilot(&measurements, a.seed);
    let strong_pruning = pilot.pruning_ratio_ppm >= policy.strong_pruning_ppm;
    let topology = ConstraintTopology {
        strong_pruning,
        compositional,
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
        realized: BTreeMap::new(),
        capability,
        oracle_step_cap: a.oracle_step_cap,
        oracle_liveness_net: a.oracle_liveness_net,
        oracle_memory_ceiling: a.oracle_memory_ceiling,
        cache: &mut run_cache,
        progress: Some(progress),
        progress_error: None,
    };
    let mut outcome = optimize_with_evaluator(
        &states,
        search_budget,
        a.seed,
        strategy_impl.as_ref(),
        &mut evaluator,
    );
    if let Some(error) = evaluator.progress_error.take() {
        return Err(RecipeOptimizeError::Io(error));
    }
    assert_pruned_is_structurally_zero(outcome.search.pruned);
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
            score_fields_complete: true,
        })
        .collect::<Vec<_>>();
    let baseline_id = states.iter().find(|s| s.baseline).map(|s| s.id.clone());
    let winner = outcome.winner.clone();
    fs::create_dir_all(Path::new(&a.out_dir))
        .map_err(|e| RecipeOptimizeError::Io(format!("create {}: {e}", a.out_dir)))?;
    let base_doc = pg_foma::plan_diagram::build_plan_document_for_plan_with_semantics(
        &semantics,
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
        .map(|p| {
            pg_foma::plan_diagram::build_plan_document_for_plan_with_semantics(&semantics, &p.plan)
        });
    let (winner_json_path, winner_mmd_path) = if let Some(d) = winner_doc {
        let j = d
            .to_json()
            .map_err(|e| RecipeOptimizeError::Runtime(e.to_string()))?;
        let m = render_mermaid(&d, RenderMode::Full).mermaid;
        fs::write(out.join("winner.plan.json"), &j)
            .map_err(|e| RecipeOptimizeError::Io(e.to_string()))?;
        fs::write(out.join("winner.plan.mmd"), &m)
            .map_err(|e| RecipeOptimizeError::Io(e.to_string()))?;
        (
            Some("winner.plan.json".into()),
            Some("winner.plan.mmd".into()),
        )
    } else {
        (None, None)
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
        schema_version: RECIPE_REPORT_SCHEMA_VERSION,
        score_schema_version: DETERMINISTIC_SCORE_SCHEMA_VERSION,
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
                "search_all_families".into(),
                a.search_all_families.to_string(),
            ),
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
        // IN-BAND DERIVATION. This ledger is derived by the run itself, from `words` -- the RAW
        // lines of the caller's corpus file, before any eligibility reasoning -- against the SAME
        // prepared oracle results that decided which occurrences were comparable. The optimizer
        // never accepts a pre-filtered eligible list and never reports "zero exclusions" without
        // naming the requested corpus it derived that zero from; `RecipeOptimizationReport::
        // validate` refuses a certifying report that omits this.
        corpus: Some(corpus_evidence),
        pilot,
        pruning: PruningWaterfall {
            generated: production_generated,
            inapplicable,
            duplicates,
            declared_not_searched,
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
            declared_not_searched,
        },
        termination: outcome.search.termination,
        baseline: baseline_id,
        // Resolved from the winning candidate's own plan entry, not re-derived from its id: the id's
        // `@strategy` suffix is a display convenience and must not become the source of truth for
        // what compiled the winner.
        // The REALIZED strategy, recorded during evaluation -- not the candidate's declaration. See
        // `Evaluator::realized` for why those differ and why the declaration would be wrong here.
        winner_strategy: winner
            .as_ref()
            .and_then(|id| evaluator.realized.get(id))
            .map(|label| (*label).to_owned()),
        winner,
        frontier: outcome.frontier,
        candidates: evaluated,
        // `report.json` names these paths rather than inlining the FULL text of
        // `baseline.plan.json`, `baseline.plan.mmd`, `winner.plan.json` and `winner.plan.mmd`,
        // which this function has already written to `out` above. Two copies of one artifact in
        // one run directory can disagree, and nothing could say which was authoritative; the files
        // are.
        baseline_plan_json_path: Some("baseline.plan.json".into()),
        baseline_plan_mermaid_path: Some("baseline.plan.mmd".into()),
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
    let candidates = read_progress_rows(&out.join(PROGRESS_FILE_NAME));
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
        "score_schema_version": DETERMINISTIC_SCORE_SCHEMA_VERSION,
        "candidates": candidates,
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        assert_pruned_is_structurally_zero, parse_args, read_progress_rows, RecipeOptimizeError,
    };

    #[test]
    fn usage_documents_search_all_families_replay_flag() {
        let error = parse_args(&["grammar.xml".into(), "words.txt".into()]).unwrap_err();
        match error {
            RecipeOptimizeError::Usage(message) => {
                assert!(message.contains("--search-all-families"));
            }
            other => panic!("expected usage error, got {other:?}"),
        }
    }

    #[test]
    fn search_all_families_parses_as_replay_opt_in() {
        let args = vec![
            "grammar.xml".into(),
            "words.txt".into(),
            "out".into(),
            "--search-all-families".into(),
        ];
        assert!(parse_args(&args).unwrap().search_all_families);
    }

    #[test]
    fn production_search_accounting_rejects_nonzero_pruned_count() {
        assert_pruned_is_structurally_zero(0);
        let result = std::panic::catch_unwind(|| assert_pruned_is_structurally_zero(1));
        assert!(
            result.is_err(),
            "a nonzero pruned count must not enter the production report until a real admissible \
             bound is wired"
        );
    }

    #[test]
    fn progress_reader_keeps_complete_rows_and_discards_malformed_or_truncated_rows() {
        let tag = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("pangloss-recipe-progress-{tag}.jsonl"));
        let complete = serde_json::json!({
            "id": "candidate-1",
            "recipe_id": "recipe-1",
            "certification": {
                "status": "full-hc-confirmed",
                "words": 1,
                "corpus_hash": "hash"
            },
            "score": {
                "states": 1,
                "arcs": 1,
                "build": 1,
                "apply": 1,
                "proposals": 1,
                "confirmation": 1,
                "confirmation_steps": 1,
                "raw_paths": 1
            },
            "pruning_reason": null,
            "realized_strategy": "plan-composed"
        });
        fs::write(
            &path,
            format!("{}\nnot-json\n{{\"id\":\"truncated\"", complete),
        )
        .unwrap();

        let rows = read_progress_rows(&path);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].report.id, "candidate-1");
        assert_eq!(rows[0].realized_strategy.as_str(), "plan-composed");
        let _ = fs::remove_file(path);
    }
}
