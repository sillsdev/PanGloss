//! DEV-ONLY census of the prefix-constrained FST word-completion idea over a compiled foma
//! proposer network; see docs/research/predict-census-design-notes.md for detail and usage.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use foma::lexcread::fsm_lexc_parse_string;
use foma::line_table::CsrArc;
use foma::options::FomaOptions;
use foma::structures::fsm_sort_arcs;

use pg_foma::confirm::{build_morpheme_owners, confirm_all, MorphemeOwner};
use pg_foma::tags;
use pg_grammar::model::{Grammar, MorphemeId};
use pg_parse::Morpher;

// --- fixtures ------------------------------------------------------------------------------

/// `(name, grammar file, wordlist file)`; ordering rationale: docs/research/predict-census-design-notes.md.
const GRAMMARS: &[(&str, &str, &str)] = &[
    ("sena", "sena-hc.xml", "sena-words.txt"),
    ("indonesian", "indonesian-hc.xml", "indonesian-words.txt"),
    ("amharic", "amharic-hc.xml", "amharic-words.txt"),
    ("aweti", "aweti.json", "aweti-words.txt"),
];

fn sample_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name)
}

/// Mirrors `pg-cli`'s `load_grammar` dispatch (and `examples/spellcheck_measure.rs`'s copy of it) for the two fixture shapes this census uses.
fn load_grammar(path: &Path) -> Grammar {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "json" => {
            let json = std::fs::read_to_string(path).expect("read snapshot");
            let snapshot = pg_snapshot::Snapshot::from_json(&json).expect("parse snapshot");
            pg_grammar::compile_project(&snapshot)
                .expect("compile snapshot")
                .0
        }
        _ => {
            let xml = std::fs::read_to_string(path).expect("read grammar xml");
            pg_grammar::load(&xml).expect("load grammar xml")
        }
    }
}

// --- memory budgets: see docs/research/predict-census-design-notes.md for the two gaps this closes ---

/// Ceiling on the compiled network's true state count; see docs/research/predict-census-design-notes.md.
const DEFAULT_MAX_STATES: usize = 2_000_000;

/// Ceiling on the sigma table's own maximum symbol number; see docs/research/predict-census-design-notes.md.
const DEFAULT_MAX_SIGMA: usize = 200_000;

/// Ceiling on `complete`'s search frontier, in estimated live bytes; see docs/research/predict-census-design-notes.md.
const DEFAULT_MAX_FRONTIER_BYTES: usize = 1_073_741_824;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// A hard pre-flight refusal, checked before `WalkNet::build` allocates anything; never a panic or a silent truncation. See docs/research/predict-census-design-notes.md.
#[derive(Debug)]
enum CensusError {
    NetworkTooLarge {
        dimension: &'static str,
        value: usize,
        limit: usize,
    },
}

impl std::fmt::Display for CensusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CensusError::NetworkTooLarge {
                dimension,
                value,
                limit,
            } => write!(
                f,
                "predict_census refuses to index this network: {value} {dimension} exceeds the \
                 budget of {limit}. Indexing a network this large by state/symbol number risks \
                 exhausting machine memory before a single word is walked; raise the matching \
                 --max-states/--max-sigma flag (or PREDICT_CENSUS_MAX_STATES/\
                 PREDICT_CENSUS_MAX_SIGMA) only if you understand why this grammar's compiled \
                 network is this large."
            ),
        }
    }
}
impl std::error::Error for CensusError {}

/// Pure size-vs-budget check shared by both `WalkNet::build` refusals, so it is testable with plain integers alone; see docs/research/predict-census-design-notes.md.
fn check_network_size(
    dimension: &'static str,
    value: usize,
    limit: usize,
) -> Result<(), CensusError> {
    if value > limit {
        Err(CensusError::NetworkTooLarge {
            dimension,
            value,
            limit,
        })
    } else {
        Ok(())
    }
}

// --- the compiled network, indexed for walking ---------------------------------------------

/// A CSR view of the compiled proposer network: per-state arc slices plus the sigma decode table.
struct WalkNet {
    /// `arcs[i]` are state `i`'s outgoing arcs; `final_state[i]` / `start[i]` its flags.
    arcs: Vec<Vec<CsrArc>>,
    final_state: Vec<bool>,
    start: usize,
    /// Symbol number -> its literal text; `None` marks an unrenderable symbol (UNKNOWN/IDENTITY), whose arcs are skipped and counted.
    sigma: Vec<Option<String>>,
}

