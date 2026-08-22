//! Builds and writes a `.pgpack`: the ADR 0001/0005 capability-trust stamp, ADR 0004's required runtime features, FST health, and the foma/runtime payloads.

use std::fs;

use pg_foma::analyzer::FomaProposer;
use pg_foma::backend_selection::{select_backends, BackendReport, BackendSelection, BackendStatus};
use pg_foma::capability::CompileDecision;
use pg_foma::emit::{EmitReport, FomaTier};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, OverrideRecord, Phase, Severity,
    ValueProvenance,
};
use pg_foma::health_evaluator::{evaluate_foma_error, evaluate_health};
use pg_foma::peel::{ReduplicationPeeler, RUNTIME_FEATURE_REDUPLICATION_PEEL};
use pg_grammar::model::Grammar;
use pg_pack::{
    BackendAdviceReference, BackendAssessment, BackendCostEvidence, CapabilityOverrideRecord,
    CapabilityTrust, FstCompletenessCertificate, OverriddenConfig, PackManifest,
    RequiredRuntimeFeatures, MANIFEST_FORMAT_TAG, MANIFEST_SCHEMA_VERSION,
};

/// This build's own foma-feature level (ADR 0004); a hand-bumped constant, not derived from any registry.
const FOMA_FEATURE_LEVEL: u32 = 1;

/// Honestly-labeled placeholder: no Rust-HermitCrab runtime-payload serializer exists yet, so the bytes announce themselves rather than imitating a real payload.
const PLACEHOLDER_RUNTIME_PAYLOAD: &[u8] =
    b"PANGLOSS-PLACEHOLDER-RUNTIME-PAYLOAD: no Rust-HermitCrab \
runtime-payload serializer exists yet anywhere in this workspace; this byte content is NOT a \
compiled artifact and must never be loaded as one.";

/// Fallback foma payload, used only when this grammar's foma compile did not succeed or `--watchdog` was passed; the real path writes `FomaProposer::foma_binary_payload()` instead.
const PLACEHOLDER_FOMA_PAYLOAD: &[u8] = b"PANGLOSS-PLACEHOLDER-FOMA-PAYLOAD: this grammar's foma \
compile did not succeed (or --watchdog was passed), so no compiled network was available to \
serialize; this byte content is NOT a compiled network and must never be loaded as one.";

/// This crate's own `Cargo.toml` semver, used as the `required_runtime_features.hc_port_semver` value.
fn this_crate_semver() -> (u32, u32, u32) {
    const MAJOR: &str = env!("CARGO_PKG_VERSION_MAJOR");
    const MINOR: &str = env!("CARGO_PKG_VERSION_MINOR");
    const PATCH: &str = env!("CARGO_PKG_VERSION_PATCH");
    (
        MAJOR
            .parse()
            .expect("CARGO_PKG_VERSION_MAJOR is always numeric"),
        MINOR
            .parse()
            .expect("CARGO_PKG_VERSION_MINOR is always numeric"),
        PATCH
            .parse()
            .expect("CARGO_PKG_VERSION_PATCH is always numeric"),
    )
}

/// A caller-supplied timestamp string: `unix:<seconds-since-epoch>`.
fn now_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

fn decision_label(decision: &CompileDecision) -> &'static str {
    match decision {
        CompileDecision::Admit => "admit",
        CompileDecision::ConfirmOnly => "confirm_only",
        CompileDecision::Refuse(_) => "refuse",
    }
}

fn backend_status_label(status: BackendStatus) -> &'static str {
    match status {
        BackendStatus::Accepted => "accepted",
        BackendStatus::Refused => "refused",
        BackendStatus::Missing => "missing",
        BackendStatus::Failed => "failed",
    }
}

fn assessment_from_report(report: &BackendReport) -> BackendAssessment {
    BackendAssessment {
        backend: report.strategy().label().to_string(),
        decision: decision_label(report.decision()).to_string(),
        status: backend_status_label(report.status()).to_string(),
        findings: report.findings().to_vec(),
        failed_predicates: report.failed_predicates().to_vec(),
        shapes: report.shapes().to_vec(),
        cost_evidence: report
            .cost_evidence()
            .iter()
            .map(|evidence| BackendCostEvidence {
                metric: evidence.metric,
                value: evidence.value,
                threshold: evidence.threshold,
                provenance: evidence.provenance,
            })
            .collect(),
        advice_references: report
            .advice_references()
            .iter()
            .map(|reference| BackendAdviceReference {
                shape_key: reference.shape_key.clone(),
                remedy_key: reference.remedy_key.clone(),
                effort: reference.effort,
            })
            .collect(),
        status_detail: report.status_detail().map(str::to_string),
    }
}

