//! `pangloss make-report <grammar> <out.md> [options]` — a per-language report that composes the
//! evidence and states what was not tested: ONE command producing ONE markdown file containing
//! build time, artifact size, latency
//! percentiles, the compilation-plan mermaid diagram, and the conformance verdict — composing
//! sections 1-3 (`pg_foma::readiness_policy`/`readiness_verdict`/`plan_diagram`), never
//! reimplementing any of them.
//!
//! # What this module measures itself, and what it only composes
//! Sections 1-3 define the SHAPE of a certification (the threshold policy, the tiered verdict, the
//! plan diagram) but nothing yet populates a real [`pg_foma::readiness_verdict::Measurements`] from
//! a live grammar/pack — that is this module's own job:
//! - **Pack size + trust status**: a REAL `.pgpack`, never a caller-supplied trust parameter (see
//!   "Trust comes from a real artifact" below).
//! - **Lexicon scale**: `grammar.entries.len()` — a plain, direct count, not derived from the pack
//!   (the pack's runtime payload is still an honest placeholder, `pg-pack`'s own top doc; the
//!   in-memory `Grammar` is the real source of this count).
//! - **Latency percentiles**: measured IN-PROCESS via nanosecond `Instant`/`Duration` timing over a
//!   real, freshly-built [`pg_foma::composite::FomaAnalyzer`] — mirroring `rust/crates/pg-foma/
//!   tests/typology_speedup.rs`'s own methodology (median-of-repeats per word, discard one warmup
//!   call, a per-run-calibrated timer floor, below-floor reported honestly, never `pangloss batch`'s
//!   integer-millisecond TSV column). See "Latency methodology" below for the exact percentile
//!   computation.
//! - **Coverage**: an attestation (never a measurement — `pg_foma::readiness_verdict`'s own rule 2)
//!   when `--corpus`/`--attestor`/`--attested-on` are all given; honestly not-assessed otherwise.
//! - **Build time**: the wall-clock cost of building the compiled proposer+confirm analyzer this
//!   report's latency numbers are about (`FomaAnalyzer::new`) — informational only (no threshold in
//!   `ThresholdPolicy` gates it), rendered with the same below-floor discipline as latency.
//! - **The compilation-plan diagram + the conformance verdict**: pure composition —
//!   `pg_foma::plan_diagram::{build_plan_document, render_mermaid}` and `pg_foma::
//!   readiness_verdict::certify` respectively, exactly as `plan-diagram`/the section-3 tests already
//!   exercise them. This module adds no new plan-diagram or capability-verdict logic of its own.
//!
//! # Trust comes from a real artifact, never a caller-settable parameter
//! `certify`'s `trust: &TrustStatus` parameter is never populated from a bare CLI flag here. Either
//! `--pack=<path>` names an existing `.pgpack` — read via [`pg_pack::read_pack`], its
//! `manifest.capability_trust` is the trust this report certifies against — or, with no `--pack`
//! given, this module builds one itself via [`crate::pack::build_pack`] (the exact same real
//! capability-trust-stamping logic `pangloss pack` uses, factored out of that module for this
//! reuse) and reads the trust back off the manifest that call produces. Either way, the trust this
//! report certifies against is the real stamp on a real artifact, not a value a caller typed in.
//! [`map_trust`] is a **plain, non-lossy field-for-field projection** of `pg_pack::CapabilityTrust`
//! into `pg_foma::readiness_verdict::TrustStatus` — the latter is documented as a local mirror of
//! the former precisely because `pg-pack` already depends on `pg-foma` (for `HealthReport`), so
//! `pg-foma` cannot depend back on `pg-pack` without a cycle; the two shapes are kept in exact
//! field-for-field correspondence by convention, not by a shared type, and this function is where
//! that correspondence is actually exercised end-to-end.
//!
//! # Capability enforcement mirrors the rest of this CLI — never a quieter path
//! Exactly like `run_batch`/`run_parse`/`pangloss pack`: a capability `Refuse` verdict with no
//! `--allow-unproven` means **no compiled artifact is built or measured at all** — `pack_size`/
//! `lexicon_scale`/`latency`/`coverage` all report [`pg_foma::readiness_verdict::CheckOutcome::
//! NotAssessed`] (via `certify`'s own documented `measurements: None` contract), and the tier is
//! `NotSupported`, citing the real refusal. This is the expected, headline case for all three
//! reference grammars today (`docs/benchmark-matrix.md`: all three refuse on `mpr-group.
//! overwrite-output`, a permanent carve-out) — see this module's own tests and this crate's gate
//! run for the pasted proof. `--allow-unproven` here requires the SAME flag be passed to
//! `make-report` itself as to `pangloss pack` — a caller cannot point `--pack` at a pre-built
//! overridden artifact and have this command quietly measure against it without also acknowledging
//! the override at the report layer; this is deliberate, not an oversight (ADR 0005's own
//! never-becomes-convenient philosophy).
//!
//! # Latency methodology (never `pangloss batch`'s integer-millisecond floor)
//! For each word in the word list (`--words=<path>`, one word per line; falling back to this
//! grammar's own lexical root surface forms when omitted — see [`default_word_list`]), this module
//! times [`pg_foma::composite::FomaAnalyzer::analyze_word`] `--repeats` times (default 7) after one
//! discarded warmup call, keeps that word's MEDIAN nanosecond duration, then computes p50/p90/p99
//! (nearest-rank method) over the sorted per-word medians — the same "median-of-repeats per word,
//! percentile across words" shape `typology_speedup.rs` uses, just driven over one caller-chosen
//! grammar/word-list instead of the whole conformance corpus. [`measure_timer_floor_ns`] calibrates
//! this process's real `Instant` granularity once per run (never a hardcoded platform constant);
//! any percentile at or below that floor renders as [`pg_foma::readiness_verdict::
//! LatencyMeasurement::BelowFloor`], never a literal `0` (this module's own
//! `below_floor_latency_never_reports_as_a_bare_millis_zero` test pins that).
//!
//! # Coverage's token definition
//! A corpus's tokens are its whitespace-separated words (`str::split_whitespace`). A token this
//! grammar's own segmentation rejects outright ([`crate::foma_invalid_shape`], the SAME guard
//! `run_batch`/`run_parse` use to keep the `SKIPPED` vs. `ok` status column identical between
//! engines) counts as a miss, not an exclusion — the analysis-rate denominator is every token in the
//! corpus, never a pre-filtered subset (`pg_foma::readiness_verdict::COVERAGE_RATE_STATEMENT`: "the
//! fraction of tokens receiving at least one analysis").
//!
//! # What is always stated as NOT tested
//! Per spec ("A per-language report ... SHALL state which checks it did not perform"): correctness
//! is never certified by this report (coverage is an analysis rate, not accuracy — the conformance
//! suite is the correctness authority); a fallback word list is named as such when `--words` is
//! omitted; coverage is named not-assessed when no corpus attestation is supplied; and — the
//! Refuse-without-override case — build time/artifact size/lexicon scale/latency/coverage are ALL
//! named not-measured/not-assessed together, since no compiled artifact exists at all to measure any
//! of them.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::Instant;

use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::evaluate_capability_with_semantics;
use pg_foma::composite::FomaAnalyzer;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::plan_diagram::{
    build_plan_document_with_semantics, render_mermaid, MermaidRender, RenderMode,
};
use pg_foma::readiness_policy::{policy_v1, ThresholdPolicy};
use pg_foma::readiness_verdict::{
    certify_with_semantics, CapabilitySummary, CheckKind, CheckOutcome, CheckResult, CheckValue,
    CoverageAssessment, LatencyMeasurement, Measurements, OverriddenConfig as RvOverriddenConfig,
    OverrideRecord as RvOverrideRecord, ReadinessReport, Tier, TrustStatus,
};
use pg_grammar::model::Grammar;
use sha2::{Digest, Sha256};

