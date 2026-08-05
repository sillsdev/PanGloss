//! `pangloss pack` — closes the ADR 0005 loop: `main.rs`'s `--allow-unproven` override path used
//! to say, verbatim, "No `.pgpack` packaging exists yet to carry ADR 0005's persistent, indelible
//! manifest stamp; this is a SESSION/REPORT-LEVEL marker only for this invocation." `pg-pack` now
//! defines exactly the types that stamp needs (`pg_pack::PackManifest`/`CapabilityTrust`/
//! `CapabilityOverrideRecord`); this module is the first real producer that writes one.
//!
//! `pack <grammar> <out.pgpack> [--allow-unproven] [--authorized-by=<name>] [--reason=<text>]`
//! loads `grammar` via [`crate::load_grammar`] (the same `.xml`/`.json`/`.fwdata` dispatch every
//! other subcommand uses), runs [`pg_foma::capability_entry::evaluate_capability`], and writes a
//! `.pgpack` via [`pg_pack::write_pack`] carrying:
//!
//! - **`capability_trust`** (ADR 0005, `docs/adr/0005-capability-override-unproven-grammars.md`):
//!   `Proven` for `Admit`/`ConfirmOnly` (ADR 0001: `ConfirmOnly` is a first-class non-failure
//!   verdict, not a degraded one); `Overridden` — with a populated
//!   [`pg_pack::CapabilityOverrideRecord`] naming who/why/when and every refused construct — for a
//!   `Refuse` verdict force-packed via `--allow-unproven`. A `Refuse` verdict WITHOUT
//!   `--allow-unproven` fails this command outright, before any file is written — the same
//!   never-overclaim discipline `main.rs`'s `run_capability_gate` already enforces for
//!   `batch`/`parse`, applied here to packaging instead of analysis.
//! - **`required_runtime_features`** (ADR 0004, `docs/adr/0004-runtime-feature-compatibility.md`):
//!   declares [`pg_foma::peel::RUNTIME_FEATURE_REDUPLICATION_PEEL`] iff
//!   `pg_foma::peel::ReduplicationPeeler::has_redup_rules()` — exactly the wiring that constant's
//!   own doc flagged as "not yet wired into `pg-pack`... whenever it lands."
//! - **`fst_health`**: `pg_foma::health_evaluator::evaluate_health`'s [`pg_foma::health::HealthReport`],
//!   fed from a standalone [`pg_foma::analyzer::FomaProposer::new_with_profile`] compile (this
//!   command's own second compiled network — the same "acceptable one-time cost for an offline
//!   diagnostic tool" judgment call `diagnostics.rs::assess_words` already makes, for the identical
//!   reason: `FomaAnalyzer` does not expose its own internal proposer/profile for external reuse).
//!
//! # What is real vs. placeholder in the payload sections (read before trusting a produced pack)
//! **The foma payload is now REAL.** `foma::io::fsm_write_binary` (a gzip'd write over any
//! `std::io::Write`, mirroring the crate's own `fsm_write_binary_file`) turned out to already exist
//! in the vendored `foma = "0.4.0"` dependency — `pg_foma::analyzer::FomaProposer::
//! foma_binary_payload` serializes the SAME compiled network this command already builds once for
//! `fst_health` (no third compile), and that is exactly the bytes written into the foma payload
//! section below. This is foma's own existing binary-memory encoding, the same one
//! `foma::io::fsm_read_binary_mem` reads back — no second network format was invented (R2A). The
//! ONE case this command cannot produce real foma bytes for is when this same compile does not
//! succeed (an emit/lexc-compile failure, an enumeration-budget refusal) or `--watchdog` is passed
//! (its worker protocol ships back only a `HealthReport`, not the compiled network) — that pack's
//! foma section falls back to the same honestly-labeled [`PLACEHOLDER_FOMA_PAYLOAD`] this module
//! always used.
//!
//! **The runtime payload is still a placeholder.** No Rust-HermitCrab runtime-payload serializer
//! exists anywhere in this workspace: `pg_grammar::model::Grammar` — the struct the
//! `pg-parse`/`pg-rules` HermitCrab port actually analyzes against — derives `serde::Serialize` on
//! almost none of its dozens of constituent types (only `StratumId`/`MprSet` do), and carries a
//! `pg_featstruct::Interner<FeatureStruct>` whose generic `Interner<V>` container has no serde impl
//! of its own either. Making the whole object graph round-trip is a large, separate serialization
//! effort (dozens of new `#[derive(Serialize, Deserialize)]`s plus at least one hand-written
//! `Interner` impl), not something this additive step invents. Rather than writing an empty byte
//! string (indistinguishable from "a real, empty payload") or fabricating bytes that *look* like a
//! real payload, the runtime section still carries the literal, human-readable
//! [`PLACEHOLDER_RUNTIME_PAYLOAD`] label as its actual content — unmissable to anyone who inspects a
//! produced `.pgpack`'s raw bytes, and `run_pack`'s own stderr summary repeats which section is
//! real vs. placeholder at pack time. **Everything else in the manifest — capability trust,
//! required runtime features, FST health, and (when the compile succeeds) the foma payload itself
//! — is real, measured from/derived from this exact grammar, never a placeholder.**