/// Preserves every backend report while attaching compile findings only to the selected backend.
fn backend_assessments(
    selection: &BackendSelection,
    gated_backend: EmissionStrategy,
    gated_compile_findings: &[HealthFinding],
    gated_compile_error: Option<&str>,
    health: &HealthReport,
) -> Vec<BackendAssessment> {
    selection
        .reports()
        .iter()
        .map(|report| {
            let mut assessment = assessment_from_report(report);
            if report.strategy() == gated_backend {
                assessment
                    .findings
                    .extend(gated_compile_findings.iter().cloned());
                if let Some(error) = gated_compile_error {
                    assessment.status = "failed".to_string();
                    assessment.status_detail = Some(error.to_string());
                }
                assessment
                    .cost_evidence
                    .extend(
                        gated_compile_findings
                            .iter()
                            .map(|finding| BackendCostEvidence {
                                metric: finding.metric,
                                value: finding.value,
                                threshold: finding.threshold,
                                provenance: finding.provenance,
                            }),
                    );
            }
            for finding in &mut assessment.findings {
                if let Some(source) = health.findings.iter().find(|source| {
                    source.code == finding.code
                        && source.phase == finding.phase
                        && source.affected == finding.affected
                        && source.metric == finding.metric
                        && source.value == finding.value
                        && source.explanation == finding.explanation
                }) {
                    finding.override_record = source.override_record.clone();
                }
            }
            assessment
        })
        .collect()
}

/// Certifies only full payloads with no uncovered construct, pending successor, or budget trip.
fn completeness_certificate(
    backend: EmissionStrategy,
    report: Option<&EmitReport>,
    compiled_payload_present: bool,
) -> Option<FstCompletenessCertificate> {
    let report = report?;
    let pending_successors = report
        .closure_refusal
        .as_ref()
        .and_then(|refusal| refusal.pending_successors)
        .unwrap_or(0);
    let complete = compiled_payload_present
        && matches!(report.tier, FomaTier::Full)
        && report.uncovered.is_empty()
        && pending_successors == 0
        && report.enum_budget_exceeded.is_none()
        && report.closure_refusal.is_none();
    complete.then_some(FstCompletenessCertificate {
        backend: backend.label().to_string(),
        uncovered_constructs: report.uncovered.len(),
        pending_successors,
        enumeration_budget_exceeded: report.enum_budget_exceeded.is_some(),
        compiled_payload_present,
    })
}

/// Applies the publication gate, recording health overrides without changing capability trust.
fn apply_health_override(
    report: &mut HealthReport,
    allow_unproven: bool,
    authorized_by: Option<&str>,
    reason: Option<&str>,
    worker_containment: bool,
) -> Result<bool, String> {
    let admission = report.admission_without_overrides();
    if admission < Severity::Error {
        return Ok(false);
    }
    if worker_containment {
        return Err(
            "FST health is a worker containment failure; it cannot be overridden and no .pgpack was written"
                .to_string(),
        );
    }
    if report
        .findings
        .iter()
        .any(|finding| finding.severity >= Severity::Error && !finding.override_allowed())
    {
        return Err(
            "FST health is an apply containment failure; it cannot be overridden and no .pgpack was written"
                .to_string(),
        );
    }
    if !allow_unproven {
        return Err(format!(
            "FST health is {admission:?}; no .pgpack was written. Pass --allow-unproven only for an explicitly authorized development build"
        ));
    }

    let record = OverrideRecord {
        authorized_by: authorized_by.unwrap_or("unspecified").to_string(),
        reason: reason
            .unwrap_or("--allow-unproven development build")
            .to_string(),
        recorded_at: now_string(),
    };
    for finding in &mut report.findings {
        if finding.override_allowed() && finding.override_record.is_none() {
            finding.override_record = Some(record.clone());
        }
    }
    Ok(true)
}

/// Records a missing serialized Foma network as an overrideable development-only error.
fn record_foma_payload_availability(report: &mut HealthReport, payload_is_real: bool) {
    if payload_is_real {
        return;
    }

    report.findings.push(HealthFinding {
        code: FindingCode::BackendCompilationFailed,
        severity: Severity::Error,
        phase: Phase::Compile,
        affected: vec!["foma-payload".to_string()],
        metric: Metric::UnknownUnboundedWork,
        value: MetricValue::Unbounded,
        provenance: ValueProvenance::Observed,
        threshold: None,
        explanation: "no compiled Foma payload is available for this pack; a successful watchdog health check does not transport the compiled network, so production publication must stop instead of silently substituting a placeholder".to_string(),
        remedies: Vec::new(),
        override_record: None,
    });
}