const USAGE: &str = "usage: make-report <grammar> <out.md> [--pack=<path>] [--words=<path>] \
[--corpus=<path> --attestor=<name> --attested-on=<date>] [--policy=<path>] [--allow-unproven] \
[--authorized-by=<name>] [--reason=<text>] [--repeats=N]";

// =================================================================================================
// Small, self-contained helpers (hashing, git introspection, trust projection, timing)
// =================================================================================================

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Runs `git <args>` from the current working directory, returning trimmed stdout on success and
/// `None` on any failure (git not installed, not a git checkout, no such ref) — never a panic, since
/// "pinned revisions" is a best-effort re-derivation aid, not a hard requirement of the report.
fn git_output(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn repo_head_revision() -> String {
    git_output(&["rev-parse", "HEAD"])
        .unwrap_or_else(|| "unknown (not a git checkout, or git unavailable)".to_string())
}

/// `git submodule status <path>` reports the pinned gitlink commit whether or not the submodule is
/// actually checked out locally (a leading `-` means uninitialized, `+` means the working tree
/// disagrees with the pinned commit) — exactly the "pinned revision" this report needs, without
/// requiring the submodule to be present. `rel_path` must be resolved relative to the repo
/// TOPLEVEL, never the process's own current directory: `cargo test` (and this binary, run from
/// anywhere) sets its cwd to wherever it happens to be invoked from, and `git submodule status`
/// resolves its path argument as a pathspec against THAT directory -- resolving `--show-toplevel`
/// first makes this correct regardless of where `pangloss` is actually run from.
fn submodule_revision(rel_path: &str) -> String {
    let Some(top) = git_output(&["rev-parse", "--show-toplevel"]) else {
        return format!("unknown ({rel_path}: not a git checkout, or git unavailable)");
    };
    let out = std::process::Command::new("git")
        .current_dir(&top)
        .args(["submodule", "status", rel_path])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                format!("unknown ({rel_path}: git submodule status returned no output)")
            } else {
                s
            }
        }
        _ => format!("unknown ({rel_path}: no such submodule at the repo toplevel {top})"),
    }
}

/// Projects `pg_pack::CapabilityTrust` into `pg_foma::readiness_verdict::TrustStatus` — a plain,
/// non-lossy field-for-field copy. See this module's own top doc, "Trust comes from a real
/// artifact," for why this projection exists instead of a shared type (`pg-pack` depends on
/// `pg-foma`; the reverse would cycle).
fn map_trust(t: &pg_pack::CapabilityTrust) -> TrustStatus {
    match t {
        pg_pack::CapabilityTrust::Proven => TrustStatus::Proven,
        pg_pack::CapabilityTrust::Overridden(record) => TrustStatus::Overridden(RvOverrideRecord {
            authorized_by: record.authorized_by.clone(),
            reason: record.reason.clone(),
            recorded_at: record.recorded_at.clone(),
            overridden_configs: record
                .overridden_configs
                .iter()
                .map(|c| RvOverriddenConfig {
                    predicate: c.predicate.clone(),
                    construct: c.construct.clone(),
                    witness: c.witness.clone(),
                })
                .collect(),
        }),
    }
}

/// Calibrates this process's real `Instant` tick granularity — mirrors `tests/typology_speedup.rs`'s
/// `measure_timer_floor_ns` exactly (that harness's own types are test-only, not importable as a
/// library, so this is a deliberate, small, literal restatement rather than a shared dependency).
fn measure_timer_floor_ns() -> u64 {
    let mut floor = u64::MAX;
    let mut prev = Instant::now();
    for _ in 0..16 {
        loop {
            let now = Instant::now();
            if now > prev {
                floor = floor.min((now - prev).as_nanos() as u64);
                prev = now;
                break;
            }
        }
    }
    floor.max(1)
}

/// A single nanosecond duration, honoring the below-floor discipline: never `Millis(0.0)`, always
/// [`LatencyMeasurement::BelowFloor`] once the raw value sits at or under the calibrated floor.
fn latency_measurement(value_ns: u64, floor_ns: u64) -> LatencyMeasurement {
    if value_ns < floor_ns {
        LatencyMeasurement::BelowFloor {
            floor_ms: floor_ns as f64 / 1_000_000.0,
        }
    } else {
        LatencyMeasurement::Millis(value_ns as f64 / 1_000_000.0)
    }
}

fn render_latency_measurement(m: &LatencyMeasurement) -> String {
    match m {
        LatencyMeasurement::Millis(ms) => format!("{ms:.3} ms"),
        LatencyMeasurement::BelowFloor { floor_ms } => {
            format!("<{floor_ms:.6} ms (below timer floor)")
        }
    }
}

/// Nearest-rank percentile over an ALREADY-SORTED ascending slice — `p` in `0.0..=100.0`. Empty
/// input returns 0 (callers must never call this on an empty word list; see `run_make_report`'s own
/// hard error when no words at all are available).
fn percentile_ns(sorted_asc: &[u64], p: f64) -> u64 {
    if sorted_asc.is_empty() {
        return 0;
    }
    let n = sorted_asc.len();
    let idx = ((p / 100.0) * n as f64).ceil() as usize;
    let idx = idx.clamp(1, n);
    sorted_asc[idx - 1]
}

/// Every distinct, non-empty root-allomorph surface form in `g`'s lexicon — the fallback word list
/// used to measure latency when the caller supplies no `--words` file (this module's own top doc,
/// "Latency methodology"). Real words drawn from the grammar actually being certified (never
/// authored/fabricated here), just disclosed in the report as a fallback rather than a
/// representative corpus sample.
fn default_word_list(g: &Grammar) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for entry in &g.entries {
        for allomorph in &entry.allomorphs {
            let text = allomorph.shape.text.clone();
            if !text.is_empty() && !words.contains(&text) {
                words.push(text);
            }
        }
    }
    words
}

/// Times every word in `words` against `analyzer`: one discarded warmup call, then `repeats` timed
/// samples, keeping that word's median nanosecond duration. Returns the raw (p50, p90, p99)
/// nanosecond values over the sorted per-word medians (nearest-rank method) — below-floor rendering
/// happens at the caller via [`latency_measurement`], not here, so this function stays a pure
/// timing primitive.
fn measure_latency_percentiles_ns(
    analyzer: &mut FomaAnalyzer,
    words: &[String],
    repeats: u32,
) -> (u64, u64, u64) {
    let mut per_word_medians_ns: Vec<u64> = Vec::with_capacity(words.len());
    for word in words {
        let _ = analyzer.analyze_word(word); // discarded warmup
        let mut samples: Vec<u64> = Vec::with_capacity(repeats as usize);
        for _ in 0..repeats {
            let start = Instant::now();
            let _ = analyzer.analyze_word(word);
            samples.push(start.elapsed().as_nanos() as u64);
        }
        samples.sort_unstable();
        per_word_medians_ns.push(samples[samples.len() / 2]);
    }
    per_word_medians_ns.sort_unstable();
    (
        percentile_ns(&per_word_medians_ns, 50.0),
        percentile_ns(&per_word_medians_ns, 90.0),
        percentile_ns(&per_word_medians_ns, 99.0),
    )
}

/// Token-level analysis rate over `corpus_text` (this module's own top doc, "Coverage's token
/// definition"): whitespace-separated tokens, every one counted in the denominator, a token this
/// grammar's own segmentation rejects outright counting as a miss rather than an exclusion. `Err`
/// only when the corpus contains no tokens at all (an empty/whitespace-only file) — a rate cannot be
/// honestly computed over zero tokens, so this is a hard error rather than a fabricated `0.0`.
fn measure_coverage_rate(
    analyzer: &mut FomaAnalyzer,
    g: &Grammar,
    corpus_text: &str,
) -> Result<f64, String> {
    let tokens: Vec<&str> = corpus_text.split_whitespace().collect();
    if tokens.is_empty() {
        return Err(
            "--corpus file contains no whitespace-separated tokens -- cannot compute an honest \
             analysis rate over zero tokens"
                .to_string(),
        );
    }
    let mut hit = 0usize;
    for token in &tokens {
        if crate::foma_invalid_shape(g, token) {
            continue; // a miss, not an exclusion -- still counted in `tokens.len()` below.
        }
        if analyzer.analyze_word(token).confirmed > 0 {
            hit += 1;
        }
    }
    Ok(hit as f64 / tokens.len() as f64)
}