impl WalkNet {
    /// Builds the CSR walk view, refusing before allocating anything sized by the compiled network's own numbering; see docs/research/predict-census-design-notes.md.
    fn build(g: &Grammar, max_states: usize, max_sigma: usize) -> Result<WalkNet, CensusError> {
        let emitted = pg_foma::emit::emit(g);
        let opts = FomaOptions::default();
        let mut net =
            fsm_lexc_parse_string(&opts, None, &emitted.lexc_source).expect("lexc compile failed");
        // Same arc sort the production proposer uses, so this walk reads arcs in the same order `apply_up` would.
        fsm_sort_arcs(&mut net, 2);

        // state_no is dense and ascending in a compiled net, but do not assume it: map explicitly.
        let mut by_state: HashMap<i32, (bool, bool, Vec<CsrArc>)> = HashMap::new();
        for (block, arcs) in net.states.iter_blocks() {
            let e = by_state
                .entry(block.state_no)
                .or_insert_with(|| (block.final_state != 0, block.start_state != 0, Vec::new()));
            e.0 |= block.final_state != 0;
            e.1 |= block.start_state != 0;
            e.2.extend_from_slice(arcs);
        }

        // Memory budget #1, checked before allocating anything sized by this count; see docs/research/predict-census-design-notes.md.
        let state_count = by_state.len();
        check_network_size("compiled states", state_count, max_states)?;

        // Dense reindex so allocated size tracks true content, not raw (possibly sparse) numbering; see docs/research/predict-census-design-notes.md.
        let mut state_ids: Vec<i32> = by_state.keys().copied().collect();
        state_ids.sort_unstable();
        let dense: HashMap<i32, usize> =
            state_ids.iter().enumerate().map(|(i, &s)| (s, i)).collect();

        let mut arcs = vec![Vec::new(); state_count];
        let mut final_state = vec![false; state_count];
        let mut start = 0usize;
        for (state_no, (fin, st, a)) in by_state {
            let i = dense[&state_no];
            arcs[i] = a
                .into_iter()
                .map(|arc| CsrArc {
                    // Fallback to 0 is defensive only: a well-formed net's every arc target is a `dense` key.
                    target: *dense.get(&arc.target).unwrap_or(&0) as i32,
                    ..arc
                })
                .collect();
            final_state[i] = fin;
            if st {
                start = i;
            }
        }

        // Memory budget #2: guards the array size actually allocated (`max_sym + 1`); see docs/research/predict-census-design-notes.md.
        let max_sym = net.sigma.iter().map(|s| s.number).max().unwrap_or(0).max(2) as usize;
        check_network_size("sigma symbol numbers", max_sym, max_sigma)?;
        let mut sigma: Vec<Option<String>> = vec![None; max_sym + 1];
        // 0/1/2 are foma's reserved EPSILON/UNKNOWN/IDENTITY slots, never renderable symbol text.
        sigma[0] = Some(String::new());
        for s in &net.sigma {
            if s.number > 2 {
                sigma[s.number as usize] = Some(s.symbol.to_string());
            }
        }
        Ok(WalkNet {
            arcs,
            final_state,
            start,
            sigma,
        })
    }

    fn sym(&self, n: i16) -> Option<&str> {
        if n < 0 {
            return Some("");
        }
        self.sigma.get(n as usize).and_then(|o| o.as_deref())
    }
}

// --- the prefix-constrained walk ------------------------------------------------------------

struct WalkCfg {
    max_completions: usize,
    max_steps: usize,
    /// Cap on how far past the typed prefix a completion may run, in bytes; bounds the free-tail phase against a grammar's own cycles.
    max_extra_bytes: usize,
    /// Cap on the search frontier's estimated live bytes; the memory dimension `max_steps`/`max_completions` never provided. See docs/research/predict-census-design-notes.md.
    max_frontier_bytes: usize,
}

struct Completion {
    surface: String,
    morphemes: Vec<(bool, MorphemeId)>,
}

/// Which budget dimension stopped a walk early, and the value observed when it tripped. See docs/research/predict-census-design-notes.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalkBudgetDimension {
    Steps,
    Completions,
    FrontierBytes,
}

impl WalkBudgetDimension {
    fn label(self) -> &'static str {
        match self {
            WalkBudgetDimension::Steps => "walk steps",
            WalkBudgetDimension::Completions => "completions",
            WalkBudgetDimension::FrontierBytes => "frontier memory bytes",
        }
    }
}

/// Per-dimension roll-up of every truncation in a prefix-length block: trip count, worst value, and limit. See docs/research/predict-census-design-notes.md.
#[derive(Clone, Copy, Debug)]
struct TruncationTally {
    count: usize,
    peak_value: usize,
    limit: usize,
}

impl TruncationTally {
    fn record(&mut self, value: usize, limit: usize) {
        self.count += 1;
        if value > self.peak_value {
            self.peak_value = value;
        }
        // Every trip of one dimension shares a limit within a block, so last-write is fine here.
        self.limit = limit;
    }
}

#[derive(Debug, Clone, Copy)]
struct WalkTruncation {
    dimension: WalkBudgetDimension,
    value: usize,
    limit: usize,
}

struct WalkOutcome {
    completions: Vec<Completion>,
    steps_used: usize,
    /// `Some` when a budget stopped the walk; the containment number is only meaningful when this is `None`.
    truncated: Option<WalkTruncation>,
    unrenderable_arcs: usize,
}

/// Deliberately an over-estimate of fixed overhead per live frontier frame; see docs/research/predict-census-design-notes.md.
const FRAME_FIXED_OVERHEAD_BYTES: usize = 128;