/// `pangloss pack <grammar> <out.pgpack> [--allow-unproven] [--authorized-by=<name>] [--reason=<text>] [--watchdog]`; `--watchdog` runs the FST-health compile in a killable child process instead of in-process.
pub fn run_pack(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut allow_unproven = false;
    let mut authorized_by: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut watchdog = false;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--allow-unproven" => allow_unproven = true,
            "--authorized-by" => {
                let v = it.next().ok_or("--authorized-by requires a value")?;
                authorized_by = Some(v.clone());
            }
            s if s.starts_with("--authorized-by=") => {
                authorized_by = Some(s["--authorized-by=".len()..].to_string());
            }
            "--reason" => {
                let v = it.next().ok_or("--reason requires a value")?;
                reason = Some(v.clone());
            }
            s if s.starts_with("--reason=") => {
                reason = Some(s["--reason=".len()..].to_string());
            }
            "--watchdog" => watchdog = true,
            s => positional.push(s),
        }
    }
    let [grammar_path, out_path] = positional[..] else {
        return Err(
            "usage: pack <grammar> <out.pgpack> [--allow-unproven] [--authorized-by=<name>] \
             [--reason=<text>] [--watchdog]"
                .into(),
        );
    };

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);

    let semantics = GrammarSemantics::derive(&grammar);
    let built = build_pack(
        grammar_path,
        &grammar,
        &semantics,
        allow_unproven,
        authorized_by.as_deref(),
        reason.as_deref(),
        watchdog,
    )?;

    fs::write(out_path, &built.bytes).map_err(|e| format!("write {out_path}: {e}"))?;

    eprintln!(
        "pack complete: {out_path} ({} bytes) -- capability_trust={}, required_runtime_features={:?}, \
         fst_health admission={:?}. NOTE: the runtime payload section is an honestly-labeled \
         PLACEHOLDER (no Rust-HermitCrab runtime-payload serializer exists yet anywhere in this \
         workspace -- see this module's own doc). The foma payload section is {} -- do not treat a \
         placeholder section as a usable compiled artifact.",
        built.bytes.len(),
        if built.manifest.capability_trust.is_unproven() { "overridden/unproven" } else { "proven" },
        built.manifest.required_runtime_features.runtime_operations,
        built.manifest.fst_health.admission(),
        if built.foma_payload_is_real {
            "REAL compiled-network bytes (foma::io::fsm_write_binary, the same encoding \
             fsm_read_binary_mem reads back)"
        } else {
            "a PLACEHOLDER (this grammar's foma compile did not succeed, or --watchdog was passed \
             and the worker protocol does not yet return the compiled network across the process \
             boundary)"
        },
    );
    Ok(())
}

/// Result of building one `.pgpack`: the manifest, the full container bytes, and whether the foma payload is real. Shared by `run_pack` and `pangloss make-report`.
pub(crate) struct BuiltPack {
    pub manifest: PackManifest,
    pub bytes: Vec<u8>,
    /// `true` iff the packed foma payload is the grammar's own compiled network rather than `PLACEHOLDER_FOMA_PAYLOAD`.
    pub foma_payload_is_real: bool,
}

