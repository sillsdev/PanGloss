//! Phase 0 dead-end attribution census for the "better proposing FST" plan
//! (`docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md`, "Phase 0 — dead-end
//! attribution census"). The 2026-07-17 pre-filter census (`prefilter_census.rs`, merged at
//! `571b8a3`) proved 91-98% of FAILING-candidate confirm time is spent on candidates whose
//! restricted reparse produces ZERO derivations at all ("cascade dead-ends") on every grammar, at
//! every sample size. This harness attributes WHY those cascades die, per grammar, weighted by wall
//! time, so the plan's per-encoding go/no-go bars (Phase 0: "≥20% of failing time on some grammar
//! AND projected end-to-end win ≥15% of that grammar's confirm") can be evaluated against real
//! numbers instead of a guess.
//!
//! ## The six buckets (plan's own taxonomy)
//! - d1: allomorph environment check failed against the intermediate shape
//! - d2: disjunctive-allomorph block (first-match-wins picked a different allomorph than the
//!   candidate's segmentation needs)
//! - d3: feature-structure unification/subsumption/MPR clash between the pinned morphemes
//! - d4: shape mismatch — no rule sequence reproduces the surface (expected to dominate Amharic:
//!   gemination/epenthesis)
//! - d5: ordering/slot violation — stratum or template order excludes the pinned rule sequence
//! - d6: other/unattributable (this harness reports the raw reasons composing it, so a large d6 can
//!   be split further later)
//!
//! ## Why this needed NEW instrumentation (read before trusting the numbers)
//! The previous census's `confirm_one_traced`/`parse_word_selected_traced` path traced only the
//! SYNTHESIS half of the pipeline (`pg_rules::stratum::synthesize_stratum_traced` and the final
//! `is_word_valid_traced`/`is_match_traced` gates) — confirmed by grep before writing a line of new
//! code: `pg_rules::stratum::StratumAnalyzer`, which runs the ANALYSIS (unapply) cascade every
//! restricted reparse starts from, had ZERO callers of `TraceSink::morphological_rule_unapplied`/
//! `_not_unapplied`/`phonological_rule_unapplied`/`_not_unapplied` anywhere in the codebase before
//! this commit (`rewrite::analyze_cached_traced` existed, built, but was explicitly documented as
//! "not yet wired into the live per-word pipeline"). Since the prior census found validity-gate
//! (post-synthesis) rejections are only 0-3% of failing time on every grammar, the dominant 91-98%
//! dead-end mass necessarily dies DURING analysis or synthesis apply-time checks that run before a
//! complete candidate word ever exists — exactly the region that was a black box. This harness's
//! only source changes (beyond this file) wire that existing-but-unused machinery up
//! (`pg-rules/src/stratum.rs`, `pg-rules/src/morph.rs`, `pg-rules/src/trace.rs`) as ADDITIVE,
//! `is_tracing()`-gated code — see those files' own 2026-07-17 doc comments for the fast-path
//! argument that production behavior/cost is unchanged (`NoopSink::is_tracing()` is `false` on
//! every existing `parse_word*`/`confirm_batch`/`confirm_all` call path).
//!
//! ## Frontier definition (the thing that actually decides the numbers)
//! For each FAILING candidate, ONE real [`TreeTraceSink`] captures both the analysis and synthesis
//! cascades in a single tree (now that analysis is wired). We walk it once from the root, computing
//! for every node a `depth` = the number of ANCESTOR nodes that represent a SUCCESSFUL rule
//! (un)application step (`MorphologicalRuleAnalysis`/`MorphologicalRuleSynthesis`/
//! `PhonologicalRuleAnalysis`/`PhonologicalRuleSynthesis`/`CompoundingRuleAnalysis`/
//! `CompoundingRuleSynthesis`, distinguished from a FAILED attempt of the same `TraceType` by
//! `node.output.is_some()` — see [`is_success_step`]). Every node that represents a DEAD attempt (a
//! "not applied"/"not unapplied" event, or a terminal `Failed` node — see [`is_frontier`]) is a
//! frontier candidate; among ALL of them in the tree, the one with the GREATEST depth is "the
//! attempt that got furthest" — ties broken by a fixed pipeline-stage ordinal (analysis < synthesis
//! < final validity/match gate), preferring the LATER stage (see [`stage_ordinal`]).
//!
//! This is a coarse but fully-defined proxy for "how many of the pinned rules' (un)applications
//! this branch actually got through before dying" — it does not reconstruct the exact pinned-rule-
//! sequence position; bookend/stratum nodes interposed between rule-application nodes are not
//! counted either way, so branches that pass through more or fewer bookends at equal rule-progress
//! are still ranked correctly relative to each other, which is all the "deepest frontier" comparison
//! needs.
//!
//! **Two known, documented coarsenings** (not hidden — their materiality is reported per grammar):
//! 1. The phonological-rule UNAPPLY loop (`StratumAnalyzer::analyze`'s prule loop) does not advance
//!    the trace cursor between successive prules within one stratum (every prule attempt in that
//!    loop is a sibling under one parent — see `pg-rules/src/stratum.rs`'s own 2026-07-17 doc note).
//!    A dead end after N successful prule unapplications and a dead end after 1 are NOT
//!    distinguished by depth; only "died during the prule loop at all" vs "died later, having
//!    escaped the prule loop" is visible. This under-counts depth for Amharic-shaped multi-prule
//!    chains specifically; it would only distort d4-internal ranking (which specific prule chain won
//!    the "deepest" tie), never which bucket (d1-d6) a candidate lands in.
//! 2. `pg-parse::Morpher::lexical_lookup_filtered` (the boundary between the analysis cascade's
//!    output and the synthesis cascade's input) has no trace event of its own. A candidate whose
//!    analysis cascade fully and successfully unapplies every pinned rule (no frontier node anywhere
//!    below it) but whose resulting shape still does not match any stored allomorph of the pinned
//!    root produces a trace tree with NO frontier node at all — reported as its own outcome,
//!    [`Outcome::LexLookupBoundary`], folded into d4 (a lexicon-shape miss is the same KIND of
//!    failure as a rule-shape miss) but counted and printed on its own line so this approximation's
//!    size is visible rather than asserted away.
//!
//! ## Time attribution (unchanged method from `prefilter_census.rs`)
//! Counts alone are not the deliverable — TIME share is, measured as a counterfactual under the
//! REAL batched `pg_foma::confirm::confirm_batch` (naive per-candidate unbatched sums are
//! untrustworthy near a go/no-go gate; validated in the prior census). Per word:
//!   - `baseline`       = `confirm_batch(all candidates)`
//!   - `keep_confirming` = `confirm_batch(only candidates that end up confirming)`
//!   - `minus_dN`       = `confirm_batch(all candidates EXCEPT the ones classified dN)`, N in 1..=6
//!   - class dN's share of FAILING time = `(baseline - minus_dN) / (baseline - keep_confirming)`
//!     CLASSIFICATION is a separate, untimed pass using [`classify_failing_candidate`]; the six timed
//!     `confirm_batch` calls per word (plus baseline/keep_confirming) never touch tracing.
//!
//! ## Usage
//!   cargo run -p pg-foma --release --example deadend_census -- <sena|indonesian|amharic> [word_cap]
//! Env overrides: `CENSUS_SENA_CAP`, `CENSUS_INDONESIAN_CAP`, `CENSUS_AMHARIC_CAP` (see `run()`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use foma::apply::apply_init;
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_foma::compose_budget::ComposeBudget;
use pg_foma::confirm::{self, MorphemeOwner};
use pg_foma::emit;
use pg_foma::peel::ReduplicationPeeler;
use pg_foma::tags::{self, Candidate};
use pg_grammar::model::{Grammar, MorphemeId};
use pg_parse::Morpher;
use pg_rules::trace::{FailureReason, TraceNode, TraceType, TreeTraceSink};
use rustc_hash::FxHashMap;