use std::fs;

use pg_foma::analyzer::FomaProposer;
use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::evaluate_capability_with_semantics;
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::health_evaluator::evaluate_health;
use pg_foma::peel::{ReduplicationPeeler, RUNTIME_FEATURE_REDUPLICATION_PEEL};
use pg_grammar::model::Grammar;
use pg_pack::{
    CapabilityOverrideRecord, CapabilityTrust, OverriddenConfig, PackManifest,
    RequiredRuntimeFeatures, MANIFEST_FORMAT_TAG, MANIFEST_SCHEMA_VERSION,
};

/// This build's own foma-feature level (ADR 0004's own dimension) — mirrors `pg-wasm`'s
/// `provided_runtime_features`'s identical constant/doc (`pg-wasm/src/pack.rs`): a plain,
/// hand-bumped compile-time capability declaration, not derived from any registry.
const FOMA_FEATURE_LEVEL: u32 = 1;

/// Honestly-labeled placeholder runtime payload — see this module's top doc, "What is real vs.
/// placeholder." Its content is never mistaken for a real payload precisely because it announces
/// itself as one.
const PLACEHOLDER_RUNTIME_PAYLOAD: &[u8] =
    b"PANGLOSS-PLACEHOLDER-RUNTIME-PAYLOAD: no Rust-HermitCrab \
runtime-payload serializer exists yet anywhere in this workspace; this byte content is NOT a \
compiled artifact and must never be loaded as one.";

/// Honestly-labeled placeholder foma payload — used only as a FALLBACK now (see this module's top
/// doc, "What is real vs. placeholder"): whenever this grammar's own foma compile succeeds (the
/// common case, `--watchdog` not passed), `run_pack` writes the real
/// `FomaProposer::foma_binary_payload()` bytes instead of this constant.
const PLACEHOLDER_FOMA_PAYLOAD: &[u8] = b"PANGLOSS-PLACEHOLDER-FOMA-PAYLOAD: this grammar's foma \
compile did not succeed (or --watchdog was passed), so no compiled network was available to \
serialize; this byte content is NOT a compiled network and must never be loaded as one.";

/// This crate's own `Cargo.toml` semantic version, read from the compile-time
/// `CARGO_PKG_VERSION_*` environment variables — used as the Rust-HermitCrab port version this
/// pack's `required_runtime_features.hc_port_semver` declares. Mirrors `pg-wasm/src/pack.rs`'s
/// identical `this_crate_semver` helper (every workspace crate shares one
/// `version.workspace = true` value, so this is the same number `pg-foma`/`pg-parse` ship at).
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

/// A plain caller-supplied timestamp string: `unix:<seconds-since-epoch>`. Matches
/// `pg_foma::health::OverrideRecord::recorded_at`/`pg_pack::CapabilityOverrideRecord::recorded_at`'s
/// own documented "no timestamp type dependency" convention -- this is a real wall-clock reading,
/// just not typed as a dedicated timestamp value.
fn now_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

