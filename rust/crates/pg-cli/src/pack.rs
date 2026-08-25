//! Builds and writes a `.pgpack`: the ADR 0001/0005 capability-trust stamp, ADR 0004's required runtime features, FST health, and the foma/runtime payloads.

use std::fs;

use pg_foma::analyzer::FomaProposer;
use pg_foma::backend_selection::{select_backends, BackendReport, BackendSelection, BackendStatus};
use pg_foma::capability::CompileDecision;
use pg_foma::emit::{EmitReport, FomaTier};
use pg_foma::enumerate::EmissionStrategy;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::health::{
    FindingCode, HealthFinding, HealthReport, Metric, MetricValue, Phase, Severity, ValueProvenance,
};

use pg_foma::health_evaluator::{evaluate_foma_error, evaluate_health};
use pg_foma::peel::{ReduplicationPeeler, RUNTIME_FEATURE_REDUPLICATION_PEEL};
use pg_foma::resource_envelope::{CompileSizeMode, ResourceEnvelopeId};
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

#[cfg(feature = "developer-tools")]
const CAPABILITY_REFUSAL_REMEDIATION: &str =
    "Pass --allow-unproven (ADR 0005) to force-pack anyway -- the pack will be indelibly stamped capability_trust=Overridden/unproven.";
#[cfg(not(feature = "developer-tools"))]
const CAPABILITY_REFUSAL_REMEDIATION: &str =
    "The grammar is outside the production capability policy; consult the saved capability/readiness report or use a developer-tools build for an explicitly authorized override workflow.";

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
pub(crate) fn backend_assessments(
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

fn merge_gated_selection_findings(
    health: &mut HealthReport,
    selection: &BackendSelection,
    gated_backend: EmissionStrategy,
) {
    let Some(report) = selection.report_for(gated_backend) else {
        return;
    };
    for finding in report.findings() {
        if !health.findings.iter().any(|existing| existing == finding) {
            health.findings.push(finding.clone());
        }
    }
}

/// Certifies only full payloads with no uncovered construct, pending successor, or budget trip.
fn completeness_certificate(
    backend: EmissionStrategy,
    report: Option<&EmitReport>,
    compiled_payload_present: bool,
    capability_trust_proven: bool,
) -> Option<FstCompletenessCertificate> {
    let report = report?;
    let pending_successors = report
        .closure_refusal
        .as_ref()
        .and_then(|refusal| refusal.pending_successors)
        .unwrap_or(0);
    let complete = capability_trust_proven
        && compiled_payload_present
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

/// Applies readiness independently of capability trust; raw NotProductionReady/MachineLimit/CannotRepresent findings never admit.
pub(crate) fn validate_health_readiness(
    report: &HealthReport,
    worker_containment: bool,
) -> Result<(), String> {
    let admission = report.admission();
    let by_class = report.admission_by_class().render();
    if worker_containment {
        return Err(format!(
            "FST health is a worker containment failure; it cannot be overridden and no .pgpack was written ({by_class})"
        ));
    }
    if report
        .findings
        .iter()
        .any(|finding| finding.phase == Phase::Apply && finding.severity >= Severity::NotProductionReady)
    {
        return Err(format!(
            "FST health is an apply containment failure; it cannot be overridden and no .pgpack was written ({by_class})"
        ));
    }
    if report
        .findings
        .iter()
        .any(|finding| finding.severity >= Severity::NotProductionReady)
    {
        return Err(format!(
            "FST health is {admission:?}; no .pgpack was written. A correctness override cannot admit an oversized artifact, a contained attempt, or an unrepresentable feature. ({by_class})"
        ));
    }
    Ok(())
}

/// Records a missing serialized Foma network as a hard publication error.
fn record_foma_payload_availability(report: &mut HealthReport, payload_is_real: bool) {
    if payload_is_real {
        return;
    }

    report.findings.push(HealthFinding {
        code: FindingCode::BackendCompilationFailed,
        severity: Severity::NotProductionReady,
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

/// Removes overridden capability gaps from readiness; proven-route gaps remain CannotRepresent.
fn project_overridden_capability_findings(
    report: &mut HealthReport,
    capability_overridden: bool,
) {
    if capability_overridden {
        report
            .findings
            .retain(|finding| finding.code != FindingCode::BackendCoverageIncomplete);
    }
}

/// `pangloss pack <grammar> <out.pgpack> [--allow-unproven] [--authorized-by=<name>] [--reason=<text>] [--watchdog]`; `--watchdog` runs the FST-health compile in a killable child process instead of in-process.
pub fn run_pack(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut allow_unproven = false;
    #[cfg(feature = "developer-tools")]
    let mut remove_size_limits = false;
    let mut authorized_by: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut watchdog = false;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--allow-unproven" => {
                crate::accept_developer_flag(a)?;
                allow_unproven = true;
            }
            "--remove-size-limits" => {
                crate::accept_developer_flag(a)?;
                #[cfg(feature = "developer-tools")]
                {
                    remove_size_limits = true;
                }
            }
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
            s => {
                crate::reject_unknown_option(s)?;
                positional.push(s);
            }
        }
    }
    #[cfg(feature = "developer-tools")]
    let size_mode = if remove_size_limits {
        CompileSizeMode::DeveloperStress
    } else {
        CompileSizeMode::Managed
    };
    #[cfg(not(feature = "developer-tools"))]
    let size_mode = CompileSizeMode::Managed;
    let [grammar_path, out_path] = positional[..] else {
        return Err(format!(
            "usage: pack <grammar> <out.pgpack>{} [--authorized-by=<name>] \
                 [--reason=<text>] [--watchdog]",
            crate::PACK_DEVELOPER_HELP
        )
        .into());
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
        size_mode,
    )?;

    validate_health_readiness(&built.manifest.fst_health, built.worker_containment_failed)?;

    fs::write(out_path, &built.bytes).map_err(|e| format!("write {out_path}: {e}"))?;

    eprintln!(
        "pack complete: {out_path} ({} bytes) -- capability_trust={}, required_runtime_features={:?}, \
         fst_health admission={:?} ({}). NOTE: the runtime payload section is an honestly-labeled \
         PLACEHOLDER (no Rust-HermitCrab runtime-payload serializer exists yet anywhere in this \
         workspace -- see this module's own doc). The foma payload section is {} -- do not treat a \
         placeholder section as a usable compiled artifact.",
        built.bytes.len(),
        if built.manifest.capability_trust.is_unproven() { "overridden/unproven" } else { "proven" },
        built.manifest.required_runtime_features.runtime_operations,
        built.manifest.fst_health.admission(),
        built.manifest.fst_health.admission_by_class().render(),
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
    /// `true` iff `--watchdog` actually killed the compile-worker process; `false` whenever the worker ran to completion, regardless of the flag.
    pub worker_containment_failed: bool,
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
    size_mode: CompileSizeMode,
) -> Result<BuiltPack, String> {
    #[cfg(not(feature = "developer-tools"))]
    if allow_unproven {
        return Err(
            "--allow-unproven requires a pg-cli build with the developer-tools feature".to_string(),
        );
    }

    // ---- ADR 0001/0005: the capability-trust stamp ---------------------------------------------
    let backend = crate::GATED_BACKEND.label();
    let selection = select_backends(semantics);
    let decision = crate::gated_backend_decision(&selection);
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
                     construct(s); no .pgpack was written. {CAPABILITY_REFUSAL_REMEDIATION}\n",
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
    let capability_overridden = matches!(capability_trust, CapabilityTrust::Overridden(_));
    let mut worker_containment_failed = false;
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
        let (health, containment_failed) =
            run_fst_health_under_watchdog(grammar_path, size_mode)?;
        worker_containment_failed = containment_failed;
        (
            health.clone(),
            None,
            health.findings,
            Some("watchdog compilation did not return a serializable FST payload".to_string()),
            None,
        )
    } else {
        let (proposer_result, compile_profile) = {
            #[cfg(feature = "developer-tools")]
            {
                if capability_overridden {
                    FomaProposer::new_unproven_with_profile_for_mode(grammar, size_mode)
                } else {
                    FomaProposer::new_with_profile_for_mode(grammar, size_mode)
                }
            }
            #[cfg(not(feature = "developer-tools"))]
            {
                FomaProposer::new_with_profile_for_mode(grammar, size_mode)
            }
        };
        match &proposer_result {
            Ok(proposer) => {
                let foma_bytes = proposer.foma_binary_payload().map_err(|e| {
                    format!(
                        "serializing the compiled foma network to its binary-memory payload: {e}"
                    )
                })?;
                // The byte count actually destined for the .pgpack foma section, not a pre-serialization estimate.
                let health = evaluate_health(
                    Some(foma_bytes.len() as u64),
                    proposer.report.as_ref(),
                    &[],
                    &[],
                    Some(&compile_profile),
                );
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
    merge_gated_selection_findings(&mut fst_health, &selection, crate::GATED_BACKEND);
    project_overridden_capability_findings(&mut fst_health, capability_overridden);
    let payload_findings_start = fst_health.findings.len();
    record_foma_payload_availability(&mut fst_health, real_foma_payload.is_some());
    let mut assessment_compile_findings = gated_compile_findings;
    assessment_compile_findings.extend(
        fst_health.findings[payload_findings_start..]
            .iter()
            .cloned(),
    );
    let backend_assessments = backend_assessments(
        &selection,
        crate::GATED_BACKEND,
        &assessment_compile_findings,
        gated_compile_error.as_deref(),
        &fst_health,
    );
    let fst_completeness = completeness_certificate(
        crate::GATED_BACKEND,
        gated_emit_report.as_ref(),
        real_foma_payload.is_some(),
        !capability_overridden,
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
        resource_envelope_id: ResourceEnvelopeId::ManagedV1,
        compile_size_mode: size_mode,
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
        worker_containment_failed,
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

/// `true` iff the supervisor observed anything other than the child completing on its own -- never just the `--watchdog` flag having been passed.
fn worker_containment_fired(outcome: &pg_foma::worker::WorkerOutcome) -> bool {
    !matches!(outcome, pg_foma::worker::WorkerOutcome::Completed(_))
}

/// Runs the FST-health compile in a re-exec'd `__compile-worker-child` process under a killable watchdog, mapping the outcome to a `HealthReport` and reporting whether the supervisor actually killed the child (a real worker-containment event, as opposed to the child completing on its own).
fn run_fst_health_under_watchdog(
    grammar_path: &str,
    size_mode: CompileSizeMode,
) -> Result<(pg_foma::health::HealthReport, bool), String> {
    let format = infer_grammar_format(grammar_path);
    let mut request = pg_foma::worker::CompileWorkerRequest::new(grammar_path.to_string(), format);
    request.size_mode = size_mode;
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
    let worker_containment_failed = worker_containment_fired(&outcome);
    Ok((outcome.health_report(), worker_containment_failed))
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
    fn health_large_multiplier_publishes_without_override() {
        let report = synthetic_health(Severity::LargeMultiplier);
        assert!(validate_health_readiness(&report, false).is_ok());
        assert_eq!(report.admission(), Severity::LargeMultiplier);
        assert!(report.findings[0].override_record.is_none());
    }

    #[test]
    fn health_not_production_ready_refuses_publication_without_override() {
        let report = synthetic_health(Severity::NotProductionReady);
        let error = validate_health_readiness(&report, false).unwrap_err();
        assert!(error.contains("no .pgpack was written"));
        assert_eq!(report.admission(), Severity::NotProductionReady);
        assert!(report.findings[0].override_record.is_none());
    }

    /// The refusal message must name the failing axis, not just the collapsed severity band.
    #[test]
    fn readiness_refusal_message_names_the_failing_axis() {
        let report = synthetic_health(Severity::NotProductionReady);
        let error = validate_health_readiness(&report, false).unwrap_err();
        assert!(
            error.contains("containment=NotProductionReady"),
            "expected the per-axis breakdown in the refusal message: {error}"
        );
        assert!(error.contains("representability=WithinLimits"));
        assert!(error.contains("readiness=WithinLimits"));
        assert!(error.contains("process=WithinLimits"));
    }

    /// Regression guard: the richer refusal message must not move which reports get refused.
    #[test]
    fn validate_health_readiness_decision_matrix_is_unchanged() {
        let severities = [
            Severity::WithinLimits,
            Severity::Elevated,
            Severity::LargeMultiplier,
            Severity::NotProductionReady,
            Severity::MachineLimit,
            Severity::CannotRepresent,
        ];
        let phases = [Phase::Characterization, Phase::Compile, Phase::Apply];
        for &severity in &severities {
            for &phase in &phases {
                for &worker_containment in &[false, true] {
                    let mut report = synthetic_health(severity);
                    report.findings[0].phase = phase;

                    let expected_ok = !worker_containment
                        && !(phase == Phase::Apply && severity >= Severity::NotProductionReady)
                        && severity < Severity::NotProductionReady;

                    let actual_ok = validate_health_readiness(&report, worker_containment).is_ok();
                    assert_eq!(
                        actual_ok, expected_ok,
                        "severity={severity:?} phase={phase:?} worker_containment={worker_containment} \
                         must decide ok={expected_ok}"
                    );
                }
            }
        }
    }

    #[test]
    fn health_machine_limit_refuses_publication_without_override() {
        let report = synthetic_health(Severity::MachineLimit);
        let error = validate_health_readiness(&report, false).unwrap_err();
        assert!(error.contains("no .pgpack was written"));
        assert_eq!(report.admission(), Severity::MachineLimit);
    }

    #[cfg(feature = "developer-tools")]
    #[test]
    fn correctness_override_does_not_override_health_not_production_ready() {
        let report = synthetic_health(Severity::NotProductionReady);
        assert!(validate_health_readiness(&report, false).is_err());
        assert_eq!(report.admission(), Severity::NotProductionReady);
        assert_eq!(report.admission(), Severity::NotProductionReady);
        assert!(report.findings[0].override_record.is_none());
    }

    #[test]
    fn proven_route_keeps_unexpected_backend_coverage_gap_cannot_represent() {
        let mut report = synthetic_health(Severity::CannotRepresent);
        report.findings[0].code = FindingCode::BackendCoverageIncomplete;

        project_overridden_capability_findings(&mut report, false);

        assert_eq!(report.admission(), Severity::CannotRepresent);
        assert_eq!(report.findings.len(), 1);
        assert!(validate_health_readiness(&report, false).is_err());
    }

    #[test]
    fn gated_selection_findings_enter_health_before_projection() {
        let mut static_health = synthetic_health(Severity::NotProductionReady);
        let static_finding = static_health.findings.remove(0);
        let gated_report = BackendReport::accepted(
            crate::GATED_BACKEND,
            CompileDecision::Admit,
            vec![static_finding],
        )
        .expect("an admitted synthetic backend report must be constructible");
        let selection = BackendSelection::from_reports(vec![gated_report]);
        let mut health = HealthReport::new(Vec::new());

        merge_gated_selection_findings(&mut health, &selection, crate::GATED_BACKEND);

        assert_eq!(health.admission(), Severity::NotProductionReady);
        assert!(health
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::ResourceBudgetReached));
        assert!(validate_health_readiness(&health, false).is_err());

        let mut overridden = health;
        project_overridden_capability_findings(&mut overridden, true);
        assert_eq!(overridden.admission(), Severity::NotProductionReady);
        assert!(overridden
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::ResourceBudgetReached));
    }

    #[test]
    fn unproven_full_emit_never_receives_completeness_certificate() {
        let report = EmitReport {
            uncovered: Vec::new(),
            counts: pg_foma::emit::EmitCounts::default(),
            tier: FomaTier::Full,
            enum_budget_exceeded: None,
            closure_refusal: None,
            closure_evidence: None,
        };

        assert!(completeness_certificate(
            crate::GATED_BACKEND,
            Some(&report),
            true,
            false,
        )
        .is_none());
        assert!(completeness_certificate(
            crate::GATED_BACKEND,
            Some(&report),
            true,
            true,
        )
        .is_some());
    }

    #[cfg(feature = "developer-tools")]
    #[test]
    fn capability_override_does_not_admit_health_cannot_represent() {
        let mut report = synthetic_health(Severity::CannotRepresent);
        report.findings[0].code = FindingCode::BackendCoverageIncomplete;
        assert!(validate_health_readiness(&report, false).is_err());
        assert_eq!(
            report.admission(),
            Severity::CannotRepresent
        );
        assert_eq!(report.admission(), Severity::CannotRepresent);
        assert!(report.findings[0].override_record.is_none());
    }

    #[cfg(feature = "developer-tools")]
    #[test]
    fn health_apply_containment_cannot_be_overridden() {
        let mut report = synthetic_health(Severity::MachineLimit);
        report.findings[0].phase = Phase::Apply;
        let error = validate_health_readiness(&report, false).unwrap_err();
        assert!(error.contains("apply containment"));
        assert!(report.findings[0].override_record.is_none());
    }

    #[test]
    fn missing_foma_payload_is_an_error_before_publication() {
        let mut report = HealthReport::new(Vec::new());
        record_foma_payload_availability(&mut report, false);

        assert_eq!(report.admission(), Severity::NotProductionReady);
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
        assert!(validate_health_readiness(&report, false).is_err());
    }

    #[cfg(feature = "developer-tools")]
    #[test]
    fn missing_foma_payload_finding_reaches_gated_backend_assessment() {
        let (_result, out_path) = run_pack_raw(
            "missing-payload-assessment",
            REFUSE_GRAMMAR_XML,
            &["--allow-unproven", "--reason=missing payload assessment"],
        );
        let grammar_path = out_path.with_file_name("grammar.xml");
        let (grammar, _warnings) = crate::load_grammar(&grammar_path.to_string_lossy())
            .expect("reload the refused grammar");
        let semantics = GrammarSemantics::derive(&grammar);
        let built = build_pack(
            &grammar_path.to_string_lossy(),
            &grammar,
            &semantics,
            true,
            None,
            Some("missing payload assessment"),
            true,
            CompileSizeMode::Managed,
        )
        .expect("developer evidence pack may be built");

        let assessment = built
            .manifest
            .backend_assessments
            .iter()
            .find(|assessment| assessment.backend == crate::GATED_BACKEND.label())
            .expect("gated backend assessment");
        assert!(assessment
            .findings
            .iter()
            .any(|finding| finding.code == FindingCode::BackendCompilationFailed));
    }

    #[test]
    fn real_foma_payload_adds_no_availability_finding() {
        let mut report = HealthReport::new(Vec::new());
        record_foma_payload_availability(&mut report, true);

        assert!(report.findings.is_empty());
        assert_eq!(report.admission(), Severity::WithinLimits);
    }

    #[cfg(feature = "developer-tools")]
    #[test]
    fn missing_foma_payload_cannot_downgrade_or_override_machine_limit_worker_failure() {
        let mut report = synthetic_health(Severity::MachineLimit);
        record_foma_payload_availability(&mut report, false);

        let error = validate_health_readiness(&report, true).unwrap_err();
        assert!(error.contains("cannot be overridden"));
        assert_eq!(report.admission(), Severity::MachineLimit);
        assert!(report
            .findings
            .iter()
            .all(|finding| finding.override_record.is_none()));
    }

    /// A completed watchdog run must not be reported as a worker containment failure just because the flag was passed.
    #[test]
    fn watchdog_flag_without_actual_containment_does_not_report_containment_failure() {
        let health = synthetic_health(Severity::LargeMultiplier);
        let outcome = pg_foma::worker::WorkerOutcome::Completed(
            pg_foma::worker::CompileWorkerOutcome::Success {
                final_state_count: Some(1),
                final_arc_count: Some(1),
                uncovered_count: 0,
                health: health.clone(),
            },
        );

        assert!(!worker_containment_fired(&outcome));
        assert!(validate_health_readiness(&health, worker_containment_fired(&outcome)).is_ok());
    }

    /// The same mapping must still report a real supervisor-observed kill as a containment failure.
    #[test]
    fn watchdog_kill_is_reported_as_worker_containment_failure() {
        let health = synthetic_health(Severity::LargeMultiplier);
        let outcome = pg_foma::worker::WorkerOutcome::WallTimeoutKilled {
            elapsed: std::time::Duration::from_secs(5),
            limit: std::time::Duration::from_secs(1),
        };

        assert!(worker_containment_fired(&outcome));
        let error =
            validate_health_readiness(&health, worker_containment_fired(&outcome)).unwrap_err();
        assert!(error.contains("worker containment failure"));
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
    #[cfg(feature = "developer-tools")]
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

    /// Regression guard: a within-threshold real payload byte count must not trip `PayloadSizeBand`.
    #[test]
    fn a_within_threshold_pack_still_publishes_cleanly() {
        let (result, out_path) = run_pack_raw("within-threshold", CLEAN_GRAMMAR_XML, &[]);
        assert!(
            result.is_ok(),
            "a within-threshold grammar must still pack successfully: {result:?}"
        );

        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let read = pg_pack::read_pack(&bytes).expect("a pack this command wrote must read back");
        assert!(
            !read
                .manifest
                .fst_health
                .findings
                .iter()
                .any(|finding| finding.code == FindingCode::PayloadSizeBand),
            "a within-threshold compiled payload must produce no PayloadSizeBand finding: {:?}",
            read.manifest.fst_health.findings
        );
    }

    /// An oversized payload byte count must produce a `PayloadSizeBand` finding at `NotProductionReady`.
    #[test]
    fn an_oversized_payload_is_labelled_not_production_ready() {
        let oversized = pg_foma::health::IDEAL_MAX_BYTES + 1;
        let health = evaluate_health(Some(oversized), None, &[], &[], None);
        let finding = health
            .findings
            .iter()
            .find(|finding| finding.code == FindingCode::PayloadSizeBand)
            .expect("an oversized payload must produce a PayloadSizeBand finding");
        assert_eq!(finding.severity, Severity::NotProductionReady);
    }

    /// Injects the byte count at the `evaluate_health` seam rather than compiling a genuine >100MB network.
    #[test]
    fn an_oversized_pack_is_refused_publication() {
        let oversized = pg_foma::health::IDEAL_MAX_BYTES + 1;
        let health = evaluate_health(Some(oversized), None, &[], &[], None);
        assert!(
            validate_health_readiness(&health, false).is_err(),
            "an oversized payload must be refused publication"
        );

        // Mirrors `run_pack`'s own gate-then-write sequence, so a refusal here must leave no file.
        let dir = scratch_dir("oversized-refusal");
        let out_path = dir.join("out.pgpack");
        let attempt: Result<(), String> = (|| {
            validate_health_readiness(&health, false)?;
            fs::write(&out_path, b"unused").map_err(|e| e.to_string())?;
            Ok(())
        })();
        assert!(attempt.is_err());
        assert!(
            !out_path.exists(),
            "no .pgpack may be written for a refused oversized payload"
        );
    }

    /// A managed-mode pack records the shipped default envelope and `CompileSizeMode::Managed`.
    #[test]
    fn managed_build_records_managed_v1_envelope_and_size_mode() {
        let (result, out_path) = run_pack_raw("managed-envelope", CLEAN_GRAMMAR_XML, &[]);
        assert!(
            result.is_ok(),
            "clean grammar must pack successfully: {result:?}"
        );

        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let read = pg_pack::read_pack(&bytes).expect("a pack this command wrote must read back");
        assert_eq!(read.manifest.resource_envelope_id, ResourceEnvelopeId::ManagedV1);
        assert_eq!(read.manifest.compile_size_mode, CompileSizeMode::Managed);
    }

    /// Pin: a `--remove-size-limits` build must record `CompileSizeMode::DeveloperStress`, never silently fall back to `Managed`.
    #[cfg(feature = "developer-tools")]
    #[test]
    fn developer_stress_build_records_developer_stress_size_mode() {
        let (result, out_path) = run_pack_raw(
            "developer-stress-envelope",
            CLEAN_GRAMMAR_XML,
            &["--remove-size-limits"],
        );
        assert!(
            result.is_ok(),
            "clean grammar under --remove-size-limits must pack successfully: {result:?}"
        );

        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let read = pg_pack::read_pack(&bytes).expect("a pack this command wrote must read back");
        assert_eq!(
            read.manifest.compile_size_mode,
            CompileSizeMode::DeveloperStress
        );
        assert_eq!(
            read.manifest.resource_envelope_id,
            ResourceEnvelopeId::ManagedV1,
            "this step only records the envelope already in use, never changes which one is selected"
        );
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

    /// A refused grammar may write local evidence, but its trust stamp and absent certificate block production use.
    #[cfg(feature = "developer-tools")]
    #[test]
    fn pack_refuse_grammar_with_allow_unproven_writes_only_unproven_evidence() {
        let (result, out_path) = run_pack_raw(
            "refuse-override",
            REFUSE_GRAMMAR_XML,
            &[
                "--allow-unproven",
                "--authorized-by=synthetic-test-operator",
                "--reason=synthetic field trial",
            ],
        );
        assert!(result.is_ok(), "developer evidence pack must build: {result:?}");
        let bytes = std::fs::read(&out_path).expect("read local developer evidence pack");
        let read = pg_pack::read_pack(&bytes).expect("read local developer evidence pack");
        assert!(read.manifest.capability_trust.is_unproven());
        assert_eq!(read.manifest.fst_health.admission(), Severity::WithinLimits);
        assert!(read.manifest.fst_completeness.is_none());
    }

    /// ADR 0005's indelibility invariant: an overridden pack's stamp survives write -> read and can never read back as `Proven`.
    #[cfg(feature = "developer-tools")]
    #[test]
    fn overridden_pack_stamp_is_indelible_across_write_then_read() {
        let (result, out_path) = run_pack_raw(
            "indelible",
            REFUSE_GRAMMAR_XML,
            &["--allow-unproven", "--reason=synthetic indelibility check"],
        );
        assert!(result.is_ok(), "developer evidence pack must build: {result:?}");
        let bytes = std::fs::read(&out_path).expect("read local developer evidence pack");
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

    #[cfg(feature = "developer-tools")]
    #[test]
    fn refused_backend_findings_stay_in_backend_assessments_not_fst_readiness() {
        let (_result, out_path) = run_pack_raw(
            "readiness-separation",
            REFUSE_GRAMMAR_XML,
            &[
                "--allow-unproven",
                "--reason=synthetic readiness separation",
            ],
        );
        let grammar_path = out_path.with_file_name("grammar.xml");
        let (grammar, _warnings) = crate::load_grammar(&grammar_path.to_string_lossy())
            .expect("reload the refused grammar");
        let semantics = GrammarSemantics::derive(&grammar);
        let built = build_pack(
            &grammar_path.to_string_lossy(),
            &grammar,
            &semantics,
            true,
            None,
            Some("synthetic readiness separation"),
            false,
            CompileSizeMode::Managed,
        )
        .expect("capability override may collect an evidence pack");

        assert_eq!(built.manifest.fst_health.admission(), Severity::WithinLimits);
        assert!(built
            .manifest
            .fst_health
            .findings
            .iter()
            .all(|finding| finding.code != FindingCode::BackendCoverageIncomplete));
        assert!(built
            .manifest
            .backend_assessments
            .iter()
            .flat_map(|assessment| assessment.findings.iter())
            .any(|finding| finding.code == FindingCode::BackendCoverageIncomplete));
    }

    /// A reduplication-shaped grammar must declare `RUNTIME_FEATURE_REDUPLICATION_PEEL` (ADR 0004) regardless of its own capability verdict.
    #[cfg(feature = "developer-tools")]
    #[test]
    fn pack_redup_grammar_declares_reduplication_peel_runtime_feature() {
        let (result, out_path) = run_pack_raw(
            "redup",
            REDUP_GRAMMAR_XML,
            &["--allow-unproven", "--reason=synthetic redup-feature check"],
        );
        assert!(result.is_ok(), "developer evidence pack must build: {result:?}");
        let bytes = std::fs::read(&out_path).expect("read local developer evidence pack");
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
    #[cfg(feature = "developer-tools")]
    #[test]
    fn pack_override_without_authorized_by_or_reason_still_records_honest_defaults() {
        let (result, out_path) = run_pack_raw(
            "no-authorized-by",
            REFUSE_GRAMMAR_XML,
            &["--allow-unproven"],
        );
        assert!(result.is_ok(), "developer evidence pack must build: {result:?}");
        let bytes = std::fs::read(&out_path).expect("read local developer evidence pack");
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