const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

// -------------------------------------------------------------------------------------------
// Sample loading + propose/peel plumbing — copied verbatim from `examples/prefilter_census.rs`
// (see that file's own doc for the property-level justification of each step; examples cannot
// depend on each other's code, only duplicate it, same relationship that file has to
// `precision_bench`/`tests/pk1_precision_recall_invariance.rs`).
// -------------------------------------------------------------------------------------------

fn sample_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../samples/data")
        .join(name)
}

fn load_grammar(name: &str) -> Option<Grammar> {
    let path = sample_path(name);
    let xml = std::fs::read_to_string(&path).ok()?;
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {name}: {e}")))
}

fn read_words(name: &str) -> Option<Vec<String>> {
    let path = sample_path(name);
    let text = std::fs::read_to_string(&path).ok()?;
    Some(
        text.lines()
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// The pinned confirm-cost outliers for a grammar, keyed off its `words_file` (e.g.
/// `sena-words.txt` -> `sena-worst-words.txt`, gitignored, generated by `examples/worst_words.rs`;
/// see `.claude/skills/dead-end-census`). Absent file => empty, never an error — the census still
/// runs on the `take(cap)` slice. `#`-prefixed lines are provenance comments; skipped.
fn read_pinned(words_file: &str) -> Vec<String> {
    let base = words_file.strip_suffix("-words.txt").unwrap_or(words_file);
    let path = sample_path(&format!("{base}-worst-words.txt"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|w| !w.is_empty() && !w.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn propose(net: &Fsm, word: &str) -> Vec<Candidate> {
    let normalized = pg_grammar::nfd::nfd(word);
    let mut handle = apply_init(net);
    let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
    let mut out = Vec::new();
    for s in handle.up(&normalized) {
        let Some(path) = tags::decode_path(&s) else {
            continue;
        };
        for c in tags::to_candidates(&path) {
            let key: (Vec<u32>, i32) = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
            if seen.insert(key) {
                out.push(c);
            }
        }
    }
    out
}

fn propose_and_peel(
    net: &Fsm,
    g: &Grammar,
    peeler: &ReduplicationPeeler,
    word: &str,
) -> Vec<Candidate> {
    let mut candidates = propose(net, word);
    // This harness's own words are not adversarial (real corpus/worst-word lists, never a
    // synthetic self-similar stress string) -- an unbounded chain-depth budget is safe here the
    // same way `crate::peel`'s own module doc reasons about it for a genuine single-layer
    // reference-grammar redup. See `pg_foma::peel`'s module doc ("Chain depth and nested
    // reduplication") for the ADR 0003 budget this now threads through.
    let budget = ComposeBudget::from_env();
    let peeled = peeler
        .peel_candidates(g, word, &budget, &mut |r: &str| propose(net, r))
        .unwrap_or_else(|e| {
            eprintln!("[deadend_census] reduplication peel refused for {word:?}: {e}");
            Vec::new()
        });
    for c in peeled {
        let already = candidates.iter().any(|existing| {
            existing.root_index == c.root_index && existing.morphemes == c.morphemes
        });
        if !already {
            candidates.push(c);
        }
    }
    candidates
}

// -------------------------------------------------------------------------------------------
// d1-d6 classification.
// -------------------------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum DClass {
    D1Environment,
    D2Disjunctive,
    D3FeatureClash,
    D4Shape,
    D5Ordering,
    D6Other,
}

const D_CLASSES: [DClass; 6] = [
    DClass::D1Environment,
    DClass::D2Disjunctive,
    DClass::D3FeatureClash,
    DClass::D4Shape,
    DClass::D5Ordering,
    DClass::D6Other,
];

impl DClass {
    fn idx(self) -> usize {
        match self {
            DClass::D1Environment => 0,
            DClass::D2Disjunctive => 1,
            DClass::D3FeatureClash => 2,
            DClass::D4Shape => 3,
            DClass::D5Ordering => 4,
            DClass::D6Other => 5,
        }
    }
    fn label(self) -> &'static str {
        match self {
            DClass::D1Environment => "d1 (environment)",
            DClass::D2Disjunctive => "d2 (disjunctive block)",
            DClass::D3FeatureClash => "d3 (feature clash)",
            DClass::D4Shape => "d4 (shape mismatch)",
            DClass::D5Ordering => "d5 (ordering/slot)",
            DClass::D6Other => "d6 (other/unattributable)",
        }
    }
}

/// Maps every real `FailureReason` this codebase emits to one of the plan's six buckets. Documented
/// once, here, rather than per call site — see this file's module doc for the frontier/reason
/// contract this classification is applied to.
fn classify_reason(r: FailureReason) -> DClass {
    use FailureReason::*;
    match r {
        Environments => DClass::D1Environment,
        DisjunctiveAllomorph => DClass::D2Disjunctive,
        RequiredSyntacticFeatureStruct
        | HeadRequiredSyntacticFeatureStruct
        | NonHeadRequiredSyntacticFeatureStruct
        | RequiredMprFeatures
        | ExcludedMprFeatures
        | HeadProdRestrictMprFeatures
        | NonHeadProdRestrictMprFeatures
        | ObligatorySyntacticFeatures => DClass::D3FeatureClash,
        Pattern | HeadPattern | NonHeadPattern | SurfaceFormMismatch => DClass::D4Shape,
        NonPartialRuleProhibitedAfterFinalTemplate
        | NonPartialRuleRequiredAfterNonFinalTemplate
        | PartialParse
        | MaxApplicationCount => DClass::D5Ordering,
        // Real HC gates that don't map cleanly onto d1-d5's mechanisms — reported here, with the
        // raw reason kept visible in the per-class histogram so this bucket can be split later if
        // it turns out to matter (mission's own instruction: report d6's contents if >10%).
        AllomorphCoOccurrenceRules
        | MorphemeCoOccurrenceRules
        | RequiredStemName
        | ExcludedStemName
        | BoundRoot => DClass::D6Other,
    }
}

// -------------------------------------------------------------------------------------------
// ADDITIVE, throwaway: sample real d5 candidates for manual superset attribution (dead-end-census
// task step 4/5). Gated on `CENSUS_DUMP_D5=1` so default census output/behavior is byte-identical
// otherwise. Global budget (`CENSUS_DUMP_D5_MAX`, default 30) across ALL grammars in one run, so a
// multi-grammar invocation doesn't spam one grammar's worth alone.
// -------------------------------------------------------------------------------------------

static D5_DUMP_BUDGET: AtomicUsize = AtomicUsize::new(usize::MAX);

fn dump_d5_enabled() -> bool {
    std::env::var("CENSUS_DUMP_D5").as_deref() == Ok("1")
}

fn init_dump_budget() {
    let max = env_usize("CENSUS_DUMP_D5_MAX", 30);
    D5_DUMP_BUDGET.store(max, Ordering::Relaxed);
}

fn take_dump_slot() -> bool {
    loop {
        let cur = D5_DUMP_BUDGET.load(Ordering::Relaxed);
        if cur == 0 {
            return false;
        }
        if D5_DUMP_BUDGET
            .compare_exchange(cur, cur - 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return true;
        }
    }
}

/// Human-readable description of one morpheme for the d5 sample dump: its owner kind
/// (`LexEntry`=root / `MRule`=affix), display name (gloss, falling back to `morph_id` then
/// `xml_key`), and — for an `MRule` — whether it is a template-slot rule (`is_template_rule`),
/// since that is exactly the fact superset attribution (module doc items 1-3) needs to check
/// against.
fn describe_morpheme(g: &Grammar, owners: &[Option<MorphemeOwner>], m: MorphemeId) -> String {
    let info = &g.morphemes[m.0 as usize];
    let name = info
        .gloss
        .as_deref()
        .or(info.morph_id.as_deref())
        .unwrap_or(info.xml_key.as_str());
    match owners.get(m.0 as usize).copied().flatten() {
        Some(MorphemeOwner::LexEntry(id)) => {
            format!("ROOT[{name}#{}]", id.0)
        }
        Some(MorphemeOwner::MRule(id)) => {
            let tmpl = match &g.mrules[id.0 as usize] {
                pg_grammar::model::MorphRuleDef::AffixProcess(def) if def.is_template_rule => {
                    "tmpl-rule"
                }
                pg_grammar::model::MorphRuleDef::AffixProcess(_) => "standalone-rule",
                pg_grammar::model::MorphRuleDef::Realizational(_) => "realizational-rule",
                pg_grammar::model::MorphRuleDef::Compounding(_) => "compounding-rule",
            };
            format!("AFX[{name}#{}/{tmpl}]", id.0)
        }
        None => format!("UNOWNED[{name}]"),
    }
}

fn dump_d5_sample(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    word: &str,
    candidate: &Candidate,
    reason: FailureReason,
) {
    if !dump_d5_enabled() || !take_dump_slot() {
        return;
    }
    let parts: Vec<String> = candidate
        .morphemes
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let base = describe_morpheme(g, owners, *m);
            if i as i32 == candidate.root_index {
                format!("*{base}*")
            } else {
                base
            }
        })
        .collect();
    println!(
        "  [D5-SAMPLE] word={word:?} reason={} seq=[{}]",
        reason_name(reason),
        parts.join(" | ")
    );
}

fn reason_name(r: FailureReason) -> &'static str {
    use FailureReason::*;
    match r {
        ObligatorySyntacticFeatures => "ObligatorySyntacticFeatures",
        AllomorphCoOccurrenceRules => "AllomorphCoOccurrenceRules",
        Environments => "Environments",
        MorphemeCoOccurrenceRules => "MorphemeCoOccurrenceRules",
        DisjunctiveAllomorph => "DisjunctiveAllomorph",
        SurfaceFormMismatch => "SurfaceFormMismatch",
        Pattern => "Pattern",
        HeadPattern => "HeadPattern",
        NonHeadPattern => "NonHeadPattern",
        RequiredSyntacticFeatureStruct => "RequiredSyntacticFeatureStruct",
        HeadRequiredSyntacticFeatureStruct => "HeadRequiredSyntacticFeatureStruct",
        NonHeadRequiredSyntacticFeatureStruct => "NonHeadRequiredSyntacticFeatureStruct",
        HeadProdRestrictMprFeatures => "HeadProdRestrictMprFeatures",
        NonHeadProdRestrictMprFeatures => "NonHeadProdRestrictMprFeatures",
        RequiredMprFeatures => "RequiredMprFeatures",
        ExcludedMprFeatures => "ExcludedMprFeatures",
        RequiredStemName => "RequiredStemName",
        ExcludedStemName => "ExcludedStemName",
        PartialParse => "PartialParse",
        BoundRoot => "BoundRoot",
        NonPartialRuleProhibitedAfterFinalTemplate => "NonPartialRuleProhibitedAfterFinalTemplate",
        NonPartialRuleRequiredAfterNonFinalTemplate => {
            "NonPartialRuleRequiredAfterNonFinalTemplate"
        }
        MaxApplicationCount => "MaxApplicationCount",
    }
}