/// Estimated live bytes one frontier frame holds; uses `len()` rather than `capacity()` for determinism. See docs/research/predict-census-design-notes.md.
fn frame_bytes(surface: &str, analysis: &str) -> usize {
    FRAME_FIXED_OVERHEAD_BYTES + surface.len() + analysis.len()
}

/// One search frame; `bytes` caches its own `frame_bytes` estimate so the running frontier total needs no re-measuring on pop.
struct Frame {
    cost_milli: i64,
    state: usize,
    surface: String,
    analysis: String,
    matched: usize,
    bytes: usize,
}

// Min-heap on cost, tie-broken on shorter surface; see docs/research/predict-census-design-notes.md.
impl Ord for Frame {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .cost_milli
            .cmp(&self.cost_milli)
            .then_with(|| other.surface.len().cmp(&self.surface.len()))
    }
}
impl PartialOrd for Frame {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Frame {
    fn eq(&self, other: &Self) -> bool {
        self.cost_milli == other.cost_milli && self.surface.len() == other.surface.len()
    }
}
impl Eq for Frame {}

/// Incremental ranking cost of traversing one arc, from its analysis-side symbol alone; see docs/research/predict-census-design-notes.md.
fn arc_cost(in_sym: &str, out_sym: &str, model: &StemModel, lambda: f64) -> f64 {
    let mut c = 0.02 * out_sym.chars().count() as f64;
    if let Some(path) = tags::decode_path(in_sym) {
        for (is_root, m) in path {
            if is_root {
                c += -model.log_p(m);
            } else {
                c += lambda;
            }
        }
    }
    c
}

/// Walks the network from its start state, best-first on ranking cost, constrained by `typed` then free to any accepting state.
/// Needs its own frontier-bytes memory dimension beyond `max_steps`/`max_completions`; see docs/research/predict-census-design-notes.md.
fn complete(
    net: &WalkNet,
    typed: &str,
    cfg: &WalkCfg,
    model: &StemModel,
    lambda: f64,
) -> WalkOutcome {
    let mut out = Vec::new();
    let mut steps = 0usize;
    let mut truncated: Option<WalkTruncation> = None;
    let mut unrenderable = 0usize;
    let mut seen: HashSet<(String, String)> = HashSet::new();

    let mut frontier = std::collections::BinaryHeap::new();
    let mut frontier_bytes = frame_bytes("", "");
    frontier.push(Frame {
        cost_milli: 0,
        state: net.start,
        surface: String::new(),
        analysis: String::new(),
        matched: 0,
        bytes: frontier_bytes,
    });

    'search: while let Some(f) = frontier.pop() {
        frontier_bytes -= f.bytes;
        steps += 1;
        if steps > cfg.max_steps {
            truncated = Some(WalkTruncation {
                dimension: WalkBudgetDimension::Steps,
                value: steps,
                limit: cfg.max_steps,
            });
            break;
        }
        let done_matching = f.matched == typed.len();
        if done_matching
            && net.final_state[f.state]
            && seen.insert((f.surface.clone(), f.analysis.clone()))
        {
            let morphemes = tags::decode_path(&f.analysis).unwrap_or_default();
            out.push(Completion {
                surface: f.surface.clone(),
                morphemes,
            });
            if out.len() >= cfg.max_completions {
                truncated = Some(WalkTruncation {
                    dimension: WalkBudgetDimension::Completions,
                    value: out.len(),
                    limit: cfg.max_completions,
                });
                break;
            }
        }
        if f.surface.len() > typed.len() + cfg.max_extra_bytes {
            continue;
        }
        for arc in &net.arcs[f.state] {
            let (Some(o), Some(i)) = (net.sym(arc.out), net.sym(arc.r#in)) else {
                unrenderable += 1;
                continue;
            };
            let next_matched = if f.matched == typed.len() {
                // Free-tail phase: every arc is admissible.
                f.matched
            } else {
                let remaining = &typed[f.matched..];
                if o.is_empty() {
                    f.matched
                } else if let Some(_) = remaining.strip_prefix(o) {
                    f.matched + o.len()
                } else if o.starts_with(remaining) {
                    // Prefix ends mid-symbol: the whole symbol is consumed and the prefix counts as fully matched.
                    typed.len()
                } else {
                    continue; // mismatch — prune this branch
                }
            };
            let mut surface = f.surface.clone();
            surface.push_str(o);
            let mut analysis = f.analysis.clone();
            analysis.push_str(i);
            let step_cost = (arc_cost(i, o, model, lambda) * 1000.0) as i64;
            let bytes = frame_bytes(&surface, &analysis);
            frontier_bytes += bytes;
            frontier.push(Frame {
                cost_milli: f.cost_milli + step_cost,
                state: arc.target as usize,
                surface,
                analysis,
                matched: next_matched,
                bytes,
            });
            // Checked one past the limit; see docs/research/predict-census-design-notes.md.
            if frontier_bytes > cfg.max_frontier_bytes {
                truncated = Some(WalkTruncation {
                    dimension: WalkBudgetDimension::FrontierBytes,
                    value: frontier_bytes,
                    limit: cfg.max_frontier_bytes,
                });
                break 'search;
            }
        }
    }

    WalkOutcome {
        completions: out,
        steps_used: steps,
        truncated,
        unrenderable_arcs: unrenderable,
    }
}

// --- ranking --------------------------------------------------------------------------------

/// Add-alpha smoothed log P(stem), from training-split root counts.
struct StemModel {
    counts: HashMap<MorphemeId, f64>,
    total: f64,
    vocab: f64,
    alpha: f64,
}

impl StemModel {
    /// Uniform model for the training pass, before any counts exist, so nothing from the held-out split leaks into the counts.
    fn uniform(vocab: usize) -> StemModel {
        StemModel {
            counts: HashMap::new(),
            total: 0.0,
            vocab: vocab.max(1) as f64,
            alpha: 0.5,
        }
    }

