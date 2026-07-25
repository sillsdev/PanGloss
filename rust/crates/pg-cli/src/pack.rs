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
//! `pg-pack`'s own crate doc is explicit that "wiring a real compiler pass to produce the
//! runtime/foma payload bytes... is later work" — no Rust-HermitCrab runtime-payload serializer
//! exists anywhere in this workspace yet, and no foma binary-memory export (a
//! `foma::io::fsm_write_binary_mem` equivalent) exists in `pg-foma` yet either. Rather than writing
//! empty byte strings (indistinguishable from "a real, empty payload") or fabricating bytes that
//! *look* like a compiled artifact, both payload sections carry a literal, human-readable
//! [`PLACEHOLDER_RUNTIME_PAYLOAD`]/[`PLACEHOLDER_FOMA_PAYLOAD`] label as their actual content —
//! unmissable to anyone who inspects a produced `.pgpack`'s raw bytes, and `run_pack`'s own stderr
//! summary repeats the warning at pack time. **Everything else in the manifest — capability trust,
//! required runtime features, FST health — is real, measured from this exact grammar, not a
//! placeholder.**

use std::fs;

use pg_foma::analyzer::FomaProposer;
use pg_foma::capability::CompileDecision;
use pg_foma::capability_entry::evaluate_capability;
use pg_foma::health_evaluator::evaluate_health;
use pg_foma::peel::{ReduplicationPeeler, RUNTIME_FEATURE_REDUPLICATION_PEEL};
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
const PLACEHOLDER_RUNTIME_PAYLOAD: &[u8] = b"PANGLOSS-PLACEHOLDER-RUNTIME-PAYLOAD: no Rust-HermitCrab \
runtime-payload serializer exists yet anywhere in this workspace; this byte content is NOT a \
compiled artifact and must never be loaded as one.";