/// A node represents a SUCCESSFUL rule (un)application step iff it is one of the six rule-
/// application `TraceType`s AND carries an `output` snapshot with no `failure_reason` — the
/// convention every trace-emitting call site in `pg-rules` already follows (see `crate::trace`'s
/// module doc and `crate::morph`'s 2026-07-17 analysis-tracing addition). This is what
/// [`find_deepest_frontier`] increments `depth` on.
fn is_success_step(n: &TraceNode) -> bool {
    n.failure_reason.is_none()
        && n.output.is_some()
        && matches!(
            n.type_,
            TraceType::MorphologicalRuleAnalysis
                | TraceType::MorphologicalRuleSynthesis
                | TraceType::PhonologicalRuleAnalysis
                | TraceType::PhonologicalRuleSynthesis
                | TraceType::CompoundingRuleAnalysis
                | TraceType::CompoundingRuleSynthesis
        )
}

/// A node represents a DEAD attempt — a frontier candidate — iff it carries a `failure_reason`
/// (covers `Failed`, every synthesis-side `_not_applied`, and the analysis-side
/// `morphological_rule_not_unapplied`/`compounding_rule_not_unapplied`-via-`morphological_rule_
/// not_unapplied` reuse added 2026-07-17), OR it is a `PhonologicalRuleAnalysis` node recording a
/// failed unapply (`phonological_rule_not_unapplied`, which — unlike its synthesis-side twin — was
/// never given a `FailureReason` parameter, since it never had a call site until this census; see
/// `pg-rules/src/trace.rs`'s doc on `morphological_rule_not_unapplied` for the sibling case that
/// WAS given one). That one reason-less case is reported as `Pattern` (d4) by construction: HC's
/// phonological analysis has no gate besides the reversal pattern/environment fit itself, so
/// "didn't unapply" and "shape mismatch" are the same fact for a `PRule`.
fn is_frontier(n: &TraceNode) -> Option<FailureReason> {
    if let Some(r) = n.failure_reason {
        return Some(r);
    }
    if matches!(n.type_, TraceType::PhonologicalRuleAnalysis)
        && n.output.is_none()
        && n.input.is_some()
    {
        return Some(FailureReason::Pattern);
    }
    None
}

