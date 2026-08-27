//! `pangloss make-report <grammar> <out.md> [options]`: one markdown report composing an artifact's health and backend assessments, the plan diagram, and the conformance verdict.
//! This module measures nothing of its own: it never compiles a grammar, so build time, latency and coverage report as not measured. Trust provenance and capability enforcement: docs/research/pg-cli-make-report-design-notes.md.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use pg_foma::backend_selection::select_backends;
use pg_foma::capability::CompileDecision;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::health::HealthReport;
use pg_foma::plan_diagram::{
    build_plan_document_with_semantics, render_mermaid, MermaidRender, RenderMode,
};
use pg_foma::readiness_policy::{policy_v1, ThresholdPolicy};
use pg_foma::readiness_verdict::{
    certify_with_semantics, CapabilitySummary, CheckKind, CheckOutcome, CheckResult, CheckValue,
    CoverageAssessment, LatencyMeasurement, Measurements, ReadinessReport, Tier, TrustStatus,
};
use sha2::{Digest, Sha256};

#[cfg(feature = "developer-tools")]
const REFUSED_REPORT_REMEDIATION: &str =
    "pass --allow-unproven to force-compile and measure anyway";
#[cfg(not(feature = "developer-tools"))]
const REFUSED_REPORT_REMEDIATION: &str =
    "the grammar is outside the production capability policy; consult the saved capability/readiness report or use a developer-tools build for an explicitly authorized override workflow";

#[cfg(feature = "developer-tools")]
const REFUSED_PACK_REMEDIATION: &str =
    "no --allow-unproven override was given, so no pack was built";
#[cfg(not(feature = "developer-tools"))]
const REFUSED_PACK_REMEDIATION: &str =
    "the grammar is outside the production capability policy, so no pack was built; consult the saved capability/readiness report or use a developer-tools build for an explicitly authorized override workflow";

// Small, self-contained helpers: hashing, git introspection, timing.

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Runs `git <args>`, returning trimmed stdout or `None` on any failure; never a panic, since "pinned revisions" is a best-effort aid, not a hard requirement of the report.
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

/// `git submodule status <path>` reports the pinned gitlink commit without requiring the submodule checked out locally; `rel_path` is resolved against the repo toplevel, not the process cwd, since `git submodule status` reads it as a pathspec against the invocation directory otherwise.
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

// Markdown rendering.

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

/// Whether the policy value backing `kind` is `Placeholder` or `Measured`, surfaced next to every threshold so a reader never mistakes a placeholder number for a calibrated one.
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

