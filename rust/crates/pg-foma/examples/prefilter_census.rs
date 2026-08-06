//! Rejection census for the deterministic candidate pre-filter design.
//! See `docs/research/pg-foma-prefilter-census-design-notes.md` for what is measured and the counterfactual-timing methodology.

//! Usage: `cargo run -p pg-foma --release --example prefilter_census -- <sena|indonesian|amharic> [word_cap]`; prints a markdown report (category counts, `FailureReason` histogram, time-share ratios). Not part of `cargo test`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
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
use pg_grammar::model::Grammar;
use pg_parse::Morpher;
use pg_rules::trace::{FailureReason, TraceType, TreeTraceSink};

const ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

// Sample loading + propose/peel plumbing, copied from `examples/precision_bench.rs` rather than imported: examples can't depend on test code.

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
    // Real corpus words, never adversarial synthetic stress strings, so an unbounded chain-depth budget is safe here.
    let budget = ComposeBudget::from_env();
    let peeled = peeler
        .peel_candidates(g, word, &budget, &mut |r: &str| propose(net, r))
        .unwrap_or_else(|e| {
            eprintln!("[prefilter_census] reduplication peel refused for {word:?}: {e}");
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

// Category model.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum Category {
    /// Derives at least one candidate but is rejected by `pg_rules::validity`'s gate: deterministically pre-checkable.
    A,
    /// `candidates_generated == 0`, or only `PartialParse`/`SurfaceFormMismatch` reasons: a cascade dead end, not pre-checkable.
    B,
    /// Everything else: `ObligatorySyntacticFeatures`, pin-resolution `None`, or a positional-routing mismatch.
    C,
}

/// The validity-gate `FailureReason`s `pg_rules::validity::allomorphs_valid_impl` actually emits; not all `FailureReason` variants exist there.
const VALIDITY_GATE_REASONS: &[FailureReason] = &[
    FailureReason::Environments,
    FailureReason::AllomorphCoOccurrenceRules,
    FailureReason::MorphemeCoOccurrenceRules,
    FailureReason::BoundRoot,
    FailureReason::RequiredStemName,
    FailureReason::ExcludedStemName,
    FailureReason::DisjunctiveAllomorph,
    FailureReason::RequiredSyntacticFeatureStruct,
];

const CASCADE_REASONS: &[FailureReason] = &[
    FailureReason::PartialParse,
    FailureReason::SurfaceFormMismatch,
];

fn reason_name(r: FailureReason) -> &'static str {
    match r {
        FailureReason::ObligatorySyntacticFeatures => "ObligatorySyntacticFeatures",
        FailureReason::AllomorphCoOccurrenceRules => "AllomorphCoOccurrenceRules",
        FailureReason::Environments => "Environments",
        FailureReason::MorphemeCoOccurrenceRules => "MorphemeCoOccurrenceRules",
        FailureReason::DisjunctiveAllomorph => "DisjunctiveAllomorph",
        FailureReason::SurfaceFormMismatch => "SurfaceFormMismatch",
        FailureReason::Pattern => "Pattern",
        FailureReason::HeadPattern => "HeadPattern",
        FailureReason::NonHeadPattern => "NonHeadPattern",
        FailureReason::RequiredSyntacticFeatureStruct => "RequiredSyntacticFeatureStruct",
        FailureReason::HeadRequiredSyntacticFeatureStruct => "HeadRequiredSyntacticFeatureStruct",
        FailureReason::NonHeadRequiredSyntacticFeatureStruct => {
            "NonHeadRequiredSyntacticFeatureStruct"
        }
        FailureReason::HeadProdRestrictMprFeatures => "HeadProdRestrictMprFeatures",
        FailureReason::NonHeadProdRestrictMprFeatures => "NonHeadProdRestrictMprFeatures",
        FailureReason::RequiredMprFeatures => "RequiredMprFeatures",
        FailureReason::ExcludedMprFeatures => "ExcludedMprFeatures",
        FailureReason::RequiredStemName => "RequiredStemName",
        FailureReason::ExcludedStemName => "ExcludedStemName",
        FailureReason::PartialParse => "PartialParse",
        FailureReason::BoundRoot => "BoundRoot",
        FailureReason::NonPartialRuleProhibitedAfterFinalTemplate => {
            "NonPartialRuleProhibitedAfterFinalTemplate"
        }
        FailureReason::NonPartialRuleRequiredAfterNonFinalTemplate => {
            "NonPartialRuleRequiredAfterNonFinalTemplate"
        }
        FailureReason::MaxApplicationCount => "MaxApplicationCount",
    }
}