/// Pipeline-stage ordinal for the "ties broken by later pipeline stage" rule (mission's frontier
/// definition). Analysis (0) < synthesis (2) < final validity/match gate (3); anything else (the
/// bookend/lexical-lookup node types, which never carry a `failure_reason` and so are never
/// returned by [`is_frontier`] anyway) gets 1, unused in practice.
fn stage_ordinal(t: TraceType) -> u8 {
    use TraceType::*;
    match t {
        PhonologicalRuleAnalysis | MorphologicalRuleAnalysis | CompoundingRuleAnalysis => 0,
        PhonologicalRuleSynthesis
        | MorphologicalRuleSynthesis
        | CompoundingRuleSynthesis
        | TemplateSynthesisOutput
        | StratumSynthesisOutput => 2,
        Failed => 3,
        _ => 1,
    }
}

struct FrontierHit {
    depth: u32,
    stage: u8,
    reason: FailureReason,
}

/// Which frontier the census reports — see [`find_frontier`]'s doc. `Deepest` is the mission's own
/// definition ("the attempt that got furthest") and is what every printed table in this file's
/// `main` report uses; `Shallowest` is the alternate definition run once per grammar (env
/// `CENSUS_FRONTIER=shallowest`) to check whether the choice materially changes the d1-d6 table —
/// see `run()`'s own doc for the comparison this produced.
#[derive(Copy, Clone, PartialEq, Eq)]
enum FrontierMode {
    Deepest,
    Shallowest,
}