/// Builds one `.pgpack` in memory: capability-trust stamp, required-runtime-feature set, FST-health report, and the written `pg_pack::write_pack` container bytes. `semantics` must be `GrammarSemantics::derive`d from `grammar`, so callers pay for the grammar walk once.
#[allow(clippy::too_many_arguments)] // one more grammar-derived input alongside `grammar` itself.
pub(crate) fn build_pack(
    grammar_path: &str,
    grammar: &Grammar,
    semantics: &GrammarSemantics<'_>,
    allow_unproven: bool,
    authorized_by: Option<&str>,
    reason: Option<&str>,
    watchdog: bool,
) -> Result<BuiltPack, String> {
    // ---- ADR 0001/0005: the capability-trust stamp ---------------------------------------------
    let backend = crate::GATED_BACKEND.label();
    let selection = select_backends(semantics);
    let decision = crate::gated_backend_decision(&selection);
    let gated_backend_findings: Vec<HealthFinding> = selection
        .report_for(crate::GATED_BACKEND)
        .map(|report| report.findings().to_vec())
        .unwrap_or_default();
    let capability_trust = match &decision {
        CompileDecision::Admit => {
            eprintln!(
                "capability: Admit [backend={backend}] -- packing a proven-clean grammar \
                 (capability_trust=Proven)"
            );
            CapabilityTrust::Proven
        }
        CompileDecision::ConfirmOnly => {
            eprintln!(
                "capability: ConfirmOnly [backend={backend}] -- packing (ADR 0001: first-class, \
                 recall-preserving via confirm, not a failure; capability_trust=Proven)"
            );
            CapabilityTrust::Proven
        }
        CompileDecision::Refuse(diags) => {
            if !allow_unproven {
                let mut msg = format!(
                    "capability gate refused this grammar: backend={backend} declined on {} \
                     construct(s); no .pgpack was written. Pass --allow-unproven (ADR 0005) to \
                     force-pack anyway -- the pack will be indelibly stamped \
                     capability_trust=Overridden/unproven.\n",
                    diags.len()
                );
                for d in diags {
                    msg.push_str(&format!(
                        "  capability-refuse: predicate={} construct={} witness={}\n",
                        d.predicate, d.construct, d.witness
                    ));
                }
                return Err(msg);
            }
            let record = CapabilityOverrideRecord {
                authorized_by: authorized_by.map(|s| s.to_string()).unwrap_or_else(|| {
                    "unspecified (--allow-unproven with no --authorized-by given)".to_string()
                }),
                reason: reason
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "--allow-unproven (no --reason given)".to_string()),
                recorded_at: now_string(),
                overridden_configs: diags
                    .iter()
                    .map(|d| OverriddenConfig {
                        predicate: d.predicate.to_string(),
                        construct: d.construct.clone(),
                        witness: d.witness.clone(),
                    })
                    .collect(),
            };
            eprintln!(
                "CAPABILITY-OVERRIDE trust=unproven: --allow-unproven force-packing {} construct(s) \
                 backend={backend} declined (ADR 0005) -- this pack's capability_trust is \
                 Overridden, PERSISTENT, \
                 and INDELIBLE (it survives write->read and can never be laundered back into a \
                 clean Proven claim by any consumer).",
                record.overridden_configs.len()
            );
            for d in diags {
                eprintln!(
                    "  capability-override: predicate={} construct={} witness={}",
                    d.predicate, d.construct, d.witness
                );
            }
            CapabilityTrust::Overridden(record)
        }
    };

    // ---- ADR 0004: the required-runtime-feature set --------------------------------------------
    let peeler = ReduplicationPeeler::new(grammar);
    let mut runtime_operations = Vec::new();
    if peeler.has_redup_rules() {
        runtime_operations.push(RUNTIME_FEATURE_REDUPLICATION_PEEL.to_string());
    }
    let required_runtime_features = RequiredRuntimeFeatures {
        payload_format_version: pg_pack::CONTAINER_VERSION,
        runtime_operations,
        foma_feature_level: FOMA_FEATURE_LEVEL,
        hc_port_semver: this_crate_semver(),
        extensions: Vec::new(),
    };

    // ---- FST health, plus the real foma payload when this same compile succeeds ----------------
    let (
        mut fst_health,
        real_foma_payload,
        gated_compile_findings,
        gated_compile_error,
        gated_emit_report,
    ): (
        HealthReport,
        Option<Vec<u8>>,
        Vec<HealthFinding>,
        Option<String>,
        Option<EmitReport>,
    ) = if watchdog {
        let health = run_fst_health_under_watchdog(grammar_path)?;
        (
            health.clone(),
            None,
            health.findings,
            Some("watchdog compilation did not return a serializable FST payload".to_string()),
            None,
        )
    } else {
        let (proposer_result, compile_profile) = if allow_unproven {
            FomaProposer::new_unproven_with_profile(grammar)
        } else {
            FomaProposer::new_with_profile(grammar)
        };
        match &proposer_result {
            Ok(proposer) => {
                let health = evaluate_health(
                    None,
                    proposer.report.as_ref(),
                    &[],
                    &[],
                    Some(&compile_profile),
                );
                let foma_bytes = proposer.foma_binary_payload().map_err(|e| {
                    format!(
                        "serializing the compiled foma network to its binary-memory payload: {e}"
                    )
                })?;
                (
                    health.clone(),
                    Some(foma_bytes),
                    health.findings,
                    None,
                    proposer.report.clone(),
                )
            }
            Err(error) => {
                let health = evaluate_foma_error(error, Some(&compile_profile));
                (
                    health.clone(),
                    None,
                    health.findings,
                    Some(error.to_string()),
                    error.emit_report().cloned(),
                )
            }
        }
    };
    // Static backend findings remain part of admission even when this compile finishes.
    fst_health.findings.extend(gated_backend_findings);
    record_foma_payload_availability(&mut fst_health, real_foma_payload.is_some());
    let _health_overridden = apply_health_override(
        &mut fst_health,
        allow_unproven,
        authorized_by,
        reason,
        watchdog,
    )?;
    let backend_assessments = backend_assessments(
        &selection,
        crate::GATED_BACKEND,
        &gated_compile_findings,
        gated_compile_error.as_deref(),
        &fst_health,
    );
    let fst_completeness = completeness_certificate(
        crate::GATED_BACKEND,
        gated_emit_report.as_ref(),
        real_foma_payload.is_some(),
    );
    // `None` iff `--watchdog` was used or this compile did not succeed; falls back to the placeholder.
    let foma_payload: &[u8] = real_foma_payload
        .as_deref()
        .unwrap_or(PLACEHOLDER_FOMA_PAYLOAD);

    // ---- Payloads: foma is real iff `real_foma_payload` is `Some`; runtime is always the placeholder ----
    let package_fingerprint = pg_pack::fingerprint_hex(PLACEHOLDER_RUNTIME_PAYLOAD, foma_payload);

    let grammar_id = grammar.name.clone().unwrap_or_else(|| {
        std::path::Path::new(grammar_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown-grammar")
            .to_string()
    });

    let manifest = PackManifest {
        format: MANIFEST_FORMAT_TAG.to_string(),
        manifest_schema_version: MANIFEST_SCHEMA_VERSION,
        grammar_id,
        package_fingerprint,
        required_runtime_features,
        capability_trust,
        fst_health,
        backend_assessments,
        fst_completeness,
        license: None,
        created_by: format!("pangloss pack {}", env!("CARGO_PKG_VERSION")),
        created_at: now_string(),
        signature: None,
    };

    let foma_payload_is_real = real_foma_payload.is_some();
    let bytes = pg_pack::write_pack(&manifest, PLACEHOLDER_RUNTIME_PAYLOAD, foma_payload)
        .map_err(|e| format!("write_pack: {e}"))?;

    Ok(BuiltPack {
        manifest,
        bytes,
        foma_payload_is_real,
    })
}