/// Walks every `Failed`-type node reachable from `sink`'s root, collecting `failure_reason`s.
fn collect_failure_reasons(sink: &TreeTraceSink) -> Vec<FailureReason> {
    let mut out = Vec::new();
    let Some(root) = sink.root() else { return out };
    let mut stack = vec![root];
    while let Some(h) = stack.pop() {
        let n = sink.node(h);
        if let Some(r) = n.failure_reason {
            if matches!(n.type_, TraceType::Failed) {
                out.push(r);
            }
        }
        stack.extend(n.children.iter().copied());
    }
    out
}

fn classify_reasons(reasons: &[FailureReason]) -> Category {
    if reasons.iter().any(|r| VALIDITY_GATE_REASONS.contains(r)) {
        Category::A
    } else if reasons.iter().any(|r| CASCADE_REASONS.contains(r)) {
        Category::B
    } else {
        Category::C
    }
}

// Per-word measurement.

struct WordCensus {
    total_candidates: usize,
    confirming: usize,
    failing: usize,
    cat_counts: [usize; 3], // A, B, C
    reason_hist: rustc_hash::FxHashMap<&'static str, usize>,
    /// This word's numerators for the aggregate time-share ratios; `None` when there was nothing to attribute or the denominator was non-positive.
    time_shares: Option<[f64; 3]>, // (baseline-minus_x) for x in A,B,C
    denom: f64, // baseline - keep_confirming
}

fn measure_word(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    word: &str,
    candidates: &[Candidate],
) -> WordCensus {
    let mut cat_counts = [0usize; 3];
    let mut reason_hist: rustc_hash::FxHashMap<&'static str, usize> =
        rustc_hash::FxHashMap::default();

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
            reason_hist,
            time_shares: None,
            denom: 0.0,
        };
    }

    // Classify every failing candidate (untimed pass).
    let mut cat_of: Vec<Category> = Vec::with_capacity(failing_idx.len());
    for &i in &failing_idx {
        let noop = pg_rules::trace::NoopSink;
        let cheap = confirm::confirm_one_traced(g, owners, morpher, &candidates[i], word, &noop);
        let cat = match cheap {
            None => Category::C, // pin resolution failed — plan's "pin-resolution None" bucket.
            Some(outcome) if outcome.candidates_generated == 0 => Category::B,
            Some(_) => {
                let sink = TreeTraceSink::new();
                let _ =
                    confirm::confirm_one_traced(g, owners, morpher, &candidates[i], word, &sink);
                let reasons = collect_failure_reasons(&sink);
                for &r in &reasons {
                    if VALIDITY_GATE_REASONS.contains(&r) {
                        *reason_hist.entry(reason_name(r)).or_insert(0) += 1;
                    }
                }
                classify_reasons(&reasons)
            }
        };
        cat_counts[match cat {
            Category::A => 0,
            Category::B => 1,
            Category::C => 2,
        }] += 1;
        cat_of.push(cat);
    }

    // Counterfactual timing: keep_confirming, and minus_{a,b,c}.
    let confirming_only: Vec<Candidate> = confirming_idx
        .iter()
        .map(|&i| candidates[i].clone())
        .collect();
    let keep_start = Instant::now();
    let _ = confirm::confirm_batch(g, owners, morpher, &confirming_only, word);
    let keep_ms = keep_start.elapsed().as_secs_f64() * 1000.0;

    let mut minus_ms = [0.0f64; 3];
    for (slot, target) in [Category::A, Category::B, Category::C]
        .into_iter()
        .enumerate()
    {
        let minus: Vec<Candidate> = candidates
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                // Keep every confirming candidate untouched, plus every failing candidate not of this category.
                match failing_idx.iter().position(|&fi| fi == *i) {
                    Some(pos) => cat_of[pos] != target,
                    None => true,
                }
            })
            .map(|(_, c)| c.clone())
            .collect();
        let start = Instant::now();
        let _ = confirm::confirm_batch(g, owners, morpher, &minus, word);
        minus_ms[slot] = start.elapsed().as_secs_f64() * 1000.0;
    }

    let denom = baseline_ms - keep_ms;
    let time_shares = if denom > 1e-6 {
        Some([
            baseline_ms - minus_ms[0],
            baseline_ms - minus_ms[1],
            baseline_ms - minus_ms[2],
        ])
    } else {
        None
    };

    WordCensus {
        total_candidates,
        confirming,
        failing,
        cat_counts,
        reason_hist,
        time_shares,
        denom: denom.max(0.0),
    }
}