/// Walk the whole trace tree once (root to leaves), tracking depth = count of ancestor
/// [`is_success_step`] nodes, and return the [`FrontierHit`] that is either the DEEPEST (mission's
/// definition — "the attempt that got furthest through the pinned rule sequence") or the
/// SHALLOWEST (the alternate definition checked for material disagreement — the first dead end any
/// branch hits, ignoring how much further OTHER branches got) — see this file's module doc for the
/// full "deepest" definition and its two documented coarsenings; the shallowest variant reuses the
/// exact same depth/stage bookkeeping, just inverted comparison direction.
fn find_frontier(sink: &TreeTraceSink, mode: FrontierMode) -> Option<FrontierHit> {
    let root = sink.root()?;
    let mut best: Option<FrontierHit> = None;
    let mut stack: Vec<(pg_rules::trace::TraceHandle, u32)> = vec![(root, 0)];
    while let Some((h, depth)) = stack.pop() {
        let n = sink.node(h);
        if let Some(reason) = is_frontier(&n) {
            let stage = stage_ordinal(n.type_);
            let better = match &best {
                None => true,
                Some(b) => match mode {
                    FrontierMode::Deepest => (depth, stage) > (b.depth, b.stage),
                    FrontierMode::Shallowest => (depth, stage) < (b.depth, b.stage),
                },
            };
            if better {
                best = Some(FrontierHit {
                    depth,
                    stage,
                    reason,
                });
            }
        }
        let child_depth = if is_success_step(&n) {
            depth + 1
        } else {
            depth
        };
        for &c in &n.children {
            stack.push((c, child_depth));
        }
    }
    best
}

/// One failing candidate's classification outcome — see this file's module doc, "Frontier
/// definition", for [`Outcome::LexLookupBoundary`]'s meaning and why it is a documented gap rather
/// than a bug.
#[derive(Copy, Clone, Debug)]
enum Outcome {
    /// `confirm_one_traced` returned `None` — the candidate's own pins don't resolve (its
    /// designated root isn't a `LexEntry`, or a non-root morpheme resolves to neither a `LexEntry`
    /// nor an `MRule`). Not a cascade-death mechanism at all; reported under d6 with its own raw
    /// label so it never masquerades as a real d1-d5 finding.
    PinResolutionFailed,
    /// A reasoned dead end was found somewhere in the trace tree.
    Frontier(FailureReason),
    /// No frontier node anywhere in the tree, yet the candidate still didn't confirm — the
    /// lexical-lookup boundary gap (module doc, coarsening #2).
    LexLookupBoundary,
}

fn dclass_of(o: Outcome) -> DClass {
    match o {
        Outcome::PinResolutionFailed => DClass::D6Other,
        Outcome::LexLookupBoundary => DClass::D4Shape,
        Outcome::Frontier(r) => classify_reason(r),
    }
}