    fn log_p(&self, m: MorphemeId) -> f64 {
        let c = self.counts.get(&m).copied().unwrap_or(0.0);
        ((c + self.alpha) / (self.total + self.alpha * self.vocab)).ln()
    }
}

/// Collapses completions to distinct surface strings, ranked best-first; see docs/research/predict-census-design-notes.md.
fn rank(
    completions: Vec<Completion>,
    model: &StemModel,
    lambda: f64,
    total_stem_probability: bool,
) -> Vec<(String, f64, Vec<Vec<(bool, MorphemeId)>>)> {
    // Dedupes on the same candidate key production `propose_budgeted` uses; see docs/research/predict-census-design-notes.md.
    let mut by_surface: HashMap<String, Vec<Vec<(bool, MorphemeId)>>> = HashMap::new();
    let mut seen_cand: HashSet<(String, Vec<u32>, i32)> = HashSet::new();
    for c in completions {
        let novel = tags::to_candidates(&c.morphemes).into_iter().any(|cand| {
            let key: Vec<u32> = cand.morphemes.iter().map(|m| m.0).collect();
            seen_cand.insert((c.surface.clone(), key, cand.root_index))
        });
        if novel {
            by_surface.entry(c.surface).or_default().push(c.morphemes);
        }
    }
    let mut scored: Vec<(String, f64, Vec<Vec<(bool, MorphemeId)>>)> = by_surface
        .into_iter()
        .map(|(surface, paths)| {
            let terms: Vec<f64> = paths
                .iter()
                .map(|p| {
                    let stem = p.iter().find(|(is_root, _)| *is_root).map(|&(_, m)| m);
                    let base = stem
                        .map(|m| model.log_p(m))
                        .unwrap_or_else(|| model.log_p(MorphemeId(u32::MAX)));
                    base - lambda * (p.len().saturating_sub(1) as f64)
                })
                .collect();
            let max = terms.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            // `sum` marginalises over every path, rewarding multiplicity; `max` is the single best path. See docs/research/predict-census-design-notes.md.
            let score = if !max.is_finite() {
                f64::NEG_INFINITY
            } else if total_stem_probability {
                max + terms.iter().map(|t| (t - max).exp()).sum::<f64>().ln()
            } else {
                max
            };
            (surface, score, paths)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    scored
}

// --- confirm descent ------------------------------------------------------------------------

struct DescentStats {
    accepted: Vec<String>,
    confirms_run: usize,
    cache_hits: usize,
    exhausted: bool,
    /// Wall-clock spent strictly inside `confirm_all`, separate from ranking/bookkeeping cost.
    confirm_ms: f64,
}

/// Descends the ranked list paying confirm until `top_n` surfaces confirm; `neg_cache` skips proven-refuted surfaces.
#[allow(clippy::too_many_arguments)]
fn descend(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    ranked: &[(String, f64, Vec<Vec<(bool, MorphemeId)>>)],
    top_n: usize,
    max_confirms: usize,
    max_paths_per_surface: usize,
    neg_cache: &mut HashSet<String>,
) -> DescentStats {
    let mut accepted = Vec::new();
    let mut confirms_run = 0usize;
    let mut cache_hits = 0usize;
    let mut confirm_ms = 0.0f64;

    for (surface, _score, paths) in ranked {
        if accepted.len() >= top_n {
            return DescentStats {
                accepted,
                confirms_run,
                cache_hits,
                exhausted: false,
                confirm_ms,
            };
        }
        if neg_cache.contains(surface) {
            cache_hits += 1;
            continue;
        }
        let mut ok = false;
        let mut spent_here = 0usize;
        for path in paths {
            if confirms_run >= max_confirms {
                return DescentStats {
                    accepted,
                    confirms_run,
                    cache_hits,
                    exhausted: true,
                    confirm_ms,
                };
            }
            // Per-surface cap, so one analysis-ambiguous surface can never eat the whole confirm budget; see docs/research/predict-census-design-notes.md.
            if spent_here >= max_paths_per_surface {
                break;
            }
            spent_here += 1;
            for cand in tags::to_candidates(path) {
                confirms_run += 1;
                let tc = Instant::now();
                let confirmed = !confirm_all(g, owners, morpher, &cand, surface).is_empty();
                confirm_ms += tc.elapsed().as_secs_f64() * 1000.0;
                if confirmed {
                    ok = true;
                    break;
                }
            }
            if ok {
                break;
            }
        }
        if ok {
            accepted.push(surface.clone());
        } else if spent_here < max_paths_per_surface {
            // Only caches a proven refutation, never a surface merely abandoned at the cap; see docs/research/predict-census-design-notes.md.
            neg_cache.insert(surface.clone());
        }
    }
    DescentStats {
        accepted,
        confirms_run,
        cache_hits,
        exhausted: false,
        confirm_ms,
    }
}

// --- driver ---------------------------------------------------------------------------------

struct Cfg {
    grammars: Vec<String>,
    max_words: usize,
    prefix_lens: Vec<usize>,
    top_n: usize,
    max_completions: usize,
    max_steps: usize,
    max_extra_bytes: usize,
    /// [`WalkCfg::max_frontier_bytes`]; see this module's memory-budgets section.
    max_frontier_bytes: usize,
    /// [`WalkNet::build`]'s `max_states` refusal cap.
    max_states: usize,
    /// [`WalkNet::build`]'s `max_sigma` refusal cap.
    max_sigma: usize,
    max_confirms: usize,
    max_paths_per_surface: usize,
    lambda: f64,
    /// Score by the sum of stem probability over all paths (true) or by the single best path (false).
    total_stem_probability: bool,
}

fn main() {
    let mut cfg = Cfg {
        grammars: vec!["sena".into(), "indonesian".into()],
        max_words: 60,
        prefix_lens: vec![2, 4, 6],
        top_n: 3,
        max_completions: 200,
        max_steps: 200_000,
        max_extra_bytes: 24,
        // Env-overridable, CLI flag takes final precedence; see docs/research/predict-census-design-notes.md.
        max_frontier_bytes: env_usize(
            "PREDICT_CENSUS_MAX_FRONTIER_BYTES",
            DEFAULT_MAX_FRONTIER_BYTES,
        ),
        max_states: env_usize("PREDICT_CENSUS_MAX_STATES", DEFAULT_MAX_STATES),
        max_sigma: env_usize("PREDICT_CENSUS_MAX_SIGMA", DEFAULT_MAX_SIGMA),
        // Deliberately small research ceiling; see docs/research/predict-census-design-notes.md.
        max_confirms: 25,
        max_paths_per_surface: 4,
        lambda: 0.5,
        total_stem_probability: true,
    };
    let args: Vec<String> = std::env::args().collect();
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--grammars" => {
                cfg.grammars = it.next().unwrap().split(',').map(str::to_string).collect()
            }
            "--max-words" => cfg.max_words = it.next().unwrap().parse().unwrap(),
            "--prefix-lens" => {
                cfg.prefix_lens = it
                    .next()
                    .unwrap()
                    .split(',')
                    .map(|s| s.parse().unwrap())
                    .collect()
            }
            "--top-n" => cfg.top_n = it.next().unwrap().parse().unwrap(),
            "--max-completions" => cfg.max_completions = it.next().unwrap().parse().unwrap(),
            "--max-steps" => cfg.max_steps = it.next().unwrap().parse().unwrap(),
            "--max-extra-bytes" => cfg.max_extra_bytes = it.next().unwrap().parse().unwrap(),
            "--max-frontier-bytes" => cfg.max_frontier_bytes = it.next().unwrap().parse().unwrap(),
            "--max-states" => cfg.max_states = it.next().unwrap().parse().unwrap(),
            "--max-sigma" => cfg.max_sigma = it.next().unwrap().parse().unwrap(),
            "--max-confirms" => cfg.max_confirms = it.next().unwrap().parse().unwrap(),
            "--max-paths-per-surface" => {
                cfg.max_paths_per_surface = it.next().unwrap().parse().unwrap()
            }
            "--score" => {
                cfg.total_stem_probability = match it.next().unwrap().as_str() {
                    "sum" => true,
                    "max" => false,
                    other => panic!("--score must be sum|max, got {other}"),
                }
            }
            other => panic!("unknown flag {other}"),
        }
    }