fn render_health_findings(health: Option<&HealthReport>) -> Option<String> {
    let report = health?;
    let mut out = String::new();
    writeln!(out, "| Severity | Code | Phase | Explanation | Remedies |").unwrap();
    writeln!(out, "|---|---|---|---|---|").unwrap();
    for finding in &report.findings {
        let remedies = if finding.remedies.is_empty() {
            "none".to_string()
        } else {
            finding
                .remedies
                .iter()
                .map(|remedy| remedy.description.replace('|', "\\|").replace('\n', " "))
                .collect::<Vec<_>>()
                .join("<br>")
        };
        writeln!(
            out,
            "| `{:?}` | `{}` | `{:?}` | {} | {} |",
            finding.severity,
            finding.code.code(),
            finding.phase,
            finding.explanation.replace('|', "\\|").replace('\n', " "),
            remedies,
        )
        .unwrap();
    }
    Some(out)
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn render_backend_assessments(
    assessments: Option<&[pg_pack::BackendAssessment]>,
) -> Option<String> {
    let assessments = assessments?;
    let mut out = String::new();
    for assessment in assessments {
        writeln!(out, "### Backend `{}`", markdown_cell(&assessment.backend)).unwrap();
        writeln!(
            out,
            "Decision: `{}`; status: `{}`",
            markdown_cell(&assessment.decision),
            markdown_cell(&assessment.status),
        )
        .unwrap();
        if let Some(detail) = &assessment.status_detail {
            writeln!(out, "Status detail: {}", markdown_cell(detail)).unwrap();
        }
        let failed_predicates = if assessment.failed_predicates.is_empty() {
            "none".to_string()
        } else {
            assessment
                .failed_predicates
                .iter()
                .map(|predicate| markdown_cell(predicate))
                .collect::<Vec<_>>()
                .join("<br>")
        };
        writeln!(out, "Failed predicates: {failed_predicates}").unwrap();
        let advice_references = if assessment.advice_references.is_empty() {
            "none".to_string()
        } else {
            assessment
                .advice_references
                .iter()
                .map(|reference| {
                    format!(
                        "{} / {} (effort={:?})",
                        markdown_cell(&reference.shape_key),
                        markdown_cell(&reference.remedy_key),
                        reference.effort,
                    )
                })
                .collect::<Vec<_>>()
                .join("<br>")
        };
        writeln!(out, "Advice references: {advice_references}").unwrap();
        let shapes = if assessment.shapes.is_empty() {
            "none".to_string()
        } else {
            assessment
                .shapes
                .iter()
                .map(|shape| markdown_cell(shape))
                .collect::<Vec<_>>()
                .join(", ")
        };
        writeln!(out, "Shapes: {shapes}").unwrap();
        let cost_evidence = if assessment.cost_evidence.is_empty() {
            "none".to_string()
        } else {
            assessment
                .cost_evidence
                .iter()
                .map(|evidence| {
                    markdown_cell(&format!(
                        "metric={:?}; value={:?}; threshold={:?}; provenance={:?}",
                        evidence.metric,
                        evidence.value,
                        evidence.threshold,
                        evidence.provenance,
                    ))
                })
                .collect::<Vec<_>>()
                .join("<br>")
        };
        writeln!(out, "Cost evidence: {cost_evidence}").unwrap();
        if assessment.findings.is_empty() {
            writeln!(out, "Findings: none").unwrap();
        } else {
            writeln!(out).unwrap();
            writeln!(out, "| Code | Severity | Explanation | Remedies |").unwrap();
            writeln!(out, "|---|---|---|---|").unwrap();
            for finding in &assessment.findings {
                let remedies = if finding.remedies.is_empty() {
                    "none".to_string()
                } else {
                    finding
                        .remedies
                        .iter()
                        .map(|remedy| {
                            let caveat = remedy
                                .caveat
                                .as_deref()
                                .map(|text| format!(" (caveat: {})", markdown_cell(text)))
                                .unwrap_or_default();
                            format!(
                                "#{}: {}{}",
                                remedy.rank,
                                markdown_cell(&remedy.description),
                                caveat,
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("<br>")
                };
                writeln!(
                    out,
                    "| `{}` | `{:?}` | {} | {} |",
                    finding.code.code(),
                    finding.severity,
                    markdown_cell(&finding.explanation),
                    remedies,
                )
                .unwrap();
            }
        }
        writeln!(out).unwrap();
    }
    Some(out)
}

fn capability_override_engaged(decision: &CompileDecision, allow_unproven: bool) -> bool {
    allow_unproven && matches!(decision, CompileDecision::Refuse(_))
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

/// The one-line summary above the embedded mermaid diagram, factored out so the golden test computes it via the same code the live command uses, never a hand-copied restatement.
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

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn render_markdown(
    grammar_id: &str,
    grammar_path: &str,
    grammar_sha256: &str,
    out_path: &str,
    policy: &ThresholdPolicy,
    verdict: &ReadinessReport,
    fst_health: Option<&HealthReport>,
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
    render_markdown_with_assessments(
        grammar_id,
        grammar_path,
        grammar_sha256,
        out_path,
        policy,
        verdict,
        fst_health,
        None,
        build_time_line,
        latency_methodology_line,
        coverage_attestation_line,
        mermaid,
        mermaid_summary_line,
        pack_pin,
        corpus_pin,
        submodule_pin,
        repo_head,
        not_tested,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_markdown_with_assessments(
    grammar_id: &str,
    grammar_path: &str,
    grammar_sha256: &str,
    out_path: &str,
    policy: &ThresholdPolicy,
    verdict: &ReadinessReport,
    fst_health: Option<&HealthReport>,
    backend_assessments: Option<&[pg_pack::BackendAssessment]>,
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

    if let Some(findings) = render_health_findings(fst_health) {
        writeln!(out, "## FST health findings").unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "Raw readiness findings from the pack; capability trust is reported separately."
        )
        .unwrap();
        writeln!(out).unwrap();
        if let Some(report) = fst_health {
            writeln!(
                out,
                "Admission: `{:?}` ({})",
                report.admission(),
                report.admission_by_class().render()
            )
            .unwrap();
            writeln!(out).unwrap();
        }
        writeln!(out, "{findings}").unwrap();
        writeln!(out).unwrap();
    }

    if let Some(assessments) = render_backend_assessments(backend_assessments) {
        writeln!(out, "## Backend assessments").unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "Every considered backend is retained here, including refused or failed routes; these diagnostics are independent of FST readiness health and capability trust."
        )
        .unwrap();
        writeln!(out).unwrap();
        writeln!(out, "{assessments}").unwrap();
        writeln!(out).unwrap();
    }

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

// The command.

pub fn run_make_report(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut pack_path: Option<String> = None;
    let mut policy_path: Option<String> = None;
    let mut allow_unproven = false;
    let mut authorized_by: Option<String> = None;
    let mut reason: Option<String> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--pack" => pack_path = Some(it.next().ok_or("--pack requires a value")?.clone()),
            s if s.starts_with("--pack=") => pack_path = Some(s["--pack=".len()..].to_string()),
            "--policy" => policy_path = Some(it.next().ok_or("--policy requires a value")?.clone()),
            s if s.starts_with("--policy=") => {
                policy_path = Some(s["--policy=".len()..].to_string())
            }
            "--allow-unproven" => {
                crate::accept_developer_flag(a)?;
                allow_unproven = true;
            }
            "--authorized-by" => {
                authorized_by = Some(it.next().ok_or("--authorized-by requires a value")?.clone())
            }
            s if s.starts_with("--authorized-by=") => {
                authorized_by = Some(s["--authorized-by=".len()..].to_string())
            }
            "--reason" => reason = Some(it.next().ok_or("--reason requires a value")?.clone()),
            s if s.starts_with("--reason=") => reason = Some(s["--reason=".len()..].to_string()),
            s => {
                crate::reject_unknown_option(s)?;
                positional.push(s);
            }
        }
    }
    let [grammar_path, out_path] = positional[..] else {
        return Err(format!(
            "usage: make-report <grammar> <out.md> [--pack=<path>] [--policy=<path>]{} \
             [--authorized-by=<name>] [--reason=<text>]",
            crate::REPORT_DEVELOPER_HELP
        ));
    };

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

    // One derivation, shared by every place this command needs the capability verdict, rather than three independent characterize walks over the same grammar.
    let semantics = GrammarSemantics::derive(&grammar);
    let selection = select_backends(&semantics);
    let decision = crate::gated_backend_decision(&selection);
    let capability_overridden = capability_override_engaged(&decision, allow_unproven);
    let attempt_compile = matches!(
        decision,
        CompileDecision::Admit | CompileDecision::ConfirmOnly
    ) || (matches!(decision, CompileDecision::Refuse(_)) && allow_unproven)
        // Supplied packs remain reportable even when their source grammar is refused.
        || pack_path.is_some();

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
    let fst_health: Option<HealthReport>;
    let backend_assessments: Option<Vec<pg_pack::BackendAssessment>>;
    let latency_methodology_line: String;
    // Separate from `verdict.checks` on purpose: that carries only the Pass/Fail/NotAssessed outcome, never the attestor/date fields a coverage attestation would need.
    let coverage_attestation_line: String;

    if !attempt_compile {
        not_tested.push(format!(
            "build time, artifact size, lexicon scale, latency, and coverage: NONE of these were \
             measured -- the capability gate refuses this grammar; {REFUSED_REPORT_REMEDIATION}, \
             so no compiled artifact exists to measure at all (this is the expected, headline \
             outcome for a permanently-refused construct, per docs/benchmark-matrix.md)."
        ));
        trust = TrustStatus::Proven; // no override was exercised; there is simply no artifact.
        measurements = None;
        fst_health = None;
        backend_assessments = Some(crate::pack::backend_assessments(
            &selection,
            crate::GATED_BACKEND,
            &[],
            None,
        ));
        build_time_line = format!(
            "not measured -- the grammar was refused and no compiled artifact was \
             ever built ({REFUSED_REPORT_REMEDIATION}; the resulting \
             report will still never certify -- trust=unproven never certifies, under any \
             configuration)."
        );
        pack_pin = format!("none -- the grammar was refused and {REFUSED_PACK_REMEDIATION}.");
        latency_methodology_line = "not measured -- see \"build time\" above.".to_string();
        coverage_attestation_line =
            "not assessed -- no compiled artifact exists to run a corpus against (independent of \
             whether --corpus was supplied)."
                .to_string();
    } else {
        // ---- artifact size and health: from a REAL artifact, never caller-supplied parameters ----
        let (_artifact_size, pack_pin_line, health, assessments): (
            u64,
            String,
            HealthReport,
            Vec<pg_pack::BackendAssessment>,
        ) = match &pack_path {
            Some(p) => {
                let bytes = fs::read(p).map_err(|e| format!("read --pack {p}: {e}"))?;
                let read = pg_pack::read_pack(&bytes).map_err(|e| format!("read_pack {p}: {e}"))?;
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
                (
                    bytes.len() as u64,
                    pin,
                    read.manifest.fst_health.clone(),
                    read.manifest.backend_assessments.clone(),
                )
            }
        };
        pack_pin = pack_pin_line;
        fst_health = Some(health);
        backend_assessments = Some(assessments);

        // Nothing is measured here: this command no longer compiles anything of its own.
        build_time_line =
            "not measured -- make-report no longer compiles a grammar; build time belongs to"
                .to_string()
                + " whatever produced the artifact.";
        latency_methodology_line =
            "not measured -- make-report no longer compiles an analyzer to time.".to_string();
        coverage_attestation_line =
            "not assessed -- make-report no longer runs a corpus.".to_string();
        not_tested.push(
            "build time, latency, and coverage: NOT MEASURED -- make-report reports on an"
                .to_string()
                + " artifact it is given and no longer compiles one itself, so it has nothing"
                + " of its own to time or to run a corpus against.",
        );
        measurements = None;
    }

    let verdict = certify_with_semantics(&semantics, &trust, measurements.as_ref(), &policy);

    // ---- compilation plan diagram (pure composition of section-visualize-compilation-plan) ----
    let plan_doc = build_plan_document_with_semantics(&semantics);
    let render = render_mermaid(&plan_doc, RenderMode::default());
    let mermaid_summary_line = render_mermaid_summary_line(&render);

    // ---- pinned revisions ----
    let corpus_pin = "not supplied -- make-report no longer runs a corpus".to_string();
    let submodule_pin = submodule_revision("machine");
    let repo_head = repo_head_revision();

    let report_md = render_markdown_with_assessments(
        &grammar_id,
        grammar_path,
        &grammar_sha256,
        out_path,
        &policy,
        &verdict,
        fst_health.as_ref(),
        backend_assessments.as_deref(),
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

    // The `&Grammar` front ends, used only by the golden-render test below; the live command drives the `_with_semantics` forms off its one shared owner.
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
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
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

    #[test]
    fn backend_assessment_renderer_includes_shapes_and_cost_evidence() {
        let assessment = pg_pack::BackendAssessment {
            backend: "synthetic-backend".to_string(),
            decision: "admit".to_string(),
            status: "accepted".to_string(),
            findings: Vec::new(),
            failed_predicates: Vec::new(),
            shapes: vec!["synthetic-shape".to_string()],
            cost_evidence: vec![pg_pack::BackendCostEvidence {
                metric: pg_foma::health::Metric::CompositeRulePairCount,
                value: pg_foma::health::MetricValue::Count(42),
                threshold: Some(pg_foma::health::MetricValue::Count(10)),
                provenance: pg_foma::health::ValueProvenance::ProvenBound,
            }],
            advice_references: Vec::new(),
            status_detail: None,
        };

        let rendered = render_backend_assessments(Some(std::slice::from_ref(&assessment)))
            .expect("assessment rendering must produce markdown");
        assert!(rendered.contains("Shapes: synthetic-shape"), "{rendered}");
        assert!(rendered.contains("Cost evidence:"), "{rendered}");
        assert!(rendered.contains("metric=CompositeRulePairCount"), "{rendered}");
        assert!(rendered.contains("value=Count(42)"), "{rendered}");
        assert!(rendered.contains("threshold=Some(Count(10))"), "{rendered}");
        assert!(rendered.contains("provenance=ProvenBound"), "{rendered}");
    }

    // A golden report over fixed, hand-picked inputs (never a live timer), since a live end-to-end run's real wall-clock timing would make a byte-for-byte golden inherently flaky.

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

        // Real, deterministic composition, never a live timer: the same functions the live command calls, over the same fixed fixture.
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
            None,
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

}