/// `pangloss pack <grammar> <out.pgpack> [--allow-unproven] [--authorized-by=<name>]
/// [--reason=<text>] [--watchdog]` — see this module's top doc for the full contract.
/// `--authorized-by`/`--reason` are only consulted when a `Refuse` verdict is actually
/// force-packed via `--allow-unproven` (ADR 0005's override record); given without
/// `--allow-unproven`, or on a grammar that never reaches `Refuse`, they are silently inert --
/// same "meaningless without enforcement" contract `main.rs`'s `--allow-unproven` already
/// documents for `batch`/`parse`.
///
/// # `--watchdog` (`harden-foma-resource-safety` section 3/4; `pg_foma::worker`'s own doc)
/// OPT-IN ONLY -- see this module's own top doc "What is real vs. placeholder" section for
/// context: this command already runs one standalone, potentially-adversarial foma compile purely
/// to produce `fst_health` (`FomaProposer::new_with_profile`, a second compiled network from the
/// SAME judgment call this module's top doc already documents). Without `--watchdog` (the
/// default), that compile runs exactly as it always has, in-process -- BYTE-FOR-BYTE UNCHANGED
/// behavior, output, and exit codes for every existing invocation. With `--watchdog`, that ONE
/// compile is instead routed through `pg_foma::worker::run_compile_worker`: this process re-execs
/// itself (`std::env::current_exe()`) with the hidden `__compile-worker-child` subcommand
/// (`main.rs`'s own dispatch), which calls `pg_foma::worker::run_worker_child` on its own
/// stdin/stdout. The child's compile runs under a killable watchdog (wall-clock deadline, sampled
/// RSS, bounded I/O) instead of this process's own stack/heap -- a hung or resource-runaway
/// compile can no longer take this whole `pangloss pack` invocation down with it. Every other part
/// of this command (grammar load, capability-trust evaluation, the manifest/payload write) is
/// unaffected by this flag either way.
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

/// The result of one `.pgpack` build: the assembled manifest, the full container bytes
/// ([`pg_pack::write_pack`]'s own output), and whether the foma payload section inside those bytes
/// is real compiled-network bytes or the honestly-labeled placeholder fallback (see this module's
/// top doc, "What is real vs. placeholder"). Factored out of [`run_pack`] so `pangloss make-report`
/// can share the SAME real pack-build logic — a real trust stamp and a real artifact size —
/// without going through `run_pack`'s own CLI arg-parsing/file-writing/stderr-summary shell, so
/// both call sites share one implementation rather than `make-report` re-deriving a second,
/// potentially-drifting notion of "what a pack is."
pub(crate) struct BuiltPack {
    pub manifest: PackManifest,
    pub bytes: Vec<u8>,
    /// `true` iff [`BuiltPack::bytes`]'s foma payload section is the grammar's own real compiled
    /// network (`FomaProposer::foma_binary_payload`), `false` iff it is the honestly-labeled
    /// [`PLACEHOLDER_FOMA_PAYLOAD`] fallback (compile did not succeed, or `watchdog` was requested).
    pub foma_payload_is_real: bool,
}