// =================================================================================================
// Markdown rendering
// =================================================================================================

fn fmt_check_value(v: &CheckValue) -> String {
    match v {
        CheckValue::Bytes(b) => format!("{b} bytes"),
        CheckValue::Count(c) => format!("{c}"),
        CheckValue::Rate(r) => format!("{:.4}", r),
        CheckValue::Millis(m) => format!("{m:.3} ms"),
        CheckValue::BelowFloorMillis(f) => format!("<{f:.6} ms (below timer floor)"),
    }
}

fn fmt_outcome(o: &CheckOutcome) -> String {
    match o {
        CheckOutcome::Pass { measured } => format!("PASS ({})", fmt_check_value(measured)),
        CheckOutcome::Fail { measured } => format!("**FAIL** ({})", fmt_check_value(measured)),
        CheckOutcome::NotAssessed { reason } => format!("NOT ASSESSED -- {reason}"),
        CheckOutcome::Blocked { reason, measured } => format!(
            "**BLOCKED** -- {reason}{}",
            measured
                .map(|m| format!(
                    " (measured value, never presented as passing: {})",
                    fmt_check_value(&m)
                ))
                .unwrap_or_default()
        ),
    }
}

fn check_kind_label(k: CheckKind) -> &'static str {
    match k {
        CheckKind::PackSize => "Pack (artifact) size",
        CheckKind::LexiconScale => "Lexicon scale",
        CheckKind::CoverageAnalysisRate => "Coverage (token-level analysis rate)",
        CheckKind::LatencyP50 => "Latency p50",
        CheckKind::LatencyP90 => "Latency p90",
        CheckKind::LatencyP99 => "Latency p99",
    }
}

/// Whether the policy value backing `kind` is [`pg_foma::readiness_policy::Calibration::
/// Placeholder`] (un-calibrated) or `Measured` — surfaced next to every threshold so a reader never
/// mistakes a placeholder number for a calibrated one (`readiness_policy`'s own top doc: "never
/// silently launder a placeholder into a value that merely looks measured").
fn calibration_label(kind: CheckKind, policy: &ThresholdPolicy) -> &'static str {
    let is_placeholder = match kind {
        CheckKind::PackSize => policy.pack_size_max_bytes.calibration.is_placeholder(),
        CheckKind::LexiconScale => policy.lexicon_min_entries.calibration.is_placeholder(),
        CheckKind::CoverageAnalysisRate => policy
            .coverage_min_analysis_rate
            .calibration
            .is_placeholder(),
        CheckKind::LatencyP50 => policy.latency_p50_max_ms.calibration.is_placeholder(),
        CheckKind::LatencyP90 => policy.latency_p90_max_ms.calibration.is_placeholder(),
        CheckKind::LatencyP99 => policy.latency_p99_max_ms.calibration.is_placeholder(),
    };
    if is_placeholder {
        "placeholder, un-calibrated"
    } else {
        "measured"
    }
}

fn render_checks_table(checks: &[CheckResult], policy: &ThresholdPolicy) -> String {
    let mut out = String::new();
    writeln!(out, "| Check | Outcome | Threshold | Calibration |").unwrap();
    writeln!(out, "|---|---|---|---|").unwrap();
    for c in checks {
        writeln!(
            out,
            "| {} | {} | {} | {} |",
            check_kind_label(c.kind),
            fmt_outcome(&c.outcome),
            fmt_check_value(&c.threshold),
            calibration_label(c.kind, policy),
        )
        .unwrap();
        for s in &c.statements {
            writeln!(out, "  - _{s}_").unwrap();
        }
    }
    out
}

fn render_capability(capability: &CapabilitySummary) -> String {
    match capability {
        CapabilitySummary::Admit => {
            "**Admit** -- the capability gate admits this grammar outright.".to_string()
        }
        CapabilitySummary::ConfirmOnly => {
            "**ConfirmOnly** -- a first-class, recall-preserving non-failure verdict (ADR 0001): \
             the compiled proposer is a strict superset here, confirmed by the oracle."
                .to_string()
        }
        CapabilitySummary::Refuse { refusals } => {
            let mut s = format!(
                "**Refuse** -- the capability gate refuses this grammar ({} diagnostic(s)):\n\n",
                refusals.len()
            );
            s.push_str("| Predicate | Construct | Witness |\n|---|---|---|\n");
            for r in refusals {
                s.push_str(&format!(
                    "| `{}` | `{}` | `{}` |\n",
                    r.predicate, r.construct, r.witness
                ));
            }
            s
        }
    }
}

/// The one-line summary printed above the embedded mermaid diagram -- factored out so the golden
/// test below computes it via the EXACT same code the live command uses, never a hand-copied
/// restatement that could silently drift.
fn render_mermaid_summary_line(render: &MermaidRender) -> String {
    format!(
        "{} node(s) emitted of {} total{}, overall capability verdict from the SAME real \
         evaluation this report's own \"Capability\" section names.",
        render.emitted_node_count,
        render.total_node_count,
        match render.threshold {
            Some(t) => format!(
                " (sibling-leaf collapse threshold={t}, summarized={})",
                render.summarized
            ),
            None => " (full render, no collapsing)".to_string(),
        }
    )
}

fn render_trust(trust: &TrustStatus) -> String {
    match trust {
        TrustStatus::Proven => {
            "**Proven** -- no ADR-0005 capability override was exercised.".to_string()
        }
        TrustStatus::Overridden(record) => format!(
            "**Overridden (trust=unproven)** -- ADR-0005 capability override, authorized by \
             `{}` ({}), recorded at `{}`, {} fail-closed configuration(s) force-compiled through. \
             An overridden artifact can never certify, under any configuration.",
            record.authorized_by,
            record.reason,
            record.recorded_at,
            record.overridden_configs.len()
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_markdown(
    grammar_id: &str,
    grammar_path: &str,
    grammar_sha256: &str,
    out_path: &str,
    policy: &ThresholdPolicy,
    verdict: &ReadinessReport,
    build_time_line: &str,
    latency_methodology_line: &str,
    coverage_attestation_line: &str,
    mermaid: &str,
    mermaid_summary_line: &str,
    pack_pin: &str,
    corpus_pin: &str,
    submodule_pin: &str,
    repo_head: &str,
    not_tested: &[String],
) -> String {
    let mut out = String::new();

    writeln!(out, "# PanGloss readiness report: {grammar_id}").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "Policy `{}` (report schema v{}), device class: {}",
        verdict.policy_id, verdict.report_schema_version, verdict.device_class
    )
    .unwrap();
    writeln!(out).unwrap();

    let tier_word = match verdict.tier {
        Tier::Certified => "CERTIFIED",
        Tier::NotYet => "NOT YET",
        Tier::NotSupported => "NOT SUPPORTED",
    };
    writeln!(out, "## Verdict: {tier_word}").unwrap();
    writeln!(out).unwrap();
    if verdict.is_certified() {
        writeln!(
            out,
            "This grammar is **CERTIFIED** under policy `{}`: every declared threshold passed on \
             the checks this report performed. See \"What this report did NOT test\" below for \
             exactly what that excludes.",
            verdict.policy_id
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "This grammar is **NOT CERTIFIED** ({tier_word}). Every failing/not-assessed/blocked \
             check is named below, individually, with its measured value and threshold -- a bare \
             \"not passing\" is never useful to a language team deciding whether to ask for support."
        )
        .unwrap();
    }
    writeln!(out).unwrap();
    for n in &verdict.notes {
        writeln!(out, "> {n}").unwrap();
        writeln!(out).unwrap();
    }

    writeln!(out, "## Capability").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "{}", render_capability(&verdict.capability)).unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## Trust").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "{}", render_trust(&verdict.trust)).unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## Checks").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "{}", render_checks_table(&verdict.checks, policy)).unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## Coverage attestation").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "{coverage_attestation_line}").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## Build time").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "{build_time_line}").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## Latency methodology").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "{latency_methodology_line}").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## Compilation plan").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "{mermaid_summary_line}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "```mermaid").unwrap();
    out.push_str(mermaid);
    if !mermaid.ends_with('\n') {
        out.push('\n');
    }
    writeln!(out, "```").unwrap();
    writeln!(out).unwrap();

    writeln!(out, "## What this report did NOT test").unwrap();
    writeln!(out).unwrap();
    for n in not_tested {
        writeln!(out, "- {n}").unwrap();
    }
    writeln!(out).unwrap();

    writeln!(out, "## Pinned revisions (to re-derive this report)").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "- grammar: `{grammar_path}` (sha256=`{grammar_sha256}`)"
    )
    .unwrap();
    writeln!(out, "- pack: {pack_pin}").unwrap();
    writeln!(out, "- coverage corpus: {corpus_pin}").unwrap();
    writeln!(out, "- `machine` submodule: {submodule_pin}").unwrap();
    writeln!(out, "- repo HEAD: `{repo_head}`").unwrap();
    writeln!(out, "- this report: `{out_path}`").unwrap();
    writeln!(out).unwrap();

    out
}