// Aggregation + report.

struct GrammarReport {
    name: String,
    words_scanned: usize,
    total_candidates: usize,
    total_confirming: usize,
    total_failing: usize,
    cat_counts: [usize; 3],
    reason_hist: rustc_hash::FxHashMap<&'static str, usize>,
    time_numer: [f64; 3],
    time_denom: f64,
    wall_ms: f64,
}

fn run_grammar(
    name: &str,
    xml_file: &str,
    words_file: &str,
    word_cap: usize,
) -> Option<GrammarReport> {
    let start = Instant::now();
    let g = load_grammar(xml_file)?;
    let all_words = read_words(words_file)?;
    let words: Vec<String> = all_words.into_iter().take(word_cap).collect();
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
    let mut cat_counts = [0usize; 3];
    let mut reason_hist: rustc_hash::FxHashMap<&'static str, usize> =
        rustc_hash::FxHashMap::default();
    let mut time_numer = [0.0f64; 3];
    let mut time_denom = 0.0f64;
    let mut words_scanned = 0usize;

    for word in &words {
        let candidates = propose_and_peel(&net, &g, &peeler, word);
        if candidates.is_empty() {
            continue;
        }
        words_scanned += 1;
        let wc = measure_word(&g, &owners, &morpher, word, &candidates);
        total_candidates += wc.total_candidates;
        total_confirming += wc.confirming;
        total_failing += wc.failing;
        for (acc, v) in cat_counts.iter_mut().zip(wc.cat_counts.iter()) {
            *acc += v;
        }
        for (r, n) in wc.reason_hist {
            *reason_hist.entry(r).or_insert(0) += n;
        }
        if let Some(shares) = wc.time_shares {
            for i in 0..3 {
                time_numer[i] += shares[i];
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
        reason_hist,
        time_numer,
        time_denom,
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
    println!(
        "failing-candidate counts by category: A(validity-gate)={} B(cascade-dead-end)={} C(other)={}",
        r.cat_counts[0], r.cat_counts[1], r.cat_counts[2]
    );
    if r.total_failing > 0 {
        println!(
            "  as %% of failing candidates: A={:.1}%% B={:.1}%% C={:.1}%%",
            100.0 * r.cat_counts[0] as f64 / r.total_failing as f64,
            100.0 * r.cat_counts[1] as f64 / r.total_failing as f64,
            100.0 * r.cat_counts[2] as f64 / r.total_failing as f64,
        );
    }
    println!();
    println!("FailureReason histogram within category (a) (validity-gate rejections):");
    let mut reasons: Vec<(&&'static str, &usize)> = r.reason_hist.iter().collect();
    reasons.sort_by(|a, b| b.1.cmp(a.1));
    for (reason, count) in reasons {
        println!("  {:32} {}", reason, count);
    }
    println!();
    println!(
        "time-share denominator (sum of baseline-minus-keep_confirming, ms) = {:.1}",
        r.time_denom
    );
    if r.time_denom > 1e-6 {
        println!(
            "GO/NO-GO metric — category (a)'s share of FAILING-candidate confirm time: {:.1}%%",
            100.0 * r.time_numer[0] / r.time_denom
        );
        println!(
            "  (for reference) B share: {:.1}%%  C share: {:.1}%%  (ratios need not sum to 100%% \
             — see module doc's non-additivity note)",
            100.0 * r.time_numer[1] / r.time_denom,
            100.0 * r.time_numer[2] / r.time_denom,
        );
    } else {
        println!(
            "GO/NO-GO metric: n/a (no measurable failing-candidate time on this corpus slice)"
        );
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

    println!("# Phase 0 candidate pre-filter rejection census");
    println!();
    println!(
        "See `docs/superpowers/specs/2026-07-16-candidate-prefilter-plan.md` Phase 0 for the \
         go/no-go criterion: category (a) under ~10%% of failing-candidate time on every grammar \
         => do not build the filter."
    );

    for (name, xml, words, cap) in grammars {
        println!();
        println!("Running {name} (cap={cap})...");
        match run_grammar(name, xml, words, cap) {
            Some(report) => print_report(&report),
            None => {
                println!("## {name}");
                println!("skipped: {xml} or {words} not present on disk");
            }
        }
    }
}

/// Amharic's deep composite/rule-chain recursion needs a big stack under release inlining, more than `precision_bench`/`knob_probe` need.
fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(run)
        .expect("failed to spawn census thread");
    handle.join().expect("census thread panicked");
}