fn raw_label(o: Outcome) -> &'static str {
    match o {
        Outcome::PinResolutionFailed => "PinResolutionFailed",
        Outcome::LexLookupBoundary => "LexLookupBoundary(untraced-boundary)",
        Outcome::Frontier(r) => reason_name(r),
    }
}

fn classify_failing_candidate(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidate: &Candidate,
    word: &str,
    mode: FrontierMode,
) -> Outcome {
    let sink = TreeTraceSink::new();
    let outcome = confirm::confirm_one_traced(g, owners, morpher, candidate, word, &sink);
    if outcome.is_none() {
        return Outcome::PinResolutionFailed;
    }
    match find_frontier(&sink, mode) {
        Some(hit) => Outcome::Frontier(hit.reason),
        None => Outcome::LexLookupBoundary,
    }
}

// -------------------------------------------------------------------------------------------
// Per-word measurement: classification (untimed) + time-share counterfactual (timed), same
// method as `prefilter_census.rs`'s `measure_word`, generalized from 3 categories to 6.
// -------------------------------------------------------------------------------------------

struct WordCensus {
    total_candidates: usize,
    confirming: usize,
    failing: usize,
    cat_counts: [usize; 6],
    /// Raw `FailureReason`/label histogram PER class (index = `DClass::idx()`), for the d3
    /// feature-clash breakdown and the d6 contents the report must show.
    raw_hist: [FxHashMap<&'static str, usize>; 6],
    time_numer: [f64; 6],
    denom: f64,
    /// `confirm_batch(all candidates)`'s own wall time for this word — summed across words to get
    /// each grammar's TOTAL confirm time, needed (alongside `denom`) for the go/no-go bar's
    /// "class share x failing-time fraction of confirm" projection (mission's exact formula).
    baseline_ms: f64,
}

fn measure_word(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    word: &str,
    candidates: &[Candidate],
    mode: FrontierMode,
) -> WordCensus {
    let mut cat_counts = [0usize; 6];
    let mut raw_hist: [FxHashMap<&'static str, usize>; 6] = Default::default();

    let baseline_start = Instant::now();
    let baseline_buckets = confirm::confirm_batch(g, owners, morpher, candidates, word);
    let baseline_ms = baseline_start.elapsed().as_secs_f64() * 1000.0;

    let mut failing_idx = Vec::new();
    let mut confirming_idx = Vec::new();
    for (i, bucket) in baseline_buckets.iter().enumerate() {
        if bucket.is_empty() {
            failing_idx.push(i);
        } else {
            confirming_idx.push(i);
        }
    }

    let total_candidates = candidates.len();
    let confirming = confirming_idx.len();
    let failing = failing_idx.len();

    if failing_idx.is_empty() {
        return WordCensus {
            total_candidates,
            confirming,
            failing,
            cat_counts,
            raw_hist,
            time_numer: [0.0; 6],
            denom: 0.0,
            baseline_ms,
        };
    }

    // Classify every failing candidate (untimed pass — see module doc: unlike the prior census,
    // there is no cheap "candidates_generated == 0" shortcut here, since attributing WHY needs the
    // real trace tree regardless of that count).
    let mut cat_of: Vec<DClass> = Vec::with_capacity(failing_idx.len());
    for &i in &failing_idx {
        let outcome = classify_failing_candidate(g, owners, morpher, &candidates[i], word, mode);
        let dclass = dclass_of(outcome);
        *raw_hist[dclass.idx()]
            .entry(raw_label(outcome))
            .or_insert(0) += 1;
        cat_counts[dclass.idx()] += 1;
        if dclass == DClass::D5Ordering {
            if let Outcome::Frontier(reason) = outcome {
                dump_d5_sample(g, owners, word, &candidates[i], reason);
            }
        }
        cat_of.push(dclass);
    }

    // Counterfactual timing: keep_confirming, and minus_dN for N in 1..=6.
    let confirming_only: Vec<Candidate> = confirming_idx
        .iter()
        .map(|&i| candidates[i].clone())
        .collect();
    let keep_start = Instant::now();
    let _ = confirm::confirm_batch(g, owners, morpher, &confirming_only, word);
    let keep_ms = keep_start.elapsed().as_secs_f64() * 1000.0;

    let mut minus_ms = [0.0f64; 6];
    for (slot, &target) in D_CLASSES.iter().enumerate() {
        let minus: Vec<Candidate> = candidates
            .iter()
            .enumerate()
            .filter(|(i, _)| match failing_idx.iter().position(|&fi| fi == *i) {
                Some(pos) => cat_of[pos] != target,
                None => true,
            })
            .map(|(_, c)| c.clone())
            .collect();
        let start = Instant::now();
        let _ = confirm::confirm_batch(g, owners, morpher, &minus, word);
        minus_ms[slot] = start.elapsed().as_secs_f64() * 1000.0;
    }

    let denom = baseline_ms - keep_ms;
    let mut time_numer = [0.0f64; 6];
    if denom > 1e-6 {
        for i in 0..6 {
            time_numer[i] = baseline_ms - minus_ms[i];
        }
    }

    WordCensus {
        total_candidates,
        confirming,
        failing,
        cat_counts,
        raw_hist,
        time_numer,
        denom: denom.max(0.0),
        baseline_ms,
    }
}

// -------------------------------------------------------------------------------------------
// Aggregation + report.
// -------------------------------------------------------------------------------------------

struct GrammarReport {
    name: String,
    words_scanned: usize,
    total_candidates: usize,
    total_confirming: usize,
    total_failing: usize,
    cat_counts: [usize; 6],
    raw_hist: [FxHashMap<&'static str, usize>; 6],
    time_numer: [f64; 6],
    time_denom: f64,
    /// Sum of every scanned word's `confirm_batch(all candidates)` wall time — this grammar's TOTAL
    /// measured confirm time (baseline, not the classification pass), for the go/no-go bar's
    /// end-to-end projection: `class_time_share * (time_denom / total_baseline_ms)`.
    total_baseline_ms: f64,
    wall_ms: f64,
}

fn run_grammar(
    name: &str,
    xml_file: &str,
    words_file: &str,
    word_cap: usize,
    mode: FrontierMode,
) -> Option<GrammarReport> {
    let start = Instant::now();
    let g = load_grammar(xml_file)?;
    let all_words = read_words(words_file)?;
    // The census slice is `take(cap)` UNION the pinned confirm-cost outliers: `take(cap)` alone
    // (front of the corpus) can miss the words that dominate confirm time entirely — the whole
    // point of pinning. Pinned words already inside the cap slice are not duplicated.
    let mut words: Vec<String> = all_words.iter().take(word_cap).cloned().collect();
    let already: HashSet<&str> = words.iter().map(String::as_str).collect();
    let pinned_extra: Vec<String> = read_pinned(words_file)
        .into_iter()
        .filter(|w| !already.contains(w.as_str()))
        .collect();
    if !pinned_extra.is_empty() {
        eprintln!(
            "  [{name}] pinned outliers added beyond take({word_cap}): {}",
            pinned_extra.len()
        );
        words.extend(pinned_extra);
    }
    if words.is_empty() {
        return None;
    }

    let emit::EmitResult { lexc_source, .. } = emit::emit(&g);
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &lexc_source)
        .unwrap_or_else(|| panic!("foma failed to compile the emitted lexc source for {name}"));

    let peeler = ReduplicationPeeler::new(&g);
    let owners = confirm::build_morpheme_owners(&g);
    let morpher = Morpher::new(&g, usize::MAX).with_word_timeout(Some(ENGINE_TIMEOUT));

    let mut total_candidates = 0usize;
    let mut total_confirming = 0usize;
    let mut total_failing = 0usize;
    let mut cat_counts = [0usize; 6];
    let mut raw_hist: [FxHashMap<&'static str, usize>; 6] = Default::default();
    let mut time_numer = [0.0f64; 6];
    let mut time_denom = 0.0f64;
    let mut total_baseline_ms = 0.0f64;
    let mut words_scanned = 0usize;

    for word in &words {
        let candidates = propose_and_peel(&net, &g, &peeler, word);
        if candidates.is_empty() {
            continue;
        }
        words_scanned += 1;
        let wc = measure_word(&g, &owners, &morpher, word, &candidates, mode);
        total_candidates += wc.total_candidates;
        total_confirming += wc.confirming;
        total_failing += wc.failing;
        total_baseline_ms += wc.baseline_ms;
        for i in 0..6 {
            cat_counts[i] += wc.cat_counts[i];
            for (r, n) in &wc.raw_hist[i] {
                *raw_hist[i].entry(r).or_insert(0) += n;
            }
        }
        if wc.denom > 1e-6 {
            for (acc, v) in time_numer.iter_mut().zip(wc.time_numer.iter()) {
                *acc += v;
            }
            time_denom += wc.denom;
        }
        println!(
            "  ... {word}: {} candidates, {} confirming, {} failing (running totals: scanned={words_scanned})",
            wc.total_candidates, wc.confirming, wc.failing
        );
    }

    Some(GrammarReport {
        name: name.to_string(),
        words_scanned,
        total_candidates,
        total_confirming,
        total_failing,
        cat_counts,
        raw_hist,
        time_numer,
        time_denom,
        total_baseline_ms,
        wall_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

fn print_report(r: &GrammarReport) {
    println!();
    println!("## {}", r.name);
    println!(
        "words_scanned={} total_candidates={} total_confirming={} total_failing={} candidate_precision={:.4}",
        r.words_scanned,
        r.total_candidates,
        r.total_confirming,
        r.total_failing,
        if r.total_candidates > 0 {
            r.total_confirming as f64 / r.total_candidates as f64
        } else {
            1.0
        }
    );
    println!();
    println!("failing-candidate counts + time share by class:");
    for &c in &D_CLASSES {
        let i = c.idx();
        let count_pct = if r.total_failing > 0 {
            100.0 * r.cat_counts[i] as f64 / r.total_failing as f64
        } else {
            0.0
        };
        let time_pct = if r.time_denom > 1e-6 {
            100.0 * r.time_numer[i] / r.time_denom
        } else {
            f64::NAN
        };
        println!(
            "  {:28} count={:7} ({:5.1}%)   time-share={:5.1}%",
            c.label(),
            r.cat_counts[i],
            count_pct,
            time_pct
        );
    }
    println!();
    for &c in &D_CLASSES {
        let i = c.idx();
        if r.cat_counts[i] == 0 {
            continue;
        }
        println!("  {} raw-reason breakdown:", c.label());
        let mut reasons: Vec<(&&'static str, &usize)> = r.raw_hist[i].iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(a.1));
        for (reason, count) in reasons {
            println!("    {:40} {}", reason, count);
        }
    }
    println!();
    println!(
        "time-share denominator (sum of baseline-minus-keep_confirming, ms) = {:.1}",
        r.time_denom
    );
    let failing_fraction = if r.total_baseline_ms > 1e-6 {
        r.time_denom / r.total_baseline_ms
    } else {
        f64::NAN
    };
    println!(
        "total measured confirm_batch (baseline) time = {:.1}ms; failing-candidate share of that = {:.1}%",
        r.total_baseline_ms,
        100.0 * failing_fraction
    );
    if r.time_denom <= 1e-6 {
        println!(
            "GO/NO-GO metric: n/a (no measurable failing-candidate time on this corpus slice)"
        );
    } else {
        println!(
            "GO/NO-GO projection per class (class_time_share x failing_fraction, vs the plan's >=15% \
             end-to-end bar; class must ALSO be >=20% of failing time on some grammar):"
        );
        for &c in &D_CLASSES {
            let i = c.idx();
            let class_share = r.time_numer[i] / r.time_denom;
            let projected = 100.0 * class_share * failing_fraction;
            println!(
                "  {:28} projected end-to-end win = {:5.1}%",
                c.label(),
                projected
            );
        }
    }
    println!();
    println!("grammar wall time: {:.1}ms", r.wall_ms);
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn run() {
    init_dump_budget();
    let args: Vec<String> = std::env::args().collect();
    let grammars: Vec<(&str, &str, &str, usize)> = if args.len() >= 2 {
        let cap_override = args.get(2).and_then(|s| s.parse().ok());
        match args[1].as_str() {
            "sena" => vec![(
                "Sena",
                "sena-hc.xml",
                "sena-words.txt",
                cap_override.unwrap_or(env_usize("CENSUS_SENA_CAP", 300)),
            )],
            "indonesian" => vec![(
                "Indonesian",
                "indonesian-hc.xml",
                "indonesian-words.txt",
                cap_override.unwrap_or(env_usize("CENSUS_INDONESIAN_CAP", 121)),
            )],
            "amharic" => vec![(
                "Amharic",
                "amharic-hc.xml",
                "amharic-words.txt",
                cap_override.unwrap_or(env_usize("CENSUS_AMHARIC_CAP", 40)),
            )],
            other => {
                eprintln!("unknown grammar {other:?} (want sena|indonesian|amharic)");
                std::process::exit(2);
            }
        }
    } else {
        vec![
            (
                "Sena",
                "sena-hc.xml",
                "sena-words.txt",
                env_usize("CENSUS_SENA_CAP", 300),
            ),
            (
                "Indonesian",
                "indonesian-hc.xml",
                "indonesian-words.txt",
                env_usize("CENSUS_INDONESIAN_CAP", 121),
            ),
            (
                "Amharic",
                "amharic-hc.xml",
                "amharic-words.txt",
                env_usize("CENSUS_AMHARIC_CAP", 40),
            ),
        ]
    };

    let mode = match std::env::var("CENSUS_FRONTIER").as_deref() {
        Ok("shallowest") => FrontierMode::Shallowest,
        _ => FrontierMode::Deepest,
    };

    println!("# Phase 0 dead-end attribution census");
    println!();
    println!(
        "See `docs/superpowers/specs/2026-07-17-better-proposing-fst-plan.md` Phase 0 for the \
         go/no-go criterion: a class is buildable only if it is >=20% of failing-candidate time on \
         at least one grammar AND the projected end-to-end win is >=15% of that grammar's confirm \
         time."
    );
    println!(
        "Frontier definition in use: {} (set CENSUS_FRONTIER=shallowest for the alternate check).",
        match mode {
            FrontierMode::Deepest => "deepest (mission's own definition)",
            FrontierMode::Shallowest => "shallowest (alternate, for the materiality check)",
        }
    );

    for (name, xml, words, cap) in grammars {
        println!();
        println!("Running {name} (cap={cap})...");
        match run_grammar(name, xml, words, cap, mode) {
            Some(report) => print_report(&report),
            None => {
                println!("## {name}");
                println!("skipped: {xml} or {words} not present on disk");
            }
        }
    }
}

/// Amharic's deep composite/rule-chain recursion needs a big stack under release inlining (same
/// trick `precision_bench`/`knob_probe`/`prefilter_census` use) — this census's tracing pass adds
/// slightly MORE recursion depth than `prefilter_census`'s did (the analysis cascade is now traced
/// too, adding one stack frame per `_traced` wrapper — see `pg-rules/src/morph.rs`'s 2026-07-17
/// addition), so this budget stays generous.
fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(run)
        .expect("failed to spawn census thread");
    handle.join().expect("census thread panicked");
}