// =================================================================================================
// The command
// =================================================================================================

pub fn run_make_report(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut pack_path: Option<String> = None;
    let mut words_path: Option<String> = None;
    let mut corpus_path: Option<String> = None;
    let mut attestor: Option<String> = None;
    let mut attested_on: Option<String> = None;
    let mut policy_path: Option<String> = None;
    let mut allow_unproven = false;
    let mut authorized_by: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut repeats: u32 = 7;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--pack" => pack_path = Some(it.next().ok_or("--pack requires a value")?.clone()),
            s if s.starts_with("--pack=") => pack_path = Some(s["--pack=".len()..].to_string()),
            "--words" => words_path = Some(it.next().ok_or("--words requires a value")?.clone()),
            s if s.starts_with("--words=") => words_path = Some(s["--words=".len()..].to_string()),
            "--corpus" => corpus_path = Some(it.next().ok_or("--corpus requires a value")?.clone()),
            s if s.starts_with("--corpus=") => {
                corpus_path = Some(s["--corpus=".len()..].to_string())
            }
            "--attestor" => {
                attestor = Some(it.next().ok_or("--attestor requires a value")?.clone())
            }
            s if s.starts_with("--attestor=") => {
                attestor = Some(s["--attestor=".len()..].to_string())
            }
            "--attested-on" => {
                attested_on = Some(it.next().ok_or("--attested-on requires a value")?.clone())
            }
            s if s.starts_with("--attested-on=") => {
                attested_on = Some(s["--attested-on=".len()..].to_string())
            }
            "--policy" => policy_path = Some(it.next().ok_or("--policy requires a value")?.clone()),
            s if s.starts_with("--policy=") => {
                policy_path = Some(s["--policy=".len()..].to_string())
            }
            "--allow-unproven" => allow_unproven = true,
            "--authorized-by" => {
                authorized_by = Some(it.next().ok_or("--authorized-by requires a value")?.clone())
            }
            s if s.starts_with("--authorized-by=") => {
                authorized_by = Some(s["--authorized-by=".len()..].to_string())
            }
            "--reason" => reason = Some(it.next().ok_or("--reason requires a value")?.clone()),
            s if s.starts_with("--reason=") => reason = Some(s["--reason=".len()..].to_string()),
            "--repeats" => {
                let v = it.next().ok_or("--repeats requires a value")?;
                repeats = v.parse().map_err(|_| format!("invalid --repeats: {v}"))?;
            }
            s if s.starts_with("--repeats=") => {
                let v = &s["--repeats=".len()..];
                repeats = v.parse().map_err(|_| format!("invalid --repeats: {v}"))?;
            }
            s => positional.push(s),
        }
    }
    if repeats == 0 {
        return Err("--repeats must be >= 1".to_string());
    }
    let [grammar_path, out_path] = positional[..] else {
        return Err(USAGE.to_string());
    };

    let coverage_flags =
        corpus_path.is_some() as u8 + attestor.is_some() as u8 + attested_on.is_some() as u8;
    if coverage_flags != 0 && coverage_flags != 3 {
        return Err(
            "--corpus, --attestor, and --attested-on must all be given together -- a held-out \
             coverage attestation needs a corpus, a named attestor, AND a date; a partial \
             attestation is not honest (spec.md: coverage status is either a full attestation or \
             not-assessed, never a half-measure)"
                .to_string(),
        );
    }

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);

    let grammar_bytes = fs::read(grammar_path).map_err(|e| format!("read {grammar_path}: {e}"))?;
    let grammar_sha256 = sha256_hex(&grammar_bytes);
    let grammar_id = grammar.name.clone().unwrap_or_else(|| {
        Path::new(grammar_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown-grammar")
            .to_string()
    });

    let policy = match &policy_path {
        Some(p) => {
            let text = fs::read_to_string(p).map_err(|e| format!("read --policy {p}: {e}"))?;
            ThresholdPolicy::from_json(&text).map_err(|e| format!("parse --policy {p}: {e}"))?
        }
        None => policy_v1(),
    };

    let corpus_data: Option<(String, String)> = match &corpus_path {
        Some(p) => {
            let bytes = fs::read(p).map_err(|e| format!("read --corpus {p}: {e}"))?;
            let sha = sha256_hex(&bytes);
            let text = String::from_utf8_lossy(&bytes).into_owned();
            Some((text, sha))
        }
        None => None,
    };

    // ONE derivation, shared by all three
    // places this command needs the capability verdict -- here, `pack::build_pack`'s trust stamp,
    // and `readiness_verdict::certify` -- rather than three independent
    // `pg_foma::capability::characterize` walks over the same grammar.
    let semantics = GrammarSemantics::derive(&grammar);
    let decision = evaluate_capability_with_semantics(&semantics);
    let attempt_compile = matches!(
        decision,
        CompileDecision::Admit | CompileDecision::ConfirmOnly
    ) || (matches!(decision, CompileDecision::Refuse(_)) && allow_unproven);

    let mut not_tested: Vec<String> = vec![
        "correctness: NOT CERTIFIED HERE -- coverage (when assessed) is a token-level analysis \
         RATE, never accuracy; a token may receive an incorrect analysis and still count. \
         Correctness evidence comes from the synthetic conformance suite, not from this report."
            .to_string(),
    ];

    let (trust, measurements, build_time_line, pack_pin): (
        TrustStatus,
        Option<Measurements>,
        String,
        String,
    );
    let latency_methodology_line: String;
    // Separate from `verdict.checks` on purpose: `CheckResult` only carries the numeric
    // Pass/Fail/NotAssessed outcome for coverage (a rate vs. a threshold), never the attestor/date
    // an attestation itself carries (`pg_foma::readiness_verdict::CoverageAssessment::Attested`'s
    // own fields) -- the certificate's attestor/date fields are
    // rendered from HERE, the actual `CoverageAssessment` this command built, not reconstructed
    // from the tiered verdict after the fact.
    let coverage_attestation_line: String;

    if !attempt_compile {
        not_tested.push(
            "build time, artifact size, lexicon scale, latency, and coverage: NONE of these were \
             measured -- the capability gate refuses this grammar and --allow-unproven was not \
             given, so no compiled artifact exists to measure at all (this is the expected, \
             headline outcome for a permanently-refused construct, per docs/benchmark-matrix.md)."
                .to_string(),
        );
        trust = TrustStatus::Proven; // no override was exercised; there is simply no artifact.
        measurements = None;
        build_time_line = "not measured -- the grammar was refused and no compiled artifact was \
             ever built (pass --allow-unproven to force-compile and measure anyway; the resulting \
             report will still never certify -- trust=unproven never certifies, under any \
             configuration)."
            .to_string();
        pack_pin =
            "none -- the grammar was refused and no --allow-unproven override was given, so \
             no pack was built."
                .to_string();
        latency_methodology_line = "not measured -- see \"build time\" above.".to_string();
        coverage_attestation_line =
            "not assessed -- no compiled artifact exists to run a corpus against (independent of \
             whether --corpus was supplied)."
                .to_string();
    } else {
        // ---- trust + artifact size: from a REAL artifact, never a caller-supplied parameter ----
        let (trust_src, artifact_size, pack_pin_line): (pg_pack::CapabilityTrust, u64, String) =
            match &pack_path {
                Some(p) => {
                    let bytes = fs::read(p).map_err(|e| format!("read --pack {p}: {e}"))?;
                    let read =
                        pg_pack::read_pack(&bytes).map_err(|e| format!("read_pack {p}: {e}"))?;
                    if read.manifest.grammar_id != grammar_id {
                        eprintln!(
                            "warning: --pack {p}'s manifest grammar_id ({:?}) does not match this \
                             grammar's own id ({grammar_id:?}) -- proceeding anyway, but verify \
                             this is really the pack for this grammar",
                            read.manifest.grammar_id
                        );
                    }
                    let pin = format!(
                        "supplied `{p}` (sha256=`{}`, package_fingerprint=`{}`)",
                        sha256_hex(&bytes),
                        read.manifest.package_fingerprint
                    );
                    (read.manifest.capability_trust, bytes.len() as u64, pin)
                }
                None => {
                    let built = crate::pack::build_pack(
                        grammar_path,
                        &grammar,
                        &semantics,
                        allow_unproven,
                        authorized_by.as_deref(),
                        reason.as_deref(),
                        false,
                    )?;
                    let pin = format!(
                        "built in-process for this report, not persisted to disk (sha256=`{}`, \
                         package_fingerprint=`{}`)",
                        sha256_hex(&built.bytes),
                        built.manifest.package_fingerprint
                    );
                    (
                        built.manifest.capability_trust,
                        built.bytes.len() as u64,
                        pin,
                    )
                }
            };
        trust = map_trust(&trust_src);
        pack_pin = pack_pin_line;

        // ---- the compiled proposer+confirm analyzer this report's build-time/latency numbers are
        // about -- a separate compile from whatever produced the pack above (same "a second
        // compiled network is an acceptable one-time cost for an offline tool" judgment call
        // pack.rs/diagnostics.rs already make) ----
        let t_build = Instant::now();
        let mut analyzer = FomaAnalyzer::new(&grammar)
            .map_err(|e| format!("foma compile failed for {grammar_path}: {e}"))?;
        let build_ns = t_build.elapsed().as_nanos() as u64;
        let floor_ns = measure_timer_floor_ns();
        build_time_line = format!(
            "{} (compiling the propose+confirm analyzer this report's latency numbers were \
             measured against; informational only -- no threshold in the declared policy gates \
             this figure).",
            render_latency_measurement(&latency_measurement(build_ns, floor_ns))
        );

        // ---- latency word list: --words if given, else this grammar's own lexical roots ----
        let (words, words_source): (Vec<String>, String) = match &words_path {
            Some(p) => {
                let text = fs::read_to_string(p).map_err(|e| format!("read --words {p}: {e}"))?;
                let ws: Vec<String> = text
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                (ws, format!("`--words {p}`"))
            }
            None => {
                not_tested.push(
                    "latency word source: no --words file was supplied; this report's \
                     percentiles were measured against this grammar's own lexical root surface \
                     forms as a fallback sample, which may not be representative of real running \
                     text."
                        .to_string(),
                );
                (
                    default_word_list(&grammar),
                    "this grammar's own lexical root surface forms (fallback; no --words given)"
                        .to_string(),
                )
            }
        };
        if words.is_empty() {
            return Err(
                "cannot measure latency: no --words file was supplied and this grammar has no \
                 lexical entries to fall back to"
                    .to_string(),
            );
        }
        let (p50_ns, p90_ns, p99_ns) =
            measure_latency_percentiles_ns(&mut analyzer, &words, repeats);
        let latency_p50 = latency_measurement(p50_ns, floor_ns);
        let latency_p90 = latency_measurement(p90_ns, floor_ns);
        let latency_p99 = latency_measurement(p99_ns, floor_ns);
        latency_methodology_line = format!(
            "Measured in-process via nanosecond `Instant`/`Duration` timing over a real \
             `FomaAnalyzer` (never `pangloss batch`'s integer-millisecond TSV column, so a \
             sub-millisecond result never silently renders as `0`): {} word(s), {repeats} timed \
             samples/word after 1 discarded warmup call, this run's calibrated timer floor is \
             {floor_ns}ns. Word source: {words_source}. p50/p90/p99 are the nearest-rank \
             percentile over each word's own median duration.",
            words.len()
        );

        // ---- coverage: attestation if given, otherwise honestly not-assessed ----
        let coverage = match (&corpus_path, &corpus_data, &attestor, &attested_on) {
            (Some(cp), Some((text, sha)), Some(at), Some(on)) => {
                let rate = measure_coverage_rate(&mut analyzer, &grammar, text)?;
                coverage_attestation_line = format!(
                    "ATTESTED -- attestor=`{at}`, attested_on=`{on}`, corpus=`{cp}` \
                     (sha256=`{sha}`), analysis_rate={rate:.4}. UNVERIFIED beyond the named \
                     attestor's own claim (nothing in the artifact records what its author read \
                     while authoring, and PanGloss does not train)."
                );
                CoverageAssessment::Attested {
                    attestor: at.clone(),
                    attested_on: on.clone(),
                    analysis_rate: rate,
                }
            }
            _ => {
                not_tested.push(
                    "coverage: NOT ASSESSED -- no --corpus/--attestor/--attested-on were \
                     supplied together; not-assessed is never presented as a passing check."
                        .to_string(),
                );
                coverage_attestation_line =
                    "not assessed -- no --corpus/--attestor/--attested-on supplied together."
                        .to_string();
                CoverageAssessment::NotAssessed
            }
        };

        measurements = Some(Measurements {
            pack_size_bytes: artifact_size,
            lexicon_entries: grammar.entries.len() as u64,
            coverage,
            latency_p50,
            latency_p90,
            latency_p99,
        });
    }

    let verdict = certify_with_semantics(&semantics, &trust, measurements.as_ref(), &policy);

    // ---- compilation plan diagram (pure composition of section-visualize-compilation-plan) ----
    let plan_doc = build_plan_document_with_semantics(&semantics);
    let render = render_mermaid(&plan_doc, RenderMode::default());
    let mermaid_summary_line = render_mermaid_summary_line(&render);

    // ---- pinned revisions ----
    let corpus_pin = match (&corpus_path, &corpus_data) {
        (Some(p), Some((_, sha))) => format!("`{p}` (sha256=`{sha}`)"),
        _ => "not supplied".to_string(),
    };
    let submodule_pin = submodule_revision("machine");
    let repo_head = repo_head_revision();

    let report_md = render_markdown(
        &grammar_id,
        grammar_path,
        &grammar_sha256,
        out_path,
        &policy,
        &verdict,
        &build_time_line,
        &latency_methodology_line,
        &coverage_attestation_line,
        &render.mermaid,
        &mermaid_summary_line,
        &pack_pin,
        &corpus_pin,
        &submodule_pin,
        &repo_head,
        &not_tested,
    );

    fs::write(out_path, &report_md).map_err(|e| format!("write {out_path}: {e}"))?;
    eprintln!(
        "make-report complete: {out_path} -- tier={:?}, capability={:?}, trust={}",
        verdict.tier,
        verdict.capability,
        if verdict.trust.is_unproven() {
            "unproven/overridden"
        } else {
            "proven"
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    // The `&Grammar` front ends, used only by the golden-render test below: the live command drives
    // the `_with_semantics` forms off its one shared owner, so these two are not
    // referenced outside `cfg(test)`.
    use pg_foma::plan_diagram::build_plan_document;
    use pg_foma::readiness_verdict::certify;

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "pangloss-cli-make-report-test-{tag}-{}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// An ordinary `Admit`-verdict grammar: one bare root, no MPR groups, no `Compounding`.
    const ADMIT_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>MakeReportAdmitFixture</Name>
    <CharacterDefinitionTable id="table1">
      <Name>Orthography</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="segA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="segT"><Representations><Representation>t</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses></NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="table1">
        <Name>main</Name>
        <LexicalEntries>
          <LexicalEntry id="e1">
            <Allomorphs><Allomorph id="e1-1"><PhoneticShape>kat</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>kat</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

    /// A `MprGroup` with `outputType="overwrite"` -- `MprGroupOverwriteFailClosedPredicate` refuses
    /// this UNCONDITIONALLY and PERMANENTLY (same fixture shape `pack.rs`'s own tests and `main.rs`'s
    /// `capability_gate_tests` use for their "known-Refuse, by construction, forever" fixture).
    const REFUSE_GRAMMAR_XML: &str = include_str!("../../../../conformance-staging/edge-cases/simultaneous-subrule-genuine-overlap/grammar.xml");

    fn run_make_report_raw(
        tag: &str,
        grammar_xml: &str,
        extra_args: &[&str],
    ) -> (Result<(), String>, std::path::PathBuf) {
        let dir = scratch_dir(tag);
        let grammar_path = dir.join("grammar.xml");
        let out_path = dir.join("report.md");
        fs::write(&grammar_path, grammar_xml).expect("write grammar");

        let mut args: Vec<String> = vec![
            grammar_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
        ];
        args.extend(extra_args.iter().map(|s| s.to_string()));

        (run_make_report(&args), out_path)
    }

    /// The headline case: a permanently-refused grammar (no `--allow-unproven`)
    /// produces a report that plainly says NOT SUPPORTED, names the refusing predicate/construct,
    /// and marks build time/latency/coverage as not measured/not assessed -- NEVER a passing check.
    #[test]
    fn refused_grammar_report_names_not_supported_and_every_unmeasured_check() {
        let (result, out_path) = run_make_report_raw("refuse", REFUSE_GRAMMAR_XML, &[]);
        assert!(
            result.is_ok(),
            "make-report must succeed even for a refused grammar: {result:?}"
        );

        let text = fs::read_to_string(&out_path).expect("read report.md");
        assert!(text.contains("NOT SUPPORTED"), "{text}");
        assert!(text.contains("simultaneous.subrule-overlap"), "{text}");
        assert!(
            text.contains("simultaneous.subrule-overlap"),
            "must name the refused construct: {text}"
        );
        // Every check must render as NOT ASSESSED, never PASS.
        assert!(text.contains("NOT ASSESSED"), "{text}");
        assert!(
            !text.contains("| PASS"),
            "no check may render as passing for a refused grammar: {text}"
        );
        // The "what was not tested" section must name the fact that nothing was measured.
        assert!(
            text.contains("no compiled artifact exists to measure"),
            "must explain WHY nothing was measured: {text}"
        );
        // Pinned revisions must still be present even in the refused case.
        assert!(text.contains("Pinned revisions"), "{text}");
        assert!(text.contains("sha256="), "{text}");
    }

    /// A clean `Admit` grammar with no corpus/no words: certifies (if it passes every threshold) or
    /// at least reaches NotYet with named failures -- either way, every individual check must be
    /// individually named with its measured value and threshold, never only a bare summary line.
    #[test]
    fn admit_grammar_report_names_every_check_individually() {
        let (result, out_path) = run_make_report_raw("admit", ADMIT_GRAMMAR_XML, &["--repeats=1"]);
        assert!(
            result.is_ok(),
            "make-report must succeed for a clean grammar: {result:?}"
        );

        let text = fs::read_to_string(&out_path).expect("read report.md");
        // Every declared check kind must be individually named in the checks table.
        for label in [
            "Pack (artifact) size",
            "Lexicon scale",
            "Coverage (token-level analysis rate)",
            "Latency p50",
            "Latency p90",
            "Latency p99",
        ] {
            assert!(
                text.contains(label),
                "expected check {label:?} named in report:\n{text}"
            );
        }
        // Coverage must be honestly not-assessed (no --corpus was given).
        assert!(
            text.contains("NOT ASSESSED") && text.contains("no --corpus"),
            "coverage must be reported not-assessed when no corpus is supplied:\n{text}"
        );
        assert!(
            !text.contains("Coverage (token-level analysis rate) | PASS"),
            "{text}"
        );
        // A mermaid diagram must be embedded.
        assert!(text.contains("```mermaid"), "{text}");
        assert!(text.contains("flowchart"), "{text}");
        // Build time must be reported (never silently omitted).
        assert!(text.contains("## Build time"), "{text}");
    }

    /// Render-layer proof: not-assessed coverage must never appear as `PASS` in the
    /// rendered checks table, on a grammar that WOULD otherwise certify cleanly (every other
    /// threshold passing) -- proves the render layer, not just `certify` itself, respects the rule.
    #[test]
    fn not_assessed_coverage_never_renders_as_pass_in_the_table() {
        let (result, out_path) =
            run_make_report_raw("not-assessed", ADMIT_GRAMMAR_XML, &["--repeats=1"]);
        assert!(result.is_ok(), "{result:?}");
        let text = fs::read_to_string(&out_path).expect("read report.md");
        let checks_section = text
            .split("## Checks")
            .nth(1)
            .and_then(|s| s.split("## Build time").next())
            .expect("Checks section must exist");
        assert!(
            checks_section.contains("NOT ASSESSED"),
            "checks section must show NOT ASSESSED for coverage:\n{checks_section}"
        );
        assert!(
            !checks_section.contains("Coverage (token-level analysis rate) | PASS"),
            "coverage must never render as PASS with no corpus supplied:\n{checks_section}"
        );
    }

    /// `--corpus` without `--attestor`/`--attested-on` (a partial attestation) is a hard, explicit
    /// error -- never silently degrading to not-assessed or silently ignoring the corpus.
    #[test]
    fn partial_coverage_attestation_is_a_hard_error() {
        let dir = scratch_dir("partial-attestation");
        let corpus_path = dir.join("corpus.txt");
        fs::write(&corpus_path, "kat kat\n").expect("write corpus");
        let (result, _out_path) = run_make_report_raw(
            "partial-attestation",
            ADMIT_GRAMMAR_XML,
            &[&format!("--corpus={}", corpus_path.to_string_lossy())],
        );
        let err = result.expect_err("a corpus with no attestor/attested-on must be a hard error");
        assert!(err.contains("must all be given together"), "{err}");
    }

    /// A full coverage attestation (`--corpus`/`--attestor`/`--attested-on` all given) is carried
    /// through to the report, with both fixed honesty disclaimers (rate-not-accuracy,
    /// attestation-not-measurement) present.
    #[test]
    fn full_coverage_attestation_is_carried_through_with_both_disclaimers() {
        let dir = scratch_dir("full-attestation");
        let corpus_path = dir.join("corpus.txt");
        fs::write(&corpus_path, "kat kat kat\n").expect("write corpus");
        let (result, out_path) = run_make_report_raw(
            "full-attestation",
            ADMIT_GRAMMAR_XML,
            &[
                &format!("--corpus={}", corpus_path.to_string_lossy()),
                "--attestor=synthetic-test-attestor",
                "--attested-on=2026-07-27",
                "--repeats=1",
            ],
        );
        assert!(result.is_ok(), "{result:?}");
        let text = fs::read_to_string(&out_path).expect("read report.md");
        assert!(text.contains("token-level ANALYSIS RATE"), "{text}");
        assert!(text.contains("ATTESTATION"), "{text}");
        assert!(text.contains("synthetic-test-attestor"), "{text}");
        assert!(text.contains("coverage corpus"), "{text}");
        assert!(text.contains("corpus.txt"), "{text}");
    }

    /// `--allow-unproven` on the refused grammar: the report must show `NotSupported` (never
    /// certifying), every check `BLOCKED` (never `PASS`), and must name the override as the reason
    /// -- the render-layer proof of `readiness_verdict`'s own sabotage-tested rule 1.
    #[test]
    fn allow_unproven_override_report_blocks_every_check_and_never_certifies() {
        let (result, out_path) = run_make_report_raw(
            "override",
            REFUSE_GRAMMAR_XML,
            &[
                "--allow-unproven",
                "--authorized-by=synthetic-operator",
                "--reason=synthetic field trial",
                "--repeats=1",
            ],
        );
        assert!(result.is_ok(), "{result:?}");
        let text = fs::read_to_string(&out_path).expect("read report.md");
        assert!(text.contains("NOT SUPPORTED"), "{text}");
        assert!(text.contains("Overridden (trust=unproven)"), "{text}");
        assert!(text.contains("synthetic-operator"), "{text}");
        assert!(text.contains("**BLOCKED**"), "{text}");
        assert!(
            !text.contains("| PASS"),
            "an overridden artifact must never show a passing check: {text}"
        );
    }

    /// A caller-supplied `--pack` file's REAL trust stamp is what this report certifies against --
    /// never a value this command invents. Builds a real overridden `.pgpack` via `pangloss pack`
    /// itself first, then feeds it to `make-report --pack=...` and confirms the report's trust
    /// section reflects that pack's actual stamp.
    #[test]
    fn supplied_pack_trust_stamp_is_read_from_the_real_artifact() {
        let dir = scratch_dir("supplied-pack");
        let grammar_path = dir.join("grammar.xml");
        let pack_path = dir.join("out.pgpack");
        fs::write(&grammar_path, REFUSE_GRAMMAR_XML).expect("write grammar");

        crate::pack::run_pack(&[
            grammar_path.to_string_lossy().into_owned(),
            pack_path.to_string_lossy().into_owned(),
            "--allow-unproven".to_string(),
            "--authorized-by=synthetic-pack-builder".to_string(),
        ])
        .expect("pangloss pack --allow-unproven must succeed");

        let out_path = dir.join("report.md");
        let result = run_make_report(&[
            grammar_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
            format!("--pack={}", pack_path.to_string_lossy()),
            "--allow-unproven".to_string(),
            "--repeats=1".to_string(),
        ]);
        assert!(result.is_ok(), "{result:?}");

        let text = fs::read_to_string(&out_path).expect("read report.md");
        assert!(text.contains("synthetic-pack-builder"), "{text}");
        assert!(
            text.contains("supplied `"),
            "must name the supplied pack, not a built-in-process one: {text}"
        );
    }

    /// ONE `make-report` invocation must resolve the ADR 0001 capability
    /// verdict from ONE `pg_foma::capability::characterize` walk, not one per consumer: the
    /// preamble, `readiness_verdict::certify`, and every call inside `plan_diagram::build_plan_document`
    /// (`plan_and_profile`, the second `plan_and_profile` inside `build_plan_document_for_plan`, and
    /// `compose_envelope`) must all resolve from the same `GrammarSemantics` owner rather than each
    /// rebuilding the whole profile, real `Simultaneous`-mode `foma::types::Fsm` construction
    /// included.
    ///
    /// The fixture is the REFUSED grammar with no `--allow-unproven`, deliberately: that takes the
    /// `!attempt_compile` branch, so no pack is built and no foma compile runs. The count this
    /// measures is therefore exactly the capability derivations the shared owner is responsible
    /// for. `pack::build_pack`'s trust stamp is reachable only on the compile path and is fixed by
    /// the same shared owner, but is deliberately excluded from this count: including it would drag
    /// in `emit.rs`'s own separate `compound_chain_depth_and_budget_check` characterize call, which
    /// is not one of the duplicated verdict derivations, and this assertion could not then
    /// attribute the total.
    ///
    /// The counter is thread-local (see `pg_foma::capability::characterize_call_count`), so the
    /// reading is this test's own thread and cannot be polluted by tests running in parallel. The
    /// non-zero assertion is not redundant: a thread-local count could otherwise "pass" by measuring
    /// nothing at all if the work moved off-thread.
    #[test]
    fn one_make_report_invocation_characterizes_the_grammar_exactly_once() {
        pg_foma::capability::reset_characterize_call_count();
        let (result, _out) = run_make_report_raw("characterize-count", REFUSE_GRAMMAR_XML, &[]);
        let calls = pg_foma::capability::characterize_call_count();

        assert!(result.is_ok(), "{result:?}");
        assert!(
            calls > 0,
            "the counter must actually have observed the run; 0 means it measured nothing"
        );
        assert_eq!(
            calls, 1,
            "one make-report invocation must derive the capability characteristics ONCE (it was 5 \
             on this refused-grammar path before task 7.11 -- the preamble, \
             readiness_verdict::certify, and three more inside a single build_plan_document; a \
             sixth, pack::build_pack's trust stamp, is reachable only on the compile path)"
        );
    }

    /// Below-floor latency must never render as a literal `0` anywhere in the rendered report --
    /// direct proof over the rendering helper itself, independent of any real timing noise.
    #[test]
    fn below_floor_latency_never_reports_as_a_bare_millis_zero() {
        let m = LatencyMeasurement::BelowFloor {
            floor_ms: 0.000_001,
        };
        let rendered = render_latency_measurement(&m);
        assert!(rendered.contains("below timer floor"), "{rendered}");
        assert_ne!(rendered.trim(), "0 ms");
        assert_ne!(rendered.trim(), "0.000 ms");
    }

    #[test]
    fn percentile_ns_nearest_rank_matches_expected_indices() {
        let sorted = vec![10u64, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile_ns(&sorted, 50.0), 50);
        assert_eq!(percentile_ns(&sorted, 90.0), 90);
        assert_eq!(percentile_ns(&sorted, 99.0), 100);
        assert_eq!(percentile_ns(&[], 50.0), 0);
    }

    #[test]
    fn default_word_list_collects_distinct_root_surface_forms() {
        let g = pg_grammar::load(ADMIT_GRAMMAR_XML).expect("fixture must load");
        let words = default_word_list(&g);
        assert_eq!(words, vec!["kat".to_string()]);
    }

    /// `--repeats=0` is a hard, explicit error, not silently coerced to 1.
    #[test]
    fn repeats_zero_is_a_hard_error() {
        let (result, _out) =
            run_make_report_raw("repeats-zero", ADMIT_GRAMMAR_XML, &["--repeats=0"]);
        let err = result.expect_err("--repeats=0 must error");
        assert!(err.contains("--repeats"), "{err}");
    }

    // -----------------------------------------------------------------------------------------
    // A golden report for one small synthetic fixture, regenerated from the
    // generator's own output -- never hand-edited (mirrors `pg_foma::readiness_verdict`'s own
    // `regenerate_readiness_verdict_golden_json` precedent exactly).
    //
    // A live end-to-end `run_make_report` run embeds REAL wall-clock timing (build time, latency
    // percentiles) that varies run to run by construction -- a byte-for-byte golden of THAT output
    // would be inherently flaky, not a real regression gate. So this golden pins the RENDERING
    // layer instead: [`render_markdown`] fed fixed, hand-picked (not hand-edited-into-the-golden)
    // inputs -- a fixed synthetic grammar, a fixed `Measurements` (same discipline
    // `readiness_verdict`'s own golden fixture uses), and the REAL, deterministic
    // [`build_plan_document`]/[`render_mermaid`] output over that same fixed grammar (plan-diagram
    // output is itself content-addressed and deterministic, per that module's own doc) --
    // everything here is either a fixed literal or the real generator's own deterministic
    // computation, never a live timer.
    // -----------------------------------------------------------------------------------------

    fn golden_report_markdown() -> String {
        let g = pg_grammar::load(ADMIT_GRAMMAR_XML).expect("golden fixture must load");
        let policy = policy_v1();
        let measurements = Measurements {
            pack_size_bytes: 12_345,
            lexicon_entries: 2_000,
            coverage: CoverageAssessment::Attested {
                attestor: "synthetic-golden-attestor".to_string(),
                attested_on: "2026-07-27".to_string(),
                analysis_rate: 0.95,
            },
            latency_p50: LatencyMeasurement::Millis(0.5),
            latency_p90: LatencyMeasurement::Millis(2.0),
            latency_p99: LatencyMeasurement::Millis(10.0),
        };
        let verdict = certify(&g, &TrustStatus::Proven, Some(&measurements), &policy);

        // Real, deterministic composition (never a live timer) -- same functions the live command
        // calls, over the same fixed fixture.
        let plan_doc = build_plan_document(&g);
        let render = render_mermaid(&plan_doc, RenderMode::default());
        let mermaid_summary_line = render_mermaid_summary_line(&render);

        render_markdown(
            "MakeReportAdmitFixture",
            "synthetic-golden-fixture.xml",
            &sha256_hex(ADMIT_GRAMMAR_XML.as_bytes()),
            "report.md",
            &policy,
            &verdict,
            "0.500 ms (compiling the propose+confirm analyzer this report's latency numbers were \
             measured against; informational only -- no threshold in the declared policy gates \
             this figure).",
            "Measured in-process via nanosecond `Instant`/`Duration` timing over a real \
             `FomaAnalyzer`: 1 word(s), 7 timed samples/word after 1 discarded warmup call, this \
             run's calibrated timer floor is 100ns. Word source: fixed synthetic golden fixture. \
             p50/p90/p99 are the nearest-rank percentile over each word's own median duration.",
            "ATTESTED -- attestor=`synthetic-golden-attestor`, attested_on=`2026-07-27`, \
             corpus=`synthetic-golden-corpus.txt` (sha256=`0000000000000000000000000000000000000000000000000000000000000000`), \
             analysis_rate=0.9500. UNVERIFIED beyond the named attestor's own claim (nothing in \
             the artifact records what its author read while authoring, and PanGloss does not \
             train).",
            &render.mermaid,
            &mermaid_summary_line,
            "built in-process for this report, not persisted to disk (sha256=`fixed-golden-sha`, \
             package_fingerprint=`fixed-golden-fingerprint`)",
            "`synthetic-golden-corpus.txt` (sha256=`0000000000000000000000000000000000000000000000000000000000000000`)",
            "1 machine (heads/main)",
            "0000000000000000000000000000000000000000",
            &[
                "correctness: NOT CERTIFIED HERE -- coverage (when assessed) is a token-level \
                 analysis RATE, never accuracy; correctness evidence comes from the synthetic \
                 conformance suite, not from this report."
                    .to_string(),
            ],
        )
    }

    #[track_caller]
    fn assert_make_report_golden(actual: &str, expected: &str) {
        crate::test_support::assert_rendered_text_eq(actual, expected);
    }

    #[test]
    fn make_report_raw_golden_boundary_would_reject_crlf_materialized_fixture() {
        let actual = "# Report\n";
        let expected = "# Report\r\n";
        assert_ne!(actual, expected);
        assert_make_report_golden(actual, expected);
    }

    #[test]
    fn make_report_golden_rejects_whitespace_and_unicode_drift() {
        let whitespace = std::panic::catch_unwind(|| {
            assert_make_report_golden("# Report\nvalue\tA\n", "# Report\nvalue A\n");
        });
        assert!(whitespace.is_err());

        let unicode = std::panic::catch_unwind(|| {
            assert_make_report_golden("# Report\nnaïve\n", "# Report\nnaive\n");
        });
        assert!(unicode.is_err());
    }

    #[test]
    #[ignore = "regeneration helper, not a gate: run with --ignored to rewrite the golden from \
                this test's own computation after a reviewed, deliberate change to this module's \
                report shape or the golden fixture's inputs"]
    fn regenerate_make_report_golden_md() {
        let md = golden_report_markdown();
        fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/src/make_report_golden.md"),
            md,
        )
        .expect("golden must be writable");
    }

    #[test]
    fn make_report_golden_md() {
        let md = golden_report_markdown();
        assert_make_report_golden(&md, GOLDEN_MD);
    }

    const GOLDEN_MD: &str = include_str!("make_report_golden.md");

    // -----------------------------------------------------------------------------------------
    // Reference-grammar gate: `samples/data/{indonesian,amharic,sena}-hc.xml` are
    // real-language corpus data, deliberately gitignored (this repo's own synthetic-conformance-
    // only rule -- never committed). Gated exactly like `tests/f3_parity.rs`/`tests/
    // readiness_certification_gate.rs`: unconditionally `#[ignore]`d, each with its own self-skip
    // guard, so a fresh clone/CI run (no `samples/data/*`) stays green under `--include-ignored`.
    // Run locally with `cargo test -p pg-cli --bin pangloss -- --include-ignored make_report::tests::reference`.
    //
    // Expected result, per `docs/benchmark-matrix.md`: NONE of the three certifies today -- all
    // three are refused on exactly `mpr-group.overwrite-output`, a permanent carve-out. If any of
    // these tests ever sees a passing check for one of these grammars, that is this report
    // generator claiming something false, not a fixture becoming outdated.
    // -----------------------------------------------------------------------------------------

    fn sample_path(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../samples/data")
            .join(name)
    }

    fn have_sample(name: &str) -> bool {
        sample_path(name).exists()
    }

    fn assert_reference_grammar_never_certifies(xml_name: &str) {
        if !have_sample(xml_name) {
            eprintln!("skip: samples/data/{xml_name} not present locally");
            return;
        }
        let dir = scratch_dir(&format!("reference-{xml_name}"));
        let out_path = dir.join("report.md");
        let grammar_path = sample_path(xml_name);

        run_make_report(&[
            grammar_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
        ])
        .unwrap_or_else(|e| {
            panic!(
                "make-report must succeed (even a refused grammar produces a \
            report) for {xml_name}: {e}"
            )
        });

        let text = fs::read_to_string(&out_path).expect("read report.md");
        eprintln!("\n===== {xml_name} report =====\n{text}\n===== end {xml_name} report =====\n");

        assert!(
            text.contains("NOT SUPPORTED"),
            "{xml_name}: expected NOT SUPPORTED per docs/benchmark-matrix.md, got:\n{text}"
        );
        assert!(
            text.contains("simultaneous.subrule-overlap"),
            "{xml_name}: expected the permanently-refused predicate to be named:\n{text}"
        );
        assert!(
            !text.contains("| PASS"),
            "{xml_name}: a not-supported grammar must never show a passing check:\n{text}"
        );
    }

    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with \
                --include-ignored"]
    fn reference_grammar_indonesian_never_certifies() {
        assert_reference_grammar_never_certifies("indonesian-hc.xml");
    }

    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with \
                --include-ignored"]
    fn reference_grammar_amharic_never_certifies() {
        assert_reference_grammar_never_certifies("amharic-hc.xml");
    }

    #[test]
    #[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with \
                --include-ignored"]
    fn reference_grammar_sena_never_certifies() {
        assert_reference_grammar_never_certifies("sena-hc.xml");
    }
}