    for (name, gfile, wfile) in GRAMMARS {
        if !cfg.grammars.iter().any(|g| g == name) {
            continue;
        }
        run_grammar(name, gfile, wfile, &cfg);
    }
}

fn run_grammar(name: &str, gfile: &str, wfile: &str, cfg: &Cfg) {
    let gpath = sample_path(gfile);
    let wpath = sample_path(wfile);
    if !gpath.exists() || !wpath.exists() {
        println!("{name}: SKIPPED (missing fixture: {gfile} / {wfile})");
        return;
    }
    println!("\n=== {name} ===");
    let t0 = Instant::now();
    let g = load_grammar(&gpath);
    let words: Vec<String> = std::fs::read_to_string(&wpath)
        .expect("read wordlist")
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(|w| pg_grammar::nfd::nfd(w))
        .collect();
    println!(
        "grammar loaded in {:.1}s; {} wordforms",
        t0.elapsed().as_secs_f64(),
        words.len()
    );

    let t1 = Instant::now();
    let net = match WalkNet::build(&g, cfg.max_states, cfg.max_sigma) {
        Ok(net) => net,
        Err(e) => {
            println!("{name}: SKIPPED ({e})");
            return;
        }
    };
    println!(
        "network built in {:.1}s; {} states, {} sigma symbols",
        t1.elapsed().as_secs_f64(),
        net.arcs.len(),
        net.sigma.iter().filter(|s| s.is_some()).count()
    );

    // Deterministic 80/20 split; training feeds the stem model only, measured prefixes come from the held-out fifth.
    let split = words.len() * 4 / 5;
    let (train, held) = words.split_at(split);

    let owners = build_morpheme_owners(&g);
    let morpher = Morpher::new(&g, usize::MAX);

    // Stem counts from the training split: a word's confirmed analyses vote for their root morpheme.
    let t2 = Instant::now();
    let mut counts: HashMap<MorphemeId, f64> = HashMap::new();
    let mut trained = 0usize;
    let uniform = StemModel::uniform(g.morphemes.len());
    for w in train.iter().take(cfg.max_words * 4) {
        let outcome = complete(
            &net,
            w,
            &WalkCfg {
                max_completions: cfg.max_completions,
                max_steps: cfg.max_steps,
                max_extra_bytes: 0,
                max_frontier_bytes: cfg.max_frontier_bytes,
            },
            &uniform,
            cfg.lambda,
        );
        for c in &outcome.completions {
            if c.surface != *w {
                continue;
            }
            for cand in tags::to_candidates(&c.morphemes) {
                if !confirm_all(&g, &owners, &morpher, &cand, w).is_empty() {
                    if let Some(&(_, m)) = c.morphemes.iter().find(|(is_root, _)| *is_root) {
                        *counts.entry(m).or_insert(0.0) += 1.0;
                    }
                    trained += 1;
                    break;
                }
            }
        }
    }
    let total: f64 = counts.values().sum();
    let model = StemModel {
        counts,
        total,
        vocab: g.morphemes.len().max(1) as f64,
        alpha: 0.5,
    };
    println!(
        "stem model: {} distinct stems from {} confirmed training analyses in {:.1}s",
        model.counts.len(),
        trained,
        t2.elapsed().as_secs_f64()
    );

    // --- instrument self-check: see docs/research/predict-census-design-notes.md ------------
    {
        let mut proposer = pg_foma::analyzer::FomaProposer::new(&g).expect("proposer");
        let (mut prod_ok, mut walk_ok, mut n_check) = (0usize, 0usize, 0usize);
        for w in held.iter().take(30) {
            n_check += 1;
            if proposer
                .propose(w)
                .iter()
                .any(|c| !confirm_all(&g, &owners, &morpher, c, w).is_empty())
            {
                prod_ok += 1;
            }
            let outcome = complete(
                &net,
                w,
                &WalkCfg {
                    max_completions: cfg.max_completions,
                    max_steps: cfg.max_steps,
                    max_extra_bytes: 0,
                    max_frontier_bytes: cfg.max_frontier_bytes,
                },
                &model,
                cfg.lambda,
            );
            let confirmed = outcome
                .completions
                .iter()
                .filter(|c| c.surface == *w)
                .flat_map(|c| tags::to_candidates(&c.morphemes))
                .any(|cand| !confirm_all(&g, &owners, &morpher, &cand, w).is_empty());
            if confirmed {
                walk_ok += 1;
            }
        }
        println!(
            "self-check on {n_check} held-out words: production propose+confirm {prod_ok}, this walk+confirm {walk_ok}{}",
            if prod_ok == walk_ok { "  [agree]" } else { "  [DISAGREE - numbers below are suspect]" }
        );
    }

    // --- the achievable denominator: see docs/research/predict-census-design-notes.md --------
    let mut confirmable: HashSet<String> = HashSet::new();
    {
        let mut proposer = pg_foma::analyzer::FomaProposer::new(&g).expect("proposer");
        for w in held.iter().take(cfg.max_words) {
            if proposer
                .propose(w)
                .iter()
                .any(|c| !confirm_all(&g, &owners, &morpher, c, w).is_empty())
            {
                confirmable.insert(w.clone());
            }
        }
        println!(
            "achievable denominator: {} of {} held-out words analyse at all ({:.1}% grammar coverage)",
            confirmable.len(),
            held.iter().take(cfg.max_words).count(),
            100.0 * confirmable.len() as f64 / held.iter().take(cfg.max_words).count().max(1) as f64
        );
    }

    // --- the measurement -------------------------------------------------------------------
    let mut neg_cache: HashSet<String> = HashSet::new();
    for &k in &cfg.prefix_lens {
        let mut n = 0usize;
        let mut n_achievable = 0usize;
        let mut contained_achievable = 0usize;
        let mut hit_at_n_achievable = 0usize;
        let mut contained = 0usize;
        let mut truncations = 0usize;
        // Tally of which budget dimension tripped, so the report always names the cap that fired; see docs/research/predict-census-design-notes.md.
        let mut truncation_dims: HashMap<&'static str, TruncationTally> = HashMap::new();
        let mut completions_total = 0usize;
        let mut rank_of_true: Vec<usize> = Vec::new();
        let mut hit_at_n = 0usize;
        let mut confirms: Vec<usize> = Vec::new();
        let mut cache_hits_total = 0usize;
        let mut walk_ms: Vec<f64> = Vec::new();
        let mut descent_ms: Vec<f64> = Vec::new();
        let mut per_confirm_ms: Vec<f64> = Vec::new();
        let mut steps_total = 0usize;
        let mut unrenderable_total = 0usize;
        let mut exhausted_total = 0usize;

        for w in held.iter().take(cfg.max_words) {
            // Truncate on a char boundary; skip words too short to have a free tail.
            let Some((byte_len, _)) = w.char_indices().nth(k) else {
                continue;
            };
            let typed = &w[..byte_len];
            n += 1;

            let tw = Instant::now();
            let outcome = complete(
                &net,
                typed,
                &WalkCfg {
                    max_completions: cfg.max_completions,
                    max_steps: cfg.max_steps,
                    max_extra_bytes: cfg.max_extra_bytes,
                    max_frontier_bytes: cfg.max_frontier_bytes,
                },
                &model,
                cfg.lambda,
            );
            walk_ms.push(tw.elapsed().as_secs_f64() * 1000.0);
            if let Some(t) = outcome.truncated {
                truncations += 1;
                truncation_dims
                    .entry(t.dimension.label())
                    .or_insert(TruncationTally {
                        count: 0,
                        peak_value: 0,
                        limit: t.limit,
                    })
                    .record(t.value, t.limit);
            }
            steps_total += outcome.steps_used;
            unrenderable_total += outcome.unrenderable_arcs;
            completions_total += outcome.completions.len();
            let is_achievable = confirmable.contains(w);
            if is_achievable {
                n_achievable += 1;
            }
            if outcome.completions.iter().any(|c| c.surface == *w) {
                contained += 1;
                if is_achievable {
                    contained_achievable += 1;
                }
            }

            let ranked = rank(
                outcome.completions,
                &model,
                cfg.lambda,
                cfg.total_stem_probability,
            );
            if let Some(pos) = ranked.iter().position(|(s, _, _)| s == w) {
                rank_of_true.push(pos + 1);
            }

            let td = Instant::now();
            let stats = descend(
                &g,
                &owners,
                &morpher,
                &ranked,
                cfg.top_n,
                cfg.max_confirms,
                cfg.max_paths_per_surface,
                &mut neg_cache,
            );
            descent_ms.push(td.elapsed().as_secs_f64() * 1000.0);
            confirms.push(stats.confirms_run);
            cache_hits_total += stats.cache_hits;
            if stats.exhausted {
                exhausted_total += 1;
            }
            if stats.confirms_run > 0 {
                per_confirm_ms.push(stats.confirm_ms / stats.confirms_run as f64);
            }
            if stats.accepted.iter().any(|s| s == w) {
                hit_at_n += 1;
                if is_achievable {
                    hit_at_n_achievable += 1;
                }
            }
        }

        if n == 0 {
            println!("prefix {k}: no eligible words");
            continue;
        }
        let pct = |x: usize| 100.0 * x as f64 / n as f64;
        println!("\n-- prefix length {k} ({n} held-out words) --");
        let pct_a = |x: usize| 100.0 * x as f64 / n_achievable.max(1) as f64;
        println!(
            "  containment, all held-out words:           {:.1}%  [walk truncated on {} of {}]",
            pct(contained),
            truncations,
            n
        );
        println!(
            "  CONTAINMENT, analysable words only:        {:.1}%  ({} of {})",
            pct_a(contained_achievable),
            contained_achievable,
            n_achievable
        );
        println!(
            "  ACCEPTED in top-{}, analysable words only:  {:.1}%  ({} of {})",
            cfg.top_n,
            pct_a(hit_at_n_achievable),
            hit_at_n_achievable,
            n_achievable
        );
        println!(
            "  mean completions per keystroke:            {:.0}",
            completions_total as f64 / n as f64
        );
        println!(
            "  rank of true word: {}",
            summarize(&rank_of_true.iter().map(|&r| r as f64).collect::<Vec<_>>())
        );
        println!(
            "  accepted in confirmed top-{}:              {:.1}%",
            cfg.top_n,
            pct(hit_at_n)
        );
        println!(
            "  confirms paid per keystroke: {}",
            summarize(&confirms.iter().map(|&c| c as f64).collect::<Vec<_>>())
        );
        println!(
            "  negative-cache hits (confirms saved):      {} total, {:.1} per keystroke",
            cache_hits_total,
            cache_hits_total as f64 / n as f64
        );
        println!(
            "  negative cache size after this pass:       {}",
            neg_cache.len()
        );
        println!("  walk ms: {}", summarize(&walk_ms));
        println!("  confirm-descent ms: {}", summarize(&descent_ms));
        println!("  PER-CONFIRM ms: {}", summarize(&per_confirm_ms));
        println!(
            "  budget: {:.0} mean walk steps, {} confirm-budget exhaustions, {} unrenderable arcs skipped",
            steps_total as f64 / n as f64,
            exhausted_total,
            unrenderable_total
        );
        if !truncation_dims.is_empty() {
            // Reports the peak observed value beside the count, not just the count; see docs/research/predict-census-design-notes.md.
            let mut dims: Vec<(&str, TruncationTally)> = truncation_dims.into_iter().collect();
            dims.sort_by(|a, b| b.1.count.cmp(&a.1.count).then_with(|| a.0.cmp(b.0)));
            let breakdown = dims
                .iter()
                .map(|(d, t)| {
                    format!(
                        "{d}={} (peak {} vs limit {})",
                        t.count, t.peak_value, t.limit
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            println!("  truncation dimension breakdown: {breakdown}");
        }
    }
}

fn summarize(v: &[f64]) -> String {
    if v.is_empty() {
        return "n/a (no samples)".to_string();
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = s.iter().sum::<f64>() / s.len() as f64;
    let p = |q: f64| s[((s.len() as f64 - 1.0) * q).round() as usize];
    format!(
        "mean {:.1} / median {:.1} / p90 {:.1} / max {:.1} (n={})",
        mean,
        p(0.5),
        p(0.9),
        s[s.len() - 1],
        s.len()
    )
}

// --- memory-budget regression tests; `test = true` is set for this example in Cargo.toml. See docs/research/predict-census-design-notes.md. ---
#[cfg(test)]
mod memory_budget_tests {
    use super::*;

    #[test]
    fn state_budget_check_passes_under_the_cap() {
        check_network_size("compiled states", 4, 4).expect("value == limit must fit");
    }

    #[test]
    fn state_budget_check_trips_over_the_cap() {
        let err = check_network_size("compiled states", 5, 4).expect_err("5 > 4 must trip");
        match err {
            CensusError::NetworkTooLarge {
                dimension,
                value,
                limit,
            } => {
                assert_eq!(dimension, "compiled states");
                assert_eq!(value, 5);
                assert_eq!(limit, 4);
            }
        }
    }

    #[test]
    fn sigma_budget_check_trips_over_the_cap() {
        let err =
            check_network_size("sigma symbol numbers", 200_001, 200_000).expect_err("must trip");
        assert!(matches!(
            err,
            CensusError::NetworkTooLarge {
                dimension: "sigma symbol numbers",
                value: 200_001,
                limit: 200_000
            }
        ));
    }

    /// Never-final self-looping net that isolates the frontier-bytes budget from every other dimension; see docs/research/predict-census-design-notes.md.
    fn tiny_cyclic_net(branching: usize) -> WalkNet {
        WalkNet {
            arcs: vec![vec![
                CsrArc {
                    r#in: 3,
                    out: 3,
                    target: 0
                };
                branching
            ]],
            final_state: vec![false],
            start: 0,
            sigma: vec![Some(String::new()), None, None, Some("a".to_string())],
        }
    }

    #[test]
    fn frontier_bytes_budget_trips_on_a_branchy_cyclic_net() {
        let net = tiny_cyclic_net(8);
        let model = StemModel::uniform(1);
        let cfg = WalkCfg {
            max_completions: usize::MAX,
            // Bounded, not `usize::MAX`, so a missing fix fails this test cleanly rather than hanging; see docs/research/predict-census-design-notes.md.
            max_steps: 1_000,
            max_extra_bytes: 1_000_000,
            max_frontier_bytes: 2_000,
        };
        let outcome = complete(&net, "", &cfg, &model, 0.5);
        match outcome.truncated {
            Some(WalkTruncation {
                dimension: WalkBudgetDimension::FrontierBytes,
                limit,
                ..
            }) => {
                assert_eq!(limit, 2_000);
            }
            other => panic!("expected a FrontierBytes truncation, got {other:?}"),
        }
    }

    #[test]
    fn frontier_bytes_budget_does_not_trip_when_generous() {
        let net = tiny_cyclic_net(8);
        let model = StemModel::uniform(1);
        let cfg = WalkCfg {
            max_completions: usize::MAX,
            // Small and finite: this walk never reaches a final state, so some cap must stop it, just not this one.
            max_steps: 50,
            max_extra_bytes: 1_000_000,
            max_frontier_bytes: usize::MAX,
        };
        let outcome = complete(&net, "", &cfg, &model, 0.5);
        assert!(
            !matches!(
                outcome.truncated,
                Some(WalkTruncation {
                    dimension: WalkBudgetDimension::FrontierBytes,
                    ..
                })
            ),
            "a usize::MAX frontier-byte cap must never be the dimension that trips, got {:?}",
            outcome.truncated
        );
    }
}