/// Builds one `.pgpack` in memory: the ADR 0001/0005 capability-trust stamp, the ADR 0004
/// required-runtime-feature set, the FST-health report (+ the real foma payload when this same
/// compile succeeds), and the assembled, written [`pg_pack::write_pack`] container bytes — see
/// [`run_pack`]'s own top-of-module doc for the full contract this implements (every side effect
/// and stderr diagnostic below is identical to what `run_pack` always printed; this function is a
/// pure extraction, not a behavior change).
///
/// `semantics` must be [`pg_foma::grammar_semantics::GrammarSemantics::derive`]d from `grammar`.
/// Taking it rather than deriving it
/// here is what lets `pangloss make-report` — which needs the capability verdict in its own
/// preamble, here, and again in `readiness_verdict::certify` — pay for the grammar walk once
/// instead of three times.
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
    let decision = evaluate_capability_with_semantics(semantics);
    let capability_trust = match &decision {
        CompileDecision::Admit => {
            eprintln!(
                "capability: Admit -- packing a proven-clean grammar (capability_trust=Proven)"
            );
            CapabilityTrust::Proven
        }
        CompileDecision::ConfirmOnly => {
            eprintln!(
                "capability: ConfirmOnly -- packing (ADR 0001: first-class, recall-preserving via \
                 confirm, not a failure; capability_trust=Proven)"
            );
            CapabilityTrust::Proven
        }
        CompileDecision::Refuse(diags) => {
            if !allow_unproven {
                let mut msg = format!(
                    "capability gate refused this grammar ({} diagnostic(s)); no .pgpack was \
                     written. Pass --allow-unproven (ADR 0005) to force-pack anyway -- the pack \
                     will be indelibly stamped capability_trust=Overridden/unproven.\n",
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
                "CAPABILITY-OVERRIDE trust=unproven: --allow-unproven force-packing {} refused \
                 construct(s) (ADR 0005) -- this pack's capability_trust is Overridden, PERSISTENT, \
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

    // ---- FST health (+ the REAL foma payload, when this same compile succeeds): a standalone
    // profiled compile, mirroring diagnostics.rs's own "a second compiled network is an acceptable
    // one-time cost for an offline tool" judgment call ------------------------------------------
    // `--watchdog` (this module's own doc): OPT-IN. Default path (watchdog == false) is BYTE-FOR-
    // BYTE the pre-existing in-process compile -- unchanged, PLUS it now also serializes that same
    // compiled network via [`pg_foma::analyzer::FomaProposer::foma_binary_payload`] (foma's own
    // existing binary-memory encoding -- R2A forbids inventing a second network format) so this
    // command no longer has to compile the grammar a THIRD time just to get the foma payload bytes.
    // `--watchdog`'s worker protocol (`pg_foma::worker::WorkerOutcome`) only ships a `HealthReport`
    // back across the process boundary today, never the compiled network itself, so the foma
    // payload stays an honest placeholder on that path (see the stderr note below).
    let (fst_health, real_foma_payload): (pg_foma::health::HealthReport, Option<Vec<u8>>) =
        if watchdog {
            (run_fst_health_under_watchdog(grammar_path)?, None)
        } else {
            let (proposer_result, compile_profile) = FomaProposer::new_with_profile(grammar);
            match &proposer_result {
                Ok(proposer) => {
                    let health = evaluate_health(
                        None,
                        Some(&proposer.report),
                        &[],
                        &[],
                        Some(&compile_profile),
                    );
                    let foma_bytes = proposer.foma_binary_payload().map_err(|e| {
                        format!(
                        "serializing the compiled foma network to its binary-memory payload: {e}"
                    )
                    })?;
                    (health, Some(foma_bytes))
                }
                Err(pg_foma::analyzer::FomaError::LexcCompileFailed(report)) => (
                    evaluate_health(None, Some(report), &[], &[], Some(&compile_profile)),
                    None,
                ),
                Err(_) => (
                    evaluate_health(None, None, &[], &[], Some(&compile_profile)),
                    None,
                ),
            }
        };
    // `None` iff `--watchdog` was used, or this grammar's own foma compile did not succeed (its
    // capability_trust may still be Proven/Overridden -- capability trust and foma-compile success
    // are independent axes, see `pg_foma::capability_entry`'s own doc) -- either way, a real
    // compiled network's bytes are simply not available to package, so this falls back to the same
    // honestly-labeled placeholder this module always used for the foma section.
    let foma_payload: &[u8] = real_foma_payload
        .as_deref()
        .unwrap_or(PLACEHOLDER_FOMA_PAYLOAD);

    // ---- Payloads: the foma section is REAL whenever `real_foma_payload` is `Some` (see above);
    // the runtime section remains an honestly-labeled placeholder (see this module's top doc: no
    // Rust-HermitCrab runtime-payload serializer exists anywhere in this workspace yet) ------------
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

/// `grammar_path`'s extension -> [`pg_foma::worker::GrammarFormat`], mirroring `crate::
/// load_grammar`'s own three-way extension dispatch exactly (`.json` -> `Json`, `.fwdata` ->
/// `Fwdata`, anything else including `.xml` -> `Xml`) so the watchdog path names the SAME format
/// the non-watchdog path would have loaded.
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

/// `--watchdog`'s implementation (this module's own doc): re-execs this same `pangloss` binary as
/// the hidden `__compile-worker-child` subcommand (`main.rs`'s dispatch) via
/// [`pg_foma::worker::run_compile_worker`], under [`pg_foma::worker::WatchdogEnvelope::
/// default_envelope`], and maps whatever [`pg_foma::worker::WorkerOutcome`] comes back into the
/// same [`pg_foma::health::HealthReport`] the non-watchdog path already produces
/// ([`pg_foma::worker::WorkerOutcome::health_report`] handles every variant, including a real
/// `Completed(Success)`'s own real report, uniformly -- no separate match needed here).
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

    /// A fresh, collision-free scratch directory per test (mirrors `main.rs`'s own test-module
    /// convention).
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

    /// An ordinary, capability-`Admit` grammar: one bare root, no `Compounding`, no `Unordered`
    /// strata, no reduplication -- the same shape `main.rs`'s own `MINI_GRAMMAR_XML` fixture uses.
    const CLEAN_GRAMMAR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>PackCleanFixture</Name>
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

    /// An `Overwrite`-output `MprGroup` -- `MprGroupOverwriteFailClosedPredicate` (`pg_foma::
    /// capability`) `Refuse`s this UNCONDITIONALLY and PERMANENTLY (a monotone-accumulation
    /// admission filter is structurally unsound for history-dependent `Overwrite` replace
    /// semantics, `pg_grammar::model::mpr_add_output`'s own doc) -- no promotion can ever flip this
    /// fixture's own verdict. A self-feeding (`multipleApplication="2"`) `Compounding`
    /// rule is NOT a safe "known-Refuse" fixture here, because `compounding.recursive` is a
    /// `ConfigPredicate`-disposition construct and can be promoted to `ConfirmOnly` by a later
    /// grammar/capability change. Do not point a future "known-Refuse" fixture at any
    /// `ConfigPredicate`-disposition construct (every one of those has at least one promotable
    /// configuration) -- `MprGroupOverwrite` is the
    /// stable, by-construction-permanent choice (`main.rs`'s own
    /// `capability_gate_tests::PERMANENTLY_REFUSED_GRAMMAR_XML`, same swap, same rationale).
    const REFUSE_GRAMMAR_XML: &str = include_str!("../../../../conformance-staging/edge-cases/simultaneous-subrule-genuine-overlap/grammar.xml");

    /// A grammar with one ordinary `MorphologicalRule` whose subrule's output copies the SAME input
    /// part TWICE (`<CopyFromInput index="stem" />` repeated) -- `pg_foma::emit::classify_affix`'s
    /// exact `Role::Reduplication` trigger (any `PartRef` echoed >= 2 times via `Copy`), so
    /// `ReduplicationPeeler::has_redup_rules()` is `true` for this fixture (`is_reduplication_rule`
    /// only inspects the RHS shape, independent of whether this compiles/confirms cleanly).
    const REDUP_GRAMMAR_XML: &str = r#"<HermitCrabInput><Language><Name>PackRedupFixture</Name>
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

    /// Runs `pack <grammar> <out.pgpack> <extra_args...>` against a fresh scratch dir, returning
    /// `run_pack`'s own `Result` plus the `out.pgpack` path (deliberately not read-and-unwrapped
    /// here -- the refuse-without-override test needs to assert the file was never created).
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
    }

    /// A `Refuse`-verdict grammar with NO `--allow-unproven`: the command must fail, and -- unlike
    /// a bare error report -- no `.pgpack` file may exist at all afterward (never a partial/empty
    /// artifact).
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

    /// The same `Refuse`-verdict grammar WITH `--allow-unproven --authorized-by=... --reason=...`:
    /// the command succeeds, and the pack reads back `Overridden` with the reason/authorized-by
    /// recorded and the refused construct(s) named -- ADR 0005's full override record, not just a
    /// boolean flag.
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
                        .any(|c| c.predicate == "simultaneous.subrule-overlap"),
                    "expected the genuine simultaneous-overlap refusal config: {:?}",
                    record.overridden_configs
                );
            }
            other => panic!("expected Overridden, got {other:?}"),
        }
    }

    /// ADR 0005's own indelibility invariant, checked directly (not just implied by the test
    /// above): an overridden pack's stamp survives write -> read byte-for-byte and can never be
    /// read back as a clean `Proven` claim -- there is no field/flag a consumer could flip.
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

        // Re-parse the same bytes again (simulating a second, independent consumer) -- the record
        // must be identical and still unproven; nothing about a mere re-read can launder it.
        let second_read = pg_pack::read_pack(&bytes).expect("second read of the same bytes");
        assert_eq!(second_read.manifest, first_read.manifest);
        assert!(second_read.manifest.capability_trust.is_unproven());
        assert_eq!(
            second_read.manifest.capability_trust, first_read.manifest.capability_trust,
            "the override record itself must be byte-for-byte identical across reads"
        );
    }

    /// A grammar whose only morphological rule is reduplication-shaped
    /// (`ReduplicationPeeler::has_redup_rules() == true`) must declare
    /// [`RUNTIME_FEATURE_REDUPLICATION_PEEL`] in the packed manifest's
    /// `required_runtime_features.runtime_operations` (ADR 0004). `--allow-unproven` is passed
    /// unconditionally here since this test's only concern is the runtime-feature declaration, not
    /// this fixture's own capability verdict (which may legitimately be `ConfirmOnly` or `Refuse`
    /// depending on how the reduplication-support predicate classifies it -- see
    /// `pg_foma::peel`'s own module doc on why that classification preserves recall).
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

    /// `--authorized-by`/`--reason` omitted on an overridden pack: the record still gets honest,
    /// non-empty placeholder text (never empty strings) -- ADR 0005 asks for who/why/when to be
    /// recorded, so an omitted flag degrades to a labeled default, never a blank field.
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

    /// The foma payload a real `pangloss pack` writes is REAL compiled-network bytes, not the
    /// [`PLACEHOLDER_FOMA_PAYLOAD`] fallback -- and those bytes actually round-trip:
    /// `pg_foma::analyzer::read_foma_binary_payload` (`foma::io::fsm_read_binary_mem` under the
    /// hood) reconstructs a network with the SAME state/arc counts as an independent, from-scratch
    /// compile of the identical grammar, and `apply_up` agrees on every word in this fixture's own
    /// lexicon between the original compile and the reconstructed twin. This is the round-trip
    /// evidence for "the gap" this module's top doc describes: a produced `.pgpack`'s foma section
    /// is a genuine, reloadable compiled artifact, not a label pretending to be one.
    #[test]
    fn pack_foma_payload_is_real_and_round_trips_via_fsm_read_binary_mem() {
        let (result, out_path) = run_pack_raw("foma-real-roundtrip", CLEAN_GRAMMAR_XML, &[]);
        assert!(
            result.is_ok(),
            "clean grammar must pack successfully: {result:?}"
        );

        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let read = pg_pack::read_pack(&bytes).expect("a pack this command wrote must read back");

        // Not the honest fallback placeholder -- this grammar's foma compile succeeds, so the
        // packed foma section must be the real thing.
        assert_ne!(
            read.foma_payload, PLACEHOLDER_FOMA_PAYLOAD,
            "a compilable grammar's foma payload must be real bytes, not the fallback placeholder"
        );
        assert!(!read.foma_payload.is_empty());

        // Independent, from-scratch compile of the SAME grammar source -- this is the "expected"
        // side the packed bytes are checked against, deliberately built via a fresh
        // `FomaProposer::new` call (not reusing anything `run_pack` built) so this test actually
        // exercises the serialized bytes end-to-end rather than comparing an object to itself.
        let grammar_path = out_path.with_file_name("grammar.xml");
        let (grammar, _warnings) = crate::load_grammar(&grammar_path.to_string_lossy())
            .expect("reload the same grammar.xml run_pack_raw wrote");
        let mut fresh_proposer = FomaProposer::new(&grammar)
            .expect("clean grammar must compile via a fresh FomaProposer");
        let (expected_states, expected_arcs) = fresh_proposer.network_counts();

        // Reconstruct the network from the PACKED bytes (never re-deriving it from the grammar) --
        // this is the read side of the exact gap this task closes.
        let reconstructed = pg_foma::analyzer::read_foma_binary_payload(&read.foma_payload)
            .expect("a real foma payload must read back via fsm_read_binary_mem");
        assert_eq!(
            (reconstructed.statecount, reconstructed.arccount),
            (expected_states, expected_arcs),
            "reconstructed network's state/arc counts must match an independent fresh compile"
        );

        // `apply_up` agreement: the fixture's own lexicon entry ("kat", see CLEAN_GRAMMAR_XML)
        // analyzes identically on the original LIVE compile (`apply_up_raw`, over
        // `fresh_proposer`'s own handle) and the network reconstructed from the packed bytes
        // (`apply_up_against`, over `reconstructed`) -- two independent code paths converging on
        // the same network content, not one side re-deriving from the other.
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