/// Honestly-labeled placeholder foma payload — see this module's top doc, "What is real vs.
/// placeholder."
const PLACEHOLDER_FOMA_PAYLOAD: &[u8] = b"PANGLOSS-PLACEHOLDER-FOMA-PAYLOAD: no foma binary-memory \
(fsm_write_binary_mem-equivalent) export exists yet in pg-foma; this byte content is NOT a \
compiled network and must never be loaded as one.";

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
        MAJOR.parse().expect("CARGO_PKG_VERSION_MAJOR is always numeric"),
        MINOR.parse().expect("CARGO_PKG_VERSION_MINOR is always numeric"),
        PATCH.parse().expect("CARGO_PKG_VERSION_PATCH is always numeric"),
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
/// [--reason=<text>]` — see this module's top doc for the full contract. `--authorized-by`/
/// `--reason` are only consulted when a `Refuse` verdict is actually force-packed via
/// `--allow-unproven` (ADR 0005's override record); given without `--allow-unproven`, or on a
/// grammar that never reaches `Refuse`, they are silently inert -- same "meaningless without
/// enforcement" contract `main.rs`'s `--allow-unproven` already documents for `batch`/`parse`.
pub fn run_pack(args: &[String]) -> Result<(), String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut allow_unproven = false;
    let mut authorized_by: Option<String> = None;
    let mut reason: Option<String> = None;

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
            s => positional.push(s),
        }
    }
    let [grammar_path, out_path] = positional[..] else {
        return Err(
            "usage: pack <grammar> <out.pgpack> [--allow-unproven] [--authorized-by=<name>] \
             [--reason=<text>]"
                .into(),
        );
    };

    let (grammar, warnings) = crate::load_grammar(grammar_path)?;
    crate::print_grammar_warnings(&warnings);

    // ---- ADR 0001/0005: the capability-trust stamp ---------------------------------------------
    let decision = evaluate_capability(&grammar);
    let capability_trust = match &decision {
        CompileDecision::Admit => {
            eprintln!("capability: Admit -- packing a proven-clean grammar (capability_trust=Proven)");
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
                authorized_by: authorized_by
                    .clone()
                    .unwrap_or_else(|| "unspecified (--allow-unproven with no --authorized-by given)".to_string()),
                reason: reason
                    .clone()
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
    let peeler = ReduplicationPeeler::new(&grammar);
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

    // ---- FST health: a standalone profiled compile, mirroring diagnostics.rs's own "a second
    // compiled network is an acceptable one-time cost for an offline tool" judgment call ---------
    let (proposer_result, compile_profile) = FomaProposer::new_with_profile(&grammar);
    let fst_health = match &proposer_result {
        Ok(proposer) => evaluate_health(None, Some(&proposer.report), &[], &[], Some(&compile_profile)),
        Err(pg_foma::analyzer::FomaError::LexcCompileFailed(report)) => {
            evaluate_health(None, Some(report), &[], &[], Some(&compile_profile))
        }
        Err(_) => evaluate_health(None, None, &[], &[], Some(&compile_profile)),
    };

    // ---- Payloads: honestly-labeled placeholders (see this module's top doc) -------------------
    let package_fingerprint =
        pg_pack::fingerprint_hex(PLACEHOLDER_RUNTIME_PAYLOAD, PLACEHOLDER_FOMA_PAYLOAD);

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

    let bytes = pg_pack::write_pack(
        &manifest,
        PLACEHOLDER_RUNTIME_PAYLOAD,
        PLACEHOLDER_FOMA_PAYLOAD,
    )
    .map_err(|e| format!("write_pack: {e}"))?;
    fs::write(out_path, &bytes).map_err(|e| format!("write {out_path}: {e}"))?;

    eprintln!(
        "pack complete: {out_path} ({} bytes) -- capability_trust={}, required_runtime_features={:?}, \
         fst_health admission={:?}. NOTE: the runtime/foma payload sections are honestly-labeled \
         PLACEHOLDER bytes (see this module's own doc for exactly what is real vs. placeholder in \
         this pack) -- do not treat them as a usable compiled artifact.",
        bytes.len(),
        if manifest.capability_trust.is_unproven() { "overridden/unproven" } else { "proven" },
        manifest.required_runtime_features.runtime_operations,
        manifest.fst_health.admission(),
    );
    Ok(())
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

    /// A self-feeding (`multipleApplication="2"`) `Compounding` rule -- `evaluate_capability`'s own
    /// `Refuse` case, ported verbatim from `main.rs`'s `capability_gate_tests::COMPOUNDING_GRAMMAR_XML`
    /// / `pg_foma::capability_entry::tests::evaluate_capability_refuses_recursive_compounding_grammar`
    /// (same fixture shape, this crate's repo-wide convention of porting a fixture verbatim across a
    /// crate boundary rather than sharing a `#[cfg(test)]`-only item).
    const REFUSE_GRAMMAR_XML: &str = r#"<HermitCrabInput><Language><Name>PackRefuseFixture</Name>
      <CharacterDefinitionTable id="t1"><Name>Main</Name>
        <SegmentDefinitions><SegmentDefinition id="ca"><Representations><Representation>a</Representation></Representations></SegmentDefinition></SegmentDefinitions>
      </CharacterDefinitionTable>
      <NaturalClasses><SegmentNaturalClass id="ncAll"><Name>All</Name><Segment segment="ca" /></SegmentNaturalClass></NaturalClasses>
      <Strata>
        <Stratum characterDefinitionTable="t1" morphologicalRules="cr1">
          <Name>S</Name>
          <MorphologicalRuleDefinitions>
            <CompoundingRule id="cr1" multipleApplication="2">
              <Name>Compound</Name>
              <CompoundingSubrules>
                <CompoundingSubrule>
                  <HeadMorphologicalInput>
                    <PhoneticSequence id="h0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </HeadMorphologicalInput>
                  <NonHeadMorphologicalInput>
                    <PhoneticSequence id="n0"><SimpleContext naturalClass="ncAll" /></PhoneticSequence>
                  </NonHeadMorphologicalInput>
                  <MorphologicalOutput>
                    <CopyFromInput index="n0" />
                    <CopyFromInput index="h0" />
                  </MorphologicalOutput>
                </CompoundingSubrule>
              </CompoundingSubrules>
            </CompoundingRule>
          </MorphologicalRuleDefinitions>
          <LexicalEntries>
            <LexicalEntry id="e1">
              <Allomorphs><Allomorph id="a1"><PhoneticShape>a</PhoneticShape></Allomorph></Allomorphs>
            </LexicalEntry>
          </LexicalEntries>
        </Stratum>
      </Strata>
    </Language></HermitCrabInput>"#;

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
        assert!(result.is_ok(), "clean grammar must pack successfully: {result:?}");

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
        assert!(result.is_ok(), "--allow-unproven must force-pack a Refuse verdict: {result:?}");

        let bytes = std::fs::read(&out_path).expect("read out.pgpack");
        let read = pg_pack::read_pack(&bytes).expect("an overridden pack must still read back cleanly");
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
                        .any(|c| c.construct.contains("Compounding")),
                    "expected a refused config naming Compounding: {:?}",
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
            second_read.manifest.capability_trust,
            first_read.manifest.capability_trust,
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
    /// `pg_foma::peel`'s own module doc, "Task 2.2's recall proof").
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
            read
                .manifest
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
        let (result, out_path) = run_pack_raw("no-authorized-by", REFUSE_GRAMMAR_XML, &["--allow-unproven"]);
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
}