/// Maps `grammar_path`'s extension to `pg_foma::worker::GrammarFormat`, matching `crate::load_grammar`'s own dispatch.
fn infer_grammar_format(grammar_path: &str) -> pg_foma::worker::GrammarFormat {
    let ext = std::path::Path::new(grammar_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "json" => pg_foma::worker::GrammarFormat::Json,
        "fwdata" => pg_foma::worker::GrammarFormat::Fwdata,
        _ => pg_foma::worker::GrammarFormat::Xml,
    }
}

/// Runs the FST-health compile in a re-exec'd `__compile-worker-child` process under a killable watchdog, mapping the outcome to a `HealthReport`.
fn run_fst_health_under_watchdog(
    grammar_path: &str,
) -> Result<pg_foma::health::HealthReport, String> {
    let format = infer_grammar_format(grammar_path);
    let request = pg_foma::worker::CompileWorkerRequest::new(grammar_path.to_string(), format);
    let envelope = pg_foma::worker::WatchdogEnvelope::default_envelope();
    let exe = std::env::current_exe()
        .map_err(|e| format!("--watchdog: could not resolve this executable's own path: {e}"))?;
    let outcome = pg_foma::worker::run_compile_worker(
        &exe,
        &["__compile-worker-child".to_string()],
        &request,
        &envelope,
    );
    eprintln!("watchdog: compile-worker outcome: {outcome:?}");
    Ok(outcome.health_report())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn synthetic_health(severity: Severity) -> HealthReport {
        HealthReport::new(vec![HealthFinding {
            code: FindingCode::ResourceBudgetReached,
            severity,
            phase: Phase::Compile,
            affected: vec!["synthetic composite route".to_string()],
            metric: Metric::UnknownUnboundedWork,
            value: MetricValue::Count(101),
            provenance: ValueProvenance::Observed,
            threshold: Some(MetricValue::Count(100)),
            explanation: "synthetic health gate".to_string(),
            remedies: Vec::new(),
            override_record: None,
        }])
    }

    #[test]
    fn health_warning_publishes_without_override() {
        let mut report = synthetic_health(Severity::Warning);
        assert!(!apply_health_override(&mut report, false, None, None, false).unwrap());
        assert_eq!(report.admission(), Severity::Warning);
        assert!(report.findings[0].override_record.is_none());
    }

    #[test]
    fn health_error_refuses_publication_without_override() {
        let mut report = synthetic_health(Severity::Error);
        let error = apply_health_override(&mut report, false, None, None, false).unwrap_err();
        assert!(error.contains("no .pgpack was written"));
        assert_eq!(report.admission(), Severity::Error);
        assert!(report.findings[0].override_record.is_none());
    }

    #[test]
    fn health_critical_refuses_publication_without_override() {
        let mut report = synthetic_health(Severity::Critical);
        let error = apply_health_override(&mut report, false, None, None, false).unwrap_err();
        assert!(error.contains("no .pgpack was written"));
        assert_eq!(report.admission(), Severity::Critical);
    }

    #[test]
    fn health_error_development_override_is_recorded_and_admitted() {
        let mut report = synthetic_health(Severity::Error);
        assert!(apply_health_override(
            &mut report,
            true,
            Some("test operator"),
            Some("exercise the fallback"),
            false,
        )
        .unwrap());
        assert_eq!(report.admission(), Severity::Ideal);
        let record = report.findings[0].override_record.as_ref().unwrap();
        assert_eq!(record.authorized_by, "test operator");
        assert_eq!(record.reason, "exercise the fallback");
    }

    #[test]
    fn health_critical_development_override_is_recorded_and_admitted() {
        let mut report = synthetic_health(Severity::Critical);
        assert!(apply_health_override(
            &mut report,
            true,
            Some("test operator"),
            Some("exercise the critical fallback"),
            false,
        )
        .unwrap());
        assert_eq!(report.admission(), Severity::Ideal);
        assert!(report.findings[0].override_record.is_some());
    }

    #[test]
    fn health_apply_containment_cannot_be_overridden() {
        let mut report = synthetic_health(Severity::Critical);
        report.findings[0].phase = Phase::Apply;
        let error = apply_health_override(&mut report, true, None, None, false).unwrap_err();
        assert!(error.contains("apply containment"));
        assert!(report.findings[0].override_record.is_none());
    }

    #[test]
    fn missing_foma_payload_is_an_error_before_publication() {
        let mut report = HealthReport::new(Vec::new());
        record_foma_payload_availability(&mut report, false);

        assert_eq!(report.admission(), Severity::Error);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(
            report.findings[0].code,
            FindingCode::BackendCompilationFailed
        );
        assert_eq!(report.findings[0].affected, vec!["foma-payload"]);
        assert_eq!(report.findings[0].value, MetricValue::Unbounded);
        assert!(report.findings[0]
            .explanation
            .contains("no compiled Foma payload"));
        assert!(apply_health_override(&mut report, false, None, None, false).is_err());
    }

    #[test]
    fn missing_foma_payload_requires_and_records_development_override() {
        let mut report = HealthReport::new(Vec::new());
        record_foma_payload_availability(&mut report, false);

        assert!(apply_health_override(
            &mut report,
            true,
            Some("test operator"),
            Some("exercise the watchdog placeholder path"),
            false,
        )
        .unwrap());
        assert_eq!(report.admission(), Severity::Ideal);
        assert!(report.findings[0].override_record.is_some());
    }

    #[test]
    fn real_foma_payload_adds_no_availability_finding() {
        let mut report = HealthReport::new(Vec::new());
        record_foma_payload_availability(&mut report, true);

        assert!(report.findings.is_empty());
        assert_eq!(report.admission(), Severity::Ideal);
    }

    #[test]
    fn missing_foma_payload_cannot_downgrade_or_override_critical_worker_failure() {
        let mut report = synthetic_health(Severity::Critical);
        record_foma_payload_availability(&mut report, false);

        let error = apply_health_override(
            &mut report,
            true,
            Some("test operator"),
            Some("must not bypass a worker failure"),
            true,
        )
        .unwrap_err();
        assert!(error.contains("cannot be overridden"));
        assert_eq!(report.admission(), Severity::Critical);
        assert!(report
            .findings
            .iter()
            .all(|finding| finding.override_record.is_none()));
    }

    /// A fresh, collision-free scratch directory per test.
    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "pangloss-cli-pack-test-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    /// An ordinary, capability-`Admit` grammar: one bare root, no compounding, no reduplication.
    const CLEAN_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PackCleanFixture</Name>
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

    /// A grammar the gated backend declines and the compiler still builds; see `crate::test_support::BACKEND_REFUSED_GRAMMAR_XML`.
    const REFUSE_GRAMMAR_XML: &str = crate::test_support::BACKEND_REFUSED_GRAMMAR_XML;

    /// A grammar whose single rule duplicates `stem` via `CopyFromInput` twice, matching `classify_affix`'s `Role::Reduplication` trigger.
    const REDUP_GRAMMAR_XML: &str = r#"<HermitCrabInput><Language><Name>PackRedupFixture</Name>
      <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions>
          <SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
          <SegmentDefinition id="cb"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
        </SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /><Segment segment="cb" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="mr1">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <MorphologicalRule id="mr1">
              <Name>Redup</Name>
              <MorphologicalSubrules>
                <MorphologicalSubrule id="sub1">
                  <MorphologicalInput>
                    <PhoneticSequence id="stem"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAll" /></OptionalSegmentSequence></PhoneticSequence>
                  </MorphologicalInput>
                  <MorphologicalOutput>
                    <CopyFromInput index="stem" />
                    <CopyFromInput index="stem" />
                  </MorphologicalOutput>
                </MorphologicalSubrule>
              </MorphologicalSubrules>
            </MorphologicalRule>
          </MorphologicalRuleDefinitions>
          <LexicalEntries>
            <LexicalEntry id="e1">
              <Allomorphs><Allomorph id="a1"><PhoneticShape>b</PhoneticShape></Allomorph></Allomorphs>
            </LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

    /// Runs `pack <grammar> <out.pgpack> <extra_args...>` against a fresh scratch dir, returning `run_pack`'s `Result` and the output path (unread, so callers can assert a file was never created).
    fn run_pack_raw(
        tag: &str,
        grammar_xml: &str,
        extra_args: &[&str],
    ) -> (Result<(), String>, std::path::PathBuf) {
        let dir = scratch_dir(tag);
        let grammar_path = dir.join("grammar.xml");
        let out_path = dir.join("out.pgpack");
        std::fs::write(&grammar_path, grammar_xml).expect("write grammar");

        let mut args: Vec<String> = vec![
            grammar_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
        ];
        args.extend(extra_args.iter().map(|s| s.to_string()));

        (run_pack(&args), out_path)
    }

    /// A clean (`Admit`-verdict) grammar packs successfully and reads back `capability_trust=Proven`.
    #[test]
    fn pack_clean_grammar_writes_proven_manifest_and_round_trips() {
        let (result, out_path) = run_pack_raw("clean", CLEAN_GRAMMAR_XML, &[]);
        assert!(
            result.is_ok(),
            "clean grammar must pack successfully: {result:?}"
        );

        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let read = pg_pack::read_pack(&bytes).expect("a pack this command wrote must read back");
        assert_eq!(read.manifest.capability_trust, CapabilityTrust::Proven);
        assert!(!read.manifest.capability_trust.is_unproven());
        assert_eq!(read.manifest.grammar_id, "PackCleanFixture");
        assert!(
            read.manifest
                .required_runtime_features
                .runtime_operations
                .is_empty(),
            "a non-reduplicating grammar must declare no runtime operations"
        );
        assert_eq!(read.manifest.backend_assessments.len(), 3);
        assert!(read
            .manifest
            .backend_assessments
            .iter()
            .all(|assessment| assessment.status == "accepted"));
        let completeness = read
            .manifest
            .fst_completeness
            .as_ref()
            .expect("a real full FST payload must carry a completeness certificate");
        assert_eq!(completeness.backend, "tuned-surface-probed");
        assert_eq!(completeness.uncovered_constructs, 0);
        assert_eq!(completeness.pending_successors, 0);
        assert!(!completeness.enumeration_budget_exceeded);
        assert!(completeness.compiled_payload_present);
    }

    /// A `Refuse`-verdict grammar with no `--allow-unproven`: pack must fail and write no file.
    #[test]
    fn pack_refuse_grammar_without_override_fails_and_writes_no_file() {
        let (result, out_path) = run_pack_raw("refuse-no-override", REFUSE_GRAMMAR_XML, &[]);
        assert!(
            result.is_err(),
            "a Refuse-verdict grammar must fail pack without --allow-unproven: {result:?}"
        );
        assert!(
            !out_path.exists(),
            "no .pgpack may be written for a refused, non-overridden pack attempt"
        );
    }

    /// The same `Refuse`-verdict grammar with `--allow-unproven`: pack succeeds and reads back `Overridden` with the reason/authorized-by and refused construct(s) recorded.
    #[test]
    fn pack_refuse_grammar_with_allow_unproven_writes_overridden_manifest_with_refused_configs() {
        let (result, out_path) = run_pack_raw(
            "refuse-override",
            REFUSE_GRAMMAR_XML,
            &[
                "--allow-unproven",
                "--authorized-by=synthetic-test-operator",
                "--reason=synthetic field trial",
            ],
        );
        assert!(
            result.is_ok(),
            "--allow-unproven must force-pack a Refuse verdict: {result:?}"
        );

        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let read =
            pg_pack::read_pack(&bytes).expect("an overridden pack must still read back cleanly");
        assert!(read.manifest.capability_trust.is_unproven());
        match &read.manifest.capability_trust {
            CapabilityTrust::Overridden(record) => {
                assert_eq!(record.authorized_by, "synthetic-test-operator");
                assert_eq!(record.reason, "synthetic field trial");
                assert!(!record.recorded_at.is_empty());
                assert!(
                    record
                        .overridden_configs
                        .iter()
                        .any(|c| c.predicate == "reduplication.peel-eligible-rule-kind"),
                    "expected the construct the gated backend declined on: {:?}",
                    record.overridden_configs
                );
            }
            other => panic!("expected Overridden, got {other:?}"),
        }
        assert_eq!(read.manifest.backend_assessments.len(), 3);
        let tuned = read
            .manifest
            .backend_assessments
            .iter()
            .find(|assessment| assessment.backend == "tuned-surface-probed")
            .expect("the gated backend must have an assessment");
        assert_eq!(tuned.status, "refused");
        assert!(!tuned.failed_predicates.is_empty());
        assert!(tuned
            .findings
            .iter()
            .any(|finding| finding.override_record.is_some()));
        assert!(read
            .manifest
            .backend_assessments
            .iter()
            .any(|assessment| assessment.status == "refused"));
        assert!(
            read.manifest.fst_completeness.is_none(),
            "an overridden partial backend must not receive a completeness certificate"
        );
    }

    /// ADR 0005's indelibility invariant: an overridden pack's stamp survives write -> read and can never read back as `Proven`.
    #[test]
    fn overridden_pack_stamp_is_indelible_across_write_then_read() {
        let (result, out_path) = run_pack_raw(
            "indelible",
            REFUSE_GRAMMAR_XML,
            &["--allow-unproven", "--reason=synthetic indelibility check"],
        );
        assert!(result.is_ok());

        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let first_read = pg_pack::read_pack(&bytes).expect("first read");
        assert!(first_read.manifest.capability_trust.is_unproven());

        // Re-parse the same bytes again (a second, independent consumer): must still read back unproven.
        let second_read = pg_pack::read_pack(&bytes).expect("second read of the same bytes");
        assert_eq!(second_read.manifest, first_read.manifest);
        assert!(second_read.manifest.capability_trust.is_unproven());
        assert_eq!(
            second_read.manifest.capability_trust, first_read.manifest.capability_trust,
            "the override record itself must be byte-for-byte identical across reads"
        );
    }

    /// A reduplication-shaped grammar must declare `RUNTIME_FEATURE_REDUPLICATION_PEEL` (ADR 0004) regardless of its own capability verdict.
    #[test]
    fn pack_redup_grammar_declares_reduplication_peel_runtime_feature() {
        let (result, out_path) = run_pack_raw(
            "redup",
            REDUP_GRAMMAR_XML,
            &["--allow-unproven", "--reason=synthetic redup-feature check"],
        );
        assert!(result.is_ok(), "redup grammar must pack: {result:?}");

        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let read = pg_pack::read_pack(&bytes).expect("read redup pack");
        assert!(
            read.manifest
                .required_runtime_features
                .runtime_operations
                .iter()
                .any(|op| op == RUNTIME_FEATURE_REDUPLICATION_PEEL),
            "expected {RUNTIME_FEATURE_REDUPLICATION_PEEL:?} declared, got {:?}",
            read.manifest.required_runtime_features.runtime_operations
        );
    }

    /// Omitting `--authorized-by`/`--reason` on an override still records non-empty, honest default text, never a blank field.
    #[test]
    fn pack_override_without_authorized_by_or_reason_still_records_honest_defaults() {
        let (result, out_path) = run_pack_raw(
            "no-authorized-by",
            REFUSE_GRAMMAR_XML,
            &["--allow-unproven"],
        );
        assert!(result.is_ok());
        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let read = pg_pack::read_pack(&bytes).expect("read pack");
        match &read.manifest.capability_trust {
            CapabilityTrust::Overridden(record) => {
                assert!(!record.authorized_by.is_empty());
                assert!(!record.reason.is_empty());
            }
            other => panic!("expected Overridden, got {other:?}"),
        }
    }

    /// A real pack's foma payload round-trips via `read_foma_binary_payload`, matching an independent fresh compile's state/arc counts and `apply_up` results.
    #[test]
    fn pack_foma_payload_is_real_and_round_trips_via_fsm_read_binary_mem() {
        let (result, out_path) = run_pack_raw("foma-real-roundtrip", CLEAN_GRAMMAR_XML, &[]);
        assert!(
            result.is_ok(),
            "clean grammar must pack successfully: {result:?}"
        );

        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let read = pg_pack::read_pack(&bytes).expect("a pack this command wrote must read back");

        // This grammar's foma compile succeeds, so the packed foma section must be the real thing.
        assert_ne!(
            read.foma_payload, PLACEHOLDER_FOMA_PAYLOAD,
            "a compilable grammar's foma payload must be real bytes, not the fallback placeholder"
        );
        assert!(!read.foma_payload.is_empty());

        // Independent, from-scratch compile of the same grammar source, checked against the packed bytes.
        let grammar_path = out_path.with_file_name("grammar.xml");
        let (grammar, _warnings) = crate::load_grammar(&grammar_path.to_string_lossy())
            .expect("reload the same grammar.xml run_pack_raw wrote");
        let mut fresh_proposer = FomaProposer::new(&grammar)
            .expect("clean grammar must compile via a fresh FomaProposer");
        let (expected_states, expected_arcs) = fresh_proposer.network_counts();

        // Reconstruct the network from the packed bytes only, never re-deriving it from the grammar.
        let reconstructed = pg_foma::analyzer::read_foma_binary_payload(&read.foma_payload)
            .expect("a real foma payload must read back via fsm_read_binary_mem");
        assert_eq!(
            (reconstructed.statecount, reconstructed.arccount),
            (expected_states, expected_arcs),
            "reconstructed network's state/arc counts must match an independent fresh compile"
        );

        // apply_up agreement: the fixture's own lexicon entry analyzes identically on the original compile and the reconstructed network.
        let word = "kat";
        let original = fresh_proposer.apply_up_raw(word);
        let reconstructed_result = pg_foma::analyzer::apply_up_against(&reconstructed, word);
        assert_eq!(
            original, reconstructed_result,
            "apply_up({word:?}) must agree between the original compile and the payload \
             reconstructed from the packed bytes"
        );
        assert!(
            !original.is_empty(),
            "sanity: {word:?} is this fixture's own lexical entry and must analyze to \
             something on the original compile"
        );
    }
}
